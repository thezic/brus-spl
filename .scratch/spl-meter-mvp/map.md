# SPL Meter MVP

Labels: `wayfinder:map`

## Destination

An **approved spec + architecture** for the MVP of a native SPL meter — enough that
implementation runs as a separate effort with no decisions left open.

The MVP being specified:

- **Meter** — dB(C) default, dB(A) selectable, **dB(Z) as a third mode** (added by ticket `04`;
  it is the filters-off diagnostic path, not a third opinion about loudness). The choice lives on
  a **settings page**, not the main screen. Rolling L_eq over a configurable window
  (default 60 s), a live instantaneous level, and a max hold cleared by a manual reset
  button. No warnings, no automatic resets.
- **Spectrogram** — live frequency display.
- **Calibration** — a single stored broadband offset, set by matching a proper SPL meter
  present at the venue.

Target platform is **iOS** (personal device, free provisioning). **Desktop is the dev
loop**, so every architecture choice must be iOS-viable. Audio capture is **native** —
the Web Audio API is ruled out.

The map is done when the spec is written and approved. **No implementation** happens
inside this map, with one exception: the device spike in
[Prove native mic capture works in a Tauri iOS app on a physical device](issues/02-ios-capture-device-spike.md),
which exists only to prove the architecture is viable.

## Notes

**Domain:** real-time audio DSP (SPL measurement, IEC frequency weighting, FFT) in a
Tauri 2 app — Rust backend, Vue 3 + TypeScript frontend, iOS target.

**Tracker:** local markdown. This repo has no issue tracker configured and no git remote,
so the map is this file and tickets are files under `issues/`. See the local-tracker
conventions in the `setup-matt-pocock-skills` skill for the frontier/claim/resolve
mechanics.

**Skills every session should consult:** `/grilling` and `/domain-modeling` by default;
`/research` for research tickets; `/prototype` for prototype tickets.

**Standing preferences for this effort:**

- **Plan, don't build.** Produce decisions. Ticket `02` is the only sanctioned code.
- **Resist complexity.** Simon has already pushed back once on an over-built proposal
  (percentile metrics, alerting, auto-resets). The bar is the simplest thing that answers
  the question. Do not add warnings, automation, or metrics he hasn't asked for.
- **He makes the call, not the app.** This is an instrument he reads and then decides
  from. It does not decide for him.

**Settled while charting** (context, not open questions):

- Reference app is **Decibel X**; this is a similar tool, not a clone.
- The ceiling is **~70 dB**, and the content is **mostly talks with pauses** — which is
  why a 15-minute window was rejected as too blind.
- **dB(C) is imposed externally**, not Simon's preference. That is precisely why dB(A)
  must be selectable: at 70 dB on speech, C-weighting counts HVAC rumble and room modes
  near full weight, so LCeq can read several dB above what the talk sounds like. Do not
  re-litigate the dB(C) default.
  **Amended by ticket `03`:** charting assumed dB(C)'s low-frequency sensitivity would
  threaten *filter* accuracy. It does not — the filter is accurate to ≤0.005 dB below
  1 kHz. The low-frequency exposure is real but lives in (a) the **microphone**, which
  broadband-offset calibration cannot correct, (b) **f32 arithmetic**, which is why f64 is
  mandated, and (c) **FFT-domain weighting**, which over-reads by up to +6.1 dB on
  infrasonic content. Aim the concern at those three, not at the filter design.
- A **proper SPL meter is available at the venue**, so calibration is a live matching
  gesture and can be crude — but the offset must persist.
- **No Apple Developer account.** Free provisioning, personal device, 7-day reinstall.

## Decisions so far

<!-- one line per resolved ticket: gist + link -->

- [Which native audio capture path works on iOS and desktop?](issues/01-native-audio-capture-path.md)
  — **`cpal` for capture, with `AVAudioSession` configured ourselves from Rust via
  `objc2-avf-audio`.** cpal never sets the session category (Apple's default permits no
  input) and never sets the sample rate, so we do both and read the rate back as ground
  truth. `NSMicrophoneUsageDescription` goes in a new `src-tauri/Info.plist`; signed macOS
  builds also need the `audio-input` entitlement under Hardened Runtime. No official Tauri
  audio plugin exists. Full findings:
  [`research/01-native-audio-capture-path.md`](research/01-native-audio-capture-path.md).
- [Prove native mic capture works in a Tauri iOS app on a physical device](issues/02-ios-capture-device-spike.md)
  — **It captures. `01`'s architecture holds on real hardware.** On an iPhone 14 Pro
  (iOS 26.5.2, cpal 0.18.1): non-zero PCM, **48 000 Hz, mono, f32, 1024-frame buffer**, no
  gaps (469 × 1024 = 480 256 frames exactly), no stream errors, and `AVAudioSession` agrees
  with cpal on the rate. Both preferred-value requests were granted exactly, though Apple
  documents them as preferences — keep reading them back. **48 kHz clears research `03`'s
  ~40 kHz filter minimum.** Free provisioning signs without a developer account, and the
  `Info.plist` merge carries `NSMicrophoneUsageDescription` through. The interruption /
  route-change / `Measurement`-mode probes moved to ticket `11`, which is now unblocked.
  Also worth carrying forward: the iOS target must link `AVFAudio`, `AudioToolbox` and
  `CoreAudio` explicitly via `bundle.iOS.frameworks` — invisible to `cargo check`, since
  nothing links until Xcode does.
- [IEC 61672 A/C weighting filters: definitions and validation](issues/03-iec-weighting-filters.md)
  — **Build, don't adopt: cascaded biquads (3 sections for A, 2 for C), f64 throughout,
  DF2T, coefficients computed at runtime from the actual sample rate, no prewarping.** The
  normative text was obtained (IS 15575-1:2005, identical with IEC 61672-1:2002), giving the
  complete tolerance table and a 34-frequency validation table ready to become a test.
  Reassuringly, the filter tracks the analogue design goal to ≤0.005 dB below 1 kHz at every
  rate — dB(C) low-frequency accuracy is not the hard part. But there is a **~40 kHz minimum
  sample rate**, and f32 state can err by +3.9 dB on infrasonic content. Full findings:
  [`research/03-iec-weighting-filters.md`](research/03-iec-weighting-filters.md).
- [Interruptions and sample gaps: how do they affect measurement validity?](issues/11-interruption-and-gap-handling.md)
  — **Stay on released cpal 0.18.1 and handle interruptions ourselves; on a gap, show the L_eq
  *and* the window coverage** (`33s of 60s`). Measured on device: Siri killed the stream
  permanently after 3 s and **not one error reached Rust**, exactly as research `01` predicted —
  a rolling L_eq would have averaged a 60 s window over 3 s of data and looked entirely plausible.
  New finding: `inputNumberOfChannels` goes 1 → 0 and stays there, so the session is left
  *deactivated* and recovery needs `setActive(true)`, not merely a stream rebuild. Also measured:
  **`Measurement` mode is worth 21 dB** — `Default` applies that much processing gain, which makes
  the mode a precondition for calibration rather than a refinement. Deliberately doing nothing
  about route changes (no refusal on low-rate routes, no calibration invalidation); both are
  accepted risks to be stated in the spec. **Foreground-only measurement, but the idle timer must
  be disabled**, or the screen sleeps mid-talk and measurement stops on its own.
- [Weighting architecture: time-domain biquads or FFT-domain?](issues/04-weighting-architecture.md)
  — **Two pipelines: biquads produce the number, the FFT only draws the spectrogram.** The trade was
  put to Simon rather than assumed, and he took defensibility — FFT-domain C over-reads by
  **+6.1 dB at N=1024** on infrasonic content, and over-reading means false alarms against the
  70 dB ceiling. **Only the selected weighting chain runs**, because A/C/Z turned out to be a
  **settings-page setting** rather than a main-UI toggle: changing it resets the rolling window,
  acceptable for a deliberate act. The FFT taps the **raw** samples ahead of the filter, so `07`
  still owns whether the display is weighted. **Z-weighting is exposed as a third mode** (Simon's
  call, against the recommendation) — it is the bypass path, and the only way to exercise
  calibration with the filters out of the way. DF2T is **hand-rolled**, no `biquad` crate. And `04`
  **declines** research `03`'s sub-40 kHz guard, closing a contradiction with `11` decision 5:
  a log line, nothing in the UI.

## Requirements discovered while charting the terrain

Not decisions and not fog — things the MVP must do that no ticket asked for, surfaced by probing:

- **Disable the idle timer.** From `11`. Without it a long talk outlives the screen and
  measurement dies silently.
- **The rolling L_eq must track coverage, not just energy.** From `11`'s gap decision — the ring
  buffer has to know which slots hold real data. Belongs to ticket `05`.
- **`AVAudioSessionModeMeasurement` is mandatory**, not preferred. From `11`: 21 dB of processing
  gain without it, very likely level-dependent, which would make the single-offset calibration
  model meaningless. Belongs to tickets `06` and `10`.
- **The iOS target must link `AVFAudio`, `AudioToolbox` and `CoreAudio`** via
  `bundle.iOS.frameworks`. From `02`. Invisible to `cargo check`, since nothing links until Xcode
  does.

## Not yet specified

- **Desktop dev-loop fidelity** — how to test the meter with reproducible signals when
  the reference meter is at the venue and not on the desk. Probably generated tones or
  pink noise at a known level, but it depends on what the weighting architecture settles.
  **Mostly answered by `04`:** with biquads settled, the *filter* needs no hardware and no
  reference meter — synthesise tones in Rust at the **exact** one-third-octave frequencies and
  assert against research `03` §4.2 (tight on the two design invariants: C = −3.010 dB at
  31.6228 Hz and 7943.282 Hz). What remains genuinely open is only the **acoustic** end: no
  desk-side way to know a real sound's true SPL, which is a calibration question (`06`), not a
  DSP one.
- **Whether the input path is linear, which decides if a single calibration offset is valid at
  all.** Graduated from ticket `11`: `Measurement` mode is 21 dB quieter than `Default`, and
  processing that large is usually level-dependent. The check is cheap and needs no hardware —
  play the same tone at two levels 20 dB apart in `Measurement` mode and confirm the measured
  delta matches. If it does not, ticket `06`'s single-offset model needs rethinking rather than
  adjusting. **Do this before the spec is written.**
- ~~**How the dB(A)/dB(C) switch presents**~~ — **mostly settled by `04`:** it is a
  **settings-page setting**, not a main-UI toggle, and there are **three** modes not two (Z was
  added). That rules out "show both at once", and `04` decision 2 depends on it — only the
  selected chain runs. What is left for the layout work is narrow: how the *active* mode is
  indicated on the main screen.
- **Whether the weighting filter validation table becomes a unit test in the repo**, and if
  so what test runner gets added — the project has none, and `CLAUDE.md` says to ask before
  adding one. Ticket `03` produced a ready-to-use 34-frequency table, so this is now a
  concrete question rather than a vague one; it needs the implementation effort to exist
  first, so it may belong to that effort rather than this map.
  **The Rust half dissolved in `04`:** hand-rolled biquads with runtime coefficient derivation
  *need* that table as a test, and it is a pure Rust test — `cargo test` plus a `#[cfg(test)]`
  module adds no dependency, no config and no tooling decision, so there is nothing to ask
  about. Only the frontend testing question remains, and nothing so far requires it.
- **Free-provisioning friction** — the 7-day reinstall cycle may become annoying enough
  to need a decision. Signing itself works (`02`), but the expiry has not been hit yet, so the
  re-signing workflow is still unrecorded.
- **Dev-loop networking on an isolated Wi-Fi** — `tauri ios dev` hot reload cannot reach the
  phone on this network (client isolation), so device work uses an embedded `ios build` plus
  `devicectl install`. Tolerable for probing; worth revisiting if iteration on device becomes
  frequent during implementation.
- **A signed macOS build with the `audio-input` entitlement is untested** — research `01`'s
  open risk 8. macOS is dev-only, so low priority, but capture will break silently in a signed
  bundle if the entitlement is wrong. Currently unowned by any ticket.
- ~~**Whether `CLAUDE.md`'s note on `src-tauri/gen/` needs correcting.**~~ **Done** — it was
  just an edit, not a decision. `CLAUDE.md` now distinguishes `gen/schemas` (off-limits)
  from `gen/apple/` (tracked and editable) and records the "keep `gen/apple/` regenerable"
  rule. Also settled while there: **`tauri dev` does get microphone access** on macOS with
  no `Info.plist` and is not terminated — TCC attributes the request to the responsible
  parent process. That was research `01`'s open risk 7, and it means the dev loop needs no
  bundling workaround.
- **Logging and export** — out of scope below, but the most probable *next* effort. If
  whoever imposed the dB(C) limit ever asks for evidence, this is what they want.

## Out of scope

Ruled out of this effort deliberately, all confirmed with Simon while charting:

- **Logging / session history** — a live instrument, not a record.
- **Export / evidence reports** — see fog above; likely the next effort, not this one.
- **Audio recording** — levels only, never the sound itself.
- **Multiple calibration profiles** — one device, one offset.
- **Per-frequency calibration curves** — would genuinely improve dB(C) accuracy on a
  phone mic, but needs a known reference signal and a real procedure. A research project,
  not a feature.
- **App Store and TestFlight distribution** — personal device only.
- **IEC 61672 conformance claims** — a phone mic is not a class 2 instrument, least of
  all at low frequency where C-weighting lives. The filters follow the standard's curves;
  the app claims nothing.
- **Android** — "cross platform" was stated, but iOS is the target and desktop is the dev
  loop. Android is a later effort.
