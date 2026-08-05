# Which native audio capture path works on iOS and desktop?

Parent: [SPL Meter MVP](../map.md)
Type: research
Status: resolved
Blocked by: —

## Question

The Web Audio API is ruled out — capture must be native. Which native path do we build on?

- Does **`cpal`** support iOS well enough to build on? At what sample rates and buffer
  sizes, and how does it behave when the audio route changes (headphones, Bluetooth, call
  interruption)?
- If `cpal` is not viable, the alternative is calling **`AVAudioEngine`** directly through
  Rust→ObjC bindings. What does that cost in practice?
- What does a **Tauri 2 iOS app** need for microphone permission — where
  `NSMicrophoneUsageDescription` goes given Tauri generates the Xcode project into
  `src-tauri/gen/apple/`, and whether it survives regeneration.
- What **macOS** entitlement or Info.plist key is needed for the desktop dev loop.

Deliverable: a recommended backend with evidence, plus the permission wiring for both
platforms.

This is the highest-leverage ticket on the map — nearly everything downstream depends on
the sample format and delivery model it settles.

## Context

A `/research` subagent resolved this ticket. Full findings, ~1150 lines with sources on
every claim: [`research/01-native-audio-capture-path.md`](../research/01-native-audio-capture-path.md).
Also committed on the throwaway branch `research/native-audio-capture-path`.

## Answer

**Use `cpal` for capture, and configure `AVAudioSession` ourselves from Rust via
`objc2-avf-audio`** — the crate cpal already depends on for iOS, so no new dependency
graph. Do not write a Swift plugin; do not hand-roll `AVAudioEngine`. Confidence: high on
the architecture (~85%), medium on it working first try on device (~60%).

cpal's iOS support is better than its reputation. `src/host/coreaudio/ios/` is a real
backend — RemoteIO Audio Units via `coreaudio-rs`, with a session event manager that
observes route changes and media-services loss and surfaces them to Rust as typed errors.
It delivers `f32` frames in a real-time callback, which is exactly the raw PCM interface
the DSP needs.

**The load-bearing finding: cpal never sets the AVAudioSession category and never
activates the session.** The only mutating session call in the entire iOS backend is
`setPreferredIOBufferDuration`. Apple's default category is `SoloAmbient`, which permits
**no input at all** — so capture fails until we do that setup ourselves. cpal's own iOS
example does it in Objective-C in the host app before calling into Rust. It's roughly 20
lines of `objc2-avf-audio`.

Related: cpal never calls `setPreferredSampleRate`, and only offers the session's
*current* rate as a single-point range (min == max). **We must set the preferred rate
ourselves and then read back `session.sampleRate()` as ground truth** — the device
decides, not us. Every downstream ticket that needs a sample rate must treat it as a
runtime value.

**Permission wiring:** `NSMicrophoneUsageDescription` goes in a new `src-tauri/Info.plist`,
which serves both iOS and macOS. It survives regenerating `gen/apple` because the merge
re-runs on `ios dev`/`build`/`run` but not on `init`. On macOS, `bundle.macOS.hardenedRuntime`
defaults to **true**, and Hardened Runtime requires the
`com.apple.security.device.audio-input` entitlement — so a *signed* build fails where
`tauri dev` succeeds. The dev loop is the permissive case, which is the opposite of the
usual expectation and a good way to lose a day. Concrete steps are in the findings doc
(`Permission wiring — concrete steps`, six steps).

**No official Tauri audio plugin exists** — all 30 official plugins were enumerated. The
community options are file recorders that expose no sample buffers, so they're useless
here.

### Consequences for the rest of the map

1. **Interruptions corrupt measurement silently.** Auto-resume after an interruption is
   only on cpal's `master`, not released 0.18.1 — on 0.18.1 an incoming call kills the
   stream permanently. Worse, even on `master`, interruptions are *never surfaced to Rust
   as errors*: the stream self-heals and produces an unannounced gap in samples. For a
   rolling L_eq that is a correctness bug, not a nuisance. Graduated into ticket `11`.
2. **The sample rate is a runtime value**, not a constant — feeds tickets `03`, `04`, `07`.
3. **Two device failure modes are easy to confuse**, and ticket `02` should watch for both:
   `InvalidInput: channel count must be at least 1` means the category wasn't set; buffers
   of exact zeros mean permission wasn't granted (Apple: an app without permission
   "captures only silence").

### Corrections and conflicts, recorded honestly

- **`src-tauri/gen/apple/` is meant to be committed** — `tauri ios init` writes a
  `.gitignore` inside it, and Tauri's docs tell you to edit `project.pbxproj` and drop
  `PrivacyInfo.xcprivacy` in there. This **contradicts this project's `CLAUDE.md`**, which
  says `src-tauri/gen/` is generated and shouldn't be hand-edited. That claim is true of
  `gen/schemas` (the only thing `src-tauri/.gitignore` actually ignores) but not of
  `gen/apple`. Sources genuinely conflict — Tauri's own repo gitignores its examples'
  `gen/` — so it's reported as a conflict rather than settled. **CLAUDE.md needs a fix
  before ticket `02` runs.**
- The plist precedence order described in tauri#13068 is **stale**, accurate only for
  tauri-cli ≤2.8.x; #14108 reversed it in 2.9.0. The local CLI is 2.11.4, so the corrected
  order applies.
- One conflict left open: Apple says an app without the purpose string is terminated, yet
  tauri#9928 reports `tauri dev` working from a bare binary with no Info.plist. Listed as a
  dev-machine spike item.
- **Counter-signal, not hidden:** both community Tauri plugins that do real iOS PCM capture
  deliberately bypass cpal on iOS — two independent authors disagreeing with this
  recommendation. Discounted because both predate cpal's 0.17→0.18 iOS rework, but it is
  why device confidence is 60% rather than 90%, and why ticket `02` exists.
