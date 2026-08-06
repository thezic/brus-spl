<script setup lang="ts">
// One sheet from `⋯`, holding settings *and* calibration *and* Reset — spec §11.9.
//
// Calibration is a rare, deliberate, two-instrument gesture, so folding it in costs nothing and
// keeps the main screen at **one affordance**. Reset lives here too, as a two-step, because §6.8
// makes it discard the window as well as the max hold and the main screen has no place for a
// thumb to land safely.
//
// **Nothing here is a source of truth.** Every command's return value is emitted straight back
// up; the two text fields are input *buffers*, which is a different thing (spec §9.2). The
// mounted lifetime of this component is the whole of the sheet's session, so the drafts and the
// armed Reset are discarded by closing it rather than by being reset by hand.

import { computed, ref } from "vue";

import Picker from "./Picker.vue";
import {
  reset,
  setCalibrationFromReference,
  setCalibrationOffset,
  setTimeWeighting,
  setWeighting,
  setWindowLength,
  type Meter,
  type Settings,
  type TimeWeighting,
  type Weighting,
  type WindowLength,
} from "../bridge";
import { coverage, db } from "../display";

const props = defineProps<{ settings: Settings; meter: Meter }>();
const emit = defineEmits<{ close: []; applied: [settings: Settings] }>();

const WEIGHTINGS: readonly Weighting[] = ["C", "A", "Z"];
const TIME_WEIGHTINGS: readonly TimeWeighting[] = ["F", "S"];
const WINDOWS: readonly WindowLength[] = [10, 30, 60, 120];

/** Spec §8.5: 0.1 dB and no coarser pair — the reference meter's own resolution. */
const TRIM_DB = 0.1;
/** The calibration match runs against a fixed 10 s slice of the ring (spec §8.3). */
const CAL_SLICE_S = 10;

const referenceDraft = ref("");
/** `null` means *not being edited*, so the field tracks the offset a trim just moved. */
const offsetDraft = ref<string | null>(null);
const rejection = ref<string | null>(null);
const armed = ref(false);

const offsetField = computed(
  () =>
    offsetDraft.value ??
    (props.settings.offset_db !== null ? props.settings.offset_db.toFixed(1) : ""),
);

/**
 * What was typed, as a number — accepting **either decimal separator**.
 *
 * Found on the phone and not findable anywhere else: iOS gives an `inputmode="decimal"` keypad
 * the *locale's* separator and no other, so on a comma-locale device there is no `.` key at all
 * and `Number("68,3")` is `NaN`. Both fields then stay permanently disabled with nothing on
 * screen to say why — the silent no-op `apply` below exists to avoid, arrived at from the one
 * direction that skips it. And it does not merely inconvenience the calibration gesture, it
 * **breaks spec §8.5's recovery path**: the offset is recoverable by reading it off a sticky note
 * and retyping it, which is impossible if the keypad cannot type the number the app displays.
 *
 * Two separators are refused rather than guessed at: `1,234` is ambiguous between a thousands
 * group and a decimal, and nothing here needs four digits before the point. Display stays on `.`
 * throughout — this is an instrument, and a hero reading `68.3` over a field reading `68,3` would
 * be a worse inconsistency than the one being fixed.
 */
function typed(text: string): number | null {
  const trimmed = text.trim();
  if (trimmed === "" || (trimmed.match(/[.,]/g)?.length ?? 0) > 1) return null;
  const value = Number(trimmed.replace(",", "."));
  return Number.isFinite(value) ? value : null;
}

const reference = computed(() => typed(referenceDraft.value));
const offset = computed(() => typed(offsetField.value));

/**
 * Every command goes through here, so the sheet has exactly one place that decides what a
 * command's answer does: it is painted (via the parent) and it clears the drafts.
 *
 * A rejection is shown rather than swallowed — `683` for `68.3` and an empty 10 s slice both
 * arrive this way, carrying their own reason, and a Match button that silently did nothing is
 * the worst of the three possible behaviours.
 */
async function apply(command: () => Promise<Settings>) {
  try {
    emit("applied", await command());
    rejection.value = null;
    referenceDraft.value = "";
    offsetDraft.value = null;
  } catch (e) {
    rejection.value = String(e);
  }
}

function match() {
  const value = reference.value;
  if (value !== null) void apply(() => setCalibrationFromReference(value));
}

function setOffset() {
  const value = offset.value;
  if (value !== null) void apply(() => setCalibrationOffset(value));
}

/** Rounded before it is sent: the wire offset is already at 0.1 dB, and `101.4 + 0.1` is not. */
function trim(delta: number) {
  const base = props.settings.offset_db ?? 0;
  void apply(() => setCalibrationOffset(Math.round((base + delta) * 10) / 10));
}

function confirmReset() {
  armed.value = false;
  void apply(reset);
}
</script>

<template>
  <div class="scrim" @click.self="emit('close')">
    <section class="sheet" role="dialog" aria-label="Settings, calibration and reset">
      <header>
        <h1>Settings</h1>
        <button type="button" class="done" @click="emit('close')">Done</button>
      </header>

      <Picker
        label="Weighting"
        :options="WEIGHTINGS"
        :selected="settings.weighting"
        @pick="(w) => apply(() => setWeighting(w))"
      />
      <Picker
        label="Time weighting"
        :options="TIME_WEIGHTINGS"
        :selected="settings.time_weighting"
        @pick="(t) => apply(() => setTimeWeighting(t))"
      />
      <Picker
        label="Window"
        :options="WINDOWS"
        suffix="s"
        :selected="settings.window_s"
        @pick="(s) => apply(() => setWindowLength(s))"
      />

      <hr />

      <!-- Spec §8.4's procedure, in its order: watch the slice fill, type what the proper meter
           reads, then trim. The 10 s slice is fixed and independent of the display's window
           (§8.3), so calibrating never disturbs the 60 s meter behind the sheet. -->
      <div class="row">
        <p class="label">{{ CAL_SLICE_S }}s reading</p>
        <p class="reading">
          <span v-if="meter.cal_leq !== null">{{ db(meter.cal_leq) }} {{ settings.unit }}</span>
          <span v-else class="absent">--</span>
          <span class="coverage">{{ coverage(meter.cal_coverage_s, CAL_SLICE_S) }}</span>
        </p>
      </div>

      <div class="row">
        <p class="label">Reference</p>
        <div class="entry">
          <input
            v-model="referenceDraft"
            type="text"
            inputmode="decimal"
            placeholder="68.3"
            aria-label="What the reference meter reads"
          />
          <button type="button" :disabled="reference === null" @click="match">Match</button>
        </div>
      </div>

      <!-- Directly editable, and not decoration: free provisioning is reinstalled weekly and
           delete-then-install loses the data container, so a visible offset makes recovery a
           sticky note and a retype (spec §8.5). -->
      <div class="row">
        <p class="label">Offset</p>
        <div class="entry">
          <input
            :value="offsetField"
            type="text"
            inputmode="decimal"
            placeholder="unset"
            aria-label="The stored calibration offset, in decibels"
            @input="offsetDraft = ($event.target as HTMLInputElement).value"
          />
          <button type="button" :disabled="offset === null" @click="setOffset">Set</button>
          <!-- Nothing to trim while uncalibrated: the offset is on the order of +100 dB, so a
               ±0.1 dB nudge from nothing is not a smaller version of the gesture (spec §8.5). -->
          <button
            type="button"
            class="trim"
            :disabled="settings.offset_db === null"
            aria-label="Trim the offset down by a tenth of a decibel"
            @click="trim(-TRIM_DB)"
          >
            −{{ TRIM_DB }}
          </button>
          <button
            type="button"
            class="trim"
            :disabled="settings.offset_db === null"
            aria-label="Trim the offset up by a tenth of a decibel"
            @click="trim(TRIM_DB)"
          >
            +{{ TRIM_DB }}
          </button>
        </div>
      </div>

      <p v-if="rejection" class="rejection">{{ rejection }}</p>

      <hr />

      <div class="row">
        <p class="label">
          Reset
          <span class="sub">clears the window and the max hold</span>
        </p>
        <div v-if="!armed" class="entry">
          <button type="button" @click="armed = true">Reset</button>
        </div>
        <div v-else class="entry">
          <button type="button" class="confirm" @click="confirmReset">Confirm</button>
          <button type="button" @click="armed = false">Cancel</button>
        </div>
      </div>
    </section>
  </div>
</template>

<style scoped>
.scrim {
  position: fixed;
  inset: 0;
  z-index: 1;
  display: flex;
  align-items: flex-end;
  justify-content: center;
  background: rgb(0 0 0 / 65%);
}

.sheet {
  box-sizing: border-box;
  width: 100%;
  max-width: 32rem;
  max-height: 92dvh;
  max-height: 92vh;
  overflow-y: auto;
  padding: 1rem 1.15rem calc(env(safe-area-inset-bottom, 0px) + 1.15rem);
  background: var(--raised);
  border-top: 1px solid var(--line);
  border-radius: 1rem 1rem 0 0;
}

header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 0.9rem;
}

h1 {
  margin: 0;
  font-family: var(--mono);
  font-size: 0.85rem;
  font-weight: 400;
  letter-spacing: 0.12em;
  text-transform: uppercase;
  color: var(--ink-dim);
}

hr {
  height: 0;
  margin: 1rem 0;
  border: none;
  border-top: 1px solid var(--line);
}

.row {
  display: flex;
  align-items: center;
  gap: 0.75rem;
  justify-content: space-between;
  min-height: 2.75rem;
}

.row + .row {
  margin-top: 0.4rem;
}

.picker + .picker {
  margin-top: 0.4rem;
}

.label {
  margin: 0;
  font-family: var(--mono);
  font-size: 0.85rem;
  color: var(--ink-dim);
}

.sub {
  display: block;
  margin-top: 0.1rem;
  font-size: 0.72rem;
  color: var(--ink-faint);
}

.reading {
  margin: 0;
  font-family: var(--mono);
  font-size: 1rem;
  font-variant-numeric: tabular-nums;
  text-align: right;
  color: var(--ink);
}

.absent {
  letter-spacing: 0.22em;
  color: var(--ink-faint);
}

.coverage {
  display: block;
  margin-top: 0.1rem;
  font-size: 0.78rem;
  color: var(--ink-dim);
}

.entry {
  display: flex;
  gap: 0.25rem;
  align-items: center;
}

input {
  box-sizing: border-box;
  width: 5.2rem;
  min-height: 2.75rem;
  padding: 0 0.5rem;
  font-family: var(--mono);
  /* 16 px or larger, or iOS zooms the whole page on focus and the meter behind is lost. */
  font-size: 1rem;
  font-variant-numeric: tabular-nums;
  text-align: right;
  color: var(--ink);
  background: var(--surface);
  border: 1px solid var(--line);
  border-radius: 0.5rem;
}

/* `68.3` and `unset` are prompts, not readings. At the field's own weight they read as stored
   values, which on the offset field is the one place that must never be ambiguous (spec §8.5). */
input::placeholder {
  color: var(--ink-faint);
  opacity: 0.7;
}

button {
  min-height: 2.75rem;
  padding: 0 0.7rem;
  font-family: var(--mono);
  font-size: 0.9rem;
  color: var(--ink);
  background: var(--surface);
  border: 1px solid var(--line);
  border-radius: 0.5rem;
}

button:disabled {
  color: var(--ink-faint);
  opacity: 0.55;
}

.trim {
  min-width: 3.1rem;
  padding: 0 0.35rem;
  font-variant-numeric: tabular-nums;
}

.done {
  color: var(--ink-dim);
  background: none;
  border-color: transparent;
}

/* The accent's second and last use: an **armed affordance**, not an alarm. Red here would be
   exactly the alarm colour spec §11.8 rules out, and Reset is not a dangerous act — the window
   refills honestly in a window length. */
.confirm {
  color: var(--bg);
  background: var(--accent);
  border-color: var(--accent);
}

.rejection {
  margin: 0.6rem 0 0;
  font-family: var(--mono);
  font-size: 0.78rem;
  text-align: right;
  color: var(--accent);
  overflow-wrap: anywhere;
}
</style>
