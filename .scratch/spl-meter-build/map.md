# SPL Meter Build

Labels: `wayfinder:map`

## Destination

A meter **you can trust at a talk**, and which has been read through a real talk at least once.

**Scope flexes; the instrument does not.** The tickets are cut so that stopping at any boundary
leaves a real instrument rather than half of one, and so the cut always falls cleanly between
them. Which things go first is written down in [Route](#route) rather than decided under pressure.

This map used to carry a target date. It is gone deliberately: it had drifted from the truth, and a
countdown in a document is worse than no countdown, because it gets quoted back as if it were a
commitment. **What survives is the priority order, which is the part that was ever useful.**

The authority for *what* to build is
[`spl-meter-mvp/spec.md`](../spl-meter-mvp/spec.md), the destination of
[the closed `spl-meter-mvp` map](../spl-meter-mvp/map.md). **No decision in the spec gets
re-litigated here** — tickets cite section numbers (`§6.3`) and the spec cites the mvp ticket
that made the call (`05 d2`), so the reasoning is two links away and never restated.

Where the spec says `NN d<n>` it means an **mvp** ticket. This map's own tickets are
**`b01`–`b13`**, prefixed precisely so the two numbering schemes can never be confused.

## Notes

**This map carries execution.** That is the wayfinder override, taken deliberately: every
decision was made by the map this one succeeds, so there is nothing left to decide and the
work is to build it. Tickets produce **code**, not decisions, and carry a fifth label,
`wayfinder:build`, rather than being mislabelled `task` — the two venue tickets are genuine
`task` tickets and the distinction is worth keeping visible.

**Tracker:** local markdown, as before. The map is this file, tickets are files under
`issues/`, and `ls issues/` is the query. Claim a ticket by setting `Status: claimed` before
any work.

**Done, for a build ticket:**

- `cargo test` green,
- runs under `npm run tauri dev` on macOS,
- `cargo clippy` clean and `cargo fmt` applied,
- `cargo check --target aarch64-apple-ios --lib` **if the ticket touches iOS-only code** —
  which a plain `cargo check` skips entirely,
- a **device pass** closes each tier, and a device build closes
  [Session, capture and the idle timer](issues/b01-session-capture-and-the-idle-timer.md) as
  the framework-link canary. Nothing links until Xcode does, so the link is not checkable any
  other way.

**Standing preferences, inherited and still binding:**

- **Resist complexity.** The spec is already the simplest thing that answers the question. If
  a ticket seems to want more machinery than the spec describes, that is a signal to re-read
  the spec, not to add it.
- **He makes the call, not the app.** No warnings, no automation, no metrics not asked for.
- **The instrument reports its own state.** Where the spec refuses to publish a number
  (`--`, `dBFS`, coverage), that refusal is the feature. Do not "fix" it.

**Facts about this repo that bite, all confirmed:** see `CLAUDE.md`. The three that will
actually cost a day if forgotten — the iOS target must link `AVFAudio`/`AudioToolbox`/
`CoreAudio` via `bundle.iOS.frameworks`; never run any `tauri ios` command with `FORCE_COLOR`
set; and `println!` does **not** reach `devicectl … --console`. That last one was read for two
tickets as *on-device diagnostics have to be on screen*, and `b08` found it is only half true:
**`idevicesyslog` carries Rust's `eprintln!` verbatim, tagged `[stderr]`**, so the log is readable
from a plain shell after all.

**Commits:** one per ticket, to `main`, following the existing `Resolve ticket NN` pattern.
There is no remote.

## Route

Three tiers and the venue. Tickets are listed one line each; the bodies hold the detail.

### Tier 1 — the trustworthy number

Everything needed for a number that can be judged against the imposed 70 dB ceiling.

| | Ticket | Blocked by |
|---|---|---|
| `b01` | [Session, capture and the idle timer](issues/b01-session-capture-and-the-idle-timer.md) | — |
| `b02` | [The weighting filters and their 34-row test](issues/b02-the-weighting-filters-and-their-test.md) | — |
| `b03` | [The metrics pipeline](issues/b03-the-metrics-pipeline.md) | `b02` |
| `b04` | [Settings, persistence and calibration arithmetic](issues/b04-settings-persistence-and-calibration.md) | `b03` |
| `b05` | [The bridge and the 10 Hz tick](issues/b05-the-bridge-and-the-tick.md) | `b03`, `b04` |
| `b06` | [The screen](issues/b06-the-screen.md) | `b05` |
| `b07` | [Interruption and recovery](issues/b07-interruption-and-recovery.md) | `b01` |
| `b08` | [Tier 1 device pass](issues/b08-tier-1-device-pass.md) | `b06`, `b07` |

`b01` and `b02` are **genuinely independent** — the filters take synthetic input and need no
hardware, no session and no device — so they are the one pair worth running as concurrent
sessions.

### Tier 2 — the picture

Blocked behind the Tier 1 device pass, and **explicitly the thing that gets dropped** if the
date closes in.

| | Ticket | Blocked by |
|---|---|---|
| `b09` | [Spectrum analysis and the column ring](issues/b09-spectrum-analysis-and-the-column-ring.md) | `b08` |
| `b10` | [The spectrogram half of the bridge](issues/b10-the-spectrogram-half-of-the-bridge.md) | `b09` |
| `b11` | [The spectrogram canvas](issues/b11-the-spectrogram-canvas.md) | `b10` |

### The venue

| | Ticket | Blocked by |
|---|---|---|
| `b12` | [Re-sign and install rehearsal](issues/b12-re-sign-and-install-rehearsal.md) | `b08` |
| `b13` | [The venue run](issues/b13-the-venue-run.md) | `b12` |

**[The venue run](issues/b13-the-venue-run.md) is the destination**, not a follow-up. The map
is done when it is closed.

### Drop order

Not a schedule. **If you have to stop, stop here** — in this order and no other:

1. ~~**[Interruption and recovery](issues/b07-interruption-and-recovery.md)**~~ — **built, so
   this slack is spent.** It was the only droppable Tier 1 ticket: without it an interruption is
   still *honest*, because §6.4's coverage figure reports the hole; it is simply not *recovered*,
   and the fix at the venue is to restart the app. Honest-but-manual was an acceptable
   degradation; nothing else in Tier 1 is. The remaining drop order therefore starts at 2.
2. **All of Tier 2**, whole. A partial picture is worse than none — §7.1's colour scale only
   means something with the fixed band layout and the legend behind it.
3. Nothing else. Tier 1 is the instrument; below it there is no meter worth carrying to a room.
   **It is closed** — `b01`–`b08`, device pass included — so this line is now a statement about
   what must not be given back rather than about what might not land.

**What you lose by dropping Tier 2**, stated so the trade is made with open eyes: the number
answers *how loud*, and only the picture answers *what is making it and whether it has been
steady*. A constant HVAC rumble and a passing door slam are the same number and completely
different pictures — and dB(C) at a 70 dB ceiling is precisely the situation that makes the
difference matter (spec §7).

## Decisions so far

<!-- one line per resolved ticket: gist + link -->

- [The weighting filters and their 34-row test](issues/b02-the-weighting-filters-and-their-test.md)
  — **Built and green: 590 filtered tone measurements across four sample rates, tight to 1e-6 dB at
  48 kHz.** Spec §5.3 needed no correction — section table, bilinear recipe, cascade order and the
  digital 1 kHz renormalisation all produced the promised numbers first time, and `03` §2.4's
  coefficient fixtures agree to 1.7e-13 dB. What did not survive was the **test specification**, in
  three places, and all three are the kind that produce a confusing red rather than an obviously
  wrong one. **`±0.01 dB below 4 kHz` is contradicted by the table it ships with** — the 3150 Hz row
  publishes −0.019 dB at 44.1 kHz, so the tolerance is now ±0.01 dB *around the predicted bilinear
  deviation*, identical below 2.5 kHz and rate-general. **The `f_H` = 7943.282 Hz invariant is
  analogue-only** — the digital filter reads −3.66 dB there at 44.1 kHz, so it splits into a 1e-9
  assertion on the module constants (which is what actually catches a transposed `f₁`/`f₄`) and an
  end-to-end one against the warped prediction; `f_L` is unaffected and reads −3.010 flat at all four
  rates. And a **fourth trap nobody named**: `sin²` averaged over a partial cycle is biased by
  **0.035 dB at 10 Hz**, three times the tolerance and indistinguishable from a low-frequency filter
  error, so the measurement window is nudged to whole half-cycles. Also confirmed independently:
  **16 kHz cannot hold the design band above ~4 kHz** (C is out by 59.9 dB at 7943 Hz), which is
  `03` §2.5's ~39 kHz bisection arrived at from the other direction. The table is doing work — a
  deliberate mutation of one pole sum fails **10 of the 21 tests**. Two of the assertions are weak
  and say so: `|H(1 kHz)| = 0` and clause 5.4.14 both survive a corrupted section, because they test
  the renormalisation rather than the filter.

- [Session, capture and the idle timer](issues/b01-session-capture-and-the-idle-timer.md)
  — **Built and verified everywhere the hardware allowed, and resolved on that basis; the phone half is
  now [the Tier 1 device pass](issues/b08-tier-1-device-pass.md)'s.** The spike is deleted whole and
  `cargo build` produces one binary. **The framework-link
  canary passed** — `tauri ios build --debug` reached `BUILD SUCCEEDED` with zero `Undefined symbols`,
  which is the one thing no `cargo check` could ever have told us, and it now covers `objc2-ui-kit`
  too: the idle timer needed **no `bundle.iOS.frameworks` entry and no regeneration**, asserted by
  building rather than by reading. `FORCE_COLOR` was in fact `3` in the shell, so the map's warning
  was live rather than theoretical. On macOS the readout measures **46.875 blocks/s = 48000/1024
  exactly**, −43…−52 dBFS quiet against −18.8 dBFS on speech. The queue is **47 slots computed from
  the granted rate**, ~10× headroom against the 4–5 blocks a 100 ms tick drains. Two calls the spec
  did not make: the **permission request is kept** and moved to its own thread (its completion block
  arrives on the main queue, so inlining it in Tauri's `setup` deadlocks the app), and a **denial does
  not abort capture** — the stream is built, delivers exact zeros, §6.3 discards them, and the meter
  lands on the designed `--` / `0s of 60s` instead of a `Failed` state that would misread as §9.4's
  `unavailable`. Non-`f32` input is refused rather than converted in the callback, which §3.3 forbids
  there. Nothing in spec §16 was vetoed — `rtrb` degrades into lost coverage exactly as §6.10 wants,
  and §16.9's channel-0 rule cost one `chunks_exact` and never fired on the mono route.
  **`devicectl install` + `launch` never ran** — the paired iPhone 14 Pro reports `unavailable` — so the
  remaining risk is launch-time rather than link-time, and it sat as the first item of
  [the Tier 1 device pass](issues/b08-tier-1-device-pass.md) rather than holding this ticket open.
  **That half is now closed, by [`b07`](issues/b07-interruption-and-recovery.md)** — the phone came
  back `available`, `devicectl install` and `process launch` both worked, and the app runs and
  captures on the device. Launch-time risk is retired; what `b08` still owns from here is the
  `Measurement`-mode read-back, which needs the log and so needs Xcode.

- [The metrics pipeline](issues/b03-the-metrics-pipeline.md)
  — **Built and green: 18 synthetic tests, `cargo test` 39 overall, and verified against two minutes
  of live audio as well as against the table.** Spec §6 needed no correction. What did need one is
  **§16.7's rationale, though not its rule** — it justifies the not-positive clause with "exact-zero
  blocks arrive, so the staleness rule never fires", which presumes a zero-power block counts as an
  arrival. It must not: advance the smoother with `x² = 0` and NOW *decays* rather than vanishing,
  taking **~11 minutes at `S`** to underflow far enough for the clause to fire — the exact
  plausible-wrong-unlabelled number §6.9 exists to prevent, and a contradiction of this ticket's own
  `--` beside `0s of 60s`. §6.3's *"zero power is the absence of a measurement, not a quiet one"*
  settles it: such a block is discarded **whole**, is **not an arrival**, and the 200 ms rule fires in
  a fifth of a second. The clause itself stays and is not redundant — `smoothed` is exactly `0.0`
  before the first block, which is what would otherwise put `10·log₁₀(0)` on the wire. **The module
  never reads the clock**; every entry point takes `now`, which is what makes a 130 s session with a
  60 s hole a microsecond-long test with no hardware and no fake-clock trait. A **mutation pass found
  a real hole in its own tests**: `the_ring_keeps_moving_with_no_audio_at_all` passes on a ring that
  never clears a slot, because at 70 s nothing has wrapped — it was testing the window's *slicing*,
  not its *clearing*, a distinction only a session longer than the 120 s ring exposes, i.e. a talk.
  Live, coverage climbed 1 s per second to **60.10 and held at 60.0 ± 0.1**, L_eq settled near −45 dB
  moving ~0.1 dB/s — §6.10's "once a second on its own", measured — and the max hold rose monotonically
  and never fell. **The weighting seam is closed**: the callback runs `chain.process(x)` at `C`,
  derived from the granted rate. Swapping the chain live, and §6.11's filter-state zeroing, are left
  to `b04`, which owns the setting that drives them.

- [Settings, persistence and calibration arithmetic](issues/b04-settings-persistence-and-calibration.md)
  — **Built and green: 70 tests overall, and the calibration verified on live audio at both ends —
  raw under `dBFS`, then exactly `raw + 101.4` under `dB`.** Spec §8 and §10 needed no correction, and
  **§16.11 met code and stands.** The module owns the values and deliberately **not** the side effects:
  clearing the window and the max hold stays in `metrics.rs`, zeroing the filter state stays in the
  callback, so a weighting change is three calls at the command site rather than one. That is what keeps
  §6.11's table testable — and its bottom row, *a calibration change clears nothing*, now has the test
  that makes the claim checkable rather than merely stated: the **raw** `Levels` are bit-identical
  across two offset changes. The inherited live chain swap is done by **pre-building all three chains**
  and passing the selection in as one relaxed `AtomicU8`, which is what makes it allocation-free — and
  the consequence is finding 1: **the chain to zero is the one being switched *to***, since a chain
  retains state from whenever it was last live. Resetting the outgoing one reads equally plausible and
  is wrong on the second switch. Finding 2 is an ordering constraint `b05` must honour: the callback
  switches at its next block boundary while the window is cleared on another thread, so **~21 ms of
  old-weighted energy leaks into the fresh window** unless the queue is drained and discarded in
  between — invisible on screen, which is why it is written on `set_weighting`'s doc comment. **`b02`'s
  finding 4 bit again, in this ticket's own tests**: 1024 samples of 31.5 Hz is two thirds of a cycle
  and biased a per-block level by 0.58 dB, so the trap belongs to any test measuring a low tone over
  the callback's real block size, not just to the weighting table. Persistence is one deliberate
  departure from §10's letter — `#[serde(default)]` **per field**, because three settings are a tap to
  restore and the offset is only recoverable beside the reference meter. Eight mutations, all caught;
  three by a single test each. **And quantifying finding 2 turned up finding 7 — the first correction
  this map has made to the spec rather than to a ticket.** §6.11's table had **no column for the
  smoother**, and the omission defeated the max-hold clear on its own weighting row: `smoothed` keeps
  the old weighting's level, so the next block re-latches the hold and a hold never falls. Probed at
  **7 dB high and permanent** — twenty times finding 2's leak, and unlike it not self-erasing. One
  line in `on_weighting_change` fixes it, two tests cover it, both fail when it is reverted, and spec
  §6.11 and §17 now carry the column and the correction. Not yet run: the swap on a **live** cpal
  stream, and write-through outside the tests — both wait on `b05`'s commands.

- [The bridge and the 10 Hz tick](issues/b05-the-bridge-and-the-tick.md)
  — **Built and green: 91 tests, and every item of the Done-when driven live against a real
  microphone rather than described.** Spec §9 needed no correction and **§16.6 met code and stands** —
  a std thread with a sleep-until loop on `Instant` measured **30 ticks in 3.000 s = 10.00 Hz**, with
  not one `emit` failure across five launches. What did need correcting is **the ticket's own
  contract, in the one place neither compiler could see it**: §9.1 writes the five-field `settings`
  block inside the tick and, in a separate sentence, that the commands return "the new `Settings`" —
  and `b04`'s `Settings` is those four values *without* the unit and with the offset unrounded. Both
  halves typecheck, both are tested, and `src/bridge.ts` declares one interface for both, so a
  command return simply read **`undefined` for `.unit`** at runtime. It matters precisely once:
  [the screen](issues/b06-the-screen.md) must take picker feedback from the return rather than the
  next tick, and the one act that changes the unit is calibration — so §9.2's footgun-denial would
  have failed at the only moment it was written for. Every command now returns the same
  `WireSettings` the tick carries, asserted **byte-identical as JSON**. **It was found by running the
  app on its first launch, not by reading it** — the seam between two individually-correct contracts
  is where a hand-written bridge goes wrong quietly, which is the argument for driving a dev-loop
  pass rather than describing one. Two smaller things the live run settled: the offset is rounded on
  the wire while the store keeps it exact (`+117.2` on screen against `117.16…` on disk), which is
  what makes §8.5's editable offset stable under a retype; and **after a Reset the max hold is
  non-null again within one tick**, at the current level (`61.1 → 58.5` beside `14.9s → 0.1s`),
  because §6.8 deliberately spares the smoother and the max compares at block rate — so `MAX --` is a
  silence state, never a Reset state. Structurally: the tick drains, deposits and publishes **under
  the `metrics` lock**, which is what makes `b04` finding 2's discard-then-clear ordering actually
  hold against the tick thread; `Capture::new` was split out of `Capture::start` so that **spec
  §6.11's whole table is now tested at the site that composes it**, with no hardware and no sleeping.
  A nine-breakage mutation pass caught eight; **the survivor is worth repeating** — a window-length
  test that fills the ring with *less* audio than the length it rejects cannot tell a re-sliced ring
  from an untouched one, which is `b03`'s ring-clearing hole in a different costume. Untested and
  said so: the drain-and-discard inside `set_weighting`, which needs a producer an unstarted
  `Capture` cannot have, and the snake_case **argument** names, which only the live press exercises.

- [The screen](issues/b06-the-screen.md)
  — **Built, and every Done-when item driven rather than described; spec §11 needed no correction.**
  Five components plus `src/display.ts`, 91 tests still green. The dev window **cannot be clicked
  from an agent session** — no accessibility grant — which turned this ticket's own trap (*drive the
  input-state line from a stubbed value*) into the method for the whole pass: a fake
  `__TAURI_INTERNALS__` so the real components could be clicked in Chrome at phone widths, and a timed
  script pressing the six commands against the **real Rust backend** in the dev window. Each caught
  what the other could not — the browser exercised the sheet (the `683` typo rejected *with its reason
  on screen*, `68.3` matched to a `106.4` offset, the armed Reset, and the `denied` line macOS cannot
  produce), the dev window proved the **labels** against real settings, where one frame reading
  `NOW · A · fast` / `LAeq 120s` / `2s of 120s` / `dB` is four commands confirmed at once.
  **Finding 1 is the one worth carrying: the hero size is a measurement, and guessing it was wrong by
  a whole rem in the dangerous direction.** §11.5's *size for the widest state* does not say what that
  costs, and the arithmetic is not the one a reader would do — `−108.4` is **3.09 em** in the system
  sans, so a flat 7.5rem renders it at **371 px and overflows a 375 px phone**, while the 6.5rem that
  fits one throws away 15 % of the glyph height on a 430 px phone. Neither is a size; the constraint
  is `font-size ≤ (100vw − 32px)/3.09`, now `min(7.5rem, calc(31vw − 12px))`, checked at four widths.
  It is invisible at the desk — the app is calibrated a minute after launching — and it is the one
  screen shown before anyone has calibrated anything. Finding 2 is its consequence in the wide
  reflow, where `vw` is the *window* and not the column, so the left column is sized by the hero
  (`minmax(23.5rem, 24rem)`) rather than by the picture. Finding 3: **the armed Reset is cyan, and the
  spec is right** — red is the alarm colour §11.8 rules out in the same sentence that names the armed
  Reset, and the accent is cool so the inferno ramp can never lend it the meaning of a *level*.
  **Corrected afterwards by `b07`'s device pass, and the correction is about the method:** the
  calibration fields **could not be typed into at all on a comma-locale phone**. iOS gives an
  `inputmode="decimal"` keypad the *locale's* separator and no other, so `Number("68,3")` is `NaN`
  and both buttons sat permanently disabled with nothing on screen — the silent no-op this ticket's
  own `apply` exists to prevent, reached from the one direction that bypasses it, and a break of
  spec §8.5's sticky-note recovery path rather than an inconvenience. **Both halves of this ticket's
  pass ran against a Mac keyboard, which has a `.` key**, so no amount of browser or dev-window
  driving could have found it. `typed()` now takes either separator and refuses two rather than
  guessing.
  Two smaller calls: coverage is **rounded rather than floored**, because `b03`'s 60.0 ± 0.1 makes
  flooring flicker `59s`/`60s` on a full window; and a command's answer **outranks the tick for
  120 ms**, since a tick already in flight arrives carrying the old settings and snaps the segment
  back for a frame. **`capture_diagnostics` is deleted** as `b05` intended and `session.rs` §3.1
  already required — with one consequence named rather than buried: the **`Measurement`-mode
  read-back, the highest-consequence check in the effort at 21 dB silently**, is now only in the log,
  so [the Tier 1 device pass](issues/b08-tier-1-device-pass.md) reads it from Xcode rather than from
  `devicectl`. Not verified live and said so: `--` was rendered at its real size and is **not** a
  redaction bar, but was never produced by a stream that actually stopped, because nothing here can
  stop one.

- [Interruption and recovery](issues/b07-interruption-and-recovery.md)
  — **Built and green: 96 tests plus one `#[ignore]`d live one that drives the whole supervisor
  against two real cpal streams in 0.7 s; the phone half is
  [the Tier 1 device pass](issues/b08-tier-1-device-pass.md)'s, as `b01`'s was.** Spec §4 needed no
  correction. The shape is one **supervisor loop on the capture thread** — forced there, because a
  `cpal::Stream` is `!Send` and stops on drop — waiting on a one-slot mailbox that three unrelated
  things post to. What §4.2 does *not* say is finding 1: **the health check must ignore any stream
  that is not `Running`**, because a rebuild publishes `Starting` and a check that fired on it would
  ask the supervisor to tear down the stream it had just built, one rebuild per tick forever. Two
  things about cpal came out of reading it rather than running it: **`DeviceChanged`'s doc advice is
  wrong for this instrument** — "no rebuild is required" is true of the samples and false of
  coefficients derived from the build-time rate. (It also claimed *neither CoreAudio backend raises
  it at all*. **That was wrong**, from a grep truncated at twenty lines;
  [`b08`](issues/b08-tier-1-device-pass.md) corrects it — cpal's iOS backend maps route changes onto
  error kinds, so the error callback is a live path there rather than defence.) Finding 3 is two opposite
  mistakes one line apart: the recovery path must **not** re-ask for the microphone (it blocks on the
  main queue, twice a second, for the length of a Siri call) and must **not** stop asking either — so
  it is `Initial` until a build has *succeeded*, not until one has been attempted, or a first failure
  before the prompt leaves an app that never asks. The loop is designed around **failure being
  normal**: every `setActive(true)` is refused while another process holds the session, so recovery
  is just the first attempt that succeeds — 500 ms retry, never give up, log once per run, keep only
  the *first* reason. §4.2's item 6 turned out to be **zero code and structurally so**: `build`
  derives the chains from the rate it reads back, a fresh chain is zero-state, and `capture.rs`
  cannot reach `metrics.rs` to reset a window by accident. The one new number: **a recovery is
  ~210 dead milliseconds** (209–222, ±4, five runs), which is *over* §6.9's 200 ms threshold and can
  only be longer on the phone — so every recovery shows `--`, correctly, and half of `b06`'s
  "`--` was never produced by a stream that actually stopped" is now closed. Five mutations, all
  caught, each by exactly one test. **And the device pass happened here rather than being deferred,
  because the iPhone 14 Pro became `available` mid-ticket — so `b01`'s unfinished half closes with
  it and something in this effort has finally run on the phone.** The Siri test passes: `--` while
  the microphone is gone, coverage dropping and climbing back with the hole still visible, and the
  meter resuming on its own, effectively instantly. Then the second item was **forced rather than
  waived** — a variant with the observer compiled out, installed, and the test repeated: **it still
  recovers, in ~0.5 s instead of instantly.** So the health check is sufficient alone and the
  notification is **the fast path, not the mechanism** — the right way round, since the reliable
  trigger is the one that cannot fail to arrive. `objc2-foundation` needed **no
  `bundle.iOS.frameworks` entry and no regeneration**, asserted by building. Untested and said so:
  the retry loop, and route-change recovery — half of which `b08` then caught by accident.

- [Tier 1 device pass](issues/b08-tier-1-device-pass.md)
  — **Tier 1 runs on the phone and every item has an answer. Two corrections to the spec, one to
  this ticket's own method, and one measurement worth more than the checklist.** The method went
  first: §2.2's *`println!` does not reach `devicectl … --console`* had been read for two tickets as
  *on-device diagnostics must be on screen*, and it is only half true — **`idevicesyslog` carries
  Rust's `eprintln!` verbatim, tagged `[stderr]`**, from a plain shell with no Xcode. That is what
  made an unattended fifteen-minute log possible, and everything below came out of it.
  **`measurement_mode: true`** — the 21 dB question, the highest-consequence check in the effort,
  answered — with rate, channels and `io_buffer_duration` all granted *exactly* (1024/48000 to the
  last digit). **The idle timer is now a measurement**: frontmost 14 min 28 s with `deviceLocked:0`
  and no dimming, against the same phone dimming after 42 s and locking after 61 s once
  backgrounded — so §4.4's two lines held the screen for ~14× the device's own auto-lock.
  **But the finding is spec §4.3.** It recorded `11` probe 2b as *the app dies* on backgrounding,
  cause uncharacterised. **It does not die** — same PID across 70 s backgrounded including a lock
  and unlock. What stops is the session: iOS refuses `setActive(true)` for a non-frontmost app with
  `AVAudioSessionErrorCodeCannotStartPlaying` (`'!pla'`), for exactly as long as it is out of the
  foreground. Recovery took **3 s from returning**, unaided — so backgrounding went from *restart
  the app* to *nothing to do*, and it is `b07`'s two least-defensible-looking choices that did it:
  **retry forever** rather than give up, and **log once per run**, which is why ~140 refused
  activations produced one line instead of burying the recovery. Not an argument for the `audio`
  background mode: the meter stays foreground-only and coverage reports the hole; the hole now
  closes itself. Smaller: a route change fires at **every** launch (harmless, 70 ms, and
  `Capture::builds` therefore reads `2` after a clean launch), `S` reads live rather than twitchy,
  `--` reads fine at arm's length, and settings survived four install-over-the-tops. Deliberately
  not tested: the **uncalibrated state**, because seeing it means clearing a good calibration and
  the honest way to do that is [`b12`](issues/b12-re-sign-and-install-rehearsal.md)'s own gesture;
  and **`denied`**, because the microphone was granted.

- [Spectrum analysis and the column ring](issues/b09-spectrum-analysis-and-the-column-ring.md)
  — **Built and green: 114 tests, and the ring driven against a real microphone at both `--debug`
  and `--release`. Spec §7.1's parameters needed no correction; §16 needed two, and the first is
  worth the ticket on its own.** **§16.4's power normalisation is right for a bin and 1.76 dB wrong
  for a band, and the band is what this display draws** — `2·|X_k|²/S1²` is the coherent gain, so a
  full-scale sine reads −3.01 dBFS in its *peak bin* while the **sum** across the band reads
  `0.5 + 0.125 + 0.125` against a true 0.5, a flat `10·log₁₀(1.5)` high for a tone and for noise
  alike. §16.4's own stated intent — *the same dBFS convention as the meter, which is what lets one
  calibration offset shift both* — is delivered only by the noise-power form `2/(N·S2)`, and the
  property that buys is now a test: **the 32 bands add back up to the signal's own unweighted
  dBFS.** It would have been invisible on screen, a uniform shift of a colour window §7.1 already
  expects to move, which is precisely the case §16 exists for. Second: **which band *borrows* is a
  function of the sample rate** — at 48 kHz the 12.5 Hz row keeps bin 2 and it is **20 Hz** that
  borrows, at 44.1 kHz the reverse — so §13.12 named the wrong row, and the rate-independent claim
  (**four rows are narrower than a bin**) is the stronger one. A third finding is the same
  arithmetic reaching higher: **the residual ripple in a flat picture is band-edge quantisation,
  predictable in closed form** — under 0.1 dB at 1 kHz and **−1.95 dB at 40 Hz** — so §14.3's *pink
  draws flat* means flat *around the prediction*, `b02` finding 1's shape exactly. Not fixed:
  fractional edges would re-litigate §16.5 for under 2 % of a 60 dB scale. **White noise gets the
  companion test §14.3 lacks** — it must rise exactly `10·log₁₀(n_bins)` per row, which kills `mean`
  as a cell value outright. The FFT tap is a second `rtrb` queue at the same ~1 s as the block queue,
  and finding 4 is that **its overflow is not the block queue's**: rtrb drops the *newest*, so the
  survivors are older than the slot they would be drawn in, and a lost sample is therefore a
  discontinuity — holes, never a splice. The one piece of design the spec left open is the slot
  assignment, and it is **`now_slot` places the columns, the sample count paces them**: `k` hops
  completed in a drain are the `k` slots ending at `now_slot`, which is what makes §9.2's backlog
  rule correct rather than merely robust. Its consequence bit the live pass first: a tick completing
  **zero** hops files nothing and the next fills that slot retroactively, so reading the ring at the
  publish instant shows holes that are not holes — measured two slots behind instead, it is **20 of
  20, sustained, with zero lost samples**. Live at 48 kHz: a quiet room peaks in the **50 Hz row at
  −53 dBFS** with everything above 1 kHz below −80 — the rumble stripe, present before anyone speaks
  — and the whole drain-and-transform costs **50–125 µs per tick in release**, 0.1 % of a tick, which
  settles §16.1's throughput argument. Untested and said so: the picture is **not on the wire**, so
  nothing has been *seen* — §14.3's eyeball tests are asserted numerically, arm's-length legibility
  is untouched, and nothing has run on the phone.

- [The spectrogram half of the bridge](issues/b10-the-spectrogram-half-of-the-bridge.md)
  — **Built and green: 122 tests, 8 of them new, both halves driven live under `npm run tauri dev`.
  §9.1's wire block, §9.5's table and §7.1's span rule all stood; §9.2's one-line description of
  `columns` did not, and that sentence is the ticket.** **"Every real column since the previous
  publish" loses exactly one column per stall, and the loss is invisible from either side alone.**
  Read literally it means *ship what this tick filed*, which is a cursor at `now_slot + 1` — and
  `b09` already established that a tick completing zero hops files nothing while the next fills
  that slot retroactively as `now_slot − 1`, so the cursor steps straight over a real column.
  Nothing fails loudly: the column is in the ring, `get_spectrogram` hands it back, and the symptom
  is **the picture disagreeing with itself across a reload, one slot at a time** — a hole that heals
  when you resize the window. The range is **`(last shipped, now_slot]`**, which is also the
  interval §9.2 already hands the *frontend* for deciding what is a hole, so what Rust ships from
  and what the canvas draws into are one expression. §9.2 and §17 carry it. Second: **the backlog
  is real on the desk and it is eight columns** — a deliberate 700 ms stall of the tick thread
  jumped `now_slot` 200 → 208 and the next tick carried 8 columns in 3098 bytes, with
  `now_slot − columns` unchanged across it and nothing lost; still inside the 8192-byte fast path,
  which only breaks past ~20 columns. Third, correcting this ticket's own Done-when: **a stalled
  *frontend* is not the backlog case at all** — `app.emit` queues and returns, the tick thread holds
  10 Hz, and a 2 s webview block delivered every queued tick with one column each, widest 1 and
  gaps 0. Two calls the spec left open: the pull's right edge is **the ring's, not the clock's**
  (`Metrics::now_slot`, so it answers as of the last tick, which is the `now_slot` the frontend is
  holding), and a pull **does not advance the tick's cursor** — the overlap is deliberate, since
  drawing a slot twice draws the same thing twice while suppressing it would punch a hole in
  whatever the pull did not cover. The re-pull table lives in `src/bridge.ts` beside
  `pictureNeedsPull`, one predicate that is true for a window-length change and nothing else; the
  DOM half is `b11`'s, because mount and resize both need a canvas. Untested and said so: nothing
  on the phone, the live gap evidence is the startup frame-fill hole rather than a stopped stream,
  and the picture is still not drawn.

- [The spectrogram canvas](issues/b11-the-spectrogram-canvas.md)
  — **Built, and it draws: pink flat, the sweep a straight diagonal, holes black, and a real room's
  rumble stripe and syllable striations in one picture under `npm run tauri dev`.** §14.3's two
  eyeball tests were finally *looked at* rather than asserted, in a throwaway harness kept on
  `prototype/b11-spectrogram-harness` that drives the real component with synthetic columns and a
  faked `invoke`. Spec §11.7's geometry needed no change; **§7.3's "drawn 1:1" did** — 1:1 needs the
  aggregated column count to equal the plot width in device pixels, which no span and screen
  generally arrange, and the rule is *unsmoothed*, of which 1:1 is one case: the pixel budget
  already stops any shrinking, so what is left is magnification, and nearest-neighbour magnification
  duplicates columns rather than blending them. §7.3 and §17 carry it. The shape that follows is
  **two canvases** — a `buckets × 32` offscreen one that is the only thing ever appended to or
  scrolled, and one blit per changed tick into the plot rect, which leaves the chrome standing and
  makes the append rule literally true. Three bugs worth the ticket: the scroll must composite with
  **`copy`**, because a self-`drawImage` under `source-over` leaves the vacated columns opaque and
  **a stopped stream smears its last column across the picture** — §7.3's own failure arrived at
  from a direction the spec did not anticipate; the bucket grid must be anchored to **slot 0**, or
  the partition moves with the right edge and the picture shears by a slot on every re-pull; and a
  pull **drops the columns that arrive while it is in flight** unless they are held and replayed —
  `b10` finding 1's twin at the other end of the same wire, and just as silent. Recorded and not
  fixed: **a hole and a −90 dBFS column are the same colour**, which §16.8 already makes
  near-unreachable. §7.2 is said out loud as one word on the caption line, `−60s · unweighted … now
  … dBFS/band`, because the comparison it guards against is made at the legend. Live: a forced
  reload repainted the full 60 s from `get_spectrogram` over real IPC, and a seeded +101.4 dB offset
  relabelled the legend `11 … 71` with the picture identical. **On the phone it is installed, launched
  and capturing** — `measurement_mode: true`, 48 kHz mono, a clean log and the tick reaching the
  webview at a flat 100 ms — **but the arm's-length verdict is not taken**: there is no way to see
  the screen from here (`idevicescreenshot` is broken on this setup, `devicectl` has no screenshot,
  and `devicectl device orientation` turns out to be simulator-only), and legibility at distance is
  a human judgement anyway. **~6 px per band is still unjudged** and is still the one answer that
  could move §7.1's band count or the ~205 px height.

## Not yet specified

Everything here is **in scope and unanswerable until the app exists**. Most of it is spec
[§13.14](../spl-meter-mvp/spec.md#1314-untested-and-known-to-be-so), whose whole content is
"never tested, and we know it".

- **What [the venue run](issues/b13-the-venue-run.md) reveals.** It is the first time this
  design meets a real *room*, and two settled parameters are still waiting on it: §7.1's
  `−90 … −30 dBFS` colour window (named by `07` as the value most likely to want moving) and
  §11.1's accepted risk that the dominant number is not the number judged against the ceiling.
  **The third is answered**: `b08` read the hero on real speech on the phone and `S` is **live
  rather than twitchy**, which is the first evidence for `09` d5's choice of `S` as the default. A
  desk is not a venue, but this was the phone and a real voice.
- **Arm's-length legibility in a dim venue — the hero half is answered, the band half is not.**
  `b06` established on a desk that `--` reads as a muted absence rather than a redaction bar, and
  `b08` confirmed **on the phone at arm's length** that it reads fine. What remains is the part
  that was always the risk: **~6 px per band, 32 bands over ~205 px**. `b11` has now drawn it,
  looked at it **on the desk** — legible at a normal viewing distance, which is not the question —
  and **put it on the phone**, where it is installed, launched and capturing. **The verdict itself
  is the one Done-when item `b11` left open**, and it is the one thing an agent cannot take: there
  is no working screenshot path off this device, and *legible at arm's length in a dim room* is a
  judgement rather than a measurement. If the answer is "no" it is a correction to the band count or
  the ~205 px height. Neither half has been seen *dim*.
- **`get_spectrogram` in a WKWebView on the phone.** 276 KB of JSON at a 120 s span parses in
  a few ms on a desk and has never run on a device. The fallback — a pixel budget passed into
  the command, aggregating in Rust — costs the single-aggregator property, so it is a trade
  rather than a fix.
- **`"denied"` on iOS, still.** The app has run on the phone across `b07` and `b08` with the
  microphone granted throughout, so §9.4's most useful state remains rendered-but-never-refused by
  a real denial. It is one tap in Settings, and it did not get taken — it now wants doing before
  [the venue run](issues/b13-the-venue-run.md), since a denied microphone at a venue is the one
  failure the reader can actually fix.
- **`"denied"` on macOS.** Per `CLAUDE.md`, `tauri dev` gets microphone access through the
  responsible parent process, so the desk may report `capturing` where the phone reports
  `denied` — spec §9.4's most useful state is the one hardest to exercise where you are
  building it. `b05` confirmed the desk half: there is no `recordPermission` to read off iOS at
  all, so macOS reported `capturing` throughout and **cannot** produce the other two. The
  precedence — permission before stream, because a denied microphone still builds a stream — has
  a unit test standing in for a device.
- **Route-change recovery, now half answered.** `b08` caught a **real** route change on device —
  `StreamInvalidated: Audio route changed`, raised as the route settles a few tens of ms after
  launch — and `b07`'s supervisor rebuilt from it in 70 ms without being asked. So the mechanism is
  demonstrated. What is still unrun is probe 3's actual gesture, a headset plugged and unplugged
  **mid-measurement**, which is a different leg: cpal's iOS backend maps `OldDeviceUnavailable` to
  `DeviceChanged`, and that path has never fired here. It also moves the sample rate, which is the
  half the launch-time event could not exercise. Do it with headphones to hand, watching
  `Capture::builds` and the rate in the log.
- **The 200 ms staleness threshold is reasoned, not measured** (§6.9). iOS drain jitter was
  never characterised — `b05` measured the *tick* at a flat 10.00 Hz, but on macOS, where the
  drain has nothing to be jittery about. If `--` flickers in practice, that number is the dial.
  `b07` added the one measured number either side of it: **a rebuild is ~210 ms of real dead
  air** on the desk and can only be longer on the phone, so a recovery *will* cross the threshold.
  That is not an argument for moving it — the audio genuinely stopped — but it does mean the phone
  will show `--` on every recovery, which the device pass should expect rather than diagnose.
- **Whether spec [§16](../spl-meter-mvp/spec.md#16-what-this-spec-decides-that-no-ticket-decided)'s
  eleven choices survive contact with code.** `realfft`, `rtrb`, the FFT power normalisation
  and eight smaller calls were made by the spec rather than by any ticket, and listed there
  expressly so they could be vetoed in review rather than discovered in code. A ticket that
  finds one of them wrong should say so in its resolution rather than working around it.
  **Ten of the eleven have now met code, and exactly one is vetoed: §16.4.** `b09` found its formula
  is the coherent gain — correct for a bin, **+1.76 dB for the band sum §7.1 actually draws** — and
  replaced it with `2/(N·S2)`, which is what its own stated intent required. §16.2, §16.3, §16.6,
  §16.7, §16.9 and §16.11 stand, as do §16.1, §16.5 and §16.8, all three met by `b09`: `realfft`
  needed no framework entry, the bin-centre rule's quantisation ripple is bounded and predictable,
  and the zero-power gap rule works as written. `b03` found §16.7's *rationale* unreliable while its
  rule holds, which is the one case where reading the justification rather than the clause would have
  produced wrong behaviour. §16.3 is asserted rather than assumed: `Cargo.lock` holds **exactly one
  `objc2` and one `objc2-foundation`**, so the graph did not duplicate. **§16.10 is the last one
  untouched.**

## Out of scope

- **A signed macOS build with the `audio-input` entitlement** (spec §2.1, research `01` open
  risk 8). Genuinely untested, and capture will break silently in a signed bundle if the
  entitlement is wrong — but this effort never produces a signed bundle. macOS is the dev
  loop, `tauri dev` is unsigned, and the target is a phone. It returns if a distributable
  macOS build is ever wanted.
- **Everything the [`spl-meter-mvp` map](../spl-meter-mvp/map.md#out-of-scope) ruled out** —
  logging and session history, export, audio recording, multiple calibration profiles,
  per-frequency calibration curves, App Store distribution, IEC conformance claims, Android.
  Still out, by reference rather than restatement.
