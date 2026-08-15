# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project state

A Tauri 2 + Vue 3 + TypeScript + Vite app: an SPL meter for iOS, read at a venue's mixing desk. It
works and has been used at one, so the planning and build efforts are both **closed** — new work
starts by writing a ticket, not by picking one up.

**`.scratch/spl-meter-mvp/spec.md` is the authority for *what* to build.** It cites the ticket behind
each call rather than reproducing the argument, so the reasoning is always two links away: in that
map's `issues/` and `research/`, and in `.scratch/spl-meter-build/`, whose `map.md` holds the Route,
the Drop order and ~400 lines of Decisions-so-far beside `issues/b01`–`b15` in full. Read the cited
ticket before reopening a decision.

**Implementation work lives on GitHub Issues** (`thezic/brus-spl`) — see
`docs/agents/issue-tracker.md`. Three numbering schemes coexist, deliberately: a bare **`#N`** is a
GitHub issue, while **`bNN`** and **`NN d<n>`** mean **read the file** under `.scratch/`. The build
map [#1](https://github.com/thezic/brus-spl/issues/1) and its tickets are closed history rather than
a queue, and `#4` shows that new work need not hang off it at all. Tickets produce code, not
decisions — nothing settled in the spec gets re-litigated on the tracker.

**Planning maps live on the tracker now too.** [#7](https://github.com/thezic/brus-spl/issues/7)
is the first, and it ran the whole `.scratch/` shape — a map, prototype and research tickets under
it, and a spec correction at the end — without a directory. Its decisions are §7.4–§7.5 and its
reasoning is in `#8`, `#9` and `#10`, which is where a *why* question goes. So `.scratch/` is the
closed history and the tracker is where planning happens; the spec is still the authority for both.

### Settled, and easy to undo by accident

Each of these was decided against an alternative that still looks reasonable from the code alone.

- **Display resolution is 0.1 dB everywhere** — `display.ts` rounds nothing. Coarsening the hero to
  1 dB was built and then reverted on the owner's call (`b14`, correcting spec §11.4): *the decimal
  stays*. The busyness was a **repaint rate**, not a precision — §6.6's measured 0.351 dB per tick
  changes a digit on essentially every one of the ten ticks a second, so the hero repaints at 2 Hz
  and keeps its tenth.
- **That throttle is not the publish rate.** `LIVE_REFRESH_MS` in `src/components/Hero.vue` paces
  one number on screen; the tick itself stays at 10 Hz or coverage, Reset and max tracking all
  break (§6.10). And `--` bypasses the throttle in **both** directions — holding a stale number past
  §6.9's 200 ms staleness threshold is exactly what that rule forbids.
- **32 third-octave bands at ~205 px**, confirmed legible at the venue (`b13`): ~6 px per band reads
  fine at arm's length and in a dim room. **Do not answer the wish for a finer picture by adding
  bands** — energy-summed rows scale with bandwidth, so finer bands shift every cell ~3 dB and would
  re-derive §7.1's `−90 … −30 dBFS` colour window that `b13` just confirmed. What "pinpoint
  problematic frequencies" actually wants is a **readout** on the picture: new scope and a real
  decision, so give it a planning map rather than a build ticket. **That map is
  [#7](https://github.com/thezic/brus-spl/issues/7) and it is settled** — spec §7.4–§7.5.
- **The hero number and `--` deliberately have no defence against the picture behind them** (§7.4,
  and §17 records it as a knowing exception to §11.5). Inferno runs to near-white, so both degrade
  over a loud high band — the obvious "fix" is a scrim, and a scrim dims the top 48 % of the plot
  whether the number needs it or not, which makes loud bands read as quieter and breaks §7.1's *the
  same colour is the same absolute level*. If a room does disagree, the reversal is `outline`: it
  costs no picture at all. Ask before painting anything over the plot.
- **The marker snaps to the band, and there is no hit assist** (§7.5). Both look like missing
  polish and are neither. An interpolated `3422 Hz` names a frequency 32 energy-summed rows cannot
  distinguish — one pixel of finger movement is 37 Hz where the band spans 730. Assist would need
  per-band history on the frontend, which §9.5 refuses, and it degrades silently when the history is
  short. **Its level comes from Rust for the slot**, not from inverting the colour ramp: that is
  what keeps §8.2 exception-free.
- **Eleven frequency labels in expanded landscape is the rule working, not a bug.** Label density
  derives from the height at a 14 px minimum pitch (§7.4), and landscape gives ~12.1 px per band —
  so the orientation that was asked to be all picture carries no more names than the inline one.
  Raising the threshold to "fix" it re-opens `b15`'s dim-room legibility call.
- **The calibration offset is `+116.7 dB`**, the first real one — measured against a reference meter
  at the venue (`b13`), where every value before it was a guess. It lives in the phone's container,
  not the repo; the `devicectl` pull/push below is how it moves.
- **No frontend test runner, and no linter or formatter beyond `cargo clippy`/`cargo fmt`.** Closed
  deliberately by mvp ticket `10`, with `src/bridge.ts` as the single accepted untested seam; Rust is
  covered by `cargo test` (`b02`), which needed no tooling decision. Ask before adding one.

Two constants are one-line judgement calls a room may disagree with, both checked on glass on
2026-08-15: `LIVE_REFRESH_MS = 500` in `Hero.vue` (300 ms was the alternative) and the frequency
ladder in `Spectrogram.vue` — which after #7 is a **14 px minimum pitch** rather than an
eleven-entry list, though it still produces those eleven at 205 px. The conditions of that check
went unrecorded, so **treat the dim-room-at-arm's-length read as still open** — a desk look is not
the same test, and #7's own prototypes were judged on a desk too.

### The rename

**The app was called `decibel-meter` through `b15`, and is `Brus` from
[#4](https://github.com/thezic/brus-spl/issues/4) on**, under the bundle identifier
`net.thezic.brus`. The repo keeps its `brus-spl` name — the suffix is a GitHub-namespace concern
only — and **`.scratch/` plus the closed issue bodies keep the old name on purpose**: they describe
work that genuinely happened under it, so this line is the bridge rather than a rewrite of the
record.

**iOS sandboxes app containers per identifier**, so the rename gave the app a fresh, empty one and
no in-app migration was ever possible: `#4` carried the calibration across by hand, pulling from the
old container and pushing into the new. The pulled file also held **`window_s: 30`, not the default
60**, which is why the checklist moved the file rather than trusting the one number everybody
remembered. The app icon is still stock Tauri; naming the app was not the same job as drawing it.

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
unzip -p src-tauri/gen/apple/build/arm64/Brus.ipa \
  "Payload/Brus.app/embedded.mobileprovision" > /tmp/prov.plist
security cms -D -i /tmp/prov.plist | plutil -p - | grep -E "CreationDate|ExpirationDate"
```

To force a fresh one: move that profile out of the cache directory and rebuild — Xcode issues a new
7-day profile during the build, no prompt and no paid account (`b12`, 48 s end to end). The signing
*certificate* is valid for a year, so this is never a certificate problem. A **new bundle identifier
needs no forcing at all** — it misses the cache, so the build issues its own profile unprompted.

`#4` demonstrated the no-forcing case: the first build under `net.thezic.brus` issued its own profile
unprompted, created 2026-08-15 11:58 UTC and expiring **2026-08-22 11:58 UTC**. The older profile in
the cache belongs to `net.thezic.decibel-meter` and is dead weight. Read the expiry rather than
assuming which one you have.

**The stored calibration offset can be pulled off the phone and pushed back**, which is the real
answer to spec §13.10:

```bash
xcrun devicectl device copy from --device <udid> \
  --domain-type appDataContainer --domain-identifier net.thezic.brus \
  --source "Library/Application Support" --destination ./backup     # `--source .` fails
xcrun devicectl device copy to   --device <udid> \
  --domain-type appDataContainer --domain-identifier net.thezic.brus \
  --source ./backup/net.thezic.brus/settings.json \
  --destination "Library/Application Support/net.thezic.brus/settings.json"
```

The app reads it at the next launch and prints what it loaded to stderr, so a restore is verifiable
with `idevicesyslog -m "settings:"` — **over USB**. `#4` found the Wi-Fi case is not equivalent: with
the phone paired over the network only, plain `idevicesyslog` answers `No device found` (it needs
`-n -u <udid>`, and `idevice_id -l` is empty while `idevice_id -n` lists the phone), and even then
**no `[stderr]` line from the app ever arrives** — the relay that carries them is the USB path. The
process filter proves the app is alive (`idevicesyslog -n -u <udid> -p Brus` shows WebKit running the
tick's JavaScript ~10×/s) but says nothing about what settings it loaded, and
`devicectl … --console` carries no app output either. **So a settings read-back needs a cable**, and
without one the only confirmation is the screen.

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

**A moved repo directory poisons `src-tauri/target`, and the failure blames something else.** The
folder was renamed to `brus-spl` at some point while `target/` kept ~2000 absolute paths into the old
one, and nothing noticed until `#4`'s package rename forced a fresh iOS *release* build. It fails
with `failed to read plugin permissions: failed to read file
'…/dev/mine/decibel-meter/src-tauri/target/aarch64-apple-ios/release/…/app_hide.toml': No such file
or directory` — a path under a directory that no longer exists. It is **not** a permissions problem
and **not** a `capabilities/default.json` problem, which is what the wording invites. Fix:
`cargo clean --release --target aarch64-apple-ios`. Desktop and `cargo check` hide it, because those
profiles had been rebuilt since the move and the iOS release artifacts had not.

**`@tauri-apps/cli` is pinned to an exact version, not `^2`.** The iOS `Info.plist` merge order is undocumented and changed silently between tauri-cli 2.4 and 2.9, and a missing `NSMicrophoneUsageDescription` is a launch-time process kill rather than a build error. After bumping it — or after any `tauri ios build` — run `./scripts/check-ios-plist.sh` to assert the key survived the merge.

## Architecture

Two processes, one repo:

- **Frontend** (`src/`) — Vue 3 SFCs with `<script setup>`, mounted in `src/main.ts`. TypeScript is `strict` with `noUnusedLocals`/`noUnusedParameters`, so unused bindings fail the build. Every string that sits beside a number on screen lives in `src/display.ts`.
- **Rust backend** (`src-tauri/src/`) — `main.rs` is a thin shim that calls `run()` in `lib.rs`; all setup belongs in `lib.rs` (the split exists so mobile targets can share the lib entry point). The crate is named `brus_lib`.

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

Columns are made in Rust — `src-tauri/src/spectrum.rs` is the FFT tap, the third-octave banding and
the 1200-column ring — and pulled across by `get_spectrogram` in `src/bridge.ts`. The picture's
arithmetic lives in `src/spectrogram.ts` (colour ramp, dB window, column-into-pixel budget),
deliberately DOM-free; the component owns the canvases and the events.

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

**To look at the picture with no hardware**: check out a harness branch and `npm run dev`, then
`/harness.html` — it drives the real component with synthetic columns and a faked `invoke`, in a
browser. That is where §14.3's eyeball tests were run, and where §7.1's colour window should be
re-judged if the venue run moves it.

**Use `prototype/07-harness`, not `prototype/b11-spectrogram-harness`.** The `b11` branch is the
original and it is **20 commits stale** — it predates the rename to Brus, `b13`'s calibration,
`b14`'s hero decimal and all of `b15`, so its `Spectrogram.vue` still has the colour legend and the
four-label axis. Looking at it means judging a picture the app no longer draws. Its single commit
has been replanted onto current `main` as `prototype/07-harness`; two further branches build on
that one, and each carries a `findings-0N.md`:

| Branch | What it drives | Source modes |
|---|---|---|
| `prototype/07-harness` | the current inline picture, `b11`'s harness as it was | pink · sweep · gap · ramp |
| `prototype/09-expanded-layout` | §7.4's expanded view, with a level control and real phone frames | + a synthetic meter that wanders at §6.6's rate and goes `--` |
| `prototype/08-marker` | §7.5's marker, every candidate on a switch | + `tones` — three narrow stripes, the only source with a real local maximum |

**The source matters when comparing candidates.** `ramp` is strictly monotone, so anything that
looks for a local maximum in it finds a constant offset rather than a feature — which is how the hit
assist first read as *no difference at all*. `tones` is the one built to discriminate.

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
