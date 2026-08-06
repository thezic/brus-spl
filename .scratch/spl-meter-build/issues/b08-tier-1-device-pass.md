# Tier 1 device pass

Parent: [SPL Meter Build](../map.md)
Type: build
Status: resolved
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

- ~~**`b01`'s unfinished half: `devicectl install` + `process launch`.**~~ **Done — closed by
  [`b07`](b07-interruption-and-recovery.md)'s device pass**, which found the phone `available`,
  installed, launched, and confirmed the app captures and tracks sound sensibly. The one snag worth
  keeping: **a locked phone cannot be launched onto** — `devicectl` fails with
  `RequestDenied … Locked`, which reads like a signing problem and is not one. Unlock first. The
  original text follows, because the reasoning is still the reasoning:
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

Diagnostics are now **only in Xcode's log** — `println!` does not reach `devicectl … --console`,
and [the screen](b06-the-screen.md) deleted the temporary `capture_diagnostics` readout that stood
in for it, because spec §3.1 keeps the session read-back out of the UI. **So the loop above cannot
answer the `Measurement`-mode question**, which is this ticket's highest-consequence check: launch
from Xcode for that one, or re-add a throwaway readout — five minutes, and it need not survive the
ticket.

## Done when

The meter runs on the phone and every item above has an answer, including the ones whose answer
is "not tested". Record anything that moves a settled parameter in the resolution, and graduate
it onto the map — the §13.14 items are in **Not yet specified** precisely so this ticket can
start clearing them.

## Resolution

**Tier 1 runs on the phone, and every item has an answer.** The pass turned up two corrections to
the spec, one to its own method, and one measurement that matters more than anything on the
checklist. Nothing was found that needs code changed.

### The method changed first: the log is readable from a plain shell

This ticket was written believing its highest-consequence check needed Xcode, because §2.2 measured
that `println!` does not reach `devicectl … --console` — which two tickets then read as *on-device
diagnostics must be on screen*. **It is only half true.** `idevicesyslog` carries Rust's `eprintln!`
verbatim, tagged `[stderr]`:

```
idevicesyslog -m "capture:" -m "session:" -m "settings:"
```

No Xcode, no throwaway readout, and it works while the app runs unattended — which is what made the
idle-timer and backgrounding findings below possible at all, since those want a log across fifteen
minutes rather than a glance at one. (`idevicescreenshot` from the same package does *not* work
here: it wants the developer disk image over the classic lockdown path.) Recorded in `CLAUDE.md` and
in spec §17.

### The checklist

**`Measurement` mode applied — the 21 dB question, answered `true`.** And every preference granted
exactly, not approximately:

```
capture: running CaptureFacts { device: "coreaudio:default", sample_format: "f32",
  sample_rate: 48000, channels: 1, buffer_frames: Some(1024),
  session: SessionFacts { sample_rate: 48000.0, input_channels: 1,
    mode: "AVAudioSessionModeMeasurement", measurement_mode: true,
    io_buffer_duration: 0.021333333333333333, permission_granted: true } }
```

`io_buffer_duration` is 1024/48000 to the last digit. This is `b01`'s measured device baseline
reproduced by the real app rather than by a spike.

**It launches and captures**, `check-ios-plist.sh` passes, and the numbers track sound sensibly — a
quiet room reads far below someone talking at it.

**The idle timer holds, and this is now a measurement rather than an impression.** The app sat
frontmost from 15:31:18 to 15:45:46 — **14 min 28 s** — with `deviceLocked:0` throughout, no display
dimming, and capture uninterrupted. The moment it was backgrounded the same phone dimmed after 42 s
and locked after 61 s, which puts auto-lock at about a minute. So spec §4.4's two lines held the
screen awake for **roughly fourteen times the device's own auto-lock**, and the evidence is the
device's own log rather than someone watching it.

**The hero on real sound: `S` reads live, not twitchy.** First real evidence on one of §13.14's
three unknowns, and it supports `09` d5's choice of `S` as the default.

**`--` at hero size reads fine at arm's length.** Half of the arm's-length question; the 32-band,
~6 px-per-band half is still open and belongs before
[the canvas](b11-the-spectrogram-canvas.md).

**Settings persist**, across a relaunch *and* across an install-over-the-top: `offset_db:
Some(119.83960458330606)` survived four reinstalls during this session.

**An interruption recovers** — closed by [`b07`](b07-interruption-and-recovery.md), both criteria.

**The uncalibrated state is deliberately not tested.** Seeing it means clearing a calibration that
is currently good, and the honest way to do that — delete and reinstall, then retype the offset — is
[the re-sign rehearsal](b12-re-sign-and-install-rehearsal.md)'s own gesture. Deferred there rather
than done twice. `−108.4` not overflowing is `b06`'s arithmetic, checked at four widths, and
unverified in situ.

**`"denied"` is still not produced.** The microphone was granted, so §9.4's most useful state remains
rendered-but-never-refused. One tap in Settings, and it stays on the map.

### The finding that matters: backgrounding is survivable now, and spec §4.3 was wrong about why

Fifteen minutes into the unattended run the log caught this:

```
15:45:46  scene net.thezic.decibel-meter-default  foreground -> inactive -> unspecified
15:45:47  capture: stream error StreamInvalidated: Audio route changed
15:45:47  capture: rebuilding — stream error StreamInvalidated
15:45:47  capture: setActive: failed: NSError { code: 561015905, "Session activation failed" }
          … ~140 retries at 500 ms, logged exactly once …
15:46:57  capture: running CaptureFacts { … }
```

`561015905` is `'!pla'` — `AVAudioSessionErrorCodeCannotStartPlaying`. **iOS refuses to activate an
audio session for an app that is not frontmost, for exactly as long as it is not frontmost.**

Spec §4.3 recorded `11` probe 2b as *the app dies; whether iOS terminated it or our own code faulted
was never characterised*. **It does not die.** Same PID either side of a 70 s background period that
included a full device lock and unlock. What stops is the session, and it stops for a reason that is
documented, specific, and not a bug. The spec now says so, and §17 carries the correction.

**And the outcome is better than §4.3 assumed, because of `b07`.** Recovery took 3 s from returning
to the foreground, unaided. Without the supervisor's never-give-up retry the app would have come
back frontmost holding a permanently dead stream — precisely the *restart the app* state §4.3 treated
as terminal. `b07`'s two least-defensible-looking choices are what did it: **retry forever** rather
than give up after N, and **log once per run** rather than per attempt, which is why ~140 failed
activations produced one line instead of burying the recovery.

Worth stating plainly: **this is not an argument for the `audio` background mode.** The meter is
foreground-only by design and the coverage figure reports the hole honestly. What changed is that
the hole now closes by itself.

### Two smaller things

**A route change fires at every launch.** The route settles a few tens of ms after the session
activates, cpal raises `StreamInvalidated`, and the supervisor rebuilds — 70 ms, before anything has
been measured. Harmless, and it means **`Capture::builds` reads `2` after a clean launch, not `1`**;
the doc comment said otherwise and now says this.

**The log-once design is verified in the wild.** One `setActive: failed` line stood for about 140
attempts, and the success that ended them was still logged. A per-attempt log would have put 140
identical lines between the failure and its resolution.

## Done when

Everything above has an answer, including the two whose answer is "not tested and here is why".
**Tier 1 is closed.** [Tier 2](b09-spectrum-analysis-and-the-column-ring.md) and
[the re-sign rehearsal](b12-re-sign-and-install-rehearsal.md) are unblocked.
