<script setup lang="ts">
// Everything the expanded view puts **over** the picture — spec §7.4, ticket #14.
//
// ```
//  ┌──────────────────────────────────────┐
//  │ NOW · C · slow                    ✕  │  ← the hero block top-left, close top-right
//  │ 66.5                                 │
//  │ dB                                   │
//  │ LCeq 60s   MAX                       │
//  │ 66.5       67.0                      │
//  │ No microphone access                 │  ← §6.9's line, and it stays
//  │                                      │
//  │                                   ⋯  │  ← Reset is a during-the-gig action
//  └──────────────────────────────────────┘
// ```
//
// **The picture itself is not here.** `App.vue` keeps one `Spectrogram` across both states — a
// second instance would mount with the settings already known, and its own re-pull watcher only
// fires on a *change*, so the expanded picture would fill in from the right over a whole span
// instead of arriving with its history. This component is the chrome that sits over that one
// picture, positioned against the plot rect the picture publishes.
//
// **Chrome sits inside the plot's corners, not the screen's** (spec §7.4). The alternative put the
// top corner over the topmost frequency label and the bottom one over the `now` caption; this
// covers **data** instead, which is recoverable by scrolling and a label is not.
//
// **Nothing here takes a touch except the two buttons.** The overlay and the hero block are
// `pointer-events: none`, so a tap lands on the picture even through the number — the marker
// (#15) is what claims those taps, and a number that swallowed them would claim the picture twice.

import { computed } from "vue";

import Hero from "./Hero.vue";
import InputLine from "./InputLine.vue";
import type { Meter, Settings } from "../bridge";
import { db, heroLabel, leqLabel, MAX_LABEL } from "../display";
import type { Geometry } from "../spectrogram";

const props = defineProps<{
  /** The plot rect in CSS px, as the picture last published it (spec §7.5). */
  plot: Geometry;
  /**
   * `null` until the first tick, ≤100 ms after the listener registers (spec §9.2) — and for as
   * long as the tick stream never starts at all.
   *
   * **The two buttons do not wait for it.** They are drawn from the plot rect alone, because §7.4
   * requires `⋯` to stay reachable when expanded and an app launched in landscape is expanded from
   * its first frame: gating the whole chrome on a meter would leave that launch with no `⋯` and no
   * way out, permanently if capture never starts. Only the hero block has anything to say about a
   * measurement, so only the hero block waits.
   */
  meter: Meter | null;
  settings: Settings | null;
  /**
   * Whether leaving is possible at all.
   *
   * **False in landscape, and then there is no close button** — spec §7.4 makes landscape always
   * expanded, so a ✕ there is a button that cannot do its job. Rendering it anyway would be either
   * inert or a control that changes what *portrait* does with nothing on screen to show it.
   */
  canClose: boolean;
}>();

const emit = defineEmits<{ close: []; more: [] }>();

/**
 * The overlay's box: **the plot's rect, not the picture's box and not the screen's.**
 *
 * The gutter is inside the canvas and its width moves with the label ladder, so nothing out here
 * may assume 40 px — spec §7.4. This is the same seam #15's marker overlay is built on.
 */
const rect = computed(() => ({
  left: `${props.plot.plotX}px`,
  top: `${props.plot.plotY}px`,
  width: `${props.plot.plotW}px`,
  height: `${props.plot.plotH}px`,
}));
</script>

<template>
  <div class="chrome" :style="rect">
    <!-- **Top-left is the quiet corner**: 8–16 kHz are the coldest rows in any real room, where the
         bottom rows are `b13`'s rumble stripe. It is a mitigation and not a defence — a
         cymbal-heavy PA lights the top too — and spec §7.4 takes that knowingly: **no outline, no
         plate, no scrim**, because a scrim dims a band of the picture whether the number needs it
         or not and §7.1's *the same colour is the same absolute level* is worth more. §17 records
         the cost. Do not "fix" this here. -->
    <div v-if="meter && settings" class="block">
      <!-- The real `Hero.vue`, resized through its four `--hero-*` variables: what is reused is
           `LIVE_REFRESH_MS` and §11.5's never-throttle-`--` rule, which are behaviour. -->
      <Hero :label="heroLabel(settings)" :value="meter.inst" :unit="settings.unit" />

      <!-- **`LCeq` and `MAX` below the number at ×0.25 of it, and coverage goes** (spec §7.4; §17
           records the coverage figure as a knowing exception to §11.2). Restated here rather than
           reusing `Secondary.vue`: that component is a full-width two-column grid *with* coverage,
           and it carries no behaviour to share — only the `--` typography, which is the three lines
           below. The strings still come from `display.ts`, so §11.3's one-header rule holds. -->
      <div class="secondary">
        <div class="cell">
          <p class="label">{{ leqLabel(settings) }}</p>
          <p class="value">
            <span v-if="meter.leq !== null">{{ db(meter.leq) }}</span>
            <span v-else class="absent">--</span>
          </p>
        </div>
        <div class="cell">
          <p class="label">{{ MAX_LABEL }}</p>
          <p class="value">
            <span v-if="meter.max !== null">{{ db(meter.max) }}</span>
            <span v-else class="absent">--</span>
          </p>
        </div>
      </div>

      <!-- §6.9's line, and it **stays when expanded**: a dead stream must never read as a peaceful
           room, and the picture alone cannot say that the microphone was refused. -->
      <InputLine :input="meter.input" />
    </div>

    <button
      v-if="canClose"
      type="button"
      class="plot-corner close"
      aria-label="Close the expanded picture"
      @click="emit('close')"
    >
      ✕
    </button>

    <!-- **`⋯` stays reachable when expanded** (spec §7.4): Reset is a during-the-gig action and
         needing a rotation to reach it fails at the worst moment. Bottom-right is thumb-reachable
         in landscape, which is the orientation that cannot leave this view. -->
    <button
      type="button"
      class="plot-corner more"
      aria-label="Settings, calibration and reset"
      @click="emit('more')"
    >
      ⋯
    </button>
  </div>
</template>

<style scoped>
.chrome {
  /** How far the block and the buttons sit inside the plot's corners. */
  --pad: 10px;

  position: absolute;
  pointer-events: none;
}

/* ─── the hero block ──────────────────────────────────────────────────────────────────────── */

.block {
  /* **One number, and everything else scales off it** — spec §7.4 fixes the hero at 56 px and the
     secondary row at ×0.25 of it, so both figures are visible here rather than pre-multiplied.
     56 px is a quarter of what the number gets inline, and that is the trade of this view: the
     picture is the subject. The measured footprint is ~140 × 133 px — 7.6 % of a landscape plot,
     though 38 % of its height, which is what #15's marker has to negotiate around.

     `--hero-sub` and the secondary's label both sit at 10 px, the same size as the picture's own
     frequency labels — the smallest text this screen already asks to be read at arm's length. */
  --hero-size: 56px;
  --hero-line: calc(var(--hero-size) * 1.05);
  --hero-absent: calc(var(--hero-size) / 3);
  --hero-sub: 10px;
  --secondary-size: calc(var(--hero-size) * 0.25);
  --secondary-label: 10px;

  position: absolute;
  top: var(--pad);
  left: var(--pad);
  /* **The block may never be wider than the plot it is anchored inside.** Its width is set by its
     widest line, and on `denied` that is §11.6's action line — `Settings ▸ Privacy & Security ▸
     Microphone`, ~325 px, which overruns the plot on a 375 px phone and paints over the picture's
     own right edge. Capped here rather than shortened there: the line is one instruction and
     wrapping it costs a row of picture nobody was reading. */
  max-width: calc(100% - var(--pad) * 2);
  text-align: left;
}

/* Both children centre themselves in their own column inline; a corner-anchored block reads with
   the edge it is anchored to, so alignment comes from the anchor rather than from the component. */
.block :deep(.hero),
.block :deep(.input-line) {
  text-align: inherit;
}

/* **The `--` state is why this is needed and the number is not.** The hero's value row is a
   centring flex box, which is invisible while the number is the widest thing in the block and
   obvious the moment it is replaced by two dashes: §11.5 makes `--` deliberately much smaller, so
   it drifts into the middle of a block every other line of which is flush left. Anchoring the row
   is what keeps the absent state where the number was. */
.block :deep(.hero > .value) {
  justify-content: flex-start;
}

.secondary {
  display: flex;
  gap: 1.2em;
  margin-top: 0.2em;
  /* The row's own size, so the gap between the two cells scales with the numbers rather than with
     the root font. */
  font-size: var(--secondary-size);
}

.label {
  margin: 0;
  font-family: var(--mono);
  font-size: var(--secondary-label);
  /* `LCeq 60s` is one label and must never break across two lines — it carries the weighting *and*
     the window (§11.3), and half of it is a different statement. */
  white-space: nowrap;
  letter-spacing: 0.06em;
  color: var(--ink-dim);
}

.value {
  margin: 0.1em 0 0;
  font-family: var(--mono);
  font-size: 1em;
  font-variant-numeric: tabular-nums;
  line-height: 1;
  color: var(--ink);
}

.absent {
  /* The same rule as the hero's, scaled: muted, and much smaller than what it replaces (§11.5). */
  font-size: 0.5em;
  letter-spacing: 0.22em;
  color: var(--ink-faint);
}

/* ─── close and `⋯` ───────────────────────────────────────────────────────────────────────────
   Their size and plate are `.plot-corner`'s, in `App.vue`'s global block — one rule shared with the
   inline picture's expand button, so the three cannot drift. Here: the offsets, and the one thing
   that is only true of these two. */

/* **The overlay takes no touches and these two do.** Everything else over the plot is transparent
   to a finger, so a tap lands on the picture even through the number — the marker (#15) is what
   claims those taps, and a hero that swallowed them would claim the picture twice. */
.plot-corner {
  pointer-events: auto;
}

.close {
  top: var(--pad);
  right: var(--pad);
}

.more {
  right: var(--pad);
  bottom: var(--pad);
}
</style>
