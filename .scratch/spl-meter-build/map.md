# SPL Meter Build

Labels: `wayfinder:map`

## Destination

A meter **you can trust at a talk on ~2026-08-12**, and which has been read through that talk
at least once.

**Scope flexes; the date does not.** The tickets are cut so the cut line — "we ran out of
days" — always falls cleanly between them, and so that stopping early leaves a real
instrument rather than half of one. The drop order is written down in
[Route](#route) rather than discovered on the last day.

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
set; and `println!` does **not** reach `devicectl … --console`, so on-device diagnostics have
to be on screen.

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

If days run short, drop in this order and no other:

1. **[Interruption and recovery](issues/b07-interruption-and-recovery.md)** — the only Tier 1
   ticket that is droppable. Without it an interruption is still *honest*, because §6.4's
   coverage figure reports the hole; it is simply not *recovered*, and the fix at the venue is
   to restart the app. Honest-but-manual is an acceptable degradation; nothing else in Tier 1
   is.
2. **All of Tier 2**, whole. A partial picture is worse than none — §7.1's colour scale only
   means something with the fixed band layout and the legend behind it.
3. Nothing else. If Tier 1 minus `b07` will not land, the date is the thing that has to move.

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
  remaining risk is launch-time rather than link-time, and it now sits as the first item of
  [the Tier 1 device pass](issues/b08-tier-1-device-pass.md) rather than holding this ticket open.
  Nothing has yet run on the phone.

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

## Not yet specified

Everything here is **in scope and unanswerable until the app exists**. Most of it is spec
[§13.14](../spl-meter-mvp/spec.md#1314-untested-and-known-to-be-so), whose whole content is
"never tested, and we know it".

- **What [the venue run](issues/b13-the-venue-run.md) reveals.** It is the first time this
  design meets a real microphone in a real room, and three settled parameters are explicitly
  waiting on it: §7.1's `−90 … −30 dBFS` colour window (named by `07` as the value most likely
  to want moving), the *feel* of the hero number on real speech, and §11.1's accepted risk
  that the dominant number is not the number judged against the ceiling.
- **Arm's-length legibility in a dim venue.** ~6 px per band, 32 bands over ~205 px, and the
  muted `--` at hero size. Carried unanswered from `07` through `09` through `10`; if the
  answer is "no" it is a correction to the band count or the ~200 px height, so it wants
  answering *before* [the spectrogram canvas](issues/b11-the-spectrogram-canvas.md) rather
  than after. `b06` narrowed one half of it: on a desk the `--` reads as a muted absence rather
  than a redaction bar, and the hero is now as large as the widest state permits at any phone
  width — but *at a desk* is not the question, and the band legibility is untouched.
- **`get_spectrogram` in a WKWebView on the phone.** 276 KB of JSON at a 120 s span parses in
  a few ms on a desk and has never run on a device. The fallback — a pixel budget passed into
  the command, aggregating in Rust — costs the single-aggregator property, so it is a trade
  rather than a fix.
- **`"denied"` on macOS.** Per `CLAUDE.md`, `tauri dev` gets microphone access through the
  responsible parent process, so the desk may report `capturing` where the phone reports
  `denied` — spec §9.4's most useful state is the one hardest to exercise where you are
  building it. `b05` confirmed the desk half: there is no `recordPermission` to read off iOS at
  all, so macOS reported `capturing` throughout and **cannot** produce the other two. The
  precedence — permission before stream, because a denied microphone still builds a stream — has
  a unit test standing in for a device.
- **Route-change recovery.** Probe 3 was never run for want of headphones. Do it with
  headphones to hand.
- **The 200 ms staleness threshold is reasoned, not measured** (§6.9). iOS drain jitter was
  never characterised — `b05` measured the *tick* at a flat 10.00 Hz, but on macOS, where the
  drain has nothing to be jittery about. If `--` flickers in practice, that number is the dial.
- **Whether spec [§16](../spl-meter-mvp/spec.md#16-what-this-spec-decides-that-no-ticket-decided)'s
  eleven choices survive contact with code.** `realfft`, `rtrb`, the FFT power normalisation
  and eight smaller calls were made by the spec rather than by any ticket, and listed there
  expressly so they could be vetoed in review rather than discovered in code. A ticket that
  finds one of them wrong should say so in its resolution rather than working around it.
  **Nothing is vetoed so far.** §16.2, §16.4, §16.6, §16.7, §16.9 and §16.11 have met code and all
  six stand — though `b03` found §16.7's *rationale* unreliable while its rule holds, which is the
  one case where reading the justification rather than the clause would have produced wrong
  behaviour. §16.1, §16.3, §16.5, §16.8 and §16.10 are still untouched.

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
