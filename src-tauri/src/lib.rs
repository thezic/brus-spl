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
fn run_capture_spike(seconds: u64, measurement_mode: bool) -> Result<String, String> {
    let seconds = seconds.clamp(1, 300);
    let report = match spike::run(Duration::from_secs(seconds), measurement_mode) {
        Ok(report) => report,
        Err(e) => return Err(format!("{:?}: {e}", e.kind())),
    };

    // Assembled line by line rather than as one giant format!, because ticket 11's probes keep
    // adding fields and the on-screen report is the only channel that reaches the device.
    let mut out = vec![
        report.verdict().to_string(),
        String::new(),
        format!(
            "mode: {}",
            if measurement_mode {
                "Measurement"
            } else {
                "Default"
            }
        ),
        format!("device: {}", report.device_id),
        format!("format: {}", report.sample_format),
        format!("sample rate: {} Hz", report.sample_rate),
        format!("channels: {}", report.channels),
        format!(
            "buffer frames: {}",
            report
                .granted_buffer_frames
                .map(|f| f.to_string())
                .unwrap_or_else(|| "<unavailable>".into())
        ),
        format!("blocks: {}", report.blocks),
        format!("all-zero blocks: {}", report.all_zero_blocks),
        format!(
            "frames: {} of {} expected",
            report.frames, report.expected_frames
        ),
        format!(
            "missing: {} frames = {:.0} ms",
            report.expected_frames.saturating_sub(report.frames),
            report.missing_ms()
        ),
        format!("rms: {:.1} dBFS", report.rms_dbfs),
        format!("peak: {:.1} dBFS", report.peak_dbfs),
    ];

    // The three things ticket 11 exists to observe. Stated explicitly either way, so "no gaps"
    // is a recorded observation rather than an absence someone has to infer.
    out.push(String::new());
    if report.gaps.is_empty() {
        out.push("gaps: none".into());
    } else {
        out.push(format!("GAPS ({}):", report.gaps.len()));
        for gap in &report.gaps {
            out.push(format!("  {gap}"));
        }
    }
    if report.session_changes.is_empty() {
        out.push("session changes: none".into());
    } else {
        out.push(format!(
            "SESSION CHANGES ({}):",
            report.session_changes.len()
        ));
        for change in &report.session_changes {
            out.push(format!("  {change}"));
        }
    }
    if report.errors.is_empty() {
        out.push("stream errors: none".into());
    } else {
        out.push(format!("STREAM ERRORS ({}):", report.errors.len()));
        for err in &report.errors {
            out.push(format!("  {err}"));
        }
    }

    if let Some(s) = report.session {
        out.push(String::new());
        out.push("AVAudioSession at start (ground truth):".into());
        out.push(format!("  sampleRate: {} Hz", s.sample_rate));
        out.push(format!("  inputNumberOfChannels: {}", s.input_channels));
        out.push(format!("  IOBufferDuration: {:.6} s", s.io_buffer_duration));
    }

    Ok(out.join("\n"))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![run_capture_spike])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
