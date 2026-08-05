pub mod capture;
pub mod session;
pub mod weighting;

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use serde::Serialize;
use tauri::Manager;

/// Everything the app owns. At this tier that is capture plus the temporary readout's own
/// counters; the metrics ring, the settings and the 10 Hz tick arrive with `b03`–`b05`.
struct AppState {
    capture: Arc<capture::Capture>,
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
    /// Unweighted (Z) level of the blocks drained by *this* call, in dBFS. `None` when no
    /// block arrived or the power was exactly zero — the same refusal spec §6.9 and §16.7
    /// make, and the reason a denied microphone shows nothing rather than a very quiet room.
    dbfs: Option<f64>,
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
    let dbfs = if n > 0 && sum_sq > 0.0 {
        Some(10.0 * (sum_sq / n as f64).log10())
    } else {
        None
    };

    Readout {
        capture: state.capture.state(),
        blocks,
        dropped: state.capture.dropped(),
        dbfs,
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

            // Capture starts itself on its own thread and reports its own state, so nothing
            // here waits on the microphone permission prompt.
            app.manage(AppState {
                capture: capture::Capture::start(),
                blocks: AtomicU64::new(0),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![capture_readout])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
