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

/**
 * The ring's slot period, in milliseconds — spec §6.2, mirroring `SLOT_MS` in
 * `src-tauri/src/metrics.rs`.
 *
 * Part of the contract rather than a display constant: `now_slot` and every `Column.slot` are
 * indices in these units, so a span in seconds is only a number of slots through this.
 */
export const SLOT_MS = 100;

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
 * One spectrogram column, carrying its **absolute** slot index.
 *
 * The band values are the one thing crossing this bridge that is **not** calibrated, and that is
 * spec §7.1's arithmetic rather than an oversight: the colour window shifts *by the offset* and
 * the values shift with it, so every colour is unchanged and only the legend relabels.
 */
export interface Column {
  slot: number;
  /** 32 bands, raw dB re FS, low to high (spec §7.1). */
  bands: number[];
}

/**
 * What one slot's 32 bands are, or **which kind of nothing** there is — spec §7.5's readout.
 *
 * Three states rather than `number[] | null`, because Rust answers `None` both for a slot that
 * aged out of the 1200-slot ring and for a slot that was a gap, and *no data kept* and *silence*
 * are different claims: §6.9's refusal to publish a fake quiet applies to a readout exactly as it
 * applies to the hero number. A marker on `outside` has lost the data it named; one on `gap` is
 * still pointing at a real, empty moment.
 *
 * Unlike `Column.bands`, these are **already calibrated and rounded to 0.1 dB** — the two disagree
 * deliberately (see `Column`). Do not add the offset on this side.
 */
export type SlotLevels =
  /** 32 calibrated per-band levels, low row first — the same order as `Column.bands`. */
  | { state: "levels"; bands: number[] }
  /** In the ring, and empty: spec §7.3's hole, drawn as nothing in the picture too. */
  | { state: "gap" }
  /**
   * Outside the 120 s ring — aged out, or ahead of the present. **Not the marker's vanish
   * trigger**: §7.5's left edge is the *display span*, 10 to 120 s, so the two coincide only at
   * 120 s and at a 10 s span a slot 30 s old is off the picture and still answers `levels`.
   * A marker leaving the plot is geometry on this side; this is the ring running out.
   */
  | { state: "outside" };

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
  /**
   * **Every real column since the previous tick**, oldest first — not one. Required for
   * correctness rather than robustness: a timer on a phone routinely has two or three genuinely
   * completed slots behind it, and a one-column payload would drop real data on the floor.
   */
  columns: Column[];
}

/**
 * Subscribes to Rust's 10 Hz publish. The first tick arrives ≤100 ms later, which is why there is
 * deliberately no `get_settings` command.
 *
 * Columns produced between page load and this registering are lost, whatever the transport;
 * `getSpectrogram` is what repairs the picture after a reload.
 */
export function onTick(handler: (tick: Tick) => void): Promise<UnlistenFn> {
  return listen<Tick>("tick", (event) => handler(event.payload));
}

/**
 * The picture's history for the **current span** — the seventh command, and the only bulk payload
 * in the design.
 *
 * ≈138 KB at a 60 s span and ≈276 KB at 120 s, so it travels the `ipc://localhost` custom protocol
 * rather than by `eval` — the right way round for something sent once per redraw-from-scratch.
 *
 * **Rust owns the history**, so every event that invalidates the canvas is one move — *pull again*
 * — rather than four different repairs. Letting this side keep its own ring was rejected on the
 * dev loop: the picture would be empty for up to two minutes after every reload.
 *
 * **What makes the frontend re-pull**, verbatim from spec §9.5:
 *
 * | event | re-pull? |
 * |---|---|
 * | mount / webview reload | ✓ |
 * | canvas resize, orientation change | ✓ |
 * | window length change (10/30/60/120 s) | ✓ |
 * | calibration offset change | — |
 * | Reset button | — |
 * | weighting change (C/A/Z) | — |
 *
 * The last three are not omissions and each has its own reason: the offset row is arithmetic
 * (§7.1 shifts the colour window *by the offset*, so no pixel moves), Reset deliberately does not
 * clear the picture (§9.3), and the picture is unweighted in every meter mode (§7.2).
 *
 * The right edge is the last tick's `now_slot`, so a pull and the tick around it overlap: a column
 * can arrive both ways, and drawing the same slot twice draws the same thing twice.
 */
export function getSpectrogram(): Promise<Column[]> {
  return invoke<Column[]>("get_spectrogram");
}

/**
 * The 32 band levels behind one slot — the eighth command, for spec §7.5's marker readout.
 *
 * **Called once, when the marker is placed.** The label is static after that: frequency is fixed by
 * the row and level by the data point, so this is a per-gesture round trip and never a per-tick
 * one. A few milliseconds against a picture the pull already lets be up to 100 ms stale.
 *
 * The level comes from here rather than from inverting the colour ramp off the canvas or from a
 * ring on this side, which is what keeps §8.2 (*calibration is applied post-log, in Rust*)
 * exception-free and §9.5 intact — the frontend still holds no history of its own.
 *
 * **It names a slot, not a bucket.** Where the pixel budget puts more than one slot in a column the
 * two differ; the marker pins to the marked bucket's middle slot (§7.5), and that choice lives with
 * the caller because the pixel budget is the frontend's fact, not Rust's.
 *
 * The ring is 120 s at every span, so at every span but 120 s this happily answers for slots the
 * picture stopped showing a while ago. **A marker vanishing at the left edge is this side's own
 * geometry**, not an `outside` coming back — the two coincide only at a 120 s span.
 */
export function getSlotLevels(slot: number): Promise<SlotLevels> {
  return invoke<SlotLevels>("get_slot_levels", { slot });
}

/**
 * Whether a settings change invalidates the canvas — spec §9.5's three settings rows, in one place
 * so the two that do nothing cannot be re-litigated at the call site.
 *
 * **Only the window length.** It moves the span, so the whole time axis is different. A weighting
 * change and a calibration change both leave every pixel of the picture exactly where it was; the
 * second of those looks like an oversight and is not — see `getSpectrogram`.
 */
export function pictureNeedsPull(previous: Settings | null, next: Settings): boolean {
  return previous !== null && previous.window_s !== next.window_s;
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
