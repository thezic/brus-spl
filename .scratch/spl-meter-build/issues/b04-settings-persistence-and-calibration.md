# Settings, persistence and calibration arithmetic

Parent: [SPL Meter Build](../map.md)
Type: build
Status: resolved
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

## Resolved (2026-08-06)

`src-tauri/src/settings.rs`, plus the live weighting swap `b03` left here. **`cargo test` is 70
green** (31 new: 23 in `settings.rs`, 5 in `capture.rs`, and 3 that drive the real
[`Metrics`](b03-the-metrics-pipeline.md) alongside the settings). `cargo clippy --all-targets`,
`cargo clippy --target aarch64-apple-ios --lib` and `cargo fmt --check` all clean; `npm run build`
passes; verified under `npm run tauri dev` on macOS across four restarts.

Every clause of spec §8 and §10 needed no correction. Nothing in §16 is vetoed — **§16.11 met code
and stands**, the file being `settings.json` in `app_config_dir()` exactly as written.

### The module owns the values, not the side effects

`settings.rs` holds the four settings, the file and the arithmetic, and deliberately holds
**neither** of a change's two side effects: clearing the window and the max hold is
`metrics.rs`'s, zeroing the filter state is the audio callback's. A weighting change is therefore
three calls at the command site rather than one, which `b05` wires and each setter's doc comment
names. The alternative — one object owning the ring, the stream and the file — would have made
§6.11's table untestable, and that table is the one thing in this design most worth testing.

The bottom row needs no code at all: no offset enters `metrics.rs`, so *a calibration change
clears nothing* holds by construction. What this ticket adds is the test that makes the claim
checkable — the **raw** `Levels` are asserted bit-identical across two offset changes, which is
the strongest available form of "nothing reset".

### Findings

1. **The chain to zero is the one being switched *to*, not the one being switched from, and
   getting that backwards is invisible on the first switch.** The callback may not allocate, so
   all three chains are built in `build()` from the granted rate and the live selection crosses in
   as one relaxed `AtomicU8` — which means a chain retains the state it held when it was last
   live, possibly minutes ago. Resetting the *outgoing* chain reads just as plausible and is
   equally correct C→A, then wrong on the way back. `Chains::sync` runs once per block, before the
   sample loop, so a block is never half one weighting.

2. **A weighting change leaks up to one block of the old weighting into the newly cleared window,
   and the fix is an ordering constraint on `b05` rather than any machinery.** The callback picks
   the change up at its next block boundary while the window is cleared on the display thread, so
   ~21 ms of energy summed through the previous filter is sitting in the SPSC queue in between.
   The order that costs nothing: `Capture::set_weighting`, then `Capture::drain` **discarding**
   what comes out, then `Metrics::on_weighting_change`. Written on `set_weighting`'s doc comment
   where the caller will be looking. Getting it wrong is not visible on screen — it is a fraction
   of a dB in the first seconds of a fresh window — which is exactly why it is written down.

3. **`b02`'s finding 4 is not confined to `b02`'s tests: `sin²` over a partial cycle bit again,
   in mine.** 1024 samples of 31.5 Hz is two thirds of a cycle, which biased a per-block level by
   **0.58 dB** and failed an assertion that Z reads −3.010 dBFS at −3.593. The trap belongs to
   *any* test measuring a low-frequency tone over the callback's real 1024-frame block, not just
   to the weighting table. Fixed by asserting the **differences from Z** — 3.0 dB and 39.4 dB, the
   standard's own numbers — summed over a 40-block span, which drops the bias below 0.04 dB and
   leaves absolute filter accuracy where it belongs, in `b02`'s 21 tests.

4. **`#[serde(default)]` per field, which is more lenient than §10's letter and deliberately so.**
   §10 says missing or corrupt falls back to defaults; a file that parses but lacks a field is
   neither. The asymmetry of cost decides it: three of the four settings are a tap to restore,
   while the offset is only recoverable by standing next to the reference meter again (§8.5), so
   an added field must not be able to take it down with it. A file that does not parse still falls
   back whole, and a value that parses but cannot be honoured — a hand-edited `window_s: 45`, a
   non-finite offset — is repaired individually with a log line rather than discarding the rest.

5. **Verified on live audio, not only synthetically, and the weighting reaching the callback is
   the part that needed it.** Temporary instrumentation logged the readout under
   `npm run tauri dev`. Uncalibrated at `C`: −34 … −49 dBFS on room noise, the raw value passed
   through **unchanged** under `dBFS`. Then a hand-written `settings.json` of
   `(A, F, 120, +101.4)`: all four loaded, the unit switched to `dB`, every value landed at
   exactly `raw + 101.4` to the last digit — and the *raw* levels dropped to −43 … −52 dB, ~7 dB
   below the `C` run on the same LF-dominated room, which is the A chain genuinely in force in the
   callback rather than merely stored. A truncated file logged `EOF while parsing` and fell back to
   `(None, C, S, 60)` on the real config path. The instrumentation was removed and the app re-run
   clean; `lib.rs` keeps only the startup load, the `AppState` field and the readout fields.

6. **Mutation-tested, eight breakages, all caught.** Offset never applied (**3 tests**), no
   filter-state zeroing on a switch (2), default `F` instead of `S` (2), the reference bound
   widened to 1000 (2), the derived offset clamped to 0–140 (1), a weighting change not written
   through (1), the 10 s slice following the window setting (**3**), and a hand-edited window
   length left unrepaired (1). The three caught by a single test each are thin, and each of those
   tests exists for exactly the property named.

### What was verified where, and the one thing that was not

`app_config_dir()` resolves to `~/Library/Application Support/net.thezic.decibel-meter/`, logged
at startup because on a device that file is the offset's only recovery path and nothing else says
where it is. Load, the corrupt fallback and the persisted weighting reaching capture were all
exercised against that real path; **write-through was exercised against the real filesystem only
by the tests**, because until `b05` there is no command that changes a setting at runtime. The
`settings.json` written by hand during this pass was removed afterwards.

Also not run: the swap on a **live cpal stream**. The mechanism is tested exactly as the callback
drives it — `sync()` once per 1024-frame block, the selection arriving through the same atomic
`Capture::set_weighting` writes — but nothing has yet flipped it while a real microphone was
running, because the command that would is `b05`'s.

### What `b05` inherits

- `SettingsStore::load(Option<PathBuf>)` is already wired in `lib.rs` and lives in `AppState`
  behind a `Mutex`. The five setters return the new `Settings`, which is what the commands return.
- `Settings` serializes **exactly** as §9.1's `settings` block — `"C"`/`"F"`/`120`/`101.4`/`null`,
  snake_case, asserted by a test against the contract rather than by reading.
- `Settings::calibrated(Option<f64>)` and `Settings::unit()` are the whole of §8.2 and §8.6. Every
  dB value crossing the bridge goes through the first, and the second travels beside them.
- **Rounding to 0.1 dB is not done here.** §9.1 rounds on the wire and §11.4 makes 0.1 dB a
  presentation decision, not a setting; the offset is stored and applied at full precision.
- `offset_from_reference(reference_db, metrics.leq_over(10).0)` is the calibration command. Its
  `Err` is a `SettingsError` with a `Display` impl; the wire form of an error is `b05`'s to choose.
- `Capture::set_weighting` / `Capture::weighting`, with finding 2's ordering constraint.
- `settings::DEFAULT_WINDOW_S` and `Metrics::new`'s hard-coded 60 are not wired together, so
  `b05` must call `metrics.set_window_s(loaded.window_s)` at startup rather than rely on the two
  agreeing.

### Verification run

| command | result |
|---|---|
| `cargo test` | **70 passed, 0 failed** (31 new, 39 `b02`+`b03`'s) |
| `cargo clippy --all-targets` | clean, no warnings |
| `cargo clippy --target aarch64-apple-ios --lib` | clean, no warnings |
| `cargo check --target aarch64-apple-ios --lib` | clean |
| `cargo fmt --check` | clean |
| `npm run build` | pass (`vue-tsc --noEmit` + `vite build`) |
| `npm run tauri dev` on macOS, 4 restarts | defaults, persisted `(A, F, 120, +101.4)`, corrupt fallback, clean re-run; finding 5 |
| mutation pass, 8 breakages | all caught; finding 6 |
