# Calibration model, procedure, and persistence

Parent: [SPL Meter MVP](../map.md)
Type: grilling
Status: open
Blocked by: 05

**Inherited from ticket [`11`](11-interruption-and-gap-handling.md) — two constraints, and one
open question that may undermine the whole model.**

- **`AVAudioSessionModeMeasurement` is a precondition, not a refinement.** Measured on device:
  the same 440 Hz sine read **−51.7 dBFS** in `Measurement` mode and **−30.6 dBFS** in `Default`
  — 21 dB of processing gain. A stored offset only means something if the mode is fixed.
- **The offset is only valid for the input it was set on.** `11` decided a route change does *not*
  invalidate the calibration, so the procedure must say plainly that plugging in a different
  microphone silently invalidates it. Built-in versus headset mics can differ by tens of dB.
- **Open, and it needs answering before this ticket can be trusted: is the input path linear?**
  Processing worth 21 dB is usually level-dependent, and a single broadband offset can only
  correct a *linear* path. The check is cheap and needs no hardware — play the same tone at two
  levels 20 dB apart in `Measurement` mode and confirm the measured delta matches. **If it does
  not, this ticket's premise fails** and the model needs rethinking rather than adjusting. Also
  in the map's fog.

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
