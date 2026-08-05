# Interruptions and sample gaps: how do they affect measurement validity?

Parent: [SPL Meter MVP](../map.md)
Type: grilling
Status: open
Blocked by: 02

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
