# The screen

Parent: [SPL Meter Build](../map.md)
Type: build
Status: resolved
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

## Resolution

**Built, and every Done-when item driven rather than described.** `App.vue` plus `Hero`,
`Secondary`, `InputLine`, `Picker` and `SettingsSheet` under `src/components/`, with the four
strings that sit beside a number in `src/display.ts`. Spec §11 needed no correction. 91 tests
still green, `npm run build` typechecks, clippy clean, and the iOS `--lib` check passes.

**The dev window cannot be clicked from an agent session** — no accessibility grant, so no
synthesised events — which turned the trap's own suggestion (*drive the input-state line from a
stubbed value*) into the method for the whole pass. Two temporary harnesses, both deleted before
the commit: a fake `window.__TAURI_INTERNALS__` so the real components could be clicked in Chrome
at phone widths, and a timed script that pressed the six commands against the **real Rust
backend** in the dev window. The split matters, because each caught what the other could not. The
browser is where the sheet was exercised — the `683` typo rejected with its reason on screen,
`68.3` matched, the offset landing at `106.4` and becoming editable, the armed Reset, and all
three input-state lines including the `denied` one macOS cannot produce. The dev window is where
the **labels** were proved against real settings: one frame reading `NOW · A · fast` over
`LAeq 120s` over `2s of 120s` in `dB` is `set_weighting`, `set_time_weighting`,
`set_window_length` and `set_calibration_from_reference` all confirmed at once, and the
`14s → 2s of 120s` step across two frames is Reset clearing the window.

**Finding 1 — the hero size is a measurement, and guessing it was wrong by a whole rem in the
dangerous direction.** §11.5 says *size the primary number for its widest state*; it does not say
what that costs, and the arithmetic is not the one a reader would do. `−108.4` is **3.09 em**
wide in the system sans — not the ~3.3 em a tabular figure set suggests, because SF Pro's display
digits are ~0.44 em — so a flat `7.5rem` renders it at **371 px** and overflows a 375 px phone,
while the `6.5rem` that fits a 375 px phone throws away 15 % of the glyph height on a 430 px one.
Neither is a size; the constraint is `font-size ≤ (100vw − 32px) / 3.09`, and the hero is now
`min(7.5rem, calc(31vw − 12px))` — checked at 375 / 393 / 402 / 430, clearing by 20 px at the
tightest. The `min()` cap is what hands over to the 26 rem width cap once the viewport stops being
the binding constraint. **This is why the widest state is worth stating as a *number*: it is
invisible at the desk, where the app is calibrated within a minute of launching, and it appears
on the one screen the app shows before anyone has calibrated it.**

**Finding 2 — the wide reflow's column is sized by the hero, not by the picture.** `vw` in that
clamp is the whole window, so at 800 px the hero is at its 7.5 rem cap while the column it sits in
is whatever the grid says. A comfortable-looking `minmax(15rem, 20rem)` would have clipped
`−108.4` in exactly the state §11.5 named — the reflow's left column is now `minmax(23.5rem,
24rem)`, which is the 371 px measured above plus slack, leaving ~352 px for the picture at 800×600.

**Finding 3 — the accent's two uses point in opposite directions from the convention, and the
spec is right.** §11.8 says the accent marks *states and affordances, never levels*, and names the
armed Reset as one of its two uses. The reflex is a red confirm; red is the alarm colour the same
sentence rules out. It is cyan — also chosen because the inferno ramp runs black → purple → red →
orange → yellow, so a warm accent is a colour the picture will later use for a *level* and a cool
one can never be. Reset is not dangerous anyway: §6.8's window refills honestly in a window length.

**Two smaller calls made rather than left to read naturally.** Coverage is **rounded, not
floored** — `b03` measured a full window at 60.0 ± 0.1, so flooring flickers `59s`/`60s` forever
on a window that is genuinely full, which is §11.8's objection to proportional digits arriving by
a different route. And a command's answer **outranks the tick for 120 ms**: §9.2 says picker
feedback comes from the return value, but a tick emitted in the moment before the command applied
is still in flight and arrives carrying the *old* settings, snapping the segment back for one
frame. Three lines, and Rust stays authoritative either way.

**`capture_diagnostics` is deleted**, as `b05` wrote into its own doc comment. Not a close call
once `session.rs` was read: its `log_mismatches` already says *nothing here reaches the UI (spec
§3.1) … acceptable precisely because none of these are conditions the user can act on*. The
consequence for [the Tier 1 device pass](b08-tier-1-device-pass.md) is real and worth naming — the
**`Measurement`-mode read-back is the highest-consequence check in the effort** (21 dB, silently)
and is now only in the log, which under that ticket's `devicectl` loop means launching from Xcode
instead. Re-adding a temporary readout is a five-minute move if that turns out to be awkward.

**Not verified live, and said so rather than glossed:** the hero's `--` was rendered at its real
size and typography in the browser and does **not** read as a redaction bar — two clearly separate
dashes, muted, a third the height — but it was never produced by a *stream that stopped*, because
nothing here can stop one. Rust's side of that path is `b03`'s and tested. The sheet has only been
seen in portrait; at phone-landscape height it scrolls, which is the mechanism rather than an
observation. And the reserved picture band is **empty, not outlined** — Tier 2 is the droppable
tier, so an outlined placeholder is the thing that would ship.

## Correction, from `b07`'s device pass

**The calibration fields could not be typed into at all on a comma-locale phone**, which is the
one class of defect this ticket's method was structurally unable to find: both halves of the pass
ran against a Mac keyboard, which has a `.` key.

iOS gives an `inputmode="decimal"` keypad the **locale's** decimal separator and no other — so on
that phone there is no `.` at all, `Number("68,3")` is `NaN`, and `typed()` returned `null` for
every decimal the reader could actually produce. Match and Set then sat permanently disabled with
nothing on screen to say why: precisely the silent no-op `apply`'s own doc comment calls *the worst
of the three possible behaviours*, arrived at from the one direction that bypasses it.

It is not merely awkward. It **breaks spec §8.5's recovery path outright** — the offset is meant to
be recoverable by reading it off a sticky note and retyping it, which free provisioning's weekly
re-sign makes a real routine rather than a hypothetical, and a keypad that cannot type the number
the app displays makes that impossible.

`typed()` now accepts either separator and refuses **two** rather than guessing (`1,234` is
ambiguous between a thousands group and a decimal). Display stays on `.` everywhere: a hero reading
`68.3` above a field reading `68,3` would be a worse inconsistency than the one being fixed. Checked
against 17 cases including `683`, which still reaches the backend and is still rejected with its
reason on screen, then confirmed on the device.
