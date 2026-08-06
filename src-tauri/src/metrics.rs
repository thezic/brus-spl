//! The energy ring, coverage, the F/S smoother and the max hold — spec §6.
//!
//! **The one idea the whole module rests on: the ring is advanced by the monotonic clock, not
//! by arriving samples** (spec §6.2). [`Metrics::deposit`] merely drops a block into whichever
//! slot the clock is in when it is drained, so **slots nobody deposited into are gap slots by
//! construction** — gap accounting is a consequence of timekeeping rather than a feature that
//! has to enumerate causes. Research probe 2 measured Siri killing the stream permanently 3 s
//! into a 30 s run with *not one error reaching Rust*; a sample-driven ring stops moving when
//! the samples stop, so it would have reported `60s of 60s` over a stream that died half a
//! minute earlier.
//!
//! Nothing here reads the clock for itself. Every entry point takes `now`, which is what makes
//! the whole pipeline deterministic under `cargo test` with no hardware and no sleeping.
//!
//! **No calibration offset lives here.** Every number this module publishes is raw dB relative
//! to full scale; spec §8.2 adds the offset post-log, on the way across the bridge, which is
//! what makes an offset change clear nothing at all (spec §6.11's bottom row). That row
//! therefore holds in this module *by construction* rather than by a line of code.

use std::time::{Duration, Instant};

use crate::capture::BlockSummary;

/// One slot is 100 ms (spec §6.3).
///
/// Chosen for **retirement smoothness**: a retiring slot is 1/600 of the energy at a 60 s
/// window. At 1 s slots it is 1/60, and on a 10 s window 1/10, which stair-steps visibly once a
/// second.
const SLOT_MS: u64 = 100;

/// The ring is allocated at the longest permitted window, always — 1200 slots ≈ 19 KB,
/// permanently (spec §6.3). A window-length change is then nothing but *how far back do I sum*,
/// so it re-slices rather than resets, and spec §8.3's fixed 10 s calibration slice is free.
const RING_SLOTS: u64 = 1200;

/// NOW is `None` when no block has arrived for this long (spec §6.9).
///
/// ~9 missed blocks at 48 kHz: far outside scheduling jitter, fast enough to be honest, slow
/// enough not to flicker. Reasoned rather than measured — iOS drain jitter has never been
/// characterised, so this is the dial if `--` flickers on the device.
const STALE_AFTER: Duration = Duration::from_millis(200);

/// The four permitted window lengths (spec §6.5). A picker, not a numeric field.
pub const WINDOW_LENGTHS_S: [u32; 4] = [10, 30, 60, 120];

/// Exponential time weighting, clause 3.5. `S` is the default (spec §6.6).
///
/// Measured rather than argued: with the live value as the hero number, at `F` it moves 1.72 dB
/// per 100 ms tick — further than any rounding step, so display resolution cannot calm it. At
/// `S` it moves 0.351 dB per tick and still covers 6.8 dB of range.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TimeWeighting {
    Fast,
    #[default]
    Slow,
}

impl TimeWeighting {
    /// The time constant τ, in seconds.
    pub fn tau_s(self) -> f64 {
        match self {
            TimeWeighting::Fast => 0.125,
            TimeWeighting::Slow => 1.0,
        }
    }
}

/// One 100 ms slot of the energy ring.
///
/// Sum-of-squares **and** count, never a mean, because spec §6.1 divides by the actual sample
/// total across the whole window — a per-block mean would have to be un-averaged to be usable,
/// and the coverage figure counts samples rather than slots.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct Slot {
    sum_sq: f64,
    n: u32,
}

/// What one tick paints, before the calibration offset is added (spec §9.1's `meter`).
///
/// `None` is the instrument reporting its own state, and each one means something different:
/// `now` is absent because nothing has arrived recently, `leq` because the window holds no real
/// data, `max` because no level has been seen since the last clear.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Levels {
    /// The time-weighted live level, in dB re FS. `None` per spec §6.9.
    pub now: Option<f64>,
    /// The rolling `L_eq` over the window, in dB re FS. `None` at zero coverage.
    pub leq: Option<f64>,
    /// The maximum time-weighted level since the last clear, in dB re FS.
    ///
    /// **Unaffected by staleness or by zero power** — a hold is historical by nature, so it
    /// simply stops rising.
    pub max: Option<f64>,
    /// Seconds of real audio actually inside the window: `Σn / f_s`. Published **always**,
    /// never conditional on being degraded (spec §6.4) — an indicator that appears only when
    /// something is wrong is a warning, and warnings were ruled out.
    pub coverage_s: f64,
}

/// The whole level pipeline: the ring, the smoother and the hold.
///
/// Single-threaded and owned by the tick (spec §6.10 — one thread boundary, and it is the SPSC
/// queue, not this).
pub struct Metrics {
    /// Ground truth from the running stream, not what was asked for. Coverage and the
    /// smoother's `dt` are both derived from it.
    sample_rate: f64,
    /// Where slot 0 starts. Only ever used as a difference, so its absolute value is arbitrary.
    epoch: Instant,
    /// `RING_SLOTS` long, allocated once and never resized.
    slots: Vec<Slot>,
    /// Absolute index of the slot the clock is currently in. Monotonic, never wraps; the ring
    /// position is `current % RING_SLOTS`.
    current: u64,
    window_s: u32,
    time_weighting: TimeWeighting,
    /// The one-pole state: a smoothed **mean square**, linear, not dB. Zero means "nothing has
    /// arrived yet", which is why spec §16.7's not-positive clause is not merely defensive.
    smoothed: f64,
    /// The maximum of `smoothed`, stored **linear**. Published as `10·log₁₀(max) + offset`,
    /// which is correct because `max(xᵢ + c) = max(xᵢ) + c` (spec §6.7).
    max_smoothed: f64,
    /// When audio last actually arrived, for spec §6.9's 200 ms rule. This is the *callback's*
    /// timestamp, not the drain's.
    last_block: Option<Instant>,
}

impl Metrics {
    /// A pipeline with an empty ring, no hold and no smoother state, whose slot 0 begins at
    /// `start`.
    pub fn new(sample_rate: f64, start: Instant) -> Self {
        Metrics {
            sample_rate,
            epoch: start,
            slots: vec![Slot::default(); RING_SLOTS as usize],
            current: 0,
            window_s: 60,
            time_weighting: TimeWeighting::default(),
            smoothed: 0.0,
            max_smoothed: 0.0,
            last_block: None,
        }
    }

    pub fn sample_rate(&self) -> f64 {
        self.sample_rate
    }

    /// A rate change **clears neither the window nor the max hold** (spec §6.11, §4.2 item 6).
    ///
    /// Only the filter state is zeroed, and that lives with the chain in `capture.rs` rather
    /// than here. The energy already in the ring was summed at the old rate, so the coverage
    /// figure re-reads it against the new one — an approximation the spec accepts deliberately
    /// rather than a defect: a route change is rare, the error decays out of the window within
    /// a window length, and the alternative is a third reset cause.
    pub fn set_sample_rate(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
    }

    pub fn window_s(&self) -> u32 {
        self.window_s
    }

    /// Changing the window **re-slices the ring — no reset** (spec §6.5).
    ///
    /// The ring holds valid energy at any length, so this setting is nothing but *how far back
    /// do I sum*. `60 → 120` must read `60s of 120s` — a genuine 120 s average of everything
    /// there is — rather than starting from empty.
    pub fn set_window_s(&mut self, window_s: u32) {
        self.window_s = window_s;
    }

    pub fn time_weighting(&self) -> TimeWeighting {
        self.time_weighting
    }

    /// An F/S change **clears the max hold and nothing else** (spec §6.11).
    ///
    /// The max clears because `L_CFmax` and `L_CSmax` differ by several dB on speech and
    /// neither is visible in the number itself. The smoother state does **not** reset (spec
    /// §6.6): it is a smoothed mean square either way, so the coefficient swaps and the state
    /// keeps, re-converging in a few hundred ms going S→F and a couple of seconds F→S. The
    /// `L_eq` accumulator is untouched, not even as a flicker (clause 3.9 NOTE 3).
    pub fn set_time_weighting(&mut self, time_weighting: TimeWeighting) {
        self.time_weighting = time_weighting;
        self.max_smoothed = 0.0;
    }

    /// A weighting change (C/A/Z) clears **the window and the max hold** (spec §6.11).
    ///
    /// Energy summed through one filter cannot be averaged with energy summed through another.
    /// Zeroing the filter state is the chain's job, in `capture.rs`; the ring's half is here.
    pub fn on_weighting_change(&mut self) {
        self.clear_window();
        self.max_smoothed = 0.0;
    }

    /// The Reset button: clears the max hold **and** the window (spec §6.8).
    ///
    /// One button, because it is not "clear the max", it is *"start measuring this talk"*.
    /// Between talks the room is not quiet, and applause, chatter and PA music otherwise sit in
    /// the window through the first full minute of the next talk — reading high against the
    /// ceiling exactly when the new speaker is being judged. The refill is honest rather than
    /// blank (`12s of 60s` climbing), so this costs nothing in trust.
    ///
    /// It does **not** touch the smoother (a live value, not history) and does **not** touch
    /// the spectrogram's column ring (spec §9.3).
    pub fn reset(&mut self) {
        self.clear_window();
        self.max_smoothed = 0.0;
    }

    /// Files one drained block: into the ring, into the smoother, and past the max compare.
    ///
    /// `now` places the block — spec §6.3 assigns blocks **whole** to whichever slot is current
    /// when drained, so placement is approximate to ±21 ms while the coverage arithmetic stays
    /// exact, because it counts samples rather than slots. `block.t` is used only for the
    /// staleness rule.
    ///
    /// **A zero-power block is discarded here rather than deposited** (spec §6.3). A denied
    /// microphone delivers callbacks of exact zeros, so `Σn > 0` while `Σp² = 0` leaves the
    /// level undefined — which put `60s of 60s` beside `--` on the prototype, the coverage
    /// figure claiming a complete minute while the meter said there was nothing. **Zero power
    /// is the absence of a measurement, not a quiet one**, so such a block does not count as an
    /// arrival either: it leaves `last_block` alone, the 200 ms rule fires, and the meter lands
    /// on `--` beside `0s of 60s` — the designed state. Side benefit: no f64 `−∞` ever reaches
    /// `serde_json`.
    pub fn deposit(&mut self, now: Instant, block: BlockSummary) {
        self.advance(now);

        let usable = block.n > 0 && block.sum_sq.is_finite() && block.sum_sq > 0.0;
        if !usable {
            return;
        }

        let slot = &mut self.slots[(self.current % RING_SLOTS) as usize];
        slot.sum_sq += block.sum_sq;
        slot.n += block.n;

        // The smoother is advanced **per block by that block's own dt** (spec §6.6), which
        // makes it mathematically identical to advancing it continuously however irregularly
        // the tick fires — so the tick's timing affects only what is painted, never what is
        // measured.
        let dt = block.n as f64 / self.sample_rate;
        let alpha = 1.0 - (-dt / self.time_weighting.tau_s()).exp();
        let mean_square = block.sum_sq / block.n as f64;
        self.smoothed += (mean_square - self.smoothed) * alpha;

        // The max compare runs **here, at block rate (46.875/s)** rather than being sampled
        // from the smoother at the 10 Hz tick, which would drop four peaks in five (spec §6.7).
        if self.smoothed > self.max_smoothed {
            self.max_smoothed = self.smoothed;
        }

        self.last_block = Some(block.t);
    }

    /// What to paint, as of `now`. Advances the ring first, so a gap that is still running
    /// shows up even though no block has arrived to advance it.
    pub fn levels(&mut self, now: Instant) -> Levels {
        self.advance(now);

        let (leq, coverage_s) = self.leq_over(self.window_s);

        let fresh = match self.last_block {
            Some(t) => now.saturating_duration_since(t) < STALE_AFTER,
            None => false,
        };
        // Both clauses of spec §6.9, and §16.7's is the load-bearing one at startup and after a
        // clear: `smoothed` is exactly 0.0 until the first block lands, and `10·log₁₀(0)` must
        // not reach `serde_json`.
        let now_db = if fresh && self.smoothed > 0.0 {
            Some(10.0 * self.smoothed.log10())
        } else {
            None
        };

        let max = if self.max_smoothed > 0.0 {
            Some(10.0 * self.max_smoothed.log10())
        } else {
            None
        };

        Levels {
            now: now_db,
            leq,
            max,
            coverage_s,
        }
    }

    /// `L_eq` and coverage over the last `seconds` of the ring, whatever the window setting is.
    ///
    /// Public because spec §8.3's calibration match runs against a **fixed 10 s slice — the
    /// last 100 slots — independent of the display's window**. That is free here: no second
    /// accumulator, nothing to override, nothing to restore, and the main meter is never
    /// disturbed. `b04` is the caller.
    ///
    /// **Re-summed every call** rather than maintained as a running add-and-subtract (spec
    /// §6.3). 600 f64 adds at 10 Hz is nothing, it removes any drift question over an
    /// eight-hour session, and coverage falls out of the same loop free.
    pub fn leq_over(&self, seconds: u32) -> (Option<f64>, f64) {
        let slots = ((seconds as u64 * 1000) / SLOT_MS).clamp(1, RING_SLOTS);
        let first = self.current.saturating_sub(slots - 1);

        let mut sum_sq = 0.0f64;
        let mut n: u64 = 0;
        for index in first..=self.current {
            let slot = self.slots[(index % RING_SLOTS) as usize];
            sum_sq += slot.sum_sq;
            n += slot.n as u64;
        }

        // Never divided by the nominal window length (spec §6.1). Folding a gap in as silence
        // costs −2.6 dB for a 27 s hole in a 60 s window, and the direction is *under*-reading
        // against a ceiling. Averaging over real data makes the figure mean "the L_eq of the
        // 33 s I actually have", which is exactly what coverage then qualifies — and it is why
        // the first 60 seconds is not a special case.
        let leq = if n > 0 && sum_sq > 0.0 {
            Some(10.0 * (sum_sq / n as f64).log10())
        } else {
            None
        };

        (leq, n as f64 / self.sample_rate)
    }

    /// Moves the ring to the slot `now` falls in, clearing every slot passed over.
    ///
    /// This is the whole of spec §6.2. Clearing on the way past is what makes an untouched slot
    /// a gap slot: the ring keeps moving whether or not audio is arriving, so a hole is
    /// recorded *whatever* caused it — interruption, a route change that killed the stream, the
    /// dead air during a rebuild — without enumerating causes.
    fn advance(&mut self, now: Instant) {
        let index = (now.saturating_duration_since(self.epoch).as_millis() / SLOT_MS as u128)
            .min(u64::MAX as u128) as u64;
        if index <= self.current {
            return;
        }

        let elapsed = index - self.current;
        if elapsed >= RING_SLOTS {
            // Longer than the ring: everything in it is older than the longest window.
            self.slots.fill(Slot::default());
        } else {
            for step in 1..=elapsed {
                self.slots[((self.current + step) % RING_SLOTS) as usize] = Slot::default();
            }
        }
        self.current = index;
    }

    fn clear_window(&mut self) {
        self.slots.fill(Slot::default());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FS: f64 = 48_000.0;
    /// The block size the granted iOS/macOS route actually delivers (`b01` finding 3).
    const FRAMES: u32 = 1024;
    /// 1024 / 48 000 s = 21.333 ms — deliberately *not* a whole number of slots, so the ±21 ms
    /// placement of spec §6.3 is exercised rather than assumed away.
    const BLOCK: Duration = Duration::from_nanos(21_333_333);

    /// A full-scale sine has a mean square of 0.5, i.e. −3.0103 dBFS — the same convention the
    /// meter and the FFT both use (spec §16.4).
    const TONE_MS: f64 = 0.5;
    const TONE_DB: f64 = -3.010_299_956_639_812;

    /// Drives [`Metrics`] off a synthetic clock. No hardware, no sleeping, no `Instant::now()`
    /// except to pick an arbitrary origin.
    struct Rig {
        m: Metrics,
        now: Instant,
    }

    impl Rig {
        fn new() -> Rig {
            let start = Instant::now();
            Rig {
                m: Metrics::new(FS, start),
                now: start,
            }
        }

        /// Advances the clock by `secs`, depositing one block every `FRAMES` samples with mean
        /// square `mean_square`. `None` advances the clock and deposits **nothing** — a gap,
        /// which is the only way a gap ever arises in this design.
        ///
        /// Returns the seconds of audio actually deposited, which is `secs` rounded **down** to
        /// a whole block.
        fn run(&mut self, secs: f64, mean_square: Option<f64>) -> f64 {
            let end = self.now + Duration::from_secs_f64(secs);
            let mut blocks = 0u32;
            while self.now + BLOCK <= end {
                self.now += BLOCK;
                if let Some(ms) = mean_square {
                    self.m.deposit(
                        self.now,
                        BlockSummary {
                            sum_sq: ms * FRAMES as f64,
                            n: FRAMES,
                            t: self.now,
                        },
                    );
                    blocks += 1;
                }
            }
            // Land exactly on `end` so a caller's arithmetic is not off by a partial block.
            self.now = end;
            blocks as f64 * FRAMES as f64 / FS
        }

        fn levels(&mut self) -> Levels {
            self.m.levels(self.now)
        }
    }

    /// Coverage is asserted to within **one slot plus one block**, which is the real
    /// quantisation of this design rather than a fudge: a whole number of 1024-frame blocks
    /// never lands on a whole number of seconds, and the window's oldest 100 ms slot retires on
    /// a slot boundary the audio knows nothing about. 0.12 s is well inside the rounding of the
    /// `33s of 60s` the coverage figure is displayed as.
    fn assert_coverage(actual: f64, expected_s: f64) {
        let tolerance = SLOT_MS as f64 / 1000.0 + FRAMES as f64 / FS + 1e-9;
        assert!(
            (actual - expected_s).abs() <= tolerance,
            "coverage {actual} s, expected {expected_s} s (±{tolerance})"
        );
    }

    fn assert_db(actual: Option<f64>, expected: f64) {
        let actual = actual.expect("expected a level, got None");
        assert!(
            (actual - expected).abs() < 1e-9,
            "{actual} dB, expected {expected} dB"
        );
    }

    // ── §14.2, case 1 ────────────────────────────────────────────────────────────────────

    #[test]
    fn steady_tone_gives_its_own_leq_and_coverage_reads_t_of_window() {
        let mut rig = Rig::new();
        rig.run(20.0, Some(TONE_MS));

        let levels = rig.levels();
        assert_db(levels.leq, TONE_DB);
        // 20s of 60s: a filling window is just a window with low coverage. There is no warm-up
        // state, no blank screen and no separate code path (spec §6.1).
        assert_coverage(levels.coverage_s, 20.0);
    }

    // ── §14.2, case 2 ────────────────────────────────────────────────────────────────────

    #[test]
    fn a_27_second_hole_reads_33_of_60_at_the_leq_of_the_real_33() {
        let mut rig = Rig::new();
        rig.run(33.0, Some(TONE_MS));
        rig.run(27.0, None);

        let levels = rig.levels();
        assert_coverage(levels.coverage_s, 33.0);
        // The whole point: the L_eq of the 33 s that exist, **not** the −2.6 dB that folding
        // the gap in as silence would give. Under-reading against a ceiling is the dangerous
        // direction.
        assert_db(levels.leq, TONE_DB);
        assert!(
            levels.leq.unwrap() > TONE_DB - 0.001,
            "gap was folded in as silence"
        );
    }

    #[test]
    fn the_ring_keeps_moving_with_no_audio_at_all() {
        let mut rig = Rig::new();
        rig.run(10.0, Some(TONE_MS));
        // A stream that dies silently — probe 2's Siri case, where not one error reached Rust.
        rig.run(60.0, None);

        let levels = rig.levels();
        assert_eq!(
            levels.coverage_s, 0.0,
            "reported coverage over a dead stream"
        );
        assert_eq!(levels.leq, None);
        assert_eq!(levels.now, None);
    }

    #[test]
    fn a_gap_clears_the_slots_the_ring_has_already_wrapped_through() {
        let mut rig = Rig::new();
        // Past 120 s, so every position in the ring holds real energy and the gap that follows
        // has to *clear* slots rather than merely slide off them. The test above passes on a
        // ring that never clears at all, because at 70 s nothing has wrapped yet — which is a
        // distinction only a long session exposes, i.e. a talk.
        rig.run(130.0, Some(TONE_MS));
        rig.run(60.0, None);

        let levels = rig.levels();
        assert_eq!(levels.coverage_s, 0.0, "a wrapped slot was read as current");
        assert_eq!(levels.leq, None);
    }

    // ── §14.2, case 3 ────────────────────────────────────────────────────────────────────

    #[test]
    fn exact_zeros_read_nothing_beside_zero_coverage() {
        let mut rig = Rig::new();
        // What a denied microphone actually delivers: callbacks, on time, of exact zeros.
        rig.run(60.0, Some(0.0));

        let levels = rig.levels();
        assert_eq!(levels.leq, None, "published a level for zero power");
        assert_eq!(
            levels.coverage_s, 0.0,
            "claimed a complete minute of a denied microphone"
        );
        assert_eq!(levels.now, None);
        assert_eq!(levels.max, None);
    }

    // ── §14.2, case 4 ────────────────────────────────────────────────────────────────────

    #[test]
    fn no_block_for_200ms_drops_now_and_leaves_the_max_alone() {
        let mut rig = Rig::new();
        rig.run(5.0, Some(TONE_MS));

        let live = rig.levels();
        let held = live.max.expect("max hold should exist after 5 s of tone");
        assert!(live.now.is_some());

        rig.run(0.25, None);
        let stale = rig.levels();
        assert_eq!(stale.now, None, "NOW survived the 200 ms staleness rule");
        // A hold is historical by nature, so it simply stops rising (spec §6.9).
        assert_db(stale.max, held);
    }

    #[test]
    fn now_is_absent_before_the_first_block_rather_than_minus_infinity() {
        let mut rig = Rig::new();
        let levels = rig.levels();
        // Spec §16.7's clause, doing its real work: `smoothed` is exactly 0.0 here.
        assert_eq!(levels.now, None);
        assert_eq!(levels.max, None);
        assert_eq!(levels.leq, None);
        assert_eq!(levels.coverage_s, 0.0);
    }

    // ── §14.2, case 6 ────────────────────────────────────────────────────────────────────

    #[test]
    fn lengthening_the_window_re_slices_rather_than_resetting() {
        let mut rig = Rig::new();
        rig.run(60.0, Some(TONE_MS));
        assert_coverage(rig.levels().coverage_s, 60.0);

        rig.m.set_window_s(120);
        let levels = rig.levels();
        // `60s of 120s`, not `0s of 120s` — a genuine 120 s average of everything there is.
        assert_coverage(levels.coverage_s, 60.0);
        assert_db(levels.leq, TONE_DB);

        // And shortening is instant and exact, using data already held.
        rig.m.set_window_s(10);
        assert_coverage(rig.levels().coverage_s, 10.0);
    }

    #[test]
    fn the_window_retires_energy_older_than_its_length() {
        let mut rig = Rig::new();
        rig.run(60.0, Some(TONE_MS));
        rig.run(60.0, Some(TONE_MS / 100.0));

        rig.m.set_window_s(60);
        let levels = rig.levels();
        assert_coverage(levels.coverage_s, 60.0);
        // The loud minute is entirely behind the window now.
        assert_db(levels.leq, TONE_DB - 20.0);
    }

    // ── §6.6 and §6.7, the two easy things to get subtly wrong ───────────────────────────

    #[test]
    fn the_smoother_is_exactly_the_continuous_one_pole_however_it_is_stepped() {
        for (weighting, tau) in [(TimeWeighting::Fast, 0.125), (TimeWeighting::Slow, 1.0)] {
            let mut rig = Rig::new();
            rig.m.set_time_weighting(weighting);
            let fed = rig.run(tau, Some(TONE_MS));

            let now = rig.levels().now.expect("a level after one τ");
            let reached = 10f64.powf(now / 10.0) / TONE_MS;

            // Advancing per block by that block's own `dt` telescopes exactly:
            // `1 − (1 − α)^k` with `1 − α = e^{−dt/τ}` is `1 − e^{−k·dt/τ}`. So the recursion is
            // *identical* to advancing continuously, and the assertion can be tight rather than
            // approximate — which is the property spec §6.6 relies on when it says the tick's
            // timing affects only what is painted, never what is measured.
            let expected = 1.0 - (-fed / tau).exp();
            assert!(
                (reached - expected).abs() < 1e-12,
                "{weighting:?} reached {reached} of the step after {fed} s, expected {expected}"
            );
            // And that is ~63% of the step at one τ, allowing for `fed` being a whole number of
            // 21.3 ms blocks rather than exactly τ.
            assert!((reached - 0.632_120_558).abs() < 0.06);
        }
    }

    #[test]
    fn the_max_catches_a_peak_that_falls_between_two_ticks() {
        let mut rig = Rig::new();
        rig.m.set_time_weighting(TimeWeighting::Fast);
        rig.run(2.0, Some(TONE_MS / 1000.0));

        // One loud block, then 100 ms of quiet before anyone looks. Sampling the smoother at
        // the 10 Hz tick would drop four peaks in five (spec §6.7); the compare runs inside the
        // drain loop instead.
        rig.now += BLOCK;
        rig.m.deposit(
            rig.now,
            BlockSummary {
                sum_sq: TONE_MS * FRAMES as f64,
                n: FRAMES,
                t: rig.now,
            },
        );
        rig.run(0.1, Some(TONE_MS / 1000.0));

        let levels = rig.levels();
        let now = levels.now.expect("a live level");
        let max = levels.max.expect("a max hold");
        assert!(
            max > now + 2.0,
            "max {max} dB is no higher than the tick-sampled {now} dB — the peak was missed"
        );
    }

    #[test]
    fn the_max_hold_survives_a_gap() {
        let mut rig = Rig::new();
        rig.run(5.0, Some(TONE_MS));
        let held = rig.levels().max.expect("a max hold");

        rig.run(120.0, None);
        let levels = rig.levels();
        assert_eq!(levels.leq, None, "the window should have emptied");
        assert_db(levels.max, held);
    }

    // ── §6.11, every row ─────────────────────────────────────────────────────────────────
    //
    // The fifth column of the table — the spectrogram's ring — is `b09`'s and empty on every
    // row. The sixth row, a calibration offset change, clears nothing *by construction* here:
    // no offset ever enters this module, so there is no code path for it to clear anything
    // through (spec §8.2, and `b04` owns the assertion that all three numbers shift by exactly
    // the delta).

    /// 60 s of tone, then 20 s at a different level, so both the window and the max hold are
    /// non-empty and distinguishable.
    fn charged() -> Rig {
        let mut rig = Rig::new();
        rig.run(20.0, Some(TONE_MS * 10.0));
        rig.run(40.0, Some(TONE_MS));
        rig
    }

    #[test]
    fn row_reset_clears_the_window_and_the_max() {
        let mut rig = charged();
        assert!(rig.levels().coverage_s > 0.0);
        assert!(rig.levels().max.is_some());

        rig.m.reset();

        let levels = rig.levels();
        assert_eq!(levels.coverage_s, 0.0);
        assert_eq!(levels.leq, None);
        assert_eq!(levels.max, None);
    }

    #[test]
    fn row_weighting_change_clears_the_window_and_the_max() {
        let mut rig = charged();
        rig.m.on_weighting_change();

        let levels = rig.levels();
        assert_eq!(levels.coverage_s, 0.0);
        assert_eq!(levels.leq, None);
        assert_eq!(levels.max, None);
    }

    #[test]
    fn row_time_weighting_change_clears_the_max_but_not_the_window() {
        let mut rig = charged();
        let before = rig.levels();
        let live = before.now.expect("a live level");

        rig.m.set_time_weighting(TimeWeighting::Fast);

        let after = rig.levels();
        // Reads as a bug unless you have read §6.7: L_CFmax and L_CSmax differ by several dB on
        // speech and neither is visible in the number itself, so the hold cannot carry over.
        assert_eq!(after.max, None);
        // The window is untouched — the L_eq accumulator never sees this setting, not even as a
        // flicker (clause 3.9 NOTE 3).
        assert_coverage(after.coverage_s, before.coverage_s);
        assert_db(after.leq, before.leq.unwrap());
        // And a τ change does not reset the smoother: the state is a smoothed mean square
        // either way, so the coefficient swaps and the state keeps (spec §6.6).
        assert_db(after.now, live);
    }

    #[test]
    fn row_window_length_change_clears_nothing() {
        let mut rig = charged();
        let before = rig.levels();

        rig.m.set_window_s(10);
        rig.m.set_window_s(60);

        let after = rig.levels();
        assert_coverage(after.coverage_s, before.coverage_s);
        assert_db(after.leq, before.leq.unwrap());
        assert_db(after.max, before.max.unwrap());
    }

    #[test]
    fn row_sample_rate_change_clears_neither() {
        let mut rig = charged();
        let before = rig.levels();
        let samples = before.coverage_s * FS;

        rig.m.set_sample_rate(44_100.0);

        let after = rig.levels();
        assert_db(after.leq, before.leq.unwrap());
        assert_db(after.max, before.max.unwrap());
        // Σn is what survives; the coverage figure simply re-reads it against the new rate.
        assert!(
            (after.coverage_s * 44_100.0 - samples).abs() < 1.0,
            "the ring lost samples across a rate change"
        );
    }

    #[test]
    fn the_calibration_slice_is_independent_of_the_window_setting() {
        let mut rig = Rig::new();
        rig.run(50.0, Some(TONE_MS * 10.0));
        rig.run(10.0, Some(TONE_MS));

        // Spec §8.3: a fixed 10 s slice, free from the ring being allocated at maximum length.
        for window_s in WINDOW_LENGTHS_S {
            rig.m.set_window_s(window_s);
            let (leq, coverage_s) = rig.m.leq_over(10);
            assert_db(leq, TONE_DB);
            assert_coverage(coverage_s, 10.0);
        }
    }
}
