# Plan: ticket `09` screen layout prototype

Ticket: [`09`](../issues/09-screen-layout.md) — Type `prototype`, Status `claimed`
Skill: `/prototype` → UI branch, sub-shape A (variants on the existing page)

## Confirmed with Simon before planning

1. **It gets onto the phone.** Browser pass first to narrow the field, then an embedded
   `tauri ios build` + `devicectl install` so the winner is judged on real glass at real distance.
   This is the one thing ticket `07` could not do — Wi-Fi client isolation kept it off the device —
   and it is `07`'s standing residual risk: 32 bands over ~200 px is **~6 px per band**, and whether
   that reads at arm's length in a dim venue is unanswered. It is also the same question that killed
   `07`'s variant C, so it deserves the build cycles.
2. **Portrait is primary; landscape must not break.** Landscape gets a sane reflow — the spectrogram
   beside the number rather than under it — but is not judged as a first-class layout. The same
   reflow covers the **800×600 desktop dev window**, which is the identical wide-and-short problem,
   so one answer serves both and the variants disagree about one thing rather than two.

## What the prototype has to answer

The ticket asks three different kinds of question, and the prototype separates them so that each is
answered by the right instrument:

| Kind | Question | How the prototype answers it |
|---|---|---|
| **Hierarchy** | what dominates, what supports it, what is on screen at all, where settings live | three structurally different **variants**, switched with `?variant=` |
| **States** | `--`, `dBFS` vs `dB`, `denied`, `0s of 60s`, a 27 s hole, max hold above 70 | a **scenario switch**, because audio cannot produce these on demand |
| **Calm** | display resolution, update feel | a **knob** (0.1 / 0.5 / 1 dB), shared by every variant |

Splitting states out is the part `07`'s plan did not need. Nearly everything `09` inherited is a
*designed state* — `05` decision 11's `--`, `06` decision 4's `dBFS`, `08` decision 8's `denied`,
`07` decision 11's holes — and a designed state that is never seen is a state that was never
designed. So each one is a scenario I can put on screen with one tap, in every variant.

## Ground the design starts from

Settled upstream, not reopened here. The prototype's job is to arrange these, not to re-litigate them:

- **The tick is the whole input** (`08`). One 10 Hz event carrying meter, settings and columns; the
  frontend holds no authoritative state. So every variant is a **pure render of one tick object** —
  which makes this prototype a free dry run of `08`'s contract: if a layout cannot be drawn from a
  tick alone, `08` decision 7 has a hole in it.
- **The spectrogram arrives with a geometry** (`07`): horizontal, time right→left, 32 fixed
  one-third-octave bands, ~200 px tall at full width, plus three gutters that are part of the design
  — ~40 px left for frequency labels, ~20 px bottom for the time axis, ~52 px right for the legend.
  Always unweighted, span follows the window, gaps drawn as holes.
- **The mode indicator is two-dimensional** (`05` decisions 7–8): weighting (C/A/Z) *and* time
  weighting (F/S), because the meaning of max hold depends on both. A bare `MAX 72.4 dB` is
  unreadable.
- **Coverage is unconditional** (`05`): `41s of 60s`, and `60s of 60s` when full. Never hidden when
  healthy — an indicator that only appears when something is wrong is a warning, which charting ruled
  out.
- **Reset discards the window as well as the max hold** (`05` decision 10), and **does not clear the
  picture** (`08` decision 5). So a mis-tap is expensive, and three seconds after a Reset the number
  and the picture legitimately disagree about how much history they hold.
- **Settings are Rust-owned and ride in every tick** (`08` decision 7), so a variant can never show a
  mode that is not running — and picker feedback comes from the command return value, not the next
  tick.
- **Calibration is numeric entry** (`06`): type what the proper meter reads, against a live 10 s
  reading with its own coverage, ±0.1 dB trim, and the offset displayed so it can be written down.
- **0.1 dB is the wire resolution** (`08`), which bounds what the resolution knob can explore: it can
  round further down, never up.

## The three variants

Each disagrees about **information hierarchy and where settings live** — not about colour, and not
about the spectrogram's internals, which `07` settled. All three get the same tick.

**A — Stacked instrument.** The straight reading of "the L_eq dominates, everything else supports
it". Portrait priority stack: the L_eq at the largest size that fits, its unit beside it, one caption
line carrying weighting · time weighting · coverage, then a row of two secondary readouts (NOW, MAX),
then the spectrogram full width with all three gutters, then Reset at the bottom out of thumb-drift
range. **Settings behind a gear**, calibration behind its own sheet from there. The conservative
option, and the baseline the other two argue with.

**B — Instrument panel.** Rejects the single-giant-number stack: the L_eq is the largest tile but
NOW, MAX and coverage are **peer tiles** in a grid, and the four settings are **inline segmented
controls** on the main screen rather than behind a sheet — the ticket's explicit alternative, tested
rather than dismissed. Calibration is still a sheet, since it is the one gesture that needs two
instruments and a keyboard. The spectrogram spans the bottom edge to edge. This is also the variant
most likely to survive landscape and 800×600 unchanged, so it doubles as the wide-layout probe. Its
bet: at a venue you want to change the window length without hunting for a gear. What it risks is
exactly what the ticket warns about — density beating legibility.

**C — One number, everything on demand.** Takes "legibility beats density" literally and is the only
variant that removes things. The main screen holds the L_eq, its unit, coverage, and the picture —
nothing else. **NOW and MAX are not on screen**; they are behind a tap. Reset is a deliberate
two-step. Everything else, calibration included, is behind a single sheet. Its bet is that at a venue
you read one number and glance at one picture, and that instantaneous level and max hold are desk
curiosity. That is a strong claim about `05`'s own output, and it is worth making badly on purpose:
if it fails it fails informatively, the way `07`'s variant C did.

**Shared, deliberately, and only these three:** the spectrogram canvas (`07` settled its guts; the
variants disagree about its size and placement), the calibration sheet's contents (`06` kept the
gesture to four things; where its entrance sits is what varies), and the tick source. Per
`/prototype`, a shared header is fine and a shared layout defeats the point — none of these is a
layout.

## The scenarios

Selected on the bar, applied to whichever variant is showing. These are the inherited designed
states, made visible:

| scenario | what it puts on screen | which decision it exercises |
|---|---|---|
| `steady` (default) | ~68 dB talk, full coverage, calibrated | the ordinary case |
| `cold` | `0s of 60s` climbing from launch | `05` decision 1 — a filling window is ordinary |
| `gap` | 27 s hole, then recovery: `33s of 60s` and holes in the picture | `05` decision 2, `07` decision 11, `08` decision 3 |
| `silent` | `--` on the instantaneous readout, number intact | `05` decision 11 |
| `dead` | `--` everywhere, `0s of 60s`, `now_slot` still advancing | `11` probe 2 — the failure that looked plausible |
| `uncalibrated` | every number labelled `dBFS`, legend `dBFS` | `06` decision 4 |
| `denied` | the input-state line, with something actionable | `08` decision 8 |
| `unavailable` | the input-state line with nothing to act on | `08` decision 8 |
| `hot` | L_eq 71.8, max hold 78.3 — above the 70 dB ceiling | that the layout does **not** turn into a warning |

`hot` is there to check a negative. The map's standing rule is no warnings, so the screen must look
the same at 72 dB as at 62 — and the only way to be sure is to look at it.

## Where the numbers come from

Two sources, and the split is deliberately about risk rather than richness:

- **`live`** (desk) — Web Audio, reusing ticket `07`'s engine wholesale from the
  `prototype/07-spectrogram` branch: `fft.ts`, `bands.ts`, `weighting.ts`, `sources.ts`,
  `colormaps.ts`, `paint.ts`. Real columns from the laptop mic or `07`'s synthetic
  rumble+speech / pink / sweep, with a rough `05`-shaped meter over the top (100 ms energy slots, an
  exponential F/S smoother, block-rate max compare, coverage). Rough on purpose: the numbers only
  have to *move like the real ones* for the layout to be judgeable. It also makes the picture and the
  number derive from the same signal, which is the coherence `08` decision 2 exists to protect.
- **`scripted`** (device) — pure TypeScript, no Web Audio at all. Generates both the meter values and
  the band columns from a scenario. This is what makes the device pass safe: **`getUserMedia` in a
  Tauri WKWebView is untested**, and a prototype whose device pass depends on it could fail for a
  reason that has nothing to do with the layout. Scripted mode also produces `denied` and `dead`,
  which no real microphone will produce on request.

Both sources emit the **same tick object**, typed once, with `08`'s field names verbatim in
snake_case. That file is a throwaway rehearsal of `src/bridge.ts` and worth keeping an eye on: if the
names read badly here, that is cheap information for `08` decision 9.

## Implementation notes

- **Sub-shape A**: `src/prototype-layout/`, mounted from `src/App.vue` on `?variant=A|B|C`. Without
  the param, `App.vue` behaves as it does today — the ticket `02` spike harness stays usable — plus
  three links into the variants, which is how the variants are reachable on the device where there is
  no address bar.
- **The bar is not gated on `import.meta.env.DEV`.** A deliberate deviation from the `/prototype`
  rule that the switcher hides in production builds: the device pass *is* a release build, and the
  bar is the only way to change variant or scenario there. Safe because this never leaves the
  throwaway branch. It also needs real tap targets, since `←`/`→` do not exist on a phone.
- **The bar carries**: variant arrows, scenario, source, the four real settings (weighting C/A/Z, time
  weighting F/S, window 10/30/60/120 s, calibration offset present/absent), the **resolution knob**,
  and a viewport readout in px so a screenshot says what size it was judged at.
- **Numbers are tabular.** Monospaced or `font-variant-numeric: tabular-nums` throughout: at 10 Hz a
  proportional digit set makes the whole number twitch sideways when one digit changes, which reads as
  instability that is not in the measurement. This is the kind of thing that only shows up in a
  running prototype, which is why it is a note here rather than a decision.
- **Dark, high contrast.** The venue is dim and read at an angle. Not a decision the prototype makes
  freely — `07` decision 10 already put an inferno-family ramp on a dark field, and a light chrome
  around a dark picture would fight it.
- **Typography and scale are the substance, not polish.** `/prototype` says skip the polish, and for a
  layout ticket whose criterion is legibility, type size, weight, contrast and spacing *are* the
  question. Everything else stays rough: no tests, no error handling, no abstractions, no persistence.
- **One command**: `npm run dev`, then `http://localhost:1420/?variant=A`. No new npm script, no new
  dependency, no Rust change.
- **The iOS build cannot be sloppy**, per `CLAUDE.md`: `env -u FORCE_COLOR` on any `tauri ios`
  invocation, and `./scripts/check-ios-plist.sh` afterwards to assert
  `NSMicrophoneUsageDescription` survived the merge. No change to `bundle.iOS.frameworks`, so no
  `tauri ios init` regeneration. Expect git churn in `gen/apple/` from the build itself.

## What I will do with it

1. Build it, then check it against the scenario list myself — mainly that every inherited state
   actually appears somewhere and that none of them reads as an error when it is a design.
2. Hand you the URL and the keys. The useful answer is usually "B's settings strip with A's number",
   so the variants are written to be raided rather than voted on.
3. **Narrow to one or two in the browser**, then one embedded iOS build and judge on the device — 6 px
   per band at arm's length, in the dimmest room to hand. That check is the point of the device pass
   and it is `07`'s open risk, so it happens even if the browser verdict is unanimous.
4. Fold the verdict into ticket `09`'s `## Answer`: the layout, the hierarchy, where settings and
   calibration live, how the two mode dimensions are indicated, how the input-state line reads, the
   display resolution, and what each was traded against.
5. Propagate. Expected downstream, to be confirmed by what wins:
   - **`10`** — the layout section of the spec, plus the exact wording of the input-state line and of
     the `dBFS` label, since both are the app speaking and both must stay inside the no-warnings rule.
   - **The map** — the fog entry *How the dB(A)/dB(C) switch presents* should close completely here,
     and `07`'s "nothing was judged on real glass" risk gets an answer either way.
   - Possibly **`07`** — if 6 px per band does not read on the device, that is a correction to `07`
     decision 3's band count or to its ~200 px height, and it belongs on `07` as a correction rather
     than being quietly absorbed here.
6. Capture the prototype on `prototype/09-screen-layout` per `/prototype`, leaving `main` with the
   decision only.

## Stated limitations of this vehicle

- **The numbers are not the real DSP.** A rough `05`-shaped meter over Web Audio, not cascaded
  biquads over cpal. It is enough to judge whether a layout is legible and calm; it is not evidence
  about accuracy, and no number this prototype shows means anything as a measurement.
- **No calibration, so `dB` is a costume.** The offset is a knob on the bar, not a derived value. The
  `uncalibrated` scenario shows the honest state; `dB` shows the label, not a real SPL.
- **A dim room at a venue is not a dim room at a desk.** The device pass answers "does 6 px per band
  read at arm's length", which is most of the risk, but it does not reproduce a venue.
- **`getUserMedia` in a Tauri WKWebView is untested**, which is precisely why the device pass runs on
  `scripted` and the live source is a desk-side convenience. If it happens to work on the phone, that
  is a bonus finding and nothing depends on it.
- **Landscape and 800×600 are checked for survival, not designed.** Per the confirmation above. If a
  variant wins in portrait and is unusable wide, that is a finding to record rather than a blocker.
