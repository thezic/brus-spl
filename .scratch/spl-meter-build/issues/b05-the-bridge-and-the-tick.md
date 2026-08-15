# The bridge and the 10 Hz tick

Parent: [SPL Meter Build](../map.md)
Type: build
Status: resolved
Blocked by: [`b03`](b03-the-metrics-pipeline.md), [`b04`](b04-settings-persistence-and-calibration.md)

## Build

`bridge.rs`, `src/bridge.ts`, and the thread that publishes. Spec
[§9](../../../docs/spec.md#9-the-rust--frontend-contract) — the wire contract in §9.1 is
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

## Resolved (2026-08-06)

`src-tauri/src/bridge.rs` and `src/bridge.ts`, plus the tick thread and the six commands wired in
`lib.rs`. **`cargo test` is 91 green** (20 new). `cargo clippy --all-targets`, `cargo clippy
--target aarch64-apple-ios --lib`, `cargo check --target aarch64-apple-ios --lib` and `cargo fmt
--check` all clean; `npm run build` passes; verified under `npm run tauri dev` on macOS across
five launches, with a real microphone, against every item of the Done-when.

Spec §9 needed no correction, and **nothing in §16 is vetoed — §16.6 met code and stands**: a
dedicated std thread with a sleep-until loop on `Instant` measured **30 ticks in 3.000 s = 10.00
Hz**, and there was not one `emit` failure across five runs. What did need correcting is **this
ticket's own contract, in one place the type system could not see** — finding 1.

### One settings shape on the wire, not two

**Finding 1 — the six commands were returning a different `Settings` from the one the tick
carries, and nothing failed.** §9.1 writes `settings: { weighting, time_weighting, window_s,
offset_db, unit }` inside the tick block and, separately, "all settings commands return the new
`Settings`". Read as two sentences that is two shapes: `b04`'s `settings::Settings` is those four
values *without* the unit and with the offset at full precision, and it is the natural thing for a
command to hand back — its setters already return it.

The cost is invisible from Rust and invisible from `vue-tsc`. `src/bridge.ts` declares one
`Settings` interface for both, so a command return simply read `undefined` for `.unit` at runtime —
exactly the failure mode this ticket's own trap list accepts for *renamed* fields, arriving here
through a shape mismatch instead. It matters because [the screen](b06-the-screen.md) is required to
take picker feedback **from the return value rather than the next tick**: the one settings act that
changes the unit is calibration, so for up to 100 ms the sheet would have painted a freshly
calibrated number under no unit at all. That is §9.2's footgun-denial defeated at the only moment
it was written for.

Fixed by giving every command the same `WireSettings` the tick carries — five fields, offset
rounded to 0.1 dB like every other dB value crossing. A command's answer and the next tick's answer
are now asserted **byte-identical as JSON**, which is the form of the claim that cannot rot.

**It was found by running the app, not by reading it.** Both halves typecheck, both halves are
covered by tests, and the tests passed: the Rust tests asserted the tick's shape and `b04`'s
asserted the store's, and neither could see that the frontend was being handed the second where it
expected the first. The seam between two individually-correct contracts is the one place a
hand-written bridge can go wrong quietly, and it is why the dev-loop pass was driven rather than
described.

### The other two things the live run said

**The offset is rounded on the wire, and the store keeps it exact.** Matching a reference stored
`+117.2` on screen against `117.16…` on disk. Rounding at the store instead was rejected — `b04`
deliberately leaves the derived offset unclamped and unrounded — and rounding on the wire is what
makes §8.5's *"the offset shown as a number and directly editable"* stable: retyping what is
displayed moves the calibration by at most 0.05 dB, against an instrument whose own honesty is
±1 dB, where `101.43871…` in an editable field would be absurd.

**After a Reset the max hold is non-null again within one tick, at the current level.** Not a bug
and worth writing down before [the screen](b06-the-screen.md) assumes otherwise: §6.8 clears the
hold and deliberately does **not** touch the smoother, and the max compare runs at block rate — so
with sound in the room the next 4–5 blocks re-latch it immediately. Measured `max 61.1 → 58.5` with
`14.9s → 0.1s` of coverage beside it. **`MAX --` after a Reset is a silence state, not a Reset
state.** The synthetic test asserts `None` because no block follows the reset on a synthetic clock;
both are right, and the difference is only whether audio is arriving.

### What the tick thread does, and the lock that makes it safe

The thread drains the SPSC queue, deposits into the ring, advances the smoother, compares the max
at block rate, re-sums the window and publishes — **all under the `metrics` lock**, with one
`Instant` for the whole drain so §6.3's whole-block placement holds. Lock order is `metrics` then
`settings`, everywhere, which is the entirety of the deadlock argument.

Holding that lock across a whole command is what makes `b04` finding 2's ordering constraint
actually work. `set_weighting` switches the callback's chain, drains and **discards**, then clears
the window — and the tick thread cannot deposit between the discard and the clear, because it wants
the same lock. One block still in flight inside the callback is not covered by any of this and
cannot be: it is the ±21 ms §6.3 already tolerates.

The **granted** sample rate is only known on the capture thread, so the ring is built at the
preferred rate and corrected on the first tick that sees a running stream. Nothing can have been
deposited before that, so the correction is exact rather than an approximation — and it is the same
path [interruption and recovery](b07-interruption-and-recovery.md) needs for a route change, where
§6.11 says a rate change clears neither the window nor the hold.

Falling behind re-bases rather than firing catch-up ticks. A tick is a *snapshot*; the data a late
tick would otherwise drop rides in `columns`, which is why that array exists while it is still
empty.

### The input state

`session::permission()` reads `AVAudioSession.recordPermission` live on every tick — the
authoritative answer §9.4 insists on, rather than the exact-zeros heuristic. Same deprecated API
`b01` already uses to *ask*, for the same reason: it works, and it works on older devices.

Precedence is **permission first, stream second**, and that order is load-bearing: a denied
microphone still builds a stream and still delivers callbacks, so `Running` must not be allowed to
report `capturing` over the top of an authoritative denial. Getting it backwards is invisible on the
desk — macOS never answers `Denied` at all — and on the phone it is the one failure the user can
fix. There is a test for the precedence for exactly that reason.

`Undetermined` — the permission prompt still on screen — reports `unavailable`, which is literally
true while it is up: no category, no stream, no input. macOS reported `capturing` throughout, which
is §9.4's desktop caveat behaving as predicted.

### Making the command site testable without a Tauri app

`State<'_, AppState>` cannot be constructed outside a running app, so every command is a one-line
delegate to an inherent method on `AppState`, and `Capture::new` was split out of `Capture::start`
to give a capture side with no thread, no session and no stream. The result is that **spec §6.11's
whole table is exercised with no hardware and no sleeping** — `b03`'s property, extended to the
place the table is actually composed rather than to its two halves separately. Each of those tests
runs through `collect`, the real publish path, so what is asserted is what the frontend would have
been sent.

What that leaves untested and what does not: **the argument names**. Tauri defaults command
arguments to camelCase and `rename_all = "snake_case"` overrides it; a mismatch is a runtime
rejection, not a compile error. Every one of the six was pressed in the live run, which is the only
place that check exists. Also untested by unit test: the drain-and-discard inside `set_weighting`,
which needs a producer the unstarted `Capture` has no way to have.

### Mutation pass

Nine deliberate breakages, **eight caught immediately and one that got through**:

`the_ring_index_advances_on_the_clock_with_no_audio` catches reading `now_slot` before `levels`;
the input-state precedence, a no-op `round_01`, an offset not applied to the max hold, the smoother
not cleared on a weighting change, the smoother cleared on a Reset, a window clear added to the F/S
change, and a hard-coded unit were all caught by one to four tests each.

**The survivor is worth recording.** `an_unsupported_window_length_changes_nothing_on_either_side`
passed over a version that re-slices the ring *before* validating the length, because it filled the
ring with 20 s and then rejected 45 s — and a 45 s window over 20 s of audio reports exactly what a
60 s window does. Coverage is only diagnostic where the ring holds **more** than the shorter of the
two lengths. Fixed by filling 50 s; the test now fails on that mutation. It is `b03`'s
`the_ring_keeps_moving_with_no_audio_at_all` again in a different costume: a window test that never
exceeds the window is testing the slicing, not the bound.

### The temporary readout

`App.vue` now listens to the tick instead of polling, and carries a control strip for the six
commands — deleted whole by [the screen](b06-the-screen.md). The capture facts the tick has no
place for moved to a `capture_diagnostics` command polled at 1 Hz, and **it no longer drains**:
draining belongs to the tick thread alone, and two drainers would split the audio between them so
that neither number meant anything.

### How the dev-loop pass was driven

Every Done-when item is a thing you press, and nothing a build can read comes back out of a
WKWebView. So the pass was run by a temporary harness — `src/selftest.ts` plus a one-line
`selftest_report` command that `eprintln!`s into the dev log — which exercised the six commands from
the frontend and asserted on the ticks it received. **It is what found finding 1**, on its first
run, from a `unit` that was `undefined`. Both files were removed before the commit; `git status` and
a `grep` for `selftest` confirm nothing is left.

Of its three initial failures, one was finding 1 and **two were the harness being wrong about the
app** — the Reset/max-hold expectation above, and an assertion that a freshly calibrated `L_eq`
reads the typed reference to ±1 dB. The second is worth its own line: it was calibrating against a
2.2 s slice, where a live room's `L_eq` genuinely moves ~1 dB between one tick and the next. That is
not the meter being unstable, it is what a two-second average of a room *is* — and it is a small
piece of live evidence for §8.3's fixed 10 s slice. With the slice full the same assertion passed at
`cal_leq 68.3` against a typed `68.3`.

### What `b06` inherits

- `src/bridge.ts` — every crossing type and thin typed wrappers around `invoke`/`listen`. The only
  file in the frontend that imports `@tauri-apps/api`, and it stays that way.
- **One `Settings` type**, five fields including the unit, returned by all six commands and carried
  in every tick. Take picker feedback from the return; it is now safe to.
- `columns: []` in every tick, already typed, so [the canvas](b11-the-spectrogram-canvas.md) changes
  no contract.
- The `input` state, which on macOS will read `capturing` and never `denied` — drive the
  input-state line from a stub to see all three (§11.6).
- Everything is rounded to 0.1 dB before it crosses. Do not round again, and do not add an offset.

### Verification run

| check | result |
|---|---|
| `cargo test` | **91 passed, 0 failed** (20 new) |
| `cargo clippy --all-targets` | clean, no warnings |
| `cargo clippy --target aarch64-apple-ios --lib` | clean, no warnings |
| `cargo check --target aarch64-apple-ios --lib` | clean |
| `cargo fmt --check` | clean |
| `npm run build` | pass (`vue-tsc --noEmit` + `vite build`) |
| tick rate, live | **30 ticks in 3.000 s = 10.00 Hz**; zero emit failures over five launches |
| the wire, live | `{"now_slot":33,"meter":{"leq":-37.6,…,"input":"capturing"},"settings":{…,"unit":"dBFS"},"columns":[]}` |
| all six commands, from the frontend | each returned the new `Settings`; finding 1 |
| `set_calibration_from_reference(683)` | rejected — `reference 683 dB is outside 0–140 dB` |
| `set_calibration_from_reference(68.3)` | stored `+113.5`; the next tick read `cal_leq 68.3` |
| `set_window_length(120)` | re-sliced, not reset — `2.1s of 120s`, then `10s of 10s` beside it |
| `reset()` | `14.9s → 0.1s` coverage, `max 61.1 → 58.5`, calibration untouched |
| restart | `Settings { weighting: A, time_weighting: Fast, window_s: 120, offset_db: Some(101.4) }`, first tick already `dB` |
| mutation pass, 9 breakages | 8 caught, 1 survivor found and fixed |
| harness removed | `src/selftest.ts` and `selftest_report` deleted; the test `settings.json` removed |
