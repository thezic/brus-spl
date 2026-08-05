<script setup lang="ts">
// Throwaway harness for the capture spike (ticket 02) — delete with src-tauri/src/spike.rs.
// This is deliberately not a meter layout: how the real screen looks is ticket 09's job,
// and guessing at it here would pre-empt that prototype.
//
// It exists because the spike's target is a physical iPhone, where there is no terminal.
// The report has to be readable on the device itself, not only in the Xcode console.
import { ref } from "vue";
import { invoke } from "@tauri-apps/api/core";

const seconds = ref(10);
const running = ref(false);
const output = ref("");
const failed = ref(false);

async function runSpike() {
  running.value = true;
  failed.value = false;
  output.value = `capturing for ${seconds.value}s…`;
  try {
    output.value = await invoke<string>("run_capture_spike", {
      seconds: seconds.value,
    });
  } catch (e) {
    failed.value = true;
    output.value = String(e);
  } finally {
    running.value = false;
  }
}
</script>

<template>
  <main class="container">
    <h1>Capture spike</h1>
    <p class="sub">
      Ticket 02 — proves cpal + AVAudioSession deliver non-zero PCM. Not the meter.
    </p>

    <div class="row">
      <label for="seconds">seconds</label>
      <input id="seconds" v-model.number="seconds" type="number" min="1" max="120" />
    </div>

    <button :disabled="running" @click="runSpike">
      {{ running ? "capturing…" : "Run capture spike" }}
    </button>

    <pre v-if="output" :class="{ failed }">{{ output }}</pre>

    <p class="hint">
      Make a steady sound while it runs. Expect a non-zero RMS well above the idle
      floor. To probe interruptions, take a call or invoke Siri mid-run; to probe
      route changes, plug in headphones.
    </p>
  </main>
</template>

<style>
:root {
  font-family: Inter, Avenir, Helvetica, Arial, sans-serif;
  font-size: 16px;
  line-height: 1.5;
  color: #0f0f0f;
  background-color: #f6f6f6;
  -webkit-font-smoothing: antialiased;
  -webkit-text-size-adjust: 100%;
}

.container {
  margin: 0 auto;
  padding: 2rem 1rem;
  max-width: 40rem;
  display: flex;
  flex-direction: column;
  gap: 1rem;
}

h1 {
  margin: 0;
  font-size: 1.5rem;
}

.sub,
.hint {
  margin: 0;
  font-size: 0.85rem;
  opacity: 0.7;
}

.row {
  display: flex;
  align-items: center;
  gap: 0.5rem;
}

input {
  width: 6rem;
  padding: 0.6em 0.8em;
  font: inherit;
  border: 1px solid #ccc;
  border-radius: 8px;
  background: #fff;
  color: inherit;
}

button {
  /* Sized for a thumb: this gets tapped on a phone, one-handed, at a venue. */
  padding: 0.9em 1.2em;
  font: inherit;
  font-weight: 500;
  cursor: pointer;
  border: 1px solid transparent;
  border-radius: 8px;
  background: #396cd8;
  color: #fff;
}

button:disabled {
  opacity: 0.6;
  cursor: default;
}

pre {
  margin: 0;
  padding: 0.8rem;
  /* The report is fixed-width columns; let it scroll rather than reflow. */
  overflow-x: auto;
  font-size: 0.8rem;
  line-height: 1.45;
  background: #fff;
  border: 1px solid #ddd;
  border-radius: 8px;
  white-space: pre;
}

pre.failed {
  border-color: #d33;
  color: #a00;
}

@media (prefers-color-scheme: dark) {
  :root {
    color: #f6f6f6;
    background-color: #2f2f2f;
  }

  input,
  pre {
    background: #1f1f1f;
    border-color: #444;
  }

  pre.failed {
    border-color: #d33;
    color: #ff8f8f;
  }
}
</style>
