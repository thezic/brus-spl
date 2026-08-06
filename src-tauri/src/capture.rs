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
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

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

/// Capture is either still coming up, running, or dead with a reason.
///
/// **`b07` deliberately added no fourth state.** A rebuild is a stream coming up, so it reports
/// `Starting`; a rebuild that failed is a stream that is dead with a reason, so it reports
/// `Failed`. Both map to spec §9.4's `unavailable`, which is what the screen should say while
/// there is no input — and §9.4 has exactly three display states, so a `Recovering` variant would
/// have been a distinction the reader never sees. The log carries what the states cannot.
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

/// How long the supervisor waits before another attempt after a failed rebuild.
///
/// Fixed rather than backed off, and never given up on: at a venue the meter is on a stand
/// across the room, so the only acceptable end state is *running*. Two attempts a second is
/// nothing next to the audio callback, and the cost of the wrong answer here is the whole talk.
/// It is also the retry cadence for the doomed attempts made *during* an interruption — see
/// [`supervise`].
const RETRY_INTERVAL: Duration = Duration::from_millis(500);

/// The one-slot mailbox the supervisor loop waits on: *a rebuild is wanted, and here is why*.
///
/// Three unrelated things post to it — the interruption observer, cpal's error callback, and the
/// 10 Hz health check — and none of them may block, allocate under a lock, or care whether the
/// others already fired. **Only the first reason of a run is kept**: the health check re-raises
/// ten times a second for as long as the session is down, and the reason worth reading is the one
/// that arrived first, not the hundredth copy of the symptom.
struct Recovery {
    wanted: Mutex<Option<String>>,
    signal: Condvar,
}

impl Recovery {
    fn new() -> Recovery {
        Recovery {
            wanted: Mutex::new(None),
            signal: Condvar::new(),
        }
    }

    /// Asks for a rebuild. The reason is built **only if the mailbox is empty**, which is what
    /// keeps the 10 Hz health check from formatting a `String` ten times a second.
    fn request<F: FnOnce() -> String>(&self, reason: F) {
        let mut wanted = self.wanted.lock().unwrap();
        if wanted.is_none() {
            *wanted = Some(reason());
            self.signal.notify_one();
        }
    }

    /// Blocks until a rebuild is wanted, then takes the reason and empties the mailbox.
    ///
    /// Emptied **before** the rebuild rather than after, so a request that arrives while a build
    /// is in flight survives it and causes another round rather than being swallowed.
    fn take(&self) -> String {
        let mut wanted = self.wanted.lock().unwrap();
        loop {
            if let Some(reason) = wanted.take() {
                return reason;
            }
            wanted = self.signal.wait(wanted).unwrap();
        }
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
    /// Spec §4.2's three recovery triggers, collapsed into one mailbox.
    recovery: Arc<Recovery>,
    /// Streams successfully built since startup. **The first is startup; anything above one is a
    /// recovery**, so this is the only place a rebuild is *observable* rather than merely logged.
    ///
    /// A diagnostic in the same sense as [`Capture::dropped`] — it does not cross the bridge and
    /// nothing on screen is derived from it. It exists because "did it come back on its own" is a
    /// question with a yes-or-no answer, and reading it off a log is not one.
    builds: AtomicU64,
}

impl Capture {
    /// The capture side before anything has been started: no session, no thread, no stream.
    ///
    /// Split out of [`Capture::start`] so `b05`'s command site can be exercised **with no
    /// hardware and no sleeping** — the property `b03` designed the metrics pipeline around. An
    /// unstarted `Capture` reports [`CaptureState::Starting`], drains nothing, and still carries
    /// the weighting selection, which is all the commands touch.
    pub fn new(weighting: Weighting) -> Arc<Capture> {
        Arc::new(Capture {
            state: Mutex::new(CaptureState::Starting),
            consumer: Mutex::new(None),
            dropped: Arc::new(AtomicU64::new(0)),
            last_error: Arc::new(Mutex::new(None)),
            weighting: Arc::new(AtomicU8::new(chain_index(weighting))),
            recovery: Arc::new(Recovery::new()),
            builds: AtomicU64::new(0),
        })
    }

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
        let capture = Capture::new(weighting);
        let owned = Arc::clone(&capture);
        std::thread::Builder::new()
            .name("capture".into())
            .spawn(move || supervise(owned))
            .expect("spawn capture thread");

        capture
    }

    /// Spec §4.2 item 4's health check, run on the 10 Hz tick with the state the tick already
    /// read.
    ///
    /// Takes the state rather than re-reading it so the tick's own snapshot is what decides, and
    /// so the rule itself is [`needs_recovery`] — a pure function, which is the only part of this
    /// path that can be tested anywhere but on a phone.
    pub fn check_health(&self, state: &CaptureState) {
        if needs_recovery(state, session::input_channels()) {
            self.recovery
                .request(|| "session reports no input channels".into());
        }
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

    /// Streams successfully built since startup.
    ///
    /// **On a device a clean launch reads `2`, not `1`** (`b08`): the audio route settles a few tens
    /// of ms after the session activates and cpal raises `StreamInvalidated: Audio route changed`,
    /// so the supervisor rebuilds once before anything has been measured. Anything above two is a
    /// recovery from something that happened during the session.
    pub fn builds(&self) -> u64 {
        self.builds.load(Ordering::Relaxed)
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

/// cpal's error callback: record the error and take the same recovery path as everything else
/// (spec §4.2 item 5).
///
/// **Every** error, without triaging by kind. cpal 0.18.1 documents `DeviceChanged` as *"the
/// stream remains active and no rebuild is required"*, which is true of the samples and not of
/// this instrument: a reroute can move the sample rate, and the weighting coefficients and the
/// published `CaptureFacts` are both derived from the rate read back at build time. Rebuilding is
/// how they follow the route. The cost of the extra rebuild is a fraction of a second of coverage;
/// the cost of skipping it is every number afterwards being filtered for the wrong rate.
///
/// **This callback is a live path on iOS, and `b08` watched it fire.** cpal 0.18.1's iOS backend
/// carries a `session_event_manager` that observes `AVAudioSessionRouteChangeNotification` and maps
/// the reasons onto error kinds — `OldDeviceUnavailable` (a headset unplugged) to `DeviceChanged`,
/// `CategoryChange`/`Override`/`RouteConfigurationChange` to `StreamInvalidated`,
/// `NoSuitableRouteForCategory` to `DeviceNotAvailable`. On device the route settles a few tens of
/// ms after launch and delivers `StreamInvalidated: Audio route changed`; the supervisor rebuilt
/// and was running again 70 ms later, before anything had been measured.
///
/// What cpal does **not** observe is `AVAudioSessionInterruptionNotification` — it watches route
/// changes and media-services loss/reset and nothing else — which is why spec §4.2 item 2's
/// observer is forced rather than chosen, and why `11` probe 2's Siri interruption reached this
/// callback not once. Route changes announce themselves; interruptions do not.
///
/// Split out of [`build`] so it can be tested with a synthetic `cpal::Error`, which is the only
/// one of the three triggers that can be fired anywhere but on a device.
fn stream_error_callback(
    last_error: Arc<Mutex<Option<String>>>,
    recovery: Arc<Recovery>,
) -> impl FnMut(cpal::Error) + Send + 'static {
    move |err: cpal::Error| {
        // Not the audio thread: cpal delivers this on a notification queue, so locking is fine.
        let kind = err.kind();
        eprintln!("capture: stream error {kind:?}: {err}");
        *last_error.lock().unwrap() = Some(format!("{kind:?}: {err}"));
        recovery.request(|| format!("stream error {kind:?}"));
    }
}

/// Spec §4.2 item 4, as a rule rather than as a poll.
///
/// **`inputNumberOfChannels == 0` is the whole signal.** `11` probe 2 invoked Siri three seconds
/// into a run and the stream died permanently: no error reached cpal's error callback, no
/// all-zero blocks arrived, nothing arrived at all — and this one property went 1 → 0 and stayed
/// there, meaning the session had been left *deactivated*.
///
/// Only a stream that claims to be `Running` is worth checking. `Starting` and `Failed` are
/// already the supervisor's business, and asking for a rebuild of a rebuild that is in flight
/// would tear down the stream it had just finished building.
fn needs_recovery(state: &CaptureState, input_channels: Option<isize>) -> bool {
    matches!(state, CaptureState::Running(_)) && input_channels.is_some_and(|channels| channels < 1)
}

/// The capture thread: build the stream, then rebuild it whenever anything says to (spec §4.2).
///
/// It owns the `cpal::Stream` for the life of the app because it has to — a stream is `!Send` and
/// stops on drop, so it cannot be handed anywhere else and cannot be left un-owned. That is also
/// why *this* loop does the rebuilding rather than whichever thread noticed the problem.
///
/// **A failed attempt re-arms rather than ending the thread.** During a Siri call the health check
/// asks for a rebuild ten times a second and every `setActive(true)` is refused while another
/// process holds the session, so failure is the *normal* path for the length of the interruption
/// and recovery is simply the first attempt that succeeds. That also makes a Mac with no input
/// device, or a permission prompt answered slowly, a retry instead of a permanently dead meter.
///
/// Logging is once per run of failures, like the tick's emit failures: two attempts a second for a
/// 30 s interruption would otherwise be sixty identical lines around the one line that matters.
fn supervise(capture: Arc<Capture>) {
    // Registered before the first build, so an interruption during startup is not missed, and
    // held for the life of the thread — which is the life of the app.
    let _observer = session::observe_interruptions({
        let recovery = Arc::clone(&capture.recovery);
        move |interruption| {
            // `Began` is not actionable: the session belongs to something else until it ends,
            // and §6.4's coverage figure is already reporting the hole. Waking the supervisor
            // here would only spend the interruption failing.
            if interruption == session::Interruption::Ended {
                recovery.request(|| "interruption ended".into());
            }
        }
    });

    let mut reported = false;
    // `Initial` until a build has actually succeeded once, not merely until the first attempt.
    // The one thing the recovery path skips is the permission prompt, and a first attempt that
    // failed *before* raising it — `setCategory` refused, say — would otherwise leave an app that
    // retries forever and never asks for the microphone.
    let mut activation = session::Activation::Initial;
    let mut stream = attempt(&capture, activation, &mut reported);
    if stream.is_some() {
        activation = session::Activation::Recovery;
    }

    loop {
        let reason = capture.recovery.take();
        // Published before the stream is dropped rather than after, so [`needs_recovery`] stops
        // seeing a `Running` stream the instant one is no longer wanted. The remaining race — a
        // health check landing between the condvar wake and this line — costs one redundant
        // rebuild and a fraction of a second of coverage; closing it properly would mean holding
        // the state lock across a blocking wait, which is a worse trade.
        *capture.state.lock().unwrap() = CaptureState::Starting;
        if !reported {
            eprintln!("capture: rebuilding — {reason}");
        }
        // Dropped **before** the session is touched: a `cpal::Stream` stops on drop, and a
        // half-live stream over a session being reactivated underneath it is a state nobody has
        // characterised. Up to 100 ms of undrained blocks go with it, which shows up as lost
        // coverage — the failure mode this whole design prefers.
        drop(stream.take());
        stream = attempt(&capture, activation, &mut reported);
        if stream.is_some() {
            activation = session::Activation::Recovery;
        }
    }
}

/// One build attempt: publish what happened, and on failure wait out [`RETRY_INTERVAL`] and ask
/// for another.
fn attempt(
    capture: &Arc<Capture>,
    activation: session::Activation,
    reported: &mut bool,
) -> Option<cpal::Stream> {
    match build(capture, activation) {
        Ok(stream) => {
            *reported = false;
            // The error that killed the last stream describes a stream that no longer exists.
            *capture.last_error.lock().unwrap() = None;
            capture.builds.fetch_add(1, Ordering::Relaxed);
            Some(stream)
        }
        Err(reason) => {
            if !*reported {
                eprintln!("capture: {reason}");
                *reported = true;
            }
            *capture.state.lock().unwrap() = CaptureState::Failed {
                reason: reason.clone(),
            };
            std::thread::sleep(RETRY_INTERVAL);
            capture
                .recovery
                .request(|| format!("retrying after {reason}"));
            None
        }
    }
}

/// Configures the session, builds the stream, and publishes the facts. Runs on the capture
/// thread; never on the main thread.
///
/// **This is the whole of spec §4.2 item 6.** A rebuild derives the weighting coefficients from
/// the rate it reads back below, and a fresh [`Chains`] starts from zero state — so recomputing
/// the coefficients for a new route and zeroing the filter state are not steps in the recovery
/// path, they are consequences of there being exactly one path that builds a stream. Nothing here
/// touches [`crate::metrics`], which is the other half of item 6: the ring keeps its slots,
/// because the same quantity through a differently-designed filter is still valid energy.
fn build(capture: &Arc<Capture>, activation: session::Activation) -> Result<cpal::Stream, String> {
    // Before cpal is touched. Apple's default `SoloAmbient` category permits no input, and
    // cpal's iOS backend reads the live session at call time — so a stream built first would
    // be built against the wrong rate even if it built at all. On the recovery path this is
    // also the `setActive(true)` spec §4.2 item 3 requires, without which the rebuild below
    // fails with `InvalidInput: channel count must be at least 1` — the same error as a
    // never-set category, and the reason that trap is written down.
    let session = session::configure(activation)?;

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

    let error_callback = stream_error_callback(
        Arc::clone(&capture.last_error),
        Arc::clone(&capture.recovery),
    );

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

    // ── `b07`: interruption and recovery (spec §4.2) ─────────────────────────────────────────
    //
    // The three triggers are an iOS notification, an iOS session property and a cpal callback,
    // and only the last of the three can be fired anywhere but on a phone. What is testable here
    // is therefore the *rules* — when a rebuild is asked for, and what happens to the request —
    // and the device pass is what proves a rebuild actually recovers a stream.

    fn running() -> CaptureState {
        CaptureState::Running(CaptureFacts {
            device: "test".into(),
            sample_format: "f32".into(),
            sample_rate: 48_000,
            channels: 1,
            buffer_frames: Some(FRAMES as u32),
            session: None,
        })
    }

    /// Spec §4.2 item 4, which is `11` probe 2's one measured symptom: an interruption took
    /// `inputNumberOfChannels` **1 → 0 and left it there**, with no stream error, no all-zero
    /// blocks and nothing else to see.
    ///
    /// The two negative rows are the ones with teeth. A rebuild in flight publishes `Starting`,
    /// and a health check that fired on it would ask the supervisor to tear down the stream it
    /// had just finished building — the loop feeding itself forever, one rebuild per tick.
    #[test]
    fn the_health_check_fires_only_on_a_running_stream_reporting_no_input_channels() {
        assert!(needs_recovery(&running(), Some(0)));
        assert!(!needs_recovery(&running(), Some(1)));
        assert!(!needs_recovery(&running(), Some(2)));

        assert!(!needs_recovery(&CaptureState::Starting, Some(0)));
        assert!(!needs_recovery(
            &CaptureState::Failed {
                reason: "no default input device".into()
            },
            Some(0)
        ));

        // Off iOS there is no session to interrogate, so the check has no opinion at all.
        assert!(!needs_recovery(&running(), None));
    }

    /// Spec §9.4's desktop caveat wearing this ticket's costume, asserted rather than noted:
    /// there is no `AVAudioSession` off iOS, so item 4's health check **cannot fire on the desk**
    /// and [the Tier 1 device pass](../../../.scratch/spl-meter-build/issues/b08-tier-1-device-pass.md)
    /// is the only place it is real.
    #[test]
    #[cfg(not(target_os = "ios"))]
    fn off_ios_there_is_no_session_to_poll_so_the_health_check_never_asks() {
        assert_eq!(session::input_channels(), None);

        let capture = Capture::new(Weighting::C);
        capture.check_health(&running());
        assert!(
            capture.recovery.wanted.lock().unwrap().is_none(),
            "the desk asked for a rebuild it has no way to know it needs"
        );
    }

    /// **Only the first reason of a run survives, and building the later ones never happens.**
    ///
    /// The health check asks ten times a second for as long as the session is down, so a mailbox
    /// that overwrote would replace *interruption ended* with the hundredth copy of the symptom
    /// it caused — and one that formatted eagerly would allocate a `String` at 10 Hz forever for
    /// a value it then throws away.
    #[test]
    fn a_queued_request_keeps_the_first_reason_and_never_builds_the_others() {
        let recovery = Recovery::new();
        recovery.request(|| "interruption ended".into());

        let mut formatted = 0;
        for _ in 0..100 {
            recovery.request(|| {
                formatted += 1;
                "session reports no input channels".into()
            });
        }
        assert_eq!(formatted, 0, "a queued request formatted its reason anyway");

        assert_eq!(recovery.take(), "interruption ended");
        assert!(
            recovery.wanted.lock().unwrap().is_none(),
            "taking a request left it in the mailbox"
        );
    }

    /// The supervisor spends almost all of its life blocked in `take`, so the wake-up is the whole
    /// mechanism: a trigger that set the flag without waking it would recover on the *next*
    /// interruption rather than this one, which on a phone is never.
    #[test]
    fn a_request_wakes_a_supervisor_that_is_already_waiting() {
        let recovery = Arc::new(Recovery::new());
        let (tx, rx) = std::sync::mpsc::channel();

        let waiting = Arc::clone(&recovery);
        std::thread::spawn(move || {
            let _ = tx.send(waiting.take());
        });
        // Long enough that the thread is inside `wait` rather than about to enter it. The
        // loop-around-the-wait is what makes the other ordering work too, but this is the
        // ordering that needs the condvar.
        std::thread::sleep(Duration::from_millis(50));

        recovery.request(|| "interruption ended".into());
        assert_eq!(
            rx.recv_timeout(Duration::from_secs(5))
                .expect("the waiting supervisor was never woken"),
            "interruption ended"
        );
    }

    /// Spec §4.2 item 5: **any** stream error takes the recovery path, without triaging by kind.
    ///
    /// `DeviceChanged` is the interesting row, because cpal 0.18.1 documents it as *"the stream
    /// remains active and no rebuild is required"* — true of the samples, false of this
    /// instrument. The weighting coefficients and the published `CaptureFacts` are both derived
    /// from the rate read back when the stream was built, so a reroute that cpal handles silently
    /// still leaves them describing the old route. Rebuilding is how they follow it.
    ///
    /// The three kinds are not arbitrary: they are exactly what cpal's iOS `session_event_manager`
    /// raises for a route change — `DeviceChanged` for a headset unplugged, `StreamInvalidated` for
    /// a category or configuration change, `DeviceNotAvailable` when no route suits the category.
    /// `b08` watched the second of them arrive on device at launch and the supervisor recover from
    /// it, so this table is the device's own behaviour rather than a guess at it.
    #[test]
    fn any_stream_error_asks_for_a_rebuild_and_records_itself() {
        for kind in [
            cpal::ErrorKind::DeviceChanged,
            cpal::ErrorKind::StreamInvalidated,
            cpal::ErrorKind::DeviceNotAvailable,
            cpal::ErrorKind::InvalidInput,
        ] {
            let last_error = Arc::new(Mutex::new(None));
            let recovery = Arc::new(Recovery::new());
            let mut on_error =
                stream_error_callback(Arc::clone(&last_error), Arc::clone(&recovery));

            on_error(cpal::Error::new(kind));

            // The mailbox is read directly rather than through `take`, which blocks: a callback
            // that recorded the error and asked for nothing would otherwise hang this test rather
            // than fail it, and a hang is not a result.
            assert_eq!(
                recovery.wanted.lock().unwrap().take(),
                Some(format!("stream error {kind:?}")),
                "a {kind:?} was recorded but no rebuild was asked for"
            );
            let recorded = last_error.lock().unwrap().clone().expect("an error");
            assert!(recorded.contains(&format!("{kind:?}")), "{recorded}");
        }
    }

    /// The supervisor loop against a **real cpal stream**, which is the one half of this ticket the
    /// desk can genuinely run: a rebuild that replaces a live stream and leaves audio flowing.
    ///
    /// It stands in for a signal the desk cannot produce, not for the device pass. What the phone
    /// adds is whether `setActive(true)` recovers a session iOS deactivated — which is the whole
    /// question, and is why this is *evidence*, not a substitute.
    ///
    /// `#[ignore]`d because it opens the microphone and takes a couple of seconds. `cargo test`
    /// stays hardware-free and instant, the property `b03` designed the pipeline around; this is
    /// run deliberately with `cargo test -- --ignored`.
    #[test]
    #[ignore = "opens the real microphone; run with `cargo test -- --ignored`"]
    fn a_requested_rebuild_replaces_a_live_stream_and_audio_keeps_flowing() {
        /// Polls `f` at 2 ms for up to two seconds — long enough for a CoreAudio stream to come
        /// up, short enough that a failure is a failure rather than a hang.
        ///
        /// The 2 ms is not idle precision: the recovery time printed at the end lands within a few
        /// ms of spec §6.9's 200 ms staleness threshold, so a 20 ms poll would have been most of
        /// the distance between *the rebuild is invisible* and *the rebuild shows `--`*.
        fn within_two_seconds<F: FnMut() -> bool>(mut f: F) -> bool {
            for _ in 0..1000 {
                if f() {
                    return true;
                }
                std::thread::sleep(Duration::from_millis(2));
            }
            false
        }

        let capture = Capture::start(Weighting::C);
        assert!(
            within_two_seconds(|| capture.builds() >= 1),
            "the first stream never came up"
        );
        assert!(matches!(capture.state(), CaptureState::Running(_)));

        // Audio is really arriving, so "flowing again" afterwards means something.
        let mut before_blocks = 0;
        assert!(
            within_two_seconds(|| {
                before_blocks += capture.drain(|_| {});
                before_blocks > 0
            }),
            "no blocks arrived before the rebuild"
        );

        // Exactly what the interruption observer, the error callback and the health check all do.
        // The trigger is stubbed; everything downstream of it is the real thing.
        let requested_at = Instant::now();
        capture.recovery.request(|| "forced by the test".into());

        assert!(
            within_two_seconds(|| capture.builds() >= 2),
            "the stream was never rebuilt"
        );
        assert!(
            matches!(capture.state(), CaptureState::Running(_)),
            "a rebuilt stream did not publish `Running`"
        );

        // The rebuilt stream feeds a **new** queue, so this is the new one being drained.
        let mut after_blocks = 0;
        assert!(
            within_two_seconds(|| {
                after_blocks += capture.drain(|_| {});
                after_blocks > 0
            }),
            "audio never resumed after the rebuild"
        );

        // Printed rather than asserted, because it is a measurement of *this* Mac and not a
        // property of the design — but it is the number that says what a recovery costs, and
        // whether spec §6.9's 200 ms staleness rule fires during one. Read it when running the
        // test; the phone's own figure belongs to the device pass.
        println!(
            "recovery took {:?} from request to audio flowing again",
            requested_at.elapsed()
        );
    }
}
