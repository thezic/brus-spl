# Write the spec

Parent: [SPL Meter MVP](../map.md)
Type: task
Status: resolved — [`../spec.md`](../../../docs/spec.md) written and **approved** (2026-08-05, no notes).
**Every other ticket on the map is resolved.**
Blocked by: —  (01, 02, 03, 04, 05, 06, 07, 08, 09, 11 all resolved)

## Question

Nothing left to decide — assemble the resolved tickets into the spec that is this map's
destination, and get Simon's approval on it.

The spec must be complete enough that an implementation session needs no further
decisions: audio backend and permissions, weighting filter approach and coefficients,
the three level metrics and how they're computed, the calibration model and persistence,
spectrogram parameters, the Rust↔frontend contract, and the screen layout.

Publish it at `spec.md` beside this map, per the local-tracker convention. *(It moved to
[`docs/spec.md`](../../../docs/spec.md) in `#17`, once everything around it had closed and it had
not.)*

State the accuracy limitations plainly — broadband-offset calibration only, phone mic, no
IEC 61672 conformance claim — so the spec doesn't imply an instrument this isn't.

**Two further limitations to state, inherited from ticket
[`11`](11-interruption-and-gap-handling.md).** Both are deliberately accepted risks where the app
does nothing at runtime, so the spec is the *only* place the honesty lives:

- **A route below ~40 kHz is measured normally and shown without any indication.** A Bluetooth
  mic route typically supplies 8–16 kHz, well under the weighting filters' floor, so the reading
  there is out of tolerance rather than merely imprecise.
- **A changed microphone silently invalidates the calibration offset.** No detection, no staleness
  flag. Built-in and headset mics can differ by tens of dB.

Also record that measurement is **foreground-only with the idle timer disabled** (capture does not
survive backgrounding), and that the L_eq display carries **window coverage** alongside the level.

**Inherited from ticket [`09`](09-screen-layout.md) — the layout, plus four things that need
*stating* rather than drawing, and two corrections to carry.**

- **The screen leads with the live level, not the rolling L_eq**, and the spec must say so together
  with the cost: **the number that dominates is not the number judged against 70 dB.** A reader will
  otherwise assume the big number is the one that matters. `09` decision 2 inverted `09`'s own
  premise, on the measured ground that a 60 s L_eq moves 0.0 dB in 8 s. The L_eq (with coverage) and
  the max hold are always visible beside it; nothing is behind a tap.
- **Defaults: `C` / `S` / 60 s / 0.1 dB display resolution.** `S` needs its reason recorded, because
  `05` decision 7 recommended F-only: at F the hero moves 1.72 dB per 100 ms tick, further than any
  rounding step, so display resolution cannot calm it. And 0.1 dB needs its reason too — coarsening
  is one setting for all three numbers and would spend a tenth on the ceiling-judged L_eq.
- **The input-state line's exact wording and its two-part shape** — `No microphone access` over
  `Settings ▸ Privacy & Security ▸ Microphone` for `denied`, label alone for `unavailable`. The
  two-part shape is what keeps it inside the no-warnings rule; one string made the two states look
  like the same kind of message.
- **`--` and `dBFS` are typographic states, not merely strings.** A spec that says "show `--`" and
  stops will get a redaction bar: at hero size two dashes render as solid blocks. Muted and much
  smaller is what makes them read as *no reading*. Same for sizing the primary number for its widest
  state, which is the **uncalibrated** one (`−108.4`, not `66.7`).

Two corrections `09` made that this spec must carry rather than repeat the originals: a **zero-power
block is a gap slot, not a covered one** (so a denied mic reads `0s of 60s`, not `60s of 60s` beside
`--`), and `07`'s **legend gutter is 58 px**. Also worth a line: settings *and* calibration live
behind one sheet reached from `⋯`, and Reset is inside it and confirmed.

**Inherited from ticket [`06`](06-calibration-model.md).**

- **Reproduce `06`'s six-step procedure verbatim.** Set dB(C), both instruments on the same steady
  sound, let the 10 s slice fill, type the reference reading, trim ±0.1 dB, **write the offset down**.
  The "steady sound" requirement is load-bearing rather than advisory: the reference meter's quantity
  is unknown, and only a non-changing sound makes the two comparable.
- **One offset serves all three weighting modes**, because it converts dBFS → dB SPL, a property of the
  microphone rather than of the weighting. Worth stating so per-mode calibration doesn't look like an
  omission.
- **Uncalibrated is a designed state, not an error.** The offset is `Option<f64>` and the app shows raw
  values labelled `dBFS` until it is set. State why: a shipped default would be an authoritative-looking
  number wrong by an unknown amount, and blanking the meter would hide that capture works.
- **Three limitations to add.** (a) **The offset is only as good as the reference meter** — a class-2
  instrument is ±1.5 dB and the offset inherits that wholesale, while the display shows 0.1 dB; the
  precision of the display is not the accuracy of the reading. (b) **Free provisioning reinstalls can
  lose the stored offset**, which is why the app displays it — the recovery path is a written-down
  number, and the spec should tell the reader to keep one. (c) **`AVAudioSessionModeMeasurement` is a
  precondition, not a refinement** — 21 dB of processing gain without it, so a stored offset means
  nothing if the mode is not fixed. The mode is read back and a mismatch logged, with nothing in the UI.

**Inherited from ticket [`08`](08-rust-frontend-boundary.md) — the Rust↔frontend contract, plus
three things to state and one table worth reproducing.**

- **Reproduce the wire contract verbatim** — the `tick` event's payload and the eight commands.
  Notable properties to state rather than leave for a reader to infer: every dB value crossing the
  bridge is **already calibrated** (`05` decision 13), the frontend holds **no authoritative state at
  all** (settings and the unit ride in every tick), the tick is a **timer** so it publishes with no
  audio at all, and each column carries an **absolute slot index** so a gap is an index jump rather
  than a marker.
- **Reset does not clear the spectrogram**, only the window and the max hold. State the reason or it
  reads as a bug: 60 s of rumble stripe is unrecoverable while the number refills honestly, and `07`
  decision 7 had already made the picture a different quantity from the number. The consequence to
  write down plainly: shortly after a Reset the number describes this talk and the picture describes
  the last 60 s of the room. Both are labelled; neither is wrong.
- **The input state is the one place the app speaks up.** `08` decision 8 crosses
  `capturing` | `denied` | `unavailable`, because a denied microphone otherwise reads as `--` beside
  `0s of 60s` forever. State the distinction that keeps this inside the no-warnings rule: it fires on
  a **permanent condition fixable only outside the app**, never on a measurement. And note the
  desktop caveat — per `CLAUDE.md`, `tauri dev` gets microphone access via the responsible parent
  process, so macOS may report `capturing` where iOS reports `denied`.
- **Reproduce `08`'s re-pull table**; one row is counter-intuitive enough to be worth the space.
  Calibrating changes every number on screen and **no pixel** of the picture, because `07` decision 9
  shifts the colour window by the offset and the band values shift with it. Only the legend relabels.
- **`05`'s reset/clear table gains a fourth column and it is entirely empty** — nothing clears the
  spectrogram's column ring. Worth stating precisely because it is all dashes.

**Inherited from ticket [`07`](07-spectrogram-form.md) — the spectrogram's parameters, and one thing
it must not be read as claiming.**

- **State the parameters plainly:** a scrolling spectrogram, horizontal with time flowing right→left,
  drawn on **32 fixed one-third-octave bands from 12.5 Hz to 16 kHz**, one column per 100 ms, **N=8192**
  Hann, and a **span that follows the L_eq window** so the picture is what is inside the number.
- **The display is always unweighted, in every meter mode.** This is the one that must be said out loud:
  the picture and the number are deliberately different quantities, so a reader who compares them
  band-by-band against a dB(A) or dB(C) reading will conclude the app is inconsistent. State the reason —
  A-weighting the display makes the rumble stripe vanish, and dB(A) mode is exactly when you want to know
  the rumble is there.
- **A band colour is not a calibrated band SPL.** The single broadband offset scales the picture as
  honestly as it scales the numbers and no more, and a phone mic's frequency response is worst exactly
  where dB(C) lives. Same limitation as `06`'s, now with a per-band face.
- **Two smaller limitations:** the **12.5 Hz band is interpolated** at N=8192 (it is narrower than one
  bin), so the lowest band on the picture is the one band the picture cannot honestly draw; and at a
  **120 s span** syllable structure is compressed into texture, which is a consequence of the span
  following the window rather than a defect.
- **The picture reports its own gaps as holes** — background, not a low level — for the same reason the
  L_eq shows `--` at zero coverage.

**Inherited from ticket [`05`](05-level-metrics-pipeline.md).**

- **There are four settings, not three:** weighting (C/A/Z), **time weighting (F/S)** — `05`
  decision 7 made it selectable, against the recommendation — window length (10/30/60/120 s, default
  60), and the calibration offset. All four are Rust-owned and all four persist.
- **Reproduce `05`'s reset/clear table verbatim.** Three pieces of state (window, max hold, filter
  state) against six events, and several rows read as bugs if unexplained — notably that an F/S
  change clears the max hold but not the window, that a rate change clears neither, and that
  changing the calibration offset clears nothing at all.
- **Coverage is shown always, not only when degraded.** `33s of 60s`, and `60s of 60s` when full. A
  figure that appears only when something is wrong is a warning, which charting ruled out; always-on
  is what makes the number trustworthy. Worth stating as a deliberate choice.
- **The L_eq averages real data only**, so the displayed number means "the L_eq of the seconds I
  actually have" and coverage is what qualifies it. State this — the alternative (folding gaps in as
  silence) would read low by 2.6 dB for a 27 s hole in a 60 s window, and a reader who assumes it is
  the one being done will misread every gapped measurement.
- **Two states where the app deliberately shows nothing:** the instantaneous readout after 200 ms
  with no audio, and the L_eq at zero coverage. Both are the instrument reporting its own state,
  which is the same principle as `11` decision 3.
- **One limitation to add to the accuracy section:** there is no coverage floor, so
  `68.2 dB · 1s of 60s` will display — honest, but it is a one-second average wearing a
  sixty-second label, and only the coverage figure says so.

**Inherited from ticket [`04`](04-weighting-architecture.md).**

- **There are three weighting modes, not two.** dB(C) default, dB(A), and **dB(Z)** — chosen on a
  settings page. The spec must say what Z is *for*: it is the instrument with its filters switched
  off, a diagnostic for the calibration chain, **not** a third opinion about loudness. A Z reading
  compared against the imposed dB(C) ceiling is meaningless, and nothing at runtime prevents that
  comparison, so the spec is the only place it gets said.
- **Changing the weighting mode resets the rolling window.** Deliberate, and a consequence of
  running only the selected chain. Worth stating so it doesn't read as a bug.
- **The spec's accuracy section can be specific rather than vague about the filter.** Below 1 kHz
  the digital filter tracks the standard's analogue design goal to ≤0.005 dB (A) and ≤0.0005 dB
  (C); all discretisation error lives above 4 kHz and stays inside the class-1 design band at
  44.1 and 48 kHz. **The microphone, not the filter, is the binding error source** — which is the
  honest framing for why no conformance is claimed.

---

## Answer

**Written and approved: [`../spec.md`](../../../docs/spec.md), 18 sections, 1 495 lines.** Approved in
Plannotator on 2026-08-05 with **no notes** — so nothing here is a revision of what was put up.

Every item this ticket listed is in it, and every decision cites the ticket that made it
(`04 d1`-style) so the spec is a destination rather than a replacement for the reasoning. The
structure: what the app is and the three principles it obeys · platform, build and permissions ·
capture · interruptions and gaps · weighting filters · level metrics · spectrogram · calibration ·
the Rust↔frontend contract · settings and persistence · screen layout · module layout ·
**accuracy and limitations, 14 subsections** · validation and tests · open questions ·
[§16](../../../docs/spec.md#16-what-this-spec-decides-that-no-ticket-decided) ·
[§17](../../../docs/spec.md#17-corrections-this-spec-carries) · provenance.

### What assembling it turned up

Three findings, all of them **gaps or contradictions between resolved tickets** that only surfaced
when the tickets were put side by side. They are the reason this was not a pure transcription job.

**1. Two upstream texts were stale, and one was a miscount.** Recorded in the spec's §17 so a
reader who goes back to a ticket is not misled:

- **`06` decision 5 lists the config-file defaults as `(None, C, F, 60 s)`.** `09` decision 5 made
  **S** the default time weighting afterwards, so the persisted default in `06` is wrong. The spec
  says `(None, C, S, 60 s)`.
- **`08` decision 9 says "five types and eight commands" while its own contract block lists
  seven.** Seven is right: there is deliberately no `get_settings` (`08` says so itself — the first
  tick arrives ≤100 ms after the listener registers) and no clear-calibration command (`06` d2
  makes the offset directly editable). This ticket's own brief inherited the miscount and asked for
  "the eight commands".
- **`09`'s ASCII sketch labels its legend `60 / 20`**, a 40 dB span, where `07` decision 9 fixes the
  window at **60 dB** ≈ 10 → 70 dB SPL. `07` is the parameter decision and wins; the sketch is
  illustrative.

**2. `09`'s zero-power correction had a second and a third home, and neither ticket found them.**
`09` corrected `05` decision 11 by treating a zero-power block as a **gap slot** rather than a
covered one, so a denied microphone reads `0s of 60s`. But the same exact-zero blocks reach two
other places, and in both the original reasoning fails for the same reason:

- **The instantaneous readout.** `05` d11's `--` rule fires on *no block for 200 ms*. Exact-zero
  blocks **do** arrive, so the rule never fires, the smoother converges on zero, and
  `10·log₁₀(0) = −∞` reaches `serde_json` — which `09` explicitly claimed its correction had
  prevented. So NOW must publish `null` when the smoothed mean square is **not positive**, in
  addition to the staleness rule.
- **The spectrogram.** A zero-power slot that still produces a column draws at the bottom of `07`
  d9's fixed colour window — which is a *very quiet room*, i.e. precisely the "dead stream reads as
  a peaceful room" failure `07` d11 exists to prevent. So a gap column is a slot with no samples
  **or** zero total power.

Both are in the spec as derived consequences rather than new choices, but they were not written
down anywhere before.

**3. Two crates and one normalisation were nobody's decision.** The map's "no decisions left open"
bar could not be met without them, so the spec makes them and flags them for veto in §16: `realfft`
for the FFT and `rtrb` for the SPSC queue (`05` d12 requires a bounded lock-free SPSC queue but
names no crate, and hand-rolling one correctly is not the 15-line job the biquad recursion is), plus
the **FFT power normalisation** — `07` measured everything through its prototype's own and never
wrote it down, yet `07` d9's colour window is quoted in dBFS, so the window is meaningless without
it. The one chosen makes a full-scale sine read −3.01 dBFS in its band, which is **the same dBFS
convention as the meter**, and that is what lets one calibration offset shift both the numbers and
the picture.

§16 lists eleven such choices in total; the other eight are mechanical (band assignment rule, the
tick's thread, channel 0 on multi-channel streams, the `unavailable` label, the config file name).

### What this ticket hands downstream

- **The map is done.** Its destination was "an approved spec + architecture … enough that
  implementation runs as a separate effort with no decisions left open", and that now exists.
- **The implementation effort inherits four open questions, none blocking** — spec §15. The one
  worth doing first is the cheapest: **whether ~6 px per band reads at arm's length in a dim
  venue**, since the build is already installed on the phone. If the answer is no it is a
  correction to `07` d3's band count or `07`/`09`'s ~200 px height.
- **The spike is still in the tree and is now due for deletion.** `11` said to delete it on its own
  resolution and `CLAUDE.md` says to delete it once the spec is written; both conditions are met.
  Deliberately **not** done here: removing `App.vue`'s harness means writing what replaces it,
  which is the first act of implementation rather than the last act of this map. Spec §12 lists
  exactly what goes and what must survive.
