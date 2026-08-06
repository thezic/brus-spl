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
    /// Whether the microphone permission is granted **now**, read from `recordPermission`
    /// after the prompt has been answered rather than taken from the prompt's own reply — one
    /// authority for the question, not two. Capture starts either way: a denied microphone
    /// delivers buffers of exact zeros, which spec §6.3 discards at drain time, so the meter
    /// reads `--` beside `0s of 60s` rather than a plausible lie.
    pub permission_granted: bool,
}

/// Why the session is being configured: the first time, or spec §4.2's recovery.
///
/// The only difference is the microphone permission prompt. [`Activation::Initial`] asks for it
/// and **blocks on the main queue** until the answer arrives; [`Activation::Recovery`] does not,
/// because by then the question is long since settled and re-asking would put a main-queue
/// round-trip on the capture thread twice a second for as long as a Siri call lasts.
///
/// Everything else — the category, the mode, the two preferences, `setActive(true)` and the
/// read-back — is deliberately identical, so there is one setup path and one place a value can
/// come back different from what was asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Activation {
    Initial,
    Recovery,
}

/// Configures and activates the session, then reads everything back.
///
/// **Must be called before cpal is touched**, and — with [`Activation::Initial`] — **must not be
/// called from the main thread**: the microphone permission completion block is delivered on the
/// main queue, and this waits for it. The capture thread is the intended caller.
///
/// `setActive(true)` runs on **both** paths, which is spec §4.2 item 3: a cpal stream rebuilt
/// against a session left deactivated by an interruption fails with `InvalidInput: channel count
/// must be at least 1` — the same error as a never-set category, and the whole of this ticket's
/// first trap.
#[cfg(target_os = "ios")]
pub fn configure(activation: Activation) -> Result<Option<SessionFacts>, String> {
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
        //
        // Re-set on the recovery path too. An interruption is not documented to clear the
        // category, but the preferences are the only lever we have over a *new* route, and
        // re-asking costs three message sends against a rebuild that is already happening.
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

        if activation == Activation::Initial {
            request_permission();
        }

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
            permission_granted: permission() == Some(MicPermission::Granted),
        };
        log_mismatches(&facts);
        Ok(Some(facts))
    }
}

/// Raises the microphone prompt and waits for the answer. Blocks; never call it on the main
/// thread.
///
/// Asked explicitly rather than left to activation to raise implicitly: the implicit path
/// measures silence for as long as the alert is up, and the stream built underneath it keeps
/// delivering exact zeros afterwards.
///
/// `requestRecordPermission` is deprecated in favour of `AVAudioApplication` (iOS 17+) but still
/// functional and works on older devices; spec §9.4 reads `recordPermission` for the *display*
/// state, which is a separate question from asking.
#[cfg(target_os = "ios")]
fn request_permission() {
    use std::sync::mpsc;
    use std::time::Duration;

    use block2::RcBlock;
    use objc2::runtime::Bool;
    use objc2_avf_audio::AVAudioSession;

    let (tx, rx) = mpsc::channel::<bool>();
    let block = RcBlock::new(move |granted: Bool| {
        let _ = tx.send(granted.as_bool());
    });
    // SAFETY: message send to the process-wide singleton; the block is copied by the runtime.
    #[allow(deprecated)]
    unsafe {
        AVAudioSession::sharedInstance().requestRecordPermission(&block);
    }
    // The answer itself is read back from `recordPermission` in `configure`; this only waits
    // for the alert to be dismissed.
    let _ = rx.recv_timeout(Duration::from_secs(60));
}

/// No `AVAudioSession` off iOS. On the macOS dev loop cpal takes the default input device as
/// it finds it, and TCC attributes the microphone request to the responsible parent process,
/// which makes the desk the permissive case and the phone the strict one (spec §3.4).
#[cfg(not(target_os = "ios"))]
pub fn configure(_activation: Activation) -> Result<Option<SessionFacts>, String> {
    Ok(None)
}

/// The authoritative answer to spec §9.4's question, as opposed to the exact-zeros heuristic.
///
/// Three states rather than a bool because the app has to distinguish *the user said no* — which
/// is fixable, but only from outside the app — from *there is no answer yet*, which is the
/// permission prompt still being on screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MicPermission {
    Granted,
    Denied,
    /// Asked, not yet answered: the prompt is up.
    Undetermined,
}

/// Reads `AVAudioSession.recordPermission`. `None` off iOS, where the question has no answer.
///
/// Deliberately **not** the exact-zeros heuristic (spec §9.4): an authoritative answer exists, so
/// inferring one from the samples would be guessing where the OS will simply say. Read live on
/// every tick rather than cached from [`configure`], because [`SessionFacts::permission_granted`]
/// records what the prompt returned once, and this records what is true now.
///
/// `AVAudioSession`'s property rather than iOS 17's `AVAudioApplication`, matching [`configure`]'s
/// choice of the deprecated `requestRecordPermission` for the same reason: it still works and it
/// works on older devices.
#[cfg(target_os = "ios")]
pub fn permission() -> Option<MicPermission> {
    use objc2_avf_audio::{AVAudioSession, AVAudioSessionRecordPermission};

    // SAFETY: read-only property access on the process-wide singleton Apple documents as
    // thread-safe. Called from the tick thread, never the audio callback.
    #[allow(deprecated)]
    unsafe {
        let granted = AVAudioSession::sharedInstance().recordPermission();
        Some(if granted == AVAudioSessionRecordPermission::Granted {
            MicPermission::Granted
        } else if granted == AVAudioSessionRecordPermission::Denied {
            MicPermission::Denied
        } else {
            MicPermission::Undetermined
        })
    }
}

/// No session, so no answer — which is spec §9.4's desktop caveat in one line: macOS reports
/// `capturing` wherever the stream is running, because TCC attributes the request to the
/// responsible parent process and there is nothing here to ask.
#[cfg(not(target_os = "ios"))]
pub fn permission() -> Option<MicPermission> {
    None
}

/// The live `inputNumberOfChannels` — spec §4.2 item 4's **pollable health check**. `None` off
/// iOS, where there is no session to interrogate.
///
/// One property rather than the whole read-back because this runs on the 10 Hz tick, and because
/// it is the only value `11` probe 2 measured moving: an interruption took it **1 → 0 and left it
/// there**, with no stream error and no other symptom. It is checked *independently of* the
/// interruption notification precisely because nothing guarantees the notification arrives —
/// probe 2 saw the stream die permanently and not one thing reached Rust.
#[cfg(target_os = "ios")]
pub fn input_channels() -> Option<isize> {
    use objc2_avf_audio::AVAudioSession;

    // SAFETY: read-only property access on the process-wide singleton, from the tick thread.
    unsafe { Some(AVAudioSession::sharedInstance().inputNumberOfChannels()) }
}

/// No session, so nothing to poll — which makes the health check a documented no-op on the desk
/// and [`crate::capture::needs_recovery`] the only part of it that can be tested there.
#[cfg(not(target_os = "ios"))]
pub fn input_channels() -> Option<isize> {
    None
}

/// The two halves of `AVAudioSessionInterruptionNotification` (spec §4.2 item 2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Interruption {
    /// The system has taken the session: a call, Siri, an alarm. Nothing to do — reactivating
    /// while another process holds the session fails, and the coverage figure is already
    /// reporting the hole.
    Began,
    /// The interruption is over and the session may be reactivated.
    Ended,
}

/// A live `NSNotificationCenter` observation. Dropping it does not unregister — see
/// [`observe_interruptions`] — so it exists to make the lifetime visible rather than to manage
/// it.
#[cfg(target_os = "ios")]
pub struct InterruptionObserver {
    _token:
        objc2::rc::Retained<objc2::runtime::ProtocolObject<dyn objc2::runtime::NSObjectProtocol>>,
}

/// Observes `AVAudioSessionInterruptionNotification`, calling `f` with each half.
///
/// **Forced rather than chosen** (spec §4.2 item 2): cpal never surfaces interruptions, and on the
/// CoreAudio backends it never surfaces route changes either — `DeviceChanged` is raised only by
/// the WASAPI and PipeWire hosts. Without this observer the only signal an interruption ever
/// produces in this process is [`input_channels`] going to zero.
///
/// `queue: None` means the block runs on whichever thread posted the notification, which is what
/// we want: it does nothing but set a flag and wake the supervisor.
///
/// **The observation is app-lifetime and is deliberately never removed.** The block-based API
/// hands back an opaque token that the notification centre itself retains until
/// `removeObserver:`; the supervisor loop that owns ours never returns, so a `Drop` that
/// unregisters would be code that cannot run.
#[cfg(target_os = "ios")]
pub fn observe_interruptions<F>(f: F) -> Option<InterruptionObserver>
where
    F: Fn(Interruption) + Send + Sync + 'static,
{
    use std::ptr::NonNull;

    use block2::RcBlock;
    use objc2_avf_audio::{AVAudioSessionInterruptionType, AVAudioSessionInterruptionTypeKey};
    use objc2_foundation::{NSNotification, NSNotificationCenter};

    // SAFETY: two weak-linked framework string constants, read exactly as `configure` reads the
    // category and mode ones. Both are `Option`, so an SDK that does not export them is a `None`
    // here rather than a crash — the health check then carries this ticket on its own.
    let (name, type_key) = unsafe {
        (
            objc2_avf_audio::AVAudioSessionInterruptionNotification?,
            AVAudioSessionInterruptionTypeKey?,
        )
    };

    let block = RcBlock::new(move |notification: NonNull<NSNotification>| {
        // SAFETY: the notification is alive for the duration of the block, by contract.
        let notification = unsafe { notification.as_ref() };
        let Some(interruption) = interruption_type(notification, type_key) else {
            // A notification whose userInfo says nothing we understand. Say so once rather than
            // guessing at `Ended` — the health check covers the case guessing would have.
            eprintln!("session: interruption notification with no readable type");
            return;
        };
        f(if interruption == AVAudioSessionInterruptionType::Began {
            Interruption::Began
        } else {
            Interruption::Ended
        });
    });

    /// Digs `AVAudioSessionInterruptionTypeKey` out of the userInfo dictionary.
    fn interruption_type(
        notification: &NSNotification,
        type_key: &objc2_foundation::NSString,
    ) -> Option<AVAudioSessionInterruptionType> {
        use objc2_foundation::NSNumber;

        let user_info = notification.userInfo()?;
        // The documented value for this key is an `NSNumber`. The downcast is what makes that
        // checked rather than assumed — a wrong type here would otherwise be a wild read.
        let value = user_info.objectForKey(type_key.as_ref())?;
        let number = value.downcast_ref::<NSNumber>()?;
        Some(AVAudioSessionInterruptionType(
            number.unsignedIntegerValue(),
        ))
    }

    // SAFETY: registering a block-based observer on the default centre. The block is copied by
    // the runtime, and is `Send + Sync` because the posting thread is not ours to choose.
    let token = unsafe {
        NSNotificationCenter::defaultCenter().addObserverForName_object_queue_usingBlock(
            Some(name),
            None,
            None,
            &block,
        )
    };
    Some(InterruptionObserver { _token: token })
}

/// No `AVAudioSession` off iOS, so no interruptions to observe. The supervisor loop is left with
/// the stream-error path alone, which on the macOS dev loop is the honest state of affairs.
#[cfg(not(target_os = "ios"))]
pub struct InterruptionObserver;

#[cfg(not(target_os = "ios"))]
pub fn observe_interruptions<F>(_f: F) -> Option<InterruptionObserver>
where
    F: Fn(Interruption) + Send + Sync + 'static,
{
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
