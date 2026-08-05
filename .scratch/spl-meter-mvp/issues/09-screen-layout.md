# Screen layout: meter and spectrogram together

Parent: [SPL Meter MVP](../map.md)
Type: prototype
Status: resolved
Blocked by: —  (07 and 08 now resolved, and 05 before them)

**Inherited from ticket [`08`](08-rust-frontend-boundary.md) — one new thing to design, and two
obligations taken away.**

- **New: an input-state line.** `08` decision 8 puts a three-value `input` field
  (`capturing` | `denied` | `unavailable`) in every tick, because a denied microphone otherwise
  renders as `--` beside `0s of 60s` forever with nothing saying why — and it is the one failure the
  user can fix, from outside the app. It must read as a **state label in the family of `dBFS`**, not
  as a warning banner: `06` decision 4's precedent, not a reversal of `11`/`05`'s no-warnings rule.
  For `denied` it should say something actionable (Settings ▸ Privacy); for `unavailable` there is
  nothing to act on, and detail lives in the log.
- **Removed: the layout holds no state.** `08` decision 7 puts the four settings *and* the unit in
  every tick, and `08` decision 4 keeps the spectrogram's history in Rust. So the frontend renders
  the last tick and caches nothing authoritative — no settings copy that can disagree with what is
  running, no column ring of its own. Text entry keeps local draft state; that is an input buffer,
  not state.
- **When to re-pull the picture:** `get_spectrogram()` on mount, on a canvas resize or orientation
  change, and on a window-length change. **Not after calibration** — `07` decision 9 shifts the
  colour window by the offset and the band values shift with it, so calibrating changes every number
  on screen and no pixel of the picture. Only the legend relabels.
- **Picker feedback comes from the command return value**, not the next tick. Every settings command
  returns the new settings, so a tap updates immediately instead of waiting up to 100 ms. If the
  controls feel sticky, that is the path to check.
- **Reset does not clear the spectrogram** (`08` decision 5). Three seconds after a Reset the number
  reads `3s of 60s` while the picture still shows the previous talk's applause — deliberate, because
  60 s of rumble stripe is unrecoverable and the number refills honestly. Both axes are labelled, so
  the layout's job is to make sure they *look* like two axes: the coverage figure belongs with the
  number, the `−60s … now` axis with the picture.

**Inherited from ticket [`07`](07-spectrogram-form.md) — the spectrogram arrives with a geometry
rather than as an unknown shape, and it brings one obligation.**

- **Wide and short: ~200 px tall at full width, sitting under the number.** `07` chose a horizontal
  frame over a vertical waterfall precisely on this ticket's behalf — the waterfall wanted 360+ px of
  the portrait height that the number, coverage, max hold, the two mode indicators and reset are all
  already competing for.
- **Three gutters are part of the design, not decoration** — ~40 px left for frequency labels, ~20 px
  bottom for the time axis, ~52 px right for the colour legend. **If space forces the legend out, the
  picture loses its absolute meaning**; that is a trade this ticket may make, but knowingly rather than
  by cropping.
- **The window length has to be visible near the picture**, because `07` decision 8 makes the
  spectrogram's span *follow* the L_eq window (10/30/60/120 s). The picture is then literally what is
  inside the number — which only reads that way if the two are seen together.
- **A gap in the picture is a hole**, drawn as background (`07` decision 11). Like `--`, it is a
  designed state rather than a rendering failure, so it should look deliberate.
- Also useful: `07`'s prototype ran at ~390 px wide and **could not be judged on real glass** (Wi-Fi
  client isolation keeps it off the phone). 32 bands over 200 px is ~6 px per band. Whether that reads
  at arm's length in a dim venue is the open question `07` handed on, and it is the same question that
  killed its "calm, no chrome" variant — so if this ticket gets to a device, that is the thing to look
  at first.

**Inherited from ticket [`05`](05-level-metrics-pipeline.md) — four obligations, one of them new
work rather than a constraint.**

- **The active-mode indicator is two-dimensional.** `04` narrowed the old "how does the A/C switch
  present" question to "how is the active mode indicated"; `05` decision 7 made the F/S time
  weighting selectable too, and decision 8 ties the *meaning of max hold* to both. So a bare
  `MAX 72.4 dB` is unreadable — the layout has to say which weighting and which time constant
  produced it.
- **`--` is a designed state, not an error state.** The instantaneous readout shows `--` after
  200 ms with no audio, and the L_eq shows `--` at zero coverage (`05` decision 11). This is the
  instrument reporting its own state, so it should read as deliberate rather than broken.
- **Coverage sits next to the L_eq, always** — `33s of 60s`, including `60s of 60s` when full. Not
  conditional on being degraded: an indicator that appears only when something is wrong is a
  warning, which charting ruled out, and always-on is what makes the number trusted.
- **Reset now discards the window as well as the max hold** (`05` decision 10 — it means "start
  measuring this talk"). So its placement has to resist a mis-tap in a way a max-only reset would
  not have needed.

Also useful: **display resolution is this ticket's readability lever.** `05` decision 12 publishes at
10 Hz, but a 60 s L_eq only moves hundredths of a dB per tick, so at 0.1 dB the digit changes about
once a second on its own. Choosing resolution is how the numbers are made calm — not by slowing the
data.

**Inherited from ticket [`06`](06-calibration-model.md) — a calibration surface this ticket did not
know it owned, plus one change to the main screen.**

The calibration surface needs four things, and the list is short because `06` deliberately kept the
gesture to numeric entry:

- **Numeric entry for the reference value** — you type what the proper meter reads and the app derives
  the offset. This is the primary gesture, done standing next to the reference meter, so it wants to be
  quick to reach and hard to fumble.
- **The live 10 s reading with its coverage**, which is what the typed value is matched against (`06`
  decision 3). It settles in 10 s and it is *not* the main display's window, so it is a distinct number
  on this surface.
- **±0.1 dB trim buttons**, for splitting the difference while watching both instruments.
- **The offset shown as a number.** Not decoration: free provisioning means the app is reinstalled
  weekly, and if the data container is ever lost, a written-down offset is the difference between a
  retype and a trip back to the venue (`06` decision 2).

And on the main screen: **the unit label switches between `dBFS` and `dB`.** `06` decision 4 makes
uncalibrated a real state — offset `Option<f64>`, `None` meaning never calibrated — and the honest
presentation is to label the raw values as the different quantity they are rather than blank them or
fake an SPL. So the layout needs both labels to look deliberate, not just the calibrated one.

Where the calibration surface *lives* is still this ticket's call — the layout question already lists
"inline on the one screen, or behind a sheet", and calibration is now the strongest case for a sheet:
it is a rare, deliberate, two-instrument gesture, unlike anything else on the screen.

## Question

One screen has to hold: rolling L_eq (the number judged against 70), instantaneous level,
max hold, the spectrogram, the dB(A)/dB(C) switch, window length, the reset button, and
calibration entry.

- **What's primary?** The rolling L_eq is the number Simon acts on. It should dominate;
  everything else supports it.
- **Glanceability.** This is read at a venue, mid-work, probably from a distance and at an
  angle. Legibility beats density.
- **iPhone orientation** — portrait or landscape, and whether it needs to work in both.
- **Desktop window** — the dev loop runs there, so the layout has to survive an 800×600
  desktop window as well as a phone.
- **Where settings live** — inline on the one screen, or behind a sheet.

Prototype it. This is a "how should it look" question, and the fog note about how the
dB(A)/dB(C) switch presents should get resolved here or graduate into its own ticket.

---

## Answer

**Variant C's frame with its hierarchy inverted: centred and chrome-free, one dominant number over
a wide picture, everything else behind a single sheet — but the dominant number is the *live* level,
not the rolling L_eq.** The L_eq with its coverage and the max hold are always visible beside it,
never behind a tap. Built as variant `D` and looked at before being written down.

The one line that summarises it: *the screen leads with the number that moves and keeps the number
that is judged permanently in view beside it.*

```
        NOW · C · slow        ← both dimensions; they govern the live number and MAX
           65.5
            dB
   LCeq 60s        MAX
     66.5         67.0
   60s of 60s                 ← coverage belongs to the L_eq and sits with it
   ┌──────────────────────┐ ┌─┐
16k│ ░░  ░▒░   ░░░  ░▒▒░  │ │█│ 60
 1k│▓███▓████▒▓██▓░ ▒███▓ │ │▓│
125│██████████████████████│ │▒│ 20
 16│██████████████████████│ │░│      ← steady = HVAC
   └──────────────────────┘ └─┘
    −60s               now   dB/band
             ⋯                        ← settings · calibration · reset
```

**The prototype is the primary source and lives on the throwaway branch
`prototype/09-screen-layout`** — `src/prototype-layout/` there, mounted from `App.vue` on
`?variant=A|B|C|D`, run with `npm run dev` and no Tauri. `main` keeps only this decision. To look at
it again:

```bash
git checkout prototype/09-screen-layout && npm run dev   # then open ?variant=D
```

Unlike `07`, this one **was** put on the device: built with `tauri ios build` and installed with
`devicectl`, which needs no networking and so is not blocked by the Wi-Fi client isolation that kept
`07` off the phone.

## Prototype findings (measured, not judged)

Numbers off the running prototype, sampling the rendered text at 10 Hz. Three of them changed the
design and one retired an argument from an earlier ticket.

**1. A 60 s L_eq is not a live number, and that is the case for the inversion.** Over 8 s on steady
talk the rolling L_eq moved **0.0 dB** and never changed a digit at any display resolution. At hero
size it is dead screen.

**2. With the live value as the hero, display resolution stops being the readability lever.**
Ticket `09` inherited resolution as its lever (`05` d12). Measured over 8 s on the same signal:

| live value | **F** (fast) | **S** (slow) |
|---|---|---|
| range | 11.6 dB | 6.7 dB |
| mean step per 100 ms tick | 1.72 dB | 0.33 dB |
| max step | 7.5 dB | 0.9 dB |
| digit changes/s @ 0.1 dB | 9.4 | 8.6 |
| @ 0.5 dB | 9.0 | 5.9 |
| @ 1 dB | 8.3 | 3.3 |

At F the signal moves further per tick than any rounding step, so coarsening barely helps —
0.1 → 1 dB only takes churn from 9.4/s to 8.3/s. **S is the lever; resolution is not.**

**3. `05` decision 7's stated reason for recommending F-only is wrong, and it was a prediction rather
than a measurement.** It argued that an S-weighted readout "occupies nearly the same perceptual slot
as a short L_eq — two slow numbers and nothing live". Against the 10 s L_eq it named, over 12 s:

| | S-weighted live | `LCeq 10s` |
|---|---|---|
| range | 6.8 dB | **0.5 dB** |
| mean step per tick | 0.351 dB | **0.017 dB** |

S moves **20× further per tick** and covers **14× the range**. They are not in the same perceptual
slot, so S makes the hero number readable *without* making it slow.

**4. `--` at hero size reads as a redaction, not as a state.** At 7–9 rem the two dashes render as a
pair of solid filled blocks. A typographic problem, not a glyph problem — `05` d11 asked for a
designed state and could not specify what makes it read as one.

**5. A denied microphone produced `60s of 60s` beside `--`.** `CLAUDE.md` records that a denied mic
delivers callbacks of exact zeros, so blocks arrive, coverage fills, and every number is
arithmetically undefined. The coverage figure claimed a complete minute while the meter said there
was nothing.

**6. The uncalibrated state is the wider one, everywhere.** `−33.3 dBFS` is a glyph wider than
`66.7 dB`, and the legend caption `dBFS/band` needs **58 px** where `07` budgeted ~52 and
overprinted the `now` label.

**7. An 800×600 desktop window is a *landscape* case, not a portrait one.** It is shorter than a
phone is tall, so a breakpoint drawn at phone-landscape height (560 px) misses it and the portrait
stack pushes Reset below the fold.

**8. Gap drawing needed no code.** A hole is a slot index the frontend's column mirror finds nothing
for — `08` d3's claim, tested rather than assumed. There is no gap-marking branch anywhere in the
prototype.

**9. Above the ceiling looks like below it.** At 73.5 dB the screen is identical to 66.7 dB apart
from the picture being brighter. The no-warnings rule survives contact with a layout.

### Not verified

- **Arm's-length legibility in a dim venue.** The app is on the device and 32 bands over ~205 px is
  ~6 px per band, but no verdict has been given. See residual risks.
- **A real microphone.** Every number above came from the scripted source. `getUserMedia` in a Tauri
  WKWebView is untested, which is why nothing depends on it.

## Decisions (2026-08-05, with Simon)

**1. Form: variant C's frame — centred, chrome-free, one dominant number over a wide picture,
everything else behind one sheet.** Rejecting the other two on their own evidence:

- **A's priority stack** is unarguable and that is all it is. It spends the screen on a hierarchy
  nobody was confused about, and its NOW/MAX cells are boxes drawn around two numbers.
- **B's inline settings lost on cost.** Its own controls run it out of height first: three segmented
  rows are ~250 px in portrait, more than the spectrogram, and it needed a separate tightening pass
  to survive a turn. A visible control is also a visible label, which is real — but `04` made
  weighting a *settings-page* value precisely because changing it is a deliberate act, and putting a
  window-length picker under the thumb argues against that.

**2. The large number is the live level. The rolling L_eq is not.** Simon's call, and it **inverts
this ticket's own premise** — the Question above says the rolling L_eq "should dominate; everything
else supports it".

Finding 1 is the argument the ticket did not make: a 60 s L_eq moves 0.0 dB in 8 s, so at hero size
it is dead screen, while the live reading is what you watch as you walk a room.

The cost, stated because ticket `10` has to state it: **the number that dominates is no longer the
number being judged against the 70 dB ceiling.** Nothing is hidden — the L_eq is on screen with its
coverage — but the screen's emphasis and the screen's purpose now point at different numbers.

**3. Nothing is behind a tap.** C hid NOW and MAX to test whether they were desk curiosity. They are
not. This also discharges `05`'s "coverage sits next to the L_eq, always" more literally than C did:
in C the coverage sat under a number it did not describe.

**4. The hero number carries a quantity label, not just its dimensions: `NOW · C · slow`.** With the
hierarchy inverted the secondary numbers are labelled (`LCeq 60s`, `MAX`) and the hero was
identifiable only by elimination. One header serves the live number and the max hold, because `05`
d8 ties the max's meaning to the same weighting/time-weighting pair.

**5. `S` is the default time weighting. Display resolution stays at 0.1 dB.**

Findings 2 and 3. F churns the hero number and resolution cannot fix it; S can, and does not make it
slow. F stays selectable and is still right for catching transients.

Coarsening the display to 0.5 dB was built and **rejected** — Simon's preference, and the better call
for a reason the measurement supports: resolution is one setting for all three numbers, and the L_eq
is already perfectly stable at 0.1 dB (finding 1). Coarsening to calm the live number therefore
spends a tenth on the number judged against the ceiling in order to fix a different number. Tenths
matter there.

**6. `--` gets its own typographic treatment, not just its own glyph** — muted, and much smaller than
the number it replaces. Finding 4. This is what makes `05` d11's "designed state, not an error state"
true on a screen rather than in a sentence, and it matters more under decision 2 than before: the
hero is now `--` after 200 ms of no audio, where C's hero L_eq was `--` only at zero coverage. That
state went from rare to common.

**7. The input-state line is two parts: a label, and an action only when there is one.** `08` d8
distinguishes `denied` (fixable, from outside the app) from `unavailable` (nothing to act on), and a
single string made them look like the same kind of message — the shape that teaches people to ignore
messages. So `No microphone access` over `Settings ▸ Privacy & Security ▸ Microphone`, and for
`unavailable` the label alone. Styled in the same family as the unit and the coverage figure, never
as a banner.

**8. One sheet, reached from `⋯`, holding settings *and* calibration. Reset lives inside it and is a
two-step.** C's arrangement kept. Calibration is a rare, deliberate, two-instrument gesture, so
folding it into the same sheet as the four settings costs nothing and keeps the main screen at one
affordance. Reset is behind the sheet *and* confirmed because `05` d10 made it discard the window as
well as the max hold, and this layout has the fewest places for a thumb to land safely.

**9. The legend stays.** C dropped it as its own bet, and this is the one place variant D departs
from what Simon picked. `07` was explicit that losing the legend costs the picture its absolute
meaning; removing C's tap toggle *freed* space rather than costing it; and the legend costs width,
not the vertical calm that made C win. So `07`'s trade is **not** taken.

**10. Portrait is primary. One wide reflow serves both phone-landscape and the 800×600 dev window.**
The picture goes beside the number instead of under it, at `max-height: 700px` — a threshold set by
finding 7, not by phone geometry. And the portrait layout is **width-capped**: in a tall desktop
window it centres at phone width rather than stretching a portrait stack across 1400 px.

**11. The primary number is sized for its widest state, not its most common one.** Finding 6. A
number sized for `66.7` overflows on `−108.4`, and the uncalibrated state is the one the app starts
life in (`06` d4).

### Recorded, not debated — forced upstream, or too cheap to argue about

- **Every number on screen is tabular-figured.** At 10 Hz a proportional digit set makes the whole
  number shuffle sideways when one digit changes, which reads as instability that is not in the
  measurement.
- **The hero number is set in the UI sans, not monospace.** At 7 rem a mono font gives the decimal
  point a full advance width, so `66.8` reads as block-dot-block. The system sans has tabular figures
  too, so it is equally stable and far better proportioned. Everything smaller stays mono.
- **Dark, high contrast.** Not a free choice: `07` d10 put an inferno ramp on a near-black field, and
  light chrome around a dark picture fights it.
- **The spectrogram is ~205 px tall with all three gutters**, per `07`'s geometry; its width is
  whatever is left.
- **The accent colour never means alarm.** It marks the input-state line and the armed Reset —
  states and affordances, never levels. Finding 9 is the test that this holds.
- **The frontend aggregates columns into its own pixel budget, in energy**, and draws 1:1 unsmoothed
  so band edges stay edges and a hole stays a hole. Doing that explicitly is what makes `07` d11
  work: letting the canvas smooth-scale blends a transparent gap column into its lit neighbours and
  produces a *dim* column, which is the "dead stream reads as a peaceful room" failure the decision
  exists to prevent.
- **The layout holds no authoritative state**, as `08` d7 requires. The prototype enforced it by
  giving the variants nothing but a tick.

### Residual risks, stated plainly

- **Arm's-length legibility is still unanswered.** The device pass got the app onto the phone — more
  than `07` managed — but no verdict has been given on whether ~6 px per band reads at distance in a
  dim room. It is the one question `07` handed on. If it fails it is a correction to `07` d3's band
  count or to its ~200 px height, and it belongs on `07` rather than being absorbed here.
- **The emphasis and the purpose point at different numbers** (decision 2). Accepted deliberately.
  The risk is a glance that reads the live value and treats it as the figure to compare against 70 —
  a number that sits several dB above the L_eq much of the time.
- **Nothing was judged on a real microphone or in a real room.** Findings 1–3 are properties of the
  smoothers and the window and hold regardless; the *feel* of the hero number on real speech is the
  part most likely to want revisiting.
- **Per-quantity display resolution was not taken.** Tenths on the L_eq and something coarser on the
  live value would calm the hero without costing the L_eq anything. Rejected as a second rule in the
  spec for a problem S already handles — but it is the escape if the hero number reads as too busy.
- **`--` at hero size is now a common screen, not an edge case**, and its treatment (decision 6) was
  judged on a desk. Whether a muted `--` reads as "no reading" rather than "app broken" at a glance
  and at distance is the same unanswered device question as the bands.

### What this ticket hands downstream

- **Ticket [`10`](10-write-the-spec.md) — the layout, plus four things that need stating rather than
  drawing.** The form and its geometry, verbatim. Then: (a) the **inversion and its cost** — the spec
  must say that the dominant number is the live level and that the number judged against 70 is the
  smaller `LCeq`, because a reader will otherwise assume the big number is the one that matters;
  (b) the **defaults `C` / `S` / 60 s / 0.1 dB**, with S's reason, since `05` recommended F-only;
  (c) the **exact wording of the input-state line** and its two-part shape, which is what keeps it
  inside the no-warnings rule; (d) that **`--` and `dBFS` are typographic states**, not merely
  strings — a spec that says "show `--`" and stops will get a redaction bar.
- **The map** — the fog entry *How the dB(A)/dB(C) switch presents* closes completely: it lives in
  the one sheet, and the active mode is indicated by the hero number's own header, both dimensions at
  once. `07`'s "nothing was judged on real glass" narrows to the single band-legibility question
  above rather than closing.

### Corrections to earlier tickets

- **Ticket `05` decision 11's premise.** It states that `--` and zero coverage arrive together —
  "`--` beside `0s of 60s`" — reasoning from `Σn = 0`. Exact zeros break that: samples arrive, so
  `Σn > 0` and coverage fills, while `Σp² = 0` leaves the level undefined (finding 5). Resolved by
  treating a **zero-power block as a gap slot rather than a covered one**, on the grounds that
  coverage exists to say whether the number can be trusted and reporting a full minute beside `--`
  is the opposite of that. Zero power is the absence of a measurement, not a quiet one. Side
  benefit: no `f64` `−∞` ever reaches `serde_json`.
- **Ticket `05` decision 7's reason, not its outcome.** Its recommendation was overruled anyway, but
  the ground it gave — that S "occupies nearly the same perceptual slot as a short L_eq" — is refuted
  by measurement (finding 3). Worth correcting because that reasoning would otherwise argue against
  decision 5's default.
- **Ticket `07`'s legend gutter: 58 px, not ~52.** The uncalibrated caption is four characters
  longer, and at 52 px it overprinted the time axis (finding 6).
- **This ticket's own Question.** "The rolling L_eq … should dominate; everything else supports it"
  is inverted by decision 2. Recorded here rather than edited away, because the premise was
  reasonable and what overturned it was seeing a static number at hero size.
