# Native audio capture path for an iOS-targeted Tauri 2 SPL meter

Research ticket 01. Investigated 2026-08-04 against primary sources only (cpal source tree at
`RustAudio/cpal`, Apple developer documentation, `tauri-apps/tauri` source and `tauri-apps/tauri-docs`,
crate docs on docs.rs). Every claim below carries a source URL. Where a claim is my inference from
code rather than a documented statement, it is labelled **[inference]**.

Project state at time of writing: unmodified `create-tauri-app` scaffold, no `src-tauri/gen/` directory
at all (so `tauri ios init` has never run), dependencies are `tauri = "2"`, `tauri-plugin-opener`,
`serde`, `serde_json`.

---

## Recommendation

**Use `cpal` (>= 0.18.1, and prefer pinning to a git revision of `master` or the next release once the
interruption fix lands) as the capture backend, and configure `AVAudioSession` yourself from Rust
using the `objc2-avf-audio` crate — the same crate cpal already depends on for iOS.**

Do **not** write a Swift/ObjC Tauri mobile plugin for capture. Do not call `AVAudioEngine` directly.

Confidence: **high** on the architecture (~85%), **medium** on it working first try on device (~60%),
because the session-configuration handshake between our code and cpal has to be sequenced correctly
and that sequencing is not documented anywhere — it is only visible by reading cpal's iOS source.

The reasoning in one paragraph: cpal's iOS backend is not the stub it was historically. As of 0.17.0
(2025-12-20) through 0.18.1 (2026-06-07) it has been substantially reworked and now wraps RemoteIO
Audio Units and queries `AVAudioSession` for its device model, observes `AVAudioSessionRouteChange`,
`MediaServicesWereLost` and `MediaServicesWereReset` notifications, and surfaces them to Rust through
the stream error callback. It hands us `&Data` buffers of `f32` frames in a real-time callback, which
is exactly the raw PCM interface an SPL meter needs. The one thing it deliberately does **not** do is
set the `AVAudioSession` category and activate the session — and cpal's own official iOS example
proves this by doing it in the host app's `AppDelegate` before calling into Rust. That gap is roughly
20 lines of `objc2-avf-audio` in Rust, which is dramatically cheaper than any alternative. There is
no official Tauri audio plugin to lean on instead.

### What would change my mind

- If a device spike shows the RemoteIO input unit returns all-zero buffers even with the category set
  correctly and permission granted, and that cannot be resolved by reordering session setup, then the
  fallback is a Swift Tauri mobile plugin wrapping `AVAudioEngine.inputNode` with a tap, streaming
  PCM to Rust over FFI. Cost estimate in §2.
- If we discover we need capture to survive backgrounding (an SPL meter left logging in a pocket),
  the `UIBackgroundModes` / `audio` requirement and cpal's total lack of any background-session
  awareness may push toward the Swift plugin, which can own the whole session lifecycle.
- If pinning to cpal `master` (needed for interruption auto-resume) proves unstable, we handle
  interruptions ourselves in the error callback and stay on the released 0.18.1.

The honest counterweight to this recommendation, stated up front: **the two community Tauri plugins that
do real PCM capture on iOS both deliberately bypass cpal on iOS** and use `AVAudioEngine` instead
(§5e). That is two independent authors reaching the opposite conclusion. I discount it because both
predate the 0.17→0.18 iOS rework and at least one needs simultaneous playback, but it is the main reason
my "works first try" confidence is ~60% rather than ~85%. It also means the fallback is de-risked: if
cpal fails on device, `stippi/tauri-plugin-audio` is a working, readable reference implementation of
exactly the `AVAudioEngine` + `installTap` + FFI + ring-buffer design we would need (§5c).

---

## 1. Does cpal support iOS well enough to build on?

### 1a. There is a real iOS backend, and what it wraps

Yes. `iOS` is listed in cpal's own platform support table with `CoreAudio` as the default backend,
no optional alternatives.
Source: <https://github.com/RustAudio/cpal/blob/master/README.md>

The backend lives at `src/host/coreaudio/ios/` and contains three files:

| File | Purpose |
| --- | --- |
| `mod.rs` (726 lines) | Host/Device/Stream, RemoteIO setup, callbacks |
| `session_event_manager.rs` | `AVAudioSession` notification observers |
| `enumerate.rs` (23 lines) | Device enumeration — a single hardcoded `Device` |

Sources:
<https://github.com/RustAudio/cpal/tree/master/src/host/coreaudio/ios>,
<https://github.com/RustAudio/cpal/blob/master/src/host/coreaudio/ios/mod.rs>

The module's own doc comment states what it wraps:

> `//! CoreAudio implementation for iOS using AVAudioSession and RemoteIO Audio Units.`

Concretely it is a three-layer stack:

- **`coreaudio-rs` 0.14.2** (features `core_audio`, `audio_toolbox`) for the `AudioUnit` wrapper —
  `AudioUnit::new_uninitialized(coreaudio::audio_unit::IOType::RemoteIO)`.
- **`objc2-audio-toolbox` 0.3** for the raw property constants
  (`kAudioOutputUnitProperty_EnableIO`, `kAudioUnitProperty_StreamFormat`).
- **`objc2-avf-audio` 0.3** for `AVAudioSession` — used read-mostly, to query `sampleRate()`,
  `inputNumberOfChannels()`, `outputNumberOfChannels()`, `IOBufferDuration()`, `inputLatency()`,
  `outputLatency()`.

Source (target-specific dependency table, lines 136–182):
<https://github.com/RustAudio/cpal/blob/master/Cargo.toml>

So: **coreaudio-rs / AudioUnit (RemoteIO) for the data path, AVAudioSession for
introspection.** It is not built on `AVAudioEngine`.

`aarch64-apple-ios` is one of the eight targets cpal builds its docs.rs documentation for, so the iOS
backend is at least kept compiling by CI.
Source: <https://github.com/RustAudio/cpal/blob/master/Cargo.toml> (`[package.metadata.docs.rs]` targets, line ~252)

**Maturity caveat — this is recent work.** The iOS backend was reworked across three releases, and one
important piece is still unreleased:

| Release | iOS-relevant changelog entry |
| --- | --- |
| 0.17.0 (2025-12-20) | "**iOS**: Complete AVAudioSession integration for device enumeration and buffer size control." |
| 0.17.0 (2025-12-20) | "**iOS**: Example by properly activating audio session." |
| 0.18.0 (2026-06-06) | "**CoreAudio**: Stream error callback now receives `ErrorKind::StreamInvalidated` … on iOS on route changes that require a stream rebuild." |
| 0.18.0 | "**CoreAudio**: Stream error callback now receives `ErrorKind::DeviceChanged` … on iOS when headphones are unplugged." |
| 0.18.0 | "**CoreAudio (iOS)**: `default_output_config()` now prefers stereo over the maximum channel count." |
| **Unreleased** | "**iOS**: Streams now resume automatically when an audio session interruption ends." |
| **Unreleased** | "**iOS**: Timestamps now include hardware latency and update when the audio route changes." |

Source: <https://github.com/RustAudio/cpal/blob/master/CHANGELOG.md>

I verified the release boundary directly rather than trusting the changelog: I fetched
`session_event_manager.rs` at tag `v0.18.1` and grepped it. **At v0.18.1 the file is 143 lines and
contains no interruption handling at all** — it observes only `AVAudioSessionRouteChangeNotification`,
`AVAudioSessionMediaServicesWereLostNotification` and
`AVAudioSessionMediaServicesWereResetNotification`. On `master` the same file is 221 lines and adds an
`AVAudioSessionInterruptionNotification` observer.
Sources:
<https://github.com/RustAudio/cpal/blob/v0.18.1/src/host/coreaudio/ios/session_event_manager.rs>,
<https://github.com/RustAudio/cpal/blob/master/src/host/coreaudio/ios/session_event_manager.rs>

This directly drives the version decision in §1c.

### 1b. Sample rates and buffer sizes actually obtainable, and who dictates them

**Sample rate: the system dictates it, and cpal will only ever offer you the rate the session is
already running at.**

`get_supported_stream_configs()` reads the current session rate and reports it as both the min and the
max of a single-point range:

```rust
let sample_rate = audio_session.sampleRate() as SampleRate;
// ...
let configs: Vec<_> = (min_channels..=max_channels)
    .map(|channels| SupportedStreamConfigRange {
        channels,
        min_sample_rate: sample_rate,
        max_sample_rate: sample_rate,
        buffer_size,
        sample_format: SUPPORTED_SAMPLE_FORMAT,
    })
    .collect();
```

Source: <https://github.com/RustAudio/cpal/blob/master/src/host/coreaudio/ios/mod.rs> (lines 473–506)

**cpal never calls `setPreferredSampleRate`.** I grepped the whole iOS module; the only setter it calls
on the session is `setPreferredIOBufferDuration_error`. Consequence for us: **if we want a known,
stable capture rate for FFT bin math, we must call `setPreferredSampleRate` ourselves before building
the stream.** Apple is explicit that this is a request, not a command:

> "This method requests a change to the input and output audio sample rate. To see the effect of this
> change, use the [`sampleRate`] property. … The available range is device dependent and is typically
> from 8000 through 48000 hertz."

Source: <https://developer.apple.com/documentation/avfaudio/avaudiosession/setpreferredsamplerate(_:)>

So the correct sequence is: set preferred rate → activate session → **read back
`session.sampleRate()`** → use that number as ground truth for the DSP. Never assume 48 000.

**Channel count for input is not negotiable.** cpal deliberately refuses to offer flexibility on the
input side:

```rust
// For input, only return the exact channel count (no flexibility)
// For output, support flexible channel counts up to the hardware maximum
let min_channels = if is_input { max_channels } else { 1 };
```

Source: <https://github.com/RustAudio/cpal/blob/master/src/host/coreaudio/ios/mod.rs> (lines 491–493)

**Sample format is fixed at `f32`.** `const SUPPORTED_SAMPLE_FORMAT: SampleFormat = SampleFormat::F32;`
with the comment "These days the default of iOS is now F32 and no longer I16". This is convenient for
us — no integer conversion step before weighting/FFT.
Source: same file, line 43.

**Buffer size: requestable within 256–4096 frames, but only as a hint.** cpal hardcodes:

```rust
// Typical iOS hardware buffer frame limits according to Apple Technical Q&A QA1631.
const BUFFER_SIZE_MIN: FrameCount = 256;
const BUFFER_SIZE_MAX: FrameCount = 4096;
```

`BufferSize::Fixed(n)` outside that range is rejected up front with `ErrorKind::UnsupportedConfig`;
inside it, cpal converts frames to seconds and calls `setPreferredIOBufferDuration_error`, with the
honest comment "iOS may not honor the exact request due to system constraints."
Source: <https://github.com/RustAudio/cpal/blob/master/src/host/coreaudio/ios/mod.rs> (lines 403–430, 468–470, 514–525)

Apple's numbers corroborate cpal's constants:

> "The typical maximum I/O buffer duration is 0.093 seconds (corresponding to 4,096 sample frames at a
> sample rate of 44.1 kHz). The minimum I/O buffer duration is at least 0.005 seconds (256 frames) but
> might be lower depending on the hardware in use. You can set a preferred I/O buffer duration before
> or after activating the audio session."

Source: <https://developer.apple.com/documentation/avfaudio/avaudiosession/setpreferrediobufferduration(_:)>

Note the mismatch: Apple says the minimum "might be lower" than 256 on some hardware; cpal's floor of
256 is therefore slightly conservative but never wrong. For an SPL meter this is irrelevant — we want
*larger* buffers (1024–4096) for FFT frames, not smaller ones.

To read the size actually granted, cpal exposes `Stream::buffer_size()`, which recomputes
`IOBufferDuration() * sampleRate()` from the live session rather than echoing back what we asked for —
so it is trustworthy.
Source: same file, lines 372–374 and 436–446.

### 1c. Route changes and interruptions

This is the area where cpal is better than I expected, but with a sharp version cliff.

**Route changes (headphones, Bluetooth): observed, and reported to Rust.** `SessionEventManager`
registers a block on `AVAudioSessionRouteChangeNotification` and maps the reason to a cpal error:

| `AVAudioSessionRouteChangeReason` | cpal `ErrorKind` delivered to the error callback |
| --- | --- |
| `OldDeviceUnavailable` (e.g. headphones unplugged) | `DeviceChanged` — "Audio route changed" |
| `CategoryChange`, `Override`, `RouteConfigurationChange` | `StreamInvalidated` — "Audio route changed" |
| `NoSuitableRouteForCategory` | `DeviceNotAvailable` |
| anything else (incl. `NewDeviceAvailable`) | *no error emitted* |

Source: <https://github.com/RustAudio/cpal/blob/master/src/host/coreaudio/ios/session_event_manager.rs>
(`route_change_error`, lines 48–71)

On every route change it also recomputes the buffer depth so capture timestamps track the new latency
(`input_latency_frames()` → `frames.store(depth, Ordering::Relaxed)`). That is the "Unreleased"
timestamp fix.

Practical reading for us: **plugging in a headset does not silently kill the stream, but it does hand
us a `DeviceChanged`/`StreamInvalidated` error and cpal does not rebuild the stream for us.** Our SPL
meter must treat those two error kinds as "tear down and rebuild the stream", and — importantly for a
calibrated instrument — as "the input device may have physically changed, so any calibration offset is
now suspect." A wired headset mic and the built-in mic have wildly different sensitivity. That is an
application-level concern cpal cannot help with.

**Interruptions (incoming call, Siri): handled on `master`, NOT handled in 0.18.1.**

On `master`, the interruption observer does this:

```rust
if AVAudioSessionInterruptionType(kind) == AVAudioSessionInterruptionType::Began {
    with_stream(&stream, StreamInner::stop_for_interruption);
    return;
}
// ... on .ended:
if !options.contains(AVAudioSessionInterruptionOptions::ShouldResume) {
    return;
}
let session = unsafe { AVAudioSession::sharedInstance() };
if unsafe { session.setActive_error(true) }.is_ok() {
    with_stream(&stream, StreamInner::resume_after_interruption);
}
```

Source: <https://github.com/RustAudio/cpal/blob/master/src/host/coreaudio/ios/session_event_manager.rs> (lines 100–124)

with a third `PlaybackState::Interrupted` state so `stop`/`resume` are idempotent and don't fight
`play()`/`pause()`:

```rust
fn stop_for_interruption(&mut self) {
    if self.state != PlaybackState::Playing { return; }
    // Unlike pause(), the OS is the one that actually halted the unit here; this
    // call only resyncs AudioUnit's own bookkeeping, so its result carries nothing.
    let _ = self.audio_unit.stop();
    self.state = PlaybackState::Interrupted;
}
```

Source: same file's `mod.rs`, lines 141–173.

**Answering the ticket's question directly: does the stream die silently on interruption?**

- On **cpal `master`**: no. It is stopped and then automatically restarted, and the session is
  re-activated. However — and this matters — **no error is emitted to Rust for an interruption.** The
  interruption block never calls `emit_error`. So from our Rust code's point of view, an incoming call
  produces *a gap in the sample stream with no notification whatsoever*. For an SPL meter computing
  time-weighted averages (LAeq over a measurement period), a silent multi-second gap is a correctness
  bug: we would average over a period that contains no data. **We must detect this ourselves**, and
  the only reliable signal available is wall-clock discontinuity in the callback, or registering our
  own additional `AVAudioSessionInterruptionNotification` observer alongside cpal's.
- On **released 0.18.1**: the stream dies and stays dead. The OS halts the RemoteIO unit; nothing
  restarts it; no interruption error is surfaced (there is no interruption observer). You *may*
  incidentally get a `route_change_error` if the interruption also changed the route, but that is not
  guaranteed. This is a genuine blocker for a continuous-capture app.

`MediaServicesWereLost` → `DeviceNotAvailable` and `MediaServicesWereReset` → `StreamInvalidated` are
emitted as errors in both versions. `MediaServicesWereReset` in particular obliges us to discard and
rebuild everything, which is standard iOS practice.

### 1d. What AVAudioSession configuration cpal does NOT do for you

**This is the single most important finding in this document.**

cpal **never sets the session category or mode, and never activates the session** on the initial setup
path. I read all 726 lines of `ios/mod.rs`: the only mutating call on `AVAudioSession` is
`setPreferredIOBufferDuration_error` (line ~420), reached only when `BufferSize::Fixed` is requested.
There is no `setCategory*`, no `setMode`, and no `setActive` anywhere in the build path. The one
`setActive_error(true)` call in the whole backend is in the interruption-*resume* handler on `master`
(`session_event_manager.rs` line 121), which only runs after an interruption has already ended — it
never runs during normal startup.

This is not an oversight I am inferring — **cpal's own official iOS example does the session setup in
the host app, in Objective-C, before calling into Rust:**

```objc
AVAudioSession *session = AVAudioSession.sharedInstance;
NSError *categoryError = nil;
BOOL isSetCategorySuccess = [session setCategory:AVAudioSessionCategoryPlayAndRecord
                                    withOptions:AVAudioSessionCategoryOptionDefaultToSpeaker | AVAudioSessionCategoryOptionAllowBluetooth
                                          error:&categoryError];
if (isSetCategorySuccess) {
    NSError *activateError = nil;
    BOOL isActivateSuccess = [session setActive:YES error:&activateError];
    if (isActivateSuccess) {
        NSLog(@"Calling rust_ios_main()");
        rust_ios_main();
    }
    // ...
}
```

Source: <https://github.com/RustAudio/cpal/blob/master/examples/ios-feedback/ios-src/AppDelegate.m>

Note the comment in that file: *"It is necessary to access the sharedInstance so that calls to
AudioSessionGetProperty will work."* — and the 0.17.0 changelog line "**iOS**: Example by properly
activating audio session", i.e. this was a bug in cpal's example that had to be fixed by adding session
activation. cpal's position is unambiguous: **session configuration is the application's job.**

**Why this is fatal if skipped.** The default category does not permit input at all:

> "`AVAudioSessionCategorySoloAmbient` (Default)" … and of the seven categories only
> `AVAudioSessionCategoryRecord` ("Input only") and `AVAudioSessionCategoryPlayAndRecord` ("Input and
> output") allow audio input.

Source: <https://developer.apple.com/library/archive/documentation/Audio/Conceptual/AudioSessionProgrammingGuide/AudioSessionCategoriesandModes/AudioSessionCategoriesandModes.html>
(Table B-1)

**[inference]** Tracing the consequence through cpal's code: under `SoloAmbient` there is no input
route, so `audio_session.inputNumberOfChannels()` returns 0. In `get_supported_stream_configs(true)`,
`max_channels` is then 0 and `min_channels = max_channels = 0`, so the range `0..=0` yields exactly one
`SupportedStreamConfigRange { channels: 0, .. }`. `default_input_config()` propagates that, and
`build_input_stream_raw` calls `crate::validate_stream_config(&config)` first, which rejects it:

```rust
if config.channels == 0 {
    return Err(Error::with_message(
        ErrorKind::InvalidInput,
        "channel count must be at least 1",
    ));
}
```

Source: <https://github.com/RustAudio/cpal/blob/master/src/lib.rs> (lines 875–895)

So the *predicted* symptom of forgetting the category is a confusing `InvalidInput: channel count must
be at least 1`, not silence. I flag this as an inference because Apple does not document
`inputNumberOfChannels`' value when no input route exists — its documentation says only "The number of
audio input channels for the current route."
Source: <https://developer.apple.com/documentation/avfaudio/avaudiosession/inputnumberofchannels>

**The separate, genuinely silent failure is the permission one**, and Apple documents it explicitly:

> "**Unless a user grants your app permission to record audio, it captures only silence (zeroed out
> audio samples).**"

Source: <https://developer.apple.com/documentation/avfaudio/avaudioapplication/requestrecordpermission(completionhandler:)>

So there are two distinct failure modes to keep separate when debugging on device:
`InvalidInput`/wrong-channel-count → category not set; buffers full of exact zeros → permission not
granted. See §3 and Open Risks.

**cpal does not request microphone permission.** I grepped the whole CoreAudio backend for
`permission`/`requestRecord`/`authoriz`: the only hits are in `mod.rs` error mapping, translating
`AudioUnitError::Unauthorized` into `ErrorKind::PermissionDenied`.
Source: <https://github.com/RustAudio/cpal/blob/master/src/host/coreaudio/mod.rs> (lines 107–109)

Apple says the system will prompt on first capture attempt anyway —

> "The first time your app attempts to record audio input, the system automatically prompts the user
> for permission. You can also explicitly ask for permission by calling this method."

— but relying on the implicit prompt means our first measurement session records silence while the
alert is up. We should request explicitly and gate stream creation on the result.

### 1e. Known open issues that would block continuous capture

I searched cpal's issue tracker via the GitHub search API for open issues mentioning iOS,
AVAudioSession, iPad. The result is **one** open issue:

- **#783 "Is it ready for production on Android and iOS?"** (opened 2023-05-27, 1 comment, still open).
  Source: <https://github.com/RustAudio/cpal/issues/783>

That is a question, not a defect report, and it predates the entire 0.17/0.18 iOS rework by two and a
half years, so it carries little information about the current state.

**I want to be honest about the limits of this search rather than present it as a clean bill of
health.** A near-empty iOS issue list is ambiguous: it is equally consistent with "the backend is
solid" and with "almost nobody ships cpal on iOS, so nobody files bugs." Given that (a) device
enumeration is a 23-line file returning a single hardcoded `Device` with a `TODO: Support enumerating
earpiece vs headset vs speaker etc?`, and (b) interruption handling landed only in the last few months
and is still unreleased, I lean toward the second reading: **the iOS backend is actively maintained but
lightly field-tested.** Treat the device spike as mandatory, not as a formality.

The concrete blockers I found are not tracker issues but properties of the code:

1. **Released 0.18.1 does not recover from interruptions** (§1c). Blocking for continuous capture.
   Mitigation: depend on `master`, or handle it ourselves.
2. **Interruptions are never surfaced as errors to Rust**, even on `master` (§1c). Blocking for
   *correct* LAeq/time-averaging. Mitigation: our own notification observer.
3. **No session category/activation** (§1d). Blocking, but cheap to fix ourselves.
4. **Nothing about background audio.** cpal has no concept of it. If we ever need capture while
   backgrounded, this is unaddressed territory.

---

## 2. If cpal were not viable — the alternative cost

Because the recommendation is to keep cpal, this section is scoped to establishing the fallback price
rather than designing it.

### 2a. Calling Apple audio APIs directly from Rust

This is the cheaper fallback and it is already partly proven, because **cpal itself does it**. The
binding crates cpal uses on iOS are all published and current:

- `coreaudio-rs` 0.14.2 — `AudioUnit`, `IOType::RemoteIO`
- `objc2-avf-audio` 0.3 (latest 0.3.2) — `AVAudioSession`, and also `AVAudioEngine`/`AVAudioApplication`
- `objc2-audio-toolbox` 0.3, `objc2-core-audio` 0.3, `objc2-core-audio-types` 0.3
- `block2` 0.6 — needed for completion-handler / notification blocks

Source: <https://github.com/RustAudio/cpal/blob/master/Cargo.toml> (lines 136–182)

Critically, `objc2-avf-audio` already exposes the exact session API surface we need, with safe-ish
`Result`-returning wrappers:

```rust
pub unsafe fn setCategory_mode_options_error(&self, category: &AVAudioSessionCategory, mode: &AVAudioSessionMode, options: AVAudioSessionCategoryOptions) -> Result<(), Retained<NSError>>
pub unsafe fn setActive_error(&self, active: bool) -> Result<(), Retained<NSError>>
pub unsafe fn setPreferredSampleRate_error(&self, sample_rate: c_double) -> Result<(), Retained<NSError>>
pub unsafe fn setPreferredIOBufferDuration_error(&self, duration: NSTimeInterval) -> Result<(), Retained<NSError>>
pub unsafe fn sampleRate(&self) -> c_double
pub unsafe fn inputNumberOfChannels(&self) -> NSInteger
pub unsafe fn recordPermission(&self) -> AVAudioSessionRecordPermission
pub unsafe fn requestRecordPermission(&self, response: &DynBlock<dyn Fn(Bool)>)
```

Source: <https://docs.rs/objc2-avf-audio/latest/objc2_avf_audio/struct.AVAudioSession.html>
(crate version 0.3.2; `setCategory*`/`setActive*` gated behind the `AVAudioSessionTypes` feature,
`requestRecordPermission` behind `block2`)

**This is why the recommendation is a hybrid rather than a choice.** We are not "avoiding" the
direct-bindings path; we are taking the ~20 lines of it that cpal leaves to the application, and
letting cpal own the ~700 lines of RemoteIO plumbing and notification observation. Marginal cost over
plain cpal: one extra dependency (which is already in our dependency graph transitively via cpal, at a
matching 0.3 minor, so no duplicate-`objc2` risk) and a small `#[cfg(target_os = "ios")]` module.

Cost of going *all* the way — replacing cpal with our own `AVAudioEngine` or RemoteIO wrapper: we
would be reimplementing render callbacks, `AudioStreamBasicDescription` construction, mach-time →
timestamp conversion, and all four notification observers. cpal's iOS backend plus its shared
`coreaudio/mod.rs` helpers is roughly 1000 lines of exactly that, much of it `unsafe` with carefully
reasoned safety comments. Estimate: **several days of work and a long tail of `unsafe` correctness
bugs, for no capability we don't already get.** Not justified unless cpal is empirically broken.

### 2b. A Swift shim via a Tauri 2 mobile plugin

This *is* a sanctioned Tauri mechanism, so it is a legitimate fallback — it just isn't the cheap one.

Tauri 2 documents native mobile plugin development. For iOS:

> "A Tauri plugin for iOS is defined as a Swift class that extends the `Plugin` class from the `Tauri`
> package."

Functions marked `@objc` and taking an `Invoke` parameter become commands callable from both Rust and
JavaScript. Rust↔Swift for non-command calls uses plain C FFI, with Swift functions annotated
`@_silgen_name` and matched by `extern "C"` on the Rust side. Plugins hook the `load` lifecycle event
and read their config via `try parseConfig(Config.self)`.

Source: <https://v2.tauri.app/develop/plugins/develop-mobile/>

So yes — **writing Swift is the officially sanctioned path for reaching platform APIs Tauri doesn't
wrap.** The catch for *our* use case is that a Tauri plugin command is a request/response IPC boundary,
which is the wrong shape for a 48 kHz continuous sample stream. We would need the Swift side to own an
`AVAudioEngine` input tap and push buffers across the FFI boundary into a Rust ring buffer via
`@_silgen_name` callbacks — i.e. the plugin scaffolding buys us almost nothing over calling
`AVAudioEngine` from `objc2-avf-audio` directly, while adding a Swift package, a second build system,
and a language boundary to debug across.

The one thing this path buys that the Rust path does not: **the Swift side can hold the whole session
lifecycle idiomatically**, including `UIApplication` lifecycle notifications and background modes. If
background capture becomes a requirement, revisit this.

Rough cost: **1–2 days** for a working PCM-streaming Swift plugin, plus ongoing cost of a Swift
codebase in a project that otherwise has none, plus it only solves iOS — macOS dev-loop capture would
still need a second implementation (cpal or CoreAudio). That last point is decisive: the cpal path
gives us one Rust capture module for both platforms.

---

## 3. Microphone permission on iOS in a Tauri 2 app

### 3a. The key, and why it is non-negotiable

`NSMicrophoneUsageDescription`:

> "A message that tells people why the app is requesting access to the device's microphone. This key is
> **required** if your app uses APIs that access the device's microphone."
> (Platforms: iOS, iPadOS, macOS, tvOS, visionOS, watchOS)

Source: <https://developer.apple.com/documentation/bundleresources/information-property-list/nsmicrophoneusagedescription>

Missing it is not a warning, it is a process kill. Two independent Apple statements:

> "If an application attempts to access any of the device's microphones without a corresponding purpose
> string, **the app exits**."

Source: <https://developer.apple.com/documentation/avfaudio/avaudioapplication/requestrecordpermission(completionhandler:)>

> "Your app needs to contain the appropriate key in its `Info.plist` file, and the appropriate
> entitlement enabled in macOS, before it requests authorization or attempts to use a capture device.
> Otherwise, **the system terminates your app**."

Source: <https://developer.apple.com/documentation/avfoundation/requesting-authorization-to-capture-and-save-media>

(The second page is written for `AVCaptureDevice` rather than `AVAudioSession`; the TCC gate and the
plist key are the same, but I note the API mismatch for accuracy. The first source is the one that
applies directly to our `AVAudioSession`/RemoteIO path.)

### 3b. Where to declare it so regeneration does not eat it — the supported place

**Put it in `src-tauri/Info.plist`.** This one file covers both iOS and macOS.

Tauri's config schema documents the merge for both platforms, in identical words:

> `bundle.iOS.infoPlist` — "Path to a Info.plist file to merge with the default Info.plist. Note that
> Tauri also looks for a `Info.plist` **and `Info.ios.plist`** file in the same directory as the Tauri
> configuration file."

> `bundle.macOS.infoPlist` — "Path to a Info.plist file to merge with the default Info.plist. Note that
> Tauri also looks for a `Info.plist` file in the same directory as the Tauri configuration file."

Source: <https://github.com/tauri-apps/tauri/blob/dev/crates/tauri-schema-generator/schemas/config.schema.json>
(definitions `IosConfig` and `MacConfig`; also mirrored in `crates/tauri-cli/config.schema.json`)

Note the asymmetry: **iOS picks up both `Info.plist` and `Info.ios.plist`; macOS picks up only
`Info.plist`.** So `src-tauri/Info.plist` is the single place that serves both, and
`src-tauri/Info.ios.plist` is the escape hatch for iOS-only keys we don't want on macOS.

**Precedent from Tauri itself:** `examples/api/src-tauri/Info.plist` in the Tauri repo is exactly this
file, containing exactly this key:

```xml
<dict>
	<key>NSCameraUsageDescription</key>
	<string>Request camera access for WebRTC</string>
	<key>NSMicrophoneUsageDescription</key>
	<string>Request microphone access for WebRTC</string>
</dict>
```

Source: <https://github.com/tauri-apps/tauri/blob/dev/examples/api/src-tauri/Info.plist>

**The merge re-runs on every build, not at `init`** — this is the property that makes the key survive
regeneration. Verified against the CLI source (HEAD of `dev` at time of writing):

| Command | Plist merge runs? | Source |
| --- | --- | --- |
| `tauri ios build` (incl. `--open`) | yes | [`mobile/ios/build.rs` L235-267](https://github.com/tauri-apps/tauri/blob/dev/crates/tauri-cli/src/mobile/ios/build.rs#L235-L267) |
| `tauri ios dev` (incl. `--open`) | yes | [`mobile/ios/dev.rs` L222-247](https://github.com/tauri-apps/tauri/blob/dev/crates/tauri-cli/src/mobile/ios/dev.rs#L222-L247) |
| `tauri ios run` | yes | `mobile/ios/run.rs` L79 calls `build::run(...)` |
| **`tauri ios init`** | **no** | `mobile/ios/project.rs` contains zero `plist` references; the plist comes from XcodeGen |

The merge helper is [`crates/tauri-cli/src/helpers/plist.rs`](https://github.com/tauri-apps/tauri/blob/dev/crates/tauri-cli/src/helpers/plist.rs) (L25-42). It is a **flat, non-recursive,
last-source-wins** `insert` per top-level key — a whole top-level key is replaced, not deep-merged.

**Corrected precedence order.** The four-source order reported in tauri-apps/tauri#13068 is **accurate
only for tauri-cli ≤ 2.8.x** (the reporter was on 2.4.0). It was changed by
[`ed7c9a410` / #14108](https://github.com/tauri-apps/tauri/pull/14108) ("feat(core): add config for
Info.plist extensions", released in **tauri-cli 2.9.0**), which reordered the vector and added existence
checks — implementing the very fix the issue asked for. `CFBundleVersion` was separately removed from the
in-memory dict by `0aa48fb9e` / #13030 (tauri-cli 2.5.0).

Current order on tauri-cli ≥ 2.9.0, **lowest → highest precedence**:

1. the existing on-disk `src-tauri/gen/apple/<AppName>_iOS/Info.plist` (i.e. XcodeGen's output)
2. in-memory dict containing **only** `CFBundleShortVersionString` — *`ios build` only; `ios dev` omits this source entirely*
3. **`src-tauri/Info.plist`** (only if it exists)
4. **`src-tauri/Info.ios.plist`** (only if it exists)
5. the file pointed at by `bundle.iOS.infoPlist` (added 2.9.0)
6. generated `bundle.fileAssociations` plist (added 2.11.0)

Source: `build.rs` L235-262 quoted verbatim in the verification pass; issue for the historical order:
<https://github.com/tauri-apps/tauri/issues/13068>

This correction is **good news for us**: our `src-tauri/Info.plist` now *beats* the CLI's generated
values rather than losing to them, which is the opposite of what #13068 describes. Since
`NSMicrophoneUsageDescription` is not a key XcodeGen's template emits
(`crates/tauri-cli/templates/mobile/ios/project.yml` sets only `LSRequiresIPhoneOS`,
`UILaunchStoryboardName`, `CFBundleShortVersionString`, `CFBundleVersion`, and an
`apple.plist-pairs` loop that tauri-cli never populates), there is nothing to collide with either way.

Two caveats to hold onto:

- The merged result is written **back over** `gen/apple/<AppName>_iOS/Info.plist`, which is also merge
  source 1. So that file accumulates keys and is dirtied by every `dev`/`build`/`run`.
- The whole mechanism — the source list, the precedence, the per-build re-application, the write-back —
  is **undocumented on the Tauri docs site.** The only acknowledgement anywhere is the one sentence in
  the `bundle.iOS.infoPlist` config reference, which says nothing about ordering. Issue #13068 is still
  open with no maintainer reply. We are relying on source-verified behaviour, not a documented contract.
  Re-verify if we ever bump tauri-cli across a minor version.

**Which order applies to us:** this project pins `"@tauri-apps/cli": "^2"`, which currently resolves to
**tauri-cli 2.11.4** (verified with `npx tauri --version` in this worktree). That is past both the 2.9.0
reorder and the 2.11.0 file-associations addition, so the corrected six-source order above is the one in
effect. Because `^2` floats, this is worth re-checking rather than assuming — see Open Risk 10.

**Choosing between `Info.plist` and `Info.ios.plist` for our case:** use **`src-tauri/Info.plist`**. The
iOS-only file would keep the key out of the macOS bundle, but we *want* it in the macOS bundle too — the
dev loop needs microphone access on macOS (§4). One file, both platforms, matching Tauri's own example.
Reserve `src-tauri/Info.ios.plist` for keys that must not leak to macOS.

### 3c. Is `gen/apple/` version-controlled or regenerated?

**The source code says: committed. The documentation says nothing at all.** Sources conflict; details
below.

**Evidence for "committed":** `tauri ios init` renders Tauri's own template set into `gen/apple`, and
that set contains a `.gitignore`:

```
xcuserdata/
build/
Externals/
```

Source: <https://github.com/tauri-apps/tauri/blob/dev/crates/tauri-cli/templates/mobile/ios/.gitignore>

That is the canonical "commit the Xcode project, ignore per-user state and build output" pattern — a
`.gitignore` scoping *what inside* `gen/apple` to ignore only makes sense if `gen/apple` itself is
tracked. The Android template `.gitignore` has the same shape.

Corroborating from our own scaffold: `src-tauri/.gitignore` as generated by `create-tauri-app` is

```
/target/
/gen/schemas
```

(verified in this repo at `src-tauri/.gitignore`; template source:
<https://github.com/tauri-apps/tauri/blob/dev/crates/tauri-cli/templates/app/src-tauri/.gitignore>).
It ignores `gen/schemas` **specifically** and leaves `gen/apple` / `gen/android` tracked. So if we run
`tauri ios init` today, `gen/apple` becomes tracked by default.

**Evidence pointing the other way:** the Tauri repo's own root `.gitignore` excludes generated projects
from its examples —

```
# examples /gen directory
/examples/**/src-tauri/gen/
/bench/**/src-tauri/gen/
```

Source: <https://github.com/tauri-apps/tauri/blob/dev/.gitignore> (L30-32)

and a code search finds no committed `project.pbxproj` anywhere in the repo outside CLI test fixtures.
I read this as monorepo hygiene rather than a recommendation — but it does mean **there is no worked
example of a committed `gen/apple` in the Tauri repo**, and it directly contradicts the `.gitignore`
their own CLI writes into user projects. Report the conflict; do not pretend it resolves cleanly.

**Tauri's docs treat `gen/apple/` as a place you edit and keep**, which is the strongest practical
signal, and worth flagging because it contradicts this project's current `CLAUDE.md` note that
"`src-tauri/gen/` is generated — don't hand-edit":

- `plugin/nfc.mdx`: "In the `src-tauri/gen/apple/<project-name>.xcodeproj/project.pbxproj` file, set all
  `IPHONEOS_DEPLOYMENT_TARGET` properties to `14.0`" — hand-editing the pbxproj.
- `plugin/file-system.mdx`: "You must create a `PrivacyInfo.xcprivacy` file in the `src-tauri/gen/apple`
  folder".
- `develop/icons.mdx`: iOS icons "need to be placed directly in the Xcode project into
  `src-tauri/gen/apple/Assets.xcassets/AppIcon.appiconset/`".

Sources: <https://v2.tauri.app/plugin/nfc/>, <https://v2.tauri.app/plugin/file-system/>,
<https://v2.tauri.app/develop/icons/>

No page in tauri-docs `v2` addresses whether to commit `gen/`, and there is no maintainer statement on
it in either repo's tracked files.

**Recommendation for this project.** Commit `gen/apple/` once it exists (it is the default, and
`PrivacyInfo.xcprivacy` and iOS app icons have no other home), but **keep it free of anything we cannot
regenerate**. Everything we need for audio — `NSMicrophoneUsageDescription`, frameworks, development
team, deployment target — is expressible in `src-tauri/Info.plist` and `tauri.conf.json`, and those are
re-applied on every build (§3b). Then `rm -rf src-tauri/gen/apple && tauri ios init` stays a safe
recovery move, which it will not be if we ever hand-edit the plist there.

Expect **build-time git churn**: every `ios dev`/`ios build`/`ios run` rewrites two tracked files —
`gen/apple/<AppName>_iOS/Info.plist` (the merge write-back) and
`gen/apple/<AppName>.xcodeproj/project.pbxproj` (via `synchronize_project_config` → `pbxproj.save()`).
Annoying but harmless.

Note also that `ensure_init` (`crates/tauri-cli/src/mobile/mod.rs`) is called by both dev and build and
**never regenerates** — if the project is stale it refuses with "Xcode project directory is outdated
because {reason}. Please delete {path}, run `tauri ios init` and try again." So regeneration is always
an explicit, manual act; nothing will silently wipe `gen/apple` under us.

**Could not determine:** whether `xcodegen generate` overwrites an already-existing
`gen/apple/<AppName>_iOS/Info.plist` when `tauri ios init` is re-run *over* a populated `gen/apple`.
Tauri's own template renderer explicitly skips existing files (`project.rs` L170:
`if !path.exists() { … } else { Ok(None) }`), but XcodeGen's file-writing behaviour is in
`yonaskolb/XcodeGen`, outside the primary sources for this ticket. Irrelevant if we follow the
recommendation above, since we never keep unique state there.

Two config keys are worth knowing because they explicitly require regeneration:

> `bundle.iOS.frameworks` — "A list of strings indicating any iOS frameworks that need to be bundled
> with the application. **Note that you need to recreate the iOS project for the changes to be
> applied.**"

Source: <https://github.com/tauri-apps/tauri/blob/dev/crates/tauri-schema-generator/schemas/config.schema.json> (`IosConfig`)

So the workflow when iOS-level config changes is: edit `tauri.conf.json` / `Info.plist`, delete
`src-tauri/gen/apple`, re-run `tauri ios init`. This is only safe if nothing hand-written lives there —
another reason to keep it that way.

Also relevant to actually getting onto a physical device with a free account:

> `bundle.iOS.developmentTeam` — "The development team. This value is **required** for iOS development
> because code signing is enforced. The `APPLE_DEVELOPMENT_TEAM` environment variable can be set to
> overwrite it."

Source: same schema. A free Apple ID does get a Personal Team with a Team ID, and Tauri's iOS signing
docs say "Automatic signing is enabled by default, and uses the account configured in Xcode to
authenticate when used on your local machine", with manual `IOS_CERTIFICATE`/`IOS_MOBILE_PROVISION`
env vars as the alternative. Tauri's macOS signing page also acknowledges free accounts exist ("when
using a free Apple Developer account, you will not be able to notarize your application"). Nothing in
Tauri's docs rules out free provisioning for `ios dev` on a local device, but nothing confirms it
either.
Sources: <https://github.com/tauri-apps/tauri-docs/blob/v2/src/content/docs/distribute/Sign/ios.mdx>,
<https://github.com/tauri-apps/tauri-docs/blob/v2/src/content/docs/distribute/Sign/macos.mdx>

---

## 4. macOS permission for the dev loop

### 4a. The two things needed

**Info.plist key: `NSMicrophoneUsageDescription`.** Documented for macOS as well as iOS (see §3a
platform list). Goes in `src-tauri/Info.plist`, same file as for iOS, merged by the bundler:

> "Tauri automatically generates core properties for your app bundle's `Info.plist` file. To extend it
> with custom values, create an `Info.plist` file in the `src-tauri` folder containing your desired
> key-value pairs. This `Info.plist` file is merged with the values generated by the Tauri CLI."

Source: <https://v2.tauri.app/distribute/macos-application-bundle/>

**Entitlement: `com.apple.security.device.audio-input`.**

> "A Boolean value that indicates whether the app may record audio using the built-in microphone and
> **access audio input using Core Audio**. To add this entitlement to your app, first enable the
> Hardened Runtime capability in Xcode, and then under Resource Access, select Audio Input."
> (macOS 10.7+)

Source: <https://developer.apple.com/documentation/bundleresources/entitlements/com.apple.security.device.audio-input>

Note the wording: it covers Core Audio access specifically, which is exactly cpal's macOS path. And
note the trigger condition — it is tied to **Hardened Runtime**, not only to the App Sandbox. This
matters a great deal for Tauri, because:

> `bundle.macOS.hardenedRuntime` — "Whether the codesign should enable hardened runtime (for
> executables) or not."

and in the Rust source:

```rust
#[serde(alias = "hardened-runtime", default = "default_true")]
pub hardened_runtime: bool,
```
```rust
hardened_runtime: true,
```

Source: <https://github.com/tauri-apps/tauri/blob/dev/crates/tauri-utils/src/config.rs> (lines 654–656, 682)

**Tauri enables Hardened Runtime by default.** Therefore any *signed* Tauri macOS bundle needs the
audio-input entitlement, sandbox or no sandbox. This is almost certainly the explanation for
tauri-apps/tauri#9928, "[bug] Access the microphone from Rust on macOS", where a developer using cpal
reported microphone access working under `tauri dev` and in unsigned builds but failing in signed
builds, with the permission prompt never appearing, despite having added `NSMicrophoneUsageDescription`.
The issue was closed with no maintainer response and no documented resolution.
Source: <https://github.com/tauri-apps/tauri/issues/9928>

**[inference]** — the connection between that issue and the hardened-runtime entitlement is my
diagnosis from combining Apple's entitlement doc with Tauri's default, not something stated in the
issue.

### 4b. Does `tauri dev` behave differently from a bundled build?

Yes, and in our favour for once. The evidence:

- `tauri dev` does not produce an `.app` bundle; it runs the binary from `target/debug/`. Tauri's own
  config schema documents dev-mode divergence explicitly for a neighbouring key —
  `bundle.macOS.minimumSystemVersion`: "**Ignored in `tauri dev`**."
  Source: <https://github.com/tauri-apps/tauri/blob/dev/crates/tauri-schema-generator/schemas/config.schema.json> (`MacConfig`)
- The reporter in #9928 states microphone access via cpal **worked** under `tauri dev` and in unsigned
  builds. Source: <https://github.com/tauri-apps/tauri/issues/9928>

**[inference]** The mechanism: an unsigned / ad-hoc-signed binary has no Hardened Runtime, so the
entitlement check does not apply, and TCC grants on the basis of the executable. This produces the
counter-intuitive but widely-reported situation that the **dev loop is the permissive case and the
shipped bundle is the strict one** — the opposite of the usual "prompts only work when bundled"
folklore the ticket anticipated.

I want to flag a genuine conflict in the sources here. Apple states flatly that an app attempting mic
access without the purpose string is terminated (§3a). A bare `target/debug/` executable has no
`Info.plist` and therefore no purpose string, yet #9928 reports dev mode working. Possible
reconciliations: TCC attributes the request to the responsible parent process (the terminal); or the
termination rule is enforced only for bundled apps; or Apple's statement is about `AVCaptureDevice`
specifically. **I could not settle this from documentation.** It is on the device-spike list.

### 4c. Note on scope

macOS is only the dev loop for this project, so the entitlement question is deferrable — but not
ignorable, because the moment we produce a signed macOS build to hand to anyone, capture will break
silently unless the entitlement is present. Cheapest correct move: add the entitlements file now, while
the context is loaded, rather than rediscover #9928 later.

---

## 5. Does Tauri 2 have an official audio/microphone plugin?

### 5a. Official: no, definitively

The official plugin set is `tauri-apps/plugins-workspace` (branch `v2`), 30 plugins: `autostart`,
`barcode-scanner`, `biometric`, `cli`, `clipboard-manager`, `deep-link`, `dialog`, `fs`, `geolocation`,
`global-shortcut`, `haptics`, `http`, `localhost`, `log`, `nfc`, `notification`, `opener`, `os`,
`persisted-scope`, `positioner`, `process`, `shell`, `single-instance`, `sql`, `store`, `stronghold`,
`updater`, `upload`, `websocket`, `window-state`.

Sources: <https://github.com/tauri-apps/plugins-workspace/blob/v2/README.md>,
`https://api.github.com/repos/tauri-apps/plugins-workspace/contents/plugins?ref=v2`,
cross-checked against <https://v2.tauri.app/plugin/>

**Nothing for audio, microphone, recording, or media capture.** The nearest mobile-hardware plugins are
`barcode-scanner` (camera) and `haptics`.

### 5b. Community (`awesome-tauri`): no capture plugin either

Source: <https://github.com/tauri-apps/awesome-tauri/blob/main/README.md>

Grepping for audio/sound/mic/record/PCM/spectrum/decibel/dB/SPL: every hit falls in the
*Applications → Audio & Video* section (Cap, Screenpipe, Whispering, Musicat, Vibe — end-user apps).
The *Plugins* section (~48 entries) contains **zero** audio or microphone plugins.

### 5c. Candidates found outside the curated lists, classified

| Plugin | iOS? | What it gives you | Verdict |
| --- | --- | --- | --- |
| [`stippi/tauri-plugin-audio`](https://github.com/stippi/tauri-plugin-audio) | yes | **raw `Vec<f32>` PCM** to Rust via `tokio::sync::mpsc::Receiver` | (a) raw PCM — but unadoptable, see below |
| [`brenogonzaga/tauri-plugin-audio-recorder`](https://github.com/brenogonzaga/tauri-plugin-audio-recorder) | yes | `RecordingResult { filePath, durationMs, fileSize, sampleRate, channels }` | (b) **file only — useless** |
| [`ayangweb/tauri-plugin-mic-recorder`](https://github.com/ayangweb/tauri-plugin-mic-recorder) | **no** (own README: iOS ✗, Android ✗) | WAV file | (b) file only, not even iOS |
| [`tauri-plugin-audio` 0.1.1 on crates.io](https://crates.io/crates/tauri-plugin-audio) (GennaroBaratta) | no ("Desktop-only") | pre-reduced telemetry: `WaveformEvent { rms, peak, peaks }` — not PCM | (c) dead; source repo 404s |
| `thelostword/tauri-plugin-audio-route` | yes | switches speaker/earpiece **output** | (c) not capture |
| `silvermine/tauri-plugin-audio` | — | playback only | (c) not capture |
| `HMUysal/tauri-plugin-audio-recorder-android` | Android only | — | (c) irrelevant |

The recording plugins fail our criterion exactly as the ticket anticipated.
`brenogonzaga/tauri-plugin-audio-recorder` is the best-maintained of them (v0.1.2, MIT, ~17 stars,
genuinely Tauri 2) but its full command surface is `start_recording`, `stop_recording`,
`pause_recording`, `resume_recording`, `get_status`, `get_devices`, `check_permission`,
`request_permission` — **no sample buffers and no Tauri events at all**. On iOS it uses
`AVAudioRecorder` writing `kAudioFormatMPEG4AAC` to a file (`ios/Sources/AudioRecorderPlugin.swift`).
It even sets `isMeteringEnabled = true` but never reads `averagePower(forChannel:)` and never exposes
it. Lossy AAC in a file is the worst possible input for an SPL measurement.

**`stippi/tauri-plugin-audio` is the one genuine raw-PCM implementation**, and it is worth knowing about
as a *reference*, not a dependency. Its iOS path is `AVAudioEngine` +
`inputNode.installTap(onBus:bufferSize:format:)`, whose callback crosses `@_silgen_name` FFI into Rust
(`rust_audio_capture_callback(channelData, frameLength, sampleRate)`) into a lock-free `ringbuf`, exposed
as `capture_queue::start_recording() -> Receiver<Vec<f32>>`. That is precisely the §2b fallback design,
already written down and readable. But: 0 stars, 0 forks, created 2026-04-26, **not on crates.io**
(README says vendor it via `path = "../tauri-plugin-audio"`), **no LICENSE file** despite
`license = "MIT"` in `Cargo.toml` (GitHub API reports `license: null`), and it still leaks the AI-tutor
app it was carved out of (`getPlaybackProgress()` returns a *sentence index*; `initSession` forces
`.playAndRecord` and always installs a playback node we do not want). Treat as a reference
implementation to learn from if we ever execute §2b.

### 5d. An adjunct worth noting — and why I still recommend hand-rolling

[`charlesportwoodii/tauri-plugin-audio-permissions`](https://github.com/charlesportwoodii/tauri-plugin-audio-permissions)
is explicitly "Built for use with Rodio/CPAL audio libraries" and offers mic permission
request/check plus "iOS AVAudioSession management" and an Android foreground service, BSD-3-Clause,
Tauri v2, iOS supported. Commands: `requestPermission`, `checkPermission`, `startForegroundService`,
`stopForegroundService`, `updateNotification`, `isServiceRunning`.

That is, on paper, exactly the gap cpal leaves. I still recommend hand-rolling the ~20 lines instead:
it is **not published to crates.io** (I checked the crates.io API — no such crate), has 3 stars and 25
commits, and its README does not state which category/mode it sets — which is the single detail we most
need to control, because we specifically want `Record` + `AVAudioSessionModeMeasurement` rather than the
`playAndRecord` default these plugins tend to assume. Taking an unpublished dependency to avoid 20 lines
of `objc2-avf-audio`, and losing control of the one setting that determines whether our measurements are
calibrated, is a bad trade. Keep it on the list as a fallback if the hand-rolled session setup fights us.

### 5e. Conclusion, including the counter-signal

There is nothing to adopt. No official plugin; community plugins are file-recorders or unadoptable.
This mostly *strengthens* the cpal recommendation — cpal is not a workaround for a missing plugin, it is
the only credible Rust-side option, and what it lacks is 20 lines rather than a subsystem.

**But I want to record the one genuine counter-signal honestly:** both of the iOS-capable plugins that
do real capture (`stippi/tauri-plugin-audio` and `brenogonzaga/tauri-plugin-audio-recorder`)
**deliberately bypass cpal on iOS** and drop to native APIs, using cpal only for desktop.
`stippi`'s cpal usage is gated `cfg(not(any(target_os = "ios", target_os = "android")))`. That is two
independent authors concluding cpal was not the right tool on iOS.

How much weight to give this: **some, but not much.** Both projects predate or ignore the 0.17.0
(2025-12-20) → 0.18.1 (2026-06-07) iOS rework documented in §1a, and `stippi`'s plugin also wants
simultaneous playback with echo handling, which is a genuine reason to want `AVAudioEngine` regardless
of cpal's quality. Still, it is evidence that the iOS backend has not been the community's default
choice, consistent with the "actively maintained but lightly field-tested" reading in §1e. It is the
main reason my confidence in "works first try on device" is only ~60%, and the reason the device spike
is non-negotiable.

---

## Permission wiring — concrete steps

### Step 1 — `src-tauri/Info.plist` (new file; serves BOTH iOS and macOS)

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>NSMicrophoneUsageDescription</key>
    <string>decibel-meter measures ambient sound pressure levels using the microphone. Audio is analysed on-device and never recorded or transmitted.</string>
</dict>
</plist>
```

Nothing needs to be added to `tauri.conf.json` for this file to be picked up — both the iOS CLI path
and the macOS bundler auto-detect `Info.plist` next to `tauri.conf.json`. Use
`bundle.iOS.infoPlist` / `bundle.macOS.infoPlist` only if you want a differently-named file.

Write the purpose string as if a user will read it, because they will — it is displayed verbatim in the
permission alert.

### Step 2 — `src-tauri/Entitlements.plist` (new file; macOS signed builds)

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>com.apple.security.device.audio-input</key>
    <true/>
</dict>
</plist>
```

If we ever enable the App Sandbox, add `<key>com.apple.security.app-sandbox</key><true/>` alongside it.

### Step 3 — `src-tauri/tauri.conf.json`

```jsonc
{
  "bundle": {
    "macOS": {
      "entitlements": "./Entitlements.plist"
    },
    "iOS": {
      "developmentTeam": "XXXXXXXXXX"   // your Personal Team ID; or set APPLE_DEVELOPMENT_TEAM
    }
  }
}
```

`hardenedRuntime` is already `true` by default — leave it alone; the entitlement above is what makes it
compatible with capture.

### Step 4 — `src-tauri/Cargo.toml`

```toml
[dependencies]
cpal = "0.18.1"

# iOS-only: configure the AVAudioSession that cpal deliberately does not touch.
# Version and feature set deliberately match cpal's own iOS dependency so the
# objc2 graph does not duplicate.
[target.'cfg(target_os = "ios")'.dependencies]
objc2-avf-audio = { version = "0.3", default-features = false, features = [
    "std",
    "AVAudioSession",
    "AVAudioSessionTypes",
    "block2",
] }
block2 = "0.6"
```

If the device spike shows interruption recovery is required before it is released, swap the cpal line
for a pinned git dependency:

```toml
cpal = { git = "https://github.com/RustAudio/cpal", rev = "<pin-a-reviewed-sha>" }
```

Pin a SHA, not a branch — `master` moved a lot in the last six months.

### Step 5 — session setup in Rust, before touching cpal

Order matters. The sequence, derived from cpal's example plus Apple's docs:

1. `AVAudioSession::sharedInstance()`
2. `setCategory_mode_options_error(Record, Measurement, …)` — **use the `AVAudioSessionModeMeasurement`
   mode.** For an SPL meter this is the correct mode, per Apple:

   > "A mode that indicates that your app is performing measurement of audio input or output. Use this
   > mode for apps that need to minimize the amount of system-supplied signal processing to input and
   > output signals. If recording on devices with more than one built-in microphone, the session uses
   > the primary microphone. For use with the `record`, `playAndRecord`, or `multiRoute` audio session
   > categories. **This mode disables some dynamics processing** on input and output signals, resulting
   > in a lower-output playback level."

   Source: <https://developer.apple.com/documentation/avfaudio/avaudiosession/mode-swift.struct/measurement>

   Two things to note. First, "the session uses the primary microphone" is exactly what a calibrated
   instrument wants — a deterministic, single, known transducer. Second, Apple says "**some**" dynamics
   processing, not all: `Measurement` reduces but does not provably eliminate system processing, so it
   is a prerequisite for calibration, not a substitute for it.

   This is a deliberate deviation from cpal's example, which uses `PlayAndRecord` +
   `DefaultToSpeaker | AllowBluetooth` because it is an audio-feedback demo. We want `Record`
   (input only) and we specifically do **not** want `AllowBluetooth`, since a Bluetooth SCO route
   forces a low sample rate and an unknown microphone (see Open Risk 2). Use `PlayAndRecord` only if we
   later need to emit a calibration tone.
3. `setPreferredSampleRate_error(48_000.0)`
4. `setPreferredIOBufferDuration_error(...)` — optional; cpal will also do this if you pass
   `BufferSize::Fixed`.
5. `requestRecordPermission(...)` and **wait for the callback**; abort if denied.
6. `setActive_error(true)`
7. **Read back `session.sampleRate()` and `session.inputNumberOfChannels()`** — these are ground truth,
   not what you asked for.
8. Only now: `cpal::default_host().default_input_device()`, then build the input stream with a config
   derived from the values read in step 7 (not from constants).

Do not skip step 7. cpal's `default_input_config()` reads the session at call time, so if you call it
before activation you get whatever the pre-activation session reported.

### Step 6 — iOS project generation

```bash
npm install
npm run tauri ios init      # creates src-tauri/gen/apple for the first time
npm run tauri ios dev       # re-merges Info.plist on every run
```

Because the plist merge re-runs on `ios dev` and `ios build`, later edits to `src-tauri/Info.plist` do
not require re-running `init`. Changes to `bundle.iOS.frameworks` do: delete `src-tauri/gen/apple` and
re-run `tauri ios init`.

Nothing needs to be added to `src-tauri/capabilities/default.json` for this design. That file gates
Tauri's own plugin/core JS APIs; native Rust capture through cpal does not pass through the capability
system at all. The frontend will only need whatever permission the event/IPC channel we use to push
level updates requires, which `core:default` already covers for `emit`.

---

## Open risks — what must be proven on a physical device

Ordered by how much damage they would do if they turn out badly. The follow-up device-spike ticket
should treat items 1–4 as its acceptance criteria.

1. **Does a cpal input stream on a real iPhone deliver non-zero PCM after our session setup?**
   The whole architecture rests on this. Instrument the spike to distinguish the three failure
   signatures explicitly, because they have different fixes:
   - `Err(InvalidInput, "channel count must be at least 1")` → category not set / not active
     (§1d inference — this spike also *verifies that inference*).
   - Stream builds, callback fires, buffers are **exactly** zero → permission not granted. Apple:
     "captures only silence (zeroed out audio samples)".
   - Stream builds, callback never fires → RemoteIO not started, or `play()` not called.
   Log `session.sampleRate()`, `session.inputNumberOfChannels()`, `session.IOBufferDuration()` and
   `stream.buffer_size()` at startup and assert none of them are 0.

2. **What sample rate and buffer size do we actually get, and are they stable?**
   Ask for 48 000 Hz and a 1024- or 2048-frame buffer; record what the session grants. Both Apple APIs
   are documented as *requests*. Then check the granted values again **after** plugging in a wired
   headset and after connecting a Bluetooth headset — Bluetooth SCO commonly forces 8–16 kHz, which
   would wreck FFT bin math and any A-weighting curve computed for 48 kHz. Determine whether we must
   refuse to measure on Bluetooth routes.

3. **Interruption behaviour end to end, on the exact cpal version we pin.** Place a real incoming call
   and invoke Siri. Verify: does the callback stop? does it resume by itself? how long is the gap? does
   *any* error reach the Rust error callback? I expect (§1c) that on `master` it self-heals with **no
   notification**, and on 0.18.1 it dies permanently. Both need confirming, because the answer decides
   whether we pin to git and whether we must register our own interruption observer to keep LAeq
   honest. Also test the lock-screen and app-backgrounding cases, which cpal does not model at all.

4. **Route-change recovery.** Unplug headphones mid-measurement. Confirm we receive
   `ErrorKind::DeviceChanged` / `StreamInvalidated`, and confirm that rebuilding the stream from the
   error callback actually works (note the error callback runs on a notification queue, not the audio
   thread — we must not rebuild the stream from inside it directly; use a channel to a supervisor task).
   Separately: confirm whether the input device identity changed, because calibration must be
   invalidated if so.

5. **`AVAudioSessionModeMeasurement` — does it actually change the input on this hardware, and by how
   much?** This is the difference between an SPL meter and a toy. Apple only promises it "disables
   **some** dynamics processing", which is not a guarantee of a flat, unprocessed signal. It cannot be
   established by reading; it needs a reference sound source. Strictly this belongs to a calibration
   ticket, but the spike should at minimum log raw RMS of the same steady source with and without the
   mode set, so we know the mode is having a measurable effect and in which direction. Also confirm
   Apple's "uses the primary microphone" claim holds, since a device-to-device change of which mic is
   primary would silently invalidate a stored calibration offset.

6. **Free provisioning actually deploying to the device.** Tauri requires
   `bundle.iOS.developmentTeam`; a free Apple ID's Personal Team should satisfy it, and Tauri's docs
   neither confirm nor deny free provisioning for `ios dev`. Unproven. Also note free-provisioning
   profiles expire after 7 days, so the spike should record the re-signing workflow. Cheap to test
   first — do this before anything else, since nothing else is testable without it.

7. **macOS: does `tauri dev` really get microphone access without an `Info.plist`?** §4b contains an
   unresolved conflict between Apple's "the system terminates your app" and issue #9928's report that
   dev mode works. Test on the dev machine (not the device): run `tauri dev` and attempt capture. If it
   is terminated instead, the dev loop needs `tauri build`-then-run, or an embedded `__TEXT,__info_plist`
   section, which would change the dev workflow materially.

8. **macOS signed build with the entitlement.** Verify the §4a diagnosis of #9928 by producing a signed
   build with and without `com.apple.security.device.audio-input` and confirming capture breaks without
   it. Low priority (macOS is dev-only) but it validates the reasoning cheaply, and protects us if we
   ever distribute a macOS build.

9. **cpal iOS field-testing depth.** §1e: the empty issue tracker is ambiguous, and I read it as
   "lightly field-tested" rather than "solid". Unquantifiable by reading. The mitigation is that items
   1–4 above are exactly the tests a lightly-tested backend would fail, so the spike doubles as our
   due diligence. If two or more of items 1–4 fail in ways we cannot fix from the application side,
   escalate to the §2b Swift-plugin fallback rather than trying to patch cpal.

10. **The iOS plist merge is source-verified, not contract-verified, and `@tauri-apps/cli` floats at
    `^2`.** §3b: the precedence order changed silently between tauri-cli 2.4 and 2.9, the behaviour is
    undocumented, and issue #13068 is open with no maintainer reply. A floating `^2` means a routine
    `npm install` can move us across a future change. Cheap mitigations for the implementation ticket:
    pin `@tauri-apps/cli` to an exact version, and add a smoke check that greps the built
    `gen/apple/<AppName>_iOS/Info.plist` for `NSMicrophoneUsageDescription` after `tauri ios build`.
    A missing key is a launch-time process kill, so this deserves a real assertion rather than trust.
    This one is verifiable on the dev machine — it does not need the device.
