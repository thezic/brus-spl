<script setup lang="ts">
// The marker and its readout — spec §7.5, ticket [#15](https://github.com/thezic/brus-spl/issues/15).
//
// **One marker on the expanded picture, naming the band it sits on and that band's level.** It
// graduates `b13`'s wish to *pinpoint problematic frequencies*, and the expanded view (§7.4) exists
// underneath it because at the inline ~5.7 px per band a fingertip covers seven rows.
//
// ```
//  ┌────────────────────────────────────────┐
//  │                    │                   │
//  │ ───────────────────┼── 3.15 kHz · 74.8 dB/band  ✕
//  │                    │                   │
//  └────────────────────────────────────────┘
// ```
//
// **It is a DOM overlay, and that is load-bearing.** §7.3 spends the picture's whole design on
// *append and scroll, never redraw the history*: the data canvas is the only thing ever scrolled,
// and the visible canvas gets one `drawImage` of it per changed tick. A marker fits on neither. On
// the data canvas it would scroll *with* the history and smear under the `"copy"` composite; on the
// visible canvas the next blit's `clearRect` would wipe it, so keeping it would mean repainting per
// tick something that changes only when a finger moves. HTML rather than a third canvas because the
// readout is *text*: the browser measures it, which is what [`placeChip`] needs before it can decide
// whether the chip fits.
//
// **The whole coupling to the picture is one [`Geometry`] object**, republished by
// `Spectrogram.vue` on every reconfigure. That is what lets the gutter's width stop being a constant
// and what keeps this file ignorant of both canvases.
//
// **The frontend still holds no authoritative state** (§9.5, §8.2). The marker is a `(slot, band)`
// and a level that came from Rust already calibrated; there is no column ring here, no dB
// arithmetic, and no per-band history — which is also why there is no hit assist (§7.5).

import { computed, onUnmounted, ref, watch } from "vue";

import { getSlotLevels, type Unit } from "../bridge";
import { markerFrequency, markerLevel } from "../display";
import {
  bandAt,
  clampToPlot,
  columnX,
  insidePlot,
  placeChip,
  rowCentre,
  slotAt,
  type Marker,
  type MarkerLevel,
} from "../marker";
import type { Geometry } from "../spectrogram";

const props = defineProps<{
  /**
   * The plot rect and pixel budget in CSS px.
   *
   * **Not nullable, and the parent is what makes that true**: it mounts this only once the picture
   * has published a rect, and *unmounts* it again whenever that rect stops being the true one — a
   * height change makes the last rect a lie by hundreds of pixels, so there is no state in which a
   * marker overlay should be on screen holding one. The same gate `ExpandedChrome` is behind.
   */
  geometry: Geometry;
  /** The right edge, from the last tick — the marker's `x` is re-derived from it every tick. */
  nowSlot: number;
  /**
   * The unit a level fetched *now* would be in.
   *
   * Read at the moment the readout is filled in and then **stored with the value**, never again:
   * §7.5's label is static, and calibrating after a marker is placed must not relabel a raw `dBFS`
   * number as `dB`.
   *
   * It has to come from the settings, since `get_slot_levels` answers with bands alone (§9.1) — and
   * it is not a tick behind, which is what would make that a real risk: the calibration commands
   * return the new settings and `App.vue` paints from that answer rather than waiting for the next
   * tick (§9.2). The sheet is also modal over the picture, so no marker can be placed during the
   * one gesture that moves the unit.
   */
  unit: Unit;
}>();

/**
 * The placed marker, **owned by the parent** — spec §7.5's *survives expanding and reopening*.
 *
 * This component is mounted only while the picture is expanded, so state kept here would be
 * discarded by the close button and by every rotation back to portrait. The parent outlives both.
 */
const marker = defineModel<Marker | null>({ required: true });

const root = ref<HTMLDivElement>();
const chip = ref<HTMLDivElement>();
let dragging = false;

/**
 * What the finger has placed, **read back synchronously** — the gesture's own copy of the two
 * numbers in [`marker`].
 *
 * Not a duplicate model, and not optional. `defineModel` with a `v-model` bound above it does *not*
 * write its own ref: the setter only emits, and the value returns as a prop when the parent
 * re-renders. So inside one pointer event `marker.value` is still the previous marker, and a
 * gesture that reads it back to decide anything — did the press place something, which slot did the
 * release land on — decides it a flush late. Every such question is asked of this instead.
 */
let placed: { slot: number; band: number } | null = null;

// ─── where it is drawn now ──────────────────────────────────────────────────────────────────

/** The crosshair's intersection in the picture box, or `null` when there is nothing to draw. */
const point = computed(() => {
  const m = marker.value;
  if (!m) return null;
  const x = columnX(props.geometry, props.nowSlot, m.slot);
  if (x === null) return null;
  return { x, y: rowCentre(props.geometry, m.band) };
});

/**
 * **Vanishes at the left edge** (spec §7.5) — the slot the marker named has scrolled out of the
 * picture, so there is nothing left for it to point at.
 *
 * The state is **dropped, not hidden**: a marker merely hidden would reappear if the span were later
 * widened, pointing at a moment the reader has no way to connect to what they marked.
 *
 * Asked as *is it off the picture* rather than as a watcher on [`point`], which is also `null` when
 * there is simply no marker: the two must not share an answer, because *there is nothing to draw*
 * and *what you marked is gone* are a no-op and a state change.
 *
 * `immediate`, because a marker can be off the picture already when this component mounts. It
 * drifts while the view is inline too — the data moves whether or not anything is drawing it — and
 * nothing is watching there.
 */
const offPicture = computed(() => {
  const m = marker.value;
  if (!m) return false;
  return columnX(props.geometry, props.nowSlot, m.slot) === null;
});

watch(
  offPicture,
  (off) => {
    if (off && !dragging) drop();
  },
  { immediate: true },
);

// ─── the readout ────────────────────────────────────────────────────────────────────────────

const frequency = computed(() =>
  marker.value ? markerFrequency(marker.value.band) : "",
);
const level = computed(() =>
  marker.value ? markerLevel(marker.value.level) : "",
);

/**
 * Whether the readout has a level half at all — **asked of the state, not of the string.**
 *
 * The one state with nothing to say is `pending`, which lasts as long as a drag: the level belongs
 * to where the marker *lands*, so there is nothing honest to show while it is still moving. Keying
 * the separator and the value off an empty string instead would make a display string decide
 * layout, and would hide any future state that happened to format as blank.
 */
const hasLevel = computed(() => marker.value?.level.state !== "pending");

/** The chip's measured size — [`placeChip`] cannot clamp a box it has not been given. */
const chipW = ref(0);
const chipH = ref(0);
let observer: ResizeObserver | null = null;

watch(chip, (element) => {
  observer?.disconnect();
  observer = null;
  if (!element) return;
  observer = new ResizeObserver(() => {
    chipW.value = element.offsetWidth;
    chipH.value = element.offsetHeight;
  });
  observer.observe(element);
});

onUnmounted(() => observer?.disconnect());

const chipBox = computed(() => {
  const p = point.value;
  if (!p || chipW.value === 0) return null;
  return placeChip(props.geometry, p.x, p.y, chipW.value, chipH.value);
});

// ─── the gesture ────────────────────────────────────────────────────────────────────────────
//
// **`drag` places and moves it** (spec §7.5) — the one gesture that shows where the marker will land
// before you commit to it, which is also what removes the need for a hit assist to compensate for a
// fingertip. A tap is the degenerate case of it and needs no separate path; the `✕` is the only
// dismissal, which is why the marker itself takes no press of its own.

/** Only the newest gesture may fill in a readout — a fast re-place starts a second round trip. */
let pending = 0;

/** A pointer event in the picture box's own coordinates — `root` is mounted, or no handler ran. */
function localPoint(event: PointerEvent): { x: number; y: number } {
  const rect = root.value!.getBoundingClientRect();
  return { x: event.clientX - rect.left, y: event.clientY - rect.top };
}

/**
 * Place or move the marker. The point is clamped into the plot, because a drag that slides into a
 * gutter has to keep tracking — lifting a finger to recover would be worse. Whether the *press*
 * was allowed to start at all is [`onDown`]'s question.
 *
 * `initial` marks the press that starts a gesture, and it is what lets a second press on an
 * unchanged `(slot, band)` re-ask for the level: as a move it would be skipped as a no-op, and that
 * skip is the only thing standing between a failed round trip and no way to retry it.
 */
function place(x: number, y: number, initial: boolean): void {
  const g = props.geometry;
  const at = clampToPlot(g, x, y);
  const slot = slotAt(g, props.nowSlot, at.x);
  const band = bandAt(g, at.y);
  // A move within the same cell is not a move: it would re-render the chip at pointer rate, and
  // — since the press is what re-asks for a level — must not be mistaken for a fresh placement.
  if (!initial && placed && placed.slot === slot && placed.band === band) return;

  placed = { slot, band };
  marker.value = { slot, band, level: { state: "pending" } };
}

/**
 * Ask Rust what that slot's bands were — **once, when the marker lands**, never per tick and never
 * per pointer move. §7.5's label is static, so this is a per-gesture round trip.
 *
 * The level comes from here rather than from inverting the colour ramp off the canvas or from a ring
 * on this side: that is what keeps §8.2 (*calibration is applied post-log, in Rust*) exception-free
 * and §9.5 intact.
 */
async function fillLevel(): Promise<void> {
  const asked = placed;
  if (!asked) return;
  const token = ++pending;

  let level: MarkerLevel;
  try {
    const answer = await getSlotLevels(asked.slot);
    if (answer.state === "levels") {
      const value = answer.bands[asked.band];
      // The bridge is hand-written, so a renamed or reordered field arrives as `undefined` rather
      // than as a compile error (`src/bridge.ts`). A readout is a number a reader trusts, so an
      // unusable answer says so instead of printing `NaN dB/band`.
      level = Number.isFinite(value)
        ? { state: "level", db: value, unit: props.unit }
        : { state: "failed" };
    } else {
      level = { state: answer.state };
    }
  } catch (error) {
    // A failed command is not a claim about the room, and must not be shown as one (spec §6.9).
    console.error("marker: get_slot_levels failed", error);
    level = { state: "failed" };
  }

  // Superseded, or the marker has since been moved, dismissed or scrolled off the picture.
  if (token !== pending || placed !== asked) return;
  marker.value = { ...asked, level };
}

function onDown(event: PointerEvent): void {
  const g = props.geometry;
  const { x, y } = localPoint(event);
  // **A press in a gutter places nothing** — the gutters are not the picture, and an existing
  // marker keeps what it had. A drag that later slides into one clamps and keeps going, because
  // lifting a finger to recover would be worse.
  if (!insidePlot(g, x, y)) return;
  place(x, y, true);
  dragging = true;
  // Capture so a drag that leaves the box keeps arriving. Guarded because a synthetic pointer has
  // no active pointer to capture and throws.
  try {
    root.value?.setPointerCapture(event.pointerId);
  } catch {
    /* not a real pointer */
  }
}

function onMove(event: PointerEvent): void {
  if (!dragging) return;
  const { x, y } = localPoint(event);
  place(x, y, false);
}

/** Release commits: the level is what the marker **landed** on, not what it passed over. */
function onUp(): void {
  if (!dragging) return;
  dragging = false;
  void fillLevel();
}

/** The `✕`, and the left edge. Nothing in flight may fill in a readout for a marker that is gone. */
function drop(): void {
  placed = null;
  pending += 1;
  marker.value = null;
}
</script>

<template>
  <div
    ref="root"
    class="overlay"
    @pointerdown="onDown"
    @pointermove="onMove"
    @pointerup="onUp"
    @pointercancel="onUp"
  >
    <template v-if="point">
      <!-- **A full crosshair** (spec §7.5): the row runs the whole plot so it can be sighted along
           to the frequency gutter, which is the reading the marker exists for, and the column does
           the same for the moment. **No halo** — the accent is chosen to survive inferno on its own,
           and a dark outline under every stroke is more ink over the picture than the marker is. -->
      <div
        class="rule row"
        :style="{
          left: `${geometry.plotX}px`,
          width: `${geometry.plotW}px`,
          top: `${point.y}px`,
        }"
      ></div>
      <div
        class="rule column"
        :style="{
          top: `${geometry.plotY}px`,
          height: `${geometry.plotH}px`,
          left: `${point.x}px`,
        }"
      ></div>
      <div class="ring" :style="{ left: `${point.x}px`, top: `${point.y}px` }"></div>

      <!-- Hidden rather than unmounted until it has been measured: [`placeChip`] needs a laid-out
           width, and a chip painted at `0,0` for that one frame lands in the picture's corner. -->
      <div
        ref="chip"
        class="chip"
        :style="{
          left: `${chipBox?.left ?? 0}px`,
          top: `${chipBox?.top ?? 0}px`,
          opacity: chipBox ? 1 : 0,
        }"
        @pointerdown.stop
      >
        <span class="frequency">{{ frequency }}</span>
        <!-- The same separator the caption line 20 px below uses — `−60s · unweighted · dB/band`
             — so two facts on one line read the way they do everywhere else on this screen. -->
        <span v-if="hasLevel" class="separator" aria-hidden="true">·</span>
        <span v-if="hasLevel" class="level">{{ level }}</span>
        <!-- **The only dismissal `drag` has**, and spec §15 records its size as open: 12 px of
             glyph is fine under a mouse and is not obviously a tap target. Left as specified for
             the device check rather than grown on the desk. -->
        <button type="button" class="dismiss" aria-label="Remove the marker" @click="drop">
          ✕
        </button>
      </div>
    </template>
  </div>
</template>

<style scoped>
/* **Covers the whole picture box and takes every pointer event over it** (spec §7.5) — gutters
   included, because a drag that slides into one has to keep being tracked. That is why the corner
   buttons over the picture sit *after* this element in the DOM and stop propagation of their own
   presses: otherwise the readout swallows them.

   `touch-action: none` so a drag across the picture is a drag and not a page scroll, and no text
   selection, so a slow press does not turn the readout blue. */
.overlay {
  position: absolute;
  inset: 0;
  touch-action: none;
  -webkit-user-select: none;
  user-select: none;
}

/* **The accent, `#5ac8fa`** (spec §7.5). §11.8's rule that the accent never means alarm is why it
   is available here: a marker is an affordance, not a level. Red was refused as an alarm colour on
   the one screen designed not to have one, and white loses against inferno's near-white top. */
.rule,
.ring {
  position: absolute;
  pointer-events: none;
}

.rule {
  background: var(--accent);
}

.row {
  height: 1px;
  transform: translateY(-0.5px);
}

.column {
  width: 1px;
  transform: translateX(-0.5px);
}

/* The ring is the prototype's and is kept as it was judged there. **It is the one part of the
   marker that is not bounded by the plot**: on the leftmost column ~7 px of it sits over the
   frequency gutter, and in landscape, where a band is ~12 px, ~2 px of it clears the top frame.
   Left rather than clipped — a clipped ring is a half-moon, which reads as a rendering fault where
   an overhanging one reads as a marker at the edge — and the left-edge case lasts a tick or two
   before §7.5 drops the marker anyway. Seen, not overlooked. */
.ring {
  width: 16px;
  height: 16px;
  margin: -8px 0 0 -8px;
  border: 2px solid var(--accent);
  border-radius: 50%;
}

.chip {
  position: absolute;
  display: flex;
  align-items: center;
  /* Tight, because there is a `·` between the two facts: this is the space either side of it. */
  gap: 0.25rem;
  padding: 0.2rem 0.4rem;
  font-family: var(--mono);
  font-size: 12px;
  font-variant-numeric: tabular-nums;
  line-height: 1.25;
  white-space: nowrap;
  color: var(--ink);
  /* Near-opaque rather than translucent: the chip sits over the hot end of the ramp as often as the
     cool end, and a translucent plate is unreadable over `#fcffa4`. This is the same exception
     `.plot-corner` takes — §7.4's *no plate, no scrim* is about the hero number, which is data;
     a readout nobody can read is not a readout. */
  background: rgba(10, 10, 12, 0.88);
  border: 1px solid var(--accent);
  border-radius: 4px;
}

.separator,
.level {
  color: var(--ink-dim);
}

.dismiss {
  all: unset;
  /* Its own clearance: the tight gap above is for the `·`, and the one target on the chip should
     not inherit it. §15 records the size of this glyph as still open. */
  margin-left: 0.25rem;
  padding: 0 0.15rem;
  color: var(--ink-dim);
  cursor: pointer;
}
</style>
