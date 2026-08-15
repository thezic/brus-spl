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

> **✅ Done — 2026-08-05.**
>
> **The spec is written and approved: [`spec.md`](../../docs/spec.md).** Every ticket on this map is
> resolved, and the destination exists. Implementation runs as a separate effort and should
> start from the spec, not from this map — the map's value now is the *reasoning* behind each
> decision, which the spec cites but does not reproduce.
>
> **That effort is now charted: [the `spl-meter-build` map](../spl-meter-build/map.md)**
> (2026-08-05). It carries execution rather than decisions, and its destination is a meter
> trustworthy enough to read a real talk — so it flexes scope against a fixed date and states
> its drop order up front. It cites this spec by section throughout; it does not re-open
> anything decided here. Its `b01`–`b13` numbering is deliberately distinct from this map's
> `01`–`11`.
>
> Four things the spec deliberately leaves open, none of them blocking: its
> [§15](../../docs/spec.md#15-open-questions-this-spec-does-not-close). The cheapest and most useful is
> **whether ~6 px per band reads at arm's length in a dim venue** — the build is already on the
> phone.
>
> **The capture spike is still in the tree and is now due for deletion** (`spike.rs`,
> `bin/spike.rs`, the `run_capture_spike` command, `App.vue`'s harness). Left in deliberately:
> removing the harness means writing what replaces it, which belongs to the implementation
> effort. Spec [§12](../../docs/spec.md#12-suggested-module-layout) lists what goes and what must survive.

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
- [Level metrics pipeline: rolling L_eq, instantaneous level, max hold](issues/05-level-metrics-pipeline.md)
  — **A ring of 100 ms energy slots advanced by the monotonic clock, not by arriving samples — which
  makes gap accounting a consequence of timekeeping rather than a feature.** The L_eq averages
  **real data only**, so the first 60 s after launch and a 27 s hole are the same case, both read
  `33s of 60s`; probe 2 in `11` showed why a sample-driven ring would instead have reported full
  coverage over a dead stream. The ring is allocated at the longest window so the length setting
  (10/30/60/120 s, default 60) is a **re-slice, not a reset**. Instantaneous level is exponentially
  time-weighted with **F/S selectable** (Simon's call, against the recommendation), and max hold is
  the maximum *of that* — L_CFmax, compared at block rate — which means **two** settings now change
  what the max means, so both must clear it. One Reset button clears max hold *and* window ("start
  measuring this talk", so a talk isn't judged through the previous one's applause). Where the design
  refuses to guess: `--` after 200 ms of no audio rather than decaying to a fake silence, and a
  queue overflow degrades into lost coverage rather than a wrong number. The calibration offset lands
  **post-log in Rust**, which makes `06`'s matching gesture interactive — nudge it and the settled
  60 s average moves with no reset. Also **corrects `04`** twice: the max hold clears on a weighting
  change too, and "mark the transition as gap slots" is now zero code.

- [Calibration model, procedure, and persistence](issues/06-calibration-model.md)
  — **One offset for all three modes, set by typing what the proper meter reads while the app sums a
  fixed 10 s slice of the ring.** The offset converts dBFS → dB SPL, which is a property of the
  *microphone*, not the weighting — and keeping it single is what preserves Z's diagnostic value, since
  per-mode offsets would force Z and C to agree. Scale established: it is on the order of **+100 dB**,
  which by itself rules out a nudge-only gesture. **Uncalibrated is a real state, not a zero:** the
  offset is `Option<f64>` and the numbers show as **`dBFS`** until it is set — a correctly-named
  different quantity rather than a fabricated default, which would have been an authoritative-looking
  number wrong by an unknown amount. The 10 s match slice is **free**, a direct payoff from `05`
  decision 5 making window length a re-slice, so the main 60 s meter is never disturbed. Persistence is
  **one JSON file written from Rust** — `serde_json` is already a dependency and `app_config_dir()`
  needs no plugin, npm package or capability entry — written through on change, because probe 2b showed
  the app can die without running shutdown code. **No metadata:** the offset goes stale with a hardware
  change, not with time, so a date would point at the wrong variable while detecting nothing. And the
  offset is **displayed for writing down**, because free provisioning's weekly reinstall can take the
  data container with it.

- [Spectrogram: parameters and visual form](issues/07-spectrogram-form.md)
  — **A scrolling spectrogram, horizontal, on 32 fixed one-third-octave bands — the display draws what
  an FFT can actually deliver, at a colour scale that means the same thing on every device.** Simon
  confirmed the word (spectrogram, not Decibel X's bars) and then took neither prototype variant whole:
  A's frame with B's axis. Three parameter questions turned out to have *measured* answers rather than
  aesthetic ones. Cells are **band energy, not per-bin mean** — pink noise draws flat with `sum` and
  picks up a fabricated **32 dB roll-off** with `mean`. **N = 8192**, because at N=2048 everything below
  **126 Hz** is bin-borrowing, which is the whole dB(C) region. And a pixel-resolution log axis
  over-promises at every N (25 % of it interpolated at N=8192), while third-octave bands starve only at
  12.5 Hz. The surprise: an energy-summed display's dB scale **moves with the row layout** (~10 dB
  between 24 rows and 220), so fixing the layout is what makes a labelled legend honest. The display is
  **always unweighted** — A-weighting it made the rumble stripe vanish, and dB(A) mode is exactly when
  you want to see rumble — and its **span follows the L_eq window**, so the picture is what is inside the
  number. Fixed dB window, no auto-ranging; gaps drawn as holes, not as fake quiet. **`08`'s feared bulk
  payload evaporates: 32 band levels, ≈1.3 kB/s, not 4 096 bins.** Variant C (calm, no chrome) failed its
  own bet and that was the useful result — with no hue the rumble and the speech merged into one haze.

- [Rust ↔ frontend boundary: what crosses, how often, in what shape](issues/08-rust-frontend-boundary.md)
  — **One 10 Hz event carrying everything the screen paints, one command returning the picture's
  history, and a frontend that holds no authoritative state at all.** Reading the vendored Tauri
  source dissolved the ticket's central question: `emit` and `Channel::send` are the *same*
  transport — both `eval` a JS string with the JSON inlined — so events-versus-channels is about
  lifecycle, not speed, and a binary format would be a **pessimisation** (raw bytes under 1 kB are
  turned into a JSON number array anyway). Events also need no capability entry, so `CLAUDE.md`'s
  four-step ceremony never applies. The tick is ≈460 bytes, ~17× under Tauri's own fast-path
  threshold. One decision is required for *correctness* rather than robustness: columns carry
  **absolute slot indices** and each tick ships every real column since the last publish, because
  `05`'s clock-advanced ring means a late tick has more than one completed slot behind it — and gaps
  then need no marker, since a missing index *is* `07` decision 11's hole. Rust owns a 1200-column
  ring so every canvas invalidation is one move, *pull again*. Settings and the unit ride in **every**
  tick, which extends `05` decision 13's footgun-denial from values to labels. Two findings the
  grilling surfaced that no ticket owned: **Reset must not clear the picture** (unrecoverable beats
  mildly inconsistent — a correction to `07` decision 8's *phrasing*), and a **denied microphone had
  no way to say so**, which adds a three-value `input` state — the one place the app speaks up, and
  still not a warning.

- [Screen layout: meter and spectrogram together](issues/09-screen-layout.md)
  — **Variant C's frame with its hierarchy inverted: one dominant number over a wide picture,
  everything else behind a single sheet — but the dominant number is the *live* level, not the
  rolling L_eq.** Simon's call, and it **inverts `09`'s own premise**; the argument the ticket had
  not made is that a 60 s L_eq moves **0.0 dB in 8 s**, so at hero size it is dead screen. The cost,
  which `10` must state: the number that dominates is no longer the number judged against 70. The
  L_eq and its coverage and the max hold are **always visible** — C hid them to test whether they
  were desk curiosity and they are not. Two things measurement settled that looked like taste:
  **`S` is the default time weighting**, because at F the hero moves 1.72 dB per tick — further than
  any rounding step, so `09`'s inherited readability lever does almost nothing (9.4 → 8.3 digit
  changes/s from 0.1 to 1 dB) — and **display resolution stays 0.1 dB**, because coarsening is one
  setting for all three numbers and would spend a tenth on the ceiling-judged L_eq to fix a
  different number. `--` turns out to need a **typographic** treatment, not just a glyph: at hero
  size two dashes render as a redaction bar. The **legend stays**, so `07`'s "if space forces it out"
  trade is not taken. **This one reached the device** — `ios build` + `devicectl` needs no
  networking, unlike the Safari route that blocked `07`.

- [Write the spec](issues/10-write-the-spec.md)
  — **[`spec.md`](../../docs/spec.md) is written and approved, with no notes.** 18 sections, and every decision
  cites the ticket that made it, so the spec is a destination rather than a replacement for the
  reasoning. Assembling it was not pure transcription: putting ten resolved tickets side by side
  surfaced **two stale texts and a miscount** (`06` d5 still lists `F` as the persisted default time
  weighting, which `09` d5 changed to `S`; `08` d9 says "eight commands" where its own contract block
  lists seven; `09`'s sketched legend shows a 40 dB span against `07` d9's 60 dB), and it found that
  **`09`'s zero-power correction had two more homes neither ticket noticed** — exact-zero blocks *do*
  arrive, so `05` d11's 200 ms staleness rule never fires and `10·log₁₀(0)` would reach `serde_json`
  after all, and a zero-power spectrogram column draws at the bottom of `07` d9's colour window,
  which is the "dead stream reads as a peaceful room" failure `07` d11 exists to prevent. Three
  things were **nobody's decision** and had to be made for the "no decisions left open" bar to mean
  anything: `realfft`, `rtrb`, and the **FFT power normalisation** — `07` quoted its colour window in
  dBFS while measuring through a normalisation it never wrote down. The one chosen makes a full-scale
  sine read −3.01 dBFS in its band, the same convention as the meter, which is what lets one
  calibration offset shift both the numbers and the picture. All eleven such choices are listed for
  veto in [§16](../../docs/spec.md#16-what-this-spec-decides-that-no-ticket-decided) and all twelve carried
  corrections in [§17](../../docs/spec.md#17-corrections-this-spec-carries).

## Requirements discovered while charting the terrain

Not decisions and not fog — things the MVP must do that no ticket asked for, surfaced by probing:

- **Disable the idle timer.** From `11`. Without it a long talk outlives the screen and
  measurement dies silently.
  **`08` settled where it lives and found it is free:** Rust sets it once at startup and nothing
  about it crosses the bridge. Per `CLAUDE.md`, Tauri's hardcoded iOS framework list already covers
  UIKit, so `objc2-ui-kit` needs **no `bundle.iOS.frameworks` entry and no `tauri ios init`
  regeneration** — unlike `02`'s AVFAudio/AudioToolbox/CoreAudio.
- ~~**The rolling L_eq must track coverage, not just energy.**~~ **Discharged by `05`**, and in a
  stronger form than `11` asked for: coverage is not a flag bolted onto the ring but a consequence of
  advancing it by the clock, so it reports missing data from causes we never detect.
- **`AVAudioSessionModeMeasurement` is mandatory**, not preferred. From `11`: 21 dB of processing
  gain without it, very likely level-dependent, which would make the single-offset calibration
  model meaningless. Belongs to tickets `06` and `10`.
  **Discharged by `06`**, and the "very likely level-dependent" worry was retired by measurement — the
  21 dB is a **fixed gain**. `06` also adds a new requirement of the same shape: **read the session mode
  back and log a mismatch**, on the reasoning `01`/`02` applied to the sample rate. If `Measurement`
  ever silently fails to apply, the stored offset is wrong by ~21 dB. A log line, nothing in the UI.
- **The iOS target must link `AVFAudio`, `AudioToolbox` and `CoreAudio`** via
  `bundle.iOS.frameworks`. From `02`. Invisible to `cargo check`, since nothing links until Xcode
  does.

## Not yet specified

**Everything still open below is carried into the spec's
[§15](../../docs/spec.md#15-open-questions-this-spec-does-not-close) and its
[§13.14](../../docs/spec.md#1314-untested-and-known-to-be-so), so none of it is lost by the map closing.
Nothing here blocks implementation.**

- **Desktop dev-loop fidelity** — how to test the meter with reproducible signals when
  the reference meter is at the venue and not on the desk. Probably generated tones or
  pink noise at a known level, but it depends on what the weighting architecture settles.
  **Mostly answered by `04`:** with biquads settled, the *filter* needs no hardware and no
  reference meter — synthesise tones in Rust at the **exact** one-third-octave frequencies and
  assert against research `03` §4.2 (tight on the two design invariants: C = −3.010 dB at
  31.6228 Hz and 7943.282 Hz). What remains genuinely open is only the **acoustic** end: no
  desk-side way to know a real sound's true SPL, which is a calibration question (`06`), not a
  DSP one.
  **`07` adds two eyeball tests for the display half**, both of which caught real errors in the
  prototype: **pink noise must draw flat** (it is equal energy per third-octave by definition, so a
  sloped picture means the band summarisation is wrong), and an **exponential sweep must draw a
  straight diagonal** (a curve means the log axis mapping is wrong). Neither needs hardware.
  **`10` writes the whole thing down as spec [§14](../../docs/spec.md#14-validation-and-tests)**, DSP half and
  display half together, so what is left of this entry is only the *acoustic* end — and that is a
  calibration limitation (spec §13.6) rather than an unspecified piece of the dev loop.
- ~~**Whether the input path is linear, which decides if a single calibration offset is valid at
  all.**~~ **Answered by measurement, 2026-08-05: it is linear.** Same tone at two source levels
  20 dB apart in `Measurement` mode read **−57.7** and **−37.8 dBFS** — a delta of **19.9 dB against
  20 expected**, where a level-dependent path would have compressed by dB rather than tenths. So
  `11` probe 4's 21 dB is a **fixed gain**, not AGC, and ticket `06`'s single-offset model is
  defensible rather than merely assumed. Recorded on
  [`06`](issues/06-calibration-model.md), with the reasoning that rules out room noise as the source
  of the 0.1 dB, plus the one thing the test does not cover: it brackets −57.7 to −37.8 dBFS, and
  limiting engages near the *top* of a range, which is plausibly above the highest tested level.
  A further run 20 dB up would bracket the real operating point — worth doing with better equipment,
  not worth blocking on.
- ~~**How the dB(A)/dB(C) switch presents**~~ — **closed by `09`.** `04` made it a
  **settings-page setting** with **three** modes rather than two, and `05` added a second dimension
  to indicate (time weighting F/S, because it ties the meaning of max hold to both). `09` settles the
  rest: the switch lives in the **one sheet** reached from `⋯`, alongside calibration, and the active
  mode is indicated by the **hero number's own header** — `NOW · C · slow`, both dimensions at once,
  above the number they describe. A bare `MAX 72.4 dB` never arises, because one header governs the
  live number and the max hold together.
- ~~**Whether the weighting filter validation table becomes a unit test in the repo**, and if
  so what test runner gets added~~ — the project has none, and `CLAUDE.md` says to ask before
  adding one. Ticket `03` produced a ready-to-use 34-frequency table, so this is now a
  concrete question rather than a vague one; it needs the implementation effort to exist
  first, so it may belong to that effort rather than this map.
  **The Rust half dissolved in `04`:** hand-rolled biquads with runtime coefficient derivation
  *need* that table as a test, and it is a pure Rust test — `cargo test` plus a `#[cfg(test)]`
  module adds no dependency, no config and no tooling decision, so there is nothing to ask
  about. Only the frontend testing question remains, and nothing so far requires it.
  **`08` narrows the frontend half to one file.** Decision 9 hand-writes the bridge types in
  `src/bridge.ts` with identical snake_case names on both sides rather than generating them, so the
  only untested seam that matters is that file — and a renamed field there is a runtime `undefined`
  rather than a build failure, because `vue-tsc` cannot see across the bridge. Accepted deliberately
  on the resist-complexity bar, with `ts-rs` named as the escape if the contract grows.
  **Closed by `10`: no runner is added at all.** The frontend half is settled the same way `08`
  narrowed it — nothing so far requires one, so none is added and `src/bridge.ts` stays the single
  accepted untested seam. Spec [§14](../../docs/spec.md#14-validation-and-tests) writes the whole test plan
  instead: the 34-frequency table with its two assertion levels and its two false-failure traps
  (exact frequencies, not nominal; discard ≥2 s of transient), the two design invariants worth tight
  tolerances, the metrics-pipeline cases including *every row* of the reset/clear table, and `07`'s
  two eyeball tests.
- **Free-provisioning friction** — the 7-day reinstall cycle may become annoying enough
  to need a decision. Signing itself works (`02`), but the expiry has not been hit yet, so the
  re-signing workflow is still unrecorded.
  **`06` found a consequence worth knowing before that happens:** the reinstall can take the app's data
  container with it, and with it the calibration offset. Installing over the top usually preserves it,
  but delete-then-install does not, nor does regenerating the Xcode project. Mitigated rather than
  solved — the offset is displayed so it can be written down and retyped in seconds. Still worth
  recording what actually survives when the expiry is first hit.
- **Dev-loop networking on an isolated Wi-Fi** — `tauri ios dev` hot reload cannot reach the
  phone on this network (client isolation), so device work uses an embedded `ios build` plus
  `devicectl install`. Tolerable for probing; worth revisiting if iteration on device becomes
  frequent during implementation.
  **`09` proved the workaround carries a whole frontend, not just a probe**: `npm run build` →
  `env -u FORCE_COLOR npx tauri ios build` → `devicectl device install app` → `devicectl device
  process launch`, about a minute per cycle once Rust is cached. That is what let `09` do the device
  pass `07` could not. Two notes for whoever repeats it: the switcher UI must not be gated on
  `import.meta.env.DEV`, because the device build is a release build; and there is no address bar, so
  anything reachable only by query string needs an in-app link.
- **Whether ~6 px per band reads at arm's length in a dim venue.** The last open question from `07`,
  and `09` narrowed it rather than closing it: the app is now *on the device* (32 bands over ~205 px)
  but no verdict has been given. It is the one thing that could still move `07` decision 3's band
  count or its ~200 px height, and `09` decision 6's muted `--` wants the same look. Cheap to answer
  — the build is installed — so it should not become a ticket unless the answer is "no".
  **`10` names this the first thing the implementation effort should do**, on exactly that
  cost/benefit: it is the only open question that could still move a settled parameter, and answering
  it needs nothing built.
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
