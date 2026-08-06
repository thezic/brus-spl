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
//    ┌──────────────────────┐ ┌─┐
// 16k│ ░░  ░▒░   ░░░  ░▒▒░  │ │█│ 71
//  1k│▓███▓████▒▓██▓░ ▒███▓ │ │▓│
// 125│██████████████████████│ │▒│
//  16│██████████████████████│ │░│ 11
//    └──────────────────────┘ └─┘
//     −60s · unweighted  now   dB/band
// ```
//
// **Two canvases, and the split is the whole design.** The *data* canvas is `buckets × 32` — one
// pixel per drawn column, one pixel per band — and is the only thing that is ever appended to or
// scrolled. The *visible* canvas holds the chrome (frame, frequency labels, legend, captions) and
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
  legendEnds,
  PICTURE_WEIGHTING,
  SPAN_END,
  spanStart,
} from "../display";
import {
  BANDS,
  bucketOf,
  decibels,
  paintColumn,
  power,
  rampColour,
  slotsPerBucket,
} from "../spectrogram";

const props = defineProps<{
  /** The last tick. `now_slot` is what makes the right edge honest; `columns` is what is drawn. */
  tick: Tick | null;
  /** The span, the legend's unit and the offset it is labelled with. `null` until the first tick. */
  settings: Settings | null;
}>();

// ─── geometry, in CSS pixels (spec §11.7) ───────────────────────────────────────────────────
//
// **The three gutters are part of the design, not decoration.** 40 px left for the frequency
// labels, 20 px bottom for the time axis and the two captions, and 58 px right for the colour
// legend — 58 and not `07`'s ~52, because `dBFS/band` is four characters longer than `dB/band`
// and at 52 px it overprinted the `now` label. **The legend stays**: it is what gives the picture
// its absolute meaning, and it costs width rather than the vertical calm the layout is built on.

const BOX_H = 205;
const GUTTER_LEFT = 40;
const GUTTER_RIGHT = 58;
const AXIS_H = 20;
const PLOT_H = BOX_H - AXIS_H;

/** Label size, and the mono stack `:root` sets — canvas takes a font string, not a CSS variable. */
const LABEL_PT = 10;
const MONO = 'ui-monospace, SFMono-Regular, Menlo, monospace';

/**
 * The four rows that get a frequency label, top first — the sketch's own ladder.
 *
 * Four, not eight: at ~5.8 px per band a fifth label is nearer its neighbour than a label is tall,
 * and the picture is read for *shape and steadiness* rather than for the level of one row.
 */
const FREQUENCIES: readonly (readonly [number, string])[] = [
  [31, "16k"],
  [19, "1k"],
  [10, "125"],
  [1, "16"],
];

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
  canvas.height = px(BOX_H);
  ctx = canvas.getContext("2d");

  // One device pixel of inset all round, so the frame drawn just outside the plot survives the
  // `clearRect` every blit does inside it.
  plotX = px(GUTTER_LEFT);
  plotY = 1;
  plotW = Math.max(1, px(width - GUTTER_LEFT - GUTTER_RIGHT) - 1);
  plotH = Math.max(BANDS, px(PLOT_H) - 2);

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
  return true;
}

/**
 * The frame, the frequency labels, the legend and the caption line. Everything on the visible
 * canvas that is not the picture.
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
  // (12.5 Hz) is the bottom row, so the row's centre is measured from the top downwards.
  g.textAlign = "right";
  g.fillStyle = palette.dim;
  for (const [band, name] of FREQUENCIES) {
    const centre = plotY + (plotH * (BANDS - 1 - band + 0.5)) / BANDS;
    // The tick sits on the row; the **label** is clamped to stay inside the picture's height. The
    // top row's centre is under 3 px from the edge, and an unclamped baseline there loses the top
    // half of `16k` off the canvas — a label that names the row it is level with is worth half a
    // pixel of parallax at the two ends.
    const text = Math.min(
      Math.max(centre, plotY + px(LABEL_PT / 2)),
      plotY + plotH - px(LABEL_PT / 2),
    );
    g.fillText(name, plotX - px(7), text);
    g.fillRect(plotX - px(4), Math.round(centre), px(4), 1);
  }

  // The legend: the ramp itself, one device-pixel row at a time, top = the loud end. Drawn from
  // the same table the picture is, so a colour on the bar is the colour in the picture.
  const legendX = plotX + plotW + px(9);
  const legendW = px(10);
  for (let row = 0; row < plotH; row++) {
    g.fillStyle = rampColour(Math.round((1 - row / (plotH - 1)) * 255));
    g.fillRect(legendX, plotY + row, legendW, 1);
  }

  const settings = props.settings;
  g.textAlign = "left";
  g.fillStyle = palette.dim;
  if (settings) {
    // **A fixed 60 dB window, shifted by the calibration offset** (spec §7.1) — so calibrating
    // moves these two numbers and no pixel of the picture.
    const [hi, lo] = legendEnds(settings);
    g.fillText(hi, legendX + legendW + px(4), plotY + px(5));
    g.fillText(lo, legendX + legendW + px(4), plotY + plotH - px(5));
  }

  // The caption line. `−60s … now` carries the span a second time (spec §11.7), and
  // **`unweighted` is said out loud** because a reader comparing the picture band-by-band against
  // a dB(A) number will otherwise conclude the app is inconsistent (spec §7.2). It sits on the
  // same line as `dBFS/band` — the two together are the whole of what a colour means.
  const baseline = px(PLOT_H) + px(AXIS_H / 2) + 1;
  g.fillStyle = palette.faint;
  if (settings) {
    g.fillText(
      `${spanStart(settings)} · ${PICTURE_WEIGHTING}`,
      plotX,
      baseline,
    );
    g.textAlign = "right";
    // `now` is pulled a few pixels inside the plot's right edge rather than flush with it: the
    // legend caption is right-aligned to the canvas and `dBFS/band` is four characters longer than
    // `dB/band`, which is what sized the gutter at 58 px in the first place. Flush, the two strings
    // touch in the uncalibrated state — which is the state the app starts life in.
    g.fillText(SPAN_END, plotX + plotW - px(4), baseline);
    g.fillText(bandUnit(settings), canvas.width - px(2), baseline);
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
 */
function chromeKey(settings: Settings | null): string | null {
  return settings
    ? `${settings.window_s}|${settings.unit}|${settings.offset_db}`
    : null;
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
    // A calibration change reaches here and nothing else does: the legend relabels, the picture
    // does not move (spec §9.5).
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
  if (!configure()) return;
  window.clearTimeout(resizeTimer);
  resizeTimer = window.setTimeout(() => void pull(), 120);
}

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
  <div ref="frame" class="picture" role="img" :aria-label="description">
    <canvas ref="surface" class="surface"></canvas>
  </div>
</template>

<style scoped>
/* ~205 px tall, width whatever is left (spec §11.7). The gutters are inside the canvas rather than
   in CSS, because the labels have to line up with band rows the canvas alone knows the size of. */
.picture {
  width: 100%;
  height: 205px;
}

.surface {
  display: block;
  width: 100%;
  height: 100%;
}
</style>
