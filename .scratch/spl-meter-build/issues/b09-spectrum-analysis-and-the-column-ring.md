# Spectrum analysis and the column ring

Parent: [SPL Meter Build](../map.md)
Type: build
Status: resolved
Blocked by: [`b08`](b08-tier-1-device-pass.md)

## Build

`spectrum.rs`: FFT frame assembly, third-octave banding, and the column ring. Spec
[§7.1](../../../docs/spec.md#71-parameters) and
[§16](../../../docs/spec.md#16-what-this-spec-decides-that-no-ticket-decided) items 1, 4
and 5.

**Tier 2 starts here, and the map says outright that Tier 2 is what gets dropped.** Read the
map's drop order before starting: if the date is close, this is not the ticket to be on.

The second pipeline. It taps the **raw** samples ahead of the weighting filter (§5.2), which is
what keeps the display unweighted and the two pipelines independent.

| | |
|---|---|
| Analysis | **N = 8192**, **Hann**, one frame per 100 ms slot |
| Rows | **32 fixed one-third-octave bands**, 12.5 Hz–16 kHz nominal |
| Cell value | **band energy — the sum of bin powers in the band, never their mean** |
| Columns | one per 100 ms slot, **the same slots as the energy ring** |
| Bin 0 | **never drawn** |

**N = 8192, not 2048**: at N=2048 everything below **126 Hz** is bin-borrowing — the whole
dB(C) region. N=8192 gives 5.86 Hz bins and resolves every band from 16 Hz up, at a 171 ms
frame.

**`sum`, never `mean`**: pink noise has equal energy per third-octave by definition, so an
honest display draws it flat. `sum` does, within 1.5 dB above 32 Hz; `mean` invents a **32 dB
roll-off** that is not in the sound.

**Exact band centres** are `f(n) = 1000·10^(0.1(n−30))` for `n = 11 … 42` (IEC 61260 base-10),
edges at `f_c · 10^(±0.05)`. The 20 kHz nominal band is dropped because its upper edge exceeds
20 kHz.

**Power normalisation** (§16.4) — the spec decided this because no ticket did, and it is
load-bearing for the colour scale. With `S1 = Σ w[n]`, one-sided bin power is:

```
P_k = 2·|X_k|² / S1²
```

so a full-scale sine at a bin centre reads its own mean square, **−3.01 dBFS — the same dBFS
convention as the meter**, which is what lets one calibration offset shift both the numbers and
the picture. §7.1's `−90 … −30 dBFS` colour window is stated against *this* normalisation and
means nothing against another.

**Band assignment** (§16.5): sum the power of bins whose **centre** falls in `[f_lo, f_hi)`; if
no bin falls in a band, take the nearest bin's power. That borrow is what makes the 12.5 Hz band
interpolated — at N=8192 it is 2.9 Hz wide against 5.86 Hz bins, and it is **the one band the
picture cannot honestly draw** (§13.12).

**The column ring**: 1200 columns (120 s max span) × 32 `f32` = **153 KB**, allocated once.
Rust owns the picture's history so that every canvas invalidation is one move — *pull again* —
rather than four different repairs.

**A slot is a gap if it received no samples *or* zero total power** (§16.8), matching §6.3's
rule for the energy ring. Without the second clause a denied microphone draws a picture at the
bottom of the colour window, which reads as **a very quiet room**.

**No column is emitted until N samples have been buffered** (0.171 s at 48 kHz), so the first
one or two slots after a start or a rebuild are holes.

**`realfft`** (§16.1), which wraps `rustfft`. Real-input transforms are ~2× the throughput of a
complex one at N=8192, and this is the only heavy arithmetic in the app. **The FFT runs off the
audio callback** — the callback only copies the raw block into the FFT ring (§3.3).

## Traps

- **Nothing here clears.** §6.11's fourth column is entirely empty and that is the point: not
  Reset, not a weighting change (the picture is unweighted), not F/S, not a window-length change
  (a re-slice), not an offset change. A rate change produces gap columns by construction, as it
  does for the energy ring.
- The colour window is quoted in **dBFS before calibration**; the offset shifts the window and
  the band values together, so every colour is unchanged and only the legend relabels (§9.5).
- Do not weight the bands. Ever. See [the canvas](b11-the-spectrogram-canvas.md) for why —
  A-weighting the display made the rumble stripe **vanish**, which is arithmetically correct and
  diagnostically backwards.

## Done when

`cargo test` green, numerically rather than by eye at this stage:

- **Pink noise gives equal band levels within 1.5 dB above 32 Hz.** This is §14.3's first
  eyeball test, asserted on the numbers before there is anything to look at — and it is the test
  that caught `mean` inventing a roll-off.
- **A full-scale sine at a bin centre reads −3.01 dBFS in its band**, confirming the
  normalisation against the meter's convention.
- An exponential sweep walks the bands monotonically.
- A block of exact zeros produces a **gap**, not a column at the bottom of the range.
- Bands are correct at 44.1 and 48 kHz, since band edges are a runtime function of the rate.

`cargo clippy` clean.

## Resolution

**Built and green: 114 tests, 18 of them new, and the ring driven against a real microphone
under `npm run tauri dev` at both `--debug` and `--release`.** Clippy clean, `cargo fmt` applied,
and `cargo check --target aarch64-apple-ios --lib` passes — `realfft` needs no
`bundle.iOS.frameworks` entry and no regeneration, asserted by checking rather than by reading.
Spec §7.1's parameters needed no correction. **§16 did**, twice, and the first of them is the
whole of this ticket's value.

**Finding 1 — §16.4's power normalisation is right for a bin and 1.76 dB wrong for a band, and
the band is what this display draws.** §16.4 decides `P_k = 2·|X_k|²/S1²` with `S1 = Σw[n]`, on
the stated grounds that a full-scale sine then reads its own mean square, −3.01 dBFS. It does —
*in its peak bin*. But §7.1's cell value is the **sum** of the bins in a band, and a Hann window
puts a third of a tone's energy in the two bins either side of the peak: `0.5 + 0.125 + 0.125`
under that normalisation, against a true mean square of `0.5`. So every band value reads
**+1.7609 dB = 10·log₁₀(1.5)** high — uniformly, for a tone and for noise alike, since the factor
is `(N·S2)/S1² = 1.5` for any signal whose energy is captured whole. The intent is the sentence
*after* the formula — *the same dBFS convention as the meter, which is what lets one calibration
offset shift both* — and only the noise-power normalisation `2/(N·S2)` delivers it for a sum.
`spectrum.rs` uses that, and the property it buys is now a test: **the 32 bands add back up to
the signal's own unweighted dBFS**. §16.4 and §17 carry the correction. It would have been
invisible on screen — a uniform 1.8 dB shift of a 60 dB colour window, on a window §7.1 already
flags as the value most likely to want moving — which is exactly why §16 exists to be checked
against code rather than believed.

**Finding 2 — which band *borrows* is a function of the sample rate, and it is not the 12.5 Hz
one at 48 kHz.** §13.12 and this ticket both name the 12.5 Hz band as the borrowed row, on the
correct observation that it is 2.9 Hz wide against 5.86 Hz bins. The conclusion does not follow:
a band narrower than a bin borrows only when no bin centre happens to land inside it, which
depends on where the grid falls. At **48 kHz** the 12.5 Hz band keeps bin 2 (11.72 Hz) and the
one borrowing row is **20 Hz**, which has no centre in `[17.8, 22.4)` and takes bin 3 — the same
bin the 16 Hz band already sums. At **44.1 kHz** the 12.5 Hz band does borrow, taking bin 2 at
10.77 Hz, *below its own lower edge*, and nothing else does. So the fabricated row moves under a
route change. The rate-independent statement is stronger and is what §13.12 now says: **four rows
are narrower than a bin** (12.5, 16, 20, 25 Hz), and every one of them reports a bin's worth of
spectrum through a narrower window whether it borrows or not. §7.1's "resolves every band from
16 Hz up" is corrected to 31.5 Hz in the same pass.

**Finding 3 — the residual ripple in a flat picture is band-edge quantisation, and it is
predictable in closed form.** A band sums a whole number of bins, so it measures `n·df` where its
edges ask for `f_hi − f_lo`. Under 0.1 dB at 1 kHz, ±0.8 dB around 250 Hz, and **−1.95 dB at
40 Hz**, where the band wants 1.57 bins and gets 1. §14.3's *pink noise draws flat* therefore
means flat **around that prediction** — the same shape of correction `b02` finding 1 made to the
weighting table's tolerance, and for the same reason: against a straight line the measured pink
spectrum spans 1.9 dB and the whole of the excess is that one row. Not fixed, deliberately:
fractional edge weighting would re-litigate §16.5, and 1 dB of ripple is under 2 % of a 60 dB
scale. **White noise gets the companion test §14.3 does not have** — it must rise exactly
`10·log₁₀(n_bins)` band by band, which pins the assignment row by row and kills `mean` as a cell
value outright, since a mean would draw it flat.

**Finding 4 — the FFT tap needs its own overflow rule, and it is not the block queue's.** The
callback copies raw channel-0 samples into a second `rtrb` queue, sized at the same ~1 s as the
block queue so both pipelines survive exactly the same stall. But overflow means something
different on each: a lost *block* is lost coverage, a smaller honest sample; a lost *sample* is a
hole inside a 171 ms frame, and rtrb drops the **newest** on a full queue, so what survives is
also *older* than the slot it would be drawn in. `Spectrum::sync_stream` therefore treats any
movement in `Capture::lost_samples` exactly as it treats a rebuild — a discontinuity, frame
buffer discarded, holes drawn — rather than splicing. Same reasoning as §16.8's zero-power rule,
applied to the queue instead of the microphone.

**The slot assignment is the one piece of design the spec left open, and it is: `now_slot`
places the columns, the sample count paces them.** One hop is exactly one slot of audio, so `k`
frames completed in a drain are the `k` slots ending at `now_slot`. That is what makes §9.2's
"every real column since the last publish" *correct* rather than merely robust, and it keeps
placement on the wall clock exactly as `metrics.rs` does — a sample-counted timebase would stop
advancing during a gap and then draw the audio that followed it in the past. The consequence
worth knowing: a tick that completes **zero** hops files nothing, and the next tick fills that
slot retroactively as `now_slot − 1`. Reading the ring at the publish instant therefore shows
holes that are not holes, which is how the live pass first read — and measuring 20 slots ending
two behind the present instead showed **20 of 20, sustained**, with `lost_samples` at 0
throughout. There is no venetian blind.

**Measured live, at 48 kHz mono on the built-in microphone:** a quiet room peaks in the **50 Hz
row at −53 dBFS** with everything above 1 kHz below −80, which is the rumble stripe the picture
exists to show, present before anyone has said anything. Speech moves the peak to the 250–400 Hz
rows at −30 to −40 dBFS and lifts 8 kHz by 40 dB. The whole drain-and-transform costs **50–125 µs
per tick in release** (3–5 ms in debug), one 8192-point real transform included — 0.1 % of a
100 ms tick, which settles §16.1's throughput argument with room to spare for a phone.

**Untested and said so.** The picture is not on the wire — `columns` is still an empty array and
`get_spectrogram` does not exist, both [`b10`](b10-the-spectrogram-half-of-the-bridge.md)'s. So
nothing here has been *seen*, and §14.3's two eyeball tests are asserted numerically rather than
looked at; the arm's-length legibility question the map has been carrying is untouched and stays
[`b11`](b11-the-spectrogram-canvas.md)'s to answer before polishing. Rate-change re-derivation of
the bands has a unit test but has never run on a real route change, which is the same probe 3
gesture `b07` and `b08` both left open. And nothing has run on the phone.
