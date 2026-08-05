# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project state

A Tauri 2 + Vue 3 + TypeScript + Vite app, still very early. The planning effort for the MVP lives in `.scratch/spl-meter-mvp/` — `map.md` is the index, `issues/` are the tickets, `research/` holds the resolved investigations. **Read the map before starting work**; most tickets there produce decisions, not code, and the spec (`issues/10`) is the destination.

The only code written so far is the **capture spike** for ticket `02` (`src-tauri/src/spike.rs`, `src/App.vue`, `src/bin/spike.rs`). It is deliberately throwaway — it exists to prove the audio architecture on a physical iPhone, not to become the meter. `App.vue` in particular is a bare harness, not a layout: how the screen actually looks is ticket `09`'s job. Delete all of it once the spec is written.

No test runner, linter, or formatter is configured. If a task needs one, ask before adding it. Note that ticket `03` produced a 34-frequency weighting-filter validation table that wants to become a unit test — whether to add a runner for it is an open question on the map, not a decision to make unilaterally.

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

cargo run --bin spike          # capture spike, 5s (ticket 02); takes a seconds argument
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

### Audio capture

The path is settled — **`cpal` for capture, with `AVAudioSession` configured ourselves from Rust via `objc2-avf-audio`**. The Web Audio API is ruled out: the target is iOS and capture must be native. Decided in `.scratch/spl-meter-mvp/research/01-native-audio-capture-path.md`; read that before changing anything here.

The one non-obvious part: **cpal never sets the `AVAudioSession` category and never activates the session** — it treats that as the application's job (its own iOS example does it in the host `AppDelegate`). Apple's default category is `SoloAmbient`, which permits no input, so skipping it guarantees failure. cpal also never calls `setPreferredSampleRate`, so the session rate is whatever the system is already running at unless we ask.

Two failure modes that look nothing alike but are easily confused:

| Symptom | Cause |
|---|---|
| `InvalidInput: channel count must be at least 1` | session category was never set |
| buffers of exact zeros | microphone permission not granted |

Permission wiring, all of which is in place:

- `src-tauri/Info.plist` — `NSMicrophoneUsageDescription`. Serves **both** iOS and macOS (iOS also picks up `Info.ios.plist`; macOS does not). Missing this key is a process kill, not a warning.
- `src-tauri/Entitlements.plist` — `com.apple.security.device.audio-input`, referenced from `bundle.macOS.entitlements`. Needed because Tauri enables Hardened Runtime by default, so *signed* macOS builds fail capture without it. Unsigned `tauri dev` binaries don't need it, which makes the dev loop the permissive case and the shipped bundle the strict one.

The spike lives in `src-tauri/src/spike.rs` and is **throwaway** — it exists to prove the above works on a physical device, not to become the meter. Run it on the desktop with `cargo run --bin spike`.
