# The spectrogram half of the bridge

Parent: [SPL Meter Build](../map.md)
Type: build
Status: resolved
Blocked by: [`b09`](b09-spectrum-analysis-and-the-column-ring.md)

## Build

Fill in `columns`, add the seventh command, and wire the re-pull rules. Spec
[§9.1](../../spl-meter-mvp/spec.md#91-the-wire-contract-verbatim),
[§9.2](../../spl-meter-mvp/spec.md#92-properties-worth-stating-rather-than-leaving-to-be-inferred)
and [§9.5](../../spl-meter-mvp/spec.md#95-what-makes-the-frontend-re-pull).

Small ticket. Two pieces:

**`columns` in the tick.** `[ { slot: 418752, bands: [...32] } ]` — **every real column since
the last publish**, each carrying its **absolute slot index**.

That is required for **correctness**, not robustness: §6.2's clock-advanced ring means a late
tick — a timer on a phone — has two or three genuinely completed slots behind it, and a
one-column payload drops real data on the floor.

**Gaps therefore need no marker.** A slot index in `(last_drawn, now_slot]` with no column *is*
§7.3's hole. Explicit `bands: null` entries were rejected as duplicating what the indices
already say, at 270 wasted entries for a 27 s hole — and `09` finding 8 confirmed there is no
gap-marking branch anywhere in the prototype.

**`get_spectrogram() → Column[]`**, returning the per-slot columns for the **current span**.
This is the only bulk payload in the design: ≈138 KB at a 60 s span, ≈276 KB at 120 s, sent
over the `ipc://localhost` custom protocol rather than by `eval`, once per redraw-from-scratch.

Letting the frontend keep its own ring was rejected on the dev loop: the picture would be empty
for up to two minutes after every reload.

**The re-pull table**, verbatim:

| event | re-pull `get_spectrogram()`? |
|---|---|
| mount / webview reload | ✓ |
| canvas resize, orientation change | ✓ |
| window length change (10/30/60/120 s) | ✓ |
| calibration offset change | **—** |
| Reset button | — |
| weighting change (C/A/Z) | — |

## Traps

- **The offset row is arithmetic, not an oversight.** §7.1 shifts the dB colour window *by the
  offset* and the band values shift with it, so every colour is unchanged and only the legend
  relabels. **Calibrating is the one settings act that changes every number on screen and no
  pixel of the picture.**
- **Reset does not clear the picture**, and this must be stated in the code's comments or it
  reads as a bug. Settled on asymmetry of cost: the number refills honestly in a window length,
  but **60 s of rumble stripe cannot be recovered once wiped**, and the stripe is the entire
  reason the picture exists. The consequence, plainly: *shortly after a Reset the number
  describes this talk and the picture describes the last 60 s of the room.* Both are labelled,
  and `07`'s "the picture is literally what is inside the number" must be read as *the same
  span*, not *the same data*.
- **Columns produced between page load and `listen()` registering are lost** no matter what the
  transport is — which is what the mount re-pull is for.
- The tick must stay on the fast path. It was ≈460 bytes empty; 32 `f32` per column is
  ≈1.3 kB/s at one column per tick, still far under the 8192-byte bulk threshold. A tick that
  ships ten columns after a stall is the exception, and correctness beats the fast path there.

## Done when

Under `npm run tauri dev`: columns arrive with monotonic slot indices, a deliberately stalled
frontend receives the backlog rather than one column, `get_spectrogram()` returns the current
span, and stopping the audio produces **absent** indices rather than zero-valued columns.
`npm run build` typechecks; `cargo clippy` clean.

## Resolution

**Built and green: 122 tests, 8 of them new, and both halves driven live under
`npm run tauri dev` on the built-in microphone.** Clippy clean, `cargo fmt` applied, `npm run
build` typechecks, and `cargo check --target aarch64-apple-ios --lib` passes — nothing here is
iOS-only, but the check is cheap and a plain `cargo check` skips `session.rs` entirely. Spec
§9.1's wire block, §9.5's table and §7.1's span rule all needed no correction. **§9.2's one-line
description of `columns` did**, and that sentence is this ticket's whole value.

**Finding 1 — "every real column since the previous publish" loses exactly one column per stall,
and the loss is invisible from either side alone.** §9.2's phrase reads as *ship what this tick
filed*, which is what a cursor parked at `now_slot + 1` implements. `b09`'s own resolution already
records why that is wrong: **a tick that completes zero hops files nothing, and the next tick
fills that slot retroactively as `now_slot − 1`.** The retroactively-filed slot is therefore filed
*after* the tick that already reported it as the present, and a `now_slot + 1` cursor steps
straight over it. Nothing fails loudly — the column is in the ring, so `get_spectrogram` hands it
back — and the symptom is that **the picture disagrees with itself across a reload, one slot at a
time**: a hole that heals when you resize the window. The cursor is therefore
`(last shipped, now_slot]`, which is also the interval §9.2 already gives the *frontend* for
deciding what is a hole, so what Rust ships from and what the canvas draws into are one expression
rather than two that have to agree. Spec §9.2 and §17 carry the correction, and
`a_slot_filed_after_its_own_tick_still_reaches_the_wire` is the test that discriminates the two.

**Finding 2 — the backlog is real on the desk, and it is eight columns, not two or three.** The
ticket predicts "a tick that ships ten columns after a stall is the exception". Stalling the tick
thread for 700 ms on purpose produced exactly that: **`now_slot` jumped 200 → 208 and the next
tick carried 8 columns in 3098 bytes**, with the cumulative invariant `now_slot − columns = 3`
(the three startup slots the FFT spends filling its buffer) **unchanged across the stall**. Not
one column was lost. 3098 bytes is still inside the 8192-byte fast path, so the trade the ticket
reserves — correctness over the fast path — was not actually spent at this stall length. It starts
costing somewhere past ~20 columns, which is two seconds of stall, well past where the ~1 s sample
queue would have overflowed and drawn honest holes anyway.

**Finding 3 — a stalled *frontend* is not the backlog case, and this ticket's Done-when conflates
them.** Blocking the webview's main thread for 2 s changes nothing about the tick: `app.emit`
queues and returns, the tick thread stays at a flat 10 Hz, and the frontend then receives every
queued tick each carrying its own single column. Measured: the widest payload stayed at **1**
across the whole frontend stall and the frontend's own gap count stayed at **0**. That is §9.2's
*no throttling or coalescing* clause behaving exactly as written — **wry delays ticks rather than
losing them** — and it means the multi-column payload is a *late tick* case only. Both were
exercised; only the second produces a backlog.

**Measured live, end to end, at 48 kHz mono.** The frontend was temporarily instrumented to count
what it received and reported, over ~390 ticks: `widest 8`, `gaps 0`, `monotonic true`, `edge lag
0`. `getSpectrogram()` called from the frontend returned **99 columns spanning slots 4..102, 32
bands each** — the first four slots absent, which is the startup frame-fill hole and therefore the
live demonstration of *absent indices rather than zero-valued columns* on both the tick and the
pull. An empty tick is 243 bytes and a one-column tick 596–607, against §9.1's ≈460 for the empty
case. Both probes were removed before commit.

**The pull's right edge is the ring's, not the clock's** — a call this ticket had to make and the
spec does not. `get_spectrogram` reads `Metrics::now_slot`, so a pull landing between ticks answers
as of the last one; reading the clock instead would hand back a right edge no tick has mentioned,
while the frontend is holding a `now_slot` from a tick. It also deliberately does **not** advance
the tick's cursor: a pull and the ticks around it overlap, a column can arrive both ways, and
drawing a slot twice draws the same thing twice — whereas suppressing the overlap would punch a
hole in whatever the pull itself did not cover.

**The re-pull rules are in `src/bridge.ts`** as `getSpectrogram`'s own doc table plus
`pictureNeedsPull(previous, next)` — one named predicate that returns true for a window-length
change and nothing else, so §9.5's three "—" rows cannot be re-litigated at a call site. The **DOM
half is [`b11`](b11-the-spectrogram-canvas.md)'s**, stated rather than skipped: mount and the
resize observer both need a canvas to hang off and there is none yet, so nothing in the app calls
the command today.

**Untested and said so.** Nothing has run on the phone. The one property that could plausibly
differ there is the stall length, which is the same unmeasured iOS drain jitter §6.9's 200 ms
threshold is still a guess about — a longer stall means a wider payload, and past ~20 columns the
tick leaves the IPC fast path. The live gap evidence is the startup frame-fill hole, not a stopped
stream: killing capture mid-run to watch the wire go absent was not exercised, and the unit tests
carry that leg the way `b09`'s carried the ring's. And **the picture is still not drawn** — `b11`
is where any of this is looked at.
