# IEC 61672 A/C weighting filters: definitions and validation

Parent: [SPL Meter MVP](../map.md)
Type: research
Status: resolved
Blocked by: —

## Question

What exactly are the A- and C-weighting curves we have to implement, and how do we know
we've implemented them correctly?

- The **IEC 61672** A- and C-weighting transfer functions, including the pole/zero
  definitions they're specified from.
- The conventional implementation as **cascaded biquads**, and how the coefficients
  depend on sample rate — including what breaks at the sample rates ticket `01`/`02`
  actually give us.
- The **tolerance bands** the standard specifies, and a practical **validation method**:
  reference tones at known frequencies with expected attenuation, so we can check our
  filter rather than trusting it.

Deliverable: coefficients or their derivation, plus a validation recipe.

Note the low-frequency end matters more than usual here: dB(C) is the default and its
whole character is the low end, so a filter that's sloppy below 100 Hz is a filter that's
wrong for this app's primary mode.

Unblocked — runs in parallel with `01`.

## Context

A `/research` subagent resolved this ticket. Full findings, ~1040 lines including a
ready-to-use validation table and computed coefficient fixtures:
[`research/03-iec-weighting-filters.md`](../research/03-iec-weighting-filters.md). Also
committed on the throwaway branch `research/iec-weighting-filters`.

## Answer

**Build, don't adopt.** Cascaded biquads — 3 sections for A, 2 for C — with **f64**
coefficients *and* state, transposed direct form II, plain bilinear transform with **no
prewarping and no oversampling**, coefficients computed **at runtime** from the actual
stream sample rate (~30 flops, so the runtime cost is irrelevant). Optionally use `biquad`
0.6 as the section runner; hand-rolling it is ~15 lines and equally fine.

**Primary source secured.** The normative text was found in full: **IS 15575-1:2005**, the
Bureau of Indian Standards adoption declared identical with IEC 61672-1:2002, published
free. That yielded clauses 5.4.6–5.4.14 (weighting equations, pole-frequency derivation
from f_L=10^1.5, f_H=10^3.9, f_A=10^2.45, D=√½), **Table 2 complete** (34 frequencies ×
A/C/Z × class 1 and 2 limits), Annex A Table A.1 (expanded uncertainties, which the
standard explicitly invites you to use as a design target), and clause 3.9 eq. (2) for
L_eq. Corroborated against three reference implementations whose source was read.

Derived poles: f₁ = 20.598997057568143, f₂ = 107.65264864304627, f₃ = 737.8622307362899,
f₄ = 12194.217147998010. **f₂ and f₃ are A-only**; f₁ and f₄ are doubled in both curves.

**L_eq ordering confirmed against clause 3.9:** weighting is applied to the signal, then
energy is averaged. Not the reverse.

### The reassuring result

**C-weighting's low-frequency accuracy is not the hard part.** Below 1 kHz the digital
filter tracks the analogue design goal to **≤0.005 dB (A) and ≤0.0005 dB (C)** at every
sample rate from 16 to 192 kHz. The charting-session worry about dB(C) accuracy in the low
end was misplaced *as regards the filter* — the microphone remains the dominant error
source, but the filter is not.

### Pitfalls, all measured rather than asserted

1. **A single high-order direct-form section doesn't just lose precision — in f32 the
   6th-order A-weighting overflows to NaN.** Measured. Always cascade into biquads. Note
   that Rimell et al. publish exactly those direct-form coefficient tables; do not
   implement them literally.
2. **f32 state is usually fine and occasionally catastrophic.** ≤0.07 dB at 48 kHz, but
   **+1.7 dB with a DC offset and +3.9 dB with 5 Hz rumble at 192 kHz** — and DC bias plus
   infrasonic HVAC and handling noise is precisely what a phone mic in a venue delivers.
   f64 costs ~20 multiply-adds per sample. Take it.
3. **Near-Nyquist pole: accept the error, don't prewarp.** −3.5 dB at 12.5 kHz and −24 dB
   at 20 kHz at 44.1 kHz, all inside class-1 tolerance there (+3.0/−6.0 and +4.0/−∞).
   Prewarping would push error *down* into the 1–5 kHz band where tolerance is ~4× tighter
   and speech energy actually lives — strictly worse.
4. **There is a minimum viable sample rate: ~36.9 kHz** for class-1 limits, **39.0 kHz**
   for the design band (bisection, independently reproducing Rimell et al.'s 35 kHz). Add
   a runtime guard below ~40 kHz. See the consequence note below — this is not hypothetical
   on iOS.
5. **Test-suite trap.** The standard's table is *labelled* 10/12.5/16 Hz but *computed at*
   10/12.5893/15.8489 Hz (Table 2 footnote b). Testing at nominal frequencies produces a
   spurious 0.28 dB "failure". Also discard ≥2 s of transient — the 20.6 Hz double pole
   settles slowly.

### Consequences for the rest of the map

- **Ticket [`04`](04-weighting-architecture.md) is now nearly pre-answered, with
  measurements.** FFT-domain weighting matches biquads to ~0.02 dB on clean content, but
  **FFT-domain C over-reads by +6.1 dB at N=1024 and +4.3 dB at N=2048** when a 12 Hz
  component is present, needing N≥8192 (170 ms) to get under 0.6 dB. A rectangular window
  costs +2.4 dB on A with a 30 Hz tone. Crucially **the error direction is over-reading**,
  i.e. false alarms against the 70 dB ceiling. Recommended split: biquads for the reported
  level, FFT for the spectrogram only.
- **The ~40 kHz floor collides with iOS audio routing.** A Bluetooth headset mic route
  typically hands over 8 or 16 kHz, far below the floor — at which point the weighting
  filter is out of tolerance and the reading is not defensible. Folded into ticket
  [`11`](11-interruption-and-gap-handling.md), which already owns route changes.
- **Nothing in Rust is adoptable** as an IEC weighting implementation. `fundsp` and `dasp`
  were both assessed and rejected; `biquad` 0.6 and `sci-rs` 0.4.1 are viable as section
  runners only.
