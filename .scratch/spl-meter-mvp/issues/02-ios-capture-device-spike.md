# Prove native mic capture works in a Tauri iOS app on a physical device

Parent: [SPL Meter MVP](../map.md)
Type: task (HITL — needs Simon at the keyboard for Apple ID signing)
Status: in progress — desktop half done, device half outstanding
Blocked by: 01

## Question

The one ticket on this map that builds something. Not a feature — a de-risking spike.

Steps:

1. Set up Xcode with **free provisioning** against Simon's Apple ID (no developer account).
2. Run `tauri ios init` and get the generated Xcode project building.
3. Capture mic samples on Simon's iPhone using whatever ticket `01` recommended, and print
   a raw RMS. Nothing else — no weighting, no UI, no spectrogram.

Exists solely to prove `01`'s recommendation before the spec is written on top of it. If
it fails, `01` reopens with a different answer.

Records on resolution: whether it worked, the **actual sample rate and buffer size** the
device handed us, and any permission or route-change surprises. Later tickets depend on
those numbers.

The spike code is throwaway — do not let it become the app.

## Context from ticket 01

Research resolved `01` with a concrete recommendation and permission wiring — read
[`research/01-native-audio-capture-path.md`](../research/01-native-audio-capture-path.md)
before starting, especially its six-step "Permission wiring" section. Summary of what this
spike must do differently from the naive attempt:

- **Set the `AVAudioSession` category and activate the session from Rust** via
  `objc2-avf-audio` *before* touching cpal. cpal does neither. Apple's default category is
  `SoloAmbient`, which permits no input, so skipping this guarantees failure.
- **Set the preferred sample rate, then read back `session.sampleRate()`** — that read-back
  value is the number to record, not what we asked for.
- Put `NSMicrophoneUsageDescription` in a new **`src-tauri/Info.plist`** (serves iOS and
  macOS both).

**Two failure modes that look similar — distinguish them:**

| Symptom | Cause |
|---|---|
| `InvalidInput: channel count must be at least 1` | session category was never set |
| buffers of exact zeros | permission not granted |

**Also probe, since `11` depends on the answers:** what actually happens on an
**interruption** (incoming call, Siri) and a **route change** (plug in headphones) — does
the stream die, silently resume, or return silence? And note the cpal version in use, since
auto-resume is only on `master`, not released 0.18.1.

**Before starting:** `CLAUDE.md` says `src-tauri/gen/` is generated and shouldn't be
hand-edited. Research found that's true of `gen/schemas` but wrong for `gen/apple/`, which
Tauri expects you to commit and edit. Fix or qualify that note so the next session isn't
misled. ✅ **Done** — `CLAUDE.md` now distinguishes the two and records the
"keep `gen/apple/` regenerable" rule.

---

## Progress: desktop half done (2026-08-05)

Everything testable without the iPhone is built and verified. **The ticket's actual
purpose — proving this on device — is untouched.** Do not treat this as resolution.

### What was verified on macOS

The spike captures real audio. Run `cargo run --bin spike` (or `-- 20` for 20 s):

```
input device: coreaudio:BuiltInMicrophoneDevice
default input config: SupportedStreamConfig { channels: 1, sample_rate: 48000,
                      buffer_size: Range { min: 15, max: 4096 }, sample_format: F32 }
requested 1024 frames, stream reports 1024
[ 3.80s] rms   -35.5 dBFS  peak   -22.3 dBFS  blocks 12    frames 12288
--- verdict: CAPTURED — non-zero PCM received ---
```

- **Non-zero PCM confirmed**, and confirmed *responsive*: playing speech through the
  speakers moved RMS from ≈−60 dBFS ambient to ≈−35 dBFS and back. It measures sound, not
  just noise.
- **Desktop numbers:** 48 000 Hz, 1 channel, `F32`, and the 1024-frame buffer request was
  granted exactly. Block rate held steady at 48 blocks/s × 1024 frames ≈ 48 kHz with no
  gaps. Note macOS advertises a buffer floor of **15** frames, far below cpal's iOS
  hardcoded 256 — the iOS constants are not the desktop constants.
- **The iOS-only `AVAudioSession` code compiles**, verified with
  `cargo check --target aarch64-apple-ios --lib`. Method names, the `objc2-avf-audio`
  feature set, and the `objc2` version range in research `01` step 4 are all correct as
  written, and the objc2 graph does not duplicate. Clippy is clean on both targets.
  This does not prove it *works*, only that it builds.
- **The Tauri command path works end to end** on desktop: `run_capture_spike` invoked from
  the webview, captured for the requested duration, and returned its report.

### Open risk 7 — resolved: `tauri dev` does get microphone access

Research `01` §4b flagged an unresolved conflict between Apple's "the system terminates
your app" without a purpose string and issue #9928's report that dev mode works. **#9928 is
right.** A bare `target/debug/` binary with no `Info.plist` captured audio and was not
terminated. No prompt appeared; `tccd` logged the authorisation and granted it against the
**responsible parent process** (the terminal), which is exactly the mechanism §4b inferred.

So the dev loop needs no bundling workaround. The permissive-dev / strict-bundle asymmetry
is real, which means the `Entitlements.plist` audio-input key only starts mattering when we
produce a *signed* macOS build — untested, and still open risk 8.

### Open risk 10 — mitigated

`@tauri-apps/cli` is now **pinned to 2.11.4** rather than floating at `^2`, so a routine
`npm install` cannot silently move us across a plist-merge change. 2.11.4 is past both the
2.9.0 reorder and the 2.11.0 file-associations addition, so the corrected six-source
precedence applies and `src-tauri/Info.plist` *beats* the generated values.
`./scripts/check-ios-plist.sh` asserts `NSMicrophoneUsageDescription` survived the merge —
run it after `tauri ios build`. It currently exits 2 ("no iOS project yet"), as expected.

### One incidental bug worth knowing

Adding the `spike` binary broke `npm run tauri dev` outright: `tauri dev` shells out to a
bare `cargo run`, which became ambiguous with two binaries. Fixed with
`default-run = "decibel-meter"` in `Cargo.toml`. Mentioned because it fails with a Cargo
error that says nothing about Tauri, so it would be baffling on a fresh clone.

### What is built

| Path | What it is |
|---|---|
| `src-tauri/src/spike.rs` | the spike — session setup, capture, RMS, failure-signature diagnostics |
| `src-tauri/src/bin/spike.rs` | desktop runner, no webview or bundle in the way |
| `src-tauri/src/lib.rs` | `run_capture_spike` command (the device needs a trigger; there is no terminal on a phone) |
| `src/App.vue` | bare harness: duration field, run button, report displayed **on screen** |
| `src-tauri/Info.plist` | `NSMicrophoneUsageDescription`, serves iOS and macOS both |
| `src-tauri/Entitlements.plist` | `com.apple.security.device.audio-input`, wired to `bundle.macOS.entitlements` |
| `scripts/check-ios-plist.sh` | asserts the purpose string survived the iOS plist merge |

`App.vue` is a harness, **not a layout** — ticket `09` owns the screen, and guessing at it
here would pre-empt that prototype. All of the above is throwaway.

The spike prints its own diagnosis rather than raw numbers, because research `01` open risk
1 identified three failure signatures that look alike on device and have different fixes.
It reports `CAPTURED`, `ALL-ZERO SAMPLES` (permission not granted), or `NO CALLBACKS`
(RemoteIO never started), and exits non-zero on the latter two — "all zeros" is otherwise a
perfectly successful-looking run. Per-interval rather than cumulative levels, so a gap
shows up as `<< NO AUDIO THIS INTERVAL`.

### Still to do — needs the iPhone and Simon at the keyboard

Nothing below is startable without the device. Acceptance criteria are research `01`
open risks 1–6.

1. **Set `bundle.iOS.developmentTeam`** in `tauri.conf.json` to the Personal Team ID, or
   export `APPLE_DEVELOPMENT_TEAM`. Left unset deliberately — I don't know the value.
   Do this first: open risk 6 says nothing else is testable until deployment works.
2. `npm run tauri ios init`, then `npm run tauri ios dev`. Then
   `./scripts/check-ios-plist.sh`.
3. Tap **Run capture spike**. The verdict line is the answer to open risk 1. The
   `AVAudioSession` block reports the granted sample rate, channel count, and buffer
   duration — **those are the numbers later tickets need**, not what we asked for.
4. **Open risk 2** — re-run with a wired headset and with Bluetooth connected, and record
   the granted rate each time. Bluetooth SCO commonly forces 8–16 kHz, which would wreck
   both the FFT bin math and any weighting curve computed for 48 kHz. Decide whether we
   refuse to measure on Bluetooth routes.
5. **Open risk 3** — take a real call and invoke Siri mid-run. On pinned 0.18.1 the
   prediction is the stream dies permanently and no error reaches Rust. Watch for
   `<< NO AUDIO THIS INTERVAL`. This decides whether we pin cpal to a `master` SHA and
   whether we need our own interruption observer to keep L_eq honest. Also try
   backgrounding and the lock screen, which cpal does not model at all.
6. **Open risk 4** — unplug headphones mid-run and confirm `DeviceChanged` /
   `StreamInvalidated` reaches the error callback. The spike only logs these; it does not
   rebuild. Whether a rebuild from a supervisor task actually works is still unproven.
7. **Open risk 5** — log RMS of a steady source with and without `Measurement` mode, to
   confirm the mode has a measurable effect and in which direction.

Everything in 4–7 feeds ticket [`11`](11-interruption-and-gap-handling.md), which is
blocked on this one.
