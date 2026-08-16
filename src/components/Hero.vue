<script setup lang="ts">
// The hero: the live level, its quantity label and its unit — spec §11.1, §11.3, §11.5.
//
// **The hero is NOW, not the L_eq.** A 60 s L_eq moved 0.0 dB over 8 s of steady talk and never
// changed a digit at any display resolution; at this size it is dead screen. The cost is stated
// on the ticket and not hidden here: the number that dominates is not the number judged against
// the 70 dB ceiling — which is why the L_eq sits beside it with its coverage, never behind a tap.
//
// **This number alone repaints slower than it arrives** — see [`LIVE_REFRESH_MS`] below. It keeps
// its tenth; what it does not keep is ten repaints a second (spec §11.4, `b13` finding 2).

import { ref, watch } from "vue";

import type { Unit } from "../bridge";
import { db } from "../display";

const props = defineProps<{
  /** `NOW · C · slow` — both dimensions, from [`heroLabel`](../display.ts). */
  label: string;
  /** `null` is `--`: the instrument reporting that it has nothing to say (spec §6.9). */
  value: number | null;
  unit: Unit;
}>();

/**
 * How often the hero is allowed to **repaint**, in milliseconds — spec §11.4.
 *
 * `b13`'s venue run found the hero too busy on real speech at `S`, and the busyness is a *rate*:
 * at §6.6's measured 0.351 dB per tick a tenths grid changes a digit on essentially every one of
 * the ten ticks a second. `b14` first answered that by coarsening the number to whole decibels,
 * which was reverted — a tenth is worth reading, and taking it away fixed the symptom by
 * discarding the information. Repainting at 2 Hz calms the same number by a factor of five and
 * costs nothing at all, because what is thrown away is a duplicate glance, not a digit.
 *
 * **This is not spec §6.10's publish rate and must never become it.** The tick stays at 10 Hz:
 * coverage still climbs a second per second, Reset still lands immediately, and block-rate max
 * tracking and the 200 ms threshold still get their drain. §6.10's own words are the licence —
 * *the tick's timing affects only what is painted, never what is measured* — and this is one
 * painted number downstream of an unchanged tick.
 *
 * A single constant on purpose, as `b14`'s step was: this is a judgement about how a number feels
 * while it moves, and 300 ms is one edit away if 500 reads sluggish in the room.
 */
const LIVE_REFRESH_MS = 500;

/** What is on screen, which lags [`props.value`] by at most [`LIVE_REFRESH_MS`]. */
const shown = ref<number | null>(props.value);
let painted = 0;

watch(
  () => props.value,
  (value) => {
    // **`--` is never throttled, in either direction.** Spec §6.9 makes the absent state the
    // instrument reporting itself, and holding a number for 500 ms after the audio stopped is
    // precisely the *frozen, stale with no tell* failure that the 200 ms threshold exists to
    // prevent — a throttle that swallowed it would put a plausible wrong number on the hero for
    // longer than the rule allows. The return from `--` is immediate for the mirrored reason:
    // the instrument has something to say again and should say it.
    if (value === null || shown.value === null) {
      shown.value = value;
      painted = performance.now();
      return;
    }

    // Ticks arrive at 10 Hz, so the first one past the interval carries the repaint and no timer
    // is needed — which also means a dead stream schedules nothing, it just goes `--` above.
    const now = performance.now();
    if (now - painted < LIVE_REFRESH_MS) return;
    shown.value = value;
    painted = now;
  },
);
</script>

<template>
  <div class="hero">
    <p class="quantity">{{ label }}</p>

    <!-- Fixed-height row. `--` after 200 ms of no audio went from rare to common when the hero
         became NOW (spec §11.5), so the state it replaces must not move the rest of the screen
         when it arrives and leaves. -->
    <p class="value">
      <span v-if="shown !== null" class="number">{{ db(shown) }}</span>
      <span v-else class="absent">--</span>
    </p>

    <!-- The unit comes from Rust beside the value rather than being decided here: spec §9.2's
         footgun-denial extended from values to labels. `dBFS` is a correctly-named different
         quantity, not an error state (spec §8.6) — so both units get identical treatment. -->
    <p class="unit">{{ unit }}</p>
  </div>
</template>

<style scoped>
.hero {
  text-align: center;
}

.quantity {
  margin: 0;
  font-family: var(--mono);
  font-size: var(--hero-sub, 0.85rem);
  letter-spacing: 0.09em;
  color: var(--ink-dim);
}

/* **The four `--hero-*` variables exist so the expanded view can reuse this component rather than
   restate it** (#14, spec §7.4). Every fallback is the inline value, so an override is the only
   thing that can change what §11.5 specifies here, and the inline screen sets none. What is being
   reused is not the CSS — it is [`LIVE_REFRESH_MS`] and the `--` rule above, which are behaviour
   and must not exist twice.

   **`--hero-sub` backs both the quantity line and the unit, whose inline sizes differ** — 0.85rem
   and 1rem. That is one knob for two slots and it is deliberate: §11.8 keeps everything smaller
   than the number in mono, and at a resized hero there is one small-text size rather than two.
   The differing fallbacks are what preserves the inline screen exactly; setting the variable
   collapses them, which is what the expanded view wants. Splitting it in two would add a knob
   nothing has a reason to turn independently. */
.value {
  display: flex;
  align-items: center;
  justify-content: center;
  height: var(--hero-line, 7.5rem);
  margin: 0.1rem 0 0;
}

.number {
  /* The UI sans, not monospace (spec §11.8): at this size a mono font gives the decimal point a
     full advance width and `66.8` reads as block-dot-block. The system sans has tabular figures
     too, which is the property that actually matters at 10 Hz. */
  font-family: var(--sans);
  /* **Sized for its widest state, not its most common one** (spec §11.5): `−108.4` is six glyphs
     and the uncalibrated state is the one the app starts life in.
     Measured rather than guessed, because the guess was wrong by a whole rem in the dangerous
     direction — that string is **3.09 em** wide here, so it needs `font-size ≤ (100vw − 32px) /
     3.09` to clear the page's own padding. `31vw − 12px` is that bound with a little slack, and
     7.5rem caps it once the layout's 26rem width cap takes over from the viewport. A flat rem
     value cannot do this: the size that fits a 375 px phone wastes 15 % of the glyph height on a
     430 px one, and the size that suits a 430 px one overflows the 375.
     **`b14` does not touch this**, and did not even while it briefly dropped the tenth: the size
     is a measurement the layout is built on — the wide reflow's column is sized from the same
     371 px — and `b14` is a fix for how often a number *repaints*, not for how much room it
     takes. The tenth being back makes `−108.4` the widest string again, as measured. */
  font-size: var(--hero-size, min(7.5rem, calc(31vw - 12px)));
  font-variant-numeric: tabular-nums;
  font-weight: 300;
  line-height: 1;
  letter-spacing: -0.02em;
  color: var(--ink);
}

.absent {
  /* Muted and **much** smaller than the number it replaces (spec §11.5). At hero size two
     dashes render as a pair of solid filled blocks — a redaction bar, which reads as *withheld*
     rather than as *nothing to say*. The letter-spacing is what keeps them two dashes. */
  font-family: var(--sans);
  font-size: var(--hero-absent, 2.5rem);
  font-weight: 300;
  line-height: 1;
  letter-spacing: 0.22em;
  color: var(--ink-faint);
}

.unit {
  margin: 0.15rem 0 0;
  font-family: var(--mono);
  font-size: var(--hero-sub, 1rem);
  letter-spacing: 0.14em;
  color: var(--ink-dim);
}
</style>
