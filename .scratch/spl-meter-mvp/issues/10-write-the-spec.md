# Write the spec

Parent: [SPL Meter MVP](../map.md)
Type: task
Status: open
Blocked by: 08, 09  (01, 02, 03, 04, 05, 06, 07, 11 now resolved)

## Question

Nothing left to decide — assemble the resolved tickets into the spec that is this map's
destination, and get Simon's approval on it.

The spec must be complete enough that an implementation session needs no further
decisions: audio backend and permissions, weighting filter approach and coefficients,
the three level metrics and how they're computed, the calibration model and persistence,
spectrogram parameters, the Rust↔frontend contract, and the screen layout.

Publish it at `.scratch/spl-meter-mvp/spec.md` per the local-tracker convention.

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
