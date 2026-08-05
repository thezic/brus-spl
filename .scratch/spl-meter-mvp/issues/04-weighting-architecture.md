# Weighting architecture: time-domain biquads or FFT-domain?

Parent: [SPL Meter MVP](../map.md)
Type: grilling
Status: resolved
Blocked by: 01, 02, 03  (all resolved)

**Hard numbers now available from `02`, measured on the device rather than assumed:**
**48 000 Hz, mono, f32, 1024-frame buffer** (`IOBufferDuration` 0.021333 s), block rate
46.875/s. So FFT sizing, bin resolution and biquad coefficients can be argued concretely.
Note 48 kHz clears research `03`'s ~40 kHz minimum — but a Bluetooth route may drop it to
8–16 kHz, which ticket [`11`](11-interruption-and-gap-handling.md) is probing; the answer here
should not assume 48 kHz is guaranteed for all routes.

## Question

The spectrogram needs an FFT regardless. So weighted levels could be derived from that
same FFT — apply the weighting curve to the magnitude bins and sum — instead of running a
separate biquad chain over the time-domain samples.

- **One pipeline or two?** FFT-domain weighting means a single analysis path feeding both
  the meter and the spectrogram. Biquads mean two independent paths, but each does its own
  job properly.
- **Accuracy.** FFT-domain weighting is bin-resolution-limited and window-dependent, which
  matters most at low frequencies — exactly where dB(C) lives. Biquads have no such
  problem.
- **What the standard assumes.** IEC 61672 is written around continuous filtering; L_eq
  from summed FFT bins is a different computation that happens to approximate it.

This decision shapes tickets `05`, `07`, and `08`, so it must be settled before any of
them.

## Evidence from ticket 03

Research measured the comparison rather than reasoning about it — see
[`research/03-iec-weighting-filters.md`](../research/03-iec-weighting-filters.md) §7. The
result is one-sided at low frequencies, which is where dB(C) lives:

| Condition | FFT-domain error vs biquads |
|---|---|
| Clean content | ~0.02 dB — indistinguishable |
| 12 Hz component present, N=1024 | **C over-reads by +6.1 dB** |
| 12 Hz component present, N=2048 | **C over-reads by +4.3 dB** |
| 12 Hz component present, N=8192 (170 ms) | under 0.6 dB |
| 30 Hz tone, rectangular window | **A over-reads by +2.4 dB** |

The error direction is **over-reading**, meaning false alarms against the 70 dB ceiling —
the app would tell Simon to turn a talk down when it didn't need turning down. And a venue
with HVAC is exactly the environment that supplies the infrasonic content that triggers it.

**Recommended answer: biquads for the reported level, FFT for the spectrogram only.** Two
pipelines, not one. The "one pipeline is simpler" argument loses because the shared pipeline
would have to run N≥8192 to be trustworthy, which is a 170 ms frame — sluggish for the live
instantaneous readout — and it would still be the less defensible of the two.

Two facts that also land here:

- **Minimum viable sample rate is ~40 kHz** (36.9 kHz for class-1 limits, 39.0 kHz for the
  design band). Below that the filter is out of tolerance regardless of topology. A runtime
  guard is needed; the Bluetooth-route case is owned by ticket
  [`11`](11-interruption-and-gap-handling.md).
- **Sample rate is a runtime value** read back from the audio session (ticket
  [`01`](01-native-audio-capture-path.md)), so coefficients are computed at startup, not
  compiled in. Research measured this at ~30 flops.

What is left for this ticket is therefore mostly confirmation rather than open exploration —
unless Simon wants to trade defensibility for a single pipeline, which he should be told is
the trade rather than discovering it later.

---

## Answer

**Two pipelines. Time-domain cascaded biquads produce the reported number; the FFT draws the
spectrogram and nothing else.** The trade was put to Simon explicitly rather than assumed, and
he took defensibility. One chain runs at a time — the selected weighting only — because the
A/C/Z choice turned out to live on a **settings page**, not in the main UI, which removed the
only argument for running several at once.

Below, the shape settled here:

```
callback ─┬─→ [selected weighting chain, f64] ─→ square ─→ Leq accumulator ─→ the number
          │      C: 2 biquads · A: 3 biquads · Z: bypass
          └─→ [ring buffer] ─→ FFT (off-thread) ─→ magnitude bins ─→ spectrogram
```

### Decisions (2026-08-05, with Simon)

**1. Biquads for the reported level, FFT for the spectrogram only. Two pipelines.**

Research `03` §7 measured the alternative rather than reasoning about it, and the result is
one-sided exactly where dB(C) lives: with a 12 Hz component present, FFT-domain C **over-reads
by +6.1 dB at N=1024** and +4.3 dB at N=2048, because the entire steep C roll-off below
f₁ = 20.6 Hz falls inside the first bin (46.9 Hz wide) and the window main lobe smears that
energy into bins whose assigned gain is ~0 dB. A rectangular window costs +2.4 dB on A with a
30 Hz tone. **The error direction is over-reading**, so the failure mode is false alarms
against the 70 dB ceiling — the app telling Simon to turn a talk down that didn't need it — and
a venue with HVAC is precisely the environment that supplies the infrasound that triggers it.

The single-pipeline option is not merely less accurate, it is also slower: to be trustworthy it
needs **N ≥ 8192** (170 ms at 48 kHz) plus a DC blocker and *still* carries ~0.6 dB of
contamination risk in a rumbly room; N ≥ 16384 (340 ms) gets it to ~0.18 dB. A 170–340 ms frame
is sluggish for a live instantaneous readout. And framed FFT processing has no filter memory
across frames, whereas the standard's L_eq (clause 3.9 eq. 2) is a continuous integral over a
time-domain weighted signal `p_A(ξ)`.

The cost of the second pipeline is trivial: DF2T is 5 multiply-adds per section per sample, so
the worst case (A, 3 sections) is **15 per sample ≈ 720 k/s at 48 kHz**.

**2. One chain at a time — the selected weighting only. Changing it resets the rolling window.**

Reversed mid-grilling. The case for running A and C in parallel rested on the assumption that
the weighting is a main-UI toggle Simon flips mid-talk, where losing the 60 s window on every
flip would be intolerable. **It is a settings-page setting**, so changing it is a deliberate,
infrequent act — the same reasoning ticket [`11`](11-interruption-and-gap-handling.md) used in
decisions 5 and 6 for route changes and calibration staleness: something you go out of your way
to do does not need machinery to absorb it. Running a second accumulator to smooth a case that
doesn't arise is exactly what the map's "resist complexity" rule forbids.

So one biquad chain, one energy accumulator, one window. On a weighting change the chain is
rebuilt, its state zeroed, and the window restarts from empty (`2s of 60s`, refilling).

Convenient side effect: the filter's settling transient is buried. The f₁ double pole settles
slowly (τ ≈ 7.7 ms, but 0.001 dB settling takes far longer — research `03` P5 discards ≥2 s in
tests), so the first fraction of a second after a rebuild reads low. Since the window is
visibly near-empty at that moment anyway, no special handling is needed.

**3. The FFT taps the raw samples, ahead of the weighting filter.**

Keeps the two pipelines genuinely independent: no ordering dependency, and no filter transient
leaking into the picture when the weighting setting changes. It also preserves both display
options for ticket [`07`](07-spectrogram-form.md) — if the spectrogram should visually match the
meter, weighting is a per-bin gain computed straight from the standard's closed-form magnitude
equation (research `03` §1.2), which is cheap and *safe here*, because decision 1's +6.1 dB
objection only applies when bins are summed into a reported number. Nothing is summed for a
colour map. A post-filter tap would forfeit the unweighted view permanently for no gain.

**4. Hand-roll the DF2T recursion. No `biquad` crate.**

The coefficient derivation is ours either way — the crate's filter *designers* are useless to us
and we would use only its public `Coefficients` fields. The recursion is ~15 lines, so
hand-rolling keeps the weighting module dependency-free and puts every line of it in code we can
read. Research `03` §5.2 rates the dependency as not a liability, so this is preference, not
necessity: `biquad` 0.6 remains a clean fallback if the hand-rolled runner ever disappoints.

**5. No sub-40 kHz runtime guard. A log line, nothing in the UI.**

This closes a live contradiction between two documents. Research `03` P3 says *"do add a runtime
guard… consider surfacing it as reduced confidence"*; ticket `11` decision 5 had already refused
to refuse or flag low-rate routes. **Ticket `11` wins, deliberately.** Coefficients are designed
at runtime from the actual rate, so at 16 kHz the filter is correctly designed *for* 16 kHz — it
simply cannot meet the standard's tolerance near Nyquist. Refusing or flagging is automation plus
a warning, for a case that requires deliberately measuring through a Bluetooth headset mic. The
honesty lives in the spec's stated accuracy limits (ticket [`10`](10-write-the-spec.md)), not in
runtime UI. A log line remains, for post-hoc debugging only.

**6. Z-weighting is a third option on the settings page.** Simon's call, against the
recommendation to keep it internal.

Z is free — clause 5.4.8 eq. 8 defines it as flat 10 Hz–20 kHz, so in this architecture it is
the bypass path: no filter, RMS straight off the raw samples. Its value is diagnostic. It is the
only way to exercise the calibration chain end-to-end with the filters out of the way, which is
otherwise hard to isolate, and that matters because ticket [`06`](06-calibration-model.md) rests
on the input path being linear. Cost is one radio button and a branch.

Consequence for ticket `10`: the spec now describes **three** modes, and should say what Z is
*for* — it is not a third opinion about loudness, it is the instrument with its filters
switched off.

### Recorded, not debated — forced by research `03` or ticket `11`

- **`f64` for coefficients *and* state.** Not f32. Research `03` P1/P2: a single 6th-order
  direct-form section in f32 **overflows to NaN**, and even correctly cascaded f32 biquads
  degrade to +1.7 dB with a DC offset and +3.9 dB with 5 Hz rumble at high rates — and DC bias
  plus infrasonic HVAC is exactly what a phone mic in a venue delivers. Convert f32 → f64 at the
  1024-frame block boundary; the IIR recursion is inherently serial, so nothing is lost.
- **Never build the expanded high-order polynomial.** Always cascade: 2 sections for C, 3 for A,
  with the near-unit-circle section (`S_hp1`) last.
- **Coefficients derived at stream start from the actual rate** (~30 flops), plain bilinear
  transform, **no prewarping, no oversampling**, and the cascade renormalised digitally at 1 kHz
  rather than using the standard's analogue `A₁₀₀₀`/`C₁₀₀₀` (research `03` §2.3 — worth the one
  line, since 1 kHz is the calibration anchor).
- **On a mid-session rate change: recompute coefficients and zero the filter state, but do *not*
  reset the window.** Mark the transition as gap slots instead, per ticket `11` decision 3.
  The asymmetry with decision 2 above is principled: a **weighting** change makes previously
  accumulated energy *incommensurable* (LCeq and LAeq are different quantities and cannot be
  averaged together), whereas a **rate** change keeps measuring the same quantity through a
  differently-designed filter, so the old slots remain valid energy. Ticket `11`'s session
  polling already makes rate changes observable.
- **Weighting before squaring and averaging** (clause 3.9), and **no F/S time weighting inside
  L_eq** (clause 3.9 NOTE 3). Smooth the *displayed* number if ticket `05` wants to; keep the
  accumulator a plain linear average of `p_weighted²`.
- **No DC blocker.** The weighting filter is itself a high-pass, and f64 removes the precision
  motive (research `03` open question 3). Adding one is unmandated complexity, and since
  decision 3 taps the FFT upstream it would also hide DC from the spectrogram, where it is
  information rather than noise.
- **Biquads run in the audio callback; the FFT runs off it**, fed from the ring buffer. The
  filter cost is deterministic and tiny (decision 1); an FFT in the callback is not.

### What this ticket hands downstream

- **Ticket [`05`](05-level-metrics-pipeline.md) — unblocked, plus two constraints.** The
  accumulator averages `p_weighted²` linearly with no time weighting inside it, and the window
  now has a **second reset cause** besides the manual reset button: a weighting-setting change.
  A rate change, by contrast, is gap slots and not a reset.
- **Ticket [`07`](07-spectrogram-form.md) — unblocked, and its input is defined.** The FFT gets
  raw unweighted samples, so `07` owns whether the display applies a weighting curve; both are
  available and it is a per-bin gain either way. One wrinkle from the no-DC-blocker decision:
  **bin 0 carries the microphone's DC bias**, so `07` should not draw it — the standard's band
  starts at 10 Hz regardless.
- **Ticket [`08`](08-rust-frontend-boundary.md) — the bulk payload is confirmed to be exactly
  one thing.** The meter crosses the bridge as a scalar plus coverage per display frame, which is
  trivial; the FFT frames are the only demanding traffic. A design that gets the spectrogram
  right gets the whole boundary right.
- **Ticket [`10`](10-write-the-spec.md) — three modes, not two,** and Z needs its purpose stated
  (decision 6). Also inherits decision 5's limitation, already listed by `11`.

### Findings for the map's open questions

- **"Whether the weighting filter validation table becomes a unit test, and what test runner gets
  added" — the Rust half of this dissolves.** Hand-rolled biquads with runtime coefficient
  derivation *need* research `03`'s 34-frequency table as a test, and it is a pure Rust test:
  `cargo test` with a `#[cfg(test)]` module adds no dependency, no config and no tooling
  decision. `CLAUDE.md`'s "ask before adding a test runner" is about *adding* one, and there is
  nothing to add. The frontend testing question is separate and untouched by this.
- **"Desktop dev-loop fidelity" — now answerable.** With biquads settled, reproducible testing
  needs no reference meter and no hardware: synthesise tones in Rust at the exact one-third-octave
  frequencies and assert against research `03` §4.2. Two rows are design invariants worth tight
  tolerances — C = −3.010 dB at f_L = 31.6228 Hz and at f_H = 7943.282 Hz — plus
  `|H(1000 Hz)| = 0` for every mode and `|L_C − L_A| < 1e-9` at 1 kHz (clause 5.4.14). Watch the
  two traps: use **exact** frequencies, not the nominal labels (0.28 dB spurious failure), and
  discard ≥2 s of transient.
- **"How the dB(A)/dB(C) switch presents" — partly answered.** It is a **settings-page setting**,
  which rules out the "show both at once" option the map floated. What remains for the layout work
  is only how the *active* mode is indicated on the main screen.

### Residual risks, stated plainly

- **Decision 5 leaves a silently-wrong number on sub-40 kHz routes.** Inherited from ticket `11`
  decision 5 rather than created here, and accepted on the same reasoning.
- **Decision 4 puts the DF2T recursion in our hands.** Low risk — the bugs in this kind of code
  live in the coefficients, which were ours regardless — and research `03`'s validation table is
  the mitigation. `biquad` 0.6 is the fallback.
- **Decision 6 adds a user-visible mode whose readings are not comparable to the other two.** A
  Z reading against a dB(C) ceiling is meaningless. Mitigated only by the spec explaining what Z
  is for; there is no runtime guard, consistent with everything else here.
