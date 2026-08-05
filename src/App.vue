<script setup lang="ts">
// TEMPORARY READOUT — ticket b01. The real screen is b06, and it replaces this file whole.
//
// This is not a meter layout and is not trying to be one. It exists because `println!` from
// Rust does not reach `xcrun devicectl … --console` (spec §2.2), so the only way to see
// whether the phone is actually capturing — and what rate, channel count and mode it was
// granted — is to put those numbers on the phone's own screen.
//
// Nothing here is gated on `import.meta.env.DEV`: a device build is a *release* build.
import { onMounted, onUnmounted, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";

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

interface Readout {
  capture: CaptureState;
  blocks: number;
  dropped: number;
  dbfs: number | null;
  lastError: string | null;
}

// 10 Hz, matching the tick b05 will replace this polling with.
const POLL_MS = 100;

const readout = ref<Readout | null>(null);
const pollError = ref<string | null>(null);
let timer: number | undefined;

async function poll() {
  try {
    readout.value = await invoke<Readout>("capture_readout");
    pollError.value = null;
  } catch (e) {
    pollError.value = String(e);
  }
}

onMounted(() => {
  void poll();
  timer = window.setInterval(() => void poll(), POLL_MS);
});

onUnmounted(() => {
  if (timer !== undefined) window.clearInterval(timer);
});
</script>

<template>
  <main>
    <p class="tag">b01 — temporary capture readout</p>

    <!-- `--` rather than a number whenever the level is undefined: exact-zero blocks from a
         denied microphone must not render as a very quiet room (spec §6.9). -->
    <p class="hero">
      {{ readout?.dbfs != null ? readout.dbfs.toFixed(1) : "--" }}
      <span class="unit">dBFS</span>
    </p>

    <template v-if="readout">
      <p class="state">
        {{ readout.capture.state }}
        <span v-if="readout.capture.state === 'failed'"> — {{ readout.capture.reason }}</span>
      </p>

      <dl>
        <dt>blocks</dt>
        <dd>{{ readout.blocks }}</dd>
        <dt>dropped</dt>
        <dd>{{ readout.dropped }}</dd>

        <template v-if="readout.capture.state === 'running'">
          <dt>rate</dt>
          <dd>{{ readout.capture.sampleRate }} Hz</dd>
          <dt>channels</dt>
          <dd>{{ readout.capture.channels }}</dd>
          <dt>format</dt>
          <dd>{{ readout.capture.sampleFormat }}</dd>
          <dt>buffer</dt>
          <dd>{{ readout.capture.bufferFrames ?? "?" }} frames</dd>
          <dt>device</dt>
          <dd>{{ readout.capture.device }}</dd>

          <template v-if="readout.capture.session">
            <dt>session rate</dt>
            <dd>{{ readout.capture.session.sampleRate }} Hz</dd>
            <dt>session ch</dt>
            <dd>{{ readout.capture.session.inputChannels }}</dd>
            <dt>mode</dt>
            <dd>
              {{ readout.capture.session.mode }}
              <span v-if="!readout.capture.session.measurementMode"> (NOT Measurement)</span>
            </dd>
            <dt>io buffer</dt>
            <dd>{{ (readout.capture.session.ioBufferDuration * 1000).toFixed(2) }} ms</dd>
            <dt>mic</dt>
            <dd>{{ readout.capture.session.permissionGranted ? "granted" : "NOT GRANTED" }}</dd>
          </template>
        </template>
      </dl>

      <p v-if="readout.lastError" class="err">stream error: {{ readout.lastError }}</p>
    </template>

    <p v-if="pollError" class="err">{{ pollError }}</p>
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
  margin: 0 0 0.75rem;
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

.err {
  margin-top: 0.75rem;
  color: #ff8f8f;
  overflow-wrap: anywhere;
}
</style>
