# Calibration model, procedure, and persistence

Parent: [SPL Meter MVP](../map.md)
Type: grilling
Status: open — unblocked, on the frontier
Blocked by: —  (was 05, now resolved)

**Inherited from ticket [`05`](05-level-metrics-pipeline.md) — the matching gesture is now
interactive, and this ticket owns more settings than it thought.**

- **The offset is applied post-log, in Rust, to each published dB value** (`05` decision 13).
  Chosen partly for this ticket: as a pre-squaring gain, every nudge of the offset would invalidate
  the accumulated window and force a refill before the next comparison against the reference meter.
  Post-log, `10·log₁₀(Σp²/n) + c` is the identical number and `max(xᵢ + c) = max(xᵢ) + c`, so
  **nothing resets** — nudge the offset and the settled 60 s average and the historical maximum both
  move instantly. Design the gesture around that; it is a live adjustment, not a wait-and-see loop.
- **A 10 s window length exists partly for this** (`05` decision 6). Selectable lengths are
  10/30/60/120 s, and the 10 s option was kept specifically because matching a reference meter wants
  a number steadier than the instantaneous readout but settling in seconds.
- **Persistence now covers four settings, not one:** the calibration offset, the weighting mode
  (C/A/Z), the time weighting (F/S — `05` decision 7 made it selectable), and the window length. All
  four are **Rust-owned** state; the frontend issues commands rather than holding them (`05`
  decision 13, and see ticket [`08`](08-rust-frontend-boundary.md)).
- **Z mode reads through the same post-log offset**, so the bypass path really does exercise the
  whole calibration chain end-to-end, which is what `04` decision 6 promised it would be good for.

**Inherited from ticket [`11`](11-interruption-and-gap-handling.md) — two constraints, and one
open question that may undermine the whole model.**

- **`AVAudioSessionModeMeasurement` is a precondition, not a refinement.** Measured on device:
  the same 440 Hz sine read **−51.7 dBFS** in `Measurement` mode and **−30.6 dBFS** in `Default`
  — 21 dB of processing gain. A stored offset only means something if the mode is fixed.
- **The offset is only valid for the input it was set on.** `11` decided a route change does *not*
  invalidate the calibration, so the procedure must say plainly that plugging in a different
  microphone silently invalidates it. Built-in versus headset mics can differ by tens of dB.
- ~~**Open, and it needs answering before this ticket can be trusted: is the input path linear?**~~
  **Measured 2026-08-05 — it is linear, and this ticket's premise holds.** Same tone at two source
  levels 20 dB apart in `Measurement` mode read **−57.7 dBFS** and **−37.8 dBFS**: a measured delta
  of **19.9 dB against 20 dB expected**. A level-dependent path would have compressed by dB, not by
  tenths, so the 21 dB `Measurement`/`Default` difference from `11` probe 4 is a fixed gain rather
  than AGC-like processing. **A single broadband offset is defensible.**

  The tight agreement also rules out its own most likely artefact. Room noise adds energy to the
  *quiet* point, shrinking the measured delta — the same direction as compression — so both error
  sources are jointly capped at 0.1 dB. For noise to account for even that, it would have to sit
  ~16 dB below −57.7 dBFS (≈ −74 dBFS); had it been near `02`'s idle figure of −59.6 dBFS the delta
  would have come out near 24 dB, not 19.9. Whatever the floor was, it was low enough not to matter.

  **Limitation worth carrying rather than forgetting:** the test brackets −57.7 to −37.8 dBFS, and
  AGC-style processing engages near the *top* of a range, not the bottom. `02` measured an idle room
  at −59.6 rms / −33.3 peak, so a venue talk plausibly sits at or above the highest tested level —
  the untested region is exactly where limiting would live if any survived `Measurement` mode. One
  further run 20 dB higher would bracket the real operating point. Not blocking: linearity over a
  20 dB span is what this ticket needed, and Simon noted the equipment on hand is not precise enough
  to chase tenths regardless.

**Inherited from ticket [`04`](04-weighting-architecture.md) — one thing that helps this ticket.**
`04` exposed **dB(Z)** as a third mode, and its stated purpose is exactly this ticket's problem: Z
is the **bypass path**, RMS straight off the raw samples with no filter in the way. That gives the
calibration procedure — and the unrun linearity check above — a way to exercise the input path
*without* the weighting filters as a confounder. Worth using rather than reasoning around.

Also from `04`: there is **no DC blocker** in the chain, so a Z reading includes the microphone's DC
bias. Irrelevant for A and C (both are high-passes) but it is a floor on how quiet a Z measurement
can read, and worth knowing before treating a Z number as ground truth.

## Question

A single broadband offset in dB, set by matching a proper SPL meter that is present at the
venue. The details that matter:

- **Applied pre- or post-weighting?** A broadband offset is mathematically the same either
  way for a pure gain, but where it sits in the pipeline changes what's testable and what
  the stored number means.
- **What the matching gesture is.** Both meters pointed at the same steady sound; does
  Simon type the reference reading, nudge with buttons, or does the app compute the delta
  from an entered target?
- **What happens when the input device or mic changes** — a different iPhone, a headset
  plugged in, a route change mid-session. The offset is device-specific; does it silently
  become wrong?
- **Persistence.** Where the offset is stored, alongside the other settings (weighting
  choice, window length). Must survive restarts.

Known limitation, already ruled out of scope: a single broadband offset cannot correct
frequency-response error, and on a phone mic that error is worst in the low end — where
dB(C) lives. The offset makes the number honest on average, not honest per band. Make sure
the spec says so rather than implying more accuracy than exists.
