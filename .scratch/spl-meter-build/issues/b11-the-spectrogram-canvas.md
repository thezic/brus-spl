# The spectrogram canvas

Parent: [SPL Meter Build](../map.md)
Type: build
Status: resolved
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

## Resolution

**Built, and it draws.** `src/components/Spectrogram.vue` plus `src/spectrogram.ts` (the ramp, the
dB window and the pixel budget — pure, no DOM), four new strings in `src/display.ts`, `SLOT_MS` on
the bridge, and `App.vue` handing the whole tick down. `npm run build` typechecks; the 122 Rust
tests are untouched and green. Both §14.3 eyeball tests were **looked at**, and so were the gap,
resize, window-change and both-legend cases. **Every Done-when clause is met, the phone included**:
it is installed and running there, and the arm's-length verdict the ticket asked for first came back
*the bands read fine*.

**The rig: a throwaway harness on branch `prototype/b11-spectrogram-harness`** (`harness.html` +
`src/harness/`, `npm run dev` → `/harness.html`), following `09`'s convention. It drives the real
component with synthetic columns in a browser and fakes `window.__TAURI_INTERNALS__.invoke`, so the
*pull* path — mount, resize, window change — is exercised too. Worth keeping: §7.1's `−90 … −30 dBFS`
colour window is the parameter [the venue run](b13-the-venue-run.md) is most likely to move, and this
is where a candidate can be looked at in ten seconds.

**Finding 1 — §7.3's "drawn 1:1" cannot be implemented literally, and does not need to be.** 1:1
would require the aggregated column count to *equal* the plot width in device pixels, which is only
true when the span happens to divide it; at a 60 s span on a dpr-3 phone the honest choices are 600
columns into 1050 px (43 % of the width unused) or four slots per column (400 ms of time resolution
thrown away to fill pixels). Neither is what the rule is protecting. **The rule is *unsmoothed*, and
1:1 is one case of it**: the pixel budget already guarantees the drawn columns are never *wider*
than the plot, so what remains is always magnification, and nearest-neighbour magnification
duplicates columns — it can neither blend nor drop one. The failure the rule exists to prevent, a
transparent gap smeared into a *dim* column, is a property of interpolation and not of scale. Spec
§7.3 and §17 carry it. So the picture is drawn as **`buckets × 32` pixels on an offscreen canvas,
blitted once per changed tick with `imageSmoothingEnabled = false`**.

**Finding 2 — the two-canvas split is what keeps the append rule true.** The data canvas is the only
thing appended to or scrolled: one `putImageData` of a 1 × 32 column, or one self-`drawImage` to
scroll. The visible canvas gets one `drawImage` inside the plot rect, which leaves the chrome —
frame, frequency labels, legend, captions — untouched, so the chrome is redrawn only when a labelled
setting or the geometry changes rather than ten times a second. History is drawn wholesale in
exactly one place, `rebuild`, which is what a pull is for.

**Finding 3 — the scroll must composite with `copy`, and the default is silently wrong.** Shifting
the data canvas left by drawing it onto itself under the default `source-over` leaves the vacated
columns holding their old pixels, because a transparent source does not erase an opaque
destination. The symptom is precisely the failure §7.3 legislates against: **a stopped stream smears
its last column across the picture instead of scrolling holes into it** — a dead stream reading as a
steady room, arrived at from a direction the spec did not anticipate. `globalCompositeOperation =
"copy"` makes absence the default and is the reason the gap rule needs no gap branch anywhere
(`09` finding 8, still true).

**Finding 4 — the bucket grid has to be anchored to slot 0, not to the right edge.** Aggregation
groups `k = ceil(span / plot px)` slots per drawn column; if the grouping is measured back from
`now_slot` it re-partitions every time the edge moves, so an appended column lands in a different
group than the pull put it in and the picture shears by one slot on every re-pull. `floor(slot / k)`
is absolute, so a column that arrives a tick late lands where it would have landed on time.

**Finding 5 — the frontend twin of `b10`'s cursor bug: a pull drops the columns that arrive while it
is in flight.** `get_spectrogram` answers from a snapshot Rust took before the round trip, and ticks
keep arriving during it. Rebuilding from the pull alone therefore discards up to a tick's worth of
real columns — **a one-slot hole that never heals**, since nothing will send those columns again.
Columns arriving while a pull is outstanding are held and replayed onto the rebuilt canvas. Same
shape of bug as `b10` finding 1, at the other end of the same wire, and equally invisible: the
picture is simply missing a slot nobody counted.

**Finding 6 — `09` finding 6's gutter arithmetic bites once more, in a new place.** With `now`
right-aligned flush to the plot's right edge and `dBFS/band` right-aligned to the canvas, the two
strings **touch** in the uncalibrated state — the same "the uncalibrated caption is four characters
longer" pressure that moved the gutter from 52 px to 58 px, arriving this time as an inter-label
collision rather than an overprint. `now` is inset 4 px from the plot edge. Also: the top frequency
label's row centre is under 3 px from the top of the picture, so an unclamped baseline loses the top
half of `16k` off the canvas — labels are clamped inside the plot's height, ticks are not.

**Finding 7 — a hole and a −90 dBFS column are the same colour, by construction.** The ramp's bottom
is `#000004` on a `#0a0a0c` field, so the two are indistinguishable at the very bottom of the scale.
This is not the failure §7.3 is about (that one is about *dim*, i.e. blended, columns) and it is
mostly unreachable: §16.8 already makes an all-zero hop a gap rather than a floor column, and `b09`
measured a genuinely quiet room peaking at −53 dBFS in the 50 Hz row, mid-ramp. Recorded rather than
fixed — the fix would be a non-background backdrop for the plot field, which reads as *a grey box*
for the first 60 s of every run and contradicts §7.3's own wording. Worth revisiting only if the
venue run produces a picture that reads as empty when it is not.

**§7.2 is said out loud as one word on the caption line: `−60s · unweighted … now … dBFS/band`.**
The reader who is about to compare a band colour against a dB(A) number does it at the legend, so
that is where the sentence had to be. It costs no vertical budget and the geometry is spec §11.7's
unchanged — 40 px left, 20 px bottom, 58 px right, ~205 px tall. A longer, louder statement was
drafted and dropped: the picture must not change shape when a settings value changes, so it cannot
say "not dB(A)" only in dB(A) mode, and the static version of that sentence is a paragraph.

**Looked at, in the harness and then live.** Pink noise (equal energy in every band) draws **one flat
colour top to bottom**. An exponential sweep draws a **straight diagonal**, with no curvature —
which is the log axis mapping being right. A source that stops for 2 s of every 5 draws **clean
black stripes**, not dim ones. At a 120 s span the same stripes compress without a venetian blind or
a moiré (`k = 2`, aggregated in energy); at 10 s and 330 px wide they magnify crisply. Then under
`npm run tauri dev` on the built-in microphone: **the rumble stripe below 125 Hz and the syllable
striations of speech are both unmistakable in the same picture**, which is the entire reason the
display is a spectrogram and not bars. A forced webview reload repainted the **full 60 s** from
`get_spectrogram` over real IPC — §9.5's mount row, live. Seeding a +101.4 dB offset into
`settings.json` and restarting relabelled the legend `11 … 71` and `dB/band` with **the picture
identical**, which is §7.1's arithmetic confirmed rather than argued.

**On the phone: it is installed and running, and the verdict is the one thing still outstanding.**
The device was `unavailable` on the first attempt and available on the second. `env -u FORCE_COLOR
npx tauri ios build --debug` reached its bundle with the new frontend in it,
`./scripts/check-ios-plist.sh` passed, and `devicectl` installed and launched onto the iPhone 14
Pro. The device log is clean and says the session is exactly what §3.1 asks for:

```
[stderr] capture: running CaptureFacts { device: "coreaudio:default", sample_format: "f32",
  sample_rate: 48000, channels: 1, buffer_frames: Some(1024),
  session: Some(SessionFacts { sample_rate: 48000.0, input_channels: 1,
    mode: "AVAudioSessionModeMeasurement", measurement_mode: true,
    io_buffer_duration: 0.021333333333333333, permission_granted: true }) }
```

No rate change, no rebuild, no error, and `WebPageProxy::runJavaScriptInFrameInScriptWorld` at a
flat 100 ms in the log — the tick reaching the webview on the device, which is the picture being
fed. So **the canvas runs on the phone**; what is unanswered is what it *looks* like there.

**The arm's-length verdict, which is this ticket's last clause: the bands read fine.** Simon's call,
on the phone, with the picture drawing live — **~6 px per band over 32 bands is legible at arm's
length**. So §7.1's band count and §11.7's ~205 px height stand as written, and the correction that
`07`, `09` and §13.14 all held in reserve for this moment is **not needed**. It had to come from a
person: `idevicescreenshot` does not work on this setup (`CLAUDE.md`), `devicectl` has no screenshot
subcommand, and legibility at distance is a judgement rather than a measurement. §13.14 and the
map's fog both carry it. **What is still unanswered is the *dim* half of that question** — nobody
has seen this screen in a dark room, and that is [`b13`](b13-the-venue-run.md)'s.

**Also learned, and small: `devicectl device orientation` is simulator-only** — it answers
`CoreDeviceError 1001, The capability "Device Orientation" is not supported by this device` on a
real phone. So the orientation-change re-pull cannot be driven from a script; rotating the phone by
hand is the test.

**Also untested:** `get_spectrogram` in a WKWebView with a *full* ring (§13.14) — the pull certainly
ran at mount, but the ring was empty then, so 276 KB at a 120 s span has still never been parsed on
the device; the resize path in the Tauri webview specifically (osascript has no accessibility
permission here and the phone cannot be rotated remotely — the ResizeObserver and its debounce were
exercised in Chrome, and the reload path in the desktop app); and the picture has never been seen
next to a **real** venue's sound, which is [`b13`](b13-the-venue-run.md)'s job.
