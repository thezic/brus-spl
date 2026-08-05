# Spectrogram: parameters and visual form

Parent: [SPL Meter MVP](../map.md)
Type: prototype
Status: resolved
Blocked by: —  (was 04, now resolved)

**Inherited from ticket [`04`](04-weighting-architecture.md).** The FFT is now the spectrogram's
alone — it does **not** feed the reported level, so FFT size is free to be chosen for how the
display looks rather than for measurement accuracy. `04` rejected deriving the number from bins
precisely so this ticket would not be constrained to N ≥ 8192.

- **The FFT reads raw, unweighted samples**, tapped ahead of the weighting filter. So this ticket
  owns whether the display applies a weighting curve. Both are available and it is a cheap per-bin
  gain from the closed-form magnitude equation either way — safe here, because `04`'s +6.1 dB
  objection to FFT-domain weighting only applies when bins are summed into a reported number, and
  a colour map sums nothing.
- **There is no DC blocker** (`04`), so **bin 0 carries the microphone's DC bias**. Don't draw it —
  the standard's band starts at 10 Hz regardless.
- Note there are now **three** meter modes (C, A, Z), so "match the meter" is a three-way
  question if this ticket goes that way.

## Question

**First, confirm the word.** Simon said "spectrogram" — a scrolling time-frequency display,
history moving across the screen. That is different from a "spectrum" — an instantaneous
bar or line display of the current frame. Decibel X shows the latter. Ask before building.

Then, once that's settled:

- **FFT size, hop size, window function** — and how they trade time resolution against
  frequency resolution given the content is speech.
- **Frequency scale** — log or linear. Log is what matches hearing; linear is what an FFT
  hands you.
- **dB range and colour mapping** for a spectrogram, or scaling for a spectrum.
- **Time axis span** if it scrolls.

Build a rough prototype and react to it. Do not settle this on paper — it's a
"how should it look" question, which is exactly what a prototype is for.

---

## Confirmed before building (2026-08-05, with Simon)

1. **It is a spectrogram**, not a spectrum. Scrolling time-frequency image, history moving across
   the screen; Decibel X's instantaneous bars are explicitly *not* what this is. The reasoning
   Simon accepted: the number already answers "how loud", so the display's unique job is "what is
   making it, and has it been steady" — and a constant HVAC source and a passing door slam look
   identical on bars.
2. **The prototype runs in the browser**, laptop mic through Web Audio with a hand-rolled FFT in
   TypeScript. Throwaway only; it does not reopen research `01`, which governs the shipped capture
   path. Plan: [`plans/07-spectrogram-prototype-plan.md`](../plans/07-spectrogram-prototype-plan.md)
   (approved, no notes).

**The prototype is the primary source for everything below, and it lives on the throwaway branch
`prototype/07-spectrogram`**, not on `main` — `src/prototype-spectrogram/` there, mounted from
`App.vue` on `?variant=A|B|C|D`, run with `npm run dev` and no Tauri. `main` keeps only this
decision. To look at it again:

```bash
git checkout prototype/07-spectrogram && npm run dev   # then open ?variant=D
```

## Prototype findings (measured, not judged)

These are numbers off the running prototype. They constrain the form question rather than settle
it, and several were surprises.

**1. Row summarisation must be band energy, not per-bin mean. Measured, decisive.**
Pink noise has equal energy per one-third-octave band by definition, so an honest banded display
draws it flat. Median band level over 24 s, 12.5 Hz → 16 kHz:

| summarisation | 13 Hz | 100 Hz | 1 kHz | 16 kHz | shape |
|---|---|---|---|---|---|
| `sum` (band energy) | −27.3 | −33.5 | −31.2 | −31.5 | **flat within ~1.5 dB above 32 Hz** |
| `mean` (per-bin) | −27.3 | −39.3 | −47.1 | −59.5 | **invents a 32 dB roll-off** |

`mean` is only correct on a linear axis; on a log axis it divides out exactly the bandwidth that
makes low and high bands comparable. Consequence: a `mean` display would exaggerate a rumble
stripe relative to speech — the opposite of the error the map is organised against, but an error
in the same place.

**2. FFT size is decided by the low end, and the numbers are stark.** Bands narrower than one bin
have to borrow the nearest bin, so they are repetition dressed up as resolution. For a
third-octave layout (32 bands, 48 kHz):

| N | bin width | frame | starved bands | lowest band actually resolved |
|---|---|---|---|---|
| 2048 | 23.44 Hz | 43 ms | 6 | **126 Hz** |
| 4096 | 11.72 Hz | 85 ms | 3 | 63 Hz |
| 8192 | 5.86 Hz | 171 ms | 1 | 32 Hz |
| 16384 | 2.93 Hz | 341 ms | 0 | 16 Hz |

At N=2048 everything below 126 Hz is fabricated — which is precisely the region dB(C) exists to
count. **N ≥ 8192, and N = 16384 if the display is to claim 12.5–20 Hz.** Free, per `04` decision 1.

**3. A pixel-resolution log axis over-promises, at every N.** With 220 log rows (variant A):

| N | starved rows | share of the axis | lowest row resolved |
|---|---|---|---|
| 2048 | 94 / 220 | 43 % | 689 Hz |
| 8192 | 54 / 220 | 25 % | 173 Hz |
| 16384 | 36 / 220 | 16 % | 87 Hz |

No practical FFT resolves 220 log rows at the bottom of the axis, so a quarter of the picture is
always interpolated. A fixed one-third-octave layout is the one that matches what the FFT can
deliver — an argument for B's axis on honesty grounds rather than taste.

**4. An energy-summed display's dB scale moves with the row layout.** Variant C's 24 rows read
about **10 dB hotter** than variant A's 220 rows on the identical signal, because a wider row sums
more bandwidth. So the dB window is not a constant of the app: it is a constant *of a chosen band
layout*. Second independent argument for fixing the layout at one-third-octave — then the colour
scale means "dBFS per third-octave band" and can be labelled.

**5. Weighting the display hides the thing the display is for.** With A-weighting applied per bin,
the 31.5 Hz band fell from −45 to −92 dBFS and the rumble stripe vanished from the picture
entirely. That is arithmetically correct and diagnostically backwards: dB(A) mode is *exactly*
when you want to know rumble is present, because the number is no longer telling you. Recommend
**always raw**, which also makes the picture invariant across a weighting change and keeps display
and number as two independent pieces of evidence.

**6. Columns must be aggregated deliberately, not decimated by the scaler.** 60 s of 100 ms
columns is 600 columns into ~350 px. Letting the canvas drop columns produced a venetian-blind
picture; blending fixed most of it. The real app should either match the column rate to the pixel
budget or aggregate frames per column — and never let the resampler choose which 100 ms slots
survive.

**7. Verified by eye, worth recording:** an exponential sweep draws a straight diagonal, so the log
axis mapping is right; and below ~40 Hz the sweep fans into horizontal streaks, which is
finding 2 made visible. A denied microphone presents as **exact zeros** — the same signature
`CLAUDE.md` records for iOS — so the prototype shows `SILENT` rather than a black picture.

### Not verified

- **A real device.** Wi-Fi client isolation blocks Safari on the phone, so geometry was judged in
  a 390×844-ish viewport. Legibility at arm's length on real glass is untested, which is exactly
  what variant C most needs.
- **A real microphone.** The automated browser handed back a silent stream; the synthetic sources
  are what produced every number above. Real speech and a real room may look different in ways
  that matter to the colour scale, though not to findings 1–4, which are arithmetic.

---

## Answer

**A scrolling spectrogram in A's frame on B's axis: horizontal, time right→left, but the rows are
fixed one-third-octave bands rather than pixel-resolution log rows.** The picture is **always
unweighted**, its **span follows the L_eq window**, and each cell is **band energy** — a summed
level, not a per-bin average. Built as variant `D` and looked at before being written down.

The shape that won, and the one line that summarises why: *the display draws what an FFT can
actually deliver, at a colour scale that means the same thing on every device.*

```
68.4 dBFS          ← 05/09's number dominates
L_Ceq 60s · C · fast · 60s of 60s
    ┌──────────────────────────────┐ ┌─┐
16k │ ░░  ░▒░   ░░░    ░▒▒░  ░░    │ │█│ −40
 1k │▓███▓████▒▓██▓░ ▒███▓ ▓███▒   │ │▓│
125 │██████████████████████████████│ │▒│ −70
 16 │██████████████████████████████│ │░│      ← steady = HVAC
    └──────────────────────────────┘ └─┘
     −60s                       now   dBFS/band
     32 one-third-octave bands · span follows the window
```

### Decisions (2026-08-05, with Simon)

**1. It is a spectrogram, not a spectrum.** The ticket's "confirm the word first" instruction,
discharged: scrolling time-frequency, history moving across the screen. Decibel X's instantaneous
bars are explicitly not this. The reasoning that settled it is about division of labour — the
number already answers *how loud*, so the only thing the display uniquely adds is *what is making
it and whether it has been steady*, and that is precisely the axis bars do not have. A constant
HVAC source and a passing door slam are indistinguishable on bars and unmistakable on a
spectrogram.

**2. Form: horizontal frame, one-third-octave rows.** Simon took neither variant whole — A's frame
(time right→left, *now* at the right edge, octave gridlines, labelled axes, continuous colour, a
legend) with B's axis (fixed base-10 one-third-octave bands, IEC 61260). Rejecting the other three
on their own evidence:

- **A's 220 pixel-resolution log rows over-promise at every FFT size** — 25 % of the axis is
  interpolated repetition at N=8192 and 16 % even at N=16384 (finding 3). The picture is prettier
  and partly fictional, in the region dB(C) exists to count.
- **B's vertical waterfall wants 360+ px of portrait height**, which is the scarcest thing on the
  screen `09` has to build: the number, coverage, max hold, mode indicators and reset all want the
  same space. Horizontal costs ~200 px and sits under the number.
- **C failed its own bet, and that is the useful result.** With 24 half-octave rows and no hue, the
  steady rumble and the speech merged into one haze — the display stopped answering the only
  question it exists for. Legibility at three metres was worth testing; it is not worth having
  instead of information.

**3. Rows are one-third-octave bands: 32 of them, 12.5 Hz to 16 kHz nominal.** Two independent
arguments arrived at the same place, neither of them aesthetic:

- **Nothing is fabricated.** At N=8192 exactly one band (12.5 Hz) is narrower than a bin, against
  54 of 220 rows for a pixel-resolution axis (findings 2 and 3).
- **The colour scale stops moving.** Energy-summed rows scale with row bandwidth, so variant C's 24
  rows read ~10 dB hotter than A's 220 on the identical signal (finding 4). With a *fixed* band
  layout, "what a colour means" is a property of the app rather than of the row height, and the
  legend can honestly be labelled **dBFS (or dB SPL) per one-third-octave band**.

The 20 kHz nominal band is dropped because its upper edge exceeds 20 kHz; including it by clamping
the edge is an implementation detail worth nobody's time.

**4. Each cell is band energy — the sum of the bin powers in the band, not their mean.** The one
place a plausible-looking choice was measurably wrong. Pink noise carries equal energy per
one-third-octave band by definition, so an honest display draws it flat: `sum` does, within
1.5 dB above 32 Hz, while `mean` invents a **32 dB roll-off** across the spectrum (finding 1).
`mean` is correct on a linear axis and wrong on a log one, because it divides out exactly the
bandwidth that makes bands comparable. `max` remains defensible for a narrow-tone hunt and is not
what this display is for.

**5. N = 8192, Hann, hop 100 ms.** N is free for the display (`04` decision 1) and is decided by the
bottom of the axis, not the top: at N=2048 everything below **126 Hz** is bin-borrowing (finding 2),
and that is the region the whole dB(C) argument lives in. N=8192 gives 5.86 Hz bins and resolves
every band from 16 Hz up, at a 171 ms frame. **N=16384 is the option to take if the 12.5 Hz band
ever matters** — it resolves all 32 — at the cost of a 341 ms frame, which smears 3.4 columns
together instead of 1.7. Hop is **100 ms, one column per `05` ring slot**, which is free rather than
a new cadence.

**6. Columns are aggregated deliberately, never dropped by the resampler.** 60 s of 100 ms columns
is 600 columns into ~350 px. Letting the canvas decimate produced a venetian-blind picture, and the
blind is not in the sound — it is the scaler choosing which 100 ms slots survive (finding 6). So the
column stream is averaged into the pixel budget by energy, and time is drawn blended while **band
edges are drawn crisp**, because a band boundary is real and a column boundary is not.

**7. The display is always raw and unweighted, in every meter mode.** Simon's call, matching the
recommendation, and measurement is what made it obvious: A-weighting the display dropped the
31.5 Hz band from −45 to −92 dBFS and the rumble stripe **disappeared from the picture** (finding 5).
That is arithmetically right and diagnostically backwards — dB(A) mode is exactly when you want to
know rumble is present, because the number has stopped telling you. Two further payoffs: the
picture does not change shape when a settings-page value changes, and the display and the number
stay **two independent pieces of evidence** rather than two renderings of one. Free either way,
since `04` decision 3 taps the FFT ahead of the filter.

**8. The time span follows the L_eq window setting** — 10/30/60/120 s, default 60. The picture is
then literally what is inside the number being judged, there is no new setting, and no need to
explain why two time axes on one screen disagree. Verified at both extremes: 10 s gives 100 columns
across the width (smooth), 120 s gives 1200 (compressed but the rumble stripe and the speech
rhythm both survive).

**9. A fixed dB window, shifted by the calibration offset. No auto-ranging.** The window is a
constant span — **60 dB** is the starting value, ≈10 → 70 dB SPL per band once `06`'s ~+100 dB
offset is applied — and the offset moves it, so the same colour always means the same absolute
level. Auto-ranging was rejected on the map's standing principle rather than on effort: it would
make a quiet room and a loud one look identical, which is the one thing a picture of levels must
not do. Uncalibrated, the legend reads **dBFS** exactly as `06` decision 4 requires of the numbers.

**10. Colour: a continuous perceptual map with a labelled legend.** Monotonic in lightness (an
inferno-family ramp), so it survives being read at an angle in a dim room, and never `jet`, which
invents banding the data does not have. The **quantised 10 dB ladder was built and rejected**: it
does make levels readable rather than sensed, but reading levels is the number's job, and it turned
the noise floor into blocks.

**11. A gap is drawn as a hole, not as silence.** Forced by consistency rather than chosen: `05`
decision 11 refuses to publish a fake quiet, and `05` decision 2's clock-driven ring produces gap
slots by construction. So a column with no data is background, visibly absent, and never a
low-level colour — otherwise a dead stream reads as a peaceful room, which is the exact failure
`11` probe 2 caught.

### Recorded, not debated — forced upstream, or too cheap to argue about

- **Bin 0 is never drawn.** It carries the microphone's DC bias, since `04` declined a DC blocker,
  and the standard's band starts at 10 Hz regardless. The lowest band drawn is 12.5 Hz.
- **The FFT runs off the audio callback** (`04`), fed from the ring buffer. Unchanged here.
- **The picture is repainted by appending a column and scrolling, not by redrawing the history.**
  The prototype redraws everything (which is what made the dB window and colour map judgeable
  retroactively) and that costs ~8 ms per frame at 800 columns in a desktop browser — fine on a
  desk, wasteful in a phone webview at 10 Hz for a picture that changes by one column.
- **The verification signals are worth keeping** for the implementation's own eyeball tests: an
  exponential sweep must draw a straight diagonal on a log axis, and pink noise must draw flat.
  Both caught real things here.

### What this ticket hands downstream

- **Ticket [`08`](08-rust-frontend-boundary.md) — the bulk payload it feared does not exist.** A
  column is **32 band levels, not 4 096 bins**: 128 bytes at 10 Hz, ≈1.3 kB/s, which is the same
  order as the meter's own scalars. So the answer to "where does downsampling happen" is **in
  Rust**, before the bridge, and it is banding rather than downsampling. Three consequences `08`
  owns: gap columns need marking so the frontend can draw a hole (decision 11); the offset is
  applied to band levels the same post-log way as to the meter (`05` decision 13), so colour means
  an absolute level; and since the span follows the window (decision 8), either Rust re-slices or
  the frontend keeps its own 120 s ring — a choice, but not an open question about volume.
- **Ticket [`09`](09-screen-layout.md) — a concrete geometry instead of "a spectrogram somewhere".**
  Wide and short: ~200 px tall at full width, sitting under the number, plus three gutters that are
  part of the design rather than decoration — ~40 px left for the frequency labels, ~20 px bottom
  for the time axis, ~52 px right for the legend. Also: the window length must be visible near the
  picture, because decision 8 ties the two together; and if space forces the legend out, the
  picture loses its absolute meaning, which is a trade `09` should make knowingly rather than by
  cropping.
- **Ticket [`10`](10-write-the-spec.md) — four things to state and one not to claim.** The display
  is **always unweighted**, so it must not be read band-by-band against a weighted number. Its
  resolution is **one-third-octave, 12.5 Hz–16 kHz**, and the 12.5 Hz band is interpolated at
  N=8192. A band colour is **not a calibrated band SPL** — a single broadband offset cannot correct
  a phone mic's frequency response, which is already a stated limitation of `06` and lands here
  verbatim. And the picture reports its own gaps as holes.

### Residual risks, stated plainly

- **Nothing was judged on real glass.** Wi-Fi client isolation kept the prototype off the phone, so
  geometry was judged in a ~390 px viewport. 32 bands over 200 px is ~6 px per band; whether that
  reads at arm's length in a dim venue is the one open question this prototype could not answer,
  and it is the same question that killed variant C.
- **Nothing was judged on a real microphone.** The automated browser handed back exact zeros, so
  every number here came from synthetic sources. Findings 1–4 are arithmetic and unaffected; the
  colour window's default (decision 9) is the part most likely to want moving once real speech in a
  real room goes through it.
- **The 12.5 Hz band is interpolated at N=8192**, i.e. the lowest band on the picture is the one
  band the picture cannot honestly draw. Bumping to N=16384 fixes it; the map has no evidence yet
  that infrasound at 12.5 Hz matters more than a 341 ms frame costs.
- **A 120 s span compresses syllable structure** to the point where the picture is about texture
  rather than events. Accepted: it follows the window by decision 8, and someone who chooses a
  two-minute average has already chosen to stop looking at seconds.
