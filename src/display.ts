// Every string the screen puts on or beside a number — spec §11.
//
// Here rather than inside the components because three of the four are composed from the
// **settings**, not from the value they label: `NOW · C · slow` and `LCeq 60s` both change when a
// picker is tapped and neither is derivable from the number underneath it. Keeping them in one
// file is what makes spec §11.3's rule — *one header serves the live number and the max hold* —
// a single function rather than a convention two components have to keep.
//
// Nothing here rounds, clamps or corrects: every dB value arrives already calibrated and already
// rounded to 0.1 dB (spec §8.2, §9.1). `toFixed(1)` is a *formatter* here, not a rounder.

import type { Settings, TimeWeighting } from "./bridge";
import { WINDOW_HI_DBFS, WINDOW_LO_DBFS } from "./spectrogram";

/** The sketch in spec §11 writes them out: `NOW · C · slow`, not `NOW · C · S`. */
const WORD: Record<TimeWeighting, string> = { F: "fast", S: "slow" };

/**
 * A number as the screen shows it, with a **typographic minus** rather than a hyphen.
 *
 * Not decoration: `U+2212` is figure-width in the system faces, so it lines up under
 * `font-variant-numeric: tabular-nums` where a hyphen does not — and the uncalibrated state is
 * the one the app starts life in (spec §11.5), where every hero number carries one.
 */
export function db(value: number): string {
  return value.toFixed(1).replace("-", "−");
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
 * The legend's caption: `dB/band` calibrated, `dBFS/band` uncalibrated.
 *
 * **`/band` is not decoration.** An energy-summed row's level scales with its bandwidth, so a
 * colour means a level *per one-third-octave band* and nothing else — which is exactly what makes
 * the fixed 32-row layout part of the scale rather than a layout choice (spec §7.1).
 *
 * This is the string that sized the 58 px legend gutter: `dBFS/band` is four characters longer
 * than `dB/band`, and the uncalibrated state is the one the app starts life in.
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

/**
 * A legend end-label: the fixed dBFS colour window, **shifted by the calibration offset**.
 *
 * The one place in the frontend that touches the offset, and it is spec §7.1's arithmetic rather
 * than a hole in §8.2's *calibration lives in Rust*: band values cross the bridge raw, the colour
 * window is fixed in dBFS, and the offset moves the window and the values together — so every
 * colour is unchanged by calibrating and **only the legend relabels**. Doing it in Rust instead
 * would mean sending two numbers that are a constant apart from the offset already in the tick.
 *
 * Whole decibels: this labels a colour ramp, not a reading. The instrument's own numbers are
 * elsewhere on the screen at 0.1 dB.
 */
export function legendLevel(dbfs: number, settings: Settings): string {
  const value = dbfs + (settings.offset_db ?? 0);
  return Math.round(value)
    .toString()
    .replace("-", "−");
}

/** The two ends of the legend, top first — [`WINDOW_HI_DBFS`] and [`WINDOW_LO_DBFS`] relabelled. */
export function legendEnds(settings: Settings): [string, string] {
  return [
    legendLevel(WINDOW_HI_DBFS, settings),
    legendLevel(WINDOW_LO_DBFS, settings),
  ];
}
