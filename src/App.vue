<script setup lang="ts">
// TEMPORARY READOUT — tickets b01–b05. The real screen is b06, and it replaces this file whole.
//
// This is not a meter layout and is not trying to be one. It exists because `println!` from Rust
// does not reach `xcrun devicectl … --console` (spec §2.2), so the only way to see what the phone
// is doing is to put it on the phone's own screen — and, from b05, to have somewhere to press the
// six commands from.
//
// Everything painted comes from the tick; nothing here is authoritative. The one exception is the
// two text fields, which are input buffers rather than state (spec §9.2).
//
// Nothing here is gated on `import.meta.env.DEV`: a device build is a *release* build.
import { onMounted, onUnmounted, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import {
  onTick,
  reset,
  setCalibrationFromReference,
  setCalibrationOffset,
  setTimeWeighting,
  setWeighting,
  setWindowLength,
  type Settings,
  type Tick,
  type TimeWeighting,
  type Weighting,
  type WindowLength,
} from "./bridge";

// The facts the tick has no place for. A separate temporary command, polled slowly — the tick is
// the 10 Hz path and this is a device diagnostic that changes once a session.
interface SessionFacts {
  sampleRate: number;
  inputChannels: number;
  mode: string;
  measurementMode: boolean;
  ioBufferDuration: number;
  permissionGranted: boolean;
}

interface CaptureFacts {
  device: string;
  sampleFormat: string;
  sampleRate: number;
  channels: number;
  bufferFrames: number | null;
  session: SessionFacts | null;
}

type CaptureState =
  | { state: "starting" }
  | ({ state: "running" } & CaptureFacts)
  | { state: "failed"; reason: string };

interface Diagnostics {
  capture: CaptureState;
  blocks: number;
  dropped: number;
  lastError: string | null;
}

const DIAGNOSTICS_MS = 1000;

const tick = ref<Tick | null>(null);
const diagnostics = ref<Diagnostics | null>(null);
const outcome = ref<string | null>(null);
const failure = ref<string | null>(null);

// Measured rather than assumed: the Done-when asks for ticks *at 10 Hz*, and a rate that has
// drifted is the kind of thing that only shows as a sluggish-feeling meter otherwise.
const ticks = ref(0);
const rate = ref(0);
let counted = 0;
let countedAt = performance.now();

const reference = ref("");
const offset = ref("");

let unlisten: (() => void) | undefined;
let timer: number | undefined;

// Command returns exist for feel, not truth (spec §9.2) — so a tap paints its own result at once
// rather than waiting up to 100 ms. Here that also makes the return value *visible*, which is
// what the Done-when is asking to see.
async function run(what: string, command: () => Promise<Settings>) {
  try {
    const settings = await command();
    outcome.value = `${what} → ${JSON.stringify(settings)}`;
    failure.value = null;
  } catch (e) {
    outcome.value = null;
    failure.value = `${what} → rejected: ${String(e)}`;
  }
}

const weightings: Weighting[] = ["C", "A", "Z"];
const timeWeightings: TimeWeighting[] = ["F", "S"];
const windows: WindowLength[] = [10, 30, 60, 120];

function calibrateFromReference() {
  const value = Number(reference.value);
  void run(`set_calibration_from_reference(${reference.value})`, () =>
    setCalibrationFromReference(value),
  );
}

function calibrateFromOffset() {
  const value = Number(offset.value);
  void run(`set_calibration_offset(${offset.value})`, () => setCalibrationOffset(value));
}

async function pollDiagnostics() {
  try {
    diagnostics.value = await invoke<Diagnostics>("capture_diagnostics");
  } catch (e) {
    failure.value = String(e);
  }
}

onMounted(async () => {
  unlisten = await onTick((payload) => {
    tick.value = payload;
    ticks.value += 1;
    counted += 1;
    const now = performance.now();
    if (now - countedAt >= 2000) {
      rate.value = (counted * 1000) / (now - countedAt);
      counted = 0;
      countedAt = now;
    }
  });
  void pollDiagnostics();
  timer = window.setInterval(() => void pollDiagnostics(), DIAGNOSTICS_MS);
});

onUnmounted(() => {
  unlisten?.();
  if (timer !== undefined) window.clearInterval(timer);
});

function db(value: number | null): string {
  return value != null ? value.toFixed(1) : "--";
}
</script>

<template>
  <main>
    <p class="tag">b01–b05 — temporary tick readout</p>

    <!-- `--` rather than a number whenever the level is undefined: exact-zero blocks from a
         denied microphone must not render as a very quiet room (spec §6.9).

         The unit comes from Rust beside the value rather than being written here, which is spec
         §9.2's footgun-denial extended from values to labels: `dBFS` while uncalibrated is a
         correctly-named different quantity, not a wrong SPL (spec §8.6). -->
    <p class="hero">
      {{ db(tick?.meter.inst ?? null) }}
      <span class="unit">{{ tick?.settings.unit ?? "dBFS" }}</span>
    </p>

    <template v-if="tick">
      <dl>
        <dt>Leq</dt>
        <dd>{{ db(tick.meter.leq) }} {{ tick.settings.unit }}</dd>
        <dt>max</dt>
        <dd>{{ db(tick.meter.max) }} {{ tick.settings.unit }}</dd>
        <dt>coverage</dt>
        <dd>{{ tick.meter.coverage_s.toFixed(1) }}s of {{ tick.settings.window_s }}s</dd>
        <dt>cal slice</dt>
        <dd>
          {{ db(tick.meter.cal_leq) }} {{ tick.settings.unit }} over
          {{ tick.meter.cal_coverage_s.toFixed(1) }}s of 10s
        </dd>
        <dt>input</dt>
        <dd>{{ tick.meter.input }}</dd>
        <dt>now_slot</dt>
        <dd>{{ tick.now_slot }} · {{ tick.columns.length }} columns</dd>
        <dt>ticks</dt>
        <dd>{{ ticks }} at {{ rate.toFixed(2) }} Hz</dd>
        <dt>offset</dt>
        <dd>
          {{
            tick.settings.offset_db != null
              ? `${tick.settings.offset_db.toFixed(1)} dB`
              : "unset — uncalibrated"
          }}
        </dd>
      </dl>

      <!-- The six commands. b06 replaces all of this with the real sheet; here it exists only so
           every command can be pressed on the desk and on the phone. -->
      <div class="controls">
        <div class="row">
          <span class="label">weighting</span>
          <button
            v-for="w in weightings"
            :key="w"
            :class="{ on: tick.settings.weighting === w }"
            @click="run(`set_weighting(${w})`, () => setWeighting(w))"
          >
            {{ w }}
          </button>
        </div>
        <div class="row">
          <span class="label">time wt</span>
          <button
            v-for="t in timeWeightings"
            :key="t"
            :class="{ on: tick.settings.time_weighting === t }"
            @click="run(`set_time_weighting(${t})`, () => setTimeWeighting(t))"
          >
            {{ t }}
          </button>
        </div>
        <div class="row">
          <span class="label">window</span>
          <button
            v-for="s in windows"
            :key="s"
            :class="{ on: tick.settings.window_s === s }"
            @click="run(`set_window_length(${s})`, () => setWindowLength(s))"
          >
            {{ s }}s
          </button>
        </div>
        <div class="row">
          <span class="label">reference</span>
          <input v-model="reference" inputmode="decimal" placeholder="68.3" />
          <button @click="calibrateFromReference">match</button>
        </div>
        <div class="row">
          <span class="label">offset</span>
          <input v-model="offset" inputmode="decimal" placeholder="101.4" />
          <button @click="calibrateFromOffset">set</button>
        </div>
        <div class="row">
          <span class="label">reset</span>
          <button @click="run('reset()', reset)">clear window + max</button>
        </div>
      </div>

      <p v-if="outcome" class="outcome">{{ outcome }}</p>
      <p v-if="failure" class="err">{{ failure }}</p>
    </template>

    <template v-if="diagnostics">
      <p class="state">
        {{ diagnostics.capture.state }}
        <span v-if="diagnostics.capture.state === 'failed'">
          — {{ diagnostics.capture.reason }}
        </span>
      </p>

      <dl>
        <dt>blocks</dt>
        <dd>{{ diagnostics.blocks }}</dd>
        <dt>dropped</dt>
        <dd>{{ diagnostics.dropped }}</dd>

        <template v-if="diagnostics.capture.state === 'running'">
          <dt>rate</dt>
          <dd>{{ diagnostics.capture.sampleRate }} Hz</dd>
          <dt>channels</dt>
          <dd>{{ diagnostics.capture.channels }}</dd>
          <dt>format</dt>
          <dd>{{ diagnostics.capture.sampleFormat }}</dd>
          <dt>buffer</dt>
          <dd>{{ diagnostics.capture.bufferFrames ?? "?" }} frames</dd>
          <dt>device</dt>
          <dd>{{ diagnostics.capture.device }}</dd>

          <template v-if="diagnostics.capture.session">
            <dt>session rate</dt>
            <dd>{{ diagnostics.capture.session.sampleRate }} Hz</dd>
            <dt>session ch</dt>
            <dd>{{ diagnostics.capture.session.inputChannels }}</dd>
            <dt>mode</dt>
            <dd>
              {{ diagnostics.capture.session.mode }}
              <span v-if="!diagnostics.capture.session.measurementMode"> (NOT Measurement)</span>
            </dd>
            <dt>io buffer</dt>
            <dd>{{ (diagnostics.capture.session.ioBufferDuration * 1000).toFixed(2) }} ms</dd>
            <dt>mic</dt>
            <dd>{{ diagnostics.capture.session.permissionGranted ? "granted" : "NOT GRANTED" }}</dd>
          </template>
        </template>
      </dl>

      <p v-if="diagnostics.lastError" class="err">stream error: {{ diagnostics.lastError }}</p>
    </template>
  </main>
</template>

<style>
:root {
  font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
  font-size: 16px;
  color: #eee;
  background: #111;
  -webkit-text-size-adjust: 100%;
}

body {
  margin: 0;
}

main {
  padding: env(safe-area-inset-top, 0) 1rem 1rem;
  padding-top: calc(env(safe-area-inset-top, 0px) + 1rem);
}

.tag {
  margin: 0;
  font-size: 0.75rem;
  opacity: 0.5;
}

.hero {
  /* Big enough to read from across a room, which is the entire point of this screen. */
  margin: 0.5rem 0;
  font-size: 3.5rem;
  font-variant-numeric: tabular-nums;
  line-height: 1;
}

.unit {
  font-size: 1rem;
  opacity: 0.6;
}

.state {
  margin: 0.75rem 0;
  opacity: 0.8;
}

dl {
  display: grid;
  grid-template-columns: max-content 1fr;
  gap: 0.15rem 0.75rem;
  margin: 0;
  font-size: 0.85rem;
}

dt {
  opacity: 0.5;
}

dd {
  margin: 0;
  overflow-wrap: anywhere;
}

.controls {
  margin: 0.75rem 0;
  font-size: 0.85rem;
}

.row {
  display: flex;
  align-items: center;
  gap: 0.35rem;
  margin-bottom: 0.35rem;
}

.label {
  width: 5.5rem;
  flex: none;
  opacity: 0.5;
}

button,
input {
  font: inherit;
  color: inherit;
  background: #1e1e1e;
  border: 1px solid #3a3a3a;
  border-radius: 4px;
  padding: 0.25rem 0.5rem;
}

input {
  width: 5rem;
}

button.on {
  background: #eee;
  color: #111;
}

.outcome {
  margin: 0.5rem 0 0;
  font-size: 0.75rem;
  opacity: 0.7;
  overflow-wrap: anywhere;
}

.err {
  margin-top: 0.75rem;
  color: #ff8f8f;
  overflow-wrap: anywhere;
}
</style>
