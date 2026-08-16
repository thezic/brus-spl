<script setup lang="ts">
// The picture — spec §7 and §11.7, ticket `b11`.
//
// **A scrolling spectrogram: time flows right→left with *now* at the right edge, on 32 fixed
// one-third-octave bands.** It is a spectrogram, not a spectrum, and that is division of labour
// rather than taste — the number already answers *how loud*, so the only thing the picture
// uniquely adds is *what is making it and whether it has been steady*. A constant HVAC source and
// a passing door slam are indistinguishable on bars and unmistakable here.
//
// ```
//     ┌──────────────────────────┐
//  16k│ ░░  ░▒░   ░░░  ░▒▒░      │
//   8k│ ░░  ░▒░   ░░░  ░▒▒░      │
//    ⋮│            ⋮             │
//   63│██████████████████████████│
// 31.5│██████████████████████████│
//   16│██████████████████████████│
//     └──────────────────────────┘
//      −60s · unweighted · dB/band  now
// ```
//
// **Two canvases, and the split is the whole design.** The *data* canvas is `buckets × 32` — one
// pixel per drawn column, one pixel per band — and is the only thing that is ever appended to or
// scrolled. The *visible* canvas holds the chrome (frame, frequency labels, caption line) and
// gets one `drawImage` of the data canvas per changed tick, with smoothing off. That is spec
// §7.3's *repaint by appending a column and scrolling, not by redrawing the history* — redrawing
// everything costs ~8 ms per frame at 800 columns even in a desktop browser, which is wasteful in
// a phone webview at 10 Hz for a picture that changes by one column.
//
// **The frontend holds no authoritative state**, here as everywhere: the two canvases are pixels,
// not a model. There is deliberately no column ring on this side — Rust owns the history, so every
// event that invalidates the canvas is one move, *pull again*, rather than four different repairs
// (spec §9.5).

import { computed, onMounted, onUnmounted, ref, watch } from "vue";

import {
  getSpectrogram,
  pictureNeedsPull,
  SLOT_MS,
  type Column,
  type Settings,
  type Tick,
} from "../bridge";
import {
  bandUnit,
  PICTURE_WEIGHTING,
  SPAN_END,
  spanStart,
} from "../display";
import {
  BANDS,
  bucketOf,
  decibels,
  type Geometry,
  labelLadder,
  paintColumn,
  power,
  slotsPerBucket,
} from "../spectrogram";

const props = defineProps<{
  /** The last tick. `now_slot` is what makes the right edge honest; `columns` is what is drawn. */
  tick: Tick | null;
  /** The span and the caption's unit — all the chrome reads. `null` until the first tick. */
  settings: Settings | null;
  /**
   * The box's height in CSS pixels, gutters included — [`BOX_H`] inline (spec §11.7) and whatever
   * the screen affords when expanded (spec §7.4). **The only geometry input**: the plot's height,
   * the label ladder and the gutter's width all follow from it and from the measured width.
   */
  height?: number;
}>();

/**
 * The plot rect and pixel budget, in CSS px, emitted **whenever [`configure`] succeeds** — spec
 * §7.5.
 *
 * An event rather than an exposed method because it changes exactly when `configure` runs and a
 * consumer must not sample it at any other moment: between a reconfigure and the pull that follows,
 * the canvas is empty and the old rect is a lie. Nothing in this component consumes it — it is the
 * seam the expanded view (#14) and the marker overlay (#15) are built on, and it is here rather
 * than there because the gutter's width is now a measurement only this component makes.
 */
const emit = defineEmits<{ geometry: [Geometry] }>();

// ─── geometry, in CSS pixels (spec §11.7, and §7.4 for what is no longer constant) ──────────
//
// **Three of these were constants and are now inputs** (#13): the box's height is a prop, the
// label ladder is derived from it, and the left gutter is measured from the ladder. Nothing about
// the inline picture moves — at 205 px each rule reproduces the value §11.7 records — but the
// component no longer has one geometry, so it publishes the one it has (spec §7.5).
//
// **Two gutters, not three** (`b15`). 40 px left for the frequency labels and 20 px bottom for the
// time axis and the caption line. The 58 px right gutter held the colour legend and **the legend
// is gone** — removed on the owner's call because it delivered no value, which is a different
// argument from the space trade `09` d9 declined. The plot takes all but 2 px of that width, and
// the 2 px are only so the frame's right stroke is never the canvas's last device-pixel column.
//
// §7.1 loses nothing in substance: the dB window is still fixed and nothing auto-ranges, so the
// same colour still *is* the same absolute level. What is gone is the only thing on screen that
// named those levels — which is why `dBFS/band` stays on the caption line below.

const BOX_H = 205;
/** The **floor**, not the value: the gutter widens for a wider label and never narrows below this. */
const GUTTER_LEFT = 40;
const GUTTER_RIGHT = 2;
const AXIS_H = 20;

/** Label size, and the mono stack `:root` sets — canvas takes a font string, not a CSS variable. */
const LABEL_PT = 10;
const MONO = 'ui-monospace, SFMono-Regular, Menlo, monospace';

/**
 * What the gutter holds beside the label itself: the gap from the label's right edge to the plot,
 * the tick drawn inside that gap, and the clear space left of the longest label.
 *
 * Named because the gutter is now **measured** against them (spec §7.4) rather than fixed at 40.
 * The gap was a literal in [`drawChrome`] and reappeared inside a `+ 11` in [`measuredGutter`] —
 * two places to change and one of them silent, since widening the gap alone would clip the very
 * labels the gutter was measured for.
 */
const LABEL_GAP = 7;
const TICK_W = 4;
const GUTTER_MARGIN = 4;

/**
 * The box and the plot inside it, in CSS px — both a function of the `height` prop (spec §7.4).
 *
 * Floored at an axis plus one device row per band: below that the rows stop being rows, and the
 * clamp keeps the element's own height agreeing with the canvas that is drawn into it.
 */
const boxH = computed(() => Math.max(AXIS_H + BANDS, props.height ?? BOX_H));
const plotHCss = computed(() => boxH.value - AXIS_H);

/**
 * The rows that get a frequency label, top first — **derived from the plot's height** by
 * [`labelLadder`](../spectrogram.ts), recomputed in [`configure`], which is the only place that
 * height can change.
 *
 * `b13` read the four-label axis as *not granular enough* and `b15` answered it with eleven, chosen
 * by hand for a 205 px box: the octave ladder, every third band, ~17 px apart, having measured
 * every *second* band at ~11.4 px as unreadable in a dim room at arm's length. What that fixed is a
 * **pitch**, and the count only followed from the one height the picture then had — so #13 keeps
 * the judgement and lets the count follow the pixels (spec §7.4). At 205 px it is still exactly
 * `b15`'s eleven, by construction rather than by agreement.
 */
let ladder: [number, string][] = labelLadder(BOX_H - AXIS_H);

const frame = ref<HTMLDivElement>();
const surface = ref<HTMLCanvasElement>();

/**
 * The data canvas: one pixel per drawn column, one per band, sized in [`configure`].
 *
 * Never in the document. Its pixels *are* the picture — the visible canvas only magnifies it.
 */
const data = document.createElement("canvas");
const dataCtx = data.getContext("2d");
/** One reused column of pixels, for the append path. */
const tail = new ImageData(1, BANDS);

let ctx: CanvasRenderingContext2D | null = null;
let dpr = 1;
let plotX = 0;
let plotY = 0;
let plotW = 0;
let plotH = 0;
/** Slots per drawn column, and drawn columns across the picture — spec §7.3's pixel budget. */
let slotsPer = 1;
let buckets = 1;
/** The measured left gutter in CSS px — [`GUTTER_LEFT`] unless a wider label needs more. */
let gutterLeft = GUTTER_LEFT;
let palette = { line: "#26262e", dim: "#8e8e96", faint: "#70707a" };

/**
 * The absolute bucket at the right edge, or `null` before anything has been drawn.
 *
 * Absolute rather than an offset from the right: a bucket's identity does not move when the
 * picture scrolls, which is what lets a column that arrives a tick late land where it belongs.
 */
let rightBucket: number | null = null;
/** The rightmost bucket's running **energy** sums, and how many real columns are in them. */
const accumulated = new Float64Array(BANDS);
let accumulatedCount = 0;
/** The last `now_slot` any tick reported — the right edge, and what a rebuild anchors to. */
let lastNowSlot = 0;
let dirty = false;

/** Columns that arrived while a pull was in flight; replayed onto the rebuilt canvas. */
let held: Column[] = [];
/** Only the newest pull may touch the canvas — a resize during a window change starts two. */
let pullToken = 0;
let pulling = false;

let resizeTimer = 0;
let observer: ResizeObserver | null = null;
let measuredWidth = 0;
/** The settings the chrome was last drawn for; the watcher's own memory of what changed. */
let shown: Settings | null = null;

/** The span in 100 ms slots (spec §7.1: **the span follows the L_eq window**). */
function spanSlots(settings: Settings | null): number {
  return ((settings?.window_s ?? 60) * 1000) / SLOT_MS;
}

/**
 * What the screen reader is told, since the picture itself is pixels.
 *
 * It says *unweighted* for the same reason the caption line does — spec §7.2 is a property of the
 * quantity, not of the rendering.
 */
const description = computed(() => {
  const span = props.settings?.window_s ?? 60;
  return `Spectrogram: the last ${span} seconds, unweighted, 12.5 Hz to 16 kHz`;
});

/** One of `:root`'s colours, so the picture's chrome cannot drift from the rest of the screen. */
function ink(name: string, fallback: string): string {
  const value = getComputedStyle(document.documentElement)
    .getPropertyValue(name)
    .trim();
  return value || fallback;
}

/**
 * How wide the left gutter has to be for the ladder it is about to hold, in CSS pixels.
 *
 * **40 px is a floor and not a value** (spec §7.4). §11.7's figure was measured against `b15`'s
 * widest string — `31.5`, 24 px at 10 px mono — plus the gap the tick sits in; a denser ladder
 * brings five-character strings (`3.15k`, `12.5k`), so the gutter is measured rather than assumed.
 * It never narrows below 40: the inline picture keeps the geometry §11.7 records, and a picture
 * whose left edge moved when a label got shorter would be a picture that jitters.
 *
 * The widest label plus [`LABEL_GAP`] is what it takes for nothing to *clip*; [`GUTTER_MARGIN`] is
 * on top of that so the longest label is not flush against the canvas's own left edge.
 *
 * Measured in device pixels and divided back, because that is the font the labels are actually
 * drawn in.
 */
function measuredGutter(g: CanvasRenderingContext2D): number {
  g.font = `${Math.round(LABEL_PT * dpr)}px ${MONO}`;
  let widest = 0;
  for (const [, name] of ladder) {
    widest = Math.max(widest, g.measureText(name).width / dpr);
  }
  return Math.max(GUTTER_LEFT, Math.ceil(widest) + LABEL_GAP + GUTTER_MARGIN);
}

/**
 * Sizes both canvases for the current width and span, and redraws the chrome. **Clears the
 * picture** — every caller follows it with a pull.
 *
 * Returns false when there is nothing to measure yet (a hidden or zero-width host), so mount
 * order cannot leave a 1-pixel canvas behind.
 */
function configure(): boolean {
  const host = frame.value;
  const canvas = surface.value;
  if (!host || !canvas || !dataCtx) return false;

  const width = Math.round(host.clientWidth);
  if (width <= 0) return false;
  measuredWidth = width;

  // Capped at 3: the tallest ratio any target here reports, and a bound on the backing store of a
  // canvas that can be 1400 px wide in a desktop window.
  dpr = Math.min(3, Math.max(1, window.devicePixelRatio || 1));
  const px = (css: number) => Math.round(css * dpr);

  canvas.width = px(width);
  canvas.height = px(boxH.value);
  ctx = canvas.getContext("2d");

  // **The height picks the ladder, and the ladder picks the gutter** — in that order, and there is
  // no circularity in it: the plot's height depends only on the box, the labels only on that
  // height, and the gutter — the one thing that depends on the labels — takes width, not height.
  plotH = Math.max(BANDS, px(plotHCss.value) - 2);
  ladder = labelLadder(plotH / dpr);
  gutterLeft = ctx ? measuredGutter(ctx) : GUTTER_LEFT;

  // One device pixel of inset all round, so the frame drawn just outside the plot survives the
  // `clearRect` every blit does inside it.
  plotX = px(gutterLeft);
  plotY = 1;
  plotW = Math.max(1, px(width - gutterLeft - GUTTER_RIGHT) - 1);

  // **The pixel budget** (spec §7.3): group slots until the grouped picture fits the pixels it
  // has, so the magnification below can never drop a column. 600 slots into 350 px is two slots
  // per column, averaged in energy — *not* 250 slots thrown away, which is the venetian blind.
  const span = spanSlots(props.settings);
  slotsPer = slotsPerBucket(span, plotW);
  buckets = Math.max(1, Math.ceil(span / slotsPer));

  // Assigning width or height clears the canvas, which is exactly what a reconfigure wants.
  data.width = buckets;
  data.height = BANDS;
  rightBucket = null;
  accumulated.fill(0);
  accumulatedCount = 0;

  palette = {
    line: ink("--line", palette.line),
    dim: ink("--ink-dim", palette.dim),
    faint: ink("--ink-faint", palette.faint),
  };

  drawChrome();
  // Derived from the device-pixel values just drawn rather than recomputed from the CSS ones, so
  // an overlay's rect *is* the plot's rect and not a second rounding of it (spec §7.5).
  emit("geometry", {
    plotX: plotX / dpr,
    plotY: plotY / dpr,
    plotW: plotW / dpr,
    plotH: plotH / dpr,
    boxW: width,
    boxH: boxH.value,
    slotsPer,
    buckets,
  });
  return true;
}

/**
 * The frame, the frequency labels and the caption line. Everything on the visible canvas that is
 * not the picture.
 *
 * Redrawn only when the geometry or a labelled setting changes — it clears the whole canvas, so
 * every caller follows it with a [`blit`].
 */
function drawChrome(): void {
  const canvas = surface.value;
  if (!ctx || !canvas) return;
  const g = ctx;
  const px = (css: number) => Math.round(css * dpr);
  g.clearRect(0, 0, canvas.width, canvas.height);

  // The frame, one device pixel, drawn *outside* the plot rect.
  g.fillStyle = palette.line;
  g.fillRect(plotX - 1, plotY - 1, plotW + 2, 1);
  g.fillRect(plotX - 1, plotY + plotH, plotW + 2, 1);
  g.fillRect(plotX - 1, plotY - 1, 1, plotH + 2);
  g.fillRect(plotX + plotW, plotY - 1, 1, plotH + 2);

  g.font = `${px(LABEL_PT)}px ${MONO}`;
  g.textBaseline = "middle";

  // Frequency labels, right-aligned into the left gutter against a tick on their own row. Band 0
  // (12.5 Hz) is the bottom row, so the row's centre is measured from the top downwards. **Every
  // label's y comes from its own band's centre** — the eleven of them are not laid out on an even
  // ladder of their own that happens to look close, which is §7.3's *band edges are drawn crisp*
  // carried into the axis. The tick rounds to the nearest device pixel because a 1 px line has to,
  // and that is the only rounding in the path.
  g.textAlign = "right";
  g.fillStyle = palette.dim;
  for (const [band, name] of ladder) {
    const centre = plotY + (plotH * (BANDS - 1 - band + 0.5)) / BANDS;
    // The tick sits on the row; the **label** is clamped to stay inside the picture's height. The
    // top row's centre is under 3 px from the edge, and an unclamped baseline there loses the top
    // half of `16k` off the canvas — a label that names the row it is level with is worth half a
    // pixel of parallax at the two ends.
    const text = Math.min(
      Math.max(centre, plotY + px(LABEL_PT / 2)),
      plotY + plotH - px(LABEL_PT / 2),
    );
    g.fillText(name, plotX - px(LABEL_GAP), text);
    g.fillRect(plotX - px(TICK_W), Math.round(centre), px(TICK_W), 1);
  }

  // **No colour legend** (`b15`). Nothing is drawn to the right of the plot at all — the ramp
  // strip and its two end-labels are gone, and the width they cost is in the picture instead.

  // The caption line, and it is now **two anchors rather than three**. `−60s … now` carries the
  // span a second time (spec §11.7); **`unweighted` is said out loud** because a reader comparing
  // the picture band-by-band against a dB(A) number will otherwise conclude the app is
  // inconsistent (spec §7.2); and `dBFS/band` stays because with the legend gone it is the only
  // thing left saying what a colour is a quantity *of*.
  //
  // The three strings used to be anchored left / plot-right / canvas-right, and `now` had to be
  // pulled 4 px inside the plot edge so it did not touch the legend caption. Both of those wanted
  // the right-hand side and the plot now owns it, so the unit joins the left run — where §7.2's
  // sentence and §7.1's unit read as one phrase — and `now` alone sits at the plot's right edge,
  // which is where §7 puts the present.
  //
  // The uncalibrated run is the widest at ~175 px (`dBFS/band` is four characters longer than
  // `dB/band`), which clears `now` with ~100 px to spare on the narrowest portrait phone and in
  // the 800×600 wide reflow alike. It only closes up under a ~230 px canvas — a 667 px-wide
  // landscape — which the three-anchor version had already run out of room in.
  const settings = props.settings;
  const baseline = px(plotHCss.value) + px(AXIS_H / 2) + 1;
  g.textAlign = "left";
  g.fillStyle = palette.faint;
  if (settings) {
    g.fillText(
      `${spanStart(settings)} · ${PICTURE_WEIGHTING} · ${bandUnit(settings)}`,
      plotX,
      baseline,
    );
    g.textAlign = "right";
    g.fillText(SPAN_END, plotX + plotW, baseline);
  }
}

/** The data canvas onto the visible one, magnified and **unsmoothed** (spec §7.3). */
function blit(): void {
  if (!ctx) return;
  ctx.clearRect(plotX, plotY, plotW, plotH);
  // **No smooth scaling.** This is what makes the gap rule work: smoothing blends a transparent
  // gap column into its lit neighbours and produces a *dim* column, which is exactly the "dead
  // stream reads as a peaceful room" failure the rule exists to prevent. Nearest-neighbour
  // magnification duplicates columns and never blends or drops one; the aggregation above has
  // already guaranteed there is nothing to shrink.
  ctx.imageSmoothingEnabled = false;
  ctx.drawImage(data, 0, 0, buckets, BANDS, plotX, plotY, plotW, plotH);
}

/**
 * Scrolls the picture left by whole columns, leaving **holes** behind.
 *
 * `copy` rather than the default `source-over`, and it is load-bearing: the vacated columns on the
 * right must come out *transparent*, and source-over would leave the previous pixels sitting
 * there — a stopped stream would smear its last column across the picture rather than scrolling
 * holes into it.
 */
function scroll(by: number): void {
  if (!dataCtx || by <= 0) return;
  if (by >= buckets) {
    dataCtx.clearRect(0, 0, buckets, BANDS);
  } else {
    dataCtx.globalCompositeOperation = "copy";
    dataCtx.drawImage(data, -by, 0);
    dataCtx.globalCompositeOperation = "source-over";
  }
  dirty = true;
}

/** Repaints the rightmost column from its running energy mean. */
function paintTail(): void {
  if (!dataCtx || accumulatedCount === 0) return;
  paintColumn(tail.data, 0, 1, (band) =>
    decibels(accumulated[band] / accumulatedCount),
  );
  dataCtx.putImageData(tail, buckets - 1, 0);
  dirty = true;
}

/** Starts a fresh rightmost bucket. */
function openBucket(bucket: number): void {
  rightBucket = bucket;
  accumulated.fill(0);
  accumulatedCount = 0;
}

/**
 * One real column into the picture.
 *
 * A column older than the right edge is dropped rather than drawn: a pull and the ticks around it
 * overlap by design (spec §9.5), so the same slot can arrive twice, and its bucket has already
 * been drawn from at least as much data as this one column carries.
 */
function feed(column: Column): void {
  const bucket = bucketOf(column.slot, slotsPer);
  if (rightBucket === null) {
    openBucket(bucket);
  } else if (bucket > rightBucket) {
    scroll(bucket - rightBucket);
    openBucket(bucket);
  } else if (bucket < rightBucket) {
    return;
  }
  // **In energy, never in dB**: averaging the logs is not the mean of the energy, which is the
  // same distinction spec §6.1 makes for the L_eq.
  for (let band = 0; band < BANDS; band++) {
    accumulated[band] += power(column.bands[band]);
  }
  accumulatedCount += 1;
  paintTail();
}

/**
 * Moves the right edge to the clock, drawing holes for whatever did not arrive.
 *
 * **This is what makes a dead stream visible.** No columns arrive during a silence, so without
 * `now_slot` nothing would say the silence is *current* — the picture would simply stop, which
 * reads as a room that has gone quiet (spec §9.2).
 */
function advance(nowSlot: number): void {
  const bucket = bucketOf(nowSlot, slotsPer);
  if (rightBucket === null) {
    rightBucket = bucket;
  } else if (bucket > rightBucket) {
    scroll(bucket - rightBucket);
    openBucket(bucket);
  }
}

/**
 * Redraws the whole picture from a pull. The **one** place history is drawn wholesale — spec
 * §7.3's rule is about the steady state, and this is what a pull exists for.
 */
function rebuild(columns: Column[]): void {
  if (!dataCtx) return;
  dataCtx.clearRect(0, 0, buckets, BANDS);

  // The right edge is the newer of what the last tick said and what the pull carries: a tick can
  // be in flight while Rust answers, so neither one alone is guaranteed to be the present.
  const newest = columns.length ? columns[columns.length - 1].slot : 0;
  const edge = Math.max(
    bucketOf(lastNowSlot, slotsPer),
    bucketOf(newest, slotsPer),
  );
  openBucket(edge);

  const sums = new Float64Array(buckets * BANDS);
  const counts = new Int32Array(buckets);
  for (const column of columns) {
    const x = buckets - 1 - (edge - bucketOf(column.slot, slotsPer));
    if (x < 0 || x >= buckets) continue;
    const base = x * BANDS;
    for (let band = 0; band < BANDS; band++) {
      sums[base + band] += power(column.bands[band]);
    }
    counts[x] += 1;
  }

  const image = new ImageData(buckets, BANDS);
  for (let x = 0; x < buckets; x++) {
    // **A bucket with no column at all is a hole** — left at alpha 0, background, visibly absent.
    // A bucket with *some* of its slots missing is drawn from the ones that exist: the gap rule is
    // about a column with no data, and averaging in a slot that never arrived would be inventing
    // a quiet that spec §6.9 refuses to publish for the number.
    if (counts[x] === 0) continue;
    const base = x * BANDS;
    const n = counts[x];
    paintColumn(image.data, x, buckets, (band) => decibels(sums[base + band] / n));
  }
  dataCtx.putImageData(image, 0, 0);

  // Carry the rightmost bucket's sums, so a bucket that is still filling keeps averaging rather
  // than restarting from the next column that arrives.
  accumulatedCount = counts[buckets - 1];
  const base = (buckets - 1) * BANDS;
  for (let band = 0; band < BANDS; band++) {
    accumulated[band] = sums[base + band];
  }
  dirty = true;
}

/**
 * Spec §9.5's repair: ask Rust for the picture's history for the current span.
 *
 * Mount, resize and a window-length change; deliberately **not** a calibration change, a Reset or
 * a weighting change — see `pictureNeedsPull` in `src/bridge.ts`, where that table lives.
 */
async function pull(): Promise<void> {
  const token = ++pullToken;
  pulling = true;
  let columns: Column[] = [];
  try {
    columns = await getSpectrogram();
  } catch (error) {
    // Rust owns the history, so a failed pull costs a repair, not the data: the canvas keeps what
    // it has and the next tick keeps appending to it.
    console.error("spectrogram: pull failed", error);
  }
  // Superseded — a newer pull owns the canvas and will replay everything held.
  if (token !== pullToken) return;

  pulling = false;
  rebuild(columns);
  for (const column of held) feed(column);
  held = [];
  advance(lastNowSlot);
  blit();
  dirty = false;
}

function paint(tick: Tick): void {
  lastNowSlot = tick.now_slot;
  if (pulling) {
    // Hold rather than draw: the rebuild is about to overwrite the canvas, and it is working from
    // a snapshot Rust took before these columns existed. Replaying them afterwards is what stops
    // a pull punching a one-slot hole in the present.
    for (const column of tick.columns) held.push(column);
    return;
  }
  for (const column of tick.columns) feed(column);
  advance(tick.now_slot);
  if (dirty) {
    blit();
    dirty = false;
  }
}

watch(
  () => props.tick,
  (tick) => {
    if (tick) paint(tick);
  },
);

/**
 * Only the values the chrome is drawn from, so the watcher below does not fire ten times a second
 * on a settings object the tick rebuilds each time.
 *
 * **`offset_db` left since `b15`.** It was here for the legend's two end-labels, which were the
 * one thing on screen the offset moved; with the legend gone a ±0.1 dB trim changes no pixel of
 * the chrome, and keeping it in the key would redraw and re-blit the whole canvas per tap for
 * nothing. `unit` stays and is what still needs the redraw — it flips `dBFS/band` to `dB/band` the
 * first time the app is calibrated.
 *
 * **The height and the ladder are not in here and must not be** (#13). They change the *geometry*,
 * not just what the chrome says, so they take [`reconfigure`]'s path — re-size, re-measure,
 * redraw, re-pull — of which `drawChrome` is one step. Adding them here would redraw the chrome a
 * second time onto a canvas that was about to be rebuilt anyway.
 */
function chromeKey(settings: Settings | null): string | null {
  return settings ? `${settings.window_s}|${settings.unit}` : null;
}

watch(
  () => chromeKey(props.settings),
  () => {
    const next = props.settings;
    if (!next) return;
    const previous = shown;
    shown = { ...next };

    // The **first** settings are the first time the span is known — the picture was configured for
    // the default until now, and a stored 120 s window would otherwise be drawn at 60 s. After
    // that it is `pictureNeedsPull`'s call, which is spec §9.5's table in one place.
    if (previous === null || pictureNeedsPull(previous, next)) {
      if (configure()) void pull();
      return;
    }
    // The first calibration reaches here and nothing else does: the caption's unit relabels, the
    // picture does not move (spec §9.5).
    drawChrome();
    blit();
  },
);

/**
 * A resize re-pulls (spec §9.5): the pixel budget is a function of the width, so the aggregation
 * has to be redone from the columns rather than from the pixels it already produced.
 *
 * Debounced, because dragging a desktop window edge fires this every frame and each pull is the
 * one bulk payload in the design. The canvas is re-sized and the chrome redrawn immediately so the
 * picture is briefly empty rather than briefly stretched — a stretched canvas is smooth-scaled,
 * which is the one thing spec §7.3 forbids.
 */
function onResize(): void {
  const host = frame.value;
  if (!host) return;
  const width = Math.round(host.clientWidth);
  const ratio = Math.min(3, Math.max(1, window.devicePixelRatio || 1));
  if (width === measuredWidth && ratio === dpr) return;
  reconfigure();
}

/** Re-measure, redraw the chrome and re-pull the history. Shared by the two things that move it. */
function reconfigure(): void {
  if (!configure()) return;
  window.clearTimeout(resizeTimer);
  resizeTimer = window.setTimeout(() => void pull(), 120);
}

/**
 * **A height change needs its own watcher**, because [`onResize`] deliberately ignores it: that
 * path keys on the width and the device ratio, which are the only things the *pixel budget*
 * depends on. The budget is unchanged here and every band row still moves, so the canvas has to be
 * re-sized and the history redrawn from the columns rather than stretched — spec §9.5 lists
 * *canvas resize, orientation change* as a re-pull, and expanding (§7.4) is both.
 *
 * Through the same debounce as a resize, which is what stops expanding — a height change and a
 * width change in one frame — from spending two of the design's one bulk payload.
 */
watch(boxH, () => reconfigure());

onMounted(() => {
  const host = frame.value;
  if (!host) return;
  configure();
  observer = new ResizeObserver(onResize);
  observer.observe(host);
  // No pull here: the span is unknown until the first settings arrive ≤100 ms from now, and the
  // watcher above pulls then. Pulling twice at mount would spend the design's one bulk payload on
  // a picture drawn at the wrong span.
});

onUnmounted(() => {
  observer?.disconnect();
  window.clearTimeout(resizeTimer);
  // Nothing else to tear down: the tick subscription belongs to `App.vue`, and both canvases go
  // with the element.
});
</script>

<template>
  <div
    ref="frame"
    class="picture"
    role="img"
    :aria-label="description"
    :style="{ height: `${boxH}px` }"
  >
    <canvas ref="surface" class="surface"></canvas>
  </div>
</template>

<style scoped>
/* ~205 px tall inline (spec §11.7) and taller when expanded (§7.4), so the height is an inline
   style from the prop rather than a rule here: the canvas is sized in the same pass that picks the
   ladder, and nothing in CSS can tell it what that pass chose. Width is whatever is left. The
   gutters are inside the canvas rather than in CSS, because the labels have to line up with band
   rows the canvas alone knows the size of. */
.picture {
  width: 100%;
}

.surface {
  display: block;
  width: 100%;
  height: 100%;
}
</style>
