# The screen

Parent: [SPL Meter Build](../map.md)
Type: build
Status: open
Blocked by: [`b05`](b05-the-bridge-and-the-tick.md)

## Build

`App.vue` and `src/components/`: the one screen, minus the picture. Spec
[§11](../../spl-meter-mvp/spec.md#11-screen-layout-and-presentation).

```
        NOW · C · slow        ← both dimensions; they govern the live number and MAX
           65.5
            dB
   LCeq 60s        MAX
     66.5         67.0
   60s of 60s                 ← coverage belongs to the L_eq and sits with it
   [ the spectrogram goes here — b11 ]
             ⋯                ← settings · calibration · reset
```

*The screen leads with the number that moves and keeps the number that is judged permanently
in view beside it.*

**The hero is NOW, the live level — not the L_eq** (§11.1). This inverted `09`'s own premise,
on evidence: a 60 s L_eq moved **0.0 dB over 8 s** of steady talk and never changed a digit at
any display resolution. At hero size it is dead screen.

**The cost, which the screen must not hide: the number that dominates is no longer the number
judged against the 70 dB ceiling.** Nothing is concealed — the L_eq is on screen with its
coverage — but the emphasis and the purpose point at different numbers, and the live value sits
several dB above the L_eq much of the time. This is the largest accepted risk in the layout.

**Nothing is behind a tap** (§11.2). The L_eq **with its coverage** and the max hold are always
visible. The no-chrome prototype hid them to test whether they were desk curiosity; they are
not.

**The hero carries a quantity label** (§11.3): `NOW · C · slow`. One header serves the live
number *and* the max hold, because §6.7 ties the max's meaning to the same weighting /
time-weighting pair — so a bare `MAX 72.4 dB` never arises. The L_eq's own label carries the
weighting and the window: `LCeq 60s` / `LAeq 30s` / `LZeq 120s`.

**`--` and `dBFS` are typographic states, not merely strings** (§11.5). A spec that says "show
`--`" and stops **gets a redaction bar**: at 7–9 rem two dashes render as a pair of solid
blocks.

- **`--` is muted and much smaller than the number it replaces.** This matters more than it
  used to: with the hero being NOW, `--` appears after 200 ms of no audio, where a hero L_eq
  would have shown it only at zero coverage. **That state went from rare to common.**
- **Size the primary number for its widest state, not its most common one.** `−108.4` is wider
  than `66.7`, and **the uncalibrated state is the one the app starts life in**.
- Both `dB` and `dBFS` must look **deliberate**.

**The input-state line** (§11.6) — a label, and an action only when there is one. A single
string made the two states look like the same kind of message, which is the shape that teaches
people to ignore messages.

| `input` | line |
|---|---|
| `capturing` | *(nothing)* |
| `denied` | **`No microphone access`** over **`Settings ▸ Privacy & Security ▸ Microphone`** |
| `unavailable` | **`Microphone unavailable`** — label alone |

Styled in the same family as the unit label and the coverage figure, **never as a banner**.

**Typography and colour** (§11.8):

- **Every number on screen is tabular-figured.** At 10 Hz a proportional digit set makes the
  whole number shuffle sideways when one digit changes, which reads as instability that is not
  in the measurement.
- **The hero is set in the UI sans, not monospace** — at 7 rem a mono font gives the decimal
  point a full advance width and `66.8` reads as block-dot-block. Everything smaller stays
  mono.
- **Dark, high contrast.** Not a free choice: §7.1 puts an inferno ramp on a near-black field.
- **The accent colour never means alarm.** It marks the input-state line and the armed Reset —
  states and affordances, never levels. The test that this holds: **at 73.5 dB the screen is
  identical to 66.7 dB apart from the picture being brighter.**

**Geometry** (§11.7): leave the ~205 px band under the number for the picture, with its gutters
budgeted — **~40 px left**, **~20 px bottom**, **58 px right** — so
[the canvas](b11-the-spectrogram-canvas.md) drops in without a relayout. **The portrait layout
is width-capped**, so a tall desktop window centres at phone width rather than stretching a
portrait stack across 1400 px. **One wide reflow at `max-height: 700px`** serves both
phone-landscape and the 800×600 dev window — that threshold is set by measurement, not phone
geometry: an 800×600 desktop window is a *landscape* case, shorter than a phone is tall, so a
breakpoint drawn at phone-landscape height (560 px) misses it and the portrait stack pushes
Reset below the fold. In the wide reflow the picture goes **beside** the number; arranging the
picture there is [the canvas](b11-the-spectrogram-canvas.md)'s job, the reflow itself is this
ticket's.

**The sheet** (§11.9) — one sheet from `⋯`, holding settings *and* calibration, because
calibration is a rare, deliberate, two-instrument gesture and folding it in keeps the main
screen at one affordance:

1. **Weighting** — C / A / Z picker.
2. **Time weighting** — F / S picker.
3. **Window length** — 10 / 30 / 60 / 120 s picker.
4. **Calibration** — the live 10 s reading with its coverage; numeric entry for the reference
   value; ±0.1 dB trim; **the offset shown as a number and directly editable**. Not decoration:
   free provisioning is reinstalled weekly and delete-then-install loses the data container, so
   a visible offset makes recovery a sticky note and a retype (§8.5).
5. **Reset** — **inside the sheet and a two-step (armed, then confirmed)**, because §6.8 makes
   it discard the window as well as the max hold.

## Traps

- **Nothing may be gated on `import.meta.env.DEV`** — a device build is a release build (§2.2).
- **Picker taps take their feedback from the command return value, not the next tick** (§9.2).
  Command returns exist for *feel*, not truth. If the controls feel sticky, that is the path to
  check. Text entry keeps local draft state — an input buffer, not a second source of truth.
- The frontend holds **no** authoritative state. Everything painted comes from the tick.
- All three settings pickers ship, not just calibration: §5.2 makes dB(A) selectability the
  reason a dB(C) default is tolerable at all, and the filters exist regardless.
- `"denied"` is **hard to exercise on macOS** — per `CLAUDE.md`, `tauri dev` gets microphone
  access through the responsible parent process, so the desk may report `capturing` where the
  phone reports `denied`. Drive the input-state line from a stubbed value to see it.

## Done when

Under `npm run tauri dev`: the meter reads and responds to sound; the hero shows `--` when the
stream stops and it does **not** look like a redaction bar; `dBFS` shows before calibration and
`dB` after; all three input-state lines render; every picker works and the sheet's calibration
entry stores an offset that moves all three numbers at once; the armed Reset clears the window
and the max hold. The portrait layout width-caps in a tall window and reflows at 800×600.
`npm run build` typechecks.
