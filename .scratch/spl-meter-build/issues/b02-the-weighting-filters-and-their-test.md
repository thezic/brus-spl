# The weighting filters and their 34-row test

Parent: [SPL Meter Build](../map.md)
Type: build
Status: open
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
