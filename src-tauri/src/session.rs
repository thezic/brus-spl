//! The `AVAudioSession` that cpal deliberately does not configure. iOS only; a documented
//! no-op everywhere else.
//!
//! cpal never sets the session category and never activates the session — it treats that as
//! the application's job, and its own iOS example does it in the host `AppDelegate`. Apple's
//! default category is `SoloAmbient`, which permits no input at all, so skipping this
//! guarantees failure. cpal also never calls `setPreferredSampleRate`, so the session runs at
//! whatever rate the system already had unless we ask.
//!
//! Two failure signatures that look nothing alike and are easily confused (spec §3.1):
//!
//! | Symptom | Cause |
//! |---|---|
//! | `InvalidInput: channel count must be at least 1` | the category was never set — or the session is deactivated after an interruption |
//! | buffers of exact zeros | microphone permission not granted |
//!
//! Everything asked for here is a *preference*. Apple documents both the sample rate and the
//! I/O buffer duration as requests, not commands, so every value is **read back after
//! activation and used as ground truth** (spec §3.1). The measured device baseline — iPhone 14
//! Pro, iOS 26.5.2 — granted all of them exactly, which is a fact about one device and one
//! route rather than a guarantee.

use serde::Serialize;

/// What we ask the session for. 48 kHz because every weighting coefficient and FFT bin is
/// derived from the *granted* rate, and 48 kHz is what the measured device runs at anyway.
pub const PREFERRED_SAMPLE_RATE: f64 = 48_000.0;

/// Block size we ask for, in frames. Larger is better for a meter — this is an averaging
/// instrument, not a low-latency effect — but cpal's iOS backend only accepts 256..=4096.
pub const PREFERRED_BUFFER_FRAMES: u32 = 1024;

/// `setPreferredIOBufferDuration` takes seconds, not frames.
pub const PREFERRED_IO_BUFFER_DURATION: f64 =
    PREFERRED_BUFFER_FRAMES as f64 / PREFERRED_SAMPLE_RATE;

/// Values read back from `AVAudioSession` **after** activation — ground truth, not what we
/// asked for.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionFacts {
    pub sample_rate: f64,
    pub input_channels: isize,
    /// The granted mode, as its raw `AVAudioSessionMode` string. Not ceremony: see
    /// [`SessionFacts::measurement_mode`].
    pub mode: String,
    /// True when the granted mode is `AVAudioSessionModeMeasurement`.
    ///
    /// **Worth 21 dB.** The same 440 Hz sine measured −51.7 dBFS in `Measurement` and
    /// −30.6 dBFS in `Default` on device (spec §3.2), and it is a fixed gain rather than AGC,
    /// which is the whole reason a single calibration offset is defensible. If the mode ever
    /// silently fails to apply, every calibrated reading is wrong by ~21 dB with nothing on
    /// screen to show it — so the read-back is checked and a mismatch logged.
    pub measurement_mode: bool,
    pub io_buffer_duration: f64,
    /// Whether the microphone permission prompt came back granted. Capture starts either
    /// way: a denied microphone delivers buffers of exact zeros, which spec §6.3 discards at
    /// drain time, so the meter reads `--` beside `0s of 60s` rather than a plausible lie.
    pub permission_granted: bool,
}

/// Configures and activates the session, then reads everything back.
///
/// **Must be called before cpal is touched**, and **must not be called from the main thread**:
/// the microphone permission completion block is delivered on the main queue, and this waits
/// for it. The capture thread is the intended caller.
#[cfg(target_os = "ios")]
pub fn configure() -> Result<Option<SessionFacts>, String> {
    use std::sync::mpsc;
    use std::time::Duration;

    use block2::RcBlock;
    use objc2::runtime::Bool;
    use objc2_avf_audio::{
        AVAudioSession, AVAudioSessionCategoryOptions, AVAudioSessionCategoryRecord,
        AVAudioSessionModeMeasurement,
    };

    fn av_err(stage: &str, e: impl std::fmt::Debug) -> String {
        format!("{stage} failed: {e:?}")
    }

    // SAFETY: every call below is an ObjC message send to `AVAudioSession`, the process-wide
    // singleton Apple documents as thread-safe. The order is spec §3.1's, which is the order
    // research `01` derived from cpal's own example plus Apple's docs.
    unsafe {
        let session = AVAudioSession::sharedInstance();

        // `Record`, not cpal's example's `PlayAndRecord` — we never play. Options are
        // deliberately empty: no `AllowBluetooth`, because a Bluetooth SCO route forces a low
        // sample rate and an unknown microphone.
        let category = AVAudioSessionCategoryRecord
            .ok_or_else(|| "AVAudioSessionCategoryRecord unavailable".to_string())?;
        let mode = AVAudioSessionModeMeasurement
            .ok_or_else(|| "AVAudioSessionModeMeasurement unavailable".to_string())?;
        session
            .setCategory_mode_options_error(category, mode, AVAudioSessionCategoryOptions::empty())
            .map_err(|e| av_err("setCategory:mode:options:", e))?;

        session
            .setPreferredSampleRate_error(PREFERRED_SAMPLE_RATE)
            .map_err(|e| av_err("setPreferredSampleRate:", e))?;
        session
            .setPreferredIOBufferDuration_error(PREFERRED_IO_BUFFER_DURATION)
            .map_err(|e| av_err("setPreferredIOBufferDuration:", e))?;

        // Ask explicitly and wait, rather than letting activation raise the prompt implicitly:
        // the implicit path measures silence for as long as the alert is up, and the stream
        // built underneath it keeps delivering exact zeros afterwards.
        //
        // This blocks until the completion block fires, which the OS delivers on the main
        // queue — so it MUST NOT run on the main thread. `requestRecordPermission` is
        // deprecated in favour of `AVAudioApplication` (iOS 17+) but still functional and
        // works on older devices; spec §9.4 reads `recordPermission` for the *display* state,
        // which is a separate question from asking.
        let (tx, rx) = mpsc::channel::<bool>();
        let block = RcBlock::new(move |granted: Bool| {
            let _ = tx.send(granted.as_bool());
        });
        #[allow(deprecated)]
        session.requestRecordPermission(&block);
        let permission_granted = rx.recv_timeout(Duration::from_secs(60)).unwrap_or(false);

        session
            .setActive_error(true)
            .map_err(|e| av_err("setActive:", e))?;

        let granted_mode = session.mode();
        let facts = SessionFacts {
            sample_rate: session.sampleRate(),
            input_channels: session.inputNumberOfChannels(),
            measurement_mode: &*granted_mode == mode,
            mode: granted_mode.to_string(),
            io_buffer_duration: session.IOBufferDuration(),
            permission_granted,
        };
        log_mismatches(&facts);
        Ok(Some(facts))
    }
}

/// No `AVAudioSession` off iOS. On the macOS dev loop cpal takes the default input device as
/// it finds it, and TCC attributes the microphone request to the responsible parent process,
/// which makes the desk the permissive case and the phone the strict one (spec §3.4).
#[cfg(not(target_os = "ios"))]
pub fn configure() -> Result<Option<SessionFacts>, String> {
    Ok(None)
}

/// Re-reads the live session without changing it. iOS only; `None` elsewhere.
///
/// Separate from [`configure`] because a route change moves these values under a running
/// stream, and asking again is the only way to notice.
#[cfg(target_os = "ios")]
pub fn current_facts() -> Option<SessionFacts> {
    use objc2_avf_audio::{AVAudioSession, AVAudioSessionModeMeasurement};

    // SAFETY: read-only property access on the process-wide singleton.
    unsafe {
        let session = AVAudioSession::sharedInstance();
        let granted_mode = session.mode();
        Some(SessionFacts {
            sample_rate: session.sampleRate(),
            input_channels: session.inputNumberOfChannels(),
            measurement_mode: AVAudioSessionModeMeasurement
                .is_some_and(|expected| &*granted_mode == expected),
            mode: granted_mode.to_string(),
            io_buffer_duration: session.IOBufferDuration(),
            permission_granted: true,
        })
    }
}

#[cfg(not(target_os = "ios"))]
pub fn current_facts() -> Option<SessionFacts> {
    None
}

/// Logs every place the session granted something other than what was asked for.
///
/// Nothing here reaches the UI (spec §3.1). On a device that means the Xcode console —
/// `println!` does not reach `devicectl … --console` (spec §2.2) — which is acceptable
/// precisely because none of these are conditions the user can act on.
#[cfg(target_os = "ios")]
fn log_mismatches(facts: &SessionFacts) {
    if facts.sample_rate != PREFERRED_SAMPLE_RATE {
        eprintln!(
            "session: asked for {PREFERRED_SAMPLE_RATE} Hz, granted {} Hz",
            facts.sample_rate
        );
    }
    if !facts.measurement_mode {
        eprintln!(
            "session: asked for AVAudioSessionModeMeasurement, granted {:?} — \
             expect ~21 dB of processing gain and a calibration offset that means nothing",
            facts.mode
        );
    }
    // A frame or two of rounding is not a mismatch; the session quantises the duration to
    // whole frames at the granted rate.
    if (facts.io_buffer_duration - PREFERRED_IO_BUFFER_DURATION).abs() > 1e-4 {
        eprintln!(
            "session: asked for {PREFERRED_IO_BUFFER_DURATION:.6} s of I/O buffer, granted {:.6} s",
            facts.io_buffer_duration
        );
    }
    if facts.input_channels < 1 {
        eprintln!(
            "session: reports {} input channels — expect cpal's \
             `InvalidInput: channel count must be at least 1` next, meaning the category did not take",
            facts.input_channels
        );
    }
    if !facts.permission_granted {
        eprintln!("session: microphone permission not granted — buffers will be exact zeros");
    }
}
