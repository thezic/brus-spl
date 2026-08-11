# The live number is too busy

Parent: [SPL Meter Build](../map.md)
Type: build
Status: open
Blocked by: —

Labels: `wayfinder:build`

## The work

[The venue run](b13-the-venue-run.md) answered *"does the hero number feel right on real speech at
`S`?"* with **no — it is too busy**. This ticket takes the escape the spec already wrote for exactly
that answer.

**Nothing here is a decision.** §11.4 names per-quantity display resolution as *"the **escape** if
the hero reads as too busy on real speech"*. It was rejected at the time as *"a second rule for a
problem S already handles"* — and `b13` established that S does not handle it. The condition has
fired; implement the escape.

### Why coarsening the live number is the only move left

Worth having in front of you, because the obvious alternatives are both closed:

- **A slower time weighting does not exist.** `S` is already the slower of the two standard
  weightings, and §11.4 establishes *"S is the lever; resolution is not"* precisely because at `F`
  the hero moves **1.72 dB per tick** (§6.6) — further than any rounding step, so no display
  resolution can calm `F`. The lever is already at its limit.
- **Uniform coarsening stays rejected**, for §11.4's own reason: resolution as one setting for all
  three numbers would **spend a tenth on the ceiling-judged L_eq in order to fix a different
  number**, and the L_eq is already perfectly stable at 0.1 dB. Tenths matter there.

So: **tenths on the L_eq and the max hold, something coarser on the live value.** How much coarser
is this ticket's one open question — §11.4 says only "something coarser", and §6.6's figures are the
evidence to pick from. 1 dB is the obvious candidate; 0.5 dB is available if 1 dB reads too blunt.
Pick it from the numbers, then look at it.

### What to be careful of

- **`display.ts` is where the strings that sit beside a number live** (`b06`). Resolution is
  formatting, so it belongs there and not in the metrics pipeline — the wire keeps full precision,
  which is what lets the L_eq stay at tenths from the same tick payload.
- **`--` and `dBFS` are typographic states** (§11.5). Whatever coarsening does, it must not change
  how those render, and the hero is still **sized for its widest state** — which is `−108.4`, a
  calibration-less value at tenths, so **coarsening the live number does not license shrinking the
  hero**. `b06` finding 1's `min(7.5rem, calc(31vw − 12px))` is a measurement and stays.
- **Display resolution is still not a setting** (spec §1016, §11.4). This adds a second rule, not a
  control.

### The spec correction this carries

§11.4 currently reads *"Display resolution stays 0.1 dB for all three numbers"* and *"Per-quantity
resolution was not taken"*. Both become false when this lands. Update **§11.4** to record that the
escape was taken, why (`b13`'s answer 2), and what the resolutions now are; and add the correction
to **§17**. The line at spec §1016 (*"Display resolution is 0.1 dB and is not a setting — see
§11.4"*) needs its first clause fixed and its second left alone.

## Done when

- The live number reads calmer on real speech, and the L_eq and max hold still read tenths.
- `cargo test` green, `cargo clippy` clean, `cargo fmt` applied. This is frontend-only, so
  `src/bridge.ts` and the Rust side should not need touching at all — if they do, question why.
- `npm run build` typechecks.
- Driven under `npm run tauri dev` against a real microphone and **looked at**, not just diffed —
  the whole finding is about how a number *feels* while it moves.
- Spec §11.4, §1016 and §17 updated per above.
- A device pass is **wanted but not required to close**: the finding came from a phone at a venue,
  and the desk cannot reproduce the room. Note the provisioning profile expires **2026-08-13
  15:25 UTC** — after that, any device check needs [`b12`](b12-re-sign-and-install-rehearsal.md)'s
  re-sign first (delete the cached profile, rebuild, ~48 s).
