# IEC 61672 A / C weighting filters — research

Ticket: choose an implementation strategy for dB(C) (default) and dB(A) (selectable)
weighting in the Rust DSP backend. Sample rate is not fixed; C-weighting low-frequency
accuracy is the priority.

Status: research complete. All numbers below were computed from the standard's own
defining equations and cross-checked against the standard's own tabulated values; the
computation scripts are described in [Appendix B](#appendix-b--how-the-numbers-here-were-produced)
so they can be re-derived.

---

## Recommendation

**Build, don't adopt.** No Rust crate implements IEC 61672 weighting in a form we can
depend on (see [§5](#5-existing-rust-crates)). But this is a small amount of code — the
whole coefficient derivation is ~40 lines and the runner is ~15.

Concretely:

| Decision | Choice | Why |
|---|---|---|
| Source of the filter | Our own code, from the standard's Annex E pole/zero prototype | Nothing to adopt; the derivation is short and now fully specified in [§1](#1-the-defining-transfer-functions)–[§2](#2-implementation-as-cascaded-biquads) |
| Discretisation | Plain bilinear transform, **no prewarping, no oversampling** | Verified to fit inside the class-1 design-target band at every one of the standard's 34 verification frequencies at both 44.1 and 48 kHz ([§3](#3-tolerance-bands)) |
| Structure | **Cascaded biquads** (3 sections for A, 2 for C), ordered with the pole nearest z = 1 **last** | A single 6th-order direct-form section is numerically unusable — measured below |
| Topology | Direct Form 2 Transposed (DF1 is equally fine) | Measured difference between DF1 and DF2T is < 0.005 dB; DF2T is fewer states |
| **Numeric precision** | **`f64` for coefficients and state.** Not f32. | f32 cascaded biquads are *usually* fine (≤ 0.03 dB at 48 kHz) but degrade to **+1.7 dB with a DC offset and ~4 dB with low-frequency rumble at 192 kHz**. f64 removes an entire class of concern for ~zero cost at these sample rates. Details in [§Pitfalls](#pitfalls) |
| When to compute coefficients | **At runtime from the actual stream sample rate.** | The rate is not ours to choose. Derivation is ~30 floating-point ops, done once when the audio stream starts. Precomputed tables would be a liability the first time iOS hands us an unexpected rate. |
| Where in the chain | Weighting **before** squaring/averaging | Confirmed against the standard's own definition ([§6](#6-relationship-to-l_eq)) |
| Sample-rate request | **Ask for 48 kHz**; accept anything ≥ 44.1 kHz without concern | 48 kHz roughly halves the (already-in-tolerance) high-frequency warping error vs 44.1 kHz |
| Crate to lean on | Optionally [`biquad`](https://docs.rs/biquad) 0.6 for the runner (`DirectForm2Transposed<f64>`, public `Coefficients` fields) | Tiny, `no_std`, one dependency, actively maintained. Hand-rolling DF2T is equally defensible — it is 15 lines. |

**The headline reassurance for this app:** below 1 kHz — the entire region that gives
C-weighting its character — the bilinear-transformed digital filter tracks the standard's
analogue design goal to **≤ 0.005 dB for A and ≤ 0.0005 dB for C** at every sample rate
from 16 kHz to 192 kHz. The low-frequency end is not where the difficulty is. All the
discretisation error lives above 4 kHz.

---

## 1. The defining transfer functions

### 1.1 What the standard actually specifies

IEC 61672-1 specifies the weightings as a **magnitude-vs-frequency formula** plus a
**pole/zero description**, not as a filter realisation. Quoting the normative text
(clause numbering from IEC 61672-1:2002; the 2013 edition moves this material to the
normative Annex E as equations E.1/E.6, with identical content):

> **5.4.6** The C-weighting characteristic is realized by two low-frequency poles at
> frequency f₁, two high-frequency poles at frequency f₄, and two zeros at 0 Hz. With
> these poles and zeros, the power response for the C-weighting characteristic, relative
> to the response at the reference frequency f_r of 1 kHz, will be down by D² = 1/2
> (approximately −3 dB) at f_L = 10^1,5 Hz and f_H = 10^3,9 Hz. The A-weighting
> characteristic is realized by adding two coupled first-order high-pass filters to the
> C-weighting characteristic. For each high-pass filter, the cut-off frequency is given by
> f_A = 10^2,45 Hz.
>
> **NOTE** The addition of the coupled high-pass filters to the C-weighting characteristic
> is equivalent to the addition of two zeros at 0 Hz and poles at frequencies f₂ and f₃.

— [IS 15575 (Part 1):2005 / IEC 61672-1:2002, clause 5.4.6](https://law.resource.org/pub/in/bis/S04/is.15575.1.2005.pdf), p. 14.
(IS 15575-1 is the Bureau of Indian Standards adoption, declared "identical with IEC
61672-1 (2002)", published in full and free of charge by law.resource.org. This is the
primary text used throughout this document.)

So, explicitly:

| | Zeros (at s = 0) | Poles |
|---|---|---|
| **C** | 2 | f₁ (double), f₄ (double) — **4 poles** |
| **A** | 4 | f₁ (double), **f₂ (single), f₃ (single)**, f₄ (double) — **6 poles** |

**f₂ and f₃ belong to A only.** f₁ and f₄ are shared, each doubled, in both. A is C with
two extra zeros at DC and two extra real poles.

### 1.2 The magnitude equations (clause 5.4.8, eqs. 6 and 7)

```
C(f) = 20 lg [                       f₄² f²                          ]  − C₁₀₀₀
              ─────────────────────────────────────────────────────
                          (f² + f₁²)(f² + f₄²)

A(f) = 20 lg [                       f₄² f⁴                          ]  − A₁₀₀₀
              ─────────────────────────────────────────────────────────────────
               (f² + f₁²)(f² + f₂²)^½ (f² + f₃²)^½ (f² + f₄²)

Z(f) = 0
```

`C₁₀₀₀` and `A₁₀₀₀` are "normalization constants, in decibels, representing the electrical
gain needed to provide frequency weightings of zero decibels at 1 kHz" (clause 5.4.8).
Note the sign: they are **subtracted**, and they are negative, so the effect is to add
gain.

### 1.3 The pole frequencies are *derived*, not given (clauses 5.4.9–5.4.11)

The standard does not hand you 20.6 / 107.7 / 737.9 / 12194. It gives a construction:

```
f_r = 1000 Hz
f_L = 10^1.5  Hz  = 31.622776601683793      (C is −3 dB here)
f_H = 10^3.9  Hz  = 7943.282347242815       (C is −3 dB here)
f_A = 10^2.45 Hz  = 281.83829312644537      (A's extra high-pass corner)
D   = sqrt(1/2)                             (D² = 1/2, clause 5.4.6)

                1     ⎡        f_L² f_H²                  ⎤
b   =  ─────────────  ⎢ f_r² + ────────  − D (f_L² + f_H²) ⎥        (eq. 11)
           1 − D      ⎣          f_r²                     ⎦

c   =  f_L² f_H²                                                    (eq. 12)

           ⎛ −b − sqrt(b² − 4c) ⎞^½                                 (eq.  9)
f₁  =      ⎜ ────────────────── ⎟
           ⎝          2         ⎠

           ⎛ −b + sqrt(b² − 4c) ⎞^½                                 (eq. 10)
f₄  =      ⎜ ────────────────── ⎟
           ⎝          2         ⎠

           3 − sqrt(5)                                              (eq. 13)
f₂  =      ─────────── · f_A
                2

           3 + sqrt(5)                                              (eq. 14)
f₃  =      ─────────── · f_A
                2
```

— [IS 15575-1:2005 / IEC 61672-1:2002, clauses 5.4.9–5.4.10](https://law.resource.org/pub/in/bis/S04/is.15575.1.2005.pdf), p. 15.

Evaluated at 40 significant digits (mpmath), then rounded to `f64`:

```
b  = −148699356.1712085036164543561413811993773
c  =    63095734448.0193249434360136622343864673

f₁ =    20.598997057568143   Hz
f₂ =   107.65264864304627    Hz
f₃ =   737.8622307362899     Hz
f₄ = 12194.217147998010      Hz
```

The standard's own clause 5.4.11 confirms these, rounded:

> **5.4.11** Approximate values for frequencies f₁ to f₄ in equations (6) and (7) are:
> f₁ = 20,60 Hz, f₂ = 107,7 Hz, f₃ = 737,9 Hz, and f₄ = 12194 Hz. Normalization constants
> C₁₀₀₀ and A₁₀₀₀, rounded to the nearest 0,001 dB, are −0,062 dB and −2,000 dB,
> respectively.

Computed exactly, the normalisation constants are

```
C₁₀₀₀ = −0.06190185672430539 dB
A₁₀₀₀ = −1.9996555356672427  dB
```

Independent corroboration of the same constants, from a source whose code we can read:
[`endolith/waveform_analysis`](https://github.com/endolith/waveform_analysis/blob/master/waveform_analysis/weighting_filters/ABC_weighting.py)
hard-codes `20.598997057568145`, `12194.21714799801`, `107.65264864304628`,
`737.8622307362899` and includes a sympy `_derive_coefficients()` that reproduces exactly
the clause 5.4.9/5.4.10 algebra above. [`lasprs`](https://code.ascee.nl/ascee/lasprs)
(`src/filter/zpkmodel.rs`, `freqWeightingFilter`) implements the same derivation at runtime
in Rust. Three independent implementations agree to all printed digits.

**Use the exact constants, not the rounded ones — but it barely matters.** Substituting
the clause-5.4.11 rounded values (20.60 / 107.7 / 737.9 / 12194.0) with rounded
normalisation (−2.000 / −0.062) changes the response by **at most 0.005 dB for A and
0.0006 dB for C** anywhere from 10 Hz to 20 kHz. There is no reason not to use the exact
values, but this is not a source of error worth worrying about.

### 1.4 s-domain transfer functions

The magnitude formulas above correspond to these transfer functions (this form is given
in ANSI S1.42, and is what MathWorks' `weightingFilter` documents itself as implementing —
["direct implementation of the filter's transfer function based on poles and zeros
specified in the ANSI S1.42 standard"](https://www.mathworks.com/help/audio/ug/audio-weighting-filters.html)):

```
                     ω₄² · s²
H_C(s) = G_C · ───────────────────────
                (s + ω₁)² (s + ω₄)²

                              ω₄² · s⁴
H_A(s) = G_A · ────────────────────────────────────────────
                (s + ω₁)² (s + ω₂) (s + ω₃) (s + ω₄)²

ωₙ = 2π fₙ ,   G_C = 10^(0.062/20) = 10^(−C₁₀₀₀/20) ,   G_A = 10^(2/20) = 10^(−A₁₀₀₀/20)
```

— reproduced with these exact symbols in
[Virtins Technology, *Frequency Weightings for Sound and Vibration Perceived by Humans*, 2024, p. 5](https://www.virtins.com/doc/Frequency-Weightings-for-Sound-and-Vibration-Perceived-by-Humans-using-Multi-Instrument.pdf),
which also reproduces the full clause-5.4.9/5.4.10 derivation verbatim.

In practice, **don't use G_A / G_C at all** — see [§2.3](#23-normalise-the-digital-cascade-not-the-analogue-prototype).

### 1.5 Sanity check against the standard's own table

The standard tabulates A, C and Z at the 34 nominal one-third-octave frequencies from
10 Hz to 20 kHz, with this footnote:

> **b)** C and A frequency weightings were calculated by use of equations (6) and (7) with
> frequency f computed from f = (f_r)[10^(0,1(n−30))] with f_r = 1 kHz and n an integer
> between 10 and 43. The results were rounded to a tenth of a decibel.

— [IS 15575-1:2005 / IEC 61672-1:2002, Table 2, footnote b](https://law.resource.org/pub/in/bis/S04/is.15575.1.2005.pdf), p. 13.

**This footnote is load-bearing for our test suite.** The table is labelled with *nominal*
frequencies (10, 12.5, 16, …) but the values were computed at the *exact* frequencies
(10, 12.5893, 15.8489, …). Evaluating the equations at 12.5 Hz instead of 12.5893 Hz gives
−63.58 dB where the table says −63.4 — a 0.18 dB discrepancy that looks like a bug and
isn't. At 16 Hz the discrepancy is 0.28 dB.

Evaluating at the exact frequencies, my computed values match all 68 tabulated A and C
values to within ±0.05 dB — i.e. exactly the rounding to one decimal place. The derivation
above is confirmed correct.

---

## 2. Implementation as cascaded biquads

### 2.1 Section assignment

All six A-weighting poles and all four C-weighting poles are **real and negative**. There
are no complex conjugate pairs anywhere in either filter. That means the biquad
factorisation needs no complex arithmetic at all — pair up real poles and read off real
coefficients.

Recommended factorisation (analogue, before discretisation):

**C-weighting — 2 sections:**

```
S_lp4 :  ω₄² / (s + ω₄)²          low-pass, double pole at f₄ = 12194.217 Hz
S_hp1 :  s²  / (s + ω₁)²          high-pass, double pole at f₁ = 20.599 Hz
```

**A-weighting — 3 sections:**

```
S_lp4 :  ω₄² / (s + ω₄)²          low-pass, double pole at f₄
S_hp23:  s²  / ((s + ω₂)(s + ω₃)) high-pass, poles at f₂ = 107.653, f₃ = 737.862 Hz
S_hp1 :  s²  / (s + ω₁)²          high-pass, double pole at f₁
```

**Cascade order matters for f32 and is free, so just do it: put `S_hp1` last.** This is
the convention SciPy's `zpk2sos` uses (poles closest to the unit circle in the final
section) and it measured marginally better than the reverse in my f32 tests. With f64 the
ordering is irrelevant.

### 2.2 Bilinear transform, per section

For a second-order analogue section `N(s)/D(s)` with

```
N(s) = n₂ s² + n₁ s + n₀
D(s) = d₂ s² + d₁ s + d₀
```

substitute `s = 2·f_s·(1 − z⁻¹)/(1 + z⁻¹)` and clear the `(1 + z⁻¹)²` factor. Writing
`c = 2·f_s`:

```
b0' = n₂c² + n₁c + n₀        a0' = d₂c² + d₁c + d₀
b1' = 2n₀ − 2n₂c²            a1' = 2d₀ − 2d₂c²
b2' = n₂c² − n₁c + n₀        a2' = d₂c² − d₁c + d₀

b0 = b0'/a0'   b1 = b1'/a0'   b2 = b2'/a0'   a1 = a1'/a0'   a2 = a2'/a0'   (a0 ≡ 1)
```

The three section types instantiate as:

| Section | n₂, n₁, n₀ | d₂, d₁, d₀ |
|---|---|---|
| `S_lp4`  | `0, 0, ω₄²` | `1, 2ω₄, ω₄²` |
| `S_hp1`  | `1, 0, 0`   | `1, 2ω₁, ω₁²` |
| `S_hp23` | `1, 0, 0`   | `1, ω₂+ω₃, ω₂ω₃` |

That's the whole derivation. It depends on the sample rate **only through `c = 2 f_s`** —
so it is a handful of multiplies, computable at stream start for any rate.

I verified this closed form against SciPy's
`bilinear_zpk` + `zpk2sos` (the same routines the two Python reference implementations use)
at 16, 44.1, 48 and 192 kHz: **maximum magnitude difference 7 × 10⁻⁸ dB** over 5 Hz to
Nyquist, for both A and C. The recipe above and the reference implementations are the same
filter.

### 2.3 Normalise the digital cascade, not the analogue prototype

Set the section gains to 1 (i.e. drop `G_A`/`G_C` entirely), evaluate the *digital* cascade
at 1 kHz, and divide one section's `b` coefficients by that magnitude:

```
g = |H_digital(e^{j2π·1000/f_s})|      then  b0,b1,b2 of section 0  /=  g
```

Why not use the standard's `A₁₀₀₀`/`C₁₀₀₀` and normalise the analogue prototype? Because
the bilinear transform shifts the response at 1 kHz very slightly, so an
analogue-normalised filter reads **+0.004 dB at 1 kHz** rather than 0.000 dB. (`lasprs`
and `endolith` both do it this way and both carry that offset.) Two reasons this is worth
one line of code to fix:

- 1 kHz is *the* calibration frequency. An acoustic calibrator produces 1 kHz; any level
  calibration we do anchors there. A systematic offset at the anchor point propagates to
  every reading at every frequency.
- Clause 5.4.14 requires that at 1 kHz the difference between a C-weighted and an
  A-weighted indication not exceed ±0.4 dB. Renormalising both filters digitally makes
  that difference exactly zero by construction, which turns clause 5.4.14 into a free
  unit test.

### 2.4 Computed coefficients (for reference / test fixtures)

Produced by the recipe in §2.2 + §2.3, sections in cascade order, `a0 = 1`:

**f_s = 48000 Hz — C-weighting**

```
S_lp4 : b = [ 0.19842466233976611,  0.39684932467953221,  0.19842466233976611]
        a = [ 1, -0.22455844607604505,  0.012606623926022036]
S_hp1 : b = [ 0.99730904074427118, -1.9946180814885424,   0.99730904074427118]
        a = [ 1, -1.9946144559779906,   0.99462170699909414]
```

**f_s = 48000 Hz — A-weighting**

```
S_lp4 : b = [ 0.24788920311064716,  0.49577840622129432,  0.24788920311064716]
        a = [ 1, -0.22455844607604505,  0.012606623926022036]
S_hp23: b = [ 0.94725756599442701, -1.894515131988854,    0.94725756599442701]
        a = [ 1, -1.8938704948105136,   0.89515976916719464]
S_hp1 : b = [ 0.99730904074427118, -1.9946180814885424,   0.99730904074427118]
        a = [ 1, -1.9946144559779906,   0.99462170699909414]
```

**f_s = 44100 Hz — C-weighting**

```
S_lp4 : b = [ 0.21765058915958915,  0.43530117831917831,  0.21765058915958915]
        a = [ 1, -0.14053607034389975,  0.0049375967669263937]
S_hp1 : b = [ 0.99707158766954873, -1.9941431753390975,   0.99707158766954873]
        a = [ 1, -1.9941388812499723,   0.99414746942822274]
```

**f_s = 44100 Hz — A-weighting**

```
S_lp4 : b = [ 0.27188178750108627,  0.54376357500217254,  0.27188178750108627]
        a = [ 1, -0.14053607034389975,  0.0049375967669263937]
S_hp23: b = [ 0.94283067235470297, -1.8856613447094059,   0.94283067235470297]
        a = [ 1, -1.8849012175245061,   0.88642147189430565]
S_hp1 : b = [ 0.99707158766954873, -1.9941431753390975,   0.99707158766954873]
        a = [ 1, -1.9941388812499723,   0.99414746942822274]
```

These are for **cross-checking a runtime implementation**, not for hardcoding. The gain
distribution across sections is arbitrary (only the product matters), so an
implementation that puts the normalisation elsewhere will produce different numbers with
an identical response — compare responses, not coefficients.

### 2.5 What breaks at low sample rates / high frequencies

The bilinear transform is an **exact frequency-warped mapping**: the digital filter's
response at frequency `f` equals the analogue prototype's response at

```
Ω(f) = (f_s/π) · tan(π f / f_s)
```

So the error is fully predictable in closed form: `err(f) = H_analogue(Ω(f)) − H_analogue(f)`.
Since Ω(f) > f always and both weightings roll off at high frequency, **the error is always
in the direction of too much attenuation**, growing without bound at Nyquist. (Equivalently:
A has 4 zeros and 6 poles, so the bilinear transform plants 2 extra zeros at z = −1,
forcing a null at Nyquist. Same fact, different lens.)

Measured error, digital minus analogue design goal, at the standard's verification
frequencies:

| f (exact, Hz) | 44.1 kHz | 48 kHz | 96 kHz |
|---:|---:|---:|---:|
| ≤ 1000 | −0.005 (A) / 0.000 (C) | −0.004 (A) / 0.000 (C) | −0.001 (A) / 0.000 (C) |
| 1995 | −0.001 | −0.001 | −0.000 |
| 3162 | −0.016 | −0.014 | −0.003 |
| 3981 | −0.044 | −0.037 | −0.009 |
| 5012 | −0.111 | −0.093 | −0.022 |
| 6310 | −0.272 | −0.225 | −0.053 |
| 7943 | −0.645 | −0.530 | −0.120 |
| 10000 | −1.501 | −1.216 | −0.261 |
| 12589 | −3.459 | −2.738 | −0.543 |
| 15849 | −8.214 | −6.214 | −1.081 |
| 19953 | −24.176 | −15.668 | −2.082 |

(A and C errors agree to 0.002 dB throughout — unsurprising, since the error is entirely
produced by the shared f₄ double pole.)

**Two things follow.**

1. **This is acceptable.** The standard's tolerance limits widen dramatically at high
   frequency precisely because analogue weighting networks have always had this problem.
   Class 1 permits +3.5 / −17.0 dB at 16 kHz and +4.0 / −∞ at 20 kHz. Checked against the
   full table ([§3](#3-tolerance-bands)), the plain-bilinear filter passes at every
   testable frequency at both 44.1 and 48 kHz — with room to spare.

2. **Below ~35 kHz it stops being acceptable.** I bisected the minimum sample rate at
   which the plain-bilinear filter satisfies the limits at every verification frequency
   below its own Nyquist:

   | Target band | Minimum f_s (my computation) |
   |---|---|
   | Class 1 tolerance limits (Table 2) | **36 951 Hz** |
   | Class 1 minus max expanded uncertainty ("design target", see §3) | **39 012 Hz** |
   | Class 2 tolerance limits | **21 362 Hz** |

   This independently reproduces the only peer-reviewed source I found on the question:
   Rimell, Mansfield & Paddan, *"Design of digital filters for frequency weightings (A and
   C) required for risk assessments of workers exposed to noise"*, **Industrial Health**
   53(1):21–27, 2014 —
   [PMC4331191](https://pmc.ncbi.nlm.nih.gov/articles/PMC4331191/) — whose Table 3 gives
   the minimum sampling frequency to meet the full standard tolerances as **35 kHz for IEC
   61672-1 class 1** and **20 kHz for class 2** (and 71–72 kHz for ANSI S1.43 type 0).
   Two independent computations, agreeing to the rounding.

**What reference implementations do about it: nothing.** They accept the error.

- `endolith/waveform_analysis` docstring: *"fs = 48000 yields a class 1-compliant filter"*
  and *"Since this uses the bilinear transform, frequency response around fs/2 will be
  inaccurate at lower sampling rates"*, with an open TODO noting that meeting ANSI
  **type 0** limits needs upsampling to ≥ 260 kHz.
- Rimell et al. mention prewarping as available (`ω' = (2/T)·tan(ωT/2)`) but their
  conclusion is a minimum sample rate, not a prewarping scheme.
- `python-acoustics` never discretises the weighting filter at all — it returns the
  analogue polynomial and leaves it to the caller.

**Why prewarping does not help us.** Prewarping shifts the frequency axis so that the
digital response matches the analogue response *exactly at one chosen frequency*. It does
not flatten the warping elsewhere; it redistributes it. Prewarping at f₄ = 12194 Hz would
pull the 8–16 kHz region into better agreement at the cost of pushing error downward into
the 1–5 kHz region, where the tolerance band is *tighter* (±1.6 dB at 4 kHz) and where
speech actually has energy. That is a strictly bad trade for this app. **Do not prewarp.**

**Why oversampling is not worth it either.** 2× oversampling (96 kHz) reduces the 16 kHz
error from 6.2 dB to 1.1 dB. But (a) we are already inside tolerance without it, (b) it
costs a polyphase resampler plus 2× the filter work on a phone, and (c) we are measuring
spoken word against a 70 dB ceiling — the energy above 8 kHz in speech is far below the
level where a few dB of extra attenuation moves the broadband number. If a future ticket
ever wants genuine 16–20 kHz fidelity, oversample then.

---

## 3. Tolerance bands

Reproduced in full from
[IS 15575 (Part 1):2005 / IEC 61672-1:2002, Table 2](https://law.resource.org/pub/in/bis/S04/is.15575.1.2005.pdf),
p. 13 — *"Frequency weightings and tolerance limits including maximum expanded uncertainty
of measurement"*. (In IEC 61672-1:2013 Ed. 2 this becomes **Table 3**, *"Frequency
weightings and acceptance limits"*, on p. 22; see the caveat below.)

| Nominal f (Hz) | A (dB) | C (dB) | Z (dB) | Class 1 tolerance | Class 2 tolerance |
|---:|---:|---:|---:|:---|:---|
| 10 | −70.4 | −14.3 | 0.0 | +3.5; −∞ | +5.5; −∞ |
| 12.5 | −63.4 | −11.2 | 0.0 | +3.0; −∞ | +5.5; −∞ |
| 16 | −56.7 | −8.5 | 0.0 | +2.5; −4.5 | +5.5; −∞ |
| 20 | −50.5 | −6.2 | 0.0 | ±2.5 | ±3.5 |
| 25 | −44.7 | −4.4 | 0.0 | +2.5; −2.0 | ±3.5 |
| 31.5 | −39.4 | −3.0 | 0.0 | ±2.0 | ±3.5 |
| 40 | −34.6 | −2.0 | 0.0 | ±1.5 | ±2.5 |
| 50 | −30.2 | −1.3 | 0.0 | ±1.5 | ±2.5 |
| 63 | −26.2 | −0.8 | 0.0 | ±1.5 | ±2.5 |
| 80 | −22.5 | −0.5 | 0.0 | ±1.5 | ±2.5 |
| 100 | −19.1 | −0.3 | 0.0 | ±1.5 | ±2.0 |
| 125 | −16.1 | −0.2 | 0.0 | ±1.5 | ±2.0 |
| 160 | −13.4 | −0.1 | 0.0 | ±1.5 | ±2.0 |
| 200 | −10.9 | 0.0 | 0.0 | ±1.5 | ±2.0 |
| 250 | −8.6 | 0.0 | 0.0 | ±1.4 | ±1.9 |
| 315 | −6.6 | 0.0 | 0.0 | ±1.4 | ±1.9 |
| 400 | −4.8 | 0.0 | 0.0 | ±1.4 | ±1.9 |
| 500 | −3.2 | 0.0 | 0.0 | ±1.4 | ±1.9 |
| 630 | −1.9 | 0.0 | 0.0 | ±1.4 | ±1.9 |
| 800 | −0.8 | 0.0 | 0.0 | ±1.4 | ±1.9 |
| **1000** | **0** | **0** | **0** | **±1.1** | **±1.4** |
| 1250 | +0.6 | 0.0 | 0.0 | ±1.4 | ±1.9 |
| 1600 | +1.0 | −0.1 | 0.0 | ±1.6 | ±2.6 |
| 2000 | +1.2 | −0.2 | 0.0 | ±1.6 | ±2.6 |
| 2500 | +1.3 | −0.3 | 0.0 | ±1.6 | ±3.1 |
| 3150 | +1.2 | −0.5 | 0.0 | ±1.6 | ±3.1 |
| 4000 | +1.0 | −0.8 | 0.0 | ±1.6 | ±3.6 |
| 5000 | +0.5 | −1.3 | 0.0 | ±2.1 | ±4.1 |
| 6300 | −0.1 | −2.0 | 0.0 | +2.1; −2.6 | ±5.1 |
| 8000 | −1.1 | −3.0 | 0.0 | +2.1; −3.1 | ±5.6 |
| 10000 | −2.5 | −4.4 | 0.0 | +2.6; −3.6 | +5.6; −∞ |
| 12500 | −4.3 | −6.2 | 0.0 | +3.0; −6.0 | +6.0; −∞ |
| 16000 | −6.6 | −8.5 | 0.0 | +3.5; −17.0 | +6.0; −∞ |
| 20000 | −9.3 | −11.2 | 0.0 | +4.0; −∞ | +6.0; −∞ |

Note also clause 5.4.5: *"For frequencies between two consecutive frequencies in table 2,
frequency weightings C or A shall be computed from equation (6) or (7) and rounded to a
tenth of a decibel. The applicable tolerance limits then are the wider of the limits given
in table 2 for the two consecutive frequencies."*

### 3.1 The right band to hold our filter to

These limits are **whole-instrument** limits and they **include measurement uncertainty**.
Annex A tells us how to convert them into a design target — and explicitly invites it:

> **A.2** The tolerance limits in this standard include the associated expanded
> uncertainties of measurement calculated for a coverage factor of 2 corresponding to a
> level of confidence of approximately 95 %. … **Manufacturers of sound level meters may
> calculate the tolerance limits available for design and manufacturing by subtracting the
> maximum permitted expanded uncertainties of measurement from the appropriate tolerance
> limits** given in this Part 1 of IEC 61672.

Table A.1, frequency weightings A, C, Z, FLAT:

| Frequency range | Max expanded uncertainty |
|---|---:|
| 10 Hz to 200 Hz | 0.5 dB |
| > 200 Hz to 1.25 kHz | 0.4 dB |
| > 1.25 kHz to 10 kHz | 0.6 dB |
| > 10 kHz to 20 kHz | 1.0 dB |

— [IS 15575-1:2005 / IEC 61672-1:2002, Annex A, Table A.1](https://law.resource.org/pub/in/bis/S04/is.15575.1.2005.pdf), p. 34.

**So the correctness target for our filter is: class-1 tolerance limit minus the Table A.1
uncertainty at that frequency.** That is the "design band" column in §4. It is the same
quantity IEC 61672-1:2013 Ed. 2 renamed *acceptance limits*.

⚠️ **Edition caveat.** The table above is Ed. 1 (2002). Ed. 2 (2013) restructured this:
its Table 3 states *acceptance limits* (uncertainty already deducted) and moves the
maximum permitted uncertainties to a normative Annex B. I could not obtain Ed. 2's Table 3
from a primary source — the IEC and ANSI previews stop before p. 22, and Ed. 2 is
paywalled. **The Ed. 2 numbers may differ slightly from `Ed.1 limit − Table A.1
uncertainty`.** Since we are explicitly not claiming conformance, this doesn't block
anything, but do not put a class number in the UI on the strength of this document.

Independent partial corroboration of the Ed. 1 figures reproduced above, from a Class 1
SLM manufacturer:
[Cirrus Research](https://cirrusresearch.com/whats-the-difference-between-a-class-1-and-class-2-sound-level-meter/)
quotes 1 kHz ±1.1 / ±1.4, 20 Hz ±2.5 / ±3.5, 16 Hz +2.5/−4.5 and +5.5/−∞, 10 kHz
+2.6/−3.6 and +5.6/−∞, 16 kHz +3.5/−17.0 and +6.0/−∞. All five agree exactly.

---

## 4. Validation recipe

### 4.1 How to run the test

1. **Frequencies: use the exact one-third-octave frequencies, not the nominal labels.**
   `f(n) = 1000 · 10^(0.1(n − 30))` for `n = 10 … 43` (standard Table 2 footnote b).
   Testing at 12.5 Hz instead of 12.5893 Hz produces a spurious 0.18 dB "failure".
2. **Amplitude is irrelevant** — the filter is linear and (in f64) has no level-dependent
   behaviour. Use a full-scale sine, amplitude 1.0. A unit-amplitude sine has RMS
   `1/√2`, i.e. −3.0103 dB, so the expected output RMS in dB is
   `−3.0103 + W(f)` where `W(f)` is the table value.
   Do also run one test at amplitude 1e-3 to catch precision regressions.
3. **Discard the transient.** The f₁ double pole has a time constant of roughly
   `1/(2π·20.6) ≈ 7.7 ms`, but the settling to 0.001 dB takes far longer. Feed at
   least 3 s of signal and measure the RMS of the last 2 s. (This is the single most
   common cause of a false failure at 10–20 Hz.)
4. **Two levels of assertion**, so a real regression is distinguishable from a tolerance
   question:
   - **Tight:** digital response vs the *analogue* prototype value in the
     "A exact"/"C exact" columns, tolerance **±0.01 dB below 4 kHz** and per the table
     above 4 kHz. This is the actual regression test.
   - **Loose:** digital response vs the *published table* value, within the design band.
     This is the standards-conformance-shaped test.
5. **Also assert:** `|H(1000 Hz)| == 0.000 dB` to within 1e-9 dB for both A and C (this is
   what §2.3 buys), and Z-weighting returns the input unchanged.

### 4.2 The table

`f_exact` in Hz. `A exact` / `C exact` are the analogue design-goal values from equations
(6)/(7) at `f_exact`, to 3 decimals — **these are the assertion targets**. `A tab` /
`C tab` are the standard's published one-decimal values. `design band` is class-1
tolerance minus Table A.1 uncertainty. The last four columns are the measured
plain-bilinear digital deviation from the analogue design goal at 44.1 and 48 kHz.

| f_nom | f_exact | A exact | A tab | C exact | C tab | class 1 | **design band** | class 2 | A@44.1k | A@48k | C@44.1k | C@48k |
|---:|---:|---:|---:|---:|---:|:---|:---|:---|---:|---:|---:|---:|
| 10 | 10.0000 | −70.430 | −70.4 | −14.330 | −14.3 | +3.5/−∞ | +3.0/−∞ | +5.5/−∞ | −0.005 | −0.004 | 0.000 | 0.000 |
| 12.5 | 12.5893 | −63.371 | −63.4 | −11.249 | −11.2 | +3.0/−∞ | +2.5/−∞ | +5.5/−∞ | −0.005 | −0.004 | 0.000 | 0.000 |
| 16 | 15.8489 | −56.688 | −56.7 | −8.531 | −8.5 | +2.5/−4.5 | +2.0/−4.0 | +5.5/−∞ | −0.005 | −0.004 | 0.000 | 0.000 |
| 20 | 19.9526 | −50.452 | −50.5 | −6.240 | −6.2 | ±2.5 | +2.0/−2.0 | ±3.5 | −0.005 | −0.004 | 0.000 | 0.000 |
| 25 | 25.1189 | −44.703 | −44.7 | −4.405 | −4.4 | +2.5/−2.0 | +2.0/−1.5 | ±3.5 | −0.005 | −0.004 | 0.000 | 0.000 |
| 31.5 | 31.6228 | −39.440 | −39.4 | −3.010 | −3.0 | ±2.0 | +1.5/−1.5 | ±3.5 | −0.005 | −0.004 | 0.000 | 0.000 |
| 40 | 39.8107 | −34.630 | −34.6 | −1.999 | −2.0 | ±1.5 | +1.0/−1.0 | ±2.5 | −0.005 | −0.004 | 0.000 | 0.000 |
| 50 | 50.1187 | −30.228 | −30.2 | −1.294 | −1.3 | ±1.5 | +1.0/−1.0 | ±2.5 | −0.005 | −0.004 | 0.000 | 0.000 |
| 63 | 63.0957 | −26.194 | −26.2 | −0.818 | −0.8 | ±1.5 | +1.0/−1.0 | ±2.5 | −0.005 | −0.004 | 0.000 | 0.000 |
| 80 | 79.4328 | −22.504 | −22.5 | −0.504 | −0.5 | ±1.5 | +1.0/−1.0 | ±2.5 | −0.005 | −0.004 | 0.000 | 0.000 |
| 100 | 100.0000 | −19.143 | −19.1 | −0.300 | −0.3 | ±1.5 | +1.0/−1.0 | ±2.0 | −0.005 | −0.004 | 0.000 | 0.000 |
| 125 | 125.8925 | −16.098 | −16.1 | −0.169 | −0.2 | ±1.5 | +1.0/−1.0 | ±2.0 | −0.005 | −0.004 | 0.000 | 0.000 |
| 160 | 158.4893 | −13.350 | −13.4 | −0.085 | −0.1 | ±1.5 | +1.0/−1.0 | ±2.0 | −0.005 | −0.004 | 0.000 | 0.000 |
| 200 | 199.5262 | −10.870 | −10.9 | −0.033 | 0.0 | ±1.5 | +1.0/−1.0 | ±2.0 | −0.004 | −0.004 | 0.000 | 0.000 |
| 250 | 251.1886 | −8.630 | −8.6 | −0.000 | 0.0 | ±1.4 | +1.0/−1.0 | ±1.9 | −0.004 | −0.004 | 0.000 | 0.000 |
| 315 | 316.2278 | −6.611 | −6.6 | +0.019 | 0.0 | ±1.4 | +1.0/−1.0 | ±1.9 | −0.004 | −0.003 | 0.000 | 0.000 |
| 400 | 398.1072 | −4.808 | −4.8 | +0.029 | 0.0 | ±1.4 | +1.0/−1.0 | ±1.9 | −0.003 | −0.003 | 0.000 | 0.000 |
| 500 | 501.1872 | −3.233 | −3.2 | +0.033 | 0.0 | ±1.4 | +1.0/−1.0 | ±1.9 | −0.002 | −0.002 | 0.000 | 0.000 |
| 630 | 630.9573 | −1.900 | −1.9 | +0.029 | 0.0 | ±1.4 | +1.0/−1.0 | ±1.9 | −0.002 | −0.001 | 0.000 | 0.000 |
| 800 | 794.3282 | −0.824 | −0.8 | +0.019 | 0.0 | ±1.4 | +1.0/−1.0 | ±1.9 | −0.001 | −0.001 | 0.000 | 0.000 |
| **1000** | **1000.0000** | **0.000** | **0.0** | **0.000** | **0.0** | ±1.1 | +0.7/−0.7 | ±1.4 | −0.000 | 0.000 | 0.000 | −0.000 |
| 1250 | 1258.9254 | +0.591 | +0.6 | −0.033 | 0.0 | ±1.4 | +1.0/−1.0 | ±1.9 | 0.000 | 0.000 | −0.000 | −0.000 |
| 1600 | 1584.8932 | +0.981 | +1.0 | −0.085 | −0.1 | ±1.6 | +1.0/−1.0 | ±2.6 | 0.000 | 0.000 | −0.001 | −0.001 |
| 2000 | 1995.2623 | +1.200 | +1.2 | −0.169 | −0.2 | ±1.6 | +1.0/−1.0 | ±2.6 | −0.001 | −0.001 | −0.003 | −0.002 |
| 2500 | 2511.8864 | +1.271 | +1.3 | −0.300 | −0.3 | ±1.6 | +1.0/−1.0 | ±3.1 | −0.005 | −0.004 | −0.007 | −0.006 |
| 3150 | 3162.2777 | +1.199 | +1.2 | −0.504 | −0.5 | ±1.6 | +1.0/−1.0 | ±3.1 | −0.016 | −0.014 | −0.019 | −0.016 |
| 4000 | 3981.0717 | +0.970 | +1.0 | −0.818 | −0.8 | ±1.6 | +1.0/−1.0 | ±3.6 | −0.044 | −0.037 | −0.047 | −0.039 |
| 5000 | 5011.8723 | +0.549 | +0.5 | −1.294 | −1.3 | ±2.1 | +1.5/−1.5 | ±4.1 | −0.111 | −0.093 | −0.114 | −0.095 |
| 6300 | 6309.5734 | −0.121 | −0.1 | −1.999 | −2.0 | +2.1/−2.6 | +1.5/−2.0 | ±5.1 | −0.272 | −0.225 | −0.274 | −0.227 |
| 8000 | 7943.2823 | −1.111 | −1.1 | −3.010 | −3.0 | +2.1/−3.1 | +1.5/−2.5 | ±5.6 | −0.645 | −0.530 | −0.647 | −0.532 |
| 10000 | 10000.0000 | −2.492 | −2.5 | −4.405 | −4.4 | +2.6/−3.6 | +2.0/−3.0 | +5.6/−∞ | −1.501 | −1.216 | −1.503 | −1.218 |
| 12500 | 12589.2541 | −4.318 | −4.3 | −6.240 | −6.2 | +3.0/−6.0 | +2.0/−5.0 | +6.0/−∞ | −3.459 | −2.738 | −3.461 | −2.740 |
| 16000 | 15848.9319 | −6.603 | −6.6 | −8.531 | −8.5 | +3.5/−17.0 | +2.5/−16.0 | +6.0/−∞ | −8.214 | −6.214 | −8.216 | −6.215 |
| 20000 | 19952.6231 | −9.317 | −9.3 | −11.249 | −11.2 | +4.0/−∞ | +3.0/−∞ | +6.0/−∞ | −24.176 | −15.668 | −24.177 | −15.668 |

**Every row passes the design band at both 44.1 and 48 kHz, for both A and C.**

### 4.3 The low-frequency rows are the ones that matter

Since C-weighting is the default mode, these are the rows to watch, expressed as an
end-to-end assertion for a unit-amplitude sine (expected RMS in dB relative to full scale
= `−3.0103 + W`):

| f_exact (Hz) | C weighting | expected RMS (dBFS) | A weighting | expected RMS (dBFS) |
|---:|---:|---:|---:|---:|
| 10.0000 | −14.330 | −17.340 | −70.430 | −73.440 |
| 12.5893 | −11.249 | −14.259 | −63.371 | −66.381 |
| 15.8489 | −8.531 | −11.541 | −56.688 | −59.698 |
| 19.9526 | −6.240 | −9.250 | −50.452 | −53.462 |
| 31.6228 | −3.010 | −6.020 | −39.440 | −42.450 |
| 63.0957 | −0.818 | −3.828 | −26.194 | −29.204 |
| 1000.0000 | 0.000 | −3.010 | 0.000 | −3.010 |

Note the 31.6228 Hz row is a **design invariant, not a coincidence**: clause 5.4.6
specifies C to be down by exactly D² = 1/2 at f_L = 10^1.5 Hz. `−3.010 dB` there is a
direct check that f₁ and f₄ were derived correctly. Same at f_H = 10^3.9 = 7943.282 Hz
(C = −3.010 dB). **Put those two in the test suite with a tight tolerance** — they will
catch a transposed constant that a sweep comparison might smear over.

### 4.4 An extra test worth having

Clause 5.4.14: at 1 kHz, `|L_C − L_A| ≤ 0.4 dB`. With digital renormalisation this should
be `0.000` exactly, so assert `< 1e-9`. Cheap, and it catches a normalisation mistake in
either filter.

---

## 5. Existing Rust crates

Findings from reading source, not documentation blurbs.

### 5.1 Nothing to adopt

| Crate | Version / last release | What it actually is | Verdict |
|---|---|---|---|
| **`lasprs`** | 0.9.1, 2025-07 | The real thing: derives f₁–f₄ from clause 5.4.9/5.4.10 at runtime, bilinear-transforms per section, `f64` by default. Also has an `SLM` with Leq/Lmax/Lpk and F/S/I time weighting, and IEC 61260 octave bands. | **Read it, don't depend on it** — see below |
| `lookas` | 1.9.0, 2026-05 | `a_weighting(hz) -> f32`: closed-form magnitude only, rounded poles, hardcoded normalisation, clamps input to ≥ 10 Hz. Applied as an FFT-bin gain. Not a filter. | avoid |
| `autoeq-iir` | 0.2.21, 2025-11 | Same closed-form magnitude, `+2.0` hardcoded normalisation, and the function is **private** (not `pub`). | avoid |
| `use-acoustics` | 0.1.0, 2026-05 | Despite the name: SPL/dB conversion helpers only. No weighting, no filters. | avoid |
| `bs1770`, `ebur128-stream`, `oximedia-metering` | — | ITU-R BS.1770 **K-weighting**. A different curve for a different purpose (broadcast loudness). Do not confuse with C. | not applicable |
| `sonogram`, `nsrt`, `pa-spl` | — | Spectrograph; serial driver for NSRT_mk4 hardware; I2C driver for a PCB Artists module. | not applicable |

`a_weighting`, `weighting`, `spl-meter` and `acoustic` **do not exist** on crates.io.

**Why `lasprs` can't be a dependency:** 0.9.1 does not compile in any usable feature
combination. `default-features = false, features = ["f64"]` fails because `daq/` isn't
gated on `cpal-api`; adding `cpal-api` still fails on `daq/streamcmd.rs` importing a `Flt`
that `daq/mod.rs` never re-exports; the only configuration that resolves needs the
`record` feature, which drags in `hdf5-sys` (build script panics without system HDF5) and
`pyo3` with `extension-module` — wrong for a Tauri binary. It is MIT OR Apache-2.0, so
reading it and porting is clean. Two caveats found by reading it: it uses **Direct Form 2**
(the numerically worst of DF1/DF2/DF2T), and it normalises the **analogue** prototype,
leaving a measured +0.0044 dB offset at 1 kHz (see §2.3).

### 5.2 `biquad` 0.6 — worth building on

<https://docs.rs/biquad> · <https://github.com/korken89/biquad-rs>

- 0.6.0 released 2026-03-22, last commit same day, **0 open issues / 0 open PRs**.
  Edition 2024, `#![no_std]`, single dependency (`num-traits`).
- Doubly generic: `Biquad<C, T = C>` — so `DirectForm2Transposed<f64>` is available, and
  f64 coefficients over f32 samples is also expressible.
- **Arbitrary coefficients: yes.** `Coefficients<C> { pub a1, pub a2, pub b0, pub b1, pub b2 }`
  — all fields public, struct-literal construction needs no trait bound, `a0` implicit 1.
  We never touch its `Type::LowPass` designer.
- Offers both `DirectForm1` and `DirectForm2Transposed`; its own docs recommend DF2T for
  static filters ("least complexity, best numerical stability"), which is our case.

**Verdict:** use it, or hand-roll the 15-line DF2T loop. Either is fine; the crate is small
enough and maintained enough that the dependency is not a liability, and it gives us a
tested inner loop.

### 5.3 `fundsp` 0.23 — avoid

Has `BiquadCoefs::arbitrary(a1,a2,b0,b1,b2)`, no weighting filters, normalized Direct
Form I. **Disqualifying issue: the signal path is f32-only regardless of the coefficient
type.** `AudioNode::tick` takes and returns `Frame<f32, _>`, so a `Biquad<f64>` keeps f64
*state* but quantises to f32 at **every node boundary** — every stage of our cascade. Also
`prelude64::biquad` takes f32 arguments and widens them, so the convenience constructors
quantise the coefficients too. Plus it's a music-synthesis framework: `hashbrown`,
`microfft`, `thingbuf`, `numeric-array`, `funutd`, optional `symphonia`. Wildly out of
proportion to running three biquads.

### 5.4 `dasp` — avoid

`dasp` / `dasp_signal` 0.11.0, **both published 2020-05-29**. Unmaintained for ~6 years.
I unpacked `dasp_signal`: `grep -riE "biquad|iir|filter"` over its entire source returns
**zero hits**. There is no biquad and no IIR anywhere in the dasp family — only
`dasp_envelope`, `dasp_rms`, `dasp_peak`, `dasp_interpolate`, `dasp_window`.

### 5.5 `sci-rs` 0.4.1 — viable alternative

<https://github.com/qsib-cbie/sci-rs>. A genuine SciPy port with the complete chain public
and generic over `F: RealField + Float`: `bilinear_zpk_dyn` (line-for-line SciPy,
including zeros-at-infinity → Nyquist and the gain compensation), `zpk2sos_dyn`,
`zpk2tf_dyn`, `ZpkFormatFilter::new(z,p,k)`, `Sos<F>` with public `b`/`a`, and
`sosfilt_dyn` (DF2T per section). So yes — it can compute our coefficients at runtime from
the analogue prototype, and it's the most literal path from the Python references.

Downside: crates.io release is 0.4.1 from **2024-11-22** (~21 months stale) though the repo
is active (last commit 2026-05, 14 open issues). Pulls `nalgebra` 0.33 + `ndarray` 0.16 +
`kalmanfilt` + `lstsq` + `gaussfilt`. **Take it only if a later ticket also wants
runtime-designed octave/third-octave bands** — then it pays for itself. For weighting
alone, the closed form in §2.2 is 40 lines and no dependency.

### 5.6 Python references (for porting)

- **[`endolith/waveform_analysis` `ABC_weighting.py`](https://github.com/endolith/waveform_analysis/blob/master/waveform_analysis/weighting_filters/ABC_weighting.py)** —
  the best reference. Analogue z/p/k with the exact hardcoded constants, `bilinear_zpk`,
  `zpk2sos`, `sosfilt`. Also contains a sympy `_derive_coefficients()` reproducing the
  standard's clause 5.4.9/5.4.10 algebra exactly. Note: the repo was **renamed** — the
  frequently-cited `waveform-analyzer` URL 404s.
- **[`python-acoustics` `iec_61672_1_2013.py`](https://github.com/python-acoustics/python-acoustics/blob/master/acoustics/standards/iec_61672_1_2013.py)** —
  **don't port this one.** It uses the *rounded* pole frequencies and rounded normalisation
  offsets, and it **never bilinear-transforms the weighting filter at all** (its
  `bilinear` call is for the time-weighting integrator). Its
  [`iec_61672_1_2013.csv`](https://github.com/python-acoustics/python-acoustics/blob/master/acoustics/data/iec_61672_1_2013.csv)
  is genuinely useful though — it's Table 2's A/C/Z columns as machine-readable data, and
  matches the standard exactly. Good test-vector source.
- **[Rimell, Mansfield & Paddan 2014](https://pmc.ncbi.nlm.nih.gov/articles/PMC4331191/)** —
  peer-reviewed, gives the exact pole frequencies and the minimum-sample-rate result.
  ⚠️ It publishes its filters as **high-order direct-form coefficient tables**
  (they call them "11th order" for A and "7th order" for C). Do not implement it that way
  — see the next section for why.

---

## Pitfalls

### P1. A single high-order direct-form section is not merely inaccurate — in f32 it blows up

This is the pitfall the ticket asked about, and it is worse than "precision loss".

Measured at 48 kHz, 8 s of sine, RMS of the last 5 s, deviation from the ideal response:

| test frequency | cascaded biquads, f64 | cascaded biquads, f32 | **direct-form 6th order, f32** | direct-form, f64 |
|---|---:|---:|---:|---:|
| A, 10 Hz | −0.0044 | +0.019 | **NaN (overflow)** | −0.0044 |
| A, 31.5 Hz | −0.0043 | −0.010 | **NaN (overflow)** | −0.0043 |
| A, 1000 Hz | 0.0000 | −0.00001 | **NaN (overflow)** | 0.0000 |
| C, 10 Hz | +0.0002 | +0.023 | +0.061 | +0.0002 |
| C, 31.5 Hz | +0.0002 | −0.005 | −0.015 | +0.0002 |

The A-weighting direct-form denominator at 48 kHz is
`[1, −4.113043, 6.553122, −4.990849, 1.785737, −0.246191, 0.011224]`. Every sample requires
summing six terms whose leading coefficients are O(1)–O(6) while the result is a
near-total cancellation. In f32 this is unstable: **it overflows to NaN.** The 4th-order
C-weighting survives f32 but with 2–3× the error of the cascaded form.

**Mitigation: always cascade into biquads.** This costs nothing and removes the problem
entirely. Never build the 6th-order polynomial. If a future maintainer is tempted to
implement Rimell et al.'s published coefficient tables directly, this is the reason not to.

### P2. `f32` state is *usually* fine and *sometimes* isn't — use `f64`

The f₁ double pole maps to z = 0.9973 at 48 kHz (`1 − r = 2.7e−3`), 0.99933 at 192 kHz
(`1 − r = 6.7e−4`). Coefficient *quantisation* in f32 is not the problem — `a1 ≈ −1.9946`
resolves the pole radius to ~6e−8, which is 4 orders of magnitude finer than `1 − r`. The
problem is **roundoff noise amplification** in the near-DC-cancelling recursion.

Measured deviation of an f32 cascade from the f64 cascade (DF2T, 8 s, discard 4 s):

| scenario | 48 kHz, C | 48 kHz, A | 192 kHz, C | 192 kHz, A |
|---|---:|---:|---:|---:|
| 31.5 Hz sine, full scale | −0.006 | −0.006 | +0.048 | +0.042 |
| 31.5 Hz sine at −60 dBFS | −0.005 | −0.006 | +0.051 | +0.046 |
| 31.5 Hz at −60 dBFS **+ DC offset 0.2** | **+0.044** | **+0.058** | **+1.685** | **+0.406** |
| 31.5 Hz at −60 dBFS **+ 5 Hz rumble 0.5** | +0.037 | +0.067 | **+0.150** | **+3.914** |
| 10 Hz sine, full scale | +0.034 | +0.028 | **−0.228** | **−0.147** |
| 1000 Hz sine | −0.00002 | −0.00001 | +0.0002 | +0.0006 |

At 48 kHz f32 is tolerable (worst case ~0.07 dB). At 192 kHz it is **not** — up to 3.9 dB
error on A with infrasonic content present. And note that the two triggers, **a DC offset**
and **infrasonic rumble**, are exactly what a phone microphone in a venue delivers: MEMS
mics have DC bias, and HVAC / footfall / handling noise lives at 5–30 Hz.

**Mitigation: `f64` for coefficients and state.** Two f64 biquads (C) or three (A) is
~20 multiply-adds per sample. At 48 kHz that's under a million flops per second — noise
next to the FFT the spectrogram already needs. There is no performance argument for f32
here, and f32 turns "sample rate we don't control" into a correctness risk.

If f32 ever becomes necessary: use DF2T or DF1 (measured difference between them is
< 0.005 dB, so topology is not the lever), keep the near-unit-circle section last, and
high-pass the input before the weighting filter to kill the DC offset.

### P3. The near-Nyquist pole — accept it, don't prewarp

Covered in [§2.5](#25-what-breaks-at-low-sample-rates--high-frequencies). Summary: the
f₄ = 12194 Hz double pole is at 0.55 of Nyquist at 44.1 kHz, and bilinear warping causes
−3.5 dB at 12.5 kHz and −24 dB at 20 kHz. **This is inside class-1 tolerance** (which is
+3.0/−6.0 and +4.0/−∞ there) and is what every reference implementation does. Prewarping
would trade this for error in the 1–5 kHz band where the tolerance is 4× tighter and where
speech energy actually is — strictly worse for us. Oversampling works but isn't needed.

**Do add a runtime guard:** if the stream sample rate is below ~40 kHz, the filter no
longer meets the class-1 design band. Log it, and consider surfacing it as reduced
confidence rather than silently reporting a number.

### P4. Nominal vs exact one-third-octave frequencies

Already flagged in §1.5 and §4.1, but it is the most likely source of a wasted afternoon:
the standard's table is *labelled* 10, 12.5, 16 Hz and *computed at* 10, 12.5893,
15.8489 Hz. A test written against nominal frequencies will show a 0.18–0.28 dB
"discrepancy" that is not a bug.

### P5. Transient settling in tests

The 20.6 Hz double pole settles slowly. A test that measures RMS over a 1 s buffer starting
at t = 0 will read low at 10–20 Hz. Always discard ≥ 2 s.

### P6. Deriving f₁ in f64 loses ~5 digits (cosmetic, but free to fix)

`f₁² = (−b − sqrt(b² − 4c))/2` is catastrophic cancellation: `−b = 1.487e8` and
`sqrt(b²−4c) = 1.48699e8`, differing in the 6th digit. Computed naively in f64 you get
`f₁ = 20.598997057618` versus the true `20.598997057568143` — 11 correct digits instead of
16. The impact on the response is ~4e−10 dB, i.e. **none**. But the fix is free: use
Vieta's relation, `f₁² = c / f₄²`, where `f₄² = (−b + sqrt(b²−4c))/2` is well-conditioned
(both terms same sign). Verified: that gives `20.598997057568138`. Or just hardcode the
exact constants from §1.3, which is what `endolith` does.

---

## 6. Relationship to L_eq

**Confirmed: weighting is applied to the pressure signal, before squaring and averaging.**

> **3.9 time-average sound level / equivalent continuous sound level**
> twenty times the logarithm to the base ten of the ratio of a root-mean-square sound
> pressure during a stated time interval to the reference sound pressure, sound pressure
> being obtained with a standard frequency weighting
>
> **NOTE 2** In symbols, time-average, A-weighted sound level, L_AT or L_Aeq,T, is given by
>
> `L_AT = L_Aeq,T = 20 lg { [ (1/T) ∫(t−T)^t p_A²(ξ) dξ ]^(1/2) / p₀ }`     (2)
>
> where … **p_A(ξ) is the A-weighted instantaneous sound pressure** …
>
> In equation (2), the numerator of the argument of the logarithm is the root-mean-square,
> **frequency-weighted** sound pressure over averaging time interval T.
>
> **NOTE 3** In principle, time weighting is not involved in a determination of
> time-average sound level.

— [IS 15575-1:2005 / IEC 61672-1:2002, clause 3.9](https://law.resource.org/pub/in/bis/S04/is.15575.1.2005.pdf), p. 4.

The weighting is *inside* the integral, applied to `p_A(ξ)` — the instantaneous
A-weighted pressure. So the chain is:

```
samples → weighting filter → square → average over T → 10·log₁₀ → dB
```

The standard makes the same ordering explicit for exponential time weighting too, in
clause 3.5 Figure 1, captioned *"Principal steps involved in forming an
exponential-time-weighted sound level"*:

> Frequency weighted input → Low-pass filter with one real pole at −1/τ → Square root →
> Result in decibels, reference p₀

Same for peak: clause 3.8 defines peak sound level as using "peak sound pressure being
obtained with a standard frequency weighting" — hence `L_Cpeak`, weighted then peak-detected.

Note also NOTE 3 above: **L_eq involves no time weighting.** Fast/Slow are for
`L_AF`/`L_AS`, not for `L_Aeq,T`. Don't apply an F/S exponential smoother inside an Leq
computation — smooth the *displayed* number if the UI needs it, but keep the Leq
accumulator as a plain linear average of `p_weighted²`.

---

## 7. Bearing on the FFT-vs-biquad decision (input to the other ticket)

Not our decision, but I measured it, and the result is one-sided at low frequencies.

Setup: 20 s of band-limited (100 Hz–6 kHz), syllabically modulated pink noise at 48 kHz as
a speech surrogate, plus a contaminant. Compared `Leq` from time-domain biquads against
`Leq` from per-bin FFT weighting (Hann window, 50 % overlap, correct window-power
normalisation, one-sided doubling). Figures are **FFT-domain minus biquad, in dB**:

| contaminant | weighting | N=1024 | N=2048 | N=4096 | N=8192 | N=16384 | N=4096 rect |
|---|---|---:|---:|---:|---:|---:|---:|
| none | C | −0.002 | +0.010 | +0.001 | +0.013 | +0.007 | +0.009 |
| none | A | +0.017 | +0.023 | +0.022 | +0.022 | +0.016 | +0.026 |
| 30 Hz tone ×10 | C | −0.209 | −0.307 | −0.130 | −0.034 | −0.007 | +0.125 |
| 30 Hz tone ×10 | A | +0.372 | +0.107 | +0.042 | +0.031 | +0.023 | **+2.388** |
| **12 Hz tone ×20** | **C** | **+6.148** | **+4.271** | **+1.723** | **+0.627** | **+0.177** | −0.260 |
| 12 Hz tone ×20 | A | +0.616 | +0.085 | +0.031 | +0.020 | +0.016 | +0.102 |
| DC offset 0.5 | C | **+0.439** | +0.217 | +0.046 | +0.015 | +0.014 | +0.010 |
| DC offset 0.5 | A | +0.022 | +0.025 | +0.018 | +0.020 | +0.017 | +0.025 |
| 15 kHz tone ×0.1 | C | +0.011 | +0.020 | +0.025 | +0.013 | +0.010 | +0.014 |
| 15 kHz tone ×0.1 | A | +0.031 | +0.036 | +0.037 | +0.027 | +0.033 | +0.035 |

Reading:

- **For clean content, FFT-domain weighting is equivalent** — agreement to ~0.02 dB at any
  window length. If the signal has no significant infrasound and no DC, either method
  works.
- **FFT-domain C-weighting fails badly on infrasonic content at short windows.** With a
  12 Hz component it over-reads by **+6.1 dB at N=1024** and +4.3 dB at N=2048. Cause: bin
  spacing is 46.9 Hz at N=1024, so the entire steep C roll-off below f₁ = 20.6 Hz falls
  inside the first bin, and the window main lobe smears that energy into bins whose
  assigned gain is ~0 dB. **This is exactly the regime C-weighting exists to characterise**
  — which makes FFT-domain weighting fragile precisely where our default mode is
  sensitive.
- **The error direction is over-reading**, which for a 70 dB ceiling means false alarms.
- **A rectangular window is dangerous** even at moderate frequencies: +2.4 dB on A with a
  30 Hz tone, from sidelobe leakage into bins where A ≈ 0 dB.
- **A DC offset alone** costs +0.44 dB on C at N=1024.

Consequences for that ticket:

1. If the reported number comes from FFT bins, it needs **N ≥ 8192 at 48 kHz** (≈ 170 ms,
   Δf ≤ 5.9 Hz) **and** a DC blocker ahead of it, and even then C carries ~0.6 dB of
   contamination risk in a rumbly room. N ≥ 16384 (≈ 340 ms) gets it to ~0.18 dB. That is
   a long window for a live meter.
2. Time-domain biquads have none of this sensitivity, cost ~20 f64 multiply-adds per
   sample, and are what the standard's own definitions describe (clause 3.9's `p_A(ξ)` is
   a time-domain signal).
3. The two are not in conflict: **use biquads for the reported level and the FFT for the
   spectrogram.** Applying weighting to the FFT magnitudes for display purposes is fine
   and cheap; just don't derive the number from it.
4. Also relevant: the standard's `L_eq` is a continuous integral with filter memory across
   the whole averaging period. Framed FFT processing has no memory across frames — as
   Virtins puts it for their own product, *"FFT filters cannot ensure the continuity across
   frames, unlike IIR and FIR filters."*
   ([Virtins 2024, p. 24](https://www.virtins.com/doc/Frequency-Weightings-for-Sound-and-Vibration-Perceived-by-Humans-using-Multi-Instrument.pdf))

---

## Open questions (flagged for the architecture decision)

1. **What sample rate does iOS actually hand us?** Everything above says: fine at ≥ 44.1 kHz,
   preferable at 48 kHz, degraded below ~40 kHz. If the capture path can request a rate,
   request 48 kHz. If it can't, we need the runtime guard from P3. Specifically:
   **what does AVAudioSession give us on the target devices, and can it change mid-session?**
   (If it can, the coefficient derivation must be re-run on rate change and the filter
   state reset — cheap, but it has to be wired up.)
2. **Could iOS give us 192 kHz?** Unlikely via the built-in mic, but possible with an
   external USB interface. If so, P2's f32 findings become severe. The f64 recommendation
   makes this moot, which is a reason to just take f64 now rather than revisit.
3. **Is a DC blocker / infrasonic high-pass wanted upstream of the weighting filter?**
   The weighting filter *is* a high-pass, so it isn't needed for correctness in f64. But
   it would reduce internal dynamic range, help if we ever want f32, and protect an
   FFT-domain path. Interacts with ticket-level decisions about the capture chain.
4. **Do we ever report a class?** This document deliberately does not claim conformance.
   If any UI copy is ever tempted toward "class 2 accuracy", the Ed. 2 Table 3 acceptance
   limits need to be obtained from the purchased standard first (see the edition caveat in
   §3.1), and the microphone — not the filter — becomes the binding constraint.
5. **Z-weighting.** Free (`Z(f) = 0`, clause 5.4.8 eq. 8, flat 10 Hz–20 kHz). Worth
   exposing as a third mode? Costs nothing, useful for debugging the calibration chain
   independently of the filters.
6. **F / S / Leq time behaviour** is a separate ticket, but note clause 3.9 NOTE 3: L_eq
   involves no time weighting. `lasprs`'s `src/slm/` has F/S/I as analogue poles
   (F = −8 rad/s, S = −1 rad/s, I asymmetric 35 ms rise / 1.5 s decay) if that ticket wants
   a reference.

---

## Appendix A — sources

Primary / normative:

- **[IS 15575 (Part 1):2005 / IEC 61672-1:2002](https://law.resource.org/pub/in/bis/S04/is.15575.1.2005.pdf)** —
  Bureau of Indian Standards adoption, declared identical with IEC 61672-1:2002, published
  in full by law.resource.org. Source for: clause 3.5/3.8/3.9 (definitions and the L_eq
  ordering), clauses 5.4.5–5.4.14 (weighting equations and pole derivation), Table 2
  (weighting values and class 1/2 tolerance limits), Annex A Table A.1 (maximum expanded
  uncertainties).
- **[IEC 61672-1:2013 Ed. 2.0 preview](https://cdn.standards.iteh.ai/samples/17900/df52d949fc904f329404e965b6268258/IEC-61672-1-2013.pdf)** —
  front matter only (15 pp.). Confirms Ed. 2's structure: Table 3 = *"Frequency weightings
  and acceptance limits"* (p. 22), Annex E (normative) = *"Analytical expressions for
  frequency-weightings C, A, and Z"* (p. 49). The tables themselves are beyond the preview.
- **[MathWorks, *Audio Weighting Filters*](https://www.mathworks.com/help/audio/ug/audio-weighting-filters.html)** —
  states its A/C weighting is a "direct implementation of the filter's transfer function
  based on poles and zeros specified in the ANSI S1.42 standard".

Authoritative reproductions:

- **[Virtins Technology, *Frequency Weightings for Sound and Vibration Perceived by Humans using Multi-Instrument*, Rev 01, 2024](https://www.virtins.com/doc/Frequency-Weightings-for-Sound-and-Vibration-Perceived-by-Humans-using-Multi-Instrument.pdf)** —
  reproduces the clause-5.4.6/5.4.9/5.4.10 derivation verbatim (p. 5) including b, c, and
  the f₁/f₄ quadratic; the ANSI S1.42 s-domain transfer functions with G_A/G_C; the full
  one-third-octave weighting table (p. 6); and a discussion of IIR / FIR / FFT
  implementation (§5, pp. 17–24).
- **[Rimell, Mansfield & Paddan, *Design of digital filters for frequency weightings (A and C) required for risk assessments of workers exposed to noise*, Industrial Health 53(1):21–27, 2014](https://pmc.ncbi.nlm.nih.gov/articles/PMC4331191/)** —
  peer-reviewed. Exact pole frequencies; bilinear transform method; Table 3 minimum sampling
  frequency for tolerance conformance (35 kHz class 1, 20 kHz class 2, 71–72 kHz ANSI type 0).
- **[Cirrus Research, *Class 1 vs Class 2*](https://cirrusresearch.com/whats-the-difference-between-a-class-1-and-class-2-sound-level-meter/)** —
  Class 1 SLM manufacturer; corroborates five tolerance-table entries exactly.

Readable reference implementations:

- **[`endolith/waveform_analysis` `ABC_weighting.py`](https://github.com/endolith/waveform_analysis/blob/master/waveform_analysis/weighting_filters/ABC_weighting.py)**
- **[`python-acoustics` `iec_61672_1_2013.py`](https://github.com/python-acoustics/python-acoustics/blob/master/acoustics/standards/iec_61672_1_2013.py)**
  and its [Table 2 CSV](https://github.com/python-acoustics/python-acoustics/blob/master/acoustics/data/iec_61672_1_2013.csv)
- **[`lasprs`](https://code.ascee.nl/ascee/lasprs)** — `src/filter/zpkmodel.rs`
  (`freqWeightingFilter`), `src/filter/biquad.rs` (`bilinear_zpk`), `src/slm/`
- **[`biquad`](https://github.com/korken89/biquad-rs)**, **[`sci-rs`](https://github.com/qsib-cbie/sci-rs)**

## Appendix B — how the numbers here were produced

Every measured figure in this document was computed rather than quoted. For reproduction:

- Pole frequencies: clause 5.4.9/5.4.10 algebra evaluated at 40 decimal digits (mpmath),
  cross-checked against `endolith`'s hardcoded constants and `lasprs`'s runtime derivation.
- Analogue reference values: equations (6)/(7) evaluated directly at
  `f(n) = 1000·10^(0.1(n−30))`, `n = 10…43`; verified against the standard's Table 2 to
  within ±0.05 dB (the table's rounding) at all 68 A and C entries.
- Digital responses: analogue z/p/k → `scipy.signal.bilinear_zpk` → `zpk2sos` →
  `sosfreqz`, renormalised at 1 kHz; independently reproduced by the hand-derived
  closed form in §2.2 (agreement < 7e−8 dB).
- Minimum sample rates: bisection on `f_s` over the condition "all Table 2 frequencies
  below `f_s/2` inside the band".
- f32/f64 and topology comparisons: explicit scalar DF1 and DF2T loops with per-operation
  `numpy.float32`/`float64` casts, 8 s of signal at 48 and 192 kHz, RMS of the tail after
  discarding 3–4 s. The direct-form comparison used the fully expanded 6th-order (A) and
  4th-order (C) polynomials.
- FFT-vs-biquad: `scipy.signal.sosfilt` for the biquad path; framed `numpy.fft.rfft` with
  Hann/rectangular windows, 50 % overlap, `1/(N²·mean(w²))` normalisation and one-sided
  doubling for the FFT path; 20 s signals at 48 kHz.
