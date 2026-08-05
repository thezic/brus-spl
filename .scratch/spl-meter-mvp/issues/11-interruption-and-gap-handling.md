# Interruptions and sample gaps: how do they affect measurement validity?

Parent: [SPL Meter MVP](../map.md)
Type: grilling (+ device probing — HITL, needs Simon and the iPhone)
Status: open — unblocked, this is the frontier
Blocked by: —  (was 02, now resolved)

## Question

Graduated from the fog by ticket
[`01`](01-native-audio-capture-path.md) — this was not visible while charting.

Research found that `cpal` on iOS **does not surface audio-session interruptions to Rust
as errors**. On released 0.18.1 an incoming call kills the capture stream permanently. On
`master`, the stream auto-resumes — but silently, producing an **unannounced gap in the
sample stream**. Route changes (headphones, Bluetooth) are a milder version of the same
problem.

For a rolling L_eq this is a correctness bug rather than an inconvenience: a gap makes the
average wrong, and the number on screen looks perfectly plausible while being wrong. Simon
reads that number and decides whether to turn the talk down. A quietly-wrong reading is the
worst possible failure mode for this app.

Decisions needed:

- **Do we pin cpal `master` at a SHA, wait for a release, or handle the reconnect
  ourselves?** Pinning a SHA is a maintenance cost; released 0.18.1 needs us to detect the
  dead stream and rebuild it.
- **Do we observe `AVAudioSessionInterruptionNotification` ourselves?** Research says we
  need to regardless of cpal version, because cpal never tells Rust. What does the observer
  hook into given we're already calling `objc2-avf-audio` for session setup?
- **What does the meter do when a gap happens?** Options, roughly in increasing honesty:
  drop the affected samples and carry on; exclude the gap from the L_eq window so the
  average covers less real time than it claims; or **invalidate the window** and make the
  user reset. The third is the only one that never shows a wrong number.
- **How is invalidity shown?** Constraint from charting: no warnings, no automation. But
  "this reading is not trustworthy" is not a warning about *sound* — it's the instrument
  reporting its own state, which is different. Worth putting to Simon explicitly rather
  than assuming the no-warnings rule covers it.
- **Does a route change change the calibration?** A different microphone means the stored
  broadband offset is wrong. Interacts with ticket
  [`06`](06-calibration-model.md).
- **A route change can also drop the sample rate below the measurable floor.** From ticket
  [`03`](03-iec-weighting-filters.md): the weighting filters need **~40 kHz minimum**
  (36.9 kHz for class-1 limits, 39.0 kHz for the design band). An iOS **Bluetooth headset
  mic route typically supplies 8 or 16 kHz** — far below that, at which point the reading
  is not merely imprecise but out of tolerance and indefensible. Does the app refuse to
  measure on such a route, or measure and mark the reading invalid? This is the same
  "instrument reports its own state" question as the gap case above, so answer both
  together.

Depends on `02` because the device spike will show what interruptions actually do on real
hardware — whether the stream dies, resumes, or returns silence.

---

## Device probes inherited from ticket 02 (2026-08-05)

`02` is resolved: capture works. Its remaining open risks were **moved here** by Simon's
decision, because they are all interruption and route-change behaviour. **Answer these by
measurement before deciding anything above** — the decisions listed above hinge on what the
hardware actually does, and research `01` was explicit that its predictions are inferences
from reading cpal's source, not observations.

**Baseline to compare against**, from `02` on an iPhone 14 Pro (iOS 26.5.2, cpal **0.18.1**,
the released version with *no* interruption handling):

| | |
|---|---|
| Sample rate | 48 000 Hz (requested and granted) |
| Channels | 1, f32 |
| Buffer | 1024 frames (`IOBufferDuration` 0.021333 s) |
| Block rate | 46.875 /s — 469 blocks in 10.005 s, no gaps |
| Idle room | rms ≈ −59.6 dBFS, peak ≈ −33.3 dBFS |

### The probes

1. **Route changes and the sample rate floor** (was `01` open risk 2). Re-run with a wired
   headset, then with a Bluetooth headset, recording the granted rate each time. Bluetooth SCO
   commonly forces **8–16 kHz**, far below research `03`'s ~40 kHz minimum — at which point the
   reading is out of tolerance, not merely imprecise. This is the evidence for the
   refuse-to-measure question above.
2. **Interruptions** (was open risk 3). Take a real incoming call; invoke Siri; background the
   app; lock the screen. On 0.18.1 the prediction is the stream **dies permanently with no
   error reaching Rust**. Watch for `<< NO AUDIO THIS INTERVAL` in the spike output — that
   marker exists for exactly this. Decides whether we pin cpal to a `master` SHA and whether we
   need our own `AVAudioSessionInterruptionNotification` observer.
3. **Route-change recovery** (was open risk 4). Unplug headphones mid-run and confirm
   `DeviceChanged` / `StreamInvalidated` actually reaches the error callback. The spike only
   *logs* these — it does not rebuild. Whether a rebuild from a supervisor task works is
   unproven. Also check whether the input device identity changed, since that invalidates
   calibration (ticket [`06`](06-calibration-model.md)).
4. **`Measurement` mode effect** (was open risk 5). Log RMS of the same steady source with and
   without `AVAudioSessionModeMeasurement`, to confirm the mode has a measurable effect and in
   which direction. Apple promises only that it disables *some* dynamics processing, so this is
   the difference between an instrument and a toy — but it cannot be settled by reading.

### Probe results (2026-08-05, iPhone 14 Pro, iOS 26.5.2, cpal 0.18.1)

#### Probe 2 — interruption (Siri): stream dies permanently, silently. **Confirmed.**

```
blocks: 141   all-zero blocks: 0
frames: 144384 of 1440054 expected    missing: 1295670 frames = 26993 ms
GAPS (1):  3.26s → 30.00s (26.74s), NEVER RESUMED
SESSION CHANGES (1):  at 3.26s: rate 48000 → 48000 Hz, channels 1 → 0
stream errors: none
```

Research `01` §1c predicted this and was right on every point:

- **The stream never resumed.** 3.0 s of audio, then nothing for the remaining 27 s.
- **No error reached Rust.** `stream errors: none` — the error callback was never called.
  Doing nothing is therefore not an option; a rolling L_eq would have averaged 60 s of window
  over 3 s of data and shown a plausible, wrong number.
- **It stopped rather than returning silence.** `all-zero blocks: 0`, so this is distinct from
  the permission-denied signature.

**New finding not in research `01`: `inputNumberOfChannels` went 1 → 0 and stayed there.** The
sample rate was unchanged. This means the session was left **deactivated** — cpal 0.18.1 has no
interruption handling at all, so nothing ever calls `setActive(true)` again. Two consequences:

1. There *is* a pollable signal for "input is gone", independent of the notification: the
   session reporting zero input channels. Useful as a backstop, or as a cheap health check.
2. Recovery requires re-activating the session ourselves, not merely rebuilding the cpal
   stream. A rebuild against a deactivated session would fail with the
   `InvalidInput: channel count must be at least 1` signature from research `01` §1d — the same
   error as a never-set category, which will be confusing if we don't expect it.

#### Probe 2b — backgrounding / lock screen: **the app does not survive it.**

Reported as "crashed (shut down) after capture finished". Not characterised further: it is not
yet known whether iOS suspended and then terminated the app, or whether something in our own
code faulted on return to the foreground. Either way, **capture does not survive
backgrounding**, which cpal does not model at all (research `01` §1e item 4).

This raises a scope question that was never discussed while charting — see the decisions below.

#### Probe 4 — `Measurement` mode: **21 dB effect. It is mandatory.**

Same 440 Hz sine source, same run length:

| Session mode | RMS |
|---|---|
| `Measurement` | **−51.7 dBFS** |
| `Default` | **−30.6 dBFS** |

**A 21.1 dB difference.** Apple only promises that `Measurement` "disables *some* dynamics
processing", which understates it considerably on this hardware: `Default` is applying roughly
21 dB of processing gain to the input.

**The important part is not the 21 dB, it is linearity.** A single stored broadband calibration
offset (ticket [`06`](06-calibration-model.md)) is only valid if the input path is linear —
gain that varies with level cannot be corrected by a constant. Processing that large is very
likely level-dependent (AGC-like), which would make `Default` mode structurally uncalibratable
rather than merely offset. So `Measurement` mode is not a refinement, it is a precondition for
the calibration model to mean anything.

Caveats worth stating: this is **one measurement pair, not a controlled sweep**, and the source
level was not independently verified between runs. The conclusion "the mode has a large
measurable effect, in the direction of less gain" is solid; the exact 21.1 dB is not a constant
to rely on.

**Follow-up probe worth running, and it needs no extra hardware** — it tests linearity, which
is what ticket `06` actually depends on: play the same sine at two known source levels a
documented distance apart (e.g. generator at −20 dBFS then −40 dBFS) in `Measurement` mode, and
check the measured RMS delta matches the source delta. If it does, a fixed offset is defensible.
If it does not, calibration needs rethinking before the spec is written.

#### Probes 1 and 3 — route changes: **not run, no headphones available.**

Still open, and both need hardware:

- **Probe 1 (Bluetooth / wired rate).** The concern stands on research `03`'s ~40 kHz floor
  regardless of measurement; what is unmeasured is what rate this device actually grants on
  those routes. The *policy* decision can be made without it.
- **Probe 3 (route-change recovery).** Whether `DeviceChanged` / `StreamInvalidated` actually
  reaches the error callback, and whether a rebuild from a supervisor task works, is unproven.
  Note probe 2 showed the error callback stayed silent for an interruption, which weakens any
  assumption that route changes will announce themselves either.

### How to run them

Two constraints learned in `02`, both of which shape this work:

- **`println!` does not reach `xcrun devicectl … --console`.** The on-screen report in
  `App.vue` is the only readable channel, so probe results must be read off the phone. Consider
  whether the spike needs a longer default duration or an on-screen scrollback before starting,
  since these probes involve doing something to the phone *mid-run* and then reading back.
- **The Wi-Fi here has client isolation**, so `tauri ios dev` hot reload does not work. Use the
  embedded-build loop:

  ```bash
  env -u FORCE_COLOR npx tauri ios build --debug
  xcrun devicectl device install app --device <udid> \
    src-tauri/gen/apple/build/arm64/decibel-meter.ipa
  xcrun devicectl device process launch --device <udid> net.thezic.decibel-meter
  ```

The spike (`src-tauri/src/spike.rs` and friends) is kept alive for these probes and should be
**deleted when this ticket resolves**.
