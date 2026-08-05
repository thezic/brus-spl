# Session, capture and the idle timer

Parent: [SPL Meter Build](../map.md)
Type: build
Status: open
Blocked by: —

## Build

The bottom of the stack: a configured `AVAudioSession`, a running cpal stream, and per-block
summaries arriving on a queue the display side can drain. Spec
[§3](../../spl-meter-mvp/spec.md#3-audio-capture) and
[§4.4](../../spl-meter-mvp/spec.md#44-the-idle-timer-must-be-disabled).

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
