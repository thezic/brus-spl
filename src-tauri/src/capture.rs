//! The cpal input stream, and the one queue that crosses out of the audio callback.
//!
//! The callback does the minimum spec §3.3 permits and nothing else: convert f32 → f64 at the
//! block boundary, take channel 0, run the weighting chain (`C` by default, spec §11.4), sum
//! `p²` and `n`, and push one
//! [`BlockSummary`] per block into a bounded lock-free SPSC queue. No locks, no allocation, no
//! logging, no `Mutex`. Everything downstream of that queue runs on the display side.
//!
//! The queue is bounded at ~1 s of blocks on purpose (spec §6.10): if the draining side
//! stalls, pushes fail and the blocks are simply never accounted for, so the failure shows up
//! as **lost coverage** — a number spec §6.4 publishes unconditionally — rather than as a
//! plausible, wrong level. Overflow degrades into a smaller honest sample, never a wrong
//! answer.

use std::sync::atomic::{AtomicU64, AtomicU8, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use rtrb::{Consumer, RingBuffer};
use serde::Serialize;

use crate::session;
use crate::weighting::{Weighting, WeightingChain};

/// One audio block, reduced to the only three things the metrics ring needs.
///
/// Sum-of-squares **and** count, never a mean: spec §6.1 divides by the actual sample total
/// across the whole window, so a per-block mean would have to be un-averaged to be usable.
#[derive(Debug, Clone, Copy)]
pub struct BlockSummary {
    /// Σ p² over the block, after weighting, in f64.
    pub sum_sq: f64,
    /// Samples that went into `sum_sq` — frames, not interleaved samples, since only
    /// channel 0 is read.
    pub n: u32,
    /// When the callback ran, on the monotonic clock. The ring is advanced by the clock at
    /// drain time (spec §6.2), so this is not what places the block; it is what tells the
    /// display side how long it has been since audio last arrived (spec §6.9's 200 ms).
    pub t: Instant,
}

/// What the stream and the session actually gave us, as opposed to what was asked for.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptureFacts {
    pub device: String,
    pub sample_format: String,
    /// Ground truth for every weighting coefficient and FFT bin downstream.
    pub sample_rate: u32,
    pub channels: u16,
    /// What the stream reports once running, which the CoreAudio backends recompute from the
    /// live device rather than echoing our request back.
    pub buffer_frames: Option<u32>,
    /// `None` off iOS, where there is no session to configure.
    pub session: Option<session::SessionFacts>,
}

/// Capture is either still coming up, running, or dead with a reason. There is no fourth
/// state at this tier — restarting a dead stream is `b07`'s job.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum CaptureState {
    Starting,
    Running(CaptureFacts),
    Failed { reason: String },
}

/// The cascade order the weighting selection is encoded in. Index, not identity, so the
/// selection can cross into the audio callback as one relaxed byte.
const CHAINS: [Weighting; 3] = [Weighting::A, Weighting::C, Weighting::Z];

fn chain_index(weighting: Weighting) -> u8 {
    match weighting {
        Weighting::A => 0,
        Weighting::C => 1,
        Weighting::Z => 2,
    }
}

/// All three chains, pre-built, with the live selection arriving as one atomic byte.
///
/// **The weighting setting has to reach a running audio callback, and the callback may not
/// allocate, lock or free** (spec §3.3). Deriving a chain on demand allocates a `Vec<Biquad>`;
/// receiving one from the settings side means freeing the outgoing one somewhere, and the
/// callback is not allowed to be that somewhere. Building all three up front costs three
/// `Vec`s of at most three biquads — a few hundred bytes, once per stream — and reduces the
/// whole problem to an index.
///
/// [`Chains::sync`] runs **once per block**, not per sample, and is where spec §6.11's third
/// column happens: the chain being switched *to* holds whatever state it held when it was last
/// live, so it is zeroed on the way in.
struct Chains {
    chains: [WeightingChain; 3],
    active: usize,
    /// Written by the command thread, read by the audio thread. `Relaxed` is sufficient: the
    /// byte is the only thing communicated, there is nothing it orders, and a switch one block
    /// late is 21 ms of the previous weighting — already inside the ±21 ms with which spec §6.3
    /// places a block in the ring.
    requested: Arc<AtomicU8>,
}

impl Chains {
    fn new(sample_rate: f64, requested: Arc<AtomicU8>) -> Chains {
        let active = requested.load(Ordering::Relaxed) as usize;
        Chains {
            chains: CHAINS.map(|weighting| WeightingChain::new(sample_rate, weighting)),
            active: active.min(CHAINS.len() - 1),
            requested,
        }
    }

    /// Picks up a weighting change, zeroing the incoming chain's state (spec §6.11).
    #[inline]
    fn sync(&mut self) {
        let requested = self.requested.load(Ordering::Relaxed) as usize;
        if requested == self.active || requested >= self.chains.len() {
            return;
        }
        // Zeroing is not hygiene. This chain last ran however long ago the mode was last
        // selected, so its state describes sound that is no longer in the room; left alone it
        // rings the old state out through a filter whose slowest pole sits at 20.6 Hz. Writing
        // zeros over `Vec<Biquad>` state allocates nothing, which is why it is allowed here.
        self.chains[requested].reset();
        self.active = requested;
    }

    #[inline]
    fn process(&mut self, sample: f64) -> f64 {
        self.chains[self.active].process(sample)
    }
}

/// The capture side, owned by the app and shared with the audio callback.
pub struct Capture {
    state: Mutex<CaptureState>,
    /// `None` until the stream is built. Locked only by the draining side; the callback never
    /// touches it.
    consumer: Mutex<Option<Consumer<BlockSummary>>>,
    /// Blocks the queue had no room for. Bumped from the callback, which is why it is an
    /// atomic and not a counter behind the same lock.
    dropped: Arc<AtomicU64>,
    /// Errors delivered to cpal's error callback, which runs on a notification queue rather
    /// than the audio thread — so locking here is fine.
    last_error: Arc<Mutex<Option<String>>>,
    /// The selected weighting, as an index into [`CHAINS`]. Set before the stream exists and
    /// read by the callback once per block.
    weighting: Arc<AtomicU8>,
}

impl Capture {
    /// Starts capture on a dedicated thread and returns immediately.
    ///
    /// Non-blocking on purpose. [`session::configure`] waits on a permission prompt whose
    /// completion block is delivered on the main queue, so doing any of this inline in Tauri's
    /// `setup` — which runs on the main thread — would deadlock the app before it drew a
    /// frame.
    ///
    /// `weighting` is the **persisted** setting, passed in rather than defaulted so the first
    /// block of the session is already filtered the way the reader left it (`b04`).
    pub fn start(weighting: Weighting) -> Arc<Capture> {
        let capture = Arc::new(Capture {
            state: Mutex::new(CaptureState::Starting),
            consumer: Mutex::new(None),
            dropped: Arc::new(AtomicU64::new(0)),
            last_error: Arc::new(Mutex::new(None)),
            weighting: Arc::new(AtomicU8::new(chain_index(weighting))),
        });

        let owned = Arc::clone(&capture);
        std::thread::Builder::new()
            .name("capture".into())
            .spawn(move || {
                match build(&owned) {
                    Ok(stream) => {
                        // `cpal::Stream` is `!Send` and stops on drop, so it has to stay on
                        // the thread that built it, alive for as long as the app is. Parking
                        // is where `b07`'s supervisor loop goes.
                        let _stream = stream;
                        loop {
                            std::thread::park();
                        }
                    }
                    Err(reason) => {
                        eprintln!("capture: {reason}");
                        *owned.state.lock().unwrap() = CaptureState::Failed { reason };
                    }
                }
            })
            .expect("spawn capture thread");

        capture
    }

    /// Takes every block summary waiting in the queue, oldest first, and returns how many.
    ///
    /// This is the seam the metrics pipeline drains at 10 Hz (`b03`, `b05`).
    pub fn drain<F: FnMut(BlockSummary)>(&self, mut f: F) -> usize {
        let mut guard = self.consumer.lock().unwrap();
        let Some(consumer) = guard.as_mut() else {
            return 0;
        };
        let mut drained = 0;
        while let Ok(summary) = consumer.pop() {
            f(summary);
            drained += 1;
        }
        drained
    }

    pub fn state(&self) -> CaptureState {
        self.state.lock().unwrap().clone()
    }

    /// Blocks lost to queue overflow since startup. Lost coverage, by construction.
    pub fn dropped(&self) -> u64 {
        self.dropped.load(Ordering::Relaxed)
    }

    /// The most recent stream error, if any. Detail lives in the log; this exists so the
    /// device readout is not silent about a stream that died (spec §9.4's `unavailable`
    /// conflates causes on purpose).
    pub fn last_error(&self) -> Option<String> {
        self.last_error.lock().unwrap().clone()
    }

    /// Switches the weighting on a **running** stream, zeroing the incoming chain's filter state
    /// (spec §6.11's second row, third column).
    ///
    /// The callback picks this up at its next block boundary, so up to one block — 21 ms at
    /// 48 kHz — is still weighted the old way after this returns. The caller must therefore
    /// **drain the queue before clearing the window**, or that block lands in the newly cleared
    /// window carrying energy summed through the previous filter. Order at the command site:
    /// this call, then [`Capture::drain`] discarding what comes out, then
    /// [`crate::metrics::Metrics::on_weighting_change`].
    pub fn set_weighting(&self, weighting: Weighting) {
        self.weighting
            .store(chain_index(weighting), Ordering::Relaxed);
    }

    /// The selected weighting. This is the callback's view, which lags a
    /// [`Capture::set_weighting`] by at most one block; the authoritative setting is the one in
    /// [`crate::settings`].
    pub fn weighting(&self) -> Weighting {
        CHAINS[(self.weighting.load(Ordering::Relaxed) as usize).min(CHAINS.len() - 1)]
    }
}

/// Configures the session, builds the stream, and publishes the facts. Runs on the capture
/// thread; never on the main thread.
fn build(capture: &Arc<Capture>) -> Result<cpal::Stream, String> {
    // Before cpal is touched. Apple's default `SoloAmbient` category permits no input, and
    // cpal's iOS backend reads the live session at call time — so a stream built first would
    // be built against the wrong rate even if it built at all.
    let session = session::configure()?;

    let host = cpal::default_host();
    let device = host
        .default_input_device()
        .ok_or_else(|| "no default input device".to_string())?;
    let device_name = device
        .id()
        .map(|id| id.to_string())
        .unwrap_or_else(|_| "<unnamed>".to_string());

    let supported = device
        .default_input_config()
        .map_err(|e| format!("default_input_config: {e}"))?;
    let sample_format = supported.sample_format();
    if sample_format != cpal::SampleFormat::F32 {
        // Both CoreAudio backends hand us f32, which is what spec §3.3 is written against.
        // Anything else would need a conversion the callback should not be doing, so it is a
        // refusal rather than a silent widening.
        return Err(format!(
            "input device reports {sample_format} samples; only f32 is supported"
        ));
    }
    let mut config: cpal::StreamConfig = supported.into();

    // Cross-check cpal against the session. cpal's iOS backend reports the session's current
    // rate as a single-point range, so these must agree — if they do not, one of the two is
    // reading stale state and every weighting coefficient downstream is derived from the
    // wrong number.
    if let Some(facts) = &session {
        if facts.sample_rate as u32 != config.sample_rate {
            eprintln!(
                "capture: rate disagreement — AVAudioSession says {} Hz, cpal says {} Hz",
                facts.sample_rate, config.sample_rate
            );
        }
    }

    config.buffer_size = cpal::BufferSize::Fixed(session::PREFERRED_BUFFER_FRAMES);

    // ~1 s of blocks (spec §6.10). Deliberately not generous: a queue big enough to ride out a
    // long stall would let stale blocks land in the wrong 100 ms slot, which is a wrong number
    // where a dropped block is only a smaller one.
    let capacity = ((config.sample_rate as f64 / session::PREFERRED_BUFFER_FRAMES as f64).ceil()
        as usize)
        .clamp(16, 512);
    let (mut producer, consumer) = RingBuffer::<BlockSummary>::new(capacity);

    // Interleaved frame stride. Channel 0 only when the stream reports more than one channel
    // (spec §3.3, §16.9): averaging correlated channels moves the level by up to 6 dB and
    // would make the calibration offset depend on the channel count. `chunks_exact(1)` is the
    // mono case, so there is one loop and one place the weighting chain plugs in.
    let stride = config.channels.max(1) as usize;
    let dropped = Arc::clone(&capture.dropped);

    // The weighting chains live in the callback and are derived here, from the **granted** rate
    // — this is the only place that rate is known, and every coefficient depends on it. All
    // three are built because the setting can change on a running stream and the callback may
    // not allocate; see [`Chains`].
    //
    // `WeightingChain::new` panics below 2 kHz, which on the capture thread would take the app
    // down silently, so the refusal is explicit and names the rate.
    if (config.sample_rate as f64) <= 2000.0 {
        return Err(format!(
            "input rate {} Hz puts the 1 kHz weighting reference at or above Nyquist",
            config.sample_rate
        ));
    }
    let mut chains = Chains::new(config.sample_rate as f64, Arc::clone(&capture.weighting));

    let last_error = Arc::clone(&capture.last_error);
    let error_callback = move |err: cpal::Error| {
        // Not the audio thread: cpal delivers this on a notification queue.
        eprintln!("capture: stream error {:?}: {err}", err.kind());
        *last_error.lock().unwrap() = Some(format!("{:?}: {err}", err.kind()));
    };

    let stream = device
        .build_input_stream(
            config,
            move |data: &[f32], _: &cpal::InputCallbackInfo| {
                let t = Instant::now();
                let mut sum_sq = 0.0f64;
                let mut n: u32 = 0;

                // One relaxed load per block, outside the sample loop. A weighting change is
                // picked up here and nowhere else, so a block is never half one weighting and
                // half another.
                chains.sync();

                for frame in data.chunks_exact(stride) {
                    // f32 → f64 at the block boundary, and all DSP in f64 from here on: `03`
                    // measured f32 filter state degrading the level by +1.7 dB with a DC
                    // offset and +3.9 dB with 5 Hz rumble, which is exactly what a phone
                    // microphone in a venue delivers.
                    let x = frame[0] as f64;

                    // ─── the weighting seam, closed by `b03`, switchable by `b04` ──────
                    // No DC blocker here or anywhere: the A and C filters are themselves
                    // high-passes, f64 removes the precision motive, and the FFT tap wants
                    // the DC (spec §3.3).
                    let y = chains.process(x);
                    // ───────────────────────────────────────────────────────────────────

                    sum_sq += y * y;
                    n += 1;
                }

                if producer.push(BlockSummary { sum_sq, n, t }).is_err() {
                    dropped.fetch_add(1, Ordering::Relaxed);
                }
            },
            error_callback,
            None,
        )
        .map_err(|e| format!("build_input_stream: {e}"))?;
    stream.play().map_err(|e| format!("play: {e}"))?;

    let facts = CaptureFacts {
        device: device_name,
        sample_format: sample_format.to_string(),
        sample_rate: config.sample_rate,
        channels: config.channels,
        buffer_frames: stream.buffer_size().ok(),
        session,
    };
    eprintln!("capture: running {facts:?}");

    *capture.consumer.lock().unwrap() = Some(consumer);
    *capture.state.lock().unwrap() = CaptureState::Running(facts);

    Ok(stream)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FS: f64 = 48_000.0;
    /// The block the granted route actually delivers (`b01` finding 3), so `sync` is exercised
    /// at the rate the callback really calls it.
    const FRAMES: usize = 1024;

    /// A tone at a frequency where the three weightings are far apart: C reads −3.0 dB at
    /// 31.5 Hz, A reads −39.4 dB, Z reads 0. Nothing here can pass by accident.
    fn tone(hz: f64, samples: usize, phase: usize) -> Vec<f64> {
        (0..samples)
            .map(|i| (std::f64::consts::TAU * hz * (i + phase) as f64 / FS).sin())
            .collect()
    }

    /// Runs `blocks` blocks of `hz` through `chains`, calling `sync` once per block exactly as
    /// the audio callback does, and returns Σp² over the whole run.
    ///
    /// Deliberately **not** per block: `b02` finding 4 measured `sin²` averaged over a partial
    /// cycle biased by up to 1 dB at these frequencies, and 1024 samples of 31.5 Hz is two thirds
    /// of a cycle. Summing over the whole run puts the bias under 0.04 dB without pretending the
    /// callback's block size is anything other than 1024.
    fn run(chains: &mut Chains, hz: f64, blocks: usize) -> f64 {
        let mut sum_sq = 0.0;
        for block in 0..blocks {
            chains.sync();
            for x in tone(hz, FRAMES, block * FRAMES) {
                let y = chains.process(x);
                sum_sq += y * y;
            }
        }
        sum_sq
    }

    /// The mean-square level of a run, in dB re FS.
    fn db(sum_sq: f64, blocks: usize) -> f64 {
        10.0 * (sum_sq / (blocks * FRAMES) as f64).log10()
    }

    fn chains_at(weighting: Weighting) -> (Chains, Arc<AtomicU8>) {
        let requested = Arc::new(AtomicU8::new(chain_index(weighting)));
        (Chains::new(FS, Arc::clone(&requested)), requested)
    }

    #[test]
    fn the_selection_starts_where_the_persisted_setting_says() {
        for weighting in CHAINS {
            let (chains, _) = chains_at(weighting);
            assert_eq!(CHAINS[chains.active], weighting);
        }
    }

    /// 31.5 Hz, where the standard's own table puts the three modes 0 / −3.0 / −39.4 dB apart.
    /// Asserted as **differences from Z**, not as absolute levels: the absolute accuracy of the
    /// filters is `b02`'s 21 tests, and what this ticket adds is only that a switch reaches the
    /// running callback and selects the right chain.
    #[test]
    fn switching_weighting_on_a_running_stream_changes_the_level() {
        const BLOCKS: usize = 40;
        let (mut chains, requested) = chains_at(Weighting::Z);
        let z = db(run(&mut chains, 31.5, BLOCKS), BLOCKS);

        // Exactly what `Capture::set_weighting` does, from another thread's point of view.
        requested.store(chain_index(Weighting::A), Ordering::Relaxed);
        let a = db(run(&mut chains, 31.5, BLOCKS), BLOCKS);
        assert!(
            (z - a - 39.4).abs() < 0.2,
            "A sat {} dB below Z at 31.5 Hz, expected 39.4",
            z - a
        );

        requested.store(chain_index(Weighting::C), Ordering::Relaxed);
        let c = db(run(&mut chains, 31.5, BLOCKS), BLOCKS);
        assert!(
            (z - c - 3.0).abs() < 0.1,
            "C sat {} dB below Z at 31.5 Hz, expected 3.0",
            z - c
        );
    }

    /// Spec §6.11's third column. A chain that was live minutes ago holds state describing sound
    /// no longer in the room, and its slowest pole is at 20.6 Hz — left alone it rings that state
    /// out into the freshly cleared window. The assertion is exact: switching back must give
    /// **bit-identical** output to a chain that has never run.
    #[test]
    fn switching_zeroes_the_filter_state_of_the_chain_being_switched_to() {
        let (mut chains, requested) = chains_at(Weighting::C);
        // Warm C on something with plenty of low-frequency energy to store.
        run(&mut chains, 31.5, 20);

        // Away and back, which is the case a `reset` on the *outgoing* chain would miss.
        requested.store(chain_index(Weighting::A), Ordering::Relaxed);
        run(&mut chains, 400.0, 5);
        requested.store(chain_index(Weighting::C), Ordering::Relaxed);
        let reused = run(&mut chains, 31.5, 3);

        let (mut cold, _) = chains_at(Weighting::C);
        let fresh = run(&mut cold, 31.5, 3);

        assert_eq!(
            reused, fresh,
            "a re-selected chain did not start from zero state"
        );
    }

    /// The two halves of the same property, stated at zero tolerance because silence has no
    /// windowing bias: **a chain fed silence must be silent, and a chain left warm is not.**
    ///
    /// The second assertion is what gives the first one teeth — without it, `reset` could be a
    /// no-op and the test would still pass.
    #[test]
    fn a_re_selected_chain_is_silent_when_the_audio_stops_and_a_warm_one_rings() {
        let silence = vec![0.0f64; FRAMES];

        let (mut chains, requested) = chains_at(Weighting::C);
        run(&mut chains, 31.5, 20);
        requested.store(chain_index(Weighting::A), Ordering::Relaxed);
        run(&mut chains, 400.0, 5);
        requested.store(chain_index(Weighting::C), Ordering::Relaxed);
        chains.sync();
        let after_switch: f64 = silence.iter().map(|&x| chains.process(x).powi(2)).sum();
        assert_eq!(
            after_switch, 0.0,
            "a re-selected chain rang out state from the last time it was live"
        );

        let (mut warm, _) = chains_at(Weighting::C);
        run(&mut warm, 31.5, 20);
        let ringing: f64 = silence.iter().map(|&x| warm.process(x).powi(2)).sum();
        assert!(
            ringing > 0.0,
            "nothing to zero away: a warm C chain fed silence produced silence"
        );
    }

    /// A byte outside the three modes can only come from a bug on the writing side, and the
    /// callback is the one place a panic is unrecoverable — it takes the audio thread with it.
    #[test]
    fn a_selection_byte_that_names_no_mode_is_ignored_rather_than_panicking() {
        let requested = Arc::new(AtomicU8::new(9));
        let mut chains = Chains::new(FS, Arc::clone(&requested));
        assert!(chains.active < CHAINS.len());
        let before = run(&mut chains, 31.5, 3);

        requested.store(200, Ordering::Relaxed);
        let after = run(&mut chains, 31.5, 3);
        assert_eq!(before, after, "an out-of-range byte changed the weighting");
    }
}
