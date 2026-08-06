<script setup lang="ts">
// The one place the app speaks up — spec §11.6, §9.4.
//
// **Two parts: a label, and an action only when there is one.** A single string made the two
// states look like the same kind of message, which is the shape that teaches people to ignore
// messages — and this line only ever fires on a permanent condition requiring action *outside*
// the app, never on a measurement. That distinction is what keeps it inside the no-warnings rule.
//
// Styled in the same family as the unit label and the coverage figure, **never as a banner**.

import { computed } from "vue";
import type { InputState } from "../bridge";

const props = defineProps<{ input: InputState }>();

/** Spec §11.6's table, verbatim. `capturing` says nothing at all — the numbers are the message. */
const LINES: Record<InputState, { label: string; action?: string } | null> = {
  capturing: null,
  denied: {
    label: "No microphone access",
    action: "Settings ▸ Privacy & Security ▸ Microphone",
  },
  unavailable: { label: "Microphone unavailable" },
};

const line = computed(() => LINES[props.input]);
</script>

<template>
  <div class="input-line" :class="{ silent: line === null }">
    <template v-if="line">
      <p class="what">{{ line.label }}</p>
      <p v-if="line.action" class="action">{{ line.action }}</p>
    </template>
  </div>
</template>

<style scoped>
.input-line {
  margin-top: 0.7rem;
  text-align: center;
}

/* The row keeps no reserved height: `denied` and `unavailable` are session-long states, not
   things that blink, so there is nothing for a reserved gap to stop from jumping. */
.silent {
  margin-top: 0;
}

.what {
  /* The accent marks **states and affordances, never levels** (spec §11.8) — this line and the
     armed Reset are its only two uses, and the colour is deliberately nowhere near the inferno
     ramp so it can never be misread as a level. */
  margin: 0;
  font-family: var(--mono);
  font-size: 0.85rem;
  letter-spacing: 0.04em;
  color: var(--accent);
}

.action {
  margin: 0.15rem 0 0;
  font-family: var(--mono);
  font-size: 0.78rem;
  letter-spacing: 0.02em;
  color: var(--ink-dim);
}
</style>
