pub mod bridge;
pub mod capture;
pub mod metrics;
pub mod session;
pub mod settings;
pub mod weighting;

use std::sync::atomic::Ordering;
use std::time::Instant;

use serde::Serialize;
use tauri::Manager;

use bridge::AppState;

/// **Temporary.** The facts the tick has no place for, replaced wholesale by the real screen in
/// `b06` — after which the same detail is only in Xcode's log, which is what `b08` reads.
///
/// It exists because `println!` does not reach `devicectl … --console` (spec §2.2), so the only
/// way to see what rate, channel count and mode the phone granted is to put them on its screen.
/// Everything a *meter* paints is in the tick and deliberately not repeated here.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Diagnostics {
    capture: capture::CaptureState,
    /// Cumulative blocks the tick has deposited.
    blocks: u64,
    /// Blocks lost to queue overflow — lost coverage, never a wrong number (spec §6.10).
    dropped: u64,
    last_error: Option<String>,
}

/// **Temporary.** Polled slowly by `App.vue` beside the tick; deleted with the readout in `b06`.
///
/// Note what it no longer does: **draining belongs to the tick thread alone**. Two drainers would
/// split the audio between them and neither number would mean anything.
#[tauri::command]
fn capture_diagnostics(state: tauri::State<'_, AppState>) -> Diagnostics {
    Diagnostics {
        capture: state.capture.state(),
        blocks: state.blocks.load(Ordering::Relaxed),
        dropped: state.capture.dropped(),
        last_error: state.capture.last_error(),
    }
}

/// `UIApplication.isIdleTimerDisabled = true`, once at startup, on the main thread.
///
/// Two lines and load-bearing: without it the screen sleeps mid-talk and measurement dies on
/// its own (spec §4.4). UIKit is already in Tauri's hardcoded framework list, so unlike
/// AVFAudio this needs no `bundle.iOS.frameworks` entry and no project regeneration.
#[cfg(target_os = "ios")]
fn disable_idle_timer() {
    use objc2::MainThreadMarker;
    use objc2_ui_kit::UIApplication;

    let Some(mtm) = MainThreadMarker::new() else {
        eprintln!("idle timer: not on the main thread, skipped — the screen will sleep");
        return;
    };
    let app = UIApplication::sharedApplication(mtm);
    app.setIdleTimerDisabled(true);
}

/// The desktop dev loop has no idle timer to disable, and the meter is never left unattended
/// on a Mac anyway.
#[cfg(not(target_os = "ios"))]
fn disable_idle_timer() {}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            // Queued rather than called inline: `setup` is on the main thread today, but
            // `UIApplication` is main-thread-only and going through the event loop makes that
            // a guarantee rather than an assumption.
            app.handle().run_on_main_thread(disable_idle_timer)?;

            // One read at startup, before anything else, because the persisted weighting decides
            // which chain the very first block goes through (spec §10). `app_config_dir()` is
            // core Tauri called from Rust rather than over the bridge, so this needs no
            // capability entry — `CLAUDE.md`'s four-step plugin ceremony does not apply.
            let path = match app.path().app_config_dir() {
                Ok(dir) => Some(dir.join(settings::FILE_NAME)),
                Err(e) => {
                    eprintln!("settings: no config directory ({e})");
                    None
                }
            };
            if let Some(path) = &path {
                // Logged because on a device this file is the only recovery path for the offset
                // (spec §8.5) and there is otherwise nothing that says where it is.
                eprintln!("settings: file is {}", path.display());
            }
            let store = settings::SettingsStore::load(path);
            let loaded = store.settings();
            eprintln!("settings: {loaded:?}");

            // The ring starts at the *preferred* rate and is corrected on the first tick that
            // sees a running stream — the granted rate is only known on the capture thread, and
            // nothing can have been deposited before it is, so the correction is exact.
            //
            // The two settings the ring holds a copy of are pushed in here rather than left to
            // coincide with its own defaults: `settings.rs` and `metrics.rs` both default to 60 s
            // and both to `S`, and relying on that would silently mis-slice the window for anyone
            // whose `settings.json` says otherwise.
            let mut metrics = metrics::Metrics::new(session::PREFERRED_SAMPLE_RATE, Instant::now());
            metrics.set_window_s(loaded.window_s);
            metrics.set_time_weighting(loaded.time_weighting);

            // Capture starts itself on its own thread and reports its own state, so nothing
            // here waits on the microphone permission prompt.
            app.manage(AppState::new(
                capture::Capture::start(loaded.weighting),
                metrics,
                store,
            ));

            // After `manage`: the tick thread looks the state up on its first pass, 100 ms later.
            bridge::spawn(app.handle().clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            bridge::set_weighting,
            bridge::set_time_weighting,
            bridge::set_window_length,
            bridge::set_calibration_from_reference,
            bridge::set_calibration_offset,
            bridge::reset,
            capture_diagnostics
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
