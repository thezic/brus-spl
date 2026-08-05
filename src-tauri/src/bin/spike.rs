//! Desktop runner for the capture spike (ticket `02`). Throwaway, like the module it calls.
//!
//! ```sh
//! cargo run --bin spike          # 5 seconds
//! cargo run --bin spike -- 20    # 20 seconds
//! ```
//!
//! Deliberately not a Tauri app: on the macOS dev loop this proves the cpal path in
//! isolation, with no webview and no bundle in the way. That also makes it the cheapest
//! way to answer research `01` open risk 7 — whether an unbundled binary with no
//! `Info.plist` gets microphone access at all, or is terminated for lacking a purpose
//! string.

use std::time::Duration;

fn main() -> std::process::ExitCode {
    let seconds = std::env::args()
        .nth(1)
        .and_then(|a| a.parse::<u64>().ok())
        .unwrap_or(5);

    match decibel_meter_lib::spike::run(Duration::from_secs(seconds)) {
        Ok(report) => {
            if report.verdict() == decibel_meter_lib::spike::Verdict::Captured {
                std::process::ExitCode::SUCCESS
            } else {
                // A non-zero exit makes the failure modes scriptable, which matters
                // because "all zeros" is otherwise a perfectly successful-looking run.
                std::process::ExitCode::FAILURE
            }
        }
        Err(e) => {
            eprintln!("spike failed: {:?}: {e}", e.kind());
            if e.kind() == cpal::ErrorKind::InvalidInput {
                eprintln!(
                    "hint: on iOS this signature means the AVAudioSession category was \
                     never set. On macOS it more likely means no input device."
                );
            }
            std::process::ExitCode::FAILURE
        }
    }
}
