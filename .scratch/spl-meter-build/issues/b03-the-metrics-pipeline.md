# The metrics pipeline

Parent: [SPL Meter Build](../map.md)
Type: build
Status: resolved
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

## Resolved (2026-08-06)

`src-tauri/src/metrics.rs`, 18 tests, all synthetic and deterministic. **`cargo test` is 39
green**, `cargo clippy --all-targets` and `--target aarch64-apple-ios --lib` both clean,
`cargo fmt` applied to the whole tree for the first time — `b02` is resolved, so the reason `b01`
held off is gone and nothing outside this ticket's files moved.

Every §14.2 case is covered and each is named after the thing it refuses to let happen. Nothing
in spec §6 needed correcting; the one place the spec's *reasoning* is wrong while its *rule* is
right is finding 1 below.

### Findings

1. **Spec §16.7's rule is right and its rationale is wrong, and following the rationale would
   have shipped a decaying number to a denied microphone.** §16.7 justifies the not-positive
   clause with "exact-zero blocks arrive, so the staleness rule never fires" — which presumes a
   zero-power block still counts as an *arrival*. Follow that and the smoother is advanced with
   `x² = 0`, so NOW decays exponentially rather than vanishing: from a quiet room's mean square
   it takes **~700 τ — about 11 minutes at `S`** — to underflow far enough for the clause to
   fire. That is precisely the "plausible, wrong, unlabelled number" §6.9 exists to prevent, and
   it contradicts this ticket's own `--` beside `0s of 60s`. §6.3's *"zero power is the absence
   of a measurement, not a quiet one"* is the sentence that resolves it: a zero-power block is
   discarded **whole** — no ring, no smoother, and **not an arrival**, so `last_block` is
   untouched, the 200 ms rule fires as designed, and the meter lands on `--` in a fifth of a
   second. The not-positive clause is kept and is not redundant: `smoothed` is exactly `0.0`
   before the first block and after nothing else, which is the case that would otherwise put
   `10·log₁₀(0)` on the wire. **No veto — §16.7's clause stays; only its stated reason is
   unreliable.** Nothing else in §16 is touched by this tier.

2. **The whole module is deterministic because it never reads the clock.** Every entry point
   takes `now`. That is what makes §6.2's clock-driven ring testable at all — a 130 s session
   with a 60 s hole runs in microseconds with no hardware, no sleeping and no fake-clock trait.
   `b05` passes `Instant::now()` from the tick and is the only place real time enters.

3. **Mutation-tested, and it found a real hole in my own tests.** Four deliberate breakages, all
   caught: L_eq divided by the nominal window length (**6 tests**), zero-power blocks deposited
   (1), the ring advancing without clearing what it passes (1 → **2 after a fix**), and the max
   compare moved from the drain loop to the tick (4). The third is the interesting one:
   `the_ring_keeps_moving_with_no_audio_at_all` passes on a ring that **never clears a slot**,
   because at 70 s nothing has wrapped yet — it was testing the window's *slicing*, not its
   *clearing*. The two are different properties and only a session longer than the 120 s ring
   tells them apart, i.e. a talk. `a_gap_clears_the_slots_the_ring_has_already_wrapped_through`
   now covers it. Two mutations are still caught by a single test each, which is thin but is
   exactly the pair §14.2 names.

4. **Verified against live audio, not only synthetically.** Temporary instrumentation drained
   the real queue into the ring under `npm run tauri dev` for two minutes: coverage climbed
   1 s per second from `1.09` to `60.10` and then **held at 60.0 ± 0.1** — the ±1 slot the tests
   encode as tolerance. L_eq settled to ~−45 dB and moved ~0.1 dB/s, which is §6.10's "the digit
   changes about once a second on its own" measured rather than predicted. NOW ranged −40 to
   −53 dB over the same interval and the max hold rose monotonically, −44.5 → −41.6 → −39.1, and
   never fell. Drains were 4–5 blocks per 100 ms poll, matching `b01`'s 46.875 blocks/s. The
   instrumentation was removed; `lib.rs`'s only diff is the module declaration and one doc line.

5. **A rate change re-reads old samples against the new rate, and that is spec §6.11's row
   rather than a defect.** Coverage is `Σn / f_s` with the *current* `f_s`, so energy summed at
   48 kHz is divided by 44.1 kHz for up to one window length after a route change. The row says
   a rate change clears neither the window nor the max, the error decays out within a window,
   and the alternative is a third reset cause. Written down here because it looks like a bug in
   the code and is not. `row_sample_rate_change_clears_neither` asserts Σn survives, which is the
   honest form of the claim.

### The weighting seam is closed

`capture.rs`'s callback now runs `chain.process(x)`. The chain is derived in `build()` from the
**granted** rate — the only place that rate is known — at `C`, the spec §11.4 default. This is
why the ticket was blocked by `b02` at all: `metrics.rs` itself needs nothing from `weighting.rs`,
since it consumes an already-weighted `sum_sq`.

Two things left deliberately for `b04`, which owns the setting that drives them: swapping the
chain on a live stream, and zeroing the filter state when it does (§6.11's third column). The
chain holds a `Vec<Biquad>`, so a swap has to hand the old one back out to be dropped off the
audio thread rather than freeing it in the callback — machinery with nothing to drive it until
there is a weighting *setting*. A rate below 2 kHz is now refused by name rather than panicking
the capture thread, which is what `WeightingChain::new` would otherwise do.

### What `b04` and `b05` inherit

- `Metrics::leq_over(seconds)` is public, so spec §8.3's fixed 10 s calibration slice is a call
  rather than a second accumulator — the payoff §6.5 promised for allocating the ring at
  maximum length.
- **No offset enters this module.** Every number is raw dB re FS, so §6.11's bottom row — a
  calibration change clears nothing — holds by construction and has no code path to test.
- `Levels { now, leq, max, coverage_s }` is what §9.1's `meter` block is built from, minus the
  offset, the unit and the `input` state.

### Verification run

| command | result |
|---|---|
| `cargo test` | **39 passed, 0 failed** (18 new, 21 `b02`'s) |
| `cargo clippy --all-targets` | clean, no warnings |
| `cargo clippy --target aarch64-apple-ios --lib` | clean, no warnings |
| `cargo fmt --check` | clean, whole tree |
| `npm run build` | pass (`vue-tsc --noEmit` + `vite build`) |
| `npm run tauri dev` on macOS | capturing and metering; finding 4 |
| mutation pass, 4 breakages | all caught; finding 3 |
