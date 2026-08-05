# The weighting filters and their 34-row test

Parent: [SPL Meter Build](../map.md)
Type: build
Status: resolved
Blocked by: —

## Build

`weighting.rs`: the A, C and Z chains, coefficients derived at runtime from the actual sample
rate, plus the `#[cfg(test)]` module that proves them. Spec
[§5.3](../../spl-meter-mvp/spec.md#53-the-filters) and
[§14.1](../../spl-meter-mvp/spec.md#141-the-weighting-filters--a-cargo-test-module-no-new-tooling);
the full derivation and both tables are in
[`research/03-iec-weighting-filters.md`](../../spl-meter-mvp/research/03-iec-weighting-filters.md).

**This ticket is independent of everything else.** Synthetic input, no session, no device, no
hardware — it is the one ticket that can run as a concurrent session alongside
[Session, capture and the idle timer](b01-session-capture-and-the-idle-timer.md).

**Build, don't adopt.** Nothing in Rust is adoptable as an IEC weighting implementation, and
the recursion is ~15 lines. **No `biquad` crate** — the coefficient derivation is ours either
way (`04 d4`).

Pole frequencies, derived from clauses 5.4.9–5.4.11:

```
f₁ =    20.598997057568143 Hz   (double pole, A and C)
f₂ =   107.65264864304627  Hz   (single pole, A only)
f₃ =   737.8622307362899   Hz   (single pole, A only)
f₄ = 12194.217147998010    Hz   (double pole, A and C)
```

All poles real and negative — **no complex arithmetic anywhere**. Zeros at `s = 0`: two for C,
four for A.

**Always cascade; never build the expanded high-order polynomial** — `03` P1 measured a single
6th-order direct-form section in f32 *overflowing to NaN*. Put `S_hp1` last, the section
nearest the unit circle:

| Mode | Sections, in cascade order |
|---|---|
| C | `S_lp4` · `S_hp1` |
| A | `S_lp4` · `S_hp23` · `S_hp1` |
| Z | none — bypass, RMS straight off the raw samples |

Discretisation is the **plain bilinear transform, no prewarping, no oversampling**. With
`c = 2·f_s`, per §5.3's table of `n₂,n₁,n₀` / `d₂,d₁,d₀`. The recipe depends on the rate
**only through `c`** — ~30 flops — so coefficients are computed at stream start from the
read-back rate and **never compiled in**.

**Normalise the digital cascade at 1 kHz, not the analogue prototype.** Drop the standard's
`A₁₀₀₀`/`C₁₀₀₀` entirely: evaluate `|H_digital(e^{j2π·1000/f_s})|` and divide section 0's `b`
coefficients by it. One line, worth it twice — 1 kHz is *the* calibration anchor, and it makes
clause 5.4.14 exactly zero by construction, which turns it into a free unit test.

**Realisation: transposed direct form II, f64 coefficients *and* state.** Worst case (A, three
sections) is 15 multiply-adds per sample, ≈720 k/s at 48 kHz.

## The test

Research `03` §4.2 is the table: 34 rows × A/C, with exact frequencies, analogue design-goal
values, published table values, class-1 / design-band / class-2 tolerances, and measured
bilinear deviations at 44.1 and 48 kHz. **Two levels of assertion**, so a real regression is
distinguishable from a tolerance question:

- **Tight** — digital response vs the *analogue design-goal* value, **±0.01 dB below 4 kHz**
  and per the table's measured deviations above it. This is the regression test.
- **Loose** — digital response vs the *published table* value, within the design band.

Plus, all cheap and all catching things a sweep would smear over:

- **The two design invariants**: `C = −3.010 dB` at `f_L = 31.6228 Hz` **and** at
  `f_H = 7943.282 Hz`. These catch a transposed constant.
- **`|H(1000 Hz)| = 0.000 dB` to within 1e-9** for both A and C — what the digital
  renormalisation buys.
- **`|L_C − L_A| < 1e-9` at 1 kHz** (clause 5.4.14, free).
- **Z returns the input unchanged.**
- One pass at **amplitude 1e-3** to catch precision regressions.

Derive and assert at **16, 44.1, 48 and 192 kHz**, since the coefficients are a runtime
function of the rate.

## Traps

Both of these produce a confusing red test rather than an obviously wrong one, and §14.1 names
them as the two most likely causes:

1. **Use the exact one-third-octave frequencies, not the nominal labels.**
   `f(n) = 1000·10^(0.1(n−30))`, `n = 10 … 43`. Testing at 12.5 Hz instead of 12.5893 Hz
   produces a spurious 0.18 dB "failure"; at 16 Hz it is 0.28 dB.
2. **Discard the transient.** Feed ≥3 s and measure the RMS of the **last 2 s**. The f₁ double
   pole's time constant is ~7.7 ms but settling to 0.001 dB takes far longer.

Two more:

- Amplitude is irrelevant in f64 — a full-scale sine has RMS `−3.0103 dBFS`, so the expected
  output is `−3.0103 + W(f)`.
- Research `03` §2.4's coefficient fixtures are **for cross-checking, not for hardcoding**. The
  gain distribution across sections is arbitrary, so compare *responses*, never coefficients.

`cargo test` plus a `#[cfg(test)]` module adds **no dependency, no config and no tooling
decision**, so `CLAUDE.md`'s "ask before adding a test runner" does not apply (`04`, confirmed
by `10`).

## Done when

`cargo test` is green across all four sample rates at both assertion levels, with the
invariants, the 1 kHz renormalisation, the clause 5.4.14 identity, the Z identity and the
low-amplitude pass. `cargo clippy` clean, `cargo fmt` applied.

## Resolution

Built as specified, in `src-tauri/src/weighting.rs` and nowhere else. 21 tests, 590 filtered
tone measurements across the four rates at the two assertion levels, `cargo test` green,
`cargo clippy` clean, `rustfmt` applied to that file only.

### The public shape

```rust
pub enum Weighting { A, C, Z }

pub struct WeightingChain { /* weighting, sample_rate, Vec<Biquad> */ }

impl WeightingChain {
    pub fn new(sample_rate_hz: f64, weighting: Weighting) -> Self;  // panics ≤ 2 kHz
    pub fn weighting(&self) -> Weighting;
    pub fn sample_rate(&self) -> f64;
    pub fn process(&mut self, sample: f64) -> f64;
    pub fn process_block(&mut self, block: &mut [f64]);
    pub fn reset(&mut self);
}
```

Nothing else is public. No dependency was added; `lib.rs`, `Cargo.toml` and every other file
were left alone.

### Findings

1. **The ticket's tight tolerance, read literally, fails on a row the ticket itself
   supplies.** "±0.01 dB below 4 kHz" cannot mean *versus the unwarped analogue design goal*,
   because research `03` §4.2's own measured columns put the 3150 Hz row (f = 3162.2777 Hz) at
   **−0.016 dB (A) and −0.019 dB (C) at 44.1 kHz**, and −0.014 / −0.016 at 48 kHz — all four
   outside ±0.01, and all four below 4 kHz. Reproduced here from the closed form to
   −0.0165 / −0.0188 / −0.0138 / −0.0158 dB. The implemented reading is **±0.01 dB around the
   expected bilinear deviation**, which below 2.5 kHz is indistinguishable from the literal
   one. Not a defect in the filter; a defect in the tolerance sentence, which `03` §4.1 and
   spec §14.1 both carry.
2. **The expected deviation is computed, not tabulated, so the tolerance is rate-general.**
   The bilinear transform is an exact warped mapping, `Ω(f) = (f_s/π)·tan(π f / f_s)`, so the
   digital response at `f` *is* the analogue response at `Ω(f)`. The test evaluates equations
   (6)/(7) at `Ω(f)` and normalises at `Ω(1000)`. This matters because the table's deviation
   columns exist only for 44.1 and 48 kHz, and the ticket asks for 16 and 192 kHz too. The
   prediction was checked against all 136 published deviation figures: **worst disagreement
   0.000877 dB**, i.e. inside their 3-decimal rounding.
3. **Measured worst deviations, tight level** (digital response vs analogue design goal plus
   predicted warping, ±0.01 dB): **16 kHz 9e-6 dB** (60 assertions), **44.1 kHz 4e-6 dB** (68),
   **48 kHz 1e-6 dB** (68), **192 kHz < 5e-7 dB** (68), **48 kHz at amplitude 1e-3 1e-6 dB**
   (68). Three orders of margin. Research `03`'s headline — that the low end is not where the
   difficulty is — holds.
4. **Loose level passes everywhere it is claimed to.** Worst approach to a class-1 design-band
   edge is **0.700 dB at 44.1 / 48 / 192 kHz** (the 1 kHz row, whose ±0.7 band is the tightest
   and whose error is nil) and **0.486 dB at 16 kHz**. The large raw errors — 24.2 dB at
   44.1 kHz, 15.7 at 48 kHz — are the 19952 Hz row, where the band's lower limit is −∞.
5. **16 kHz cannot be held to the design band above 4 kHz, and the test says so out loud
   rather than loosening the band.** Measured error vs the published table at 16 kHz:
   3981 Hz −0.51 dB (in, band ±1.0), 5012 Hz **−1.53 dB for C** (out, band ±1.5, by 0.026 dB),
   6310 Hz −5.79 dB (out), 7943 Hz −59.9 dB (out). This is research `03` §2.5's bisected
   39 012 Hz minimum, confirmed independently. The loose test therefore stops at 4 kHz for
   rates below 39 kHz; the tight test still runs every row below Nyquist at 16 kHz and passes.
   16 kHz is in the rate list to exercise the runtime derivation, not to claim conformance.
6. **The f_H design invariant is analogue-only and a literal digital reading of it fails.**
   C is −3.010 dB at f_L = 31.6228 Hz *and* at f_H = 7943.282 Hz in the prototype, but the
   digital filter reads **−3.66 dB at 44.1 kHz and −3.54 dB at 48 kHz** at f_H — the warping is
   −0.647 / −0.532 dB there, and it is in the ticket's own table. So the invariant is asserted
   twice: on the module's constants at **1e-9** (which is what actually catches a transposed
   f₁/f₄), and end-to-end against the warped prediction at 0.01 dB. At f_L the digital filter
   does read −3.010 dB flat at all four rates.
7. **The 1 kHz assertions are exact but weak, and that is worth knowing.** `|H(1 kHz)| = 0 dB`
   to 1e-9 and `|L_C − L_A| < 1e-9` both come free from the renormalisation and stay true
   under a deliberately corrupted section — verified by mutation (see 10). They test the
   renormalisation, not the filter. Both are asserted on the cascade's analytic magnitude, not
   on an RMS measurement, since an RMS estimate is nowhere near 1e-9.
8. **A partial-cycle bias in the RMS estimator is a third trap the ticket does not name.**
   The mean of `sin²` over a non-whole number of half-cycles is biased; at 10 Hz over a flat
   2 s that is **0.035 dB**, three times the tight tolerance, and it would have looked exactly
   like a low-frequency filter error. The measurement window is therefore nudged to the nearest
   whole number of half-cycles that lands closest to an integer sample count, which drops the
   bias below 1e-5 dB. Warm-up is 1.5 s and the measurement ~2 s, per trap 2.
9. **Research `03` §2.4's fixtures agree to 1.7e-13 dB.** Compared as responses across every
   testable row at 44.1 and 48 kHz, both modes, as instructed. The coefficient derivation is
   therefore confirmed against an independent implementation as well as against the closed
   form.
10. **The suite was mutation-tested.** Replacing `S_hp23`'s `ω₂+ω₃` with `2ω₂` — a plausible
    copy-paste slip — fails 10 of the 21 tests, including all four tight rates, all four loose
    rates and the fixture comparison. The mutation was reverted and the suite is green.
11. **The pole constants are hardcoded and separately re-derived in a test** from clauses
    5.4.9–5.4.11, using Vieta (`f₁² = c/f₄²`) rather than the cancelling root, per `03` P6.
    They agree to better than 1e-12 relative.

### Where the spec and research were right

Spec [§16](../../spl-meter-mvp/spec.md#16-what-this-spec-decides-that-no-ticket-decided)'s
eleven choices are untouched by this ticket — none of them concerns the filter. `05` (band
assignment), `04` (FFT normalisation) and the rest belong to `b03`/`b09`. Nothing in §5.3 was
found wrong: the section table, the bilinear recipe, the cascade order and the digital 1 kHz
renormalisation all produced the numbers they promised, first time.

### Not done

- **`cargo check --target aarch64-apple-ios --lib` was not run.** This ticket touches no
  iOS-only code, and running it would compile `b01`'s in-flight `session.rs`.
- **Not exercised under `npm run tauri dev`.** Nothing calls `weighting.rs` yet — `lib.rs` only
  declares the module — so there is no runtime path to exercise. It compiles into the same lib
  `tauri dev` builds, which `cargo test` proves.
- **No serde derives on `Weighting`.** The wire enum is `bridge.rs`'s to own (spec §12); adding
  them here would be the module reaching across a seam it does not own. One line for whoever
  writes `bridge.rs`, either way.
