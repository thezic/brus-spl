# Plan: ticket `07` spectrogram prototype

Ticket: [`07`](../issues/07-spectrogram-form.md) — Type `prototype`, Status `claimed`
Skill: `/prototype` → UI branch, sub-shape A (variants on the existing page)

## Confirmed with Simon before planning

1. **It is a spectrogram**, not a spectrum — a scrolling time-frequency image, history moving
   across the screen. The ticket's "confirm the word" question is answered; Decibel X's
   instantaneous bars are *not* what this is.
2. **The prototype runs in the browser**, fed by the laptop microphone through Web Audio with a
   hand-rolled FFT in TypeScript. Throwaway code only — this does not reopen research `01`, which
   governs the shipped capture path. Also agreed: a throwaway prototype is a decision tool, not the
   implementation the map's "plan, don't build" rule forbids, and per `/prototype` it ends on a
   throwaway branch with only the decision landing on `main`.

## What the prototype has to answer

The ticket asks two different kinds of question, and the prototype separates them deliberately:

| Kind | Question | How the prototype answers it |
|---|---|---|
| **Form** | orientation, frequency axis, colour mapping, annotation, density | three structurally different **variants**, switched with `?variant=` |
| **Parameters** | FFT size, hop, window, dB range, span, weighted or raw | **live controls** on the switcher bar, identical for every variant |

Form is a "pick one" question, so it wants variants. Parameters are a "turn the knob until it looks
right" question, so they want knobs — and every variant must react to the same knobs or the
comparison is unfair.

## Ground the design starts from

- **Fs = 48 000 Hz**, block 1024 frames, block rate 46.875/s (`02`, measured on device). The
  browser's `AudioContext` will almost certainly report 48 000 too; the prototype prints it.
- **FFT size is free** (`04` decision 1) — it feeds nothing but pixels.
- **The FFT reads raw, unweighted samples** (`04` decision 3), so applying a weighting curve for
  display is a per-bin gain and is *this ticket's* choice. Both directions are prototyped.
- **Bin 0 carries the mic's DC bias** (`04`, no DC blocker) — never drawn. The standard's band
  starts at 10 Hz anyway.
- **`05`'s ring advances in 100 ms slots**, so one spectrogram column per slot (10 columns/s) is a
  free alignment rather than a new cadence to invent.
- **Pixels, not DSP, bound the parameters.** Two consequences that shape the starting values:
  - A log axis over 10 Hz–20 kHz is 3.3 decades, of which **10–100 Hz is a third of the height**.
    N=2048 puts 4 bins in that decade, N=8192 puts 15, N=16384 puts 30. dB(C) lives exactly there,
    so the low end is the constraint that picks N — and a large N is now free.
  - A 60 s span across ~350 px is ~170 ms per column regardless of N, so a display cannot show
    syllable detail at that span whatever the window length. N=8192 is itself 170 ms, which is why
    the trade is worth *looking* at rather than deriving.
- **Uncalibrated is a real state** (`06` decision 4). The prototype has no calibration, so its
  colour scale is labelled **dBFS** throughout, and the plan for the real thing is that the scale
  shifts with the offset (same colour = same absolute level) rather than auto-ranging.

## The three variants

Each disagrees about structure, not colour. All three get the same signal, the same controls and
the same window of history.

**A — Instrument.** Horizontal, log frequency on Y (10 Hz–20 kHz), time flowing right→left with
*now* at the right edge. Octave gridlines and labels, tick marks every 10 s. Continuous
perceptual colour map (inferno) over a fixed dB window. Wide-and-short, sized to sit under a large
number in portrait. This is the "it looks like a measurement instrument" option.

**B — Waterfall, third-octave.** Vertical: frequency on X as **31 discrete one-third-octave
bands** (12.5 Hz–20 kHz), time scrolling downwards with *now* at the top. Bands are bin *sums*, so
unlike raw bins each cell is a physically meaningful band level and the colour scale can be
labelled in dB per band. Colour is a **coarse quantised ladder** (10 dB steps) so levels are read
off it rather than sensed. Tall-and-narrow, which is the shape portrait actually has spare.

**C — Calm.** The anti-instrument. ~24 log rows, no gridlines, no labels, no legend, monochrome
intensity on dark, heavy smoothing, 30 s span. Judged on one thing only: whether it still reads
from three metres away at an angle, which is the venue condition ticket `09` cares about.

## Controls (on the switcher bar, shared by all variants)

- **Source** — mic · synthetic rumble + speech · pink noise · slow sweep. The synthetic sources
  exist because a desk has no HVAC: the rumble picture must be reproducible to be judged, and the
  sweep makes the axis mapping verifiable by eye.
- **FFT size** — 2048 · 4096 · 8192 · 16384 (window is Hann throughout; a rectangular option is
  pointless for a display).
- **Hop / column rate** — 100 ms (the `05` slot) · 50 ms · 200 ms.
- **Span** — 10 · 30 · 60 · 120 s, matching `05`'s window lengths, to test whether the span should
  simply *follow* the window setting.
- **Weighting for display** — raw (Z) · A · C, to settle whether the picture should match the
  number.
- **dB window** — floor and top sliders, in dBFS.
- **Row summarisation** — mean · max of the bins falling in a row. A real decision, cheap to make a
  knob: max keeps a narrow tone visible when many bins share a row, mean is honest about band
  energy.

## Implementation notes

- **Sub-shape A**: variants mount inside the existing `src/App.vue` (the spike harness), gated on
  `?variant=`. With no param, `App.vue` behaves exactly as it does today, so the ticket `02` spike
  harness stays usable.
- **Samples**: `AnalyserNode.getFloatTimeDomainData` pulled on an interval — it hands back the most
  recent `fftSize` samples, which is all we need, and avoids AudioWorklet plumbing. Hann window and
  FFT are ours, so N, hop and window stay under our control (`AnalyserNode`'s own FFT hardcodes its
  window and smoothing, which is why we do not use `getFloatFrequencyData`).
- **FFT**: iterative radix-2 complex FFT, ~50 lines, no dependency. Real-input optimisation is not
  worth the lines at 10 columns/s.
- **Rendering**: keep a JS ring of column vectors and repaint the whole image via `putImageData`
  each tick. At ~350×250 that is trivial, avoids scroll-blit artefacts, and — the real reason —
  makes a change to the dB window or the colour map apply *retroactively to the visible history*,
  which is what makes the knobs judgeable.
- **Level scale**: magnitude normalised by N and the Hann coherent gain so the numbers are honest
  dBFS rather than arbitrary. Labelled `dBFS`, per `06` decision 4.
- **Throwaway markers**: everything lives under `src/prototype-spectrogram/`, the switcher bar is
  gated on `import.meta.env.DEV`, and no state persists.
- **One command**: `npm run dev`, then `http://localhost:1420/?variant=A`. No new npm script, no
  new dependency, no Rust, no Cargo change.

## What I will do with it

1. Build it, run it, and check it against the four sources myself — mainly that the axis mapping is
   right (the sweep proves that) and that the dB window lands somewhere sane.
2. Hand you the URL and the variant keys. The useful answer is usually "B's frequency axis with A's
   colour", so the variants are written to be raided rather than voted on.
3. Fold the verdict into ticket `07`'s `## Answer`: the form decision plus the settled parameters
   (N, hop, span, window, dB range, weighted-or-raw, row summarisation), and what each choice was
   traded against.
4. Propagate. Expected downstream, to be confirmed by what wins:
   - **`08`** — the column payload shape and rate, which is the bulk traffic it is waiting on. A
     variant that draws ~64 log rows at 10 Hz sends 640 values/s, not 4 096 bins per frame, and it
     also decides that bin→row downsampling happens **in Rust**.
   - **`09`** — the spectrogram's aspect ratio and orientation, which is a large part of its layout
     constraint, plus whether it needs a colour legend.
   - **`10`** — what the display does and does not claim, especially that a per-bin or per-band
     colour is not a calibrated band SPL.
5. Capture the prototype on a throwaway branch per `/prototype`, leaving `main` with the decision
   only.

## Stated limitations of this vehicle

- **Laptop mic, laptop room.** No venue HVAC and no phone microphone response, which is why the
  synthetic sources exist. The prototype judges the *picture*, not the acoustics.
- **No real device.** Wi-Fi client isolation blocks Safari on the phone (map note), so phone
  geometry is judged in a 390×844 browser viewport. Legibility at arm's length on real glass is
  therefore still unverified — the one thing variant C most wants a real screen for.
- **Browser sample rate is not device ground truth**, though both are expected to be 48 kHz. The
  prototype prints what it got.
