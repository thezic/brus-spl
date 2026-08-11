# The picture's frequency axis

Parent: [SPL Meter Build](../map.md)
Type: build
Status: open
Blocked by: —

Labels: `wayfinder:build`

## The work

Two changes to the spectrogram's chrome, from [the venue run](b13-the-venue-run.md), and they want
doing **together** because the first one pays for the second.

1. **Remove the colour legend.** The plot reclaims the 58 px right gutter.
2. **Label the frequency axis more densely.** The reclaimed width is what makes room for it.

Neither touches the columns, the band layout, the colour ramp or the calibration. This is the
visible canvas's chrome only — which `b11` established is redrawn just when a labelled setting or
the geometry changes, so the append-and-scroll path is untouched.

### 1 — the legend goes, and it is the owner's call

*"I own the app. I say it adds no value, we can use the horizontal space better."*

**This reverses a made decision, and the reversal is legitimate because the argument is new.** `09`
d9 and §11.7 both say *"the legend stays"* — but what `07` offered and `09` declined was the trade
*"if **space** forces the legend out, the picture loses its absolute meaning"*, declined because
space did not force it (*"the legend costs width, not the vertical calm the layout is built on"*).
The objection now is not about space; it is that the legend **delivers no value**. That was never
argued either way, so there is nothing here being re-litigated. Do not re-open it in code.

**What it costs, and what it does not:**

- **§7.1's fixed dB window is unaffected in substance.** Nothing auto-ranges, so the same colour
  still *is* the same absolute level — *"a quiet room and a loud one"* still cannot look identical.
  What goes is the only thing on screen **naming** those levels.
- **§7.2 survives intact, which is why this is cheap.** `b11` put `unweighted` on the **caption
  line**, not on the legend, so the guard against comparing the picture band-by-band against a
  dB(C) reading keeps its place. **Do not remove that word while removing the legend** — it is the
  one sentence §7.2 calls *the thing that must be said out loud*.
- `legendEnds(settings)` in `src/spectrogram.ts` becomes dead. Delete it rather than leave it —
  `noUnusedLocals` is on, and a dead exported helper is how a reverted decision creeps back.
- The caption line is currently **right-aligned to the canvas** and `b11` sized it around
  `dBFS/band` needing 58 px where `07` budgeted ~52 and overprinted the `now` label. With the
  legend gone that constraint changes; **re-check the caption line does not now collide with
  anything**, at phone widths and in the wide reflow.

### 2 — denser frequency labels

The venue verdict on the axis was *"isn't granular enough"*, alongside *"higher granularity would
be good"* for the bands themselves. **The bands are not what changes here** — see the warning
below. What changes is how many of the 32 rows get a printed label in the ~40 px left gutter
(§11.7).

Pick the density from what fits legibly at phone widths, in a **dim** room and at arm's length,
since `b13` confirmed that is the real reading condition. §7.1's nominal centres are the label
values (12.5, 16, 20, 25, 31.5, 40, 50, 63, 80, 100, 125, 160, 200, 250, 315, 400, 500, 630, 800,
1000, 1250, 1600, 2000, 2500, 3150, 4000, 5000, 6300, 8000, 10000, 12500, 16000 Hz) — the existing
label set is a subset of these and the change is which subset. **Band edges are drawn crisp**
(§7.3); labels must line up with the band they name, not with a pixel boundary that happens to be
near it.

### Do NOT increase the band count

`b13`'s finding 3.2, restated here because this ticket is where the temptation lands: going finer
than one-third-octave is **not** the cheap way to serve "pinpoint problematic frequencies", and it
would undo an answer the venue run just gave.

§7.1 chose 32 rows partly so *"the colour scale stops moving"* — energy-summed rows scale with row
bandwidth, and *"24 rows read ~10 dB hotter than 220 on the identical signal"* (`07` f3, f4). Every
halving of band width moves every cell by ~3 dB, which would **re-derive `−90 … −30 dBFS`**, the one
parameter `b13` confirmed is correct. It also breaks `b09`'s test property that the 32 bands sum
back to the signal's own unweighted dBFS. **If a finer picture is genuinely wanted, it is a design
question and not this ticket.**

### The spec corrections this carries

- **§11.7** — *"The legend stays"* and the **58 px right** gutter both become false. Record the
  reversal, its argument, and the consequence that §7.1's absolute-level property remains true of
  the data while no longer being readable off the screen. Update the gutter budget.
- **§11.7** again for the label density, if the number of labels is part of its geometry.
- **§17** — the correction log, per `b09` (§16.4) and `b11` (§7.3).
- `07` d9 is not edited; the mvp map is closed and its tickets are the record of what was decided
  *then*. §11.7 is where the current truth lives.

## Done when

- The legend is gone, the plot is wider by that gutter, and the caption line still fits — checked at
  phone widths **and** in the wide reflow, per `b06` finding 2.
- `unweighted` is still on the caption line.
- The frequency axis carries more labels, each aligned to its band.
- `npm run build` typechecks; `legendEnds` is deleted, not orphaned.
- **Looked at, not just diffed.** `git checkout prototype/b11-spectrogram-harness && npm run dev`,
  then `/harness.html`, drives the real component with synthetic pink / sweep / gap / ramp columns
  and a faked `invoke` — that is where §14.3's eyeball tests were run and it is the right place for
  this. Then confirm against a real room under `npm run tauri dev`.
- A device pass is **wanted**: the labels have to be legible in a dim room at arm's length, which is
  the condition `b13` established and the desk cannot reproduce. The provisioning profile expires
  **2026-08-13 15:25 UTC**; after that, re-sign per
  [`b12`](b12-re-sign-and-install-rehearsal.md) first.
- Spec §11.7 and §17 updated per above.
