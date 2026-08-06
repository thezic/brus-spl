# Interruption and recovery

Parent: [SPL Meter Build](../map.md)
Type: build
Status: resolved
Blocked by: [`b01`](b01-session-capture-and-the-idle-timer.md)

## Build

The observer, the reactivation, and the health check. Spec
[§4](../../spl-meter-mvp/spec.md#4-interruptions-gaps-and-recovery).

**This is the first ticket to drop if days run short** — see the map's drop order. Without it
an interruption is still *honest*, because §6.4's coverage reports the hole; it is simply not
*recovered*, and the fix at the venue is to restart the app.

What actually happens, measured (`11` probe 2, Siri invoked 3 s into a 30 s run):

```
GAPS (1):  3.26s → 30.00s (26.74s), NEVER RESUMED
SESSION CHANGES (1):  at 3.26s: rate 48000 → 48000 Hz, channels 1 → 0
stream errors: none
all-zero blocks: 0
```

**The stream died permanently and not one error reached Rust.**

What to build (§4.2):

1. **Observe `AVAudioSessionInterruptionNotification`** via `NSNotificationCenter`, using
   `objc2-foundation` as a direct dependency **version-matched to `objc2-avf-audio`'s
   constraint** so the objc2 graph does not duplicate (§16.3). Forced rather than chosen —
   cpal never surfaces interruptions.
2. **Recovery must `setActive(true)` before rebuilding the cpal stream.**
3. **`inputNumberOfChannels == 0` is a pollable health check**, independent of the
   notification, checked on the 10 Hz tick. `11` measured it going 1 → 0 and staying there, so
   the session is left *deactivated* — which is why a stream rebuild alone is not enough.
4. **Any cpal stream error takes the same recovery path** — reactivate, rebuild. Route changes
   may or may not announce themselves, so nothing assumes an announcement.
5. **After a rebuild, recompute the filter coefficients from the newly read-back rate and zero
   the filter state — but do *not* reset the window.** Same quantity through a
   differently-designed filter, so the old slots remain valid energy.

**Stay on released cpal 0.18.1.** `master`'s auto-resume is *silent*, so gap accounting is
needed either way; we already own session setup, so an observer is an extension rather than new
machinery; and pinning a SHA is ongoing maintenance for something we would still have to wrap.

## Traps

- **The rebuild failure looks exactly like a setup regression.** A rebuild against a
  deactivated session fails with `InvalidInput: channel count must be at least 1` — *the same
  error as a never-set category* (§3.1). Expect it, or an afternoon goes into
  [`b01`](b01-session-capture-and-the-idle-timer.md)'s code looking for a bug that is not
  there.
- **The dead air during a rebuild needs no code.** §6.2's clock-advanced ring produces gap
  slots by construction, for the rebuild and for the interruption alike. Resist the urge to
  mark them.
- A rate change clears **neither** the window nor the max hold — only the filter state
  (§6.11).
- **Route-change recovery has never been tested**; `11` probe 3 was never run for want of
  headphones. If headphones are to hand, run it — plug in and unplug mid-measurement — and
  record what happened in the resolution. If not, say so, and it stays in the map's fog.

## Done when

- `cargo clippy` clean, `cargo check --target aarch64-apple-ios --lib` passes.
- **On the device**: start measuring, invoke Siri, dismiss it, and the meter **resumes on its
  own**. The coverage figure must show the hole for the duration of the interruption and then
  climb back — a recovery that also erased the evidence would be wrong.
- The health check alone recovers a stream killed without any notification (force it by
  whatever means is available; if none is, say so).

## Resolution

**Built, and the supervisor driven against a real cpal stream — but the phone half is
[the Tier 1 device pass](b08-tier-1-device-pass.md)'s, as `b01`'s was.** 96 tests plus one
`#[ignore]`d live one, clippy clean on both targets, the iOS `--lib` check passes, and the app
ran 45 s under `npm run tauri dev` with one build and zero spurious rebuilds. Spec §4 needed no
correction.

The shape is one **supervisor loop on the capture thread** — it has to be there, because a
`cpal::Stream` is `!Send` and stops on drop — waiting on a one-slot mailbox that three unrelated
things post to: the `AVAudioSessionInterruptionNotification` observer (`objc2-foundation`, a
direct dependency version-matched per §16.3), cpal's error callback, and the 10 Hz health check
reading `inputNumberOfChannels`. `session::current_facts` was dead code after `b06` deleted
`capture_diagnostics`; it is replaced by the narrower `input_channels`.

**Finding 1 — the health check must ignore any stream that is not `Running`, and the reason is a
loop that eats itself.** §4.2 item 4 says only *checked on the 10 Hz tick*. But a rebuild
publishes `Starting` while it runs, and a check that fired on that would ask the supervisor to
tear down the stream it had just finished building — one rebuild per tick, forever, with the
meter permanently at `--` and the log a wall of successful builds. The rule is now
`needs_recovery(state, channels)`, guarded on `Running`, and it is the only part of item 4 that
can be tested anywhere but on a phone.

**Finding 2 — cpal's own advice about `DeviceChanged` is wrong for this instrument.** cpal 0.18.1 documents that kind as *"the stream remains
active and no rebuild is required"* — true of the samples, false here: the weighting coefficients
and the published `CaptureFacts` are both derived from the rate read back **when the stream was
built**, so a reroute cpal absorbs silently leaves them describing the old route. Item 5's *any*
is therefore right as written and is honoured without triage.

**Corrected by [`b08`](b08-tier-1-device-pass.md), and the correction matters.** This ticket also
claimed `DeviceChanged` is raised *only* by the WASAPI and PipeWire hosts and that neither CoreAudio
backend has a route-change error path at all. **That is wrong**, and it came from a grep truncated
at twenty lines read as though it were the whole answer. cpal 0.18.1's iOS backend carries a
`session_event_manager` observing `AVAudioSessionRouteChangeNotification`, which maps reasons onto
kinds: `OldDeviceUnavailable` (a headset unplugged) → **`DeviceChanged`**,
`CategoryChange`/`Override`/`RouteConfigurationChange` → `StreamInvalidated`,
`NoSuitableRouteForCategory` → `DeviceNotAvailable`. The error callback is therefore a **live path
on iOS**, not defence, and `b08` watched it fire. What cpal genuinely does not observe is
`AVAudioSessionInterruptionNotification` — it watches route changes and media-services loss/reset
and nothing else — so item 2's observer stays forced rather than chosen, and probe 2's silence is
explained precisely instead of over-generalised.

**Finding 3 — the recovery path must not re-ask for the microphone, and must not stop asking
either.** Two opposite mistakes, one line apart. `Activation::Recovery` skips
`requestRecordPermission` because that call blocks on a completion block delivered to the *main
queue*, and during a Siri call the health check asks for a rebuild ten times a second — so the
naive one-path version puts a main-queue round trip on the capture thread twice a second for the
length of the interruption. But making only the *first attempt* `Initial` is the other error: an
attempt that fails **before** raising the prompt — `setCategory` refused, say — would leave an app
retrying forever that never asks for the microphone at all. It is `Initial` until a build has
actually **succeeded**, not until one has been attempted.

**Finding 4 — a failed rebuild is the normal path, not an error, and the loop is designed around
that.** Every `setActive(true)` is refused while another process holds the session, so for the
whole length of an interruption *failure is what happens*, ten requests a second producing one
doomed attempt every 500 ms; recovery is simply the first attempt that succeeds. Hence: retry
fixed at 500 ms, never give up, log **once per run** of failures, and keep only the **first**
reason in the mailbox — the reason worth reading is `interruption ended`, not the hundredth copy
of the symptom it caused. A supervisor that treated a failed attempt as terminal would have
recovered from nothing; one that logged each attempt would have buried the single line that
matters under sixty.

**Finding 5 — item 6 is zero code, and that is structural rather than lucky.** *Recompute the
coefficients from the newly read-back rate, zero the filter state, do not reset the window* needed
no recovery-specific code: `build` derives `Chains` from the rate it reads back, a fresh `Chains`
is zero-state by construction, and nothing in `capture.rs` can reach `metrics.rs` to reset a
window even by accident. The trap about dead air needing no code is the same observation one level
up. What item 6 actually describes is the consequence of there being exactly **one** path that
builds a stream, which is why the recovery path reuses `configure` rather than adding a
`reactivate`.

**Finding 6 — a recovery is ~210 ms of real dead air, which is over §6.9's threshold, so every
recovery will show `--`.** Measured five times on the desk at 209 / 210 / 211 / 219 / 222 ms from
request to audio flowing again, at ±4 ms. That is *longer* than the 200 ms staleness rule, and the
phone's figure can only be larger — macOS returns from `session::configure` immediately, where iOS
does `setActive(true)` and five property read-backs. So the flicker is expected, and it is **not**
an argument for moving that dial: the audio genuinely stopped, and an instrument that reports its
own state should say so. It also fills half of `b06`'s gap — `--` had never been produced by a
stream that actually stopped, because nothing could stop one; now something can, and the number
says a recovery is long enough for it to appear.

**One race named rather than closed.** Between the condvar waking the supervisor and its
publishing `Starting`, a tick can still see `Running` and ask again, costing one redundant rebuild
and ~210 ms of coverage. Closing it means holding the state lock across a blocking wait, which is
a worse trade; the ordering is arranged so the window is as small as two adjacent statements.

**What the desk actually proved, and what it cannot.** The `#[ignore]`d
`a_requested_rebuild_replaces_a_live_stream_and_audio_keeps_flowing` runs the whole supervisor
against two real CoreAudio streams in 0.7 s: audio flowing, a stubbed trigger, the stream dropped
and rebuilt, `Running` republished, and audio flowing out of the **new** queue. `cargo test` stays
hardware-free and instant — run it with `cargo test -- --ignored`. Five mutations, all caught,
each by exactly one test: an unguarded health check, an overwriting mailbox, an error callback
that records without asking, a request that does not wake the waiter, and a rebuild that does not
count itself. The counter (`Capture::builds`) exists because *did it come back on its own* is a
yes-or-no question and reading it off a log is not one.

## Device pass

**Both device Done-when items passed, on an iPhone 14 Pro that became `available` while this
ticket was open — so the phone half is closed here rather than deferred, and `b01`'s unfinished
half goes with it: this is the first time anything in this effort has run on the device.**
`tauri ios build --debug` reached `BUILD SUCCEEDED` with `objc2-foundation` added, which — like
`objc2-ui-kit` at `b01` — needed **no `bundle.iOS.frameworks` entry and no regeneration**,
asserted by building rather than by reading. `check-ios-plist.sh` passed, `devicectl install` and
`process launch` both worked (the one failure was `RequestDenied … Locked`: a locked phone cannot
be launched onto, which is a fact about `devicectl` and not about the app).

**Item 1 — the Siri test passes.** Measuring, Siri invoked, Siri dismissed: the hero reads `--`
while the microphone is gone, the 60 s coverage figure **drops and then climbs back**, and the
meter resumes **on its own**. The hole stays visible, which is the half of the criterion a
recovery that also erased the evidence would have failed. Recovery was *effectively instant*,
consistent with the notification arriving the moment the interruption ended and with the ~210 ms
measured on the desk.

**Item 2 — the health check alone recovers it, and this is the finding worth carrying.** Forced
by building a variant with the observer compiled out entirely, installing it, and repeating the
Siri test: **it still recovers, in ~0.5 s instead of instantly.** So the 10 Hz poll of
`inputNumberOfChannels` is sufficient on its own — which is what `11` probe 2's total silence
always implied — and the notification observer is **the fast path rather than the mechanism**.
That is the right way round for an instrument: the reliable trigger is the one that cannot fail
to arrive, and the announcement merely saves half a second. Both were live in the shipped build,
and half a second is a fifth of the hole an interruption already costs.

**Also observed, and it belongs to [the Tier 1 device pass](b08-tier-1-device-pass.md) rather than
here:** the app launches, captures, and the numbers **track sound sensibly** — a quiet room reads
much lower than someone talking at it. First evidence that the capture path works on the phone at
all. It is *not* evidence about `Measurement` mode, which is a 21 dB fixed gain that would look
entirely plausible if it silently failed — [`b08`](b08-tier-1-device-pass.md) answered that one
separately, and found a way to read the log that does not need Xcode at all.

**Untested and said so.** The retry loop has no test, because a build failure cannot be forced
without taking the hardware away. `Activation::Recovery`'s skipped permission prompt ran on device
but was never *observed* — it is inferred from the recovery working at all. And **route-change
recovery is half out of the fog**: [`b08`](b08-tier-1-device-pass.md) caught a real
`StreamInvalidated: Audio route changed` at launch and the supervisor recovering from it in 70 ms.
Probe 3's actual gesture — a headset plugged and unplugged mid-measurement — is still not run for
want of headphones, and it is the leg that raises `DeviceChanged` rather than `StreamInvalidated`.
