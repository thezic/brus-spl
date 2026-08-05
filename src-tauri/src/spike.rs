//! Throwaway capture spike for tickets `02` and `11` — **delete when `11` resolves.**
//!
//! `02` is resolved: this proved that `cpal` plus a hand-configured `AVAudioSession` delivers
//! non-zero PCM on a physical iPhone, at 48 kHz / mono / f32 / 1024 frames. It computes a raw
//! RMS and prints it. No weighting, no calibration, no UI. Do not let it become the meter.
//!
//! It is now kept alive for ticket `11`'s device probes: interruptions, route changes, and
//! `Measurement`-mode effect. Those needed three additions — gap detection, mid-run session
//! polling, and a selectable session mode — because an interruption produces **no error at
//! all** on released cpal 0.18.1, so missing samples are the only evidence available.
//!
//! Everything ticket `11` needs must reach the **on-screen** report: `println!` from Rust does
//! not show up in `xcrun devicectl … --console`, so stdout is useless on a device.
//!
//! The interesting part is not the RMS — it is the diagnostics. Research ticket `01`
//! identified three failure signatures that look similar on device and have different
//! fixes, so the spike is built to tell them apart:
//!
//! | Observation | Cause |
//! |---|---|
//! | `InvalidInput: channel count must be at least 1` | session category never set |
//! | stream builds, buffers are **exactly** zero | microphone permission not granted |
//! | stream builds, callback never fires | RemoteIO not started, or `play()` not called |
//!
//! See `.scratch/spl-meter-mvp/research/01-native-audio-capture-path.md` for the
//! reasoning, and its "Open risks" section for what this spike is meant to settle.

use std::fmt;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Error, ErrorKind, Sample, SampleFormat};

/// How often the reporting thread prints an aggregate line.
const REPORT_INTERVAL: Duration = Duration::from_millis(250);

/// Ticket `02` asks us to record the cpal version, because iOS interruption auto-resume
/// exists only on `master`, not on any release. Keep in step with `Cargo.toml`.
const CPAL_VERSION: &str = "0.18.1 (crates.io release)";

/// What we ask the audio session for. Both are *requests* on iOS — Apple documents
/// `setPreferredSampleRate` and `setPreferredIOBufferDuration` as preferences, not
/// commands, which is exactly why the spike reads the granted values back.
// Only the iOS path asks for a rate; on desktop we take whatever the device reports.
#[cfg(target_os = "ios")]
const PREFERRED_SAMPLE_RATE: f64 = 48_000.0;
const PREFERRED_BUFFER_FRAMES: u32 = 1024;

/// Everything the spike learned. The point of the exercise: later tickets need these
/// numbers, so they are returned rather than only printed.
#[derive(Debug, Clone)]
pub struct SpikeReport {
    pub device_id: String,
    pub sample_format: String,
    /// Ground truth as reported by the stream config we actually built with.
    pub sample_rate: u32,
    pub channels: u16,
    /// What the stream reports once running, which on both CoreAudio backends is
    /// recomputed from the live device rather than echoing back our request.
    pub granted_buffer_frames: Option<u32>,
    /// iOS only: read back from `AVAudioSession` after activation.
    pub session: Option<SessionFacts>,
    /// Whether `AVAudioSessionModeMeasurement` was requested (iOS only). Ticket `11` probe 4
    /// compares the same steady source with this on and off.
    pub measurement_mode: bool,
    pub blocks: u64,
    pub frames: u64,
    pub all_zero_blocks: u64,
    /// Frames we should have received: measured run time × sample rate. The gap between this
    /// and `frames` is the whole point of ticket `11` — an unannounced gap makes a rolling
    /// L_eq silently wrong.
    ///
    /// There is an inherent floor of roughly one buffer (~21 ms at 1024 frames / 48 kHz):
    /// whatever the driver had in flight when we tore the stream down is never delivered.
    /// **Treat anything under ~50 ms as accounting noise, not a gap.** A real interruption
    /// is seconds.
    pub expected_frames: u64,
    /// Stretches where no audio arrived at all.
    pub gaps: Vec<Gap>,
    /// iOS only: mid-run changes to the session's reported rate or channel count, e.g. when a
    /// headset is plugged in. Each entry is `at <seconds>: <what changed>`.
    pub session_changes: Vec<String>,
    /// Peak absolute sample over the whole run, in dBFS.
    pub peak_dbfs: f32,
    /// RMS over the whole run, in dBFS.
    pub rms_dbfs: f32,
    /// Stream errors delivered to the error callback, in order.
    pub errors: Vec<String>,
}

impl SpikeReport {
    /// Milliseconds of audio that never arrived. For a rolling L_eq this is the error budget:
    /// the average silently covers less real time than it claims.
    pub fn missing_ms(&self) -> f64 {
        let missing = self.expected_frames.saturating_sub(self.frames);
        let frames_per_second = self.sample_rate as f64 * self.channels.max(1) as f64;
        if frames_per_second > 0.0 {
            missing as f64 / frames_per_second * 1000.0
        } else {
            0.0
        }
    }

    /// The spike's verdict. This is what ticket `02` records on resolution.
    pub fn verdict(&self) -> Verdict {
        if self.blocks == 0 {
            Verdict::CallbackNeverFired
        } else if self.all_zero_blocks == self.blocks {
            Verdict::AllZeroSamples
        } else {
            Verdict::Captured
        }
    }
}

/// A stretch during which the audio callback delivered nothing.
///
/// Exists because research `01` predicts that on released cpal 0.18.1 an interruption kills the
/// stream with **no error reaching Rust**, and on `master` it silently resumes. Either way the
/// only evidence available to us is missing samples, so the spike has to measure their absence.
#[derive(Debug, Clone, Copy)]
pub struct Gap {
    pub start_s: f32,
    pub end_s: f32,
    /// Whether audio resumed before the run ended. `false` means the stream stayed dead —
    /// the predicted outcome of an interruption on 0.18.1.
    pub recovered: bool,
}

impl fmt::Display for Gap {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{:.2}s → {:.2}s ({:.2}s){}",
            self.start_s,
            self.end_s,
            self.end_s - self.start_s,
            if self.recovered {
                ", resumed"
            } else {
                ", NEVER RESUMED"
            }
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// Non-zero PCM arrived. The architecture holds.
    Captured,
    /// Every buffer was exactly zero — Apple: "captures only silence (zeroed out audio
    /// samples)" when permission is not granted.
    AllZeroSamples,
    /// The stream built but no audio ever arrived.
    CallbackNeverFired,
}

impl fmt::Display for Verdict {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Verdict::Captured => write!(f, "CAPTURED — non-zero PCM received"),
            Verdict::AllZeroSamples => write!(
                f,
                "ALL-ZERO SAMPLES — microphone permission almost certainly not granted"
            ),
            Verdict::CallbackNeverFired => {
                write!(f, "NO CALLBACKS — RemoteIO never started, or play() failed")
            }
        }
    }
}

/// Values read back from `AVAudioSession` *after* activation. Research `01` step 7: these
/// are ground truth, not what we asked for. Never assume 48 kHz.
#[derive(Debug, Clone, Copy)]
pub struct SessionFacts {
    pub sample_rate: f64,
    pub input_channels: isize,
    pub io_buffer_duration: f64,
}

/// Lock-free accumulator. The audio callback writes to it; the reporting thread drains it.
///
/// Deliberately not a `Mutex`: blocking in a real-time audio callback risks priority
/// inversion and dropouts, and a diagnostic whose own instrumentation causes glitches
/// would report a capture bug that isn't there.
#[derive(Default)]
struct Accum {
    /// Running sum of squares, as `f64` bits.
    energy_bits: AtomicU64,
    samples: AtomicU64,
    /// Monotonic max of absolute sample value, as `f32` bits. Only valid for
    /// non-negative values, which is fine since we only ever store `abs()`.
    peak_bits: AtomicU32,
    /// Same, but reset every time the reporting thread reads it.
    interval_peak_bits: AtomicU32,
    blocks: AtomicU64,
    all_zero_blocks: AtomicU64,
}

/// A reading of the accumulator. Counters are cumulative over the run; `interval_peak` is
/// not — it is reset on read.
struct Snapshot {
    energy: f64,
    samples: u64,
    /// Peak over the whole run so far.
    peak: f32,
    /// Peak since the previous read. Printed instead of the run peak because a monotonic
    /// high-water mark stops being informative after the first loud moment, and the whole
    /// point on device is watching the level respond.
    interval_peak: f32,
    blocks: u64,
    all_zero_blocks: u64,
}

impl Accum {
    fn add_energy(&self, delta: f64) {
        let mut cur = self.energy_bits.load(Ordering::Relaxed);
        loop {
            let next = (f64::from_bits(cur) + delta).to_bits();
            match self.energy_bits.compare_exchange_weak(
                cur,
                next,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => return,
                Err(actual) => cur = actual,
            }
        }
    }

    fn max_peak(slot: &AtomicU32, value: f32) {
        let mut cur = slot.load(Ordering::Relaxed);
        loop {
            if f32::from_bits(cur) >= value {
                return;
            }
            match slot.compare_exchange_weak(
                cur,
                value.to_bits(),
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => return,
                Err(actual) => cur = actual,
            }
        }
    }

    /// Records one callback's worth of samples.
    ///
    /// Generic over the sample type and converting inline, so no path allocates. That is
    /// not premature tidiness: this runs in a real-time audio callback, and collecting a
    /// `Vec` per block risks the very dropouts the spike is trying to measure — they would
    /// print as a capture gap and be read as an interruption bug that isn't there.
    fn push_block<T>(&self, samples: &[T])
    where
        T: Copy,
        f32: cpal::FromSample<T>,
    {
        let mut energy = 0.0f64;
        let mut peak = 0.0f32;
        let mut all_zero = true;
        for &raw in samples {
            // iOS is always f32, where this is the identity. Integer formats land in
            // -1.0..=1.0, so the RMS means the same thing on every platform.
            let s: f32 = Sample::from_sample(raw);
            // Exact-zero test on purpose: the permission-denied signature is buffers of
            // *exact* zeros, not merely quiet ones. A near-silent room still has noise in
            // the last mantissa bits, so this distinguishes the two.
            if s != 0.0 {
                all_zero = false;
            }
            energy += (s as f64) * (s as f64);
            let a = s.abs();
            if a > peak {
                peak = a;
            }
        }
        self.add_energy(energy);
        self.samples
            .fetch_add(samples.len() as u64, Ordering::Relaxed);
        Self::max_peak(&self.peak_bits, peak);
        Self::max_peak(&self.interval_peak_bits, peak);
        self.blocks.fetch_add(1, Ordering::Relaxed);
        if all_zero {
            self.all_zero_blocks.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Reads the accumulator, resetting the interval peak.
    ///
    /// The fields are read independently, so a snapshot taken mid-callback can include a
    /// block's energy but not its sample count. Worst case that inflates one printed RMS
    /// line by about a block's worth — a few tenths of a dB at these block rates — and the
    /// run totals, taken after the stream is dropped, are unaffected. Tolerated rather than
    /// fixed: a seqlock here would be more machinery than a throwaway diagnostic warrants.
    fn snapshot(&self) -> Snapshot {
        Snapshot {
            energy: f64::from_bits(self.energy_bits.load(Ordering::Relaxed)),
            samples: self.samples.load(Ordering::Relaxed),
            peak: f32::from_bits(self.peak_bits.load(Ordering::Relaxed)),
            interval_peak: f32::from_bits(self.interval_peak_bits.swap(0, Ordering::Relaxed)),
            blocks: self.blocks.load(Ordering::Relaxed),
            all_zero_blocks: self.all_zero_blocks.load(Ordering::Relaxed),
        }
    }
}

/// dBFS for a linear amplitude in 0..=1. Returns `-inf` for exact silence, which is the
/// honest answer and prints as `-inf` rather than a misleading large negative number.
fn dbfs(amplitude: f64) -> f32 {
    if amplitude <= 0.0 {
        f32::NEG_INFINITY
    } else {
        (20.0 * amplitude.log10()) as f32
    }
}

/// Runs the spike for `duration`, printing progress, and returns what it learned.
///
/// On iOS this configures and activates the `AVAudioSession` first, in the order research
/// `01` derived from cpal's example plus Apple's docs — cpal does neither, and Apple's
/// default `SoloAmbient` category permits no input at all.
pub fn run(duration: Duration, measurement_mode: bool) -> Result<SpikeReport, Error> {
    println!("--- capture spike (tickets 02, 11) ---");
    println!("cpal {CPAL_VERSION}");
    println!("measurement mode: {measurement_mode}");

    let session = configure_audio_session(measurement_mode)?;
    if let Some(facts) = session {
        println!(
            "AVAudioSession after activation: sampleRate={} Hz, inputNumberOfChannels={}, IOBufferDuration={:.6} s",
            facts.sample_rate, facts.input_channels, facts.io_buffer_duration
        );
        // Research 01 open risk 1: assert none of these are zero.
        if facts.sample_rate == 0.0 || facts.input_channels == 0 {
            eprintln!(
                "!! session reports no usable input (rate={}, channels={}) — \
                 expect InvalidInput from cpal next, meaning the category did not take",
                facts.sample_rate, facts.input_channels
            );
        }
    }

    let host = cpal::default_host();
    let device = host.default_input_device().ok_or_else(|| {
        Error::with_message(ErrorKind::DeviceNotAvailable, "no default input device")
    })?;
    let device_id = device
        .id()
        .map(|id| id.to_string())
        .unwrap_or_else(|_| "<unnamed>".to_string());
    println!("input device: {device_id}");

    // Derive the config from what the device reports now, after session activation —
    // never from constants. cpal's iOS backend reads the live session at call time.
    let supported = device.default_input_config()?;
    println!("default input config: {supported:?}");

    let sample_format = supported.sample_format();
    let mut config: cpal::StreamConfig = supported.into();

    // Cross-check cpal against the session. cpal's iOS backend reports the session's
    // current rate as both the min and max of a single-point range, so these should agree
    // — if they don't, one of the two is reading stale state and every downstream FFT bin
    // and weighting coefficient would be computed against the wrong number.
    if let Some(facts) = session {
        if facts.sample_rate as u32 != config.sample_rate {
            eprintln!(
                "!! rate disagreement: AVAudioSession says {} Hz, cpal says {} Hz — \
                 trust neither until this is understood",
                facts.sample_rate, config.sample_rate
            );
        }
    }

    // Ask for a buffer in the range cpal accepts on iOS (256..=4096 frames). Larger is
    // better for us — FFT frames, not low latency. Still only a hint.
    config.buffer_size = cpal::BufferSize::Fixed(PREFERRED_BUFFER_FRAMES);

    let accum = Arc::new(Accum::default());
    let (err_tx, err_rx) = mpsc::channel::<Error>();

    let stream = build_input_stream(&device, &config, sample_format, accum.clone(), err_tx)?;
    stream.play()?;

    let granted_buffer_frames = stream.buffer_size().ok();
    println!(
        "requested {} frames, stream reports {}",
        PREFERRED_BUFFER_FRAMES,
        granted_buffer_frames
            .map(|f| f.to_string())
            .unwrap_or_else(|| "<unavailable>".into())
    );
    println!("capturing for {:.1}s...", duration.as_secs_f32());

    let mut errors = Vec::new();
    let mut gaps: Vec<Gap> = Vec::new();
    let mut gap_start: Option<f32> = None;
    let mut session_changes: Vec<String> = Vec::new();
    let mut last_session = session;
    let mut prev = accum.snapshot();
    let start = Instant::now();
    while start.elapsed() < duration {
        std::thread::sleep(REPORT_INTERVAL.min(duration.saturating_sub(start.elapsed())));

        // The error callback runs on a notification queue, not the audio thread, so
        // draining it here is the supervisor pattern research 01 open risk 4 asks for:
        // never rebuild a stream from inside the callback.
        while let Ok(err) = err_rx.try_recv() {
            let needs_rebuild = matches!(
                err.kind(),
                ErrorKind::StreamInvalidated | ErrorKind::DeviceNotAvailable
            );
            println!(
                "!! stream error: {:?}: {}{}",
                err.kind(),
                err,
                if needs_rebuild {
                    "  (would require a stream rebuild)"
                } else {
                    ""
                }
            );
            errors.push(format!("{:?}: {}", err.kind(), err));
        }

        let snap = accum.snapshot();
        // Per-interval, not cumulative: a running average flattens out and stops showing
        // whether the microphone is responding. Deltas also make a capture gap obvious —
        // 0 blocks in an interval is exactly what an interruption looks like.
        let d_energy = snap.energy - prev.energy;
        let d_samples = snap.samples - prev.samples;
        let d_blocks = snap.blocks - prev.blocks;
        let d_zeros = snap.all_zero_blocks - prev.all_zero_blocks;
        let rms = if d_samples > 0 {
            (d_energy / d_samples as f64).sqrt()
        } else {
            0.0
        };
        let now = start.elapsed().as_secs_f32();

        // Gap tracking. This is ticket 11's core measurement: an interruption gives us no
        // error, so silence in the callback is the only evidence there is.
        if d_blocks == 0 {
            gap_start.get_or_insert(now);
        } else if let Some(began) = gap_start.take() {
            gaps.push(Gap {
                start_s: began,
                end_s: now,
                recovered: true,
            });
            println!("!! gap ended: {began:.2}s → {now:.2}s");
        }

        // Poll the session so a mid-run route change is visible — plugging in a headset can
        // move the sample rate, and a Bluetooth route can drop it below the ~40 kHz the
        // weighting filters need (research 03).
        if let Some(current) = current_session_facts() {
            if let Some(before) = last_session {
                if current.sample_rate != before.sample_rate
                    || current.input_channels != before.input_channels
                {
                    let change = format!(
                        "at {now:.2}s: rate {} → {} Hz, channels {} → {}",
                        before.sample_rate,
                        current.sample_rate,
                        before.input_channels,
                        current.input_channels
                    );
                    println!("!! session changed: {change}");
                    session_changes.push(change);
                }
            }
            last_session = Some(current);
        }

        println!(
            "[{:5.2}s] rms {:>7.1} dBFS  peak {:>7.1} dBFS  blocks {:<5} frames {:<8}{}",
            now,
            dbfs(rms),
            dbfs(snap.interval_peak as f64),
            d_blocks,
            d_samples,
            if d_blocks == 0 {
                "  << NO AUDIO THIS INTERVAL (gap?)"
            } else if d_zeros == d_blocks {
                "  << all buffers exactly zero"
            } else {
                ""
            }
        );
        prev = snap;
    }

    // A gap still open at the end means the stream never came back.
    if let Some(began) = gap_start {
        gaps.push(Gap {
            start_s: began,
            end_s: start.elapsed().as_secs_f32(),
            recovered: false,
        });
    }

    // Measured before teardown, because the expectation must be based on how long the stream
    // actually ran, not the duration we asked for. The report loop overshoots by up to one
    // interval, and using the nominal duration would let that overshoot mask a short gap —
    // which is precisely what ticket 11 needs to detect.
    let ran_for = start.elapsed().as_secs_f64();
    drop(stream);

    // Run totals, so these are cumulative on purpose.
    let total = accum.snapshot();
    let rms = if total.samples > 0 {
        (total.energy / total.samples as f64).sqrt()
    } else {
        0.0
    };

    // Frames per channel × channels, so the comparison is apples to apples.
    let expected_frames =
        (ran_for * config.sample_rate as f64) as u64 * config.channels.max(1) as u64;

    let report = SpikeReport {
        device_id,
        sample_format: sample_format.to_string(),
        sample_rate: config.sample_rate,
        channels: config.channels,
        granted_buffer_frames,
        session,
        measurement_mode,
        blocks: total.blocks,
        frames: total.samples,
        all_zero_blocks: total.all_zero_blocks,
        expected_frames,
        gaps,
        session_changes,
        peak_dbfs: dbfs(total.peak as f64),
        rms_dbfs: dbfs(rms),
        errors,
    };

    println!(
        "frames {} of {} expected ({} missing = {:.0} ms)",
        report.frames,
        report.expected_frames,
        report.expected_frames.saturating_sub(report.frames),
        report.missing_ms()
    );
    for gap in &report.gaps {
        println!("!! GAP {gap}");
    }
    for change in &report.session_changes {
        println!("!! SESSION CHANGE {change}");
    }
    println!("--- verdict: {} ---", report.verdict());
    Ok(report)
}

/// Builds the input stream for whichever sample format the device hands us.
///
/// iOS is always `f32`, but the macOS dev loop can offer others, and the spike has to run
/// on both. Non-float formats are normalised to `f32` so the RMS means the same thing.
fn build_input_stream(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    sample_format: SampleFormat,
    accum: Arc<Accum>,
    err_tx: mpsc::Sender<Error>,
) -> Result<cpal::Stream, Error> {
    let err_fn = move |err: Error| {
        // Ignore a closed channel: it only means the run already finished.
        let _ = err_tx.send(err);
    };

    // One arm per format because cpal's callback is typed; `push_block` is generic, so
    // every arm is the same allocation-free body.
    macro_rules! build {
        ($sample:ty) => {{
            let accum = accum.clone();
            device.build_input_stream(
                *config,
                move |data: &[$sample], _: &cpal::InputCallbackInfo| accum.push_block(data),
                err_fn,
                None,
            )
        }};
    }

    match sample_format {
        // The only format iOS uses, and the one where conversion is the identity.
        SampleFormat::F32 => build!(f32),
        SampleFormat::I8 => build!(i8),
        SampleFormat::I16 => build!(i16),
        SampleFormat::I32 => build!(i32),
        SampleFormat::F64 => build!(f64),
        other => Err(Error::with_message(
            ErrorKind::UnsupportedConfig,
            format!("spike does not handle sample format {other}"),
        )),
    }
}

/// Configures and activates the `AVAudioSession`. iOS only; a no-op elsewhere.
///
/// cpal never sets the category and never activates the session — its own iOS example does
/// this in the host `AppDelegate` instead. Apple's default category is `SoloAmbient`,
/// which permits no input, so skipping this guarantees failure.
#[cfg(target_os = "ios")]
fn configure_audio_session(measurement_mode: bool) -> Result<Option<SessionFacts>, Error> {
    use block2::RcBlock;
    use objc2::runtime::Bool;
    use objc2_avf_audio::{
        AVAudioSession, AVAudioSessionCategoryOptions, AVAudioSessionCategoryRecord,
        AVAudioSessionModeDefault, AVAudioSessionModeMeasurement,
    };
    use std::sync::mpsc;

    // Generic over the error type so this module needs no direct `objc2-foundation`
    // dependency just to name `Retained<NSError>`.
    fn av_err(stage: &str, e: impl fmt::Debug) -> Error {
        Error::with_message(ErrorKind::BackendError, format!("{stage} failed: {e:?}"))
    }

    // SAFETY: every call below is an ObjC message send to `AVAudioSession`, which is
    // documented as thread-safe and is the process-wide singleton. The order is the one
    // research 01 step 5 derived: category+mode, preferred rate, permission, activate,
    // then read back.
    unsafe {
        let session = AVAudioSession::sharedInstance();

        // `Record` (input only), not cpal's example's `PlayAndRecord` — we never play.
        // `Measurement` mode because Apple documents it as minimising system-supplied
        // signal processing and pinning the primary microphone, which is what a
        // calibrated instrument needs. Note Apple says it disables *some* dynamics
        // processing, so this is a prerequisite for calibration, not a substitute.
        //
        // Options are deliberately empty: no `AllowBluetooth`, because a Bluetooth SCO
        // route forces a low sample rate and an unknown microphone.
        let category = AVAudioSessionCategoryRecord.ok_or_else(|| {
            Error::with_message(
                ErrorKind::BackendError,
                "AVAudioSessionCategoryRecord unavailable",
            )
        })?;
        // Ticket 11 probe 4 needs the comparison, so the mode is selectable rather than fixed.
        // `Default` is the control case: whatever processing Apple applies by default.
        let (mode, mode_name) = if measurement_mode {
            (AVAudioSessionModeMeasurement, "Measurement")
        } else {
            (AVAudioSessionModeDefault, "Default")
        };
        let mode = mode.ok_or_else(|| {
            Error::with_message(
                ErrorKind::BackendError,
                format!("AVAudioSessionMode{mode_name} unavailable"),
            )
        })?;
        session
            .setCategory_mode_options_error(category, mode, AVAudioSessionCategoryOptions::empty())
            .map_err(|e| av_err("setCategory:mode:options:", e))?;
        println!("session category=Record mode={mode_name} options=none");

        session
            .setPreferredSampleRate_error(PREFERRED_SAMPLE_RATE)
            .map_err(|e| av_err("setPreferredSampleRate:", e))?;
        session
            .setPreferredIOBufferDuration_error(
                PREFERRED_BUFFER_FRAMES as f64 / PREFERRED_SAMPLE_RATE,
            )
            .map_err(|e| av_err("setPreferredIOBufferDuration:", e))?;

        // Request permission and *wait*. Relying on the implicit prompt means the first
        // measurement records silence while the alert is up.
        //
        // This blocks until the completion block fires, which the OS delivers on the main
        // queue — so it MUST NOT be called from the main thread, or it deadlocks. Both
        // callers are fine: `--bin spike` blocks its own main thread but has no run loop
        // to starve, and Tauri runs non-async commands on a worker thread. Anyone making
        // `run_capture_spike` async, or calling this from a UI callback, will hang.
        //
        // `requestRecordPermission` is deprecated in favour of `AVAudioApplication`
        // (iOS 17+), but still functional and works on older devices too. For a
        // throwaway spike that trade is worth taking; the real app should revisit.
        let (tx, rx) = mpsc::channel::<bool>();
        let block = RcBlock::new(move |granted: Bool| {
            let _ = tx.send(granted.as_bool());
        });
        #[allow(deprecated)]
        session.requestRecordPermission(&block);
        let granted = rx.recv_timeout(Duration::from_secs(60)).map_err(|_| {
            Error::with_message(ErrorKind::PermissionDenied, "permission prompt timed out")
        })?;
        if !granted {
            return Err(Error::with_message(
                ErrorKind::PermissionDenied,
                "microphone permission denied by the user",
            ));
        }
        println!("microphone permission granted");

        session
            .setActive_error(true)
            .map_err(|e| av_err("setActive:", e))?;

        Ok(Some(SessionFacts {
            sample_rate: session.sampleRate(),
            input_channels: session.inputNumberOfChannels(),
            io_buffer_duration: session.IOBufferDuration(),
        }))
    }
}

/// Non-iOS platforms have no `AVAudioSession`. On macOS the dev loop relies on TCC
/// prompting for the process; see research `01` §4 and open risk 7.
#[cfg(not(target_os = "ios"))]
fn configure_audio_session(_measurement_mode: bool) -> Result<Option<SessionFacts>, Error> {
    Ok(None)
}

/// Re-reads the live session, for polling mid-run. iOS only; `None` elsewhere.
///
/// Separate from `configure_audio_session` because a route change can move the sample rate
/// under a running stream, and the only way to notice is to keep asking.
#[cfg(target_os = "ios")]
fn current_session_facts() -> Option<SessionFacts> {
    use objc2_avf_audio::AVAudioSession;

    // SAFETY: read-only property access on the process-wide singleton, which Apple documents
    // as thread-safe.
    unsafe {
        let session = AVAudioSession::sharedInstance();
        Some(SessionFacts {
            sample_rate: session.sampleRate(),
            input_channels: session.inputNumberOfChannels(),
            io_buffer_duration: session.IOBufferDuration(),
        })
    }
}

#[cfg(not(target_os = "ios"))]
fn current_session_facts() -> Option<SessionFacts> {
    None
}
