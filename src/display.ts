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
