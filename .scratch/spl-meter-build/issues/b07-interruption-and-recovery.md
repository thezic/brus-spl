# Interruption and recovery

Parent: [SPL Meter Build](../map.md)
Type: build
Status: open
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
