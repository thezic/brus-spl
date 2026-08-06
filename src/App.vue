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

import { onMounted, onUnmounted, ref, shallowRef } from "vue";

import Hero from "./components/Hero.vue";
import InputLine from "./components/InputLine.vue";
import Secondary from "./components/Secondary.vue";
import SettingsSheet from "./components/SettingsSheet.vue";
import Spectrogram from "./components/Spectrogram.vue";
import { onTick, type Meter, type Settings, type Tick } from "./bridge";
import { heroLabel } from "./display";

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
  unlisten = await onTick(paint);
});

onUnmounted(() => unlisten?.());
</script>

<template>
  <main class="screen">
    <section class="readout">
      <template v-if="meter && settings">
        <Hero :label="heroLabel(settings)" :value="meter.inst" :unit="settings.unit" />
        <InputLine :input="meter.input" />
        <Secondary :meter="meter" :settings="settings" />
      </template>
    </section>

    <!-- The picture, in the box `b06` reserved for it: ~205 px tall with all three gutters
         budgeted — ~40 px left for frequency labels, ~20 px bottom for the time axis, 58 px
         right for the colour legend (spec §11.7). It takes the whole tick rather than the meter,
         because what it draws is `now_slot` and `columns`; the settings it takes are the span and
         the legend's own label. -->
    <div class="picture">
      <Spectrogram :tick="tick" :settings="settings" />
    </div>

    <!-- One affordance on the whole screen (spec §11.9). -->
    <button
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
   to line up with band rows only the canvas knows the height of. */
.picture {
  grid-area: picture;
  width: 100%;
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
   Here the picture goes **beside** the number; arranging anything inside it is `b11`'s job. */
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
