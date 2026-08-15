# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project state

A Tauri 2 + Vue 3 + TypeScript + Vite app. The planning effort is **closed**: its destination is `.scratch/spl-meter-mvp/spec.md`, which is now the authority for *what* to build. The reasoning behind every decision lives in that map's `issues/` and `research/`, two links away — the spec cites the ticket that made each call rather than reproducing the argument.

**Open implementation work lives on GitHub Issues** — the map is [#1](https://github.com/thezic/brus-spl/issues/1) and the open tickets are its sub-issues. **Read that map issue before starting work.** Its tickets produce code, not decisions, and no decision in the spec gets re-litigated there. See `docs/agents/issue-tracker.md` for the mechanics.

**`.scratch/spl-meter-build/` is the archive of the same effort** — `map.md` holds the Route, the Drop order and ~400 lines of Decisions-so-far, and `issues/b01`–`b13` are the closed tickets in full. Read it too: it is where the reasoning is, and the spec cites it by path. The two numbering schemes (`01`–`11` for the mvp map, `b01`–`b15` here) are deliberately distinct, and a third now exists — a bare `#N` means a GitHub issue, while `bNN` and `NN d<n>` mean **read the file**.

The capture spike is **gone**, deleted by `b01`, which replaced it with `session.rs` + `capture.rs`. `b06` built the real screen: `src/App.vue` plus six components under `src/components/`, with the strings that sit beside a number in `src/display.ts`. **Tier 1 is closed**, device pass included. **Tier 2 is closed**: `b09` added `spectrum.rs` — the FFT tap, the third-octave banding and the 1200-column ring — `b10` put it on the wire, and `b11` drew it (`Spectrogram.vue` + `spectrogram.ts`), on the phone included. `b12` rehearsed the re-sign and install — read its resolution before signing, because **a rebuild does not re-sign**.

**The venue run (`b13`) is closed: the talk was read and the app worked.** So the map's destination is reached, and §13.14's legibility questions are all answered — ~6 px per band over 32 bands reads fine at arm's length **and in a dim room**, so the band count and the ~205 px height stand. **The app also has its first real calibration**, `+116.7 dB` against a reference meter; every offset before it was a guess, because there was no calibration tool at the desk.

**Both of the talk's two asks are now built** — neither was a thing it found broken. [`b14` = #2](https://github.com/thezic/brus-spl/issues/2) calmed the live number, but **not the way §11.4's escape said to**. Coarsening it to 1 dB was built and reverted on the owner's call — *the decimal stays*. The busyness is a **repaint rate**, not a precision: §6.6's measured 0.351 dB per tick changes a digit on essentially every one of the ten ticks a second, so the hero now repaints at **2 Hz and keeps its tenth**. `LIVE_REFRESH_MS` in `src/components/Hero.vue` is the one constant. **Display resolution is 0.1 dB everywhere again**, so `display.ts` rounds nothing. Two things there are load-bearing: `--` is exempt from the throttle in both directions, because holding a stale number past §6.9's 200 ms threshold is exactly what that rule forbids; and this is **not** §6.10's publish rate — the tick stays at 10 Hz or coverage, Reset and max tracking all break. [`b15` = #3](https://github.com/thezic/brus-spl/issues/3) removed the colour legend on the owner's call and gave the plot 56 of its 58 px, and the frequency axis now carries **the octave ladder, eleven labels where there were four**. **Both carried spec corrections** (§11.4 and §11.7, logged in §17); `b15`'s reverses §11.7's *"the legend stays"*. **Do not answer `b15`'s wish for a finer picture by adding bands** — energy-summed rows scale with bandwidth, so finer bands shift every cell ~3 dB and would re-derive §7.1's `−90 … −30 dBFS` colour window, which `b13` just confirmed is correct. The map's own note stands: a **readout** on the picture is what "pinpoint problematic frequencies" actually wants, and it is new scope and a real decision — use the wider plot at one more talk first.

No linter or formatter is configured beyond `cargo clippy`/`cargo fmt`, and there is **no frontend test runner** — `b02` added `cargo test` with a `#[cfg(test)]` module, which needs no tooling decision. Ticket `10` closed the question deliberately: no frontend runner is added, and `src/bridge.ts` is the single accepted untested seam. Ask before adding one.

## Agent skills

### Issue tracker

GitHub Issues on `thezic/brus-spl`, via the `gh` CLI — with the closed `.scratch/` history
excepted. See `docs/agents/issue-tracker.md`.

### Triage labels

The five canonical roles, label strings unchanged. See `docs/agents/triage-labels.md`.

### Domain docs

Single-context — root `CONTEXT.md` + `docs/adr/`, neither of which exists yet. See
`docs/agents/domain.md`.

## Commands

```bash
npm install                # install JS deps (Rust deps resolve on first cargo build)

npm run tauri dev          # primary dev loop: starts Vite on :1420, then the Tauri window
npm run tauri build        # production bundle (all targets per tauri.conf.json)

npm run dev                # Vite only, browser at :1420 — Tauri `invoke` calls will fail here
npm run build              # vue-tsc --noEmit (typecheck) + vite build
```

Rust-only checks, run from `src-tauri/`:

```bash
cargo check
cargo clippy
cargo fmt
cargo test                     # 122 tests; hardware-free and instant, deliberately
cargo test -- --ignored        # the one test that opens the real microphone (b07's supervisor)

cargo check --target aarch64-apple-ios --lib   # typechecks the iOS-only AVAudioSession code
```

That last one matters: the `AVAudioSession` setup is behind `cfg(target_os = "ios")`, so a plain `cargo check` skips it entirely and will not catch a break there. Needs `rustup target add aarch64-apple-ios`.

Typechecking the frontend happens only via `npm run build`; there is no standalone typecheck script.

**The iOS target must link `AVFAudio`, `AudioToolbox` and `CoreAudio` explicitly.** They are
listed in `bundle.iOS.frameworks` in `tauri.conf.json`. Rust builds as a staticlib
(`libapp.a`) and Xcode does the final link, so the `#[link(name = "…", kind = "framework")]`
attributes in `objc2-avf-audio`, `objc2-audio-toolbox` and `objc2-core-audio` never reach
that link step — Tauri's own hardcoded framework list covers only UIKit/WebKit/Metal and
friends. Without them the build fails at `Ld` with `Undefined symbols for architecture
arm64` naming `_AVAudioSessionCategoryRecord`, `_AudioComponentFindNext` and similar. Note
this compiles and `cargo check`s perfectly happily; only the Xcode link catches it.
**Changing `frameworks` requires regenerating the project** —
`rm -rf src-tauri/gen/apple && env -u FORCE_COLOR npx tauri ios init`.

**A rebuild does not re-sign, and free provisioning gives you 7 days from the profile's
*creation*.** `tauri ios build` reuses whatever is cached in
`~/Library/Developer/Xcode/UserData/Provisioning Profiles/` and says nothing, so a build made this
morning can embed a profile that dies tomorrow — the failure is silent until the app refuses to
launch. Read the expiry rather than assuming it:

```bash
unzip -p src-tauri/gen/apple/build/arm64/decibel-meter.ipa \
  "Payload/decibel-meter.app/embedded.mobileprovision" > /tmp/prov.plist
security cms -D -i /tmp/prov.plist | plutil -p - | grep -E "CreationDate|ExpirationDate"
```

To force a fresh one: move that profile out of the cache directory and rebuild — Xcode issues a new
7-day profile during the build, no prompt and no paid account (`b12`, 48 s end to end). The signing
*certificate* is valid for a year, so this is never a certificate problem.

**The stored calibration offset can be pulled off the phone and pushed back**, which is the real
answer to spec §13.10:

```bash
xcrun devicectl device copy from --device <udid> \
  --domain-type appDataContainer --domain-identifier net.thezic.decibel-meter \
  --source "Library/Application Support" --destination ./backup     # `--source .` fails
xcrun devicectl device copy to   --device <udid> \
  --domain-type appDataContainer --domain-identifier net.thezic.decibel-meter \
  --source ./backup/net.thezic.decibel-meter/settings.json \
  --destination "Library/Application Support/net.thezic.decibel-meter/settings.json"
```

The app reads it at the next launch and prints what it loaded to stderr, so a restore is verifiable
with `idevicesyslog -m "settings:"`.

**`devicectl device orientation` is simulator-only** — it answers `CoreDeviceError 1001` on a real
phone, so a rotation/reflow test needs a hand on the device.

**A locked phone cannot be launched onto.** `devicectl device process launch` fails with
`ERROR … CoreDeviceError 10002` / `RequestDenied` / `Unable to launch … because the device was not,
or could not be, unlocked`. It reads like a signing or provisioning failure and is not one — unlock
the phone and re-run. `devicectl device install app` works fine on a locked device, so the install
succeeding is not evidence that the launch will.

**Never run `tauri ios init` with `FORCE_COLOR` set in the environment.** Use
`env -u FORCE_COLOR npx tauri ios init`. Tauri's generated "Build Rust Code" script phase
ends with `... --configuration ${CONFIGURATION:?} ${FORCE_COLOR} ${ARCHS:?}`, where Tauri
intends `FORCE_COLOR` to hold its own `--force-color` *flag*. npm and many agent/CI shells
set the same variable name to a colour *level number* instead. XcodeGen substitutes
environment variables when it writes `project.pbxproj`, so a `FORCE_COLOR=3` in the
generating shell bakes a literal `3` into the script, which cargo-mobile2 then parses as an
architecture and fails with the misleading `Arch specified by Xcode was invalid. {arch}
isn't a known arch` (note the unsubstituted `{arch}` — a Tauri formatting bug, so the
message never names the offending value). The same applies at *build* time: `ios dev` and
`ios build` should also be run with it unset. Check with `echo $FORCE_COLOR` before
regenerating; recover with `rm -rf src-tauri/gen/apple && env -u FORCE_COLOR npx tauri ios init`.

**`@tauri-apps/cli` is pinned to an exact version, not `^2`.** The iOS `Info.plist` merge order is undocumented and changed silently between tauri-cli 2.4 and 2.9, and a missing `NSMicrophoneUsageDescription` is a launch-time process kill rather than a build error. After bumping it — or after any `tauri ios build` — run `./scripts/check-ios-plist.sh` to assert the key survived the merge.

## Architecture

Two processes, one repo:

- **Frontend** (`src/`) — Vue 3 SFCs with `<script setup>`, mounted in `src/main.ts`. TypeScript is `strict` with `noUnusedLocals`/`noUnusedParameters`, so unused bindings fail the build.
- **Rust backend** (`src-tauri/src/`) — `main.rs` is a thin shim that calls `run()` in `lib.rs`; all setup belongs in `lib.rs` (the split exists so mobile targets can share the lib entry point). The crate is named `decibel_meter_lib`.

The two communicate over Tauri commands: a `#[tauri::command]` fn registered in `tauri::generate_handler![]`, called from the frontend with `invoke("name", { args })` from `@tauri-apps/api/core`.

### Things that bite

- **`src-tauri/capabilities/default.json` gates everything.** Tauri 2 denies plugin/core APIs unless the corresponding permission is listed there. Adding a plugin means: add the Cargo dependency, register it with `.plugin(...)` in `lib.rs`, add the npm package, *and* add its permission to the capability file. Missing the last step produces a runtime "not allowed" error, not a build failure.
- **The Vite port is fixed at 1420 with `strictPort: true`** to match `devUrl` in `tauri.conf.json`. A stale process on that port breaks `tauri dev` outright.
- **`src-tauri/gen/` is generated, but only `gen/schemas` is off-limits.** Don't hand-edit the JSON schemas there — it's gitignored and rebuilt every compile. `gen/apple/` (and `gen/android/`) are different: Tauri's own `ios init` template ships a `.gitignore` scoping what *inside* them to ignore, `src-tauri/.gitignore` excludes only `gen/schemas`, and the Tauri docs instruct you to edit files there directly (`PrivacyInfo.xcprivacy`, `Assets.xcassets/AppIcon.appiconset/`, deployment targets in `project.pbxproj`). So they are tracked and editable.
  Even so, **keep `gen/apple/` free of anything we can't regenerate**, so `rm -rf src-tauri/gen/apple && tauri ios init` stays a safe recovery move. Microphone permission, entitlements, and the development team all belong in `src-tauri/Info.plist` / `Entitlements.plist` / `tauri.conf.json`, which are re-merged on every `ios dev`/`build`/`run` — not in the generated plist. Expect build-time git churn: each iOS build rewrites `gen/apple/<AppName>_iOS/Info.plist` and `gen/apple/<AppName>.xcodeproj/project.pbxproj`.
- Vite ignores `src-tauri/**` for HMR; Rust changes are picked up by Tauri's own watcher and trigger a recompile + app restart.
- **Rust's `eprintln!` *does* reach the device log — read it with `idevicesyslog`.** `println!` does not reach `devicectl … --console` (spec §2.2), which was read for two tickets as meaning on-device diagnostics must be on screen. They need not: `idevicesyslog -m "capture:" -m "session:"` streams the app's stderr verbatim, tagged `[stderr]`, from a plain shell with no Xcode. That is how `b08` answered the `Measurement`-mode read-back. (`idevicescreenshot` from the same package does *not* work here — it wants the developer disk image over the classic lockdown path.)
- **`inputmode="decimal"` on iOS offers the locale's decimal separator and no other.** On a comma-locale phone there is no `.` key at all, so `Number(text)` on the raw field value is `NaN` for every decimal the user can actually type, and the control simply stays disabled with nothing on screen to explain it. Any numeric input must accept `,` as well as `.` — see `typed()` in `src/components/SettingsSheet.vue`. Not reproducible on the desk in any way: the Mac keyboard has a dot.

### The picture

`src/components/Spectrogram.vue` draws on **two canvases, and the split is load-bearing**: an
offscreen `buckets × 32` canvas is the only thing ever appended to or scrolled, and the visible
canvas gets one `drawImage` of it per changed tick with `imageSmoothingEnabled = false`. That is
spec §7.3's *append and scroll, never redraw the history*, and it is why the chrome (frame, frequency
labels, caption line) is redrawn only when a labelled setting or the geometry changes. **There is no
colour legend** — `b15` removed it and the plot took all but 2 px of its 58 px gutter, so the only
setting that still forces a chrome redraw is `unit`, which flips `dBFS/band` to `dB/band` at the
first calibration. A ±0.1 dB trim now changes no pixel at all, which is why `offset_db` left
`chromeKey`.

Two ways to break it silently, both found by `b11`:

- **The scroll must use `globalCompositeOperation = "copy"`.** Under the default `source-over`, a
  self-`drawImage` leaves the vacated columns holding their old pixels, so a stopped stream smears
  its last column across the picture instead of scrolling holes into it — §7.3's own "a dead stream
  reads as a peaceful room" failure, from the one direction the spec does not name.
- **Columns arriving while a `get_spectrogram` pull is in flight must be held and replayed.** The
  pull answers from a snapshot taken before the round trip, so rebuilding from it alone drops a
  tick's worth of real columns into a one-slot hole that never heals.

**To look at the picture with no hardware**: `git checkout prototype/b11-spectrogram-harness && npm
run dev`, then `/harness.html` — it drives the real component with synthetic pink / sweep / gap /
ramp columns and a faked `invoke`, in a browser. That is where §14.3's eyeball tests were run, and
where §7.1's colour window should be re-judged if the venue run moves it.

### Audio capture

The path is settled — **`cpal` for capture, with `AVAudioSession` configured ourselves from Rust via `objc2-avf-audio`**. The Web Audio API is ruled out: the target is iOS and capture must be native. Decided in `.scratch/spl-meter-mvp/research/01-native-audio-capture-path.md`; read that before changing anything here.

The one non-obvious part: **cpal never sets the `AVAudioSession` category and never activates the session** — it treats that as the application's job (its own iOS example does it in the host `AppDelegate`). Apple's default category is `SoloAmbient`, which permits no input, so skipping it guarantees failure. cpal also never calls `setPreferredSampleRate`, so the session rate is whatever the system is already running at unless we ask.

Two failure modes that look nothing alike but are easily confused:

| Symptom | Cause |
|---|---|
| `InvalidInput: channel count must be at least 1` | session category was never set, **or** the session is deactivated after an interruption |
| buffers of exact zeros | microphone permission not granted |

Permission wiring, all of which is in place:

- `src-tauri/Info.plist` — `NSMicrophoneUsageDescription`. Serves **both** iOS and macOS (iOS also picks up `Info.ios.plist`; macOS does not). Missing this key is a process kill, not a warning.
- `src-tauri/Entitlements.plist` — `com.apple.security.device.audio-input`, referenced from `bundle.macOS.entitlements`. Needed because Tauri enables Hardened Runtime by default, so *signed* macOS builds fail capture without it. Unsigned `tauri dev` binaries don't need it, which makes the dev loop the permissive case and the shipped bundle the strict one.

This now lives in `src-tauri/src/session.rs` (iOS-only, behind `cfg(target_os = "ios")`) and `src-tauri/src/capture.rs`, built by ticket `b01`, which replaced the throwaway spike. Spec §3 is the authority.

**An interruption kills the stream permanently and tells cpal nothing** (spec §4, ticket `b07`). A Siri call reached Rust as no error at all — the only symptom was `AVAudioSession.inputNumberOfChannels` going 1 → 0 and staying there. So `capture.rs` runs a **supervisor loop** on the capture thread that rebuilds the stream on any of three signals: an `AVAudioSessionInterruptionNotification` observer, a cpal stream error, and a 10 Hz poll of `inputNumberOfChannels` from the tick. **For interruptions the poll is the mechanism and the notification only makes it faster** — measured on device by building a variant with the observer compiled out, which still recovered from a Siri interruption in ~0.5 s against effectively-instant with it. **Route changes are the other way round**: cpal's iOS backend has a `session_event_manager` observing `AVAudioSessionRouteChangeNotification` that maps reasons onto error kinds (`OldDeviceUnavailable` → `DeviceChanged`, `CategoryChange`/`Override`/`RouteConfigurationChange` → `StreamInvalidated`, `NoSuitableRouteForCategory` → `DeviceNotAvailable`), so those arrive through the error callback. cpal does **not** observe `AVAudioSessionInterruptionNotification` — that one is ours. Recovery must `setActive(true)` **before** rebuilding — a rebuild against a deactivated session fails with `InvalidInput: channel count must be at least 1`, which is the *same* error as a never-set category, so it reads as a regression in `b01`'s setup code and is not one. A rebuild costs ~210 ms of real dead air, over §6.9's 200 ms staleness threshold, so `--` on screen during a recovery is expected.

**Backgrounding stops the session, not the process** (`b08`, correcting spec §4.3's record of probe 2b as *the app dies*). iOS refuses `setActive(true)` for a non-frontmost app with `AVAudioSessionErrorCodeCannotStartPlaying` (561015905, `'!pla'`) for exactly as long as it is backgrounded — measured at 70 s, same PID throughout, recovering 3 s after returning to the foreground unaided. The supervisor's retry-forever is what makes that automatic; without it the app returns frontmost holding a dead stream. Also: **a route change fires at every launch** as the route settles, so `Capture::builds` reads `2` after a clean launch, not `1`.

Two properties of that code are load-bearing and easy to undo by accident:

- **The session is configured before cpal is touched, and every value is read back as ground truth** — `sampleRate`, `inputNumberOfChannels`, `mode`, `IOBufferDuration`. Apple documents the rate and buffer duration as *preferences*. The `mode` read-back is not ceremony: `AVAudioSessionModeMeasurement` is worth **21 dB of fixed processing gain**, so if it silently fails to apply, the stored calibration offset is wrong by ~21 dB with nothing on screen to show it.
- **The capture callback does the minimum and allocates nothing** — f32→f64 at the block boundary, channel 0 only when the stream reports more than one (never average; it moves the level by up to 6 dB and would make calibration depend on channel count), no DC blocker, and one `{sum_sq, n, t}` summary pushed into a bounded lock-free SPSC queue (`rtrb`). A queue overflow must degrade into lost coverage, never a wrong number.
