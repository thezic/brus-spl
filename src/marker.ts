// The marker's arithmetic — which row a touch is on, which slot it pins to, where it is drawn now
// and where its label fits. Spec §7.5, ticket [#15](https://github.com/thezic/brus-spl/issues/15).
//
// Separated from `PictureMarker.vue` for the same reason `spectrogram.ts` is separated from
// `Spectrogram.vue`: none of it touches the DOM, so the half that can be checked by reading it is
// readable on its own. The component owns the pointer events and the elements; this file owns
// *which row is that, which slot does it name, and where does the chip fit*.
//
// **Nothing here does dB arithmetic and nothing here holds a column** (spec §8.2, §9.5). The level
// in [`MarkerLevel`] arrives calibrated from Rust through `getSlotLevels` and is carried, not
// computed — this file never sees a band value at all. The prototype's hit-assist candidates
// (`peak1`/`peak2`) are gone with the decision that refused them: they needed per-band history on
// this side, which is the thing §9.5 does not allow.

import type { Unit } from "./bridge";
import { BANDS, type Geometry } from "./spectrogram";

/**
 * A placed marker. **Pinned to a `(slot, band)`, never to a pixel** (spec §7.5) — a slot's identity
 * never moves, which is what makes the marker survive a re-pull, a rotation, expanding and
 * reopening while every pixel under it is redrawn.
 *
 * There is deliberately **no continuous row position** here. The prototype carried one for the
 * interpolate candidate, and §7.5 refused it: 32 energy-summed rows cannot distinguish the
 * frequencies an interpolated readout would print, so a fractional row has nothing left to say.
 */
export interface Marker {
  /** Absolute slot, counted from slot 0 — the middle of the bucket that was touched. */
  slot: number;
  /** The row, `0` = 12.5 Hz at the bottom of the picture. */
  band: number;
  /** The readout's level half, resolved **once**, when the marker was placed. */
  level: MarkerLevel;
}

/**
 * What the readout can say about the level, and **the four nothings are four different claims**.
 *
 * `gap` and `outside` are spec §7.5's *an evicted slot and a gap must not read as the same thing*:
 * Rust answers `None` to both, and *no data kept* and *there was nothing here* are different
 * statements about the room. §6.9's refusal to publish a fake quiet applies to a readout exactly as
 * it applies to the hero number, so neither may render as a level and neither may render as blank.
 *
 * **The unit rides with the value** rather than being read from the live settings at paint time.
 * §7.5 makes the label static once placed, and the *unit* is part of the label: a level fetched
 * uncalibrated is a raw `dBFS/band` number, and calibrating afterwards must not silently relabel it
 * `dB/band`. This is spec §9.2's *the unit travels with the numbers* applied to a number that
 * outlives the tick it came from.
 */
export type MarkerLevel =
  /** Placed, and the round trip has not answered yet — only ever true while a drag is live. */
  | { state: "pending" }
  | { state: "level"; db: number; unit: Unit }
  /** In the ring and empty: spec §7.3's hole, which the picture draws as background. */
  | { state: "gap" }
  /** Outside the 120 s ring — the *evicted* case, which is the only one a marker can reach. */
  | { state: "outside" }
  /** The command itself failed. Not a claim about the room, so it must not look like one. */
  | { state: "failed" };

/** A row index clamped into the picture — a drag may leave the plot, a band may not. */
export function clampBand(band: number): number {
  return Math.min(BANDS - 1, Math.max(0, Math.round(band)));
}

/**
 * The row a `y` in the picture box lands on.
 *
 * Band 0 is the **bottom** row, so the coordinate is measured up from the plot's bottom edge — the
 * same flip `paintColumn` makes on the way into the image, and the same one `drawChrome` makes when
 * it places a frequency label. Rounding is what makes the readout *snap*: §7.5's whole argument for
 * the snapped label is that the row is the finest thing the picture can name.
 */
export function bandAt(g: Geometry, y: number): number {
  return clampBand((BANDS * (g.plotY + g.plotH - y)) / g.plotH - 0.5);
}

/** The centre `y` of a row in the picture box — [`bandAt`]'s inverse at the integers. */
export function rowCentre(g: Geometry, band: number): number {
  return g.plotY + (g.plotH * (BANDS - 1 - band + 0.5)) / BANDS;
}

/** The bucket at the right edge — the same derivation `Spectrogram.vue`'s `advance` makes. */
function rightBucket(g: Geometry, nowSlot: number): number {
  return Math.floor(nowSlot / g.slotsPer);
}

/**
 * The absolute **bucket** an `x` in the picture box names.
 *
 * Clamped to the drawn columns at both ends: the plot's right edge is a legal place to end a drag
 * and an unclamped `floor` puts it one bucket *past* the present, which is a slot the picture has
 * never drawn and which Rust correctly answers `outside` for — so the marker would vanish the
 * instant it was placed.
 */
export function bucketAt(g: Geometry, nowSlot: number, x: number): number {
  const raw = Math.floor(((x - g.plotX) / g.plotW) * g.buckets);
  const fromLeft = Math.min(g.buckets - 1, Math.max(0, raw));
  return rightBucket(g, nowSlot) - (g.buckets - 1 - fromLeft);
}

/**
 * The slot a touch pins to: the **middle** slot of the bucket under it (spec §7.5).
 *
 * A bucket is one drawn column and can hold several slots (spec §7.3's pixel budget), so a touch
 * names a bucket and the marker has to name a slot — the anchor is a slot precisely so that
 * re-bucketing cannot move it. The bucket's *first* slot would work everywhere except the seam:
 * when a resize halves `slotsPer` a first-slot anchor always re-lands in the earlier of the two
 * buckets that replace it, so the marker jumps one column left for no reason a reader can see. The
 * middle re-lands in whichever new bucket actually contains the moment.
 *
 * **Never past the present**, which the middle alone does not guarantee: the rightmost bucket is
 * still filling, so at `slotsPer = 2` its middle is `now + 1` half the time — a slot Rust answers
 * `outside` for, because it is *ahead* of the ring rather than aged out of it. The readout would
 * then say *not kept* about the newest column on the picture. The clamp stays inside the touched
 * bucket by construction (`rightBucket · slotsPer ≤ now`), so it costs the anchor nothing.
 */
export function slotAt(g: Geometry, nowSlot: number, x: number): number {
  const bucket = bucketAt(g, nowSlot, x);
  return Math.min(nowSlot, bucket * g.slotsPer + Math.floor(g.slotsPer / 2));
}

/**
 * Where a pinned slot is drawn **now**, as an `x` in the picture box — the centre of its column.
 *
 * `null` once the slot has scrolled off the left edge, which is spec §7.5's *vanishes at the left
 * edge*: the caller drops the marker rather than hiding it. Also `null` to the right of the edge,
 * which a marker cannot normally reach — [`bucketAt`] clamps — but a shrinking span can leave a
 * slot briefly ahead of the picture's own right edge between a reconfigure and the tick that
 * follows it.
 */
export function columnX(g: Geometry, nowSlot: number, slot: number): number | null {
  const fromLeft =
    g.buckets - 1 - (rightBucket(g, nowSlot) - Math.floor(slot / g.slotsPer));
  if (fromLeft < 0 || fromLeft >= g.buckets) return null;
  return g.plotX + (g.plotW * (fromLeft + 0.5)) / g.buckets;
}

/** Is the point inside the plot? The gutters are not the picture, and a press there places nothing. */
export function insidePlot(g: Geometry, x: number, y: number): boolean {
  return (
    x >= g.plotX &&
    x <= g.plotX + g.plotW &&
    y >= g.plotY &&
    y <= g.plotY + g.plotH
  );
}

/** The gap from the marker to the chip, and the chip's clearance from the plot edges, in CSS px. */
export const CHIP_GAP = 12;
export const CHIP_PAD = 4;

/**
 * Where the label sits: **beside the marker, flipped to its left near the right edge, and clamped
 * inside the plot** (spec §7.5).
 *
 * Beside rather than above, because above costs no width and walks straight into the frequency
 * gutter as the marker drifts left; beside costs the width the marker was placed to look at and
 * stays over the picture. The **clamp** is what §7.5 asks for by name: unclamped, the chip keeps its
 * offset from the marker and measured 75 px off the left edge of the picture in the prototype —
 * over the frequency labels and partly off the canvas. Clamped, the chip stops at the plot edge and
 * the marker slides out from under it: the readout stays readable and stops pointing precisely,
 * which is the better half of the trade for a label that is already static.
 *
 * `w`/`h` are the chip's **measured** size, so this runs after the browser has laid the text out —
 * `12.5 Hz · no data` and `3.15 kHz · −52.3 dBFS/band` differ by more than a factor of two, and
 * guessing would clamp the wrong box.
 */
export function placeChip(
  g: Geometry,
  mx: number,
  my: number,
  w: number,
  h: number,
): { left: number; top: number } {
  const left = g.plotX + CHIP_PAD;
  const right = g.plotX + g.plotW - CHIP_PAD;
  const top = g.plotY + CHIP_PAD;
  const bottom = g.plotY + g.plotH - CHIP_PAD;
  const flipped = mx + CHIP_GAP + w > right;
  return {
    left: clamp(flipped ? mx - CHIP_GAP - w : mx + CHIP_GAP, left, Math.max(left, right - w)),
    top: clamp(my - h / 2, top, Math.max(top, bottom - h)),
  };
}

function clamp(value: number, lo: number, hi: number): number {
  return Math.min(hi, Math.max(lo, value));
}
