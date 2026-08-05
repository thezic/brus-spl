# The spectrogram half of the bridge

Parent: [SPL Meter Build](../map.md)
Type: build
Status: open
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
