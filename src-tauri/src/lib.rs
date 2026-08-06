pub mod capture;
pub mod metrics;
pub mod session;
pub mod settings;
pub mod weighting;

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use serde::Serialize;
use tauri::Manager;

/// Everything the app owns. At this tier that is capture, the settings, and the temporary
/// readout's own counters; the metrics ring and the 10 Hz tick arrive with `b05`.
struct AppState {
    capture: Arc<capture::Capture>,
    /// The four settings and their file. Behind a `Mutex` rather than owned by the tick thread
    /// because the commands `b05` adds write to it while the tick reads it, and a write goes to
    /// disk (spec §10) — not something to do from an audio path, and this is not one.
    settings: Mutex<settings::SettingsStore>,
    /// Blocks drained since startup. Lives here rather than in `capture` because it counts
    /// what the *display* side has seen, which is the number worth watching on a device.
    blocks: AtomicU64,
}

/// **Temporary.** The whole readout for this tier, replaced wholesale by the real screen in
/// `b06`.
///
/// It exists because `println!` does not reach `devicectl … --console` (spec §2.2), so the
/// only way to see whether the phone is capturing is to put it on the phone's screen.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Readout {
    capture: capture::CaptureState,
    /// Cumulative blocks drained.
    blocks: u64,
    /// Blocks lost to queue overflow — lost coverage, never a wrong number (spec §6.10).
    dropped: u64,
    /// Level of the blocks drained by *this* call, through the selected weighting, **with the
    /// calibration offset applied post-log** (spec §8.2). `None` when no block arrived or the
    /// power was exactly zero — the same refusal spec §6.9 and §16.7 make, and the reason a
    /// denied microphone shows nothing rather than a very quiet room.
    level: Option<f64>,
    /// `dB` or `dBFS`, travelling **with** the value so it can never be painted under the wrong
    /// label (spec §9.2). The real tick does the same.
    unit: settings::Unit,
    /// The four settings, in the shape spec §9.1 puts them on the wire — snake_case, unlike the
    /// rest of this temporary struct.
    settings: settings::Settings,
    last_error: Option<String>,
}

/// **Temporary.** Polled by `App.vue` until `b05` replaces polling with the 10 Hz tick event.
#[tauri::command]
fn capture_readout(state: tauri::State<'_, AppState>) -> Readout {
    let mut sum_sq = 0.0f64;
    let mut n: u64 = 0;
    let drained = state.capture.drain(|summary| {
        sum_sq += summary.sum_sq;
        n += summary.n as u64;
    });
    let blocks = state.blocks.fetch_add(drained as u64, Ordering::Relaxed) + drained as u64;

    // 10·log₁₀ of the mean square: the same dBFS convention the meter and the FFT both use
    // (spec §16.4), so a full-scale sine reads −3.01 dBFS rather than 0.
    let raw = if n > 0 && sum_sq > 0.0 {
        Some(10.0 * (sum_sq / n as f64).log10())
    } else {
        None
    };

    let settings = state.settings.lock().unwrap().settings();

    Readout {
        capture: state.capture.state(),
        blocks,
        dropped: state.capture.dropped(),
        level: settings.calibrated(raw),
        unit: settings.unit(),
        settings,
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

            // Capture starts itself on its own thread and reports its own state, so nothing
            // here waits on the microphone permission prompt.
            app.manage(AppState {
                capture: capture::Capture::start(loaded.weighting),
                settings: Mutex::new(store),
                blocks: AtomicU64::new(0),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![capture_readout])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
