<script setup lang="ts">
// The two numbers that are never behind a tap — spec §11.2.
//
// The L_eq **with its coverage** and the max hold. The no-chrome prototype hid them to test
// whether they were desk curiosity; they are not. The L_eq is the number actually judged against
// the 70 dB ceiling, which is precisely why it cannot be a tap away from a screen whose hero is
// something else.
//
// The coverage figure belongs with the **L_eq** and sits under it, so that spec §9.3's deliberate
// divergence — the number describes this talk, the picture describes the last 60 s of the room —
// reads as two labelled axes rather than an inconsistency.
//
// MAX carries no dimensions of its own: spec §11.3's one header above the hero serves both, so a
// bare `MAX 72.4 dB` never arises.

import type { Meter, Settings } from "../bridge";
import { coverage, db, leqLabel, MAX_LABEL } from "../display";

defineProps<{
  meter: Meter;
  settings: Settings;
}>();
</script>

<template>
  <div class="secondary">
    <div class="cell">
      <p class="label">{{ leqLabel(settings) }}</p>
      <p class="value">
        <span v-if="meter.leq !== null">{{ db(meter.leq) }}</span>
        <span v-else class="absent">--</span>
      </p>
      <p class="coverage">{{ coverage(meter.coverage_s, settings.window_s) }}</p>
    </div>

    <div class="cell">
      <p class="label">{{ MAX_LABEL }}</p>
      <p class="value">
        <span v-if="meter.max !== null">{{ db(meter.max) }}</span>
        <span v-else class="absent">--</span>
      </p>
      <!-- No coverage under MAX: a hold is historical by nature and has no window to be a
           fraction of (spec §6.9). The gap is deliberate, not a missing figure. -->
    </div>
  </div>
</template>

<style scoped>
.secondary {
  display: grid;
  grid-template-columns: 1fr 1fr;
  align-items: start;
  gap: 0 1rem;
  width: 100%;
  margin-top: 1.1rem;
}

.cell {
  text-align: center;
}

.label,
.coverage {
  margin: 0;
  font-family: var(--mono);
  font-size: 0.8rem;
  letter-spacing: 0.06em;
  color: var(--ink-dim);
}

.value {
  display: flex;
  align-items: baseline;
  justify-content: center;
  /* Everything smaller than the hero stays mono (spec §11.8), and every number on screen is
     tabular-figured — at 10 Hz a proportional digit set makes the whole number shuffle sideways
     when one digit changes, which reads as instability that is not in the measurement. */
  height: 2.2rem;
  margin: 0.1rem 0;
  font-family: var(--mono);
  font-size: 1.9rem;
  font-variant-numeric: tabular-nums;
  line-height: 1;
  color: var(--ink);
}

.absent {
  /* The same rule as the hero's, scaled: muted, and much smaller than what it replaces. */
  font-size: 0.95rem;
  letter-spacing: 0.22em;
  color: var(--ink-faint);
}

.coverage {
  margin-top: 0.1rem;
}
</style>
