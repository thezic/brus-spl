# Session, capture and the idle timer

Parent: [SPL Meter Build](../map.md)
Type: build
Status: resolved
Blocked by: —

## Build

The bottom of the stack: a configured `AVAudioSession`, a running cpal stream, and per-block
summaries arriving on a queue the display side can drain. Spec
[§3](../../../docs/spec.md#3-audio-capture) and
[§4.4](../../../docs/spec.md#44-the-idle-timer-must-be-disabled).

**`session.rs`** (iOS only, behind `cfg(target_os = "ios")`) — §3.1, in this order and before
touching cpal:

```
category  = AVAudioSessionCategoryRecord
mode      = AVAudioSessionModeMeasurement
setPreferredSampleRate(48000)
setPreferredIOBufferDuration(1024 / 48000)
setActive(true)
```

Then **read back `sampleRate`, `inputNumberOfChannels`, `mode` and `IOBufferDuration` and use
them as ground truth**, logging any mismatch between asked and granted. Nothing in the UI.
Apple documents both the rate and the buffer duration as *preferences*; `02` had both granted
exactly, which is a fact about one device and one route rather than a guarantee.

The mode read-back is not ceremony. §3.2: `Measurement` is worth **21 dB of fixed processing
gain**, so if it ever silently fails to apply, the stored calibration offset is wrong by ~21 dB
with nothing to show it.

**`capture.rs`** — cpal 0.18.1 stream on the default input device. In the callback, and
nothing more (§3.3):

- **f32 → f64 at the block boundary.** All DSP in f64; `03` measured f32 state degrading by
  +1.7 dB with a DC offset and +3.9 dB with 5 Hz rumble, which is exactly what a phone mic in
  a venue delivers.
- **Channel 0 only if the stream reports more than one channel.** Averaging correlated
  channels moves the level by up to 6 dB and would make calibration depend on channel count
  (§16.9).
- **No DC blocker** — the weighting filters are high-passes, and the FFT tap wants the DC.
- Sum `p²` and `n` over the block and push one summary `{sum_sq, n, t}` into a **bounded
  lock-free SPSC queue** (`rtrb`, §16.2), sized at ~1 s of blocks. **No locks, no
  allocation.** An overflow must degrade into lost coverage, never a wrong number (§6.10).

The weighting chain that belongs between those two steps arrives with
[the weighting filters](b02-the-weighting-filters-and-their-test.md); until then push the
unweighted sum, which is Z and is a real mode anyway.

**The idle timer** — `UIApplication.isIdleTimerDisabled = true`, once at startup, from Rust
via `objc2-ui-kit`, **on the main thread**. Two lines and load-bearing: without it the screen
sleeps mid-talk and measurement dies on its own. UIKit is already in Tauri's hardcoded
framework list, so this needs **no `bundle.iOS.frameworks` entry and no project
regeneration** — unlike AVFAudio (§2.1).

**Delete the spike**, since this is what replaces it: `src-tauri/src/spike.rs`,
`src-tauri/src/bin/spike.rs`, the `run_capture_spike` command, and the `[[bin]]` and
`default-run` stanzas in `Cargo.toml`. **The permission wiring, the framework linking,
`tauri.conf.json` and `scripts/check-ios-plist.sh` are not throwaway and must survive**
(§12).

`App.vue`'s harness goes too — but this ticket needs *something* on screen, because `println!`
does not reach `devicectl … --console` (§2.2). Replace it with a deliberately temporary
readout: block count, reported rate and channels, and raw dBFS. [The screen](b06-the-screen.md)
replaces it properly.

## Traps

- **The framework link is invisible to `cargo check`, including
  `--target aarch64-apple-ios`.** Nothing links until Xcode does. `AVFAudio`, `AudioToolbox`
  and `CoreAudio` are already listed in `bundle.iOS.frameworks`; if that list is touched, the
  project must be regenerated with
  `rm -rf src-tauri/gen/apple && env -u FORCE_COLOR npx tauri ios init`.
- **Never run a `tauri ios` command with `FORCE_COLOR` set.** `echo $FORCE_COLOR` first. The
  failure is the misleading `Arch specified by Xcode was invalid. {arch}`.
- **Two failure signatures that look nothing alike** (§3.1): `InvalidInput: channel count must
  be at least 1` means the category was never set *or* the session is deactivated after an
  interruption; **buffers of exact zeros** mean permission was not granted.
- cpal never sets the category and never activates the session — Apple's default
  `SoloAmbient` permits no input, so skipping it guarantees failure.
- A device build is a **release** build. Nothing may be gated on `import.meta.env.DEV`.

## Done when

- `cargo clippy` clean, `cargo fmt` applied, `cargo check --target aarch64-apple-ios --lib`
  passes.
- On macOS under `npm run tauri dev`: block summaries arrive, the temporary readout shows a
  plausible dBFS that responds to sound, and the reported rate/channels match what the device
  actually gave.
- **The device canary.** `npm run build` → `env -u FORCE_COLOR npx tauri ios build --debug` →
  `devicectl device install app` → `devicectl device process launch`. It must **link**, launch,
  and show non-zero levels on the phone. This is the whole point of doing this ticket first: it
  is the earliest possible moment the Xcode link step can fail, and every later ticket piles
  code on top of it.
- The spike is gone and `cargo build` produces one binary.

## Resolved (2026-08-06)

Code is in place and verified as far as the hardware allowed. **The one outstanding item —
`devicectl install` + `process launch` — is now
[the Tier 1 device pass](b08-tier-1-device-pass.md)'s**, recorded there explicitly rather than
held open here. The risk it carries is launch-time, not link-time, and link-time is the only
part that had to be proven first.

**What exists now**

| file | what it holds |
|---|---|
| `src-tauri/src/session.rs` | the §3.1 sequence, the read-back, the mismatch log, and `current_facts()` for `b07`'s polling |
| `src-tauri/src/capture.rs` | `Capture::start()`, the cpal stream, the callback, the `rtrb` queue, `drain()` |
| `src-tauri/src/lib.rs` | idle timer, capture ownership, and the one temporary command |
| `src/App.vue` | the temporary readout, replaced whole by [the screen](b06-the-screen.md) |

`src-tauri/src/spike.rs`, `src-tauri/src/bin/spike.rs`, the `run_capture_spike` command and the
`[[bin]]`/`default-run` stanzas are gone. `Info.plist`, `Entitlements.plist`,
`bundle.iOS.frameworks` and `scripts/check-ios-plist.sh` are untouched (§12).

### Findings

1. **The canary passed at the step that matters.** `env -u FORCE_COLOR npx tauri ios build
   --debug` reached `** BUILD SUCCEEDED **` and exported
   `gen/apple/build/arm64/decibel-meter.ipa`, signed against the existing free-provisioning
   profile. Zero occurrences of `Undefined symbols` in the build log. **This is the whole
   reason the ticket went first**, and it now covers `objc2-ui-kit` as well as the three
   frameworks `02` proved: the idle timer needed **no `bundle.iOS.frameworks` entry and no
   `tauri ios init` regeneration**, exactly as §2.1 predicted — asserted by building, not by
   reading.
   `./scripts/check-ios-plist.sh` passes after the build: `NSMicrophoneUsageDescription`
   survived the merge.

2. **Install and launch were not run.** `xcrun devicectl list devices` reports the paired
   iPhone 14 Pro in state `unavailable`, and `devicectl device install app` fails with
   `CoreDeviceService was unable to locate a device matching the requested device identifier`.
   So *link* is proven and *runs on the phone* is not. The remaining risk is launch-time, not
   link-time, and it is [the Tier 1 device pass](b08-tier-1-device-pass.md)'s to close;
   the one thing that could not have been discovered any other way is closed.

3. **macOS, under `npm run tauri dev`, is capturing.** Ground truth as reported, not as asked
   for: `coreaudio:BuiltInMicrophoneDevice`, `f32`, **48 000 Hz, 1 channel, 1024-frame
   buffer** — the same shape as the device baseline in §2.

4. **Block rate and level both measured, not assumed.** The frontend's 10 Hz poll drained 4–5
   blocks per call with `n` of 4096/5120 frames, i.e. **46.875 blocks/s = 48000/1024 exactly**.
   A quiet room read **−43 to −52 dBFS**; the same room with speech played through the
   speakers peaked at **−18.8 dBFS** against a quiet ceiling of **−42.9 dBFS** over the same
   interval length. The readout responds to sound, and the level is unweighted (Z) mean-square
   dBFS.

5. **The queue is 47 slots**, `ceil(48000 / 1024)` = ~1.00 s of blocks (§6.10), computed from
   the granted rate rather than hardcoded. Against 4–5 blocks drained per 100 ms tick that is
   ~10× headroom. The overflow counter was not separately displayed during the macOS run, so
   "no blocks were dropped" is an inference from the drain counts rather than a reading.

6. **Nothing in spec [§16](../../../docs/spec.md#16-what-this-spec-decides-that-no-ticket-decided)
   failed contact with code, of the four items this tier touches.** `rtrb` (§16.2) is 0.3.4 and
   its `Producer::push` returning `Err` on a full queue is exactly the degrade-into-lost-coverage
   shape §6.10 wants — no veto. Channel 0 only above one channel (§16.9) cost one `chunks_exact`
   and never fired on the measured mono route. The mean-square dBFS convention (§16.4) and the
   refusal to publish a level at zero power (§16.7) are both already in the temporary readout,
   which is why a denied microphone will show `--` rather than a very quiet room. §16.1, §16.3,
   §16.5, §16.6, §16.8, §16.10 and §16.11 are untouched at this tier.

### Two calls this ticket made that the spec did not

7. **The microphone permission request is kept, and moved off the main thread.** §3.1's ordered
   list does not mention asking; the spike did ask, and dropping it means the implicit prompt
   fires during activation while the stream underneath it records exact zeros. So
   `requestRecordPermission` is retained between `setPreferredIOBufferDuration` and
   `setActive` — the five listed steps still happen in the listed order. Two consequences worth
   writing down:
   - **It blocks on a completion block the OS delivers on the main queue**, so
     `session::configure()` must never run on the main thread. That is why `Capture::start()`
     spawns a thread and returns immediately instead of doing this in Tauri's `setup`. Anyone
     who inlines it will deadlock the app before it draws a frame.
   - **A denial does not abort capture.** The stream is built anyway and delivers exact zeros,
     which §6.3 discards at drain time, so the meter lands on `--` beside `0s of 60s` — the
     designed state — rather than on a `Failed` capture that would read as `unavailable` and
     conflate the one failure the user can actually fix (§9.4).

8. **Non-`f32` input is refused rather than converted.** §3.3 is written against f32 and both
   CoreAudio backends deliver it; the spike's five-format ladder existed to survive unknown
   desktop hardware. Converting in the callback is work §3.3 forbids there, so the stream build
   returns an error naming the format instead. If a dev machine ever reports otherwise this is
   the line that will say so.

### Where the weighting chain plugs in

One place, marked in the source. `src-tauri/src/capture.rs`, inside the per-frame loop of the
data callback:

```rust
let x = frame[0] as f64;
// ─── the weighting seam ───
let y = x;          // ← b03 replaces with `chain.process(x)`
sum_sq += y * y;
```

Everything either side of it is final: f32 → f64 has already happened, channel 0 has already
been selected, and `sum_sq`/`n` are already what [the metrics pipeline](b03-the-metrics-pipeline.md)
consumes. The chain state has to live in the closure and be `Send`, since the callback owns it.
Nothing was stubbed or duplicated — until `b03` lands the sum is unweighted, which is Z.

The drain side is `Capture::drain(|BlockSummary { sum_sq, n, t }| …)`, which pops everything
waiting, oldest first, and returns the count. `b05`'s 10 Hz thread calls it in place of the
temporary command; `BlockSummary::t` is there for §6.9's 200 ms staleness rule and nothing else,
since the ring is advanced by the clock (§6.2) rather than by the block.

### Verification run

| command | result |
|---|---|
| `cargo check` | pass |
| `cargo check --target aarch64-apple-ios --lib` | pass |
| `cargo clippy --all-targets` | no warning in any file this ticket owns (28 warnings, all in `weighting.rs`, `b02` mid-write) |
| `cargo clippy --target aarch64-apple-ios --lib` | pass, no warnings |
| `cargo test` | 21 passed, 0 failed (all `b02`'s) |
| `rustfmt --check` on the four owned files | clean |
| `cargo build` | one binary, `target/debug/decibel-meter` |
| `npm run build` | pass (`vue-tsc --noEmit` + `vite build`) |
| `npm run tauri dev` on macOS | capturing; findings 3 and 4 |
| `npx tauri ios build --debug` | `** BUILD SUCCEEDED **`, `.ipa` exported |
| `./scripts/check-ios-plist.sh` | pass |
| `devicectl device install app` / `process launch` | **not run** — device `unavailable` |

`cargo fmt` was **not** run: it formats the whole module tree from `lib.rs`, including
`weighting.rs`, which `b02` owns and is writing. `rustfmt` was pointed at the four owned files
instead, and would change nothing in them.
