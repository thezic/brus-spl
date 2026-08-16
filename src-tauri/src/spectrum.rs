//! FFT frame assembly, third-octave banding and the column ring — spec §7.1.
//!
//! **The second pipeline, and it is deliberately independent of the first.** It taps the
//! **raw** samples ahead of the weighting filter (spec §5.2, §7.2), so nothing the reader does
//! to the meter changes the picture: the number answers *how loud* and the picture answers
//! *what is making it and whether it has been steady*, and they stay two pieces of evidence
//! rather than two renderings of one.
//!
//! Like [`crate::metrics`], **nothing here reads the clock**. Every entry point takes the slot
//! index the caller already has, which is what makes the whole thing testable with no hardware,
//! no threads and no sleeping — and what guarantees the columns are indexed against the *same*
//! slots as the energy ring rather than against a second, subtly different timebase.
//!
//! **No calibration offset lives here either.** Every band value is raw dB relative to full
//! scale; spec §8.2's offset is added on the way across the bridge, which is what makes spec
//! §9.5's offset row true — the window and the values shift together, so every colour is
//! unchanged and only the legend relabels.

use std::ops::RangeInclusive;
use std::sync::Arc;

use realfft::num_complex::Complex;
use realfft::{RealFftPlanner, RealToComplex};

use crate::metrics::{RING_SLOTS, SLOT_MS};

/// 32 fixed one-third-octave rows, 12.5 Hz–16 kHz nominal (spec §7.1).
///
/// **Fixed, and never adaptive.** An energy-summed row's level scales with its bandwidth, so a
/// 24-row layout reads ~10 dB hotter than a 220-row one on the identical signal — which is what
/// would make a labelled colour legend a lie. The row count is part of the scale.
pub const BANDS: usize = 32;

/// The analysis length (spec §7.1). **8192, not 2048**: at N=2048 everything below 126 Hz is
/// bin-borrowing — the whole dB(C) region — while N=8192 gives 5.86 Hz bins at 48 kHz and
/// resolves every band from **31.5 Hz** up, at a 171 ms frame. (§7.1 says 16 Hz; see
/// [`tests::which_band_borrows_is_a_function_of_the_rate_and_it_is_not_always_the_12_5_hz_one`]
/// for the four rows that are narrower than a bin, and spec §13.12 for the correction.)
pub const FFT_N: usize = 8192;

/// IEC 61260 base-10 band index of the first row: `f(n) = 1000·10^(0.1(n−30))`, so `n = 11` is
/// 12.589 Hz (12.5 nominal) and `n = 42` is 15848.9 Hz (16 k nominal).
///
/// The 20 kHz nominal band is dropped rather than forgotten: its upper edge exceeds 20 kHz.
const BAND_INDEX_FIRST: i32 = 11;

/// The floor a band level is clamped to, in dB re FS.
///
/// Not a display value and never drawn as one: it is 90 dB below the bottom of spec §7.1's
/// −90 dBFS colour window, so nothing can reach it that is not mathematically zero. It exists
/// because a band with *exactly* zero power gives `10·log₁₀(0) = −∞`, and `serde_json` writes a
/// non-finite float as `null` — which would put a `null` inside a `[f32; 32]` and break the
/// wire's own type. The same class of hazard spec §16.7 legislates against for NOW, at the one
/// place in this module where it can arise.
const BAND_FLOOR_DB: f32 = -200.0;

/// A ring position that has never been written. Slot indices start at 0, so `0` cannot serve as
/// the "empty" mark and a sentinel is needed; `u64::MAX` is 58 billion years of 100 ms slots
/// away from any index this app will produce.
const NO_SLOT: u64 = u64::MAX;

/// The bins one band sums, as a half-open range of bin indices.
///
/// Precomputed from the **granted** sample rate, because band edges are a runtime function of it
/// (spec §7.1) and the rate can move under a route change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct BandBins {
    lo: usize,
    hi: usize,
}

/// The FFT frame buffer, the banding, and the 1200-column history.
///
/// Single-threaded and owned by the tick, exactly like [`crate::metrics::Metrics`]. The one
/// thread boundary is the raw-sample queue out of the audio callback (spec §3.3, §6.10).
pub struct Spectrum {
    sample_rate: f64,
    /// Samples per 100 ms slot: one frame is emitted per hop, so the column cadence is the slot
    /// cadence whenever audio is flowing.
    hop: usize,
    /// [`crate::capture::Capture::builds`] and [`crate::capture::Capture::lost_samples`] as of
    /// the last [`Spectrum::sync_stream`]. Either one moving is a break in the sample stream,
    /// and splicing two eras of audio across one 171 ms frame would draw a column of something
    /// that was never in the room.
    generation: u64,
    lost: u64,

    /// The last [`FFT_N`] samples, circular. `write` is both the next write position and the
    /// position of the **oldest** sample once the buffer is full.
    buffer: Vec<f64>,
    write: usize,
    /// Samples buffered since the last discontinuity, saturating at [`FFT_N`]. **No column is
    /// emitted until this reaches `FFT_N`** (spec §7.3), so the first one or two slots after a
    /// start or a rebuild are holes rather than a frame padded with silence.
    filled: usize,
    /// Samples accumulated toward the next frame, and their total power. The power is spec
    /// §16.8's gap test: a slot that received only exact zeros is **a gap, not a quiet column**,
    /// or a denied microphone would draw a picture at the bottom of the colour window — which
    /// reads as a very quiet room.
    in_hop: usize,
    hop_power: f64,

    fft: Arc<dyn RealToComplex<f64>>,
    window: Vec<f64>,
    /// The one-sided power normalisation, `2 / (N·S₂)` — see [`Spectrum::new`].
    scale: f64,
    input: Vec<f64>,
    output: Vec<Complex<f64>>,
    scratch: Vec<Complex<f64>>,
    bands: Vec<BandBins>,

    /// This drain's frames, in order, `None` for a hop that was a gap. Reused rather than
    /// reallocated, and it holds gaps as well as columns because **the gaps are what keep the
    /// slot assignment aligned**: a hop is a slot's worth of audio whether or not it was silent.
    pending: Vec<Option<[f32; BANDS]>>,

    /// 1200 columns × 32 `f32` = 153 KB, allocated once (spec §9.2). Rust owns the picture's
    /// history so that every canvas invalidation is one move — *pull again* — rather than four
    /// different repairs.
    columns: Vec<[f32; BANDS]>,
    /// The absolute slot each ring position holds. A slot is present iff
    /// `stamps[slot % RING_SLOTS] == slot`, which is what makes an aged-out column disappear
    /// with no clearing pass at all.
    stamps: Vec<u64>,
    /// The newest slot a frame has been assigned to, so a backlog cannot overwrite a column the
    /// previous tick already published.
    last_slot: Option<u64>,
}

impl Spectrum {
    /// An empty ring, an empty frame buffer, and the bands for `sample_rate`.
    pub fn new(sample_rate: f64) -> Spectrum {
        let mut planner = RealFftPlanner::<f64>::new();
        let fft = planner.plan_fft_forward(FFT_N);
        let input = fft.make_input_vec();
        let output = fft.make_output_vec();
        let scratch = fft.make_scratch_vec();

        // Periodic Hann, so `w[0] = 0` and the window tiles at the hop rather than repeating a
        // sample. S₁ = N/2 and S₂ = 3N/8 exactly, but both are summed rather than asserted —
        // the normalisation is load-bearing for the colour scale and a hand-derived constant
        // that silently disagrees with the window is exactly the kind of error that shows up as
        // a picture 1.8 dB out with nothing on screen to say so.
        let window: Vec<f64> = (0..FFT_N)
            .map(|n| 0.5 - 0.5 * (std::f64::consts::TAU * n as f64 / FFT_N as f64).cos())
            .collect();
        let s2: f64 = window.iter().map(|w| w * w).sum();

        // ─── the power normalisation, and this is spec §16.4 corrected ─────────────────────
        //
        // §16.4 decides `P_k = 2·|X_k|² / S₁²` with `S₁ = Σw[n]`, so that a full-scale sine at a
        // bin centre reads its own mean square, −3.01 dBFS. That is the **coherent** gain, and
        // it is right for reading a tone off the one bin it peaks in — but this display's cell
        // value is not a bin, it is the **sum of the bins in a band** (spec §7.1), and a Hann
        // window puts a third of a tone's energy in the two bins either side of the peak.
        // Summed, `2·|X_k|²/S₁²` therefore reads **+1.76 dB = 10·log₁₀(1.5)** high — uniformly,
        // for a tone and for noise alike, since the factor is `S₁²/(N·S₂) = 1.5` for any signal
        // whose energy is captured whole.
        //
        // The intent of §16.4 is the sentence after the formula — *the same dBFS convention as
        // the meter, which is what lets one calibration offset shift both* — and only the
        // **noise-power** normalisation `2/(N·S₂)` delivers that for a band sum. It is the
        // standard one for exactly this reason: with it, `Σ_k P_k` over every bin is the signal's
        // mean square, so summing the 32 bands recovers the meter's own unweighted dBFS, and a
        // full-scale sine reads −3.0103 dBFS **in its band** — which is what this ticket's own
        // Done-when asks for and what §16.4's formula cannot give.
        //
        // Verified both ways in the tests rather than argued: `a_full_scale_sine_reads_its_own_
        // mean_square_in_its_band` asserts −3.0103, and `section_16_4s_normalisation_is_1_76_dB_
        // high_for_a_band_sum` pins the size of the correction so the claim is checkable.
        let scale = 2.0 / (FFT_N as f64 * s2);

        Spectrum {
            sample_rate,
            hop: hop_samples(sample_rate),
            generation: 0,
            lost: 0,
            buffer: vec![0.0; FFT_N],
            write: 0,
            filled: 0,
            in_hop: 0,
            hop_power: 0.0,
            fft,
            window,
            scale,
            input,
            output,
            scratch,
            bands: band_bins(sample_rate),
            pending: Vec::with_capacity(16),
            columns: vec![[0.0; BANDS]; RING_SLOTS as usize],
            stamps: vec![NO_SLOT; RING_SLOTS as usize],
            last_slot: None,
        }
    }

    pub fn sample_rate(&self) -> f64 {
        self.sample_rate
    }

    /// Follows the stream: a new rate re-derives the bands, and **any** break in the sample
    /// stream discards the partial frame.
    ///
    /// `builds` and `lost` are [`crate::capture::Capture::builds`] and
    /// [`crate::capture::Capture::lost_samples`], two cumulative counters the tick already has.
    /// Both mean the same thing here — *the samples either side of this are not one signal*:
    ///
    /// - a **rebuild** leaves a hole of whatever the interruption cost (`b07` measured ~210 ms),
    /// - a **queue overflow** loses the newest samples, so what is left is not only incomplete
    ///   but *older* than the slot it would be drawn in, and this module's whole placement rule
    ///   is that the last hop is `now_slot`.
    ///
    /// Splicing either one would draw a column of a transient that never happened, at the exact
    /// moment the meter is honestly showing `--`. Discarding costs the one slot a fresh start
    /// costs, which spec §7.3 already spends.
    ///
    /// **Nothing else in this module clears** (spec §6.11's fourth column is empty): not Reset,
    /// not a weighting change, not F/S, not a window-length change, not an offset change.
    pub fn sync_stream(&mut self, builds: u64, lost: u64, sample_rate: f64) {
        if sample_rate != self.sample_rate {
            self.sample_rate = sample_rate;
            self.hop = hop_samples(sample_rate);
            self.bands = band_bins(sample_rate);
            self.discontinuity();
        }
        if builds != self.generation || lost != self.lost {
            self.generation = builds;
            self.lost = lost;
            self.discontinuity();
        }
    }

    /// Throws away the partial frame. The **column ring is untouched** — history stays history.
    fn discontinuity(&mut self) {
        self.filled = 0;
        self.in_hop = 0;
        self.hop_power = 0.0;
    }

    /// Feeds raw samples, completing frames as the hops close.
    ///
    /// Nothing is filed until [`Spectrum::publish`]: the queue hands its contents over as two
    /// slices when it has wrapped, and a hop boundary can fall between them, so *where* the
    /// frames land is decided once per drain rather than once per slice.
    pub fn ingest(&mut self, samples: &[f32]) {
        for &sample in samples {
            let x = sample as f64;
            self.buffer[self.write] = x;
            self.write = (self.write + 1) % FFT_N;
            self.filled = (self.filled + 1).min(FFT_N);

            self.hop_power += x * x;
            self.in_hop += 1;
            if self.in_hop >= self.hop {
                let column = self.frame();
                self.pending.push(column);
                self.in_hop = 0;
                self.hop_power = 0.0;
            }
        }
    }

    /// Files everything [`Spectrum::ingest`] has completed since the last publish.
    ///
    /// **`now_slot` places the columns, the sample count paces them.** One hop is exactly one
    /// slot of audio, so `k` frames completed since the last publish are the `k` slots ending at
    /// `now_slot` — which is what makes a late tick file its backlog rather than one column.
    /// Spec §6.2's clock-advanced ring means a timer on a phone routinely has two or three
    /// genuinely completed slots behind it, and dropping them would be dropping real data.
    ///
    /// Assignment is backwards from `now_slot` rather than forwards from the last column so that
    /// placement follows the wall clock, exactly as [`crate::metrics::Metrics::deposit`] does:
    /// a sample-counted timebase would stop advancing during a gap and then draw the audio that
    /// followed it in the past.
    pub fn publish(&mut self, now_slot: u64) {
        let produced = self.pending.len();
        for index in 0..produced {
            // The last frame is `now_slot`, the one before it `now_slot - 1`, and so on.
            let slot = now_slot.saturating_sub((produced - 1 - index) as u64);
            // A slot the previous tick already published. Only reachable when the clock is
            // behind the sample stream — a stalled tick that then drains two hops into one slot
            // — and the newer frame is the one covering that slot, so the older is dropped.
            if self.last_slot.is_some_and(|last| slot <= last) {
                continue;
            }
            if let Some(bands) = self.pending[index] {
                let position = (slot % RING_SLOTS) as usize;
                self.columns[position] = bands;
                self.stamps[position] = slot;
            }
        }
        if produced > 0 {
            self.last_slot = Some(now_slot.max(self.last_slot.unwrap_or(0)));
        }
        self.pending.clear();
    }

    /// One column, or `None` if that slot is a gap or has aged out of the ring.
    pub fn column(&self, slot: u64) -> Option<[f32; BANDS]> {
        let position = (slot % RING_SLOTS) as usize;
        (self.stamps[position] == slot).then(|| self.columns[position])
    }

    /// The slots the ring can answer **about** at all, given the newest slot published.
    ///
    /// Named rather than left inline in [`Spectrum::columns_in`]'s clamp because a readout has to
    /// tell *the ring keeps nothing for this slot* from *there was no audio here* (spec §7.5), and
    /// [`Spectrum::column`] answers `None` to both. Inside this range a `None` is a gap; outside
    /// it, it is the absence of a claim.
    ///
    /// The upper end is `now_slot` for the same reason the pull's right edge is: a slot ahead of
    /// the present is not something the app is withholding.
    pub fn retained_slots(now_slot: u64) -> RangeInclusive<u64> {
        now_slot.saturating_sub(RING_SLOTS - 1)..=now_slot
    }

    /// Every real column in `first ..= last`, oldest first.
    ///
    /// **Gaps are absent rather than marked** (spec §9.2): a slot index in the range with no
    /// entry *is* the hole. Explicit `null` entries were rejected as duplicating what the
    /// indices already say, at 270 wasted entries for a 27 s hole.
    pub fn columns_in(&self, first: u64, last: u64) -> Vec<(u64, [f32; BANDS])> {
        let first = first.max(*Spectrum::retained_slots(last).start());
        (first..=last)
            .filter_map(|slot| self.column(slot).map(|bands| (slot, bands)))
            .collect()
    }

    /// One frame over the last [`FFT_N`] samples, or `None` if this slot is a hole.
    ///
    /// Two ways to be a hole, and they are spec §7.3's and §16.8's respectively: not enough
    /// samples buffered yet, or a hop that carried no power at all.
    fn frame(&mut self) -> Option<[f32; BANDS]> {
        if self.filled < FFT_N || self.hop_power <= 0.0 {
            return None;
        }

        // Once the buffer is full, `write` is the oldest sample: unwrap it into the FFT input in
        // time order, windowing on the way through.
        let oldest = self.write;
        for n in 0..FFT_N {
            self.input[n] = self.buffer[(oldest + n) % FFT_N] * self.window[n];
        }

        let Spectrum {
            fft,
            input,
            output,
            scratch,
            ..
        } = self;
        if fft.process_with_scratch(input, output, scratch).is_err() {
            // Only reachable on a length mismatch, which is a programming error rather than a
            // runtime condition — but a panic here is on the tick thread, so it takes the app's
            // whole display with it. A hole is the honest degradation.
            return None;
        }

        let mut bands = [0.0f32; BANDS];
        for (row, bins) in self.bands.iter().enumerate() {
            // **Sum, never mean** (spec §7.1). Pink noise has equal energy per third-octave by
            // definition, so an honest display draws it flat — `sum` does, and `mean` invents a
            // 32 dB roll-off that is not in the sound.
            let mut power = 0.0f64;
            for bin in bins.lo..bins.hi {
                power += self.output[bin].norm_sqr();
            }
            let db = 10.0 * (power * self.scale).log10();
            bands[row] = (db as f32).max(BAND_FLOOR_DB);
        }
        Some(bands)
    }
}

/// Samples per 100 ms slot — 4800 at 48 kHz, 4410 at 44.1 kHz.
fn hop_samples(sample_rate: f64) -> usize {
    ((sample_rate * SLOT_MS as f64 / 1000.0).round() as usize).max(1)
}

/// The exact centre of band `row`, in Hz: `f(n) = 1000·10^(0.1(n−30))` (IEC 61260 base-10).
fn band_centre(row: usize) -> f64 {
    1000.0 * 10f64.powf(0.1 * (BAND_INDEX_FIRST + row as i32 - 30) as f64)
}

/// Which bins each band sums, at `sample_rate` (spec §16.5).
///
/// **Bin centres decide, and the range is half-open**: a bin belongs to the band whose
/// `[f_lo, f_hi)` contains its centre, so a bin sitting exactly on a shared edge lands in the
/// upper band and lands there once.
///
/// **Bin 0 is never drawn** (spec §7.1) — it carries the microphone's DC bias, since spec §3.3
/// declines a DC blocker, and the standard's bands start at 10 Hz regardless.
///
/// A band with **no bin centre inside it borrows the nearest bin**, which is what makes the
/// 12.5 Hz row interpolated rather than empty: at N=8192 and 48 kHz it is 2.9 Hz wide against
/// 5.86 Hz bins. Spec §13.12 names it as the one band the picture cannot honestly draw. The same
/// rule quietly handles a band above Nyquist — it borrows the Nyquist bin — which is unreachable
/// at any rate `capture.rs` will build a stream at, and deterministic rather than a panic if it
/// ever is.
fn band_bins(sample_rate: f64) -> Vec<BandBins> {
    let df = sample_rate / FFT_N as f64;
    let last_bin = FFT_N / 2;

    (0..BANDS)
        .map(|row| {
            let centre = band_centre(row);
            // Edges at `f_c · 10^(±0.05)`.
            let lower = centre * 10f64.powf(-0.05);
            let upper = centre * 10f64.powf(0.05);

            // `k·df ≥ f_lo` and `k·df < f_hi`, so both ends are a ceiling and the upper one is
            // exclusive — which is exactly what makes a bin on an edge belong to one band only.
            let lo = (lower / df).ceil().max(1.0) as usize;
            let hi = ((upper / df).ceil() as usize).min(last_bin + 1);

            if lo < hi {
                BandBins { lo, hi }
            } else {
                let nearest = ((centre / df).round() as usize).clamp(1, last_bin);
                BandBins {
                    lo: nearest,
                    hi: nearest + 1,
                }
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const FS: f64 = 48_000.0;
    const FS_44K: f64 = 44_100.0;

    /// A full-scale sine's mean square is 0.5 — the convention `metrics.rs` measures against,
    /// which is the whole point of the normalisation.
    const TONE_DB: f64 = -3.010_299_956_639_812;

    /// Drives [`Spectrum`] the way the tick does: whole hops in, columns out.
    struct Rig {
        s: Spectrum,
        slot: u64,
    }

    impl Rig {
        fn new(sample_rate: f64) -> Rig {
            Rig {
                s: Spectrum::new(sample_rate),
                slot: 0,
            }
        }

        /// Feeds exactly one slot's worth of samples from `signal` and returns the column it
        /// produced, advancing the slot clock in step — the aligned case, which is what a
        /// healthy tick does.
        fn slot(&mut self, signal: &mut impl FnMut() -> f32) -> Option<[f32; BANDS]> {
            let hop = self.s.hop;
            let samples: Vec<f32> = (0..hop).map(|_| signal()).collect();
            self.slot += 1;
            self.s.ingest(&samples);
            self.s.publish(self.slot);
            self.s.column(self.slot)
        }

        /// Runs `slots` slots and returns the columns that were not holes.
        fn run(&mut self, slots: usize, signal: &mut impl FnMut() -> f32) -> Vec<[f32; BANDS]> {
            (0..slots).filter_map(|_| self.slot(signal)).collect()
        }

        /// Slots enough to fill the frame buffer, so the next one produces a column.
        fn prime(&mut self, signal: &mut impl FnMut() -> f32) {
            let slots = FFT_N.div_ceil(self.s.hop);
            self.run(slots, signal);
        }
    }

    /// A sine at exactly the centre of bin `bin`, so the transform sees a whole number of cycles
    /// per frame and the main lobe is the textbook three bins.
    fn bin_centred_tone(bin: usize) -> impl FnMut() -> f32 {
        let mut n = 0usize;
        move || {
            let x = (std::f64::consts::TAU * bin as f64 * n as f64 / FFT_N as f64).sin();
            n += 1;
            x as f32
        }
    }

    /// A deterministic white source in [−1, 1). xorshift rather than a dependency: the tests
    /// need repeatability, not statistical pedigree.
    struct White(u64);

    impl White {
        fn new() -> White {
            White(0x2545_f491_4f6c_dd1d)
        }

        fn next(&mut self) -> f64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            // 53 bits into [−1, 1).
            ((self.0 >> 11) as f64 / (1u64 << 52) as f64) - 1.0
        }
    }

    /// Paul Kellet's refined pink filter — flat to ±0.05 dB from ~9 Hz up, and the standard way
    /// to get equal energy per octave out of a white source without an FFT.
    fn pink() -> impl FnMut() -> f32 {
        let mut white = White::new();
        let mut b = [0.0f64; 7];
        move || {
            let w = white.next();
            b[0] = 0.998_86 * b[0] + w * 0.055_517_9;
            b[1] = 0.993_32 * b[1] + w * 0.075_075_9;
            b[2] = 0.969_00 * b[2] + w * 0.153_852_0;
            b[3] = 0.866_50 * b[3] + w * 0.310_485_6;
            b[4] = 0.550_00 * b[4] + w * 0.532_952_2;
            b[5] = -0.761_6 * b[5] - w * 0.016_898_0;
            let out = b[0] + b[1] + b[2] + b[3] + b[4] + b[5] + b[6] + w * 0.536_2;
            b[6] = w * 0.115_926;
            (out * 0.11) as f32
        }
    }

    /// Averages columns in **linear power** and returns the result in dB.
    ///
    /// Averaging in dB would be averaging the logs, which is not the mean of the energy — the
    /// same distinction spec §6.1 makes for `L_eq`.
    fn mean_db(columns: &[[f32; BANDS]]) -> [f64; BANDS] {
        let mut out = [0.0f64; BANDS];
        for (row, value) in out.iter_mut().enumerate() {
            let sum: f64 = columns
                .iter()
                .map(|c| 10f64.powf(c[row] as f64 / 10.0))
                .sum();
            *value = 10.0 * (sum / columns.len() as f64).log10();
        }
        out
    }

    /// The first row whose exact centre is above `hz`.
    fn first_row_above(hz: f64) -> usize {
        (0..BANDS).find(|&row| band_centre(row) > hz).unwrap()
    }

    /// The nominal width of a band, `f_c·(10^0.05 − 10^−0.05)`.
    fn band_width(row: usize) -> f64 {
        band_centre(row) * (10f64.powf(0.05) - 10f64.powf(-0.05))
    }

    /// **Finding: how far a band reads from flat on a flat signal, from bin quantisation
    /// alone.**
    ///
    /// A band sums a whole number of bins, so it measures `n·df` of spectrum where its own edges
    /// ask for `f_hi − f_lo`. The error is bounded by half a bin either way, which is nothing at
    /// 39 bins (1 kHz) and **−1.95 dB at 40 Hz**, where the band wants 1.57 bins and gets 1.
    ///
    /// This is the same shape of correction `b02` finding 1 made to the weighting table's
    /// tolerance: the residual is not noise, it is *predictable in closed form*, so the honest
    /// test is flatness **around the prediction** rather than flatness against a straight line.
    /// It is a consequence of spec §16.5's bin-centre rule, which is deliberate and not
    /// re-litigated here — 1 dB of ripple is under 2 % of a 60 dB colour scale.
    fn quantisation_db(rate: f64, row: usize) -> f64 {
        let df = rate / FFT_N as f64;
        let bins = band_bins(rate)[row];
        10.0 * ((bins.hi - bins.lo) as f64 * df / band_width(row)).log10()
    }

    /// max − min of a slice, the flatness measure both eyeball tests reduce to.
    fn spread(values: &[f64]) -> f64 {
        let lo = values.iter().cloned().fold(f64::MAX, f64::min);
        let hi = values.iter().cloned().fold(f64::MIN, f64::max);
        hi - lo
    }

    // ─── the bands themselves ───────────────────────────────────────────────────────────────

    #[test]
    fn the_band_centres_are_the_iec_series() {
        let nominal = [
            12.5, 16.0, 20.0, 25.0, 31.5, 40.0, 50.0, 63.0, 80.0, 100.0, 125.0, 160.0, 200.0,
            250.0, 315.0, 400.0, 500.0, 630.0, 800.0, 1000.0, 1250.0, 1600.0, 2000.0, 2500.0,
            3150.0, 4000.0, 5000.0, 6300.0, 8000.0, 10000.0, 12500.0, 16000.0,
        ];
        assert_eq!(nominal.len(), BANDS);
        for (row, want) in nominal.iter().enumerate() {
            let exact = band_centre(row);
            // Every exact centre is within 2 % of its nominal name, which is what pins the
            // series to the right end: an off-by-one in `n` would land a whole band away.
            assert!(
                (exact / want - 1.0).abs() < 0.02,
                "row {row}: exact {exact} against nominal {want}"
            );
        }
        // 1 kHz is the anchor of the series, exactly.
        assert!((band_centre(19) - 1000.0).abs() < 1e-9);
    }

    #[test]
    fn every_band_takes_its_own_bins_and_bin_zero_is_never_drawn() {
        for rate in [FS, FS_44K] {
            let bands = band_bins(rate);
            let df = rate / FFT_N as f64;
            for (row, bins) in bands.iter().enumerate() {
                assert!(bins.lo >= 1, "row {row} at {rate} Hz reaches bin 0");
                assert!(bins.lo < bins.hi, "row {row} at {rate} Hz is empty");
                assert!(bins.hi <= FFT_N / 2 + 1, "row {row} at {rate} Hz overruns");

                // A borrowed band is the only one whose bins may sit outside its own edges.
                let centre = band_centre(row);
                let lower = centre * 10f64.powf(-0.05);
                let upper = centre * 10f64.powf(0.05);
                let borrowed = (lower / df).ceil().max(1.0) as usize
                    >= ((upper / df).ceil() as usize).min(FFT_N / 2 + 1);
                if !borrowed {
                    assert!(bins.lo as f64 * df >= lower, "row {row} reaches below f_lo");
                    assert!(
                        (bins.hi - 1) as f64 * df < upper,
                        "row {row} reaches above f_hi"
                    );
                }
            }
            // Consecutive bands never share a bin, and never skip one between them.
            for pair in bands.windows(2) {
                if pair[0].hi != pair[1].lo {
                    // The only permitted disagreement is a borrow, where a band with no bin of
                    // its own reuses a neighbour's.
                    assert!(
                        pair[1].hi - pair[1].lo == 1 || pair[0].hi - pair[0].lo == 1,
                        "bands overlap or skip: {pair:?} at {rate} Hz"
                    );
                }
            }
        }
    }

    #[test]
    fn which_band_borrows_is_a_function_of_the_rate_and_it_is_not_always_the_12_5_hz_one() {
        // **Finding.** The ticket and spec §13.12 both name the 12.5 Hz band as *the* borrowed
        // one — "at N=8192 it is 2.9 Hz wide against 5.86 Hz bins". The width is right and the
        // conclusion does not follow: a band narrower than a bin borrows only when no bin centre
        // happens to fall inside it, which depends on where the bin grid lands, which depends on
        // the rate. Measured, at the two rates this app ever runs at:
        //
        //   48 kHz  — the 12.5 Hz band keeps bin 2 (11.72 Hz) and does **not** borrow; the one
        //             band that does is **20 Hz**, which has no centre in [17.8, 22.4) and takes
        //             bin 3 — the same bin the 16 Hz band already sums.
        //   44.1 kHz — the 12.5 Hz band **does** borrow, taking bin 2 at 10.77 Hz, which is
        //             below its own lower edge.
        //
        // So the substance of §13.12 stands — the bottom of the picture is interpolated and
        // cannot be drawn honestly — while *which* row is fabricated moves under a route change.
        // The stronger statement, and the one that is rate-independent, is the second assertion:
        // every band below 40 Hz is narrower than a bin, so all five are reporting a bin's worth
        // of spectrum through a narrower window whether they borrow or not.
        let borrows = |rate: f64| -> Vec<usize> {
            let df = rate / FFT_N as f64;
            (0..BANDS)
                .filter(|&row| {
                    let centre = band_centre(row);
                    let lo = (centre * 10f64.powf(-0.05) / df).ceil().max(1.0) as usize;
                    let hi = ((centre * 10f64.powf(0.05) / df).ceil() as usize).min(FFT_N / 2 + 1);
                    lo >= hi
                })
                .collect()
        };
        assert_eq!(borrows(FS), vec![2], "48 kHz");
        assert_eq!(borrows(FS_44K), vec![0], "44.1 kHz");

        let df = FS / FFT_N as f64;
        let starved: Vec<usize> = (0..BANDS).filter(|&row| band_width(row) < df).collect();
        assert_eq!(starved, vec![0, 1, 2, 3], "narrower than one 5.86 Hz bin");
        // And from 40 Hz up every band is wider than a bin — which is the actual content of
        // "N=8192 resolves every band from 16 Hz up", one octave and a half higher than stated.
        assert!(band_width(4) > df && band_centre(4) < 32.0);
    }

    // ─── the normalisation ──────────────────────────────────────────────────────────────────

    #[test]
    fn a_full_scale_sine_reads_its_own_mean_square_in_its_band() {
        // Bin 171 at 48 kHz is 1002.0 Hz — inside the 1 kHz band, which is ~39 bins wide, so the
        // Hann main lobe is captured whole and the band sum is the tone's mean square.
        let bin = 171;
        let mut tone = bin_centred_tone(bin);
        let mut rig = Rig::new(FS);
        rig.prime(&mut tone);
        let column = rig.slot(&mut tone).expect("a column");

        let row = 19; // 1 kHz
        let bands = band_bins(FS);
        let centre = bin as f64 * FS / FFT_N as f64;
        assert!(
            bands[row].lo + 1 < bin && bin + 1 < bands[row].hi,
            "{centre} Hz"
        );

        assert!(
            (column[row] as f64 - TONE_DB).abs() < 0.01,
            "1 kHz band reads {} against {TONE_DB}",
            column[row]
        );
        // And nowhere else: the neighbours are 60 dB down or more, so the band is not merely
        // right on average.
        assert!(column[row - 1] < -60.0 && column[row + 1] < -60.0);
    }

    #[test]
    fn section_16_4s_normalisation_is_1_76_db_high_for_a_band_sum() {
        // The correction, stated as a number so it can be checked rather than believed. §16.4's
        // `2/S₁²` over the `2/(N·S₂)` used here is `(N·S₂)/S₁²`, which for a periodic Hann is
        // `(N · 3N/8)/(N/2)² = 1.5` exactly — so §16.4 reads +10·log₁₀(1.5) = +1.7609 dB high.
        let window: Vec<f64> = (0..FFT_N)
            .map(|n| 0.5 - 0.5 * (std::f64::consts::TAU * n as f64 / FFT_N as f64).cos())
            .collect();
        let s1: f64 = window.iter().sum();
        let s2: f64 = window.iter().map(|w| w * w).sum();

        let section_16_4 = 2.0 / (s1 * s1);
        let used = 2.0 / (FFT_N as f64 * s2);
        let excess_db = 10.0 * (section_16_4 / used).log10();
        assert!(
            (excess_db - 1.760_912_590_556_812).abs() < 1e-9,
            "the correction is {excess_db} dB, not 1.7609"
        );

        // Which is exactly the +1.76 dB a band sum would carry: a full-scale sine's three-bin
        // main lobe is 0.5 + 0.125 + 0.125 = 0.75 under §16.4, against its true 0.5.
        assert!((10.0 * (0.75f64 / 0.5).log10() - excess_db).abs() < 1e-9);
    }

    #[test]
    fn the_bands_sum_to_the_meters_own_dbfs() {
        // The property the normalisation buys, and the reason it is worth correcting §16.4 for:
        // 32 band energies added back up are the signal's mean square, so the picture and the
        // number are the same quantity differently sliced — which is what lets **one**
        // calibration offset shift both (spec §16.4's own rationale).
        let mut white = White::new();
        let mut signal = move || white.next() as f32;
        let mut rig = Rig::new(FS);
        rig.prime(&mut signal);
        let columns = rig.run(20, &mut signal);
        let mean = mean_db(&columns);

        let total: f64 = mean.iter().map(|db| 10f64.powf(db / 10.0)).sum();
        // A uniform source over [−1, 1) has mean square 1/3 = −4.77 dBFS. The bands cover
        // 11.2 Hz to 17.8 kHz, so they miss the corners of a 24 kHz-wide white spectrum — 0.9 dB
        // of it — which is why this is a bound rather than an equality.
        let bands_db = 10.0 * total.log10();
        let full = 10.0 * (1.0f64 / 3.0).log10();
        assert!(
            bands_db < full && bands_db > full - 1.5,
            "bands sum to {bands_db} dBFS against a signal at {full} dBFS"
        );
    }

    // ─── the two eyeball tests, asserted on the numbers ─────────────────────────────────────

    #[test]
    fn pink_noise_gives_equal_band_levels_above_32_hz() {
        // §14.3's first eyeball test, run before there is anything to look at — and the one that
        // caught `mean` inventing a 32 dB roll-off in `07`'s prototype.
        for rate in [FS, FS_44K] {
            let mut signal = pink();
            let mut rig = Rig::new(rate);
            rig.prime(&mut signal);
            // 60 columns: a single periodogram has ~100 % relative error per bin, and the
            // narrowest band above 32 Hz holds only a couple of bins, so one frame is 3 dB of
            // noise on its own. Averaging in energy over 6 s of columns puts it under 0.5 dB.
            let columns = rig.run(60, &mut signal);
            let mean = mean_db(&columns);

            let first = first_row_above(32.0);
            // Flat **around the predicted bin quantisation**, which at 40 Hz is −1.95 dB on its
            // own — see [`quantisation_db`]. Against a straight line the same data spans 1.9 dB
            // and the whole of the excess is that one row.
            let residual: Vec<f64> = (first..BANDS)
                .map(|row| mean[row] - quantisation_db(rate, row))
                .collect();
            assert!(
                spread(&residual) < 1.5,
                "at {rate} Hz pink spans {:.2} dB across rows {first}..{BANDS}: {:?}",
                spread(&residual),
                residual
            );
        }
    }

    #[test]
    fn white_noise_rises_one_db_per_band() {
        // The analytic companion to the pink test, and it checks the thing pink cannot: a band
        // is 10^0.1 = 1.259× wider than the one below it, so **summed** bin power over white
        // noise must rise exactly 1.0 dB per row. A `mean` cell value would draw this flat, and
        // a bandwidth error anywhere would bend it.
        let mut white = White::new();
        let mut signal = move || white.next() as f32;
        let mut rig = Rig::new(FS);
        rig.prime(&mut signal);
        let mean = mean_db(&rig.run(60, &mut signal));

        // Exactly, rather than approximately: white noise puts the same power in every bin, so a
        // band's level is `10·log₁₀(n_bins)` plus one constant for the whole picture. That both
        // pins the bin assignment band by band and kills `mean` as a cell value outright — a
        // mean would make this constant instead of stepping.
        let bands = band_bins(FS);
        let first = first_row_above(100.0);
        let residual: Vec<f64> = (first..BANDS)
            .map(|row| mean[row] - 10.0 * ((bands[row].hi - bands[row].lo) as f64).log10())
            .collect();
        assert!(
            spread(&residual) < 0.6,
            "white is not n_bins-shaped: {residual:?}"
        );
        // And the nominal step really is ~1 dB per row, which is the property the fixed 32-row
        // layout exists to keep still: a band is 10^0.1 = 1.259× wider than the one below it.
        let nominal = mean[BANDS - 1] - mean[first];
        assert!(
            (nominal / (BANDS - 1 - first) as f64 - 1.0).abs() < 0.1,
            "rows {first}..{BANDS} climb {nominal:.2} dB in total"
        );
    }

    #[test]
    fn an_exponential_sweep_walks_the_bands_monotonically() {
        // §14.3's second eyeball test: a curve rather than a diagonal means the log axis mapping
        // is wrong. Asserted as *the loudest band never moves backwards*.
        let mut n = 0usize;
        let mut phase = 0.0f64;
        let (f0, f1) = (100.0f64, 12_000.0f64);
        let seconds = 12.0;
        let total = (FS * seconds) as usize;
        let mut sweep = move || {
            let t = (n as f64 / total as f64).min(1.0);
            let hz = f0 * (f1 / f0).powf(t);
            phase += std::f64::consts::TAU * hz / FS;
            n += 1;
            phase.sin() as f32
        };

        let mut rig = Rig::new(FS);
        rig.prime(&mut sweep);
        let columns = rig.run(100, &mut sweep);
        assert!(columns.len() > 90);

        let peaks: Vec<usize> = columns
            .iter()
            .map(|c| {
                (0..BANDS)
                    .max_by(|&a, &b| c[a].partial_cmp(&c[b]).unwrap())
                    .unwrap()
            })
            .collect();
        for pair in peaks.windows(2) {
            assert!(
                pair[1] >= pair[0],
                "the sweep walked backwards: {:?}",
                &peaks
            );
        }
        // And it actually travelled, rather than sitting still monotonically.
        assert!(peaks[peaks.len() - 1] - peaks[0] > 15, "{peaks:?}");
    }

    #[test]
    fn a_tone_lands_in_its_own_band_at_both_rates() {
        // Band edges are a runtime function of the rate, so the same tone must land in the same
        // row at 44.1 and 48 kHz — the one place a rate change could silently shift the picture.
        for rate in [FS, FS_44K] {
            for row in [10usize, 15, 19, 25, 30] {
                let centre = band_centre(row);
                let mut phase = 0.0f64;
                let mut tone = move || {
                    phase += std::f64::consts::TAU * centre / rate;
                    phase.sin() as f32
                };
                let mut rig = Rig::new(rate);
                rig.prime(&mut tone);
                let column = rig.slot(&mut tone).expect("a column");
                let peak = (0..BANDS)
                    .max_by(|&a, &b| column[a].partial_cmp(&column[b]).unwrap())
                    .unwrap();
                assert_eq!(peak, row, "{centre} Hz at {rate} Hz landed in row {peak}");
            }
        }
    }

    // ─── gaps ───────────────────────────────────────────────────────────────────────────────

    #[test]
    fn exact_zeros_produce_a_gap_not_a_column_at_the_bottom_of_the_range() {
        // Spec §16.8, and the reason it exists: a denied microphone delivers callbacks of exact
        // zeros, and a column drawn from them sits at the bottom of the colour window — which
        // reads as **a very quiet room** rather than as no microphone.
        let mut tone = bin_centred_tone(171);
        let mut rig = Rig::new(FS);
        rig.prime(&mut tone);
        assert!(rig.slot(&mut tone).is_some());

        let mut silence = || 0.0f32;
        // The frame is 171 ms, so the first hop of zeros still carries audio in the rest of the
        // buffer — but the *hop* is what decides, so the very next slot is already a hole.
        for _ in 0..5 {
            assert!(rig.slot(&mut silence).is_none());
        }
    }

    #[test]
    fn no_column_is_emitted_until_the_frame_is_full() {
        // Spec §7.3: 8192 samples is 0.171 s, so the first one or two slots after a start are
        // holes rather than a frame padded with silence. **One at 48 kHz and two at 44.1** —
        // 8192/4800 is 1.71 hops and 8192/4410 is 1.86, so the second hop closes over a full
        // buffer at both rates and it is only ever one. The "or two" is the 32 kHz case, which
        // this app does not reach.
        let mut tone = bin_centred_tone(171);
        let mut rig = Rig::new(FS);
        assert!(rig.slot(&mut tone).is_none(), "slot 1 holds 4800 samples");
        assert!(rig.slot(&mut tone).is_some(), "slot 2 closes over 8192");
    }

    #[test]
    fn a_rebuild_discards_the_partial_frame_but_never_the_history() {
        let mut tone = bin_centred_tone(171);
        let mut rig = Rig::new(FS);
        rig.prime(&mut tone);
        let drawn = rig.slot(&mut tone).expect("a column");
        let slot = rig.slot;

        // `b07`'s supervisor rebuilt the stream: the samples either side of the hole are not one
        // signal, so the frame buffer goes.
        rig.s.sync_stream(2, 0, FS);
        assert!(rig.slot(&mut tone).is_none());
        // …and the history stays. Spec §6.11's fourth column is empty, and 60 s of rumble stripe
        // cannot be recovered once wiped.
        assert_eq!(rig.s.column(slot), Some(drawn));
    }

    #[test]
    fn a_rate_change_re_derives_the_bands() {
        let mut rig = Rig::new(FS);
        let at_48k = rig.s.hop;
        rig.s.sync_stream(0, 0, FS_44K);
        assert_eq!(rig.s.sample_rate(), FS_44K);
        assert_eq!(at_48k, 4800);
        assert_eq!(rig.s.hop, 4410);
        assert_eq!(rig.s.bands, band_bins(FS_44K));
    }

    // ─── the ring and the slot assignment ───────────────────────────────────────────────────

    #[test]
    fn a_late_tick_publishes_its_backlog_rather_than_one_column() {
        // The correctness argument for `columns` being an array (spec §9.2): a timer on a phone
        // routinely has two or three genuinely completed slots behind it, and a one-column
        // payload would drop real data on the floor.
        let mut tone = bin_centred_tone(171);
        let mut rig = Rig::new(FS);
        rig.prime(&mut tone);

        let base = rig.slot;
        let hop = rig.s.hop;
        let samples: Vec<f32> = (0..hop * 3).map(|_| tone()).collect();
        rig.s.ingest(&samples);
        rig.s.publish(base + 3);

        // Three hops of audio ending at slot base+3 are slots base+1, base+2 and base+3.
        for slot in base + 1..=base + 3 {
            assert!(rig.s.column(slot).is_some(), "slot {slot} was dropped");
        }
        assert_eq!(rig.s.columns_in(base + 1, base + 3).len(), 3);
    }

    #[test]
    fn a_column_never_overwrites_one_the_previous_tick_published() {
        let mut tone = bin_centred_tone(171);
        let mut rig = Rig::new(FS);
        rig.prime(&mut tone);
        let published = rig.slot(&mut tone).expect("a column");
        let slot = rig.slot;

        // The clock has not moved but two hops arrived — a stalled tick. The newer frame covers
        // the slot; the older is dropped rather than landing on an index already published.
        let hop = rig.s.hop;
        let mut other = bin_centred_tone(600);
        let samples: Vec<f32> = (0..hop * 2).map(|_| other()).collect();
        rig.s.ingest(&samples);
        rig.s.publish(slot);
        assert_eq!(rig.s.column(slot), Some(published));
    }

    #[test]
    fn the_ring_holds_120_seconds_and_no_more() {
        let mut tone = bin_centred_tone(171);
        let mut rig = Rig::new(FS);
        rig.prime(&mut tone);
        let first = rig.slot;
        assert!(rig.slot(&mut tone).is_some());

        // Jump a whole ring ahead. The slot that lands on the same ring position must not read
        // as the old column — this is what the stamp is for, and without it the picture would
        // show two-minute-old audio at the right edge.
        let hop = rig.s.hop;
        let samples: Vec<f32> = (0..hop).map(|_| tone()).collect();
        let later = first + RING_SLOTS;
        rig.s.ingest(&samples);
        rig.s.publish(later);
        assert!(rig.s.column(later).is_some());
        assert_eq!(rig.s.column(first), None);

        // And `columns_in` never reaches further back than the ring can honestly answer for.
        assert!(rig.s.columns_in(0, later).len() <= RING_SLOTS as usize);
    }

    /// [`Spectrum::retained_slots`] is the bound `columns_in` clamps to, so the two cannot drift
    /// apart — which matters because spec §7.5's readout uses it to tell an aged-out slot from a
    /// gap, and a range one slot wider than the ring would call a slot outside the ring silent.
    #[test]
    fn the_retained_range_is_exactly_what_the_ring_can_answer_for() {
        let now = 5_000;
        let retained = Spectrum::retained_slots(now);
        assert_eq!(retained, 3_801..=5_000);
        assert_eq!(retained.count() as u64, RING_SLOTS);
        assert!(
            !Spectrum::retained_slots(now).contains(&3_800),
            "one too far back"
        );
        assert!(
            !Spectrum::retained_slots(now).contains(&(now + 1)),
            "a slot ahead of the present is not data being withheld"
        );

        // Early on, the ring holds everything there has ever been — including slot 0.
        assert_eq!(Spectrum::retained_slots(3), 0..=3);
    }

    #[test]
    fn a_gap_slot_is_simply_absent_from_the_range() {
        let mut tone = bin_centred_tone(171);
        let mut rig = Rig::new(FS);
        rig.prime(&mut tone);
        let before = rig.slot(&mut tone).map(|_| rig.slot).expect("a column");

        let mut silence = || 0.0f32;
        rig.run(3, &mut silence);
        let after_silence = rig.slot;
        rig.run(3, &mut tone);
        let end = rig.slot;

        let listed: Vec<u64> = rig
            .s
            .columns_in(before, end)
            .into_iter()
            .map(|(slot, _)| slot)
            .collect();
        assert!(listed.contains(&before));
        for slot in before + 1..=after_silence {
            assert!(!listed.contains(&slot), "slot {slot} should be a hole");
        }
        // The frame is 171 ms, so the first slot back carries some of the silence — it is still
        // a real column, because the hop that closed it carried power.
        assert!(listed.contains(&end));
    }
}
