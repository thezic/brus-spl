# The spectrogram canvas

Parent: [SPL Meter Build](../map.md)
Type: build
Status: open
Blocked by: [`b10`](b10-the-spectrogram-half-of-the-bridge.md)

## Build

The picture itself. Spec [§7](../../spl-meter-mvp/spec.md#7-spectrogram) and
[§11.7](../../spl-meter-mvp/spec.md#117-geometry).

**A scrolling spectrogram — horizontal, time flowing right→left with *now* at the right edge,
on 32 fixed one-third-octave bands.** It is a spectrogram, **not** a spectrum: Decibel X's
instantaneous bars are explicitly not this. The reasoning is division of labour — the number
already answers *how loud*, so the only thing the display uniquely adds is **what is making it
and whether it has been steady**, and a constant HVAC source and a passing door slam are
indistinguishable on bars and unmistakable on a spectrogram.

*The display draws what an FFT can actually deliver, at a colour scale that means the same
thing on every device.*

**Drawing rules** (§7.3), each of which was earned:

- **Columns are aggregated deliberately into the pixel budget, in energy — never decimated by
  the resampler.** 60 s of 100 ms columns is 600 columns into ~350 px; letting the canvas drop
  columns produced a **venetian-blind** picture, and the blind is not in the sound, it is the
  scaler choosing which slots survive. **The frontend does this**, because it is the only side
  that knows the canvas width.
- **Time is drawn blended; band edges are drawn crisp** — a band boundary is real, a column
  boundary is not.
- **Drawn 1:1 and unsmoothed.** No canvas smooth-scaling. This is what makes the gap rule work:
  smooth-scaling blends a transparent gap column into its lit neighbours and produces a *dim*
  column, which is exactly the "dead stream reads as a peaceful room" failure the rule exists
  to prevent.
- **A gap is drawn as a hole — background, visibly absent, never a low-level colour.** A slot
  index in `(last_drawn, now_slot]` with no column is a hole; nothing marks it explicitly.
- **Repaint by appending a column and scrolling**, not by redrawing history — redrawing costs
  ~8 ms per frame at 800 columns even in a desktop browser, which is wasteful in a phone
  webview at 10 Hz for a picture that changes by one column.

**Colour and scale** (§7.1):

- **A fixed 60 dB span, `−90 … −30 dBFS` uncalibrated, shifted by the calibration offset. No
  auto-ranging** — the same colour always means the same absolute level. Auto-ranging would
  make a quiet room and a loud one look identical, which is the one thing a picture of levels
  must not do.
- **A continuous perceptual ramp, monotonic in lightness (inferno family). Never `jet`** — it
  survives being read at an angle in a dim room, where `jet` invents banding the data does not
  have. A quantised 10 dB ladder was built and **rejected**: reading levels is the number's
  job, and it turned the noise floor into blocks.

**The display is always unweighted, in every meter mode** (§7.2). **This must be said out loud
in the UI's own terms**, because a reader comparing the picture band-by-band against a dB(A)
reading will conclude the app is inconsistent. It is not — the picture and the number are
deliberately different quantities. Measurement made it obvious: with A-weighting applied per
bin the 31.5 Hz band fell from −45 to −92 dBFS and **the rumble stripe vanished entirely**,
which is arithmetically correct and diagnostically backwards. **dB(A) mode is exactly when you
want to know the rumble is there, because the number has stopped telling you.**

**Geometry** (§11.7): ~205 px tall at full width, width whatever is left, with three gutters
that are part of the design rather than decoration — **~40 px left** for frequency labels,
**~20 px bottom** for the time axis, **58 px right** for the colour legend (58, not `07`'s ~52:
`dBFS/band` is four characters longer than `dB/band` and overprinted the `now` label).

**The legend stays.** `07` offered the trade "if space forces the legend out, the picture loses
its absolute meaning"; **the trade is not taken** — removing a tap toggle freed the space, and
the legend costs width, not the vertical calm the layout is built on.

**The window length is visible near the picture**, since the span follows it: `LCeq 60s` above
and `−60s … now` below both carry it. In the wide reflow the picture goes **beside** the
number.

## Traps

- **The colour scale moves with the row layout.** An energy-summed display's dB scale shifts
  ~10 dB between 24 rows and 220 on the identical signal — which is *why* the 32-band layout is
  fixed, and what makes a labelled legend honest. Do not make the row count adaptive.
- **The span follows the L_eq window**, so there is no second time setting and the two axes on
  screen cannot disagree — but see §9.3: after a Reset the number describes this talk and the
  picture describes the last 60 s of the room. Both are labelled; that is the design.
- At a **120 s span the picture is about texture, not events** — syllable structure compresses
  at 1200 columns. A consequence of the span following the window, not a defect.
- **Arm's-length legibility is unanswered** (map fog, §13.14): ~6 px per band over ~205 px. It
  is the one open question that could still move §7.1's band count or this ticket's height, so
  **look at it on the phone before polishing**, not after.

## Done when

§14.3's two eyeball tests, both of which caught real errors in the prototype:

- **Pink noise draws flat.** A sloped picture means the band summarisation is wrong.
- **An exponential sweep draws a straight diagonal.** A curve means the log axis mapping is
  wrong. Below ~40 Hz the sweep fans into horizontal streaks — that is bin-borrowing made
  visible, and is expected.

Plus: stopping the audio leaves **holes**, not dim columns; a resize re-pulls and redraws
without a venetian blind; the legend reads correctly in both `dB/band` and `dBFS/band`; and it
has been looked at **on the phone**, at arm's length, with a verdict recorded.

`npm run build` typechecks.
