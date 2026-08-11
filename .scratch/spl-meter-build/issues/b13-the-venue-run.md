# The venue run

Parent: [SPL Meter Build](../map.md)
Type: task
Status: resolved
Blocked by: [`b12`](b12-re-sign-and-install-rehearsal.md)

## The work

**This is the destination.** The map is done when this ticket is closed — not when the code
matches the spec.

It is also the first time this design meets a real microphone in a real room. Every number in
the spec that came from a prototype, a synthetic source or a scripted probe gets its first
honest test here.

### Calibrate, per spec §8.4

In order, and the order matters:

1. Set the weighting to **dB(C)** on the sheet — or to whatever the reference meter is showing.
2. Put the phone and the reference meter **in the same place, pointed at the same steady
   sound.** **Steady is load-bearing, not advisory**: the two instruments may not be reporting
   the same quantity, so if it shows an F-weighted live level and the app shows a 10 s L_eq,
   they agree only when the sound is not changing.
3. Wait for the calibration reading to fill its 10 s slice. Coverage is displayed — judge it.
4. Read the reference meter and **type its value**. The app stores `reference − raw slice`, and
   every displayed number moves at once, including the settled 60 s average and the max hold.
5. Trim ±0.1 dB if the two disagree slightly.
6. **Write the offset down.**

Expect an offset on the order of **+100 dB**. If it comes out near zero or in the hundreds,
something is wrong upstream — most likely `Measurement` mode failed to apply, which is worth
~21 dB (§13.9).

### Read a talk

Then use it. Reset at the start of the talk — that is what the button is for, "start measuring
this talk" — and read it as you would.

### What to bring back

The map's **Not yet specified** section exists for this ticket to clear. Each of these has a
settled parameter waiting on the answer:

- **Does `−90 … −30 dBFS` fit real speech in a real room?** §7.1 names the colour window as the
  value most likely to want moving. (Only if [the canvas](b11-the-spectrogram-canvas.md)
  landed.)
- **Does the hero number feel right on real speech at `S`?** If it reads as too busy, §11.4
  names the escape: per-quantity display resolution, tenths on the L_eq and something coarser
  on the live value — deliberately not taken, and available.
- **Arm's-length legibility in a dim venue.** ~6 px per band, and the muted `--` at hero size.
  A "no" is a correction to the band count or the ~200 px height.
- **The accepted risk in §11.1**: the number that dominates the screen is not the number judged
  against the 70 dB ceiling. Does that bite in practice, or is having the L_eq permanently
  beside it enough?
- **Did anything interrupt it**, and did it recover — or did you have to restart?
- **`get_spectrogram` on the phone**, if the picture is there: 276 KB of JSON at a 120 s span
  has never been parsed in a WKWebView.

### Two things to be honest about while reading it

- **The offset inherits the reference meter's error wholesale.** A class-2 instrument is
  ±1.5 dB while the app displays 0.1 dB. **The precision of the display is not the accuracy of
  the reading** (§13.6).
- **The offset is only valid for the input it was set on.** Plugging in a headset mic can move
  the sensitivity by tens of dB with nothing at runtime to say so — §13.4, the largest single
  risk in the design.

## Done when

The talk has been read, and the answers above are recorded in the resolution. Anything that
moves a settled parameter graduates onto the map as a fresh ticket; anything that does not is
the map closing.

## Resolution

**The talk was read, the app worked, and the design met a real room.** Simon's verdict on the whole
instrument was *"the app worked well"* — which is the answer this map was cut to earn. Every one of
the six bring-backs has an answer, **the parameter the spec flagged hardest survived unchanged**,
and the three things the run *did* move are all about the picture rather than the number.

Two of the answers are read off the phone rather than remembered: `settings.json` in the app data
container (`b12` finding 4's `devicectl device copy from`) ends the run as **`weighting: C`,
`time_weighting: S`, `window_s: 30`, `offset_db: 116.7`**.

### Calibration — the first real one this instrument has had

Reference-based per §8.4, against the reference meter, landing on **`+116.7 dB`**.

It is **on the order of +100 dB exactly as §8.4 predicts**, which corroborates `b08`'s direct
`measurement_mode: true` read-back from the other end: had `AVAudioSessionModeMeasurement` silently
failed to apply, §13.9's 21 dB would have put this number near +95 or +137 and it is neither.
Two smaller notes on the value itself:

- **`116.7` is an exact tenth**, where `reference − raw_slice` comes out irrational-looking (`b12`
  read `113.26162153261384` off this same phone). So the stored value arrived through §8.4 step 5's
  **trim**, or through §8.5's editable offset field — not raw out of the calibration entry.
- **Every offset before this one was a guess.** There was no calibration tool at the desk, so
  `b12`'s `113.26`, `b04`'s `101.4` and `b05`'s `117.2` were placeholders and test values. The
  ~3.4 dB difference between the desk figure and this one is therefore **not a spread between two
  calibrations** — it is the gap between a guess and a measurement, and there is nothing in it to
  explain. What every earlier ticket claimed to verify was the **arithmetic** (`raw + offset` moving
  every number at once, the store keeping the value exact while the wire rounds it), and all of that
  still stands. **§13.6's *the precision of the display is not the accuracy of the reading* now has
  one reading on the accurate side of it, and only one.**

### The six answers

**1 — `−90 … −30 dBFS` fits real speech in a real room, and §7.1's residual risk closes.** The
picture *used its range well*: not washed out at the loud end, not mostly black at the quiet one.
This is the parameter §7.1 names as *"the value most likely to want moving once real speech in a
real room goes through it"*, carried as an open risk since `07`. **It does not want moving.** It is
also the one answer here that constrains the others — see finding 3.

**2 — the hero at `S` is too busy. §11.4's escape fires.** See finding 1.

**3 — arm's-length legibility in a dim venue: the numbers read fine, and the bands are legible.**
So §13.14's last legibility question closes: **~6 px per band and §11.7's ~205 px height stand in
low light as well as they did on the desk**, and the correction held in reserve since `07` is
finally not needed for legibility at all. Simon added *"higher granularity would be good"* — that is
a diagnostic wish, not a legibility failure, and it is finding 3 rather than a correction here.

**4 — §11.1's accepted risk does not bite.** *"The layout of the numbers were good."* The number
that dominates the screen is still the live level rather than the ceiling-judged L_eq, and having
the L_eq permanently beside it **is** enough in practice. That risk was accepted on paper by `09`
and is now accepted in a room.

**5 — nothing interrupted it, and nothing went wrong.** Recorded as **unexercised, not as passed**:
the venue produced no Siri call, no route change and no backgrounding, so `b07`'s supervisor and
`b08`'s 3-second foreground recovery remain demonstrated only by deliberate provocation on the desk
and in `b07`'s own device pass. The honest statement is that a talk did not happen to need it.

**6 — `get_spectrogram` ran in the WKWebView with nothing noticed, at up to a 60 s span.** The span
follows the window (`Spectrogram.vue:148`, §7.1), and the window moved 10 → 30 during the run —
which is the *only* case `pictureNeedsPull` is true for, so a real pull over real IPC on the phone
is proven rather than inferred. Simon recalls a 60 s span at some point, so **≈138 KB is the largest
parse with evidence behind it. The 120 s / 276 KB worst case is still unrun**, and the aggregating-
in-Rust fallback is still a trade rather than a fix.

### Finding 1 — `S` is too busy, and the spec pre-authorised the fix

§11.4 wrote this outcome down before it happened: per-quantity display resolution is *"the **escape**
if the hero reads as too busy on real speech"*, rejected at the time as *"a second rule for a problem
S already handles"*. **S did not handle it.** The condition the escape was conditioned on has fired,
so taking it re-litigates nothing.

Worth being clear why the escape is the *only* move: §11.4 also establishes *"S is the lever;
resolution is not"*, because at `F` the hero moves **1.72 dB per tick** (§6.6) — further than any
rounding step, so coarsening cannot calm `F`. **`S` is already the slower of the two standard time
weightings**, so there is no slower lever to pull. What is left is exactly what §11.4 named:
**tenths on the L_eq, something coarser on the live number.** Uniform coarsening stays rejected for
the reason §11.4 gives — it would spend a tenth on the ceiling-judged L_eq to fix a different
number. Graduated as [`b14`](b14-the-live-number-is-too-busy.md).

### Finding 2 — the colour legend earns nothing, and 58 px is the wrong price for it

*"I own the app. I say it adds no value, we can use the horizontal space better."* — **the owner's
call, and it reverses a made decision on an argument that was never answered.**

`09` d9 and §11.7 both say *"the legend stays"*, but read what was actually declined: `07` offered
the trade *"if **space** forces the legend out, the picture loses its absolute meaning"*, and it was
declined because space did **not** force it — *"the legend costs width, not the vertical calm the
layout is built on."* The objection now is not that it costs too much space; it is that it
**delivers no value**. That claim has never been made or answered, so this is not a re-litigation.

Recorded rather than argued, because a reversal that lives only in a ticket is a spec that lies:

- **§11.7's *"the legend stays"* and its 58 px right gutter are reversed**, and the spec carries the
  correction in §11.7 and §17 — the pattern `b09` used for §16.4 and `b11` for §7.3.
- **§7.1's fixed window is unaffected in substance and loses its readout.** Nothing auto-ranges, so
  the same colour still *is* the same absolute level; what goes is the only thing on screen naming
  those levels. §11.7 should say so plainly rather than leave a future reader thinking the property
  itself went away.
- **§7.2 survives intact, which is why the removal is cheap.** `b11` put `unweighted` on the
  **caption line**, not on the legend, so the guard against comparing the picture band-by-band
  against a dB(C) reading keeps its place.

Graduated as [`b15`](b15-the-pictures-frequency-axis.md), together with finding 3.1 — the reclaimed
gutter is what makes room for the labels.

### Finding 3 — the picture says *that* a band is hot, not *which frequency*

All three of Simon's unprompted notes are one complaint: the frequency legend *"isn't granular
enough"*, *"higher granularity would be good"*, and *"it would be good to be able to mark a position
in the graph to see the frequency to pinpoint problematic frequencies."* They are priced very
differently, and one of them collides with answer 1.

**3.1 — denser frequency labels: cheap, and pure code.** §11.7's ~40 px left gutter and `b11`'s
chrome-redraw-on-labelled-change already support it. In [`b15`](b15-the-pictures-frequency-axis.md).

**3.2 — more bands than one-third-octave: expensive, and it would undo answer 1. Not recommended.**
§7.1 chose 32 rows partly so *"the colour scale stops moving"*: energy-summed rows scale with row
bandwidth, and *"24 rows read ~10 dB hotter than 220 on the identical signal"* (`07` f3, f4). Going
finer moves every cell's energy by ~3 dB per halving, which **re-derives `−90 … −30 dBFS`** — the
one parameter this run confirmed is correct — and breaks `b09`'s property that the 32 bands sum back
to the signal's own unweighted dBFS. **The band count is not the cheap way to serve this wish.**

**3.3 — a readout: mark a position, get that band's centre frequency and level.** This is what the
wish actually wants, and it leaves the band layout, the colour scale and the calibration untouched.
It is also **new scope and a genuine decision** — does marking freeze the picture or track live;
what is the touch target when a band is ~6 px on glass; does it read the band under the finger or
snap to a labelled one; does it survive a re-pull. **Deliberately not graduated yet**, and the
reason is in answer 3 rather than in caution: findings 2 and 3.1 widen the plot by ~17 % and add
labels, which may dissolve most of the need. **Use it at one more talk first.** If the wish
survives that, it is a feature and deserves a short planning map of its own rather than a build
ticket that decides it quietly.

### Read, not tested

The ticket's two honesty items were read and are unchanged by the run: the offset **inherits the
reference meter's error wholesale** (§13.6 — a class-2 instrument is ±1.5 dB while the app displays
0.1 dB), and the offset is **only valid for the input it was set on** (§13.4, the largest single
risk in the design — the phone's own microphone throughout, and nothing at runtime would say
otherwise).

Still unexercised after the venue, and none of it blocked the talk: **`"denied"` on iOS** (never
seen, one tap in Settings, and it did not get done before the run), the **120 s / 276 KB**
`get_spectrogram` parse, **route-change recovery mid-measurement** with a headset, and **§6.9's
200 ms staleness threshold**, which remains reasoned rather than measured on iOS.
