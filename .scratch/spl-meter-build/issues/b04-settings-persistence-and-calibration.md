# Settings, persistence and calibration arithmetic

Parent: [SPL Meter Build](../map.md)
Type: build
Status: open
Blocked by: [`b03`](b03-the-metrics-pipeline.md)

## Build

`settings.rs`: the four settings, their JSON file, and the calibration arithmetic. Spec
[§8](../../spl-meter-mvp/spec.md#8-calibration) and
[§10](../../spl-meter-mvp/spec.md#10-settings-and-persistence). The *surface* for all of this
is [The screen](b06-the-screen.md); this ticket is the machinery behind it.

**Four settings, all Rust-owned.** The frontend issues commands and holds nothing.

| Setting | Values | Default | Clears |
|---|---|---|---|
| Weighting | C / A / Z | **C** | window + max hold, zeroes filter state |
| Time weighting | F / S | **S** | max hold |
| Window length | 10 / 30 / 60 / 120 s | **60** | nothing (re-slice) |
| Calibration offset | `Option<f64>` | **`None`** | nothing |

**The offset is applied post-log, in Rust** (§8.2): `published_dB = 10·log₁₀(Σp²/Σn) + c`,
added after the log to **every** dB value crossing the bridge. As a gain before squaring, the
ring would hold calibrated energy and changing the offset would invalidate everything already
accumulated — a third reset cause, firing at the worst possible moment, since the calibration
gesture *is* repeatedly nudging the offset while watching the number. Post-log the number is
identical and **nothing resets**, which is what makes the gesture interactive: nudge it and the
settled 60 s average moves at once.

Kept in **Rust** rather than Vue so an uncalibrated number can never cross the bridge and get
rendered by mistake.

**One offset for all three modes** (§8.1). The conversion is dBFS → dB SPL, a property of the
**microphone**, not the weighting — and §5.3 renormalises all three chains to 0 dB at 1 kHz, so
they already share an absolute anchor. Per-mode offsets would also **break Z's whole purpose**
by forcing Z and C into agreement on the calibration spectrum. Z reads through the same offset,
which is what makes it the end-to-end diagnostic for the calibration chain.

**The 10 s match slice** (§8.3): computed against a **fixed 10 s slice of the ring — the last
100 slots — independent of the display's window setting.** This is *free*, a direct payoff from
§6.5 allocating the ring at maximum length: no second accumulator, nothing to override, nothing
to restore, and **the main 60 s meter is never disturbed**. At **zero** coverage in that slice
the L_eq is undefined, so the compute step cannot run and the command returns an error. Below
that, coverage is displayed and the reader judges — no refusal.

**Uncalibrated is a designed state, not an error** (§8.6). `Option<f64>`, **not** a default of
0. With `None`, all three numbers publish raw and the unit is **`dBFS`** — not a wrong SPL but
a correctly-named different quantity, with the unit carrying the honesty. A shipped default of
~+100 dB was rejected **firmly**: it would read roughly right out of the box at the cost of an
authoritative-looking number wrong by an unknown amount, which is the precise failure this
whole design is organised against.

**Persistence** (§10): one JSON file, `settings.json` in `app.path().app_config_dir()`, written
from Rust. `serde` and `serde_json` are **already** dependencies and `app_config_dir()` is core
Tauri called from Rust rather than over the bridge, so this needs **no new dependency, no npm
package and no capability entry** — `CLAUDE.md`'s four-step plugin ceremony does not apply.
`tauri-plugin-store` and `localStorage` both fail the ownership test.

- **Write through on every change, not on exit.** The app dies on backgrounding, possibly
  without running shutdown code (`11` probe 2b), so a deferred write is a lost setting. The
  file is four values.
- **Missing or corrupt falls back to defaults** — `(None, C, S, 60)` — with a log line and
  nothing in the UI.
- **A write failure is logged, not surfaced.** The setting applies in memory regardless;
  refusing it would be worse.

**Bounds** (§8.5): the **typed reference value** is bounds-checked to ~0–140 dB, because a
fat-fingered `683` for `68.3` would otherwise store a ~600 dB offset. **The derived offset
itself stays unclamped.**

**No metadata** (§8.7): four values stored and nothing about the calibration itself — no date,
no reference value, no mode. The reasoning is sharper than resisting complexity: **the offset
does not go stale with time, it goes stale with a hardware change.** A date would direct
attention to the wrong variable while detecting nothing.

## Traps

- The default time weighting is **S**, not F. `06 d5`'s own text still says `(None, C, F, 60 s)`
  — superseded by `09 d5`, and listed in spec §17.
- The offset is on the order of **+100 dB**, not a small trim — `02` measured an idle room at
  −59.6 dBFS in `Measurement` mode against a plausible 35–40 dB SPL. This is what ruled out a
  nudge-only gesture (1000 taps at 0.1 dB from cold).
- A calibration change must clear **nothing** — not the window, not the max hold, not the
  filter state, not the spectrogram ring. §6.11's bottom row.

## Done when

`cargo test` green on the arithmetic and the persistence round-trip, plus §14.2's remaining
case: **an offset change resets nothing and shifts all three numbers by exactly the delta.**
Also: the 10 s slice is unaffected by the window-length setting; zero coverage in the slice
errors rather than returning a number; a missing and a corrupt `settings.json` both fall back
to `(None, C, S, 60)`; the 0–140 bound rejects `683`.
