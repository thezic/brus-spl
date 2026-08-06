# Tier 1 device pass

Parent: [SPL Meter Build](../map.md)
Type: build
Status: open
Blocked by: [`b06`](b06-the-screen.md), [`b07`](b07-interruption-and-recovery.md)

## Build

Nothing new. This ticket exists because **the phone is the only place most of Tier 1 can
actually be judged**, and because a tier that has not run on the device is not done.

The loop (spec §2.2, ~1 minute per cycle once Rust is cached):

```bash
npm run build
env -u FORCE_COLOR npx tauri ios build --debug
xcrun devicectl device install app --device <udid> \
  src-tauri/gen/apple/build/arm64/decibel-meter.ipa
xcrun devicectl device process launch --device <udid> net.thezic.decibel-meter
./scripts/check-ios-plist.sh
```

## What to check, and why each is here

- **`b01`'s unfinished half: `devicectl install` + `process launch`.** Inherited from
  [Session, capture and the idle timer](b01-session-capture-and-the-idle-timer.md), which
  proved the *link* — `tauri ios build --debug` reached `BUILD SUCCEEDED` with zero
  `Undefined symbols` — but never got the app onto the phone, because the paired iPhone 14 Pro
  reported `unavailable` to `xcrun devicectl list devices`. Nothing on the device has ever run.
  **Check the device is `available` before anything else in this ticket**, since every item
  below is blocked on the same install. If it is still `unavailable`, that is the first thing to
  fix, not a reason to defer the pass.
- **It launches and captures.** A missing `NSMicrophoneUsageDescription` is a launch-time
  process kill, not a build error — which is what `check-ios-plist.sh` exists to catch after
  the undocumented `Info.plist` merge.
- **`Measurement` mode applied.** Read the log for a mode mismatch. If it silently failed, every
  number is 21 dB out and the calibration you set at the venue will be meaningless (§3.2, §13.9).
- **The reported rate, channel count and buffer duration**, against what was asked for. They are
  preferences, not guarantees.
- **The idle timer.** Leave it on screen, untouched, for longer than the device's auto-lock
  interval. If the screen sleeps, measurement dies and the whole thing is moot for a 45-minute
  talk.
- **The hero on real sound.** Does `S` read as live rather than twitchy? This is one of the
  three §13.14 unknowns and the first real evidence about it.
- **`--` at hero size**, in situ. It should read as a muted absence, not a redaction bar. Also
  worth a first look at **arm's-length legibility** — of the `--` and of the numbers — since
  that question is due before [the canvas](b11-the-spectrogram-canvas.md), not after.
- **The uncalibrated state.** The app starts life showing `dBFS`; check that it looks
  deliberate and that the widest state (`−108.4`) does not overflow.
- **Settings persist across a relaunch**, and across an install-over-the-top.
- **An interruption recovers**, if [`b07`](b07-interruption-and-recovery.md) landed.

Diagnostics must be **on screen or in Xcode's log** — `println!` does not reach
`devicectl … --console`.

## Done when

The meter runs on the phone and every item above has an answer, including the ones whose answer
is "not tested". Record anything that moves a settled parameter in the resolution, and graduate
it onto the map — the §13.14 items are in **Not yet specified** precisely so this ticket can
start clearing them.
