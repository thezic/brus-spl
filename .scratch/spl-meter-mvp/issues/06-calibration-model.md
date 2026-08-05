# Calibration model, procedure, and persistence

Parent: [SPL Meter MVP](../map.md)
Type: grilling
Status: open
Blocked by: 05

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
