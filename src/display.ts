// Every string the screen puts on or beside a number — spec §11.
//
// Here rather than inside the components because three of the four are composed from the
// **settings**, not from the value they label: `NOW · C · slow` and `LCeq 60s` both change when a
// picker is tapped and neither is derivable from the number underneath it. Keeping them in one
// file is what makes spec §11.3's rule — *one header serves the live number and the max hold* —
// a single function rather than a convention two components have to keep.
//
// Nothing here calibrates or corrects: every dB value arrives already calibrated and already
// rounded to 0.1 dB (spec §8.2, §9.1). `toFixed(1)` is a *formatter* here, not a rounder.
//
// The one exception is **display resolution**, which is formatting and so lives here rather than in
// the metrics pipeline: [`liveDb`] shows the live number in coarser steps than the tenths it
// arrives in (spec §11.4). The wire keeps its full precision either way, which is exactly what lets
// the L_eq and the max hold stay at tenths off the same tick payload.

import type { Settings, TimeWeighting } from "./bridge";

/** The sketch in spec §11 writes them out: `NOW · C · slow`, not `NOW · C · S`. */
const WORD: Record<TimeWeighting, string> = { F: "fast", S: "slow" };

/**
 * A number as the screen shows it, with a **typographic minus** rather than a hyphen.
 *
 * Not decoration: `U+2212` is figure-width in the system faces, so it lines up under
 * `font-variant-numeric: tabular-nums` where a hyphen does not — and the uncalibrated state is
 * the one the app starts life in (spec §11.5), where every hero number carries one.
 *
 * Every number on screen goes through here, which is what keeps that one rule in one place.
 */
function fixed(value: number, decimals: number): string {
  return value.toFixed(decimals).replace("-", "−");
}

/** A dB value at the 0.1 dB the wire carries — the L_eq, the max hold and the calibration slice. */
export function db(value: number): string {
  return fixed(value, 1);
}

/**
 * The live number's display step, in decibels — **the live number's alone** (spec §11.4).
 *
 * The escape §11.4 pre-authorised, taken because `b13`'s venue run answered *"does the hero feel
 * right on real speech at S?"* with **no, it is too busy** (`b14`). Not a setting and not a general
 * coarsening: the L_eq is judged against a ceiling and is already perfectly stable at tenths, so
 * spending a tenth there to calm a different number was rejected and stays rejected.
 *
 * **§11.4 carries the arithmetic** for 1 rather than 0.5, off §6.6's measured 0.351 dB per tick.
 * **A single constant on purpose**: this is a judgement about how a number feels while it moves,
 * and 0.5 is one edit away if 1 dB reads blunt in the room.
 */
export const LIVE_STEP_DB = 1;

/** As many decimals as the step is written with — `1` → none, `0.5` → one. */
const LIVE_DECIMALS = (String(LIVE_STEP_DB).split(".")[1] ?? "").length;

/**
 * The **live** level as the screen shows it, at [`LIVE_STEP_DB`].
 *
 * The only place in the app that rounds a value, and it rounds for the eye and not for the
 * measurement — nothing downstream reads this string. `--` and the unit are untouched: they are
 * typographic states (spec §11.5), and coarsening a number says nothing about the states that
 * replace it.
 */
export function liveDb(value: number): string {
  return fixed(Math.round(value / LIVE_STEP_DB) * LIVE_STEP_DB, LIVE_DECIMALS);
}

/** Spec §11.3. Both dimensions, because §6.7 ties MAX's meaning to the same pair. */
export function heroLabel(settings: Settings): string {
  return `NOW · ${settings.weighting} · ${WORD[settings.time_weighting]}`;
}

/** `LCeq 60s` / `LAeq 30s` / `LZeq 120s` — the weighting *and* the window (spec §11.3). */
export function leqLabel(settings: Settings): string {
  return `L${settings.weighting}eq ${settings.window_s}s`;
}

/**
 * `33s of 60s`, and `60s of 60s` when full (spec §6.4). Published **always**, never conditional
 * on being degraded — an indicator that appears only when something is wrong is a warning.
 *
 * Rounded rather than truncated: `coverage_s` sits at 60.0 ± 0.1 on a full window (`b03`), so
 * flooring would flicker between `59s` and `60s` forever on a window that is genuinely full —
 * instability that is not in the measurement, which is the same objection §11.8 makes to
 * proportional digits.
 */
export function coverage(seconds: number, windowS: number): string {
  return `${Math.round(seconds)}s of ${windowS}s`;
}

// ─── the picture's own four strings (spec §7, §11.7) ────────────────────────────────────────

/**
 * The left end of the time axis: `−60s`, and it moves with the window because §7.1 ties the span
 * to it. With `now` at the right edge these are the two labels that carry the span, and they are
 * the reason there is no second time setting to disagree with.
 */
export function spanStart(settings: Settings): string {
  return `−${settings.window_s}s`;
}

/** The right end of the time axis. Spec §7's *time flows right→left with now at the right edge*. */
export const SPAN_END = "now";

/**
 * The picture's unit, on the caption line: `dB/band` calibrated, `dBFS/band` uncalibrated.
 *
 * **`/band` is not decoration.** An energy-summed row's level scales with its bandwidth, so a
 * colour means a level *per one-third-octave band* and nothing else — which is exactly what makes
 * the fixed 32-row layout part of the scale rather than a layout choice (spec §7.1).
 *
 * It was the legend's caption until `b15` removed the legend; it stays because it is now the
 * **only** thing on screen saying what a colour is a quantity *of*, and it sits beside
 * [`PICTURE_WEIGHTING`] where that sentence belongs. `dBFS/band` is four characters longer than
 * `dB/band` and the uncalibrated state is the one the app starts life in, so it is the width the
 * caption line has to be laid out for.
 */
export function bandUnit(settings: Settings): string {
  return `${settings.unit}/band`;
}

/**
 * Said out loud beside the picture, because spec §7.2 requires it to be: **the display is always
 * unweighted, in every meter mode.**
 *
 * A reader who compares the picture band-by-band against a dB(A) reading will conclude the app is
 * inconsistent. It is not — the picture and the number are deliberately different quantities, and
 * dB(A) mode is exactly when you want to see that the rumble is still there, because the number
 * has stopped telling you. This word and `bandUnit`'s caption sit on the same caption line, which
 * is where that comparison is actually made.
 */
export const PICTURE_WEIGHTING = "unweighted";

// **There is no legend, and so no `legendEnds`/`legendLevel` here** (`b15`, spec §11.7). The
// colour bar and its two end-labels are removed on the owner's call — they delivered no value —
// and with them goes the only place the frontend touched `offset_db`. §8.2's *calibration lives in
// Rust* is now true without an exception: nothing on this side does dB arithmetic.
//
// §7.1 is unaffected in substance. The window is still fixed at `−90 … −30 dBFS` with no
// auto-ranging, so the same colour still *is* the same absolute level — what is gone is the only
// thing on screen that **named** those levels.
