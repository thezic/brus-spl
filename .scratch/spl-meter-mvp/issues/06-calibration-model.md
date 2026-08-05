# Calibration model, procedure, and persistence

Parent: [SPL Meter MVP](../map.md)
Type: grilling
Status: resolved
Blocked by: —  (was 05, now resolved)

**Inherited from ticket [`05`](05-level-metrics-pipeline.md) — the matching gesture is now
interactive, and this ticket owns more settings than it thought.**

- **The offset is applied post-log, in Rust, to each published dB value** (`05` decision 13).
  Chosen partly for this ticket: as a pre-squaring gain, every nudge of the offset would invalidate
  the accumulated window and force a refill before the next comparison against the reference meter.
  Post-log, `10·log₁₀(Σp²/n) + c` is the identical number and `max(xᵢ + c) = max(xᵢ) + c`, so
  **nothing resets** — nudge the offset and the settled 60 s average and the historical maximum both
  move instantly. Design the gesture around that; it is a live adjustment, not a wait-and-see loop.
- **A 10 s window length exists partly for this** (`05` decision 6). Selectable lengths are
  10/30/60/120 s, and the 10 s option was kept specifically because matching a reference meter wants
  a number steadier than the instantaneous readout but settling in seconds.
- **Persistence now covers four settings, not one:** the calibration offset, the weighting mode
  (C/A/Z), the time weighting (F/S — `05` decision 7 made it selectable), and the window length. All
  four are **Rust-owned** state; the frontend issues commands rather than holding them (`05`
  decision 13, and see ticket [`08`](08-rust-frontend-boundary.md)).
- **Z mode reads through the same post-log offset**, so the bypass path really does exercise the
  whole calibration chain end-to-end, which is what `04` decision 6 promised it would be good for.

**Inherited from ticket [`11`](11-interruption-and-gap-handling.md) — two constraints, and one
open question that may undermine the whole model.**

- **`AVAudioSessionModeMeasurement` is a precondition, not a refinement.** Measured on device:
  the same 440 Hz sine read **−51.7 dBFS** in `Measurement` mode and **−30.6 dBFS** in `Default`
  — 21 dB of processing gain. A stored offset only means something if the mode is fixed.
- **The offset is only valid for the input it was set on.** `11` decided a route change does *not*
  invalidate the calibration, so the procedure must say plainly that plugging in a different
  microphone silently invalidates it. Built-in versus headset mics can differ by tens of dB.
- ~~**Open, and it needs answering before this ticket can be trusted: is the input path linear?**~~
  **Measured 2026-08-05 — it is linear, and this ticket's premise holds.** Same tone at two source
  levels 20 dB apart in `Measurement` mode read **−57.7 dBFS** and **−37.8 dBFS**: a measured delta
  of **19.9 dB against 20 dB expected**. A level-dependent path would have compressed by dB, not by
  tenths, so the 21 dB `Measurement`/`Default` difference from `11` probe 4 is a fixed gain rather
  than AGC-like processing. **A single broadband offset is defensible.**

  The tight agreement also rules out its own most likely artefact. Room noise adds energy to the
  *quiet* point, shrinking the measured delta — the same direction as compression — so both error
  sources are jointly capped at 0.1 dB. For noise to account for even that, it would have to sit
  ~16 dB below −57.7 dBFS (≈ −74 dBFS); had it been near `02`'s idle figure of −59.6 dBFS the delta
  would have come out near 24 dB, not 19.9. Whatever the floor was, it was low enough not to matter.

  **Limitation worth carrying rather than forgetting:** the test brackets −57.7 to −37.8 dBFS, and
  AGC-style processing engages near the *top* of a range, not the bottom. `02` measured an idle room
  at −59.6 rms / −33.3 peak, so a venue talk plausibly sits at or above the highest tested level —
  the untested region is exactly where limiting would live if any survived `Measurement` mode. One
  further run 20 dB higher would bracket the real operating point. Not blocking: linearity over a
  20 dB span is what this ticket needed, and Simon noted the equipment on hand is not precise enough
  to chase tenths regardless.

**Inherited from ticket [`04`](04-weighting-architecture.md) — one thing that helps this ticket.**
`04` exposed **dB(Z)** as a third mode, and its stated purpose is exactly this ticket's problem: Z
is the **bypass path**, RMS straight off the raw samples with no filter in the way. That gives the
calibration procedure — and the unrun linearity check above — a way to exercise the input path
*without* the weighting filters as a confounder. Worth using rather than reasoning around.

Also from `04`: there is **no DC blocker** in the chain, so a Z reading includes the microphone's DC
bias. Irrelevant for A and C (both are high-passes) but it is a floor on how quiet a Z measurement
can read, and worth knowing before treating a Z number as ground truth.

## Question

A single broadband offset in dB, set by matching a proper SPL meter that is present at the
venue. The details that matter:

- **Applied pre- or post-weighting?** A broadband offset is mathematically the same either
  way for a pure gain, but where it sits in the pipeline changes what's testable and what
  the stored number means.
- **What the matching gesture is.** Both meters pointed at the same steady sound; does
  Simon type the reference reading, nudge with buttons, or does the app compute the delta
  from an entered target?
- **What happens when the input device or mic changes** — a different iPhone, a headset
  plugged in, a route change mid-session. The offset is device-specific; does it silently
  become wrong?
- **Persistence.** Where the offset is stored, alongside the other settings (weighting
  choice, window length). Must survive restarts.

Known limitation, already ruled out of scope: a single broadband offset cannot correct
frequency-response error, and on a phone mic that error is worst in the low end — where
dB(C) lives. The offset makes the number honest on average, not honest per band. Make sure
the spec says so rather than implying more accuracy than exists.

---

## Answer

**One offset for all three modes, set by typing what the proper meter reads while the app sums a
fixed 10 s slice of the ring.** Uncalibrated is a real state rather than a zero — the numbers are
shown labelled `dBFS`, a correctly-named different quantity instead of a wrong SPL. Persistence is
one JSON file written from Rust, needing no plugin, no npm package and no capability entry.

The scale, so the numbers below are concrete: `02` measured an idle room at **−59.6 dBFS** in
`Measurement` mode, and a room like that is plausibly 35–40 dB SPL, which puts the offset on the
order of **+100 dB**. It is not a small trim, and that single fact rules out a nudge-only gesture
(1 000 taps at 0.1 dB from cold). A 70 dB talk would then read about −30 dBFS — worth noting, since
that sits ~8 dB *above* the top of the span the linearity check bracketed.

### Decisions (2026-08-05, with Simon)

**1. One offset, shared by all three weighting modes. Set it in dB(C).**

The offset converts dBFS → dB SPL, and that conversion is a property of the **microphone sensitivity
and input gain** — hardware, not weighting. `04` renormalised all three chains digitally to 0 dB at
1 kHz, so they already share an absolute anchor: a scale established in one mode *is* the scale in
the others.

Per-mode offsets got a fair hearing, because the phone mic's response is not flat, so the broadband
offset one would actually *measure* differs by mode on the same sound — C counts the low end where a
phone mic is worst. Rejected on two grounds. It needs three calibration gestures against a reference
meter that probably cannot show all three modes, Z least of all. And it would **break Z's stated
purpose**: `04` decision 6 exposed Z precisely so the chain could be exercised with the filters out
of the way, and per-mode offsets would force Z and C into agreement on the calibration spectrum,
destroying the very comparison Z exists to provide.

Set it in **dB(C)** — the imposed mode and the default — so the residual response error is smallest
where the ceiling is actually judged. More generally: match whatever weighting the reference meter
is showing.

**2. Type the reference reading. Plus a ±0.1 dB trim, and the offset itself displayed and directly
editable.**

Three entry affordances, and each earns its place differently.

- **Typing the reference value is primary.** You read 68.3 off the proper meter and type 68.3; the
  app stores `68.3 − (current 10 s slice)`. No mental arithmetic, one step from cold, and identical
  whether the offset is unset or already set.
- **±0.1 dB trim** for splitting the difference while watching both meters over a few seconds.
  Recorded at 0.1 dB and no coarser pair: the reference meter's own resolution is 0.1 dB and a
  class-2 instrument is ±1.5 dB, so trimming more coarsely is theatre and more finely is measuring
  nothing.
- **The offset is displayed as a number and directly editable**, and this exists for a reason
  specific to this project. Free provisioning expires every 7 days, so the app is re-signed and
  reinstalled weekly. Installing over the top usually preserves the data container — but
  delete-then-install does not, nor does switching machines or regenerating the Xcode project. With
  the offset visible, recovery is a sticky note and a retype; with it hidden, losing the container
  means being uncalibrated until you are next standing beside the proper meter. Large asymmetry,
  one label.

**Nudge-only was ruled out by arithmetic**, not preference — see the +100 dB magnitude above.

**3. The offset is computed against a fixed 10 s slice of the ring, independent of the display's
window setting.**

This is a payoff from `05` decision 5, which allocated the ring at the maximum window length and made
the length setting a pure re-slice. So the calibration view sums the **last 100 slots** while the main
meter stays on 60 s: no second accumulator, no temporarily overriding a user setting, nothing to
restore afterwards.

Better than both alternatives. Using **whatever window is selected** means calibrating against a
120 s average if that is what was left on — two minutes to settle — and is identical to this option
when the window is already 10 s. Using the **instantaneous reading** matches what a hand-held meter
typically displays, but then both numbers are moving and you are aligning two bouncing targets by
eye, and the result would silently depend on whether F or S is selected.

**The procedure must require a steady sound**, and that is a line in the procedure rather than code:
the reference meter's quantity is unknown, so if it shows an F-weighted instantaneous level and we
show a 10 s L_eq, the two agree only when the sound is not changing.

Arithmetic, not policy: at **zero** coverage in that slice the L_eq is undefined, so the compute step
cannot run. Below that, coverage is displayed and Simon judges — no refusal, consistent with `05`
decision 11 and `11` decision 3.

**4. Uncalibrated is a real state. The numbers are shown labelled `dBFS`.**

The offset is `Option<f64>`, not a default of 0 — so "never calibrated" is a state `08` and `09` can
distinguish rather than a magic zero. With `None`, the three numbers display as raw values labelled
**dBFS**: not a wrong SPL but a correctly-named *different quantity*, with the unit carrying the
honesty. It also keeps the instrument useful before there is any absolute scale — you can confirm
the mic is capturing, watch the room, and exercise Z mode, none of which needs one.

`--` was the consistency play, since `05` decision 11 refuses to publish untrustworthy numbers. But
an uncalibrated meter is not *broken*, it is un-scaled, and blanking it means you cannot tell capture
works at all until ticket `07`'s spectrogram exists.

**A shipped default of ~+100 dB was rejected firmly.** It would read roughly right out of the box and
make calibration a refinement — at the cost of showing an authoritative-looking number wrong by an
unknown amount, which is the precise failure mode this map has organised itself against since `11`
decision 3.

**5. Persistence is one JSON file written from Rust.**

`serde` and `serde_json` are **already** Cargo dependencies, and `app.path().app_config_dir()` is
core Tauri called from Rust rather than over the IPC bridge — so this needs **no new dependency, no
npm package, and no capability entry**. It also matches the ownership model `05` decision 13 and `08`
settled: all four settings live in Rust and the frontend issues commands.

`tauri-plugin-store` is the wrong shape twice. It is JS-first, so the frontend would own
authoritative state, inverting the decision above; and it is the full four-step ceremony `CLAUDE.md`
warns about — Cargo dependency, `.plugin()` registration, npm package, *and* a capability entry —
where omitting the last produces a runtime "not allowed" rather than a build failure. `localStorage`
fails the same ownership test and would leave every reading invalid until the offset had been pushed
down at startup.

Three consequences recorded with it:

- **Write through on every change, not on exit.** Probe 2b showed the app dies on backgrounding,
  possibly without running shutdown code, so a deferred write is a lost setting. Changes are
  deliberate settings acts, and the file is four values.
- **One file, all four settings** — offset, weighting, time weighting, window length. One write, one
  read at startup.
- **Missing or corrupt falls back to defaults** (`None`, C, F, 60 s) with a log line and nothing in
  the UI, consistent with `04` decision 5.

**6. No metadata. Four values, nothing about the calibration itself.**

No date, no record of the reference value, no mode. The reasoning is sharper than "resist
complexity": **the offset does not go stale with time, it goes stale with a hardware change.** `11`
decision 6 established that the real invalidator is a different microphone, accepted deliberately
with no detection. An offset set six months ago on the built-in mic is exactly as valid as one set
this morning, while one set ten seconds ago is wrong the moment a headset goes in. A displayed date
would direct attention to elapsed time — the wrong variable — while detecting nothing.

And the question a date would be *used* for, "did I retype this after the last reinstall?", is
already answered by decision 2 displaying the offset. Unset versus ~+100 dB is unmistakable.

### The procedure

What the spec should say, in order:

1. Set the weighting to **dB(C)** on the settings page — or to whatever the reference meter is
   showing.
2. Put the phone and the reference meter in the same place, pointed at the same **steady** sound.
   Steady matters: the two instruments may not be reporting the same quantity, and only a
   non-changing sound makes them comparable.
3. Wait for the calibration reading to fill its 10 s slice. Coverage is displayed; judge it.
4. Read the reference meter and type its value. The offset is stored immediately, and every displayed
   number — including the settled 60 s average and the max hold — moves at once.
5. Trim ±0.1 dB if the two disagree slightly.
6. **Write the offset number down.** It is the recovery path if the app's data container is ever
   lost, and it costs nothing now.

### Recorded, not debated — forced by upstream tickets, or too cheap to argue about

- **The offset is applied post-log, in Rust** (`05` decision 13). This is what makes steps 4 and 5
  above interactive rather than a wait-and-see loop: `10·log₁₀(Σp²/n) + c` is the identical number to
  a pre-squaring gain, and `max(xᵢ + c) = max(xᵢ) + c`, so nothing resets and no window refills.
- **`AVAudioSessionModeMeasurement` is a precondition, not a refinement** (`11` probe 4). A stored
  offset means nothing if the mode is not fixed — the mode is worth 21 dB.
- **Read the session mode back and log a mismatch.** Not asked for; added on the same reasoning `01`
  and `02` applied to the sample rate, where preferred values are requests rather than guarantees. If
  `Measurement` ever silently fails to apply, the stored offset is wrong by ~21 dB, which is worth a
  log line. No UI, consistent with `04` decision 5.
- **The typed reference value is bounds-checked to a plausible SPL range** (~0–140 dB). A
  fat-fingered `683` for `68.3` would otherwise store a ~600 dB offset. This is input validation on a
  physically-bounded quantity, not a measurement guard of the kind `04` decision 5 and `11` decision
  5 refused; the derived offset itself stays unclamped.
- **The offset is only valid for the input it was set on** (`11` decision 6). No detection, no
  staleness flag. The procedure and the spec say it plainly; nothing at runtime enforces it.
- **Z reads through the same offset**, so the bypass path exercises the whole calibration chain
  end-to-end, which is what `04` decision 6 promised. Remember there is no DC blocker, so a Z reading
  carries the mic's DC bias as a floor.

### Residual risks, stated plainly

- **The offset is only as good as the reference meter.** A class-2 instrument is ±1.5 dB and the
  offset inherits that error wholesale, while the app will display 0.1 dB. The spec should say so —
  the precision of the display is not the accuracy of the reading.
- **A broadband offset cannot correct frequency response**, and on a phone mic the error is worst in
  the low end, which is where dB(C) lives. Already in scope as a known limitation; unchanged by
  anything here.
- **A changed microphone silently invalidates it**, by `11` decision 6's deliberate choice. The
  largest single risk on this ticket, and it lives in the spec rather than in code.
- **Linearity is verified only across −57.7 to −37.8 dBFS**, while the operating point plausibly
  sits ~8 dB above the top of that span, and limiting engages near the top of a range. One further
  run 20 dB up would bracket it.

### What this ticket hands downstream

- **Ticket [`08`](08-rust-frontend-boundary.md) — three additions to the contract.** The calibration
  view needs the **fixed 10 s slice reading and its coverage** published alongside the display's own
  L_eq (decision 3), so the meter payload is no longer the only meter traffic — though it is needed
  only while that view is open, which is a design choice `08` owns. It needs an **uncalibrated flag**
  (`Option<f64>`, decision 4) so the frontend knows to label `dBFS` rather than `dB`. And two
  commands: set-from-reference (taking the typed value) and set-offset-directly.
- **Ticket [`09`](09-screen-layout.md) — a calibration surface it did not have.** Numeric entry for
  the reference value, the live 10 s reading with its coverage, ±0.1 dB trim buttons, and **the offset
  displayed as a number** for writing down. Plus a unit label that switches between `dBFS` and `dB`
  on the main screen, since decision 4 makes uncalibrated a visible state.
- **Ticket [`10`](10-write-the-spec.md) — the procedure verbatim, and three limitations.** The
  six-step procedure above; the class-2 reference-meter caveat; the reinstall recovery note (write the
  offset down); and that `Measurement` mode is a precondition rather than a refinement. The
  broadband-offset and changed-microphone limitations were already on `10`'s list from `11`.
