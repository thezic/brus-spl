# SPL Meter MVP — specification

Status: **draft, awaiting approval** · Assembled by ticket
[`10`](issues/10-write-the-spec.md) from every resolved ticket on
[the map](map.md) · 2026-08-05

This is the destination of the `spl-meter-mvp` map. It is written to be complete enough that
an implementation session needs **no further decisions**. Where a decision was made by a
ticket, the ticket is cited (`04 d1` = ticket 04, decision 1) so the reasoning can be read in
full rather than re-argued. Section [16](#16-what-this-spec-decides-that-no-ticket-decided)
lists the handful of choices this spec makes that no ticket made, and section
[17](#17-corrections-this-spec-carries) lists the corrections it carries forward — both are
short, and both are the places to look first when reviewing.

---

## 1. What the app is

A **live sound-level instrument** for one person, on one iPhone, read at a venue and acted on
by the person holding it.

Three numbers and a picture, on one screen:

- **NOW** — the live, exponentially time-weighted level. The largest thing on the screen.
- **L_eq** — a rolling equivalent-continuous level over a configurable window (default 60 s),
  shown with **how much of that window is real data**. This is the number judged against the
  externally imposed **70 dB** ceiling.
- **MAX** — the highest NOW seen since the last Reset.
- **A scrolling spectrogram** — 32 one-third-octave bands, answering *what is making the
  sound and has it been steady*, which the numbers cannot.

Weighting is **dB(C)** by default, with **dB(A)** and **dB(Z)** selectable. Calibration is a
single stored broadband offset, set by matching a proper SPL meter present at the venue.

### 1.1 The three principles the whole design obeys

Restated from the map, because several decisions below only make sense against them:

1. **He makes the call, not the app.** No warnings, no automatic resets, no thresholds, no
   colour that means alarm. The instrument reports; the reader decides.
2. **Never show a confident wrong number.** Where the instrument does not know, it says so —
   `--` for an undefined level, a coverage figure beside every average, `dBFS` when
   uncalibrated, a hole in the picture where there is no data. This is the one thing that
   overrides tidiness.
3. **Resist complexity.** The simplest thing that answers the question. Nothing was added
   that was not asked for or forced.

### 1.2 Out of scope

Confirmed out of scope with Simon while charting, and unchanged by anything since:

- Logging / session history — a live instrument, not a record.
- Export / evidence reports — the most probable *next* effort, not this one.
- Audio recording — levels only, never the sound itself.
- Multiple calibration profiles, and per-frequency calibration curves.
- App Store / TestFlight distribution — personal device, free provisioning.
- Android.
- **Any claim of IEC 61672 conformance.** See section [13](#13-accuracy-and-limitations).

---

## 2. Platform, build and permissions

| | |
|---|---|
| Target | **iOS**, personal device, free provisioning (7-day re-sign) |
| Dev loop | **macOS desktop**; every architecture choice is iOS-viable |
| Stack | Tauri 2 · Rust backend · Vue 3 + TypeScript frontend (`strict`) |
| Measured device baseline | iPhone 14 Pro, iOS 26.5.2: **48 000 Hz, mono, f32, 1024-frame buffer**, block rate 46.875/s (`02`) |

Nothing about 48 kHz / mono / 1024 is assumed. Apple documents both the sample rate and the
buffer duration as *preferences*; on the measured device both requests were granted exactly,
which is a fact about that device and route rather than a guarantee (`01`, `02`). **Every
value is read back and used as ground truth.**

### 2.1 Permissions and entitlements — already in place, must survive

- **`src-tauri/Info.plist`** — `NSMicrophoneUsageDescription`. Serves **both** iOS and macOS.
  A missing key is a launch-time process kill, not a build error.
- **`src-tauri/Entitlements.plist`** — `com.apple.security.device.audio-input`, referenced
  from `bundle.macOS.entitlements`. Needed because Tauri enables Hardened Runtime by default,
  so a *signed* macOS build fails capture without it while unsigned `tauri dev` succeeds. The
  dev loop is the permissive case and the shipped bundle the strict one — still **untested**
  (research `01` open risk 8).
- **`bundle.iOS.frameworks: ["AVFAudio", "AudioToolbox", "CoreAudio"]`** in
  `tauri.conf.json`. Rust builds as a staticlib and *Xcode* does the final link, so the
  `#[link(kind = "framework")]` attributes inside `objc2-avf-audio`,
  `objc2-audio-toolbox` and `objc2-core-audio` never reach that link step, and Tauri's
  hardcoded list covers only UIKit/WebKit/Metal and friends. **Invisible to `cargo check`,
  including `--target aarch64-apple-ios`** — nothing links until Xcode does (`02`).
  Changing this list requires
  `rm -rf src-tauri/gen/apple && env -u FORCE_COLOR npx tauri ios init`.
- **`objc2-ui-kit`** (section [4.4](#44-the-idle-timer-must-be-disabled)) needs **no**
  frameworks entry and therefore **no regeneration** — UIKit is already in Tauri's hardcoded
  list (`08`).
- `@tauri-apps/cli` stays **pinned to an exact version** (2.11.4). The iOS `Info.plist` merge
  order is undocumented and changed silently between 2.4 and 2.9. Run
  `./scripts/check-ios-plist.sh` after any `tauri ios build`.
- **Never run any `tauri ios` command with `FORCE_COLOR` set.** Use `env -u FORCE_COLOR`. See
  `CLAUDE.md`; the failure is a misleading `Arch specified by Xcode was invalid. {arch}`.

### 2.2 Dev-loop constraint

The Wi-Fi in use has **client isolation**, so `tauri ios dev` hot reload cannot reach the
phone. The working device loop, proven to carry a whole frontend (`09`), ~1 minute per cycle
once Rust is cached:

```bash
npm run build
env -u FORCE_COLOR npx tauri ios build --debug
xcrun devicectl device install app --device <udid> \
  src-tauri/gen/apple/build/arm64/decibel-meter.ipa
xcrun devicectl device process launch --device <udid> net.thezic.decibel-meter
```

Two consequences: `println!` **does not** reach `devicectl … --console`, so on-device
diagnostics must be visible on screen or in Xcode's log; and a device build is a *release*
build, so nothing may be gated on `import.meta.env.DEV`.

---

## 3. Audio capture

**`cpal` 0.18.1 for capture, with `AVAudioSession` configured by us from Rust via
`objc2-avf-audio`** (`01`, proven on device by `02`).

The load-bearing fact: **cpal never sets the session category and never activates the
session** — it treats that as the application's job. Apple's default category is
`SoloAmbient`, which permits no input, so skipping it guarantees failure. cpal also never
calls `setPreferredSampleRate`.

### 3.1 iOS session configuration, in order, before touching cpal

```
category  = AVAudioSessionCategoryRecord
mode      = AVAudioSessionModeMeasurement        ← mandatory, see 3.2
setPreferredSampleRate(48000)
setPreferredIOBufferDuration(1024 / 48000)
setActive(true)
```

Then **read back and use as ground truth**: `sampleRate`, `inputNumberOfChannels`, `mode`,
`IOBufferDuration`. Log any mismatch between what was asked for and what was granted; nothing
in the UI (`01`, `02`, `06`).

Two failure signatures that look nothing alike and are easily confused:

| Symptom | Cause |
|---|---|
| `InvalidInput: channel count must be at least 1` | session category was never set — **or** the session is deactivated after an interruption (`11`) |
| buffers of exact zeros | microphone permission not granted |

### 3.2 `AVAudioSessionModeMeasurement` is a precondition, not a refinement

Measured on device: the same 440 Hz sine read **−51.7 dBFS** in `Measurement` mode and
**−30.6 dBFS** in `Default` — **21 dB of processing gain** (`11` probe 4). A stored
calibration offset means nothing if the mode is not fixed.

The 21 dB was subsequently shown to be a **fixed gain, not AGC**: the same tone at two source
levels 20 dB apart read −57.7 and −37.8 dBFS, a delta of 19.9 dB against 20 expected
(`06`). That is what makes the single-offset calibration model defensible rather than merely
assumed.

**The mode is read back and a mismatch logged.** If `Measurement` ever silently fails to
apply, the stored offset is wrong by ~21 dB — worth a log line, nothing in the UI (`06`).

### 3.3 Sample handling

- Samples arrive as **f32** in a real-time callback. **Convert f32 → f64 at the block
  boundary** and do all DSP in f64 (`03` P1/P2 — f32 state degrades by +1.7 dB with a DC
  offset and +3.9 dB with 5 Hz rumble, and DC bias plus infrasonic HVAC is exactly what a
  phone mic in a venue delivers).
- **If the stream reports more than one channel, use channel 0 only.** Averaging correlated
  channels changes the level by up to 6 dB and would make calibration depend on channel count.
- **No DC blocker.** The A and C weighting filters are themselves high-passes, f64 removes the
  precision motive, and the FFT tap is upstream — where DC is information, not noise (`04`).
- **The audio callback does the minimum**: f32 → f64, run the selected weighting chain, sum
  `p²` and `n`, push one summary per block into a bounded lock-free SPSC queue, and copy the
  raw block into the FFT ring. No locks, no allocation. The FFT runs **off** the callback.

### 3.4 Desktop

No session configuration; cpal's default input device, whatever it reports. Per `CLAUDE.md`,
`tauri dev` **does** get microphone access on macOS with no `Info.plist` — TCC attributes the
request to the responsible parent process — so the desktop is the permissive case.

---

## 4. Interruptions, gaps and recovery

### 4.1 What actually happens

Measured on device (`11` probe 2), Siri invoked 3 s into a 30 s run:

```
GAPS (1):  3.26s → 30.00s (26.74s), NEVER RESUMED
SESSION CHANGES (1):  at 3.26s: rate 48000 → 48000 Hz, channels 1 → 0
stream errors: none
all-zero blocks: 0
```

**The stream died permanently and not one error reached Rust.** A rolling L_eq that trusted
the sample stream would have averaged a 60 s window over 3 s of data and looked entirely
plausible. This single measurement is why the metrics pipeline is built the way section
[6](#6-level-metrics) describes.

New finding: **`inputNumberOfChannels` went 1 → 0 and stayed there**, so the session was left
*deactivated*.

### 4.2 What the app does about it

1. **Stay on released cpal 0.18.1 and handle interruptions ourselves** (`11` d1). `master`'s
   auto-resume is *silent*, so gap accounting is needed either way; we already own session
   setup, so an observer is an extension rather than new machinery; pinning a SHA is ongoing
   maintenance for something we would still have to wrap.
2. **Observe `AVAudioSessionInterruptionNotification`** ourselves (`11` d2). Forced, not
   chosen — cpal never surfaces interruptions.
3. **Recovery must `setActive(true)` before rebuilding the cpal stream.** A rebuild against a
   deactivated session fails with `InvalidInput: channel count must be at least 1`, *the same
   error as a never-set category*. Expect it, or it will look like a regression in the setup
   code (`11` d2).
4. **`inputNumberOfChannels == 0` is a pollable health check**, independent of the
   notification. Checked on the 10 Hz tick.
5. **Any cpal stream error takes the same recovery path** — reactivate, rebuild. Route changes
   may or may not announce themselves (probe 3 was never run for want of headphones); probe 2
   proved the error callback silent for an interruption, so nothing assumes an announcement.
6. **After a rebuild, recompute the filter coefficients from the newly read-back rate and zero
   the filter state — but do *not* reset the window** (`04`, re-confirmed `05` d4). Same
   quantity through a differently-designed filter, so the old slots remain valid energy. The
   dead air during a rebuild becomes gap slots by construction (section
   [6.2](#62-the-monotonic-clock-advances-the-ring)) — **zero code**.

### 4.3 Measurement is foreground-only

Capture does not survive backgrounding (`11` probe 2b — the app dies; whether iOS terminated
it or our own code faulted was never characterised). **The `audio` background mode is not
added**; that is the case research `01` warned would push toward a Swift/`AVAudioEngine`
rewrite, and it is not needed for an instrument being read at a venue (`11` d4).

### 4.4 The idle timer must be disabled

`UIApplication.isIdleTimerDisabled = true`, set once at startup from Rust via `objc2-ui-kit`
on the main thread. **Without it the screen sleeps mid-talk and measurement dies on its own** —
a real MVP requirement discovered by probing, not by charting (`11` d4). It does not cross the
bridge and needs no framework entry or project regeneration (`08`).

---

## 5. Frequency weighting

**Three modes: dB(C) (default), dB(A), dB(Z)** — chosen on the settings sheet, not on the main
screen (`04` d6).

### 5.1 What Z is for

**Z is the instrument with its filters switched off — a diagnostic for the calibration chain,
not a third opinion about loudness.** Clause 5.4.8 eq. 8 defines Z as flat 10 Hz–20 kHz, so in
this architecture it is the bypass path: RMS straight off the raw samples. Its value is that it
is the only way to exercise the whole calibration chain end-to-end with the weighting filters
out of the way (`04` d6, `06`).

**A Z reading compared against the imposed dB(C) ceiling is meaningless, and nothing at
runtime prevents that comparison.** This paragraph is the only place that gets said.

Note there is no DC blocker, so a Z reading carries the microphone's DC bias as a floor.

### 5.2 Two pipelines, not one

```
callback ─┬─→ [selected weighting chain, f64] ─→ square ─→ Leq ring ─→ the numbers
          │      C: 2 biquads · A: 3 biquads · Z: bypass
          └─→ [raw ring] ─→ FFT off-thread ─→ 32 bands ─→ the spectrogram
```

**Time-domain cascaded biquads produce the reported number; the FFT draws the spectrogram and
nothing else** (`04` d1). The trade was put to Simon explicitly and he took defensibility:
research `03` §7 measured FFT-domain C **over-reading by +6.1 dB at N=1024** and +4.3 dB at
N=2048 when a 12 Hz component is present, because the entire steep C roll-off below
f₁ = 20.6 Hz falls inside the first bin. **The error direction is over-reading**, so the
failure mode is false alarms against the 70 dB ceiling — and a venue with HVAC is precisely
the environment that supplies the infrasound that triggers it.

**Only the selected chain runs** (`04` d2). Changing the weighting mode rebuilds the chain,
zeroes its state, and **restarts the rolling window** — LCeq and LAeq are different quantities
and cannot be averaged together. Deliberate, and a settings-sheet act rather than a main-screen
toggle, so it needs no machinery to absorb it. Convenient side effect: the f₁ double pole's
slow settling transient is buried, because the window is visibly near-empty at that moment
anyway.

**The FFT taps the raw samples ahead of the filter** (`04` d3), which keeps the pipelines
independent and preserves the unweighted display section [7](#7-spectrogram) requires.

### 5.3 The filters

Sources and the full derivation: [`research/03-iec-weighting-filters.md`](research/03-iec-weighting-filters.md),
from IS 15575-1:2005 (the Bureau of Indian Standards adoption declared identical with
IEC 61672-1:2002, published free), corroborated against three reference implementations.

**Build, don't adopt.** Nothing in Rust is adoptable as an IEC weighting implementation
(`03`). Hand-roll the recursion; no `biquad` crate (`04` d4 — the coefficient derivation is
ours either way, the recursion is ~15 lines, and `biquad` 0.6 remains a clean fallback).

**Derived pole frequencies** (clauses 5.4.9–5.4.11; f_L = 10^1.5, f_H = 10^3.9,
f_A = 10^2.45, D = √½):

```
f₁ =    20.598997057568143 Hz   (double pole, A and C)
f₂ =   107.65264864304627  Hz   (single pole, A only)
f₃ =   737.8622307362899   Hz   (single pole, A only)
f₄ = 12194.217147998010    Hz   (double pole, A and C)
```

All poles are **real and negative** — no complex arithmetic anywhere. Zeros at s = 0: two for
C, four for A.

**Section assignment — always cascade, never build the expanded high-order polynomial** (`03`
P1: a single 6th-order direct-form section in f32 *overflows to NaN*). Put `S_hp1` last, the
section nearest the unit circle:

| Mode | Sections, in cascade order |
|---|---|
| **C** | `S_lp4` = ω₄²/(s+ω₄)² · `S_hp1` = s²/(s+ω₁)² |
| **A** | `S_lp4` · `S_hp23` = s²/((s+ω₂)(s+ω₃)) · `S_hp1` |
| **Z** | none — bypass |

**Discretisation: plain bilinear transform, no prewarping, no oversampling** (`03`, `04`).
With `c = 2·f_s`, for a section `N(s)/D(s)` where `N = n₂s² + n₁s + n₀`, `D = d₂s² + d₁s + d₀`:

```
b0' = n₂c² + n₁c + n₀        a0' = d₂c² + d₁c + d₀
b1' = 2n₀ − 2n₂c²            a1' = 2d₀ − 2d₂c²
b2' = n₂c² − n₁c + n₀        a2' = d₂c² − d₁c + d₀

b0 = b0'/a0'  b1 = b1'/a0'  b2 = b2'/a0'  a1 = a1'/a0'  a2 = a2'/a0'   (a0 ≡ 1)
```

| Section | n₂, n₁, n₀ | d₂, d₁, d₀ |
|---|---|---|
| `S_lp4` | `0, 0, ω₄²` | `1, 2ω₄, ω₄²` |
| `S_hp1` | `1, 0, 0` | `1, 2ω₁, ω₁²` |
| `S_hp23` | `1, 0, 0` | `1, ω₂+ω₃, ω₂ω₃` |

The recipe depends on the sample rate **only through `c = 2 f_s`** — ~30 flops, so
coefficients are **computed at stream start from the actual read-back rate**, never compiled
in.

**Normalise the digital cascade at 1 kHz, not the analogue prototype** (`03` §2.3). Drop the
standard's `A₁₀₀₀`/`C₁₀₀₀` entirely, evaluate the digital cascade at 1 kHz, and divide one
section's `b` coefficients by that magnitude:

```
g = |H_digital(e^{j2π·1000/f_s})|   then  b0,b1,b2 of section 0  /=  g
```

Worth the one line twice over: 1 kHz is *the* calibration anchor, and it makes clause 5.4.14
(`|L_C − L_A| ≤ 0.4 dB` at 1 kHz) exactly zero by construction, which turns it into a free
unit test.

**Realisation: transposed direct form II (DF2T), f64 coefficients *and* state.** 5 multiply-
adds per section per sample, so the worst case (A, 3 sections) is 15 per sample ≈ 720 k/s at
48 kHz.

**Coefficient fixtures for cross-checking a runtime implementation** (not for hardcoding — the
gain distribution across sections is arbitrary, so compare *responses*, not coefficients):
research `03` §2.4 tabulates all sections at 44 100 and 48 000 Hz.

**Weighting goes before squaring and averaging** (clause 3.9 eq. 2 — the weighting is inside
the integral), and **no F/S time weighting inside L_eq** (clause 3.9 NOTE 3).

**No sub-40 kHz guard** (`04` d5). See section [13.3](#133-a-low-rate-route-is-measured-and-shown-without-any-indication).

---

## 6. Level metrics

**A ring of 100 ms energy slots advanced by the monotonic clock, not by arriving samples —
which makes gap accounting a consequence of timekeeping rather than a feature** (`05`).

```
audio callback (real-time)                     display tick (10 Hz)
──────────────────────────                     ────────────────────
f32 → f64 at the block boundary                drain the queue; per block:
selected chain  C:2  A:3  Z:bypass               ├─ deposit (Σp², n) in the clock-current slot
Σp² over the block, plus n                       ├─ advance the F/S smoother by n/fs
push {Σp², n, t} ── bounded SPSC ────────→       └─ max hold compare      (46.875 /s)
                                               re-sum the ring → L_eq, coverage
                                               + calibration offset, in dB
                                               publish the tick event
```

### 6.1 The L_eq averages real data only

`L_eq = 10·log₁₀(Σ sum_sq / Σ n) + offset` over the slots in the window — **never divided by
the nominal window length** (`05` d1).

Folding a gap in as silence costs **−2.6 dB** for a 27 s hole in a 60 s window, and the
direction is *under*-reading against a ceiling. Averaging over real data makes the displayed
figure mean "the L_eq of the 33 s I actually have", which is exactly what the coverage figure
then qualifies.

**The first 60 seconds stops being a special case.** A filling window is a window with low
coverage, so startup needs no separate code path, no blank screen and no "warming up" state.
The meter is live and honest from the first tick.

### 6.2 The monotonic clock advances the ring

The clock advances the ring; the callback merely deposits into whichever slot is current when
drained. **Slots nobody deposited into are gap slots by construction** (`05` d2).

This is the direct answer to section [4.1](#41-what-actually-happens): a sample-driven ring
stops moving when the samples stop, so it would have reported **full coverage** over a stream
that ended half a minute earlier. Because the clock drives it, missing data is caught
*whatever* the cause — interruption, a route change that killed the stream, the dead air during
a rebuild — **without enumerating causes**. Given probe 2 proved the error callback stays
silent, any design that only accounts for gaps it *detects* will eventually show full coverage
over a hole.

The interruption observer is still required, for **recovery**, not for accounting.

### 6.3 The ring

- **One slot is 100 ms**, holding `(sum_sq: f64, n: u32)`. Sum-of-squares *and* count, not a
  mean, because section [6.1](#61-the-l_eq-averages-real-data-only) divides by the actual sample
  total.
- **A time grid, not one slot per callback block.** A gap emits *no blocks at all*, so a
  block-indexed ring has nowhere to record it; and a block is 21.333 ms *at 48 kHz*, a device
  quantity that would rescale history on a route change.
- 100 ms is chosen for **retirement smoothness**: a retiring slot is 1/600 of the energy at a
  60 s window. At 1 s slots it is 1/60, and on a 10 s window 1/10, which stair-steps visibly
  once a second.
- **Blocks are assigned whole** to whichever slot is current when drained, so placement is
  approximate to ±21 ms while the coverage arithmetic stays **exact** — it counts samples, not
  slots.
- **Re-sum the window every tick** rather than maintaining a running add-and-subtract sum. 600
  f64 adds at 10 Hz is 6 000 adds/s, and it removes any question of drift over an eight-hour
  session. Coverage falls out of the same loop free.
- **The ring is allocated at the longest permitted window, always** — 1200 slots ≈ 19 KB,
  permanently (`05` d5).

**A zero-power block is discarded at drain time, not deposited** (`09` correction to `05` d11).
A denied microphone delivers callbacks of *exact zeros*, so blocks arrive and `Σn > 0` while
`Σp² = 0` leaves the level undefined — which put `60s of 60s` beside `--` on the prototype, the
coverage figure claiming a complete minute while the meter said there was nothing. Coverage
exists to say whether the number can be trusted, and **zero power is the absence of a
measurement, not a quiet one**. So a denied mic reads `--` beside `0s of 60s`. Side benefit: no
f64 `−∞` ever reaches `serde_json`.

### 6.4 Coverage

`coverage_s = Σ n / f_s` over the window slots, published **always** — `33s of 60s`, and
`60s of 60s` when full (`05`, `11` d3).

**Not conditional on being degraded.** An indicator that appears only when something is wrong
is a warning, which charting ruled out; always-on is what makes the number trusted.

### 6.5 The window length

**10 / 30 / 60 / 120 s, default 60. A picker, not a numeric field.** Changing it **re-slices
the ring — no reset** (`05` d5, d6).

The ring holds valid energy at any length, so the setting is nothing but *how far back do I
sum*. 60 → 120 s is instant and uses data already held (`60s of 120s`, a genuine 120 s average
of everything there is, rather than starting from empty); shortening is instant and exact; and
**no third reset cause is created**.

The 10 s option has a job beyond impatience: section [8](#8-calibration)'s matching gesture
wants a number steadier than the live readout but settling in seconds. 120 s stops well short
of the blindness that got a 15-minute window rejected while charting. A free numeric field was
rejected as a keyboard on a phone at a venue.

### 6.6 The live level (NOW)

Exponential time weighting, clause 3.5 Figure 1: a one-pole low-pass on the **squared**
weighted pressure, then to dB.

```
y += (x² − y)·(1 − e^{−dt/τ})        τ_F = 125 ms      τ_S = 1 s
NOW = 10·log₁₀(y) + offset
```

**F/S is selectable; `S` is the default** (`05` d7, default set by `09` d5). The smoother is
advanced **per block by that block's own `dt`**, which makes it mathematically identical to
advancing it continuously however irregularly the tick fires.

Why S rather than F, measured rather than argued (`09` findings 2–3): with the live value as
the hero number, at **F it moves 1.72 dB per 100 ms tick** — further than any rounding step, so
display resolution cannot calm it (0.1 → 1 dB only takes churn from 9.4 to 8.3 digit changes/s).
At S it moves 0.351 dB per tick and covers 6.8 dB of range, against **0.017 dB and 0.5 dB for
the 10 s L_eq** — 20× and 14×, so S is genuinely live and still readable. F stays selectable
and is still the right choice for catching transients.

A τ change **does not reset the smoother**: its state is a smoothed mean square either way, so
the coefficient swaps and the state keeps. It re-converges in a few hundred ms going S→F and a
couple of seconds F→S.

The L_eq accumulator is **untouched** by this setting (clause 3.9 NOTE 3). It never reaches the
ring, not even as a flicker.

### 6.7 Max hold

**The maximum time-weighted level — L_CSmax by default (clause 3.6), compared inside the drain
loop at block rate (46.875/s)**, not sampled from the smoother at the 10 Hz tick, which would
drop four peaks in five (`05` d9).

Stored as the maximum *linear* smoothed mean square; published as `10·log₁₀(max) + offset`,
which is correct because `max(xᵢ + c) = max(xᵢ) + c`.

The alternatives got a fair hearing and lost. **Max of the rolling L_eq** is the
compliance-shaped figure if the imposed 70 dB is an LCeq limit, but it is lagged by up to a
window length, and under section [6.1](#61-the-l_eq-averages-real-data-only) it would latch onto
whatever the L_eq read at low coverage — letting a 2-second average set the session maximum.
Fixing that needs a coverage gate, which is machinery. **True peak** (L_Cpeak, clause 3.8) is
the impulse quantity, is the only one of the three that must run per-sample in the audio
callback, and against a 70 dB speech ceiling mostly reports chair scrapes.

**Two settings change what MAX means**, and neither is visible in the number itself: L_CFmax
and L_CSmax differ by several dB on speech, and L_CFmax and L_AFmax are not comparable either.
So the max clears on **both** a weighting change and an F/S change, and section
[11](#11-screen-layout-and-presentation)'s header labels both dimensions above it.

### 6.8 Reset

**One button, clearing the max hold *and* the window** (`05` d10). It is not "clear the max",
it is **"start measuring this talk"**.

The window is self-clearing in a window length, so the question is whether waiting is ever too
long. It is, in the actual use case: between talks the room is not quiet, and applause,
chatter, pack-up and PA music sit in the window through the **first full minute of the next
talk**, reading high against the ceiling exactly when the new speaker is being judged. The
refill is honest rather than blank (`12s of 60s` climbing), so resetting costs nothing in
trust. Two buttons would ask Simon to care about a distinction that does not arise at a venue.

**Reset does not clear the spectrogram.** Section [9.3](#93-reset-does-not-clear-the-picture).

### 6.9 Where the app deliberately shows nothing

Two states, both the instrument reporting its own state rather than guessing (`05` d11):

- **NOW is `None` when no block has arrived for 200 ms**, or when the smoothed mean square is
  not positive. Fed zeros, the readout would decay smoothly toward −∞ dBFS, which on screen is
  indistinguishable from *the room went quiet* — a plausible, wrong, unlabelled number, and
  worse than the L_eq case because it carries no coverage figure beside it. Frozen, it is stale
  with no tell. Absent, it is honest. 200 ms is ~9 missed blocks: far outside scheduling
  jitter, fast enough to be honest, slow enough not to flicker.
- **L_eq is `None` at zero coverage**, where `Σn = 0` makes it the log of nothing.

**Max hold is unaffected** by either — a hold is historical by nature, so it simply stops
rising. It is `None` only until the first real level arrives, and after a clear.

### 6.10 The 10 Hz tick, and why it is not slower

One publish at 10 Hz for everything (`05` d12). The audio callback is decoupled by a **bounded
lock-free SPSC queue** of per-block summaries, sized at ~1 s of blocks, so an overflow from a
stalled UI thread **degrades into lost coverage, not a wrong number** — the failure mode lands
in the honest column by construction. There is one thread boundary and no separate metrics
thread.

Because the max compare runs at block rate and the smoother advances by each block's own `dt`,
**the tick's timing affects only what is painted, never what is measured.**

Simon asked whether the L_eq could repaint every 1/5/10 s instead. It could, but it needs
nothing — one tick swaps one slot out of 600:

| the arriving 100 ms slot | L_eq moves by |
|---|---|
| near the window average | ~0.00 dB |
| 10 dB above it | 0.065 dB |
| 20 dB above it (a shout, a dropped chair) | 0.66 dB |

At 0.1 dB resolution the digit therefore changes about once a second **on its own**. The calm
number is the physics of a 600-slot average, not a refresh policy. Slowing the publish would
break three things: the 10 s window has 100 slots and is *supposed* to be lively, coverage
climbs 1 s per second so a 10 s publish would show `33s of 60s` when it is really 43, and after
a Reset the display would sit on the old number. The internal tick cannot go below 10 Hz
regardless — the drain is what block-rate max tracking and the 200 ms threshold need.

### 6.11 What clears what

Verbatim from `05`, with `08`'s fourth column. **Several rows read as bugs if unexplained**, so
they are explained above: an F/S change clears the max but not the window (§6.7), a rate change
clears neither (§4.2 item 6), and a calibration change clears nothing at all (§8.2).

| | window | max hold | filter state | spectrogram ring |
|---|---|---|---|---|
| Reset button | ✓ | ✓ | — | — |
| Weighting change (C/A/Z) | ✓ | ✓ | zeroed | — |
| F/S change | — | ✓ | — | — |
| Window length change | — | — | — | — |
| Sample-rate change | — | — | zeroed | — |
| Calibration offset change | — | — | — | — |

**The fourth column is entirely empty, and that is the point** (`08` correction). Nothing
clears the spectrogram's column ring: not Reset, not a weighting change (the picture is
unweighted), not F/S, not a window-length change (a re-slice), not an offset change; and a rate
change produces gap columns by construction exactly as it does for the energy ring.

---

## 7. Spectrogram

**A scrolling spectrogram — horizontal, time flowing right→left with *now* at the right edge,
drawn on 32 fixed one-third-octave bands** (`07`). It is a spectrogram, **not** a spectrum:
Decibel X's instantaneous bars are explicitly not this. The reasoning is division of labour —
the number already answers *how loud*, so the only thing the display uniquely adds is *what is
making it and whether it has been steady*, and a constant HVAC source and a passing door slam
are indistinguishable on bars and unmistakable on a spectrogram.

The line that summarises the form: *the display draws what an FFT can actually deliver, at a
colour scale that means the same thing on every device.*

### 7.1 Parameters

| | | Why |
|---|---|---|
| Analysis | **N = 8192**, **Hann**, one frame per 100 ms slot | At N=2048 everything below **126 Hz** is bin-borrowing — the whole dB(C) region (`07` f2). N=8192 gives 5.86 Hz bins and resolves every band from 16 Hz up, at a 171 ms frame. |
| Rows | **32 fixed one-third-octave bands, 12.5 Hz–16 kHz nominal** | Nothing is fabricated (one starved band vs 54 of 220 for a pixel-resolution log axis), **and the colour scale stops moving** — energy-summed rows scale with row bandwidth, so 24 rows read ~10 dB hotter than 220 on the identical signal (`07` f3, f4). |
| Cell value | **Band energy — the sum of bin powers in the band, never their mean** | Pink noise has equal energy per third-octave by definition, so an honest display draws it flat. `sum` does, within 1.5 dB above 32 Hz; `mean` invents a **32 dB roll-off** (`07` f1). |
| Columns | **one per 100 ms slot**, the same slots as the energy ring | Free rather than a new cadence. |
| Span | **follows the L_eq window** — 10/30/60/120 s | The picture is then what is inside the number, there is no new setting, and two time axes on one screen cannot disagree (`07` d8). |
| Weighting | **always raw and unweighted, in every meter mode** | See §7.2. |
| dB window | **fixed 60 dB span, shifted by the calibration offset. No auto-ranging.** | The same colour always means the same absolute level. Auto-ranging would make a quiet room and a loud one look identical, which is the one thing a picture of levels must not do (`07` d9). |
| Colour | **a continuous perceptual ramp, monotonic in lightness (inferno family). Never `jet`.** | Survives being read at an angle in a dim room; `jet` invents banding the data does not have. A quantised 10 dB ladder was built and rejected — reading levels is the number's job, and it turned the noise floor into blocks (`07` d10). |
| Bin 0 | **never drawn** | It carries the microphone's DC bias, since `04` declined a DC blocker, and the standard's band starts at 10 Hz regardless. |

Nominal band centres, 32 of them: 12.5, 16, 20, 25, 31.5, 40, 50, 63, 80, 100, 125, 160, 200,
250, 315, 400, 500, 630, 800, 1000, 1250, 1600, 2000, 2500, 3150, 4000, 5000, 6300, 8000,
10000, 12500, 16000 Hz. Exact centres are `f(n) = 1000·10^(0.1(n−30))` for `n = 11 … 42` (IEC
61260 base-10); band edges are `f_c · 10^(±0.05)`. The 20 kHz nominal band is dropped because
its upper edge exceeds 20 kHz.

The **fixed dB window** is `−90 … −30 dBFS` uncalibrated, which is ≈ 10 → 70 dB SPL per band
once section [8](#8-calibration)'s ~+100 dB offset is applied. It is the value most likely to
want moving once real speech in a real room goes through it (`07` residual risk).

### 7.2 The display is always unweighted, in every meter mode

**This is the one thing that must be said out loud**, because a reader who compares the picture
band-by-band against a dB(A) or dB(C) reading will conclude the app is inconsistent. It is not:
**the picture and the number are deliberately different quantities.**

Measurement is what made it obvious: with A-weighting applied per bin, the 31.5 Hz band fell
from −45 to −92 dBFS and **the rumble stripe vanished from the picture entirely** (`07` f5).
That is arithmetically correct and diagnostically backwards — **dB(A) mode is exactly when you
want to know the rumble is there, because the number has stopped telling you.**

Two further payoffs: the picture does not change shape when a settings value changes, and the
display and the number stay **two independent pieces of evidence** rather than two renderings
of one.

### 7.3 Drawing rules

- **Columns are aggregated deliberately into the pixel budget, in energy — never decimated by
  the resampler** (`07` d6). 60 s of 100 ms columns is 600 columns into ~350 px; letting the
  canvas drop columns produced a venetian-blind picture, and the blind is not in the sound, it
  is the scaler choosing which 100 ms slots survive. The **frontend** does this aggregation,
  because it is the only side that knows the canvas width (`08`).
- **Time is drawn blended; band edges are drawn crisp** — a band boundary is real and a column
  boundary is not.
- **Drawn 1:1 and unsmoothed**, no canvas smooth-scaling (`09`). This is what makes the gap
  rule below actually work: letting the canvas smooth-scale blends a transparent gap column
  into its lit neighbours and produces a *dim* column, which is exactly the "dead stream reads
  as a peaceful room" failure the rule exists to prevent.
- **A gap is drawn as a hole — background, visibly absent, never a low-level colour** (`07`
  d11). Forced by consistency rather than chosen: §6.9 refuses to publish a fake quiet, and the
  clock-driven ring produces gap slots by construction. **A slot is a gap if it received no
  samples or zero total power**, matching §6.3.
- **The picture is repainted by appending a column and scrolling**, not by redrawing the
  history — redrawing everything costs ~8 ms per frame at 800 columns even in a desktop
  browser, which is wasteful in a phone webview at 10 Hz for a picture that changes by one
  column.
- **No column is emitted until N samples have been buffered** (0.171 s at 48 kHz), so the first
  one or two slots after a start or a rebuild are holes.

---

## 8. Calibration

**One stored broadband offset, shared by all three weighting modes, set by typing what the
proper meter reads while the app sums a fixed 10 s slice of the ring** (`06`).

The scale, so the numbers are concrete: `02` measured an idle room at **−59.6 dBFS** in
`Measurement` mode, and a room like that is plausibly 35–40 dB SPL, which puts the offset on
the order of **+100 dB**. It is not a small trim, and that single fact rules out a nudge-only
gesture (1 000 taps at 0.1 dB from cold). A 70 dB talk would then read about −30 dBFS.

### 8.1 One offset for all three modes

The offset converts **dBFS → dB SPL**, and that conversion is a property of the **microphone
sensitivity and input gain** — hardware, not weighting. Section [5.3](#53-the-filters)
renormalises all three chains digitally to 0 dB at 1 kHz, so they already share an absolute
anchor: a scale established in one mode *is* the scale in the others (`06` d1).

Per-mode offsets got a fair hearing — the phone mic's response is not flat, so the broadband
offset one would actually *measure* differs by mode on the same sound. Rejected on two grounds.
It needs three calibration gestures against a reference meter that probably cannot show all
three modes, Z least of all. And it would **break Z's stated purpose**: per-mode offsets would
force Z and C into agreement on the calibration spectrum, destroying the very comparison Z
exists to provide (§5.1).

Set it in **dB(C)** — the imposed mode and the default — so the residual response error is
smallest where the ceiling is actually judged. More generally: match whatever weighting the
reference meter is showing.

### 8.2 Applied post-log, in Rust

`published_dB = 10·log₁₀(Σp²/Σn) + c`, added **after the log**, to every dB value that crosses
the bridge (`05` d13).

As a **gain before squaring** the ring would hold calibrated energy, and changing the offset
would make everything already accumulated belong to the old calibration — a third reset cause,
firing at the worst possible moment, since the calibration gesture *is* repeatedly nudging the
offset while watching the number against the reference meter.

Post-log the number is identical — `10·log₁₀(Σp²/n) + c` is exactly `10·log₁₀(Σ(g·p)²/n)` for
`c = 20·log₁₀ g`, and `max(xᵢ + c) = max(xᵢ) + c`, so the historical maximum shifts correctly
too. **Nothing resets, and the calibration gesture becomes interactive**: nudge it and the
settled 60 s average moves instantly.

Kept on the **Rust** side rather than in Vue so an uncalibrated number can never cross the
bridge and get rendered by mistake.

**Z reads through the same offset**, so the bypass path exercises the whole calibration chain
end-to-end, which is what §5.1 promised it would be good for.

### 8.3 The 10 s match slice

The offset is computed against a **fixed 10 s slice of the ring, independent of the display's
window setting** (`06` d3) — the last 100 slots. This is free, a direct payoff from §6.5
allocating the ring at maximum length: no second accumulator, no temporarily overriding a user
setting, nothing to restore afterwards, **and the main 60 s meter is never disturbed.**

Better than both alternatives. Using *whatever window is selected* means calibrating against a
120 s average if that is what was left on. Using the *live reading* matches what a hand-held
meter typically displays, but then both numbers are moving and you are aligning two bouncing
targets by eye — and the result would silently depend on whether F or S is selected.

Arithmetic, not policy: at **zero** coverage in that slice the L_eq is undefined, so the
compute step cannot run and the command returns an error. Below that, coverage is displayed and
Simon judges — no refusal.

### 8.4 The procedure

Reproduced verbatim from `06`. What the app's calibration surface must support, in order:

1. Set the weighting to **dB(C)** on the settings sheet — or to whatever the reference meter is
   showing.
2. Put the phone and the reference meter in the same place, pointed at the same **steady**
   sound. Steady matters: the two instruments may not be reporting the same quantity, and only
   a non-changing sound makes them comparable.
3. Wait for the calibration reading to fill its 10 s slice. Coverage is displayed; judge it.
4. Read the reference meter and type its value. The offset is stored immediately, and every
   displayed number — including the settled 60 s average and the max hold — moves at once.
5. Trim ±0.1 dB if the two disagree slightly.
6. **Write the offset number down.** It is the recovery path if the app's data container is
   ever lost, and it costs nothing now.

The **steady sound requirement is load-bearing rather than advisory**: the reference meter's
quantity is unknown, so if it shows an F-weighted live level and we show a 10 s L_eq, the two
agree only when the sound is not changing.

### 8.5 The three entry affordances

- **Typing the reference value is primary.** You read 68.3 off the proper meter and type 68.3;
  the app stores `68.3 − (raw 10 s slice)`. No mental arithmetic, one step from cold, and
  identical whether the offset is unset or already set. The typed value is **bounds-checked to
  ~0–140 dB** — a fat-fingered `683` for `68.3` would otherwise store a ~600 dB offset. The
  *derived offset* itself stays unclamped.
- **±0.1 dB trim** for splitting the difference while watching both meters. 0.1 dB and no
  coarser pair: the reference meter's own resolution is 0.1 dB and a class-2 instrument is
  ±1.5 dB, so trimming more coarsely is theatre and more finely is measuring nothing.
- **The offset is displayed as a number and directly editable.** Not decoration: free
  provisioning expires every 7 days, so the app is re-signed and reinstalled weekly. Installing
  over the top usually preserves the data container — but delete-then-install does not, nor does
  switching machines or regenerating the Xcode project. With the offset visible, recovery is a
  sticky note and a retype; with it hidden, losing the container means being uncalibrated until
  you are next standing beside the proper meter.

**Nudge-only was ruled out by arithmetic**, not preference — see the +100 dB magnitude above.

### 8.6 Uncalibrated is a designed state, not an error

The offset is **`Option<f64>`, not a default of 0**. With `None`, all three numbers display as
raw values labelled **`dBFS`** — not a wrong SPL but a correctly-named *different quantity*,
with the unit carrying the honesty (`06` d4).

Two rejected alternatives, both worth stating so this does not look like an omission:

- **`--` was the consistency play**, since §6.9 refuses to publish untrustworthy numbers. But an
  uncalibrated meter is not *broken*, it is un-scaled, and blanking it means you cannot tell
  capture works at all.
- **A shipped default of ~+100 dB was rejected firmly.** It would read roughly right out of the
  box and make calibration a refinement — at the cost of showing an authoritative-looking number
  **wrong by an unknown amount**, which is the precise failure mode this whole design is
  organised against.

### 8.7 No metadata

Four values stored, and **nothing about the calibration itself** — no date, no record of the
reference value, no mode (`06` d6).

The reasoning is sharper than "resist complexity": **the offset does not go stale with time, it
goes stale with a hardware change.** An offset set six months ago on the built-in mic is
exactly as valid as one set this morning, while one set ten seconds ago is wrong the moment a
headset goes in. A displayed date would direct attention to elapsed time — the wrong variable —
while detecting nothing. And the question a date would be *used* for, "did I retype this after
the last reinstall?", is already answered by §8.5 displaying the offset: unset versus ~+100 dB
is unmistakable.

---

## 9. The Rust ↔ frontend contract

**One event at 10 Hz carrying everything the screen paints, plus one command that returns the
picture's history. The frontend holds no authoritative state at all** (`08`).

The tick is **≈460 bytes, ≈4.6 kB/s** — about **17× under** the 8192-byte threshold at which
Tauri's own IPC switches to its faster bulk path, so it never leaves the fast path.

### 9.1 The wire contract, verbatim

```
event "tick" — Rust's own 10 Hz publish, ≈460 bytes
{
  now_slot: 418752,                    // clock-advanced ring index (§6.2)
  meter: {
    leq: 68.4 | null,                  // rolling L_eq   ┐ all calibrated,
    inst: 71.2 | null,                 // NOW, F or S    │ post-log, in Rust
    max: 74.9 | null,                  // max hold       ┘ (§8.2)
    coverage_s: 41.3,
    cal_leq: 68.1 | null,              // fixed 10 s slice (§8.3)
    cal_coverage_s: 10.0,
    input: "capturing" | "denied" | "unavailable"
  },
  settings: { weighting, time_weighting, window_s, offset_db: 101.4 | null, unit },
  columns: [ { slot: 418752, bands: [...32] } ]   // every real slot since last publish
}

commands  — all settings commands return the new Settings
  set_weighting(weighting)                       // "C" | "A" | "Z"
  set_time_weighting(time_weighting)             // "F" | "S"
  set_window_length(window_s)                    // 10 | 30 | 60 | 120
  set_calibration_from_reference(reference_db)   // Result, 0–140 bounds check
  set_calibration_offset(offset_db)              // Result
  reset()
  get_spectrogram() → Column[]                   // per-slot columns for the current span
```

Field types: `weighting: "C" | "A" | "Z"`, `time_weighting: "F" | "S"`,
`window_s: 10 | 30 | 60 | 120`, `unit: "dB" | "dBFS"`, all dB values `f64` rounded to 0.1 dB,
`slot: u64`, `bands: [f32; 32]`.

**Seven commands, not eight.** `08` decision 9's prose says "five types and eight commands"
while its own contract block lists seven; the seven above are the complete set. There is
deliberately **no `get_settings`** (the first tick arrives ≤100 ms after the listener
registers) and **no clear-calibration command** (uncalibrated is the initial state; a wrong
offset is retyped).

### 9.2 Properties worth stating rather than leaving to be inferred

- **Every dB value crossing the bridge is already calibrated** (§8.2). Do not move the offset to
  the frontend for convenience.
- **The frontend holds no authoritative state.** All four settings *and* the unit ride in
  **every** tick, so the tick is a complete snapshot of everything the UI paints. This extends
  §8.2's footgun-denial from **values to labels**: the unit travels with the numbers, so a value
  can never be painted under the wrong one. Command return values exist for **feel, not truth** —
  a picker tap updates from the authoritative return rather than waiting up to 100 ms. Text
  entry keeps local draft state, which is an input buffer, not a second source of truth.
- **The tick is a timer, not audio-driven.** It publishes with no audio at all, which is what
  makes settings-in-every-tick and the input state safe.
- **Every column carries its absolute slot index, and each tick ships every real column since
  the previous publish.** This is required for **correctness**, not robustness: the clock-advanced
  ring means a late tick — a timer on a phone — has two or three genuinely completed slots behind
  it, and a one-column payload would drop real data on the floor. Columns produced between page
  load and `listen()` registering are lost no matter what the transport is.
- **Gaps therefore need no marker.** A slot index in `(last_drawn, now_slot]` with no column *is*
  §7.3's hole. Explicit `bands: null` entries were rejected as duplicating what the indices
  already say, at 270 wasted entries for a 27 s hole. `09` finding 8 confirmed this: there is no
  gap-marking branch anywhere in the prototype.
- **`now_slot` is what makes the picture's right edge honest.** Without it, a run of gap slots up
  to the present is invisible — no columns arrive, so nothing says the silence is *current*. It is
  the same information the coverage figure carries for the number.
- **Rust owns the picture's history.** A 1200-column ring (120 s max span) at 32 f32 is **153 KB**.
  `get_spectrogram()` returns the columns for the current span, so every event that invalidates the
  canvas is one move — *pull again* — rather than four different repairs. Letting the frontend keep
  its own ring was rejected on the dev loop: the picture would be empty for up to two minutes after
  every reload.
- **`Option<f64>` serializes to `null`.** `--` is `null`, never a sentinel like `-999`.
- **No throttling or coalescing in Rust.** wry pushes scripts into `pending_scripts` before load
  and `evaluateJavaScript` after, so a stalled webview **delays** ticks rather than losing them.
- **Events need no capability entry.** `core:default` already includes `core:event:default`, so
  `CLAUDE.md`'s four-step plugin ceremony does not apply to this design at any point.
- **A binary format would be a pessimisation.** A `Raw(bytes)` payload under 1024 bytes is
  converted to a JSON *array of numbers* and evaled anyway — strictly worse than sending numbers.
- **Types are hand-written in one file, `src/bridge.ts`**, with wire names **snake_case on both
  sides** so a Rust field name and its TS field name are literally the same string. That file owns
  every crossing type plus thin typed wrappers around `invoke`/`listen`, and is the only file in
  the frontend that imports `@tauri-apps/api`. The honest cost: **a renamed field is a runtime
  `undefined`, not a compile error**, because `vue-tsc` cannot see across the bridge. Accepted
  deliberately; `ts-rs` is the escape if the contract grows.

### 9.3 Reset does not clear the picture

Only the window and the max hold (§6.8). **State the reason or it reads as a bug** (`08` d5).

Settled on asymmetry of cost: the number refills honestly and cheaply, but **60 s of rumble
stripe cannot be recovered once wiped**, and the stripe is the entire reason the picture exists.
It also keeps a mis-tapped Reset from getting more expensive.

The consequence, plainly: **shortly after a Reset the number describes this talk and the picture
describes the last 60 s of the room.** Both are labelled, and neither is wrong — §7.2 had already
made the picture a deliberately different quantity from the number. `07`'s "the picture is
literally what is inside the number" must be read as *the same span*, not *the same data*.

### 9.4 The input state is the one place the app speaks up

`input: "capturing" | "denied" | "unavailable"`, read from
`AVAudioApplication`/`AVAudioSession` `recordPermission` — an authoritative answer exists, so
the exact-zeros heuristic is the wrong tool (`08` d8).

Without it, a denied microphone renders as `--` beside `0s of 60s` forever, with nothing
anywhere saying why, **for the one failure the user can actually fix and only from outside the
app.**

**The distinction that keeps this inside the no-warnings rule:** it fires on a **permanent
condition requiring action outside the app, never on a measurement.** §1.1 and §6.9 refuse
warnings *about sound*; §8.6 set the precedent that an unusual state is shown as a
correctly-named state rather than hidden, and `dBFS` is exactly that. A bare `is_capturing`
boolean was rejected for collapsing *fix it in Settings* and *this is a bug or a missing device*
into one message that can only be vague about both — the shape of message that teaches people to
ignore messages.

**Desktop caveat:** per `CLAUDE.md`, `tauri dev` gets microphone access via the responsible
parent process, so **macOS may report `capturing` where iOS reports `denied`**. Decision 8's most
useful state is the one hardest to exercise on the desk.

`"unavailable"` **conflates causes** — stream build failure, no input device, a never-set
category. Detail lives only in the log, which on a device means Xcode.

### 9.5 What makes the frontend re-pull

| event | re-pull `get_spectrogram()`? |
|---|---|
| mount / webview reload | ✓ |
| canvas resize, orientation change | ✓ |
| window length change (10/30/60/120 s) | ✓ |
| calibration offset change | **—** |
| Reset button | — (§9.3) |
| weighting change (C/A/Z) | — (the picture is unweighted, §7.2) |

**The offset row is arithmetic, not an oversight.** §7.1 shifts the dB colour window *by the
offset* and the band values shift with it, so every colour is unchanged — only the legend
relabels. **Calibrating is the one settings act that changes every number on screen and no pixel
of the picture.**

`get_spectrogram` is the only bulk payload: ≈138 KB at a 60 s span, ≈276 KB at 120 s, over the
`ipc://localhost` custom protocol rather than by `eval`, once per redraw-from-scratch.

---

## 10. Settings and persistence

**Four settings, not three** (`05`, `06`). All four are **Rust-owned**; the frontend issues
commands and holds nothing.

| Setting | Values | Default | Clears |
|---|---|---|---|
| Weighting | C / A / Z | **C** | window + max hold, zeroes filter state |
| Time weighting | F / S | **S** | max hold |
| Window length | 10 / 30 / 60 / 120 s | **60** | nothing (re-slice) |
| Calibration offset | `Option<f64>` | **`None`** (uncalibrated) | nothing |

Display resolution is **0.1 dB** and is **not** a setting — see §11.4.

**Persistence is one JSON file written from Rust** (`06` d5): `settings.json` in
`app.path().app_config_dir()`. `serde` and `serde_json` are **already** dependencies and
`app_config_dir()` is core Tauri called from Rust rather than over the bridge, so this needs
**no new dependency, no npm package and no capability entry**.

`tauri-plugin-store` is the wrong shape twice: it is JS-first, so the frontend would own
authoritative state, inverting the ownership model; and it is the full four-step ceremony
`CLAUDE.md` warns about, where omitting the capability entry produces a runtime "not allowed"
rather than a build failure. `localStorage` fails the same ownership test.

Three consequences recorded with it:

- **Write through on every change, not on exit.** The app dies on backgrounding, possibly without
  running shutdown code, so a deferred write is a lost setting. Changes are deliberate settings
  acts and the file is four values.
- **One file, all four settings.** One write, one read at startup.
- **Missing or corrupt falls back to defaults** (`None`, C, **S**, 60 s) with a log line and
  nothing in the UI. A **config-file write failure is logged, not surfaced** — the setting is
  applied in memory regardless; refusing it would be worse.

---

## 11. Screen layout and presentation

**Centred and chrome-free: one dominant number over a wide picture, everything else behind a
single sheet** (`09` d1). Portrait is primary.

```
        NOW · C · slow        ← both dimensions; they govern the live number and MAX
           65.5
            dB
   LCeq 60s        MAX
     66.5         67.0
   60s of 60s                 ← coverage belongs to the L_eq and sits with it
   ┌──────────────────────┐ ┌─┐
16k│ ░░  ░▒░   ░░░  ░▒▒░  │ │█│
 1k│▓███▓████▒▓██▓░ ▒███▓ │ │▒│
125│██████████████████████│ │░│
 16│██████████████████████│ │ │      ← steady = HVAC
   └──────────────────────┘ └─┘
    −60s               now   dB/band
             ⋯                        ← settings · calibration · reset
```

The line that summarises it: *the screen leads with the number that moves and keeps the number
that is judged permanently in view beside it.*

Rejected alternatives, on their own evidence: a **priority stack** is unarguable and that is all
it is — it spends the screen on a hierarchy nobody was confused about. **Inline settings** lost on
cost; three segmented rows are ~250 px in portrait, more than the spectrogram, and putting a
window-length picker under the thumb argues against §5.2's reason for making weighting a settings
value at all. A **calm, no-chrome, no-hue** variant failed its own bet, and that was the useful
result: with 24 half-octave rows and no hue the steady rumble and the speech merged into one haze,
and the display stopped answering the only question it exists for.

### 11.1 The screen leads with the live level — and what that costs

**The large number is NOW, the live level. The rolling L_eq is not.** Simon's call, and it
**inverts `09`'s own premise**, which had said the L_eq "should dominate; everything else supports
it" (`09` d2).

The argument the ticket had not made: **a 60 s L_eq moved 0.0 dB over 8 s of steady talk and never
changed a digit at any display resolution** (`09` f1). At hero size it is dead screen, while the
live reading is what you watch as you walk a room.

**The cost, stated because a reader will otherwise assume the big number is the one that matters:
the number that dominates is no longer the number judged against the 70 dB ceiling.** Nothing is
hidden — the L_eq is on screen with its coverage — but **the screen's emphasis and the screen's
purpose point at different numbers**, and the live value sits several dB above the L_eq much of the
time. This is the largest accepted risk in the layout.

### 11.2 Nothing is behind a tap

The L_eq **with its coverage** and the max hold are **always visible** beside the hero number. The
no-chrome variant hid them to test whether they were desk curiosity; **they are not** (`09` d3).

The coverage figure belongs with the **L_eq**, and the `−60s … now` axis with the **picture**, so
that §9.3's deliberate divergence reads as two labelled axes rather than an inconsistency.

### 11.3 The hero number carries a quantity label, not just its dimensions

**`NOW · C · slow`** (`09` d4). With the hierarchy inverted, the secondary numbers are labelled
(`LCeq 60s`, `MAX`) and the hero was identifiable only by elimination. **One header serves the live
number and the max hold**, because §6.7 ties the max's meaning to the same weighting/time-weighting
pair — so a bare `MAX 72.4 dB` never arises. The L_eq's own label carries the weighting and the
window: `LCeq 60s` / `LAeq 30s` / `LZeq 120s`.

### 11.4 Defaults: `C` / `S` / 60 s / 0.1 dB — and why S and 0.1

- **`S` is the default time weighting**, against `05` d7's recommendation of F-only. Reason in
  §6.6: at F the hero moves **1.72 dB per tick**, further than any rounding step, so display
  resolution barely helps (9.4 → 8.3 digit changes/s from 0.1 to 1 dB). **S is the lever;
  resolution is not.**
- **Display resolution stays 0.1 dB** for all three numbers. Coarsening to 0.5 dB was built and
  **rejected**: resolution is one setting for all three numbers, and the L_eq is already perfectly
  stable at 0.1 dB, so coarsening to calm the live number would **spend a tenth on the
  ceiling-judged L_eq in order to fix a different number.** Tenths matter there.
- **Per-quantity resolution was not taken** — tenths on the L_eq and something coarser on the live
  value would calm the hero for free. Rejected as a second rule for a problem S already handles, but
  it is the **escape** if the hero reads as too busy on real speech.

### 11.5 `--` and `dBFS` are typographic states, not merely strings

**A spec that says "show `--`" and stops will get a redaction bar**: at hero size (7–9 rem) two
dashes render as a pair of solid filled blocks (`09` f4). So:

- **`--` is muted and much smaller than the number it replaces.** This is what makes "designed
  state, not error state" true on a screen rather than in a sentence — and it matters more under
  §11.1 than before, because the hero is now `--` after 200 ms of no audio, where a hero L_eq would
  have been `--` only at zero coverage. **That state went from rare to common.**
- **The primary number is sized for its widest state, not its most common one.** `−108.4` is wider
  than `66.7`, and **the uncalibrated state is the one the app starts life in** (`09` f6, d11).
  Likewise the legend caption `dBFS/band` needs **58 px** where `07` budgeted ~52 and overprinted
  the `now` label.
- Both `dB` and `dBFS` must look **deliberate**, not just the calibrated one.

### 11.6 The input-state line

Two parts: **a label, and an action only when there is one** (`09` d7). A single string made the
two states look like the same kind of message — the shape that teaches people to ignore messages.

| `input` | line |
|---|---|
| `capturing` | *(nothing)* |
| `denied` | **`No microphone access`** over **`Settings ▸ Privacy & Security ▸ Microphone`** |
| `unavailable` | **`Microphone unavailable`** — label alone, nothing to act on |

Styled **in the same family as the unit label and the coverage figure, never as a banner.**

(`09` d7 specifies the `denied` strings verbatim and, for `unavailable`, only "the label alone".
`Microphone unavailable` is this spec's wording — see §16.)

### 11.7 Geometry

- **The spectrogram is ~205 px tall at full width with all three gutters**, sitting under the
  number; its width is whatever is left. Gutters are part of the design, not decoration: **~40 px
  left** for frequency labels, **~20 px bottom** for the time axis, **58 px right** for the colour
  legend.
- **The legend stays.** `07` offered the trade "if space forces the legend out, the picture loses
  its absolute meaning"; **the trade is not taken** (`09` d9) — removing a tap toggle *freed* space,
  and the legend costs width, not the vertical calm the layout is built on.
- **The window length is visible near the picture**, because §7.1 ties the span to it. `LCeq 60s`
  above and `−60s … now` below both carry it.
- **Portrait is primary; one wide reflow serves both phone-landscape and the 800×600 dev window**,
  at `max-height: 700px`. That threshold is set by measurement, not phone geometry: **an 800×600
  desktop window is a *landscape* case, not a portrait one** — it is shorter than a phone is tall,
  so a breakpoint drawn at phone-landscape height (560 px) misses it and the portrait stack pushes
  Reset below the fold (`09` f7). In the wide reflow the picture goes **beside** the number.
- **The portrait layout is width-capped** — in a tall desktop window it centres at phone width
  rather than stretching a portrait stack across 1400 px.

### 11.8 Typography and colour

- **Every number on screen is tabular-figured.** At 10 Hz a proportional digit set makes the whole
  number shuffle sideways when one digit changes, which reads as instability that is not in the
  measurement.
- **The hero number is set in the UI sans, not monospace.** At 7 rem a mono font gives the decimal
  point a full advance width, so `66.8` reads as block-dot-block. The system sans has tabular
  figures too. **Everything smaller stays mono.**
- **Dark, high contrast.** Not a free choice: §7.1 puts an inferno ramp on a near-black field, and
  light chrome around a dark picture fights it.
- **The accent colour never means alarm.** It marks the input-state line and the armed Reset —
  states and affordances, never levels. The test that this holds: **at 73.5 dB the screen is
  identical to 66.7 dB apart from the picture being brighter** (`09` f9). The no-warnings rule
  survives contact with a layout.

### 11.9 The sheet

**One sheet, reached from `⋯`, holding settings *and* calibration** (`09` d8). Calibration is a
rare, deliberate, two-instrument gesture, so folding it in costs nothing and keeps the main screen
at one affordance.

Contents:

1. **Weighting** — C / A / Z picker.
2. **Time weighting** — F / S picker.
3. **Window length** — 10 / 30 / 60 / 120 s picker.
4. **Calibration** — the live 10 s reading with its coverage; numeric entry for the reference
   value; ±0.1 dB trim; the offset shown as a number and directly editable (§8.5).
5. **Reset** — **inside the sheet and a two-step (armed, then confirmed)**, because §6.8 makes it
   discard the window as well as the max hold and this layout has the fewest places for a thumb to
   land safely.

Picker taps take their feedback from the **command return value**, not the next tick (§9.2). If the
controls feel sticky, that is the path to check.

---

## 12. Suggested module layout

Not binding on file names; binding on the seams, which are the ones the tickets designed.

```
src-tauri/src/
  lib.rs          setup, command registration, 10 Hz tick thread, idle timer
  session.rs      AVAudioSession config + read-back + interruption observer   (iOS)
  capture.rs      cpal stream build, supervisor / rebuild, SPSC producer
  weighting.rs    pole derivation, bilinear transform, DF2T cascade, 1 kHz renormalisation
  metrics.rs      energy ring, coverage, F/S smoother, max hold
  spectrum.rs     FFT frame assembly, third-octave banding, column ring
  settings.rs     the four settings, JSON persistence, calibration arithmetic
  bridge.rs       the serde types that cross (mirrors src/bridge.ts)

src/
  bridge.ts       every crossing type + typed invoke/listen wrappers — the ONLY
                  file that imports @tauri-apps/api
  App.vue         the one screen
  components/     Hero, Secondary, Spectrogram (canvas), SettingsSheet
```

**All of the capture spike is deleted** (`08`): `src-tauri/src/spike.rs`,
`src-tauri/src/bin/spike.rs`, the `run_capture_spike` command, and `App.vue`'s harness.
`default-run` in `Cargo.toml` can go with the second binary. **The permission wiring, framework
linking, `tauri.conf.json` changes and `scripts/check-ios-plist.sh` are not throwaway and must
survive.**

---

## 13. Accuracy and limitations

**Read this section before quoting a number from this app to anyone.**

### 13.1 No conformance is claimed

The filters follow IEC 61672's curves; **the app claims nothing**. A phone microphone is not a
class-2 instrument, least of all at low frequency where C-weighting lives. **No class number
appears anywhere in the UI** — and note that even the tolerance tables used during development come
from IEC 61672-1:2002 Ed. 1 via IS 15575-1:2005; Ed. 2 (2013) restructured them and could not be
obtained from a primary source.

### 13.2 The microphone is the binding error source, not the filter

The spec can be specific rather than vague here (`04`, `03`):

- Below 1 kHz the digital filter tracks the standard's analogue design goal to **≤0.005 dB (A) and
  ≤0.0005 dB (C)** at every sample rate from 16 to 192 kHz.
- **All discretisation error lives above 4 kHz** and is always in the direction of *too much*
  attenuation, growing without bound at Nyquist: −0.53 dB at 7.9 kHz, −6.2 dB at 15.8 kHz at 48 kHz.
- **Every row of the standard's 34-frequency table passes the class-1 design band at both 44.1 and
  48 kHz**, for both A and C.
- Prewarping was deliberately **not** applied: it would push error down into the 1–5 kHz band where
  tolerance is ~4× tighter and speech energy actually lives.

So **the low-frequency exposure that dB(C) creates is real, but it lives in the microphone**, which
a broadband offset cannot correct — not in the filter design.

### 13.3 A low-rate route is measured and shown without any indication

The weighting filters need **~40 kHz minimum** (36 951 Hz for class-1 limits, 39 012 Hz for the
design band). **A Bluetooth mic route typically supplies 8–16 kHz**, well under that floor, at which
point the reading is **out of tolerance rather than merely imprecise**.

**The app does not refuse and does not flag it** (`11` d5, `04` d5). Coefficients are designed at
runtime from the actual rate, so at 16 kHz the filter is correctly designed *for* 16 kHz; it simply
cannot meet the standard's tolerance near Nyquist. Refusing or flagging is automation plus a warning,
for a case that requires deliberately measuring through a Bluetooth headset mic. **A log line
remains, for post-hoc debugging only.**

**Residual risk, accepted deliberately: on such a route the displayed number is wrong with no
indication.** Note the tension with §6.4, which exists precisely to avoid silently-wrong numbers —
the difference is that a gap happens *to* you unbidden, whereas a Bluetooth mic route is something
you have to go out of your way to create.

### 13.4 A changed microphone silently invalidates the calibration offset

**No detection, no staleness flag** (`11` d6). Built-in and headset microphones can differ in
sensitivity by **tens of dB** — far more than the sample-rate error above — and the reading would be
wrong with nothing to suggest it. **This is the largest single risk in the design**, accepted
deliberately on the same reasoning: plugging in a headset mic is a deliberate act, not something
that happens to you.

The offset is only valid for the input it was set on. §8.4 says so; nothing at runtime enforces it.

### 13.5 A broadband offset cannot correct frequency response

One offset makes the number honest **on average, not per band** — and on a phone mic the response
error is worst in the low end, **which is where dB(C) lives**. Per-frequency calibration curves would
genuinely improve this and are out of scope: they need a known reference signal and a real procedure.

**A band colour is therefore not a calibrated band SPL.** The single offset scales the picture as
honestly as it scales the numbers and no more — the same limitation as above, now with a per-band face
(`07`).

### 13.6 The offset is only as good as the reference meter

A class-2 instrument is **±1.5 dB** and the offset inherits that error **wholesale**, while the app
displays 0.1 dB. **The precision of the display is not the accuracy of the reading** (`06`).

### 13.7 Linearity is verified over 20 dB, and not at the top of the range

The single-offset model rests on the input path being linear, which was **measured**: −57.7 and
−37.8 dBFS for source levels 20 dB apart, a delta of **19.9 dB against 20 expected**. Room noise
cannot account for even that 0.1 dB — it would have had to sit ~16 dB below −57.7 dBFS.

**But the test brackets −57.7 to −37.8 dBFS, and limiting engages near the *top* of a range.** A
70 dB talk plausibly reads about −30 dBFS, which is ~8 dB **above** the highest tested level — the
untested region is exactly where limiting would live if any survived `Measurement` mode. One further
run 20 dB up would bracket the real operating point. Not blocking (`06`).

### 13.8 There is no coverage floor

**`68.2 dB · 1s of 60s` will display.** Honest, but it is a one-second average wearing a
sixty-second label, and **only the coverage figure says so** (`05`). Accepted deliberately: the
instrument reports its state and the reader judges.

### 13.9 `Measurement` mode is a precondition, not a refinement

21 dB of processing gain without it (§3.2), so **a stored offset means nothing if the mode is not
fixed**. The mode is read back and a mismatch logged, with nothing in the UI.

### 13.10 A free-provisioning reinstall can lose the stored offset

Installing over the top usually preserves the app's data container; **delete-then-install does not**,
nor does switching machines or regenerating the Xcode project. Mitigated rather than solved: the
offset is **displayed so it can be written down and retyped in seconds** (§8.5). **Keep a written
copy** (`06`).

### 13.11 Measurement is foreground-only

Capture does not survive backgrounding (§4.3). The idle timer is disabled so the screen does not
sleep mid-talk, but leaving the app stops measurement.

### 13.12 The lowest band on the picture is the one band the picture cannot honestly draw

At N=8192 the **12.5 Hz band is narrower than one bin** (2.9 Hz against 5.86 Hz) and is interpolated
from the nearest bin. N=16384 resolves all 32 bands, at the cost of a 341 ms frame; there is no
evidence yet that infrasound at 12.5 Hz matters more than that costs (`07`).

### 13.13 At a 120 s span the picture is about texture, not events

Syllable structure is compressed into texture at 1200 columns. A consequence of the span following
the window rather than a defect — someone who chooses a two-minute average has already chosen to stop
looking at seconds (`07`).

### 13.14 Untested, and known to be so

- **Arm's-length legibility in a dim venue.** The layout reached the device, but **no verdict has
  been given** on whether ~6 px per band (32 bands over ~205 px) reads at distance, and the same
  question applies to the muted `--` at hero size. If it fails, the fix is a correction to the band
  count or the ~200 px height. **Cheap to answer — the build is installed.**
- **A real microphone and a real room.** Every prototype number came from synthetic or scripted
  sources. The parts most likely to want revisiting are the colour window's default (§7.1) and the
  *feel* of the hero number on real speech.
- **The `get_spectrogram` snapshot on a phone.** 276 KB of JSON at a 120 s span parses in a few ms on
  a desk; nobody has run it in a WKWebView on an iPhone. The fallback is to pass a pixel budget into
  `get_spectrogram` and aggregate in Rust — which costs the single-aggregator property, so it is a
  trade rather than a fix.
- **`"denied"` on macOS** (§9.4), and **route-change recovery** (§4.2 item 5 — probe 3 was never run;
  do it with headphones to hand).
- **A signed macOS build with the `audio-input` entitlement** (§2.1).
- **The 200 ms staleness threshold is reasoned, not measured.** iOS drain jitter was never
  characterised, so if `--` flickers in practice, that number is the dial.
- **Hand-written bridge types drift silently** (§9.2).

---

## 14. Validation and tests

### 14.1 The weighting filters — a `cargo test` module, no new tooling

Hand-rolled biquads with runtime coefficient derivation *need* research `03`'s 34-frequency table as
a test, and it is a pure Rust test: `cargo test` plus a `#[cfg(test)]` module adds **no dependency, no
config and no tooling decision**, so `CLAUDE.md`'s "ask before adding a test runner" does not apply
(`04`).

The table is [`research/03-iec-weighting-filters.md`](research/03-iec-weighting-filters.md) §4.2 —
34 rows × A/C with exact frequencies, analogue design-goal values, published table values, class-1 /
design-band / class-2 tolerances, and measured bilinear deviations at 44.1 and 48 kHz.

How to run it, and the traps:

1. **Use the exact one-third-octave frequencies, not the nominal labels.**
   `f(n) = 1000·10^(0.1(n−30))`, `n = 10 … 43`. Testing at 12.5 Hz instead of 12.5893 Hz produces a
   spurious 0.18 dB "failure"; at 16 Hz it is 0.28 dB. **This is the single most likely cause of a
   confusing red test.**
2. **Discard the transient.** Feed ≥3 s and measure the RMS of the last 2 s. The f₁ double pole's
   time constant is ~7.7 ms but settling to 0.001 dB takes far longer. **The second most likely
   cause.**
3. **Amplitude is irrelevant in f64** — use a full-scale sine (RMS = −3.0103 dBFS, so expected output
   is `−3.0103 + W(f)`). Also run one pass at amplitude 1e-3 to catch precision regressions.
4. **Two levels of assertion**, so a real regression is distinguishable from a tolerance question:
   - **Tight** — digital response vs the *analogue* design-goal value, **±0.01 dB below 4 kHz** and
     per the table's measured deviations above it. This is the regression test.
   - **Loose** — digital response vs the *published table* value, within the design band (class-1
     limit minus the Table A.1 uncertainty). This is the standards-shaped test.
5. **Two design invariants with tight tolerances**, because they catch a transposed constant that a
   sweep comparison would smear over: **C = −3.010 dB at f_L = 31.6228 Hz and at f_H = 7943.282 Hz.**
6. **`|H(1000 Hz)| = 0.000 dB` to within 1e-9** for both A and C (this is what the digital
   renormalisation buys), and **`|L_C − L_A| < 1e-9` at 1 kHz** (clause 5.4.14, free).
7. **Z returns the input unchanged.**

Sample-rate coverage: derive and assert at **16, 44.1, 48 and 192 kHz**, since coefficients are a
runtime function of the rate.

### 14.2 The metrics pipeline

Synthetic, deterministic, no hardware:

- A known steady tone for `T` seconds gives a known L_eq; **coverage must read `T of window`** while
  filling.
- **Feed nothing for 27 s of a 60 s window** and assert `33s of 60s` with the L_eq of the real 33 s,
  **not** the −2.6 dB that folding gaps in as silence would give.
- **Feed exact zeros** and assert `--` beside `0s of 60s` — not `60s of 60s` (§6.3).
- **No block for 200 ms** ⇒ NOW is `null`; max hold does not change.
- **Every row of §6.11's table**, which is cheap and is where a plausible-looking implementation goes
  wrong.
- **Window length change re-slices**: 60 → 120 s must read `60s of 120s`, not `0s of 120s`.
- **Offset change resets nothing** and shifts all three numbers by exactly the delta.

### 14.3 The spectrogram — two eyeball tests, both of which caught real errors

Neither needs hardware (`07`, and the map's dev-loop-fidelity note):

- **Pink noise must draw flat.** It is equal energy per third-octave by definition, so a sloped
  picture means the band summarisation is wrong — this is what caught `mean` inventing a 32 dB
  roll-off.
- **An exponential sweep must draw a straight diagonal.** A curve means the log axis mapping is
  wrong. Below ~40 Hz the sweep fans into horizontal streaks, which is bin-borrowing made visible and
  is expected.

### 14.4 Desktop dev-loop fidelity

With biquads settled, **the filter needs no hardware and no reference meter** — synthesise tones in
Rust at the exact one-third-octave frequencies and assert against §14.1. What remains genuinely open
is only the **acoustic** end: there is no desk-side way to know a real sound's true SPL, which is a
calibration question (§13.6), not a DSP one.

### 14.5 The frontend

**No test runner is added.** Nothing so far requires one, and `src/bridge.ts` is the only untested
seam that matters — accepted deliberately in §9.2, with `ts-rs` named as the escape. Typechecking
happens via `npm run build` (`vue-tsc --noEmit`), which **cannot see across the bridge**.

---

## 15. Open questions this spec does not close

None block implementation. Listed so they are not mistaken for oversights.

- **Arm's-length band legibility** (§13.14) — the one question the prototypes handed on. Cheap to
  answer; if the answer is "no", it is a correction to §7.1's band count or §11.7's height.
- **Free-provisioning friction.** Signing works, but the 7-day expiry has never been hit, so the
  re-signing workflow is unrecorded. §13.10 is the consequence that is already mitigated.
- **A signed macOS build with the `audio-input` entitlement** (§2.1) — macOS is dev-only, so low
  priority, but capture will break silently in a signed bundle if the entitlement is wrong.
- **Logging and export** — deliberately out of scope, and the most probable *next* effort. If whoever
  imposed the dB(C) limit ever asks for evidence, this is what they want.

---

## 16. What this spec decides that no ticket decided

Small, mechanical, and listed so they can be vetoed in review rather than discovered in code.

1. **FFT crate: `realfft`** (which wraps `rustfft`). No ticket named one — `07`'s prototype used a
   hand-rolled TypeScript FFT. Real-input transforms are ~2× the throughput of a complex one at
   N=8192, and this is the only heavy arithmetic in the app.
2. **SPSC queue crate: `rtrb`.** `05` d12 requires a bounded lock-free SPSC queue but named no crate.
   Hand-rolling one correctly is not a 15-line job the way the biquad recursion is, and `rtrb` is
   small and purpose-built for exactly the audio-callback boundary.
3. **`objc2-foundation` as a direct dependency**, version-matched to `objc2-avf-audio`'s constraint so
   the objc2 graph does not duplicate — needed for the `NSNotificationCenter` observer in §4.2.
4. **The FFT's power normalisation:** with `S1 = Σ w[n]`, one-sided bin power is
   `P_k = 2·|X_k|² / S1²`, so a full-scale sine at a bin centre reads its own mean square (−3.01 dBFS)
   — **the same dBFS convention as the meter**, which is what lets one calibration offset shift both.
   `07` measured with its prototype's own normalisation and never wrote one down; §7.1's −90 … −30 dBFS
   colour window is stated against this one.
5. **Band assignment:** sum the power of bins whose **centre** falls in `[f_lo, f_hi)`; if no bin
   falls in a band, take the nearest bin's power (the "borrow" that makes the 12.5 Hz band
   interpolated, §13.12).
6. **The 10 Hz tick is a dedicated std thread** with a sleep-until loop on `Instant`, emitting via
   `AppHandle`. `05` and `08` require the cadence and its ownership but not the mechanism.
7. **NOW publishes `None` when the smoothed mean square is not positive**, in addition to `05` d11's
   200 ms staleness rule. Derived, not invented: exact-zero blocks arrive, so the staleness rule never
   fires, and `10·log₁₀(0)` must not reach `serde_json`. The same reasoning as `09`'s zero-power
   correction, applied to the smoother instead of the ring.
8. **A gap column is a slot with no samples *or* zero total power** (§7.3), for consistency with the
   same correction — otherwise a denied mic draws a picture at the bottom of the colour window, which
   reads as a very quiet room.
9. **Channel 0 only when the stream reports more than one channel** (§3.3). Averaging would change the
   level by up to 6 dB and make calibration depend on channel count.
10. **`Microphone unavailable`** as the `unavailable` label (§11.6). `09` d7 specified the shape and the
    `denied` strings but not this one.
11. **Config file name `settings.json`** in `app_config_dir()`.

---

## 17. Corrections this spec carries

Places where an earlier ticket's text is superseded. The spec states the corrected version above; this
list exists so a reader who goes back to a ticket is not misled.

| Corrected | Correction |
|---|---|
| `04`'s "the window resets on a weighting change" | **The max hold clears too** (`05` d8) — L_CFmax and L_AFmax are no more comparable than LCeq and LAeq. |
| `04`'s "mark the transition as gap slots" on a rate change | **Zero code** (`05`) — a clock-advanced ring produces gap slots by construction. |
| `05` d7's *reason* for recommending F-only (not its outcome) | Refuted by measurement: S moves **20× further per tick** than the 10 s L_eq it was compared to, so it is not "nearly the same perceptual slot" (`09` f3). **And S is the default** (`09` d5). |
| `05` d11's premise that `--` and zero coverage arrive together | **A zero-power block is a gap slot, not a covered one** (`09`) — exact zeros mean `Σn > 0` while the level is undefined. A denied mic reads `0s of 60s`, not `60s of 60s`. |
| `05`'s reset/clear table | **Gains a fourth column, entirely empty** (`08`) — nothing clears the spectrogram ring. |
| `06` d5's default list `(None, C, F, 60 s)` | **`(None, C, S, 60 s)`** — `09` d5 changed the default time weighting after `06` was written. |
| `07` d8's "the picture is literally what is inside the number" | Read as **the same span, not the same data** (`08` d5) — Reset does not clear the picture. |
| `07`'s ~52 px legend gutter | **58 px** (`09` f6) — `dBFS/band` is four characters longer than `dB/band` and overprinted the `now` label. |
| `07`'s "if space forces the legend out" trade | **Not taken** (`09` d9) — the legend stays. |
| `08` d9's "five types and eight commands" | **Seven commands** — its own contract block lists seven (§9.1). |
| `09`'s own Question, "the rolling L_eq should dominate" | **Inverted** (`09` d2) — the hero is the live level, and §11.1 states the cost. |
| `CLAUDE.md`'s note that `src-tauri/gen/` is generated | Already fixed: `gen/schemas` is off-limits, `gen/apple/` is tracked and editable but must stay regenerable. |

---

## 18. Provenance

| Section | From |
|---|---|
| 2, 3 | [`01`](issues/01-native-audio-capture-path.md) · [`02`](issues/02-ios-capture-device-spike.md) · [`research/01`](research/01-native-audio-capture-path.md) |
| 4 | [`11`](issues/11-interruption-and-gap-handling.md) |
| 5 | [`03`](issues/03-iec-weighting-filters.md) · [`04`](issues/04-weighting-architecture.md) · [`research/03`](research/03-iec-weighting-filters.md) |
| 6 | [`05`](issues/05-level-metrics-pipeline.md) |
| 7 | [`07`](issues/07-spectrogram-form.md) |
| 8 | [`06`](issues/06-calibration-model.md) |
| 9, 10 | [`08`](issues/08-rust-frontend-boundary.md) |
| 11 | [`09`](issues/09-screen-layout.md) |
| 13 | all of them |
