<script setup lang="ts" generic="T extends string | number">
// One segmented picker, used for all three settings rows in the sheet (spec §11.9).
//
// **`selected` is a prop, never local state.** The frontend holds nothing authoritative: what is
// highlighted is whatever Rust last said, arriving either in a tick or as a command's return
// value (spec §9.2). There is deliberately no `v-model` here — a model would give the segment an
// opinion of its own between the tap and the answer, which is the second source of truth the
// whole contract is organised against.

defineProps<{
  label: string;
  options: readonly T[];
  selected: T;
  /** `s` for the window lengths, so `60` reads `60s`. */
  suffix?: string;
}>();

defineEmits<{ pick: [value: T] }>();
</script>

<template>
  <div class="picker">
    <p class="label">{{ label }}</p>
    <div class="segments">
      <button
        v-for="option in options"
        :key="option"
        type="button"
        class="segment"
        :class="{ on: option === selected }"
        :aria-pressed="option === selected"
        @click="$emit('pick', option)"
      >
        {{ option }}{{ suffix ?? "" }}
      </button>
    </div>
  </div>
</template>

<style scoped>
.picker {
  display: flex;
  align-items: center;
  gap: 0.75rem;
  justify-content: space-between;
}

.label {
  margin: 0;
  font-family: var(--mono);
  font-size: 0.85rem;
  color: var(--ink-dim);
}

.segments {
  display: flex;
  gap: 0.25rem;
}

.segment {
  min-width: 2.9rem;
  /* 44 px: a thumb at a venue, in the dark. */
  min-height: 2.75rem;
  padding: 0 0.5rem;
  font-family: var(--mono);
  font-size: 0.95rem;
  font-variant-numeric: tabular-nums;
  color: var(--ink-dim);
  background: var(--surface);
  border: 1px solid var(--line);
  border-radius: 0.5rem;
}

/* The selected segment is marked by contrast, not by hue: the accent means *state or
   affordance*, and a setting that is simply on is neither (spec §11.8). */
.segment.on {
  color: var(--bg);
  background: var(--ink);
  border-color: var(--ink);
}
</style>
