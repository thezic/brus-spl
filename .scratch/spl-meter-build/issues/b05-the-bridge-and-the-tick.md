# The bridge and the 10 Hz tick

Parent: [SPL Meter Build](../map.md)
Type: build
Status: open
Blocked by: [`b03`](b03-the-metrics-pipeline.md), [`b04`](b04-settings-persistence-and-calibration.md)

## Build

`bridge.rs`, `src/bridge.ts`, and the thread that publishes. Spec
[§9](../../spl-meter-mvp/spec.md#9-the-rust--frontend-contract) — the wire contract in §9.1 is
**verbatim** and this ticket implements it as written.

**One event at 10 Hz carrying everything the screen paints, plus commands. The frontend holds
no authoritative state at all.** The tick is ≈460 bytes, ≈4.6 kB/s — about **17× under** the
8192-byte threshold at which Tauri's IPC switches to its bulk path, so it never leaves the fast
path.

```
event "tick"
{
  now_slot: 418752,
  meter: {
    leq: 68.4 | null,
    inst: 71.2 | null,
    max: 74.9 | null,
    coverage_s: 41.3,
    cal_leq: 68.1 | null,
    cal_coverage_s: 10.0,
    input: "capturing" | "denied" | "unavailable"
  },
  settings: { weighting, time_weighting, window_s, offset_db: 101.4 | null, unit },
  columns: []
}
```

Field types: `weighting: "C"|"A"|"Z"`, `time_weighting: "F"|"S"`,
`window_s: 10|30|60|120`, `unit: "dB"|"dBFS"`, all dB values `f64` rounded to 0.1 dB,
`slot: u64`, `bands: [f32; 32]`.

**Ship `columns` as an empty array from the start**, even though nothing fills it until
[Spectrum analysis and the column ring](b09-spectrum-analysis-and-the-column-ring.md), so the
type does not change under the frontend later.

**Six of the seven commands** — all but `get_spectrogram`, which belongs to
[the spectrogram half of the bridge](b10-the-spectrogram-half-of-the-bridge.md). All settings
commands return the new `Settings`:

```
set_weighting(weighting)
set_time_weighting(time_weighting)
set_window_length(window_s)
set_calibration_from_reference(reference_db)   // Result, 0–140 bounds check
set_calibration_offset(offset_db)              // Result
reset()
```

There is deliberately **no `get_settings`** (the first tick arrives ≤100 ms after the listener
registers) and **no clear-calibration command** (uncalibrated is the initial state; a wrong
offset is retyped).

**The tick thread** (§16.6): a dedicated std thread with a sleep-until loop on `Instant`,
emitting via `AppHandle`. It drains the SPSC queue, deposits into the ring, advances the
smoother, compares the max at block rate, re-sums the window, and publishes. **The tick is a
timer, not audio-driven** — it publishes with no audio at all, which is what makes
settings-in-every-tick and the `input` state safe.

**`src/bridge.ts`** owns every crossing type plus thin typed wrappers around `invoke`/`listen`,
and is **the only file in the frontend that imports `@tauri-apps/api`**. Wire names are
**snake_case on both sides**, so a Rust field name and its TS field name are literally the same
string.

**The `input` state** (§9.4): `"capturing" | "denied" | "unavailable"`, read from
`AVAudioApplication`/`AVAudioSession` `recordPermission` — an authoritative answer exists, so
the exact-zeros heuristic is the wrong tool. Without it, a denied microphone renders as `--`
beside `0s of 60s` forever with nothing saying why, **for the one failure the user can actually
fix and only from outside the app**. `"unavailable"` conflates its causes deliberately — stream
build failure, no input device, a never-set category — and the detail lives only in the log.

## Traps

- **Every column carries its absolute slot index, and each tick ships every real column since
  the previous publish.** This is required for **correctness**, not robustness: §6.2's
  clock-advanced ring means a late tick — a timer on a phone — has two or three genuinely
  completed slots behind it, and a one-column payload drops real data on the floor. It is why
  `columns` is an array even while empty.
- **`now_slot` is what makes the picture's right edge honest.** Without it a run of gap slots
  up to the present is invisible: no columns arrive, so nothing says the silence is *current*.
  Same information the coverage figure carries for the number.
- **Every dB value crossing the bridge is already calibrated.** Do not move the offset to the
  frontend for convenience.
- **All four settings *and* the unit ride in every tick**, so the tick is a complete snapshot
  of what the UI paints. This extends §8.2's footgun-denial from values to **labels**: the unit
  travels with the numbers, so a value can never be painted under the wrong one.
- **`Option<f64>` serializes to `null`.** `--` is `null`, never a sentinel like `-999`.
- **No throttling or coalescing in Rust.** wry pushes scripts into `pending_scripts` before load
  and `evaluateJavaScript` after, so a stalled webview **delays** ticks rather than losing them.
- **Events need no capability entry** — `core:default` already includes `core:event:default`,
  so `CLAUDE.md`'s four-step ceremony does not apply anywhere in this design.
- **A binary format would be a pessimisation.** A `Raw(bytes)` payload under 1024 bytes is
  converted to a JSON array of numbers and evaled anyway.
- The honest cost of hand-written types: **a renamed field is a runtime `undefined`, not a
  compile error**, because `vue-tsc` cannot see across the bridge. Accepted deliberately;
  `ts-rs` is the escape if the contract grows.

## Done when

Under `npm run tauri dev` on macOS: ticks arrive at 10 Hz with plausible values, all six
commands return the new `Settings`, `set_calibration_from_reference(683)` is rejected,
`reset()` clears the window and the max hold, and every setting survives an app restart.
`npm run build` typechecks. `cargo clippy` clean.
