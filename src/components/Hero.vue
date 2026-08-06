<script setup lang="ts">
// The hero: the live level, its quantity label and its unit — spec §11.1, §11.3, §11.5.
//
// **The hero is NOW, not the L_eq.** A 60 s L_eq moved 0.0 dB over 8 s of steady talk and never
// changed a digit at any display resolution; at this size it is dead screen. The cost is stated
// on the ticket and not hidden here: the number that dominates is not the number judged against
// the 70 dB ceiling — which is why the L_eq sits beside it with its coverage, never behind a tap.

import type { Unit } from "../bridge";
import { db } from "../display";

defineProps<{
  /** `NOW · C · slow` — both dimensions, from [`heroLabel`](../display.ts). */
  label: string;
  /** `null` is `--`: the instrument reporting that it has nothing to say (spec §6.9). */
  value: number | null;
  unit: Unit;
}>();
</script>

<template>
  <div class="hero">
    <p class="quantity">{{ label }}</p>

    <!-- Fixed-height row. `--` after 200 ms of no audio went from rare to common when the hero
         became NOW (spec §11.5), so the state it replaces must not move the rest of the screen
         when it arrives and leaves. -->
    <p class="value">
      <span v-if="value !== null" class="number">{{ db(value) }}</span>
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
  font-size: 0.85rem;
  letter-spacing: 0.09em;
  color: var(--ink-dim);
}

.value {
  display: flex;
  align-items: center;
  justify-content: center;
  height: 7.5rem;
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
     430 px one, and the size that suits a 430 px one overflows the 375. */
  font-size: min(7.5rem, calc(31vw - 12px));
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
  font-size: 2.5rem;
  font-weight: 300;
  line-height: 1;
  letter-spacing: 0.22em;
  color: var(--ink-faint);
}

.unit {
  margin: 0.15rem 0 0;
  font-family: var(--mono);
  font-size: 1rem;
  letter-spacing: 0.14em;
  color: var(--ink-dim);
}
</style>
