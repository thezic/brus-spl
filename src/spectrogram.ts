// The picture's arithmetic — the colour ramp, the dB window and the column-into-pixel budget.
// Spec §7.1 and §7.3, ticket `b11`.
//
// Separated from `Spectrogram.vue` because none of it touches the DOM: every function here is a
// pure mapping from numbers to numbers, which is the half of the canvas that can be reasoned
// about by reading it. The component owns the two canvases and the events; this file owns *what
// colour is that, and how many slots fit in a pixel*.
//
// **Nothing here applies the calibration offset.** Band values cross the bridge raw (spec §7.1),
// and the colour window is fixed in dBFS, so the offset shifts the window and the values by the
// same amount and every colour is unchanged. Since `b15` removed the legend the offset is not
// visible anywhere in the picture at all — calibrating relabels nothing here and moves no pixel.

/** 32 fixed one-third-octave rows, mirroring `BANDS` in `src-tauri/src/spectrum.rs`. */
export const BANDS = 32;

/**
 * The fixed colour window, in raw dB re FS (spec §7.1). **60 dB, and no auto-ranging** — the same
 * colour always means the same absolute level, because auto-ranging would make a quiet room and a
 * loud one look identical, which is the one thing a picture of levels must not do.
 *
 * ≈10 → 70 dB SPL per band once §8's ~+100 dB offset is applied. Spec §7.1 names this as the
 * value most likely to want moving once real speech in a real room has gone through it, so it is
 * two constants rather than a pair of literals buried in the ramp lookup.
 */
export const WINDOW_LO_DBFS = -90;
export const WINDOW_HI_DBFS = -30;

/**
 * Inferno, at its ten deciles. **Monotonic in lightness, and never `jet`** (spec §7.1): it
 * survives being read at an angle in a dim room, where `jet` invents banding the data does not
 * have. A quantised 10 dB ladder was built and rejected upstream — reading levels is the number's
 * job, and quantising turned the noise floor into blocks.
 *
 * Ten anchors rather than the full 256-entry table because the ramp's own curvature is well under
 * a display step between deciles, and a table this size can be read and checked against
 * matplotlib's own hex samples by eye.
 */
const INFERNO: readonly (readonly [number, number, number])[] = [
  [0, 0, 4], // #000004
  [27, 12, 65], // #1b0c41
  [74, 12, 107], // #4a0c6b
  [120, 28, 109], // #781c6d
  [165, 44, 96], // #a52c60
  [207, 68, 70], // #cf4446
  [237, 105, 37], // #ed6925
  [251, 155, 6], // #fb9b06
  [247, 209, 61], // #f7d13d
  [252, 255, 164], // #fcffa4
];

/**
 * The ramp expanded to 256 steps, `rgb` triples — built once, so painting a column is a table
 * lookup per band rather than an interpolation.
 */
const RAMP = (() => {
  const ramp = new Uint8Array(256 * 3);
  const last = INFERNO.length - 1;
  for (let step = 0; step < 256; step++) {
    const t = (step / 255) * last;
    const anchor = Math.min(Math.floor(t), last - 1);
    const fraction = t - anchor;
    const lo = INFERNO[anchor];
    const hi = INFERNO[anchor + 1];
    for (let channel = 0; channel < 3; channel++) {
      ramp[step * 3 + channel] = Math.round(
        lo[channel] + fraction * (hi[channel] - lo[channel]),
      );
    }
  }
  return ramp;
})();

/**
 * Where a raw band level lands on the ramp, as an index into [`RAMP`].
 *
 * Clamped at both ends rather than left to run off the scale: over the top is *the loudest colour*
 * and under the bottom is *the quietest colour*, which is what a fixed window means. A value that
 * clamps is still a real measurement — it is only the colour that has run out.
 */
export function rampStep(dbfs: number): number {
  const t = (dbfs - WINDOW_LO_DBFS) / (WINDOW_HI_DBFS - WINDOW_LO_DBFS);
  if (!(t > 0)) return 0; // also catches NaN, which must not index the table
  if (t >= 1) return 255;
  return Math.round(t * 255);
}

// `rampColour` lived here to draw the legend's strip a rect at a time. **The legend is gone**
// (`b15`, spec §11.7), so the ramp has exactly one consumer left — [`paintColumn`], which indexes
// [`RAMP`] directly — and a CSS-colour accessor with no caller is how a reverted decision creeps
// back in.

/**
 * Writes one column of band levels into an RGBA buffer, **row 0 at the top of the image**.
 *
 * The picture reads like the axis beside it: band 31 (16 kHz) at the top, band 0 (12.5 Hz) at the
 * bottom, so the row index is flipped on the way in and nowhere else.
 *
 * `stride` is the image width in pixels, so this serves both the one-pixel column an append
 * writes and the full-width image a redraw builds.
 */
export function paintColumn(
  rgba: Uint8ClampedArray,
  x: number,
  stride: number,
  level: (band: number) => number,
): void {
  for (let band = 0; band < BANDS; band++) {
    const at = rampStep(level(band)) * 3;
    const pixel = ((BANDS - 1 - band) * stride + x) * 4;
    rgba[pixel] = RAMP[at];
    rgba[pixel + 1] = RAMP[at + 1];
    rgba[pixel + 2] = RAMP[at + 2];
    // Opaque. A **gap is left at alpha 0** and shows the page through (spec §7.3): background,
    // visibly absent, never a low-level colour — otherwise a dead stream reads as a peaceful room.
    rgba[pixel + 3] = 255;
  }
}

/**
 * How many 100 ms slots share one pixel column — spec §7.3's *aggregate deliberately, never let
 * the resampler decimate*.
 *
 * 60 s of 100 ms columns is 600 columns into a few hundred pixels. Dropping the ones that do not
 * fit produced a **venetian-blind** picture upstream, and the blind is not in the sound: it is the
 * scaler choosing which slots survive. So slots are grouped into buckets and averaged **in
 * energy**, and the bucket is sized so the grouped picture is never wider than the pixels it has —
 * which means the canvas only ever magnifies, and magnification cannot drop a column.
 *
 * **The frontend does this** because it is the only side that knows the canvas width (spec §7.3).
 */
export function slotsPerBucket(spanSlots: number, widthPx: number): number {
  return Math.max(1, Math.ceil(spanSlots / Math.max(1, widthPx)));
}

/**
 * The absolute bucket a slot belongs to.
 *
 * **Anchored to slot 0, not to the right edge**, which is the whole reason appending works: a
 * bucket's identity does not move when the picture scrolls, so a column that arrives late lands in
 * the same place it would have landed on time.
 */
export function bucketOf(slot: number, slotsPerBucket: number): number {
  return Math.floor(slot / slotsPerBucket);
}

/** dB → linear power. Aggregation is in energy; averaging the logs would not be the mean. */
export function power(db: number): number {
  return 10 ** (db / 10);
}

/** Linear power → dB. */
export function decibels(power: number): number {
  return 10 * Math.log10(power);
}
