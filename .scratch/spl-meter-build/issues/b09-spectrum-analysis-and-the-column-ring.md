# Spectrum analysis and the column ring

Parent: [SPL Meter Build](../map.md)
Type: build
Status: open
Blocked by: [`b08`](b08-tier-1-device-pass.md)

## Build

`spectrum.rs`: FFT frame assembly, third-octave banding, and the column ring. Spec
[§7.1](../../spl-meter-mvp/spec.md#71-parameters) and
[§16](../../spl-meter-mvp/spec.md#16-what-this-spec-decides-that-no-ticket-decided) items 1, 4
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
