pub mod spike;

use std::time::Duration;

/// Runs the throwaway capture spike (ticket `02`) and returns its findings as text.
///
/// This exists because the spike's real target is a physical iPhone, and a bare binary
/// cannot be signed and installed on one — it has to be reached from inside the app. On
/// the desktop prefer `cargo run --bin spike`.
///
/// Blocking on purpose: it captures for a fixed window. Tauri runs commands on a thread
/// pool, so this does not stall the UI thread, but it does hold one pool thread for the
/// duration. Acceptable for a spike; not a pattern for the meter.
///
/// Output also goes to stdout, which is where the Xcode console will show it.
#[tauri::command]
fn run_capture_spike(seconds: u64) -> Result<String, String> {
    let seconds = seconds.clamp(1, 120);
    match spike::run(Duration::from_secs(seconds)) {
        Ok(report) => Ok(format!(
            "{}\n\ndevice: {}\nformat: {}\nsample rate: {} Hz\nchannels: {}\n\
             buffer frames: {}\nblocks: {}  frames: {}  all-zero blocks: {}\n\
             rms: {:.1} dBFS   peak: {:.1} dBFS{}{}",
            report.verdict(),
            report.device_id,
            report.sample_format,
            report.sample_rate,
            report.channels,
            report
                .granted_buffer_frames
                .map(|f| f.to_string())
                .unwrap_or_else(|| "<unavailable>".into()),
            report.blocks,
            report.frames,
            report.all_zero_blocks,
            report.rms_dbfs,
            report.peak_dbfs,
            report
                .session
                .map(|s| format!(
                    "\n\nAVAudioSession (ground truth):\n  sampleRate: {} Hz\n  \
                     inputNumberOfChannels: {}\n  IOBufferDuration: {:.6} s",
                    s.sample_rate, s.input_channels, s.io_buffer_duration
                ))
                .unwrap_or_default(),
            if report.errors.is_empty() {
                String::new()
            } else {
                format!("\n\nstream errors:\n  {}", report.errors.join("\n  "))
            },
        )),
        Err(e) => Err(format!("{:?}: {e}", e.kind())),
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![run_capture_spike])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
