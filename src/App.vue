<script setup lang="ts">
// The one screen — spec §11, tickets `b06` and `b11`.
//
//         NOW · C · slow        ← both dimensions; they govern the live number and MAX
//            65.5
//             dB
//    LCeq 60s        MAX
//      66.5         67.0
//    60s of 60s                 ← coverage belongs to the L_eq and sits with it
//    [ the spectrogram ]        ← b11: unweighted, span following the window
//              ⋯                ← settings · calibration · reset
//
// *The screen leads with the number that moves and keeps the number that is judged permanently
// in view beside it.*
//
// **The frontend holds no authoritative state.** Every value painted arrives in a tick. The two
// refs below are the last snapshot Rust sent, not a model — nothing here computes, defaults or
// remembers a meter value, and there is deliberately no placeholder before the first tick, which
// arrives ≤100 ms after the listener registers (spec §9.2).
//
// Nothing is gated on `import.meta.env.DEV`: a device build is a *release* build (spec §2.2).

import { computed, onMounted, onUnmounted, ref, shallowRef, watch } from "vue";

import ExpandedChrome from "./components/ExpandedChrome.vue";
import Hero from "./components/Hero.vue";
import InputLine from "./components/InputLine.vue";
import Secondary from "./components/Secondary.vue";
import SettingsSheet from "./components/SettingsSheet.vue";
import Spectrogram from "./components/Spectrogram.vue";
import { onTick, type Meter, type Settings, type Tick } from "./bridge";
import { heroLabel } from "./display";
import type { Geometry } from "./spectrogram";

const meter = ref<Meter | null>(null);
const settings = ref<Settings | null>(null);
const sheetOpen = ref(false);

/**
 * The whole tick, for the picture — which needs `now_slot` and `columns`, not the meter.
 *
 * `shallowRef` because it is handed straight to a canvas: nothing reads a field of it reactively,
 * and making 32 floats × N columns deeply reactive ten times a second would be paying for
 * proxying nobody observes.
 */
const tick = shallowRef<Tick | null>(null);

/**
 * How long a command's own answer outranks the tick.
 *
 * Picker taps take their feedback from the **command return value**, not the next tick (spec
 * §9.2) — but a tick emitted in the moment before the command applied can still be in flight and
 * would arrive carrying the *old* settings, snapping the segment back for one frame. Just over
 * one tick period covers the crossing.
 *
 * This is not the frontend holding state: Rust is authoritative either way, and within 100 ms of
 * the window closing it has said the same thing itself.
 */
const SETTLE_MS = 120;
let appliedAt = 0;

// ─── the expanded view (spec §7.4, #14) ─────────────────────────────────────────────────────
//
// **Two inputs, one derived state.** `landscape` is the device; `portraitExpanded` is the button.
// Rotation writes only the first, so **rotation never changes which state you are in** — turning
// the phone sideways expands, turning it back restores whatever portrait was doing, and neither
// move touches what the other orientation remembers.
//
// **Not persisted.** Nothing goes near `settings.json`: the app launches inline, in portrait,
// whatever it was doing last.

/** What portrait remembers. Landscape has no say in it and never writes it. */
const portraitExpanded = ref(false);

const orientation = window.matchMedia("(orientation: landscape)");
const landscape = ref(orientation.matches);
const expanded = computed(() => landscape.value || portraitExpanded.value);

function onOrientation(event: MediaQueryListEvent) {
  landscape.value = event.matches;
}

/**
 * The plot's rect in CSS px, republished by the picture on every reconfigure (spec §7.5).
 *
 * `null` until the first one lands, which is why both overlaid buttons are gated on it: an
 * unpositioned button in the corner of a picture that has not been measured is worse than none for
 * the one frame it would take to correct itself.
 */
const plot = ref<Geometry | null>(null);

/**
 * The box the picture is given, in CSS px — **measured, not computed from the viewport.**
 *
 * `100dvh` less four safe-area insets is a number only the layout engine knows, and getting it
 * wrong by the home indicator's height puts the `⋯` button under it. So the element is measured and
 * the number handed to the picture, which is the only side that can turn a height into a label
 * ladder and a gutter.
 */
const box = ref<HTMLDivElement>();
const boxHeight = ref(0);
let observer: ResizeObserver | null = null;

/**
 * `undefined` inline, so the picture keeps spec §11.7's 205 px and its own default — the inline
 * geometry is untouched by this ticket.
 */
const pictureHeight = computed(() =>
  expanded.value && boxHeight.value > 0 ? boxHeight.value : undefined,
);

/**
 * **A rect belongs to the height it was measured at.** Handing the picture a new one makes the last
 * rect a lie — for the frame or two before the picture republishes, and by more than a frame's
 * worth of pixels: expanding takes the plot from 183 px to ~820, so chrome drawn against the stale
 * rect lands in the middle of the screen.
 *
 * Keyed on the height rather than on [`expanded`] on purpose. A flip *always* changes the height,
 * so this covers it — but it also covers the two other moments the height moves without the state
 * doing so: the first measurement after mounting straight into landscape, and a rotation between
 * two expanded orientations. And when the height genuinely does not change, nothing is discarded:
 * the old rect is still the true one, and the picture would emit no replacement for it.
 */
watch(pictureHeight, () => {
  plot.value = null;
});

/**
 * The expand button's **offsets** from the plot's top-right corner. Its size and its plate are
 * `.plot-corner`'s, shared with the two the expanded view draws.
 *
 * Positioned from the published rect rather than from the picture's box, for the reason §7.4 gives
 * the expanded chrome: the gutters are inside the canvas and nothing out here may assume their
 * width. Anchored by `right` rather than by `left`, so the button's width stays in CSS and is not
 * arithmetic here as well.
 *
 * It covers the newest few seconds of the top bands — the quiet corner in a real room, and data
 * rather than a label, which is the trade §7.4 already took for close and `⋯`.
 */
const EXPAND_PAD = 6;
const expandStyle = computed(() => {
  const rect = plot.value;
  if (!rect) return undefined;
  return {
    right: `${rect.boxW - (rect.plotX + rect.plotW) + EXPAND_PAD}px`,
    top: `${rect.plotY + EXPAND_PAD}px`,
  };
});

let unlisten: (() => void) | undefined;

function paint(next: Tick) {
  tick.value = next;
  meter.value = next.meter;
  if (performance.now() - appliedAt > SETTLE_MS) settings.value = next.settings;
}

function applied(answer: Settings) {
  settings.value = answer;
  appliedAt = performance.now();
}

onMounted(async () => {
  orientation.addEventListener("change", onOrientation);
  if (box.value) {
    observer = new ResizeObserver(([entry]) => {
      boxHeight.value = Math.round(entry.contentRect.height);
    });
    observer.observe(box.value);
  }
  unlisten = await onTick(paint);
});

onUnmounted(() => {
  orientation.removeEventListener("change", onOrientation);
  observer?.disconnect();
  unlisten?.();
});
</script>

<template>
  <main class="screen" :class="{ full: expanded }">
    <section v-if="!expanded" class="readout">
      <template v-if="meter && settings">
        <Hero :label="heroLabel(settings)" :value="meter.inst" :unit="settings.unit" />
        <InputLine :input="meter.input" />
        <Secondary :meter="meter" :settings="settings" />
      </template>
    </section>

    <!-- The picture, in the box `b06` reserved for it: ~205 px tall with both gutters budgeted —
         ~40 px left for frequency labels and ~20 px bottom for the time axis and the caption line
         (spec §11.7). There is no third: `b15` removed the colour legend and the plot took all but
         2 px of its 58 px. It takes the whole tick rather than the meter, because what it draws is
         `now_slot` and `columns`; the settings it takes are the span and the caption's unit.

         **One instance across both states, deliberately** (spec §7.4): expanding hands it a new
         height and it takes §9.5's re-pull path — re-measure, re-derive the ladder and the gutter,
         rebuild from Rust. A second instance for the expanded view would mount with the settings
         already known and never fire the watcher that pulls, so the picture would fill in from the
         right over a whole span instead of arriving with its history. -->
    <div ref="box" class="picture">
      <Spectrogram
        :tick="tick"
        :settings="settings"
        :height="pictureHeight"
        @geometry="plot = $event"
      />

      <!-- **The inline picture gains this and nothing else** (spec §11.7). -->
      <button
        v-if="!expanded && plot"
        type="button"
        class="plot-corner expand"
        :style="expandStyle"
        aria-label="Expand the picture"
        @click="portraitExpanded = true"
      >
        ⤢
      </button>

      <!-- Gated on the rect alone, **not** on the meter: §7.4 needs `⋯` reachable the moment the
           view is on screen, and an app launched in landscape is expanded from its first frame.
           The hero block inside does the waiting. -->
      <ExpandedChrome
        v-if="expanded && plot"
        :plot="plot"
        :meter="meter"
        :settings="settings"
        :can-close="!landscape"
        @close="portraitExpanded = false"
        @more="sheetOpen = true"
      />
    </div>

    <!-- One affordance on the whole screen (spec §11.9). When expanded it moves into the plot's
         bottom-right corner instead — `⋯` is never gone, because Reset is a during-the-gig
         action. -->
    <button
      v-if="!expanded"
      type="button"
      class="more"
      aria-label="Settings, calibration and reset"
      @click="sheetOpen = true"
    >
      ⋯
    </button>

    <!-- Mounted only while open, so the input buffers and the armed Reset are discarded by
         closing it rather than by being cleared by hand. -->
    <SettingsSheet
      v-if="sheetOpen && meter && settings"
      :meter="meter"
      :settings="settings"
      @applied="applied"
      @close="sheetOpen = false"
    />
  </main>
</template>

<style>
/* Dark and high contrast, and not a free choice: spec §7.1 puts an inferno ramp on a near-black
   field, and light chrome around a dark picture fights it.

   The accent is deliberately cool. The inferno ramp runs black → purple → red → orange → yellow,
   so a warm accent could be read as a level; a cyan cannot. **The accent never means alarm**
   (spec §11.8) — it marks the input-state line and the armed Reset, states and affordances only.
   The test that this holds: at 73.5 dB the screen is identical to 66.7 dB apart from the picture
   being brighter. */
:root {
  --bg: #0a0a0c;
  --surface: #16161b;
  --raised: #121216;
  --line: #26262e;
  --ink: #ececee;
  --ink-dim: #8e8e96;
  --ink-faint: #70707a;
  --accent: #5ac8fa;

  --sans: -apple-system, BlinkMacSystemFont, "Segoe UI", system-ui, sans-serif;
  --mono: ui-monospace, SFMono-Regular, Menlo, monospace;

  font-family: var(--sans);
  font-size: 16px;
  color: var(--ink);
  background: var(--bg);
  color-scheme: dark;
  -webkit-text-size-adjust: 100%;
  -webkit-tap-highlight-color: transparent;
}

body {
  margin: 0;
  background: var(--bg);
}

button {
  font: inherit;
  color: inherit;
  cursor: pointer;
  -webkit-appearance: none;
  appearance: none;
}

input {
  -webkit-appearance: none;
  appearance: none;
}

/* **A button that sits inside the plot's corners** — the expand button on the inline picture, and
   close and `⋯` when expanded (spec §7.4). Global rather than scoped twice, because the three are
   one affordance seen in two places and the numbers below must not drift apart:

   - **44 px is iOS's minimum tap target, and it is not a formality here.** After #15 a missed press
     lands on the picture and places a marker, so a wrong-target tap has a visible consequence.
   - **The plate is what makes the glyph findable over the hot end of inferno.** §7.4's *no
     treatment* does not reach it: that rule is about the hero number and the input-state line,
     which are data. A button is an affordance, and a ✕ nobody can see makes the view unleaveable.

   Each site supplies only its own offsets from the plot's corner — 6 px inline against a 183 px
   plot, 10 px expanded against one three to four times taller. */
.plot-corner {
  position: absolute;
  display: flex;
  align-items: center;
  justify-content: center;
  width: 44px;
  height: 44px;
  font-size: 20px;
  line-height: 1;
  color: var(--ink);
  background: rgba(4, 4, 6, 0.55);
  border: none;
  border-radius: 22px;
}
</style>

<style scoped>
/* Portrait is primary and **width-capped**: in a tall desktop window the stack centres at phone
   width rather than stretching across 1400 px (spec §11.7). */
.screen {
  display: grid;
  grid-template-areas:
    "readout"
    "picture"
    "more";
  align-content: center;
  justify-items: center;
  box-sizing: border-box;
  max-width: 26rem;
  min-height: 100vh;
  min-height: 100dvh;
  margin: 0 auto;
  padding: calc(env(safe-area-inset-top, 0px) + 1rem) 1rem
    calc(env(safe-area-inset-bottom, 0px) + 1rem);
  gap: 1.5rem;
}

.readout {
  grid-area: readout;
  width: 100%;
  /* Held open across the ≤100 ms before the first tick, so nothing shifts when it lands. */
  min-height: 14rem;
}

/* The gutters live inside the canvas now (`b11`), not in this padding: the frequency labels have
   to line up with band rows only the canvas knows the height of. `relative`, because both the
   expand button and the expanded chrome are positioned against the plot rect inside it. */
.picture {
  position: relative;
  grid-area: picture;
  width: 100%;
}

/* ─── expanded: the picture is the screen (spec §7.4) ────────────────────────────────────────
   The grid is gone rather than rearranged — every other area is `v-if`'d away, and what is left is
   one box the picture fills. **The safe areas stay paid for**: in landscape the notch takes a
   side, and the left gutter's frequency labels are exactly what would go under it. */
.screen.full {
  display: block;
  box-sizing: border-box;
  height: 100vh;
  height: 100dvh;
  max-width: none;
  min-height: 0;
  padding: env(safe-area-inset-top, 0px) env(safe-area-inset-right, 0px)
    env(safe-area-inset-bottom, 0px) env(safe-area-inset-left, 0px);
}

.screen.full .picture {
  height: 100%;
}

.more {
  grid-area: more;
  min-width: 4rem;
  min-height: 2.75rem;
  font-size: 1.5rem;
  line-height: 1;
  color: var(--ink-dim);
  background: none;
  border: none;
}

/* **One wide reflow, at `max-height: 700px`, serving both phone-landscape and the 800×600 dev
   window.** The threshold is measurement, not phone geometry: an 800×600 desktop window is a
   *landscape* case — shorter than a phone is tall — so a breakpoint drawn at phone-landscape
   height (560 px) misses it and the portrait stack pushes Reset below the fold (spec §11.7).
   Here the picture goes **beside** the number; arranging anything inside it is `b11`'s job.

   **§7.4 has taken most of this reflow's ground and it is kept rather than deleted.** Landscape is
   now always expanded, so the two cases this was written for — the phone turned sideways and the
   800×600 dev window — never reach it: they are expanded before the query is asked. What is left
   is a window that is short *and* portrait, which is a desk shape rather than a phone one. It
   stays because §11.7 still specifies it and because deleting a layout is a spec change, not a
   build ticket's call — but do not read it as a live phone case. */
@media (max-height: 700px) {
  .screen {
    grid-template-areas:
      "readout picture"
      "more picture";
    /* Wide enough for the hero's own widest state, which is what sets it: `−108.4` is 371 px at
       the capped 7.5rem, and `vw` is the whole window here rather than the column, so the column
       has to be the thing that fits it. */
    grid-template-columns: minmax(23.5rem, 24rem) 1fr;
    grid-template-rows: auto auto;
    align-content: center;
    align-items: center;
    max-width: none;
    column-gap: 2rem;
    row-gap: 0.5rem;
  }

  .readout {
    min-height: 0;
  }

  .picture {
    align-self: center;
  }
}
</style>
