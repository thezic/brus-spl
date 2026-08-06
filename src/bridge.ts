// The frontend half of the Rust ↔ Vue contract — spec §9, mirroring `src-tauri/src/bridge.rs`.
//
// **This is the only file in the frontend that imports `@tauri-apps/api`.** Everything that
// crosses the bridge is typed here and nowhere else, so there is one place to read the contract
// and one place a change to it has to land.
//
// Wire names are **snake_case on both sides**: a Rust field name and its name here are literally
// the same string, which is why the interfaces below break the frontend's camelCase habit. That
// includes command arguments — the Rust commands carry `rename_all = "snake_case"` to override
// Tauri's camelCase default.
//
// **The honest cost of hand-written types: a renamed field is a runtime `undefined`, not a
// compile error**, because `vue-tsc` cannot see across the bridge. Accepted deliberately; `ts-rs`
// is the escape if the contract grows. The Rust side pins its own half with a serialization test.
//
// **The frontend holds no authoritative state.** Every value painted comes from a tick; command
// return values exist for *feel*, not truth (spec §9.2) — a picker tap updates from the
// authoritative return rather than waiting up to 100 ms for the next tick.

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export type Weighting = "C" | "A" | "Z";
export type TimeWeighting = "F" | "S";
/** A picker, not a numeric field (spec §6.5). */
export type WindowLength = 10 | 30 | 60 | 120;
/** `dBFS` is a correctly-named different quantity, not an error state (spec §8.6). */
export type Unit = "dB" | "dBFS";
/** Spec §9.4. `unavailable` conflates its causes deliberately; the detail is only in the log. */
export type InputState = "capturing" | "denied" | "unavailable";

/**
 * The four settings **and** the unit, so a value can never be painted under the wrong label.
 *
 * The **one** settings shape on the bridge: this is what every command returns as well as what
 * rides in every tick, so a picker's own feedback and the next tick agree field for field. The
 * offset is rounded to 0.1 dB on both, which is what makes retyping the displayed value stable.
 */
export interface Settings {
  weighting: Weighting;
  time_weighting: TimeWeighting;
  window_s: WindowLength;
  /** `null` while uncalibrated — never a sentinel. */
  offset_db: number | null;
  unit: Unit;
}

/**
 * Every dB value here is **already calibrated and rounded to 0.1 dB** (spec §8.2). Do not add an
 * offset on this side for convenience; there is no raw number to be confused with.
 *
 * `null` is `--`: the instrument reporting that it has nothing to say.
 */
export interface Meter {
  leq: number | null;
  /** NOW, the live time-weighted level — the hero (spec §11.1). */
  inst: number | null;
  max: number | null;
  coverage_s: number;
  cal_leq: number | null;
  cal_coverage_s: number;
  input: InputState;
}

/**
 * One spectrogram column, carrying its **absolute** slot index. Always empty until `b09`; typed
 * from the start so the contract does not change under this file later.
 */
export interface Column {
  slot: number;
  /** 32 bands (spec §7.1). */
  bands: number[];
}

/** Everything the screen paints, once per 100 ms. */
export interface Tick {
  /**
   * The clock-advanced ring index. A slot in `(last_drawn, now_slot]` with no column **is** a
   * gap (spec §7.3) — there is no gap marker and none is needed. Without this the right edge of
   * the picture cannot say that a silence is *current*.
   */
  now_slot: number;
  meter: Meter;
  settings: Settings;
  columns: Column[];
}

/**
 * Subscribes to Rust's 10 Hz publish. The first tick arrives ≤100 ms later, which is why there is
 * deliberately no `get_settings` command.
 *
 * Columns produced between page load and this registering are lost, whatever the transport;
 * `get_spectrogram` (`b10`) is what repairs the picture after a reload.
 */
export function onTick(handler: (tick: Tick) => void): Promise<UnlistenFn> {
  return listen<Tick>("tick", (event) => handler(event.payload));
}

/** C / A / Z. Clears the window, the max hold, the smoother and the filter state (spec §6.11). */
export function setWeighting(weighting: Weighting): Promise<Settings> {
  return invoke<Settings>("set_weighting", { weighting });
}

/** F / S. Clears the max hold and nothing else. */
export function setTimeWeighting(time_weighting: TimeWeighting): Promise<Settings> {
  return invoke<Settings>("set_time_weighting", { time_weighting });
}

/** 10 / 30 / 60 / 120 s. Clears nothing — the ring re-slices, so `60 → 120` reads `60s of 120s`. */
export function setWindowLength(window_s: WindowLength): Promise<Settings> {
  return invoke<Settings>("set_window_length", { window_s });
}

/**
 * Type what the proper meter reads; Rust stores the difference (spec §8.5).
 *
 * Rejects a reference outside 0–140 dB — the `683`-for-`68.3` case — and rejects a match against
 * an empty 10 s slice. Both arrive as a rejected promise carrying the reason.
 */
export function setCalibrationFromReference(reference_db: number): Promise<Settings> {
  return invoke<Settings>("set_calibration_from_reference", { reference_db });
}

/** The offset typed directly, or trimmed by ±0.1 dB. Unclamped on purpose; clears nothing. */
export function setCalibrationOffset(offset_db: number): Promise<Settings> {
  return invoke<Settings>("set_calibration_offset", { offset_db });
}

/**
 * *Start measuring this talk*: clears the window **and** the max hold (spec §6.8), and
 * deliberately **not** the picture (spec §9.3).
 *
 * Returns the unchanged settings only so all six commands have one shape.
 */
export function reset(): Promise<Settings> {
  return invoke<Settings>("reset");
}
