# The metrics pipeline

Parent: [SPL Meter Build](../map.md)
Type: build
Status: open
Blocked by: [`b02`](b02-the-weighting-filters-and-their-test.md)

## Build

`metrics.rs`: the ring, coverage, the F/S smoother, the max hold, and Reset. Spec
[§6](../../spl-meter-mvp/spec.md#6-level-metrics) in full, tests per
[§14.2](../../spl-meter-mvp/spec.md#142-the-metrics-pipeline).

**The one idea the whole ticket rests on: the ring is advanced by the monotonic clock, not by
arriving samples.** The audio callback merely deposits into whichever slot is current when
drained, so **slots nobody deposited into are gap slots by construction** — and gap accounting
becomes a consequence of timekeeping rather than a feature that has to enumerate causes.

This is not a nicety. `11` probe 2 measured Siri killing the stream permanently 3 s into a 30 s
run **with not one error reaching Rust**. A sample-driven ring stops moving when the samples
stop, so it would have reported `60s of 60s` over a stream that died half a minute earlier.

**The ring** (§6.3):

- One slot is **100 ms**, holding `(sum_sq: f64, n: u32)` — sum-of-squares *and* count, never a
  mean, because §6.1 divides by the actual sample total.
- **Allocated at the longest permitted window, always**: 1200 slots ≈ 19 KB, permanently.
- Blocks are assigned **whole** to the current slot, so placement is approximate to ±21 ms
  while the coverage arithmetic stays **exact** — it counts samples, not slots.
- **Re-sum the window every tick** rather than maintaining a running add-and-subtract. 600 f64
  adds at 10 Hz is nothing, it removes any drift question over an eight-hour session, and
  coverage falls out of the same loop free.
- **A zero-power block is discarded at drain time, not deposited.** A denied microphone
  delivers callbacks of exact zeros, so `Σn > 0` while `Σp² = 0` leaves the level undefined —
  which put `60s of 60s` beside `--` on the prototype, the coverage figure claiming a complete
  minute while the meter said there was nothing. **Zero power is the absence of a measurement,
  not a quiet one.** Side benefit: no f64 `−∞` ever reaches `serde_json`.

**L_eq** (§6.1): `10·log₁₀(Σ sum_sq / Σ n)` over the window's slots — **never divided by the
nominal window length**. Folding a gap in as silence costs −2.6 dB for a 27 s hole in a 60 s
window, and the direction is *under*-reading against a ceiling. A consequence worth noticing:
**the first 60 seconds stops being a special case** — a filling window is just a window with
low coverage, so there is no warm-up state, no blank screen, no separate code path.

**Coverage** (§6.4): `Σn / f_s` over the window slots, published **always** — not conditional
on being degraded. An indicator that appears only when something is wrong is a warning.

**Window length** (§6.5): 10 / 30 / 60 / 120 s, default 60, and changing it **re-slices the
ring — no reset**. The ring holds valid energy at any length, so the setting is nothing but
*how far back do I sum*. `60 → 120` must read `60s of 120s`, a genuine 120 s average of
everything there is.

**NOW** (§6.6): a one-pole low-pass on the **squared** weighted pressure,
`y += (x² − y)·(1 − e^{−dt/τ})`, `τ_F = 125 ms`, `τ_S = 1 s`, **S default**. Advance it **per
block by that block's own `dt`**, which makes it mathematically identical to advancing it
continuously however irregularly the tick fires. A τ change **does not reset** the smoother —
the state is a smoothed mean square either way. The L_eq accumulator is **untouched** by this
setting, not even as a flicker (clause 3.9 NOTE 3).

**Max hold** (§6.7): the maximum time-weighted level, **compared inside the drain loop at block
rate (46.875/s)**, not sampled from the smoother at 10 Hz — which would drop four peaks in
five. Store the maximum *linear* smoothed mean square and publish `10·log₁₀(max) + offset`,
correct because `max(xᵢ + c) = max(xᵢ) + c`.

**Reset** (§6.8): **one** button, clearing the max hold *and* the window. It is not "clear the
max", it is *"start measuring this talk"* — between talks the room is not quiet, and applause,
chatter and PA music otherwise sit in the window through the first full minute of the next
talk, reading high against the ceiling exactly when the new speaker is being judged.

**Where the app deliberately shows nothing** (§6.9): NOW is `None` when no block has arrived
for **200 ms** *or* when the smoothed mean square is not positive; L_eq is `None` at zero
coverage. **Max hold is unaffected by either** — a hold is historical, so it simply stops
rising.

## Traps

- The 200 ms rule alone is not enough, and this is the correction `10` found: **exact-zero
  blocks do arrive**, so blocks keep coming and the staleness rule never fires. Without the
  not-positive clause, `10·log₁₀(0)` reaches `serde_json` (§16.7).
- **Every row of §6.11's table**, which is cheap to test and is exactly where a
  plausible-looking implementation goes wrong. Several rows read as bugs unless you have read
  the reasoning: an F/S change clears the max but **not** the window; a rate change clears
  neither; a calibration change clears **nothing at all**.
- No F/S time weighting inside L_eq. Ever.

## Done when

`cargo test` green on §14.2's cases, all synthetic and deterministic:

- a known steady tone for `T` seconds gives a known L_eq, with coverage reading `T of window`
  while filling;
- **feed nothing for 27 s of a 60 s window** ⇒ `33s of 60s` with the L_eq of the real 33 s, not
  the −2.6 dB that folding gaps in as silence gives;
- **feed exact zeros** ⇒ `--` beside `0s of 60s`, not `60s of 60s`;
- **no block for 200 ms** ⇒ NOW is `null`, max hold unchanged;
- **every row of §6.11's table**;
- `60 → 120 s` re-slices to `60s of 120s`, not `0s of 120s`.

(The offset-change case belongs to
[Settings, persistence and calibration arithmetic](b04-settings-persistence-and-calibration.md).)
