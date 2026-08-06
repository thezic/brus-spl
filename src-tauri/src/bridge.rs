//! The wire contract, the 10 Hz tick thread and the six commands — spec §9.
//!
//! **One event carrying everything the screen paints, plus commands. The frontend holds no
//! authoritative state at all** (spec §9). The tick is ≈460 bytes, ≈4.6 kB/s — about **17×
//! under** the 8192-byte threshold at which Tauri's IPC switches to its bulk path, so it never
//! leaves the fast path. There is deliberately no throttling or coalescing here: wry pushes
//! scripts into `pending_scripts` before load and `evaluateJavaScript` after, so a stalled
//! webview **delays** ticks rather than losing them.
//!
//! **The tick is a timer, not audio-driven** (spec §16.6). It publishes with no audio at all,
//! which is what makes settings-in-every-tick and the `input` state safe: a dead stream still
//! produces ticks, and they still say `--`, `0s of 60s` and why.
//!
//! Wire names are **snake_case on both sides**, so a Rust field name and its TS field name in
//! `src/bridge.ts` are literally the same string. The honest cost, accepted deliberately: a
//! renamed field is a runtime `undefined`, not a compile error, because `vue-tsc` cannot see
//! across the bridge. [`tests::the_wire_shape_is_section_9_1_verbatim`] pins the Rust half of it.
//!
//! **Every dB value crossing this module is already calibrated** (spec §8.2) and rounded to
//! 0.1 dB. Nothing downstream of here ever sees a raw dBFS number, which is what makes it
//! impossible to render one by mistake.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::capture::{Capture, CaptureState};
use crate::metrics::{Levels, Metrics, TimeWeighting};
use crate::session::{self, MicPermission};
use crate::settings::{Settings, SettingsStore, Unit};
use crate::weighting::Weighting;

/// The event name the frontend listens for. **Events need no capability entry** — `core:default`
/// already includes `core:event:default`, so `CLAUDE.md`'s four-step plugin ceremony does not
/// apply anywhere in this design.
pub const EVENT: &str = "tick";

/// 10 Hz (spec §9). Fast enough that a picker tap looks instant and NOW reads as live; slow
/// enough that 0.1 dB of movement per tick at `S` is legible rather than a blur.
const TICK_INTERVAL: Duration = Duration::from_millis(100);

/// The calibration match runs against a **fixed 10 s slice**, independent of the display's
/// window setting (spec §8.3).
const CAL_SLICE_S: u32 = 10;

/// The one place the app speaks up (spec §9.4).
///
/// Three states, not a bool: a bare `is_capturing` collapses *fix it in Settings* and *this is a
/// bug or a missing device* into one message that can only be vague about both — the shape of
/// message that teaches people to ignore messages. `"unavailable"` conflates its own causes
/// deliberately (stream build failure, no input device, a never-set category); that detail lives
/// only in the log.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Input {
    Capturing,
    Denied,
    Unavailable,
}

/// Spec §9.1's `meter`. Every dB value here is post-offset and rounded to 0.1 dB.
///
/// `Option<f64>` serializes to `null` — `--` is `null`, never a sentinel like `-999`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Meter {
    /// The rolling `L_eq` over the window: the number judged against the ceiling.
    pub leq: Option<f64>,
    /// NOW, time-weighted `F` or `S`. §9.1's name for what [`Levels::now`] calls `now`.
    pub inst: Option<f64>,
    pub max: Option<f64>,
    /// Seconds of real audio inside the window. Published **always**, never conditional on being
    /// degraded (spec §6.4) — an indicator that appears only when something is wrong is a
    /// warning, and warnings were ruled out.
    pub coverage_s: f64,
    /// The fixed 10 s calibration slice (spec §8.3), and its own coverage, so the sheet can show
    /// what it is about to match against.
    pub cal_leq: Option<f64>,
    pub cal_coverage_s: f64,
    pub input: Input,
}

/// One spectrogram column, spec §9.1's `columns` element.
///
/// **Shipped from the start, always empty until
/// [`b09`](../../../.scratch/spl-meter-build/issues/b09-spectrum-analysis-and-the-column-ring.md)**,
/// so the type does not change under the frontend later.
///
/// Every column carries its **absolute** slot index, and each tick ships every real column since
/// the previous publish. That is required for correctness, not robustness: spec §6.2's
/// clock-advanced ring means a late tick — a timer on a phone — has two or three genuinely
/// completed slots behind it, and a one-column payload would drop real data on the floor. It is
/// why `columns` is an array even while it is empty.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Column {
    pub slot: u64,
    pub bands: [f32; 32],
}

/// The four settings **and the unit**, as spec §9.1 puts them on the wire.
///
/// The unit rides in every tick beside the values, which extends spec §8.2's footgun-denial from
/// values to **labels**: a number can never be painted under the wrong unit, because the unit
/// travels with the numbers rather than being decided by the frontend.
///
/// **This is the only settings shape that crosses the bridge** — it is what every command returns
/// as well as what rides in every tick, so `src/bridge.ts` needs exactly one `Settings` type. The
/// six commands originally handed back [`crate::settings::Settings`] instead, which is the same
/// four values *without* the unit and with the offset unrounded. Nothing failed loudly: the
/// frontend simply read `undefined` for the unit off a command return, which is precisely the
/// footgun §9.2 legislates against — and `b06` takes picker feedback from the return value rather
/// than the next tick, so it would have painted a calibrated number under no unit at all for up to
/// 100 ms after the one act that changes the unit.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct WireSettings {
    pub weighting: Weighting,
    pub time_weighting: TimeWeighting,
    pub window_s: u32,
    /// **Rounded to 0.1 dB like every other dB value on the wire**, while [`SettingsStore`] keeps
    /// the derived value exact. The sheet shows this number and lets it be retyped (spec §8.5),
    /// so retyping what is displayed can move the calibration by up to 0.05 dB — two orders of
    /// magnitude below the instrument's own honesty, and much better than putting
    /// `101.43871…` in an editable field. Rounding here rather than at the store is what keeps a
    /// command's answer and the next tick's answer the same number.
    pub offset_db: Option<f64>,
    pub unit: Unit,
}

impl From<Settings> for WireSettings {
    fn from(settings: Settings) -> WireSettings {
        WireSettings {
            weighting: settings.weighting,
            time_weighting: settings.time_weighting,
            window_s: settings.window_s,
            offset_db: settings.offset_db.map(round_01),
            unit: settings.unit(),
        }
    }
}

/// The whole of what the screen paints, once per 100 ms.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Tick {
    /// The clock-advanced ring index (spec §6.2). **This is what makes the picture's right edge
    /// honest**: without it a run of gap slots up to the present is invisible, because no columns
    /// arrive and so nothing says the silence is *current*. It carries the same information for
    /// the picture that the coverage figure carries for the number.
    pub now_slot: u64,
    pub meter: Meter,
    pub settings: WireSettings,
    pub columns: Vec<Column>,
}

/// Everything the app owns, and the only state either side of the bridge has.
///
/// **Lock order is `metrics` then `settings`, everywhere, without exception.** Both are taken
/// together by the tick and by three of the six commands, so a consistent order is the whole of
/// the deadlock argument. Neither is ever taken from the audio callback — the one thread boundary
/// that touches it is the SPSC queue (spec §6.10).
pub struct AppState {
    pub capture: Arc<Capture>,
    /// The ring, the smoother and the hold. `Mutex` rather than owned by the tick thread because
    /// spec §6.11's side effects are commands: a weighting change has to clear the window, and it
    /// arrives on Tauri's command thread.
    pub metrics: Mutex<Metrics>,
    /// The four settings and their file. A write goes to disk (spec §10) — not something to do
    /// from an audio path, and this is not one.
    pub settings: Mutex<SettingsStore>,
    /// Blocks deposited since startup. A diagnostic, counting what the *display* side has seen,
    /// which is the number worth watching on a device.
    pub blocks: AtomicU64,
}

impl AppState {
    pub fn new(capture: Arc<Capture>, metrics: Metrics, settings: SettingsStore) -> AppState {
        AppState {
            capture,
            metrics: Mutex::new(metrics),
            settings: Mutex::new(settings),
            blocks: AtomicU64::new(0),
        }
    }
}

/// Rounds to 0.1 dB, spec §9.1's resolution for every number on the wire.
///
/// Done here rather than in the frontend so there is exactly one answer to *what does the meter
/// say* — the same reason the calibration offset stays in Rust (spec §8.2).
fn round_01(value: f64) -> f64 {
    (value * 10.0).round() / 10.0
}

/// Calibrate, then round. In that order, because rounding first would let the 0.05 dB of
/// rounding error ride through the addition rather than being the last thing that happens.
fn published(settings: &Settings, raw_db: Option<f64>) -> Option<f64> {
    settings.calibrated(raw_db).map(round_01)
}

/// Spec §9.4's three states, from the two authoritative sources.
///
/// Pure and taking both answers as arguments so the mapping is testable off a device, where the
/// interesting one — `denied` — cannot be produced at all (spec §9.4's desktop caveat).
///
/// **Denial wins over everything**, including a stream that built: on a denied microphone cpal
/// happily delivers buffers of exact zeros, so the stream *is* running and it is saying nothing.
/// `Undetermined` — the permission prompt still on screen — lands on `unavailable`, which is
/// literally true while it is up: no category, no stream, no input.
fn input_state(permission: Option<MicPermission>, capture: &CaptureState) -> Input {
    if permission == Some(MicPermission::Denied) {
        return Input::Denied;
    }
    match capture {
        CaptureState::Running(_) => Input::Capturing,
        CaptureState::Starting | CaptureState::Failed { .. } => Input::Unavailable,
    }
}

/// Assembles one tick from already-gathered inputs.
///
/// Split from [`collect`] so the wire shape can be tested without an app, a stream or a clock.
fn tick_payload(
    now_slot: u64,
    levels: Levels,
    cal: (Option<f64>, f64),
    settings: Settings,
    input: Input,
) -> Tick {
    let (cal_leq, cal_coverage_s) = cal;
    Tick {
        now_slot,
        meter: Meter {
            leq: published(&settings, levels.leq),
            inst: published(&settings, levels.now),
            max: published(&settings, levels.max),
            coverage_s: round_01(levels.coverage_s),
            cal_leq: published(&settings, cal_leq),
            cal_coverage_s: round_01(cal_coverage_s),
            input,
        },
        settings: settings.into(),
        columns: Vec::new(),
    }
}

/// Drains the queue into the ring, advances everything, and reads off what to paint.
///
/// This is the whole of the tick's work and it runs under the `metrics` lock, which is what lets
/// a weighting change clear the window without a block from the previous chain landing in it
/// afterwards (`b04` finding 2).
fn collect(state: &AppState, now: Instant) -> Tick {
    // One read serves the rate cross-check, the input state and the health check.
    let capture = state.capture.state();

    // Spec §4.2 item 4: `inputNumberOfChannels == 0` is pollable, and it is polled here because
    // the tick is the one thing in this app that already runs forever at a known rate. It is
    // deliberately **independent of** the interruption notification — `11` probe 2 measured a
    // stream that died permanently with nothing at all reaching Rust — and it costs one property
    // read per 100 ms, off the audio path. It asks; the capture thread rebuilds.
    state.capture.check_health(&capture);

    let mut metrics = state.metrics.lock().unwrap();

    // The **granted** rate is only known once the stream is up, and it arrives on the capture
    // thread some time after this one starts. Nothing has been deposited before that moment, so
    // correcting it here is exact rather than approximate — and the same path serves `b07`'s
    // route changes, where spec §6.11 says a rate change clears neither the window nor the hold.
    if let CaptureState::Running(facts) = &capture {
        if facts.sample_rate as f64 != metrics.sample_rate() {
            eprintln!(
                "tick: sample rate {} Hz → {} Hz",
                metrics.sample_rate(),
                facts.sample_rate
            );
            metrics.set_sample_rate(facts.sample_rate as f64);
        }
    }

    // One `now` for the whole drain: spec §6.3 assigns a block **whole** to whichever slot is
    // current when it is drained, which is what keeps placement approximate to ±21 ms while the
    // coverage arithmetic stays exact.
    let drained = state.capture.drain(|block| metrics.deposit(now, block));
    state.blocks.fetch_add(drained as u64, Ordering::Relaxed);

    let levels = metrics.levels(now);
    let cal = metrics.leq_over(CAL_SLICE_S);
    // After `levels`, which is what advanced the ring.
    let now_slot = metrics.now_slot();
    drop(metrics);

    let settings = state.settings.lock().unwrap().settings();

    tick_payload(
        now_slot,
        levels,
        cal,
        settings,
        input_state(session::permission(), &capture),
    )
}

/// Starts the publishing thread. Called once, from `setup`, after the state is managed.
pub fn spawn(app: AppHandle) {
    std::thread::Builder::new()
        .name("tick".into())
        .spawn(move || tick_loop(app))
        .expect("spawn tick thread");
}

/// A dedicated std thread with a sleep-until loop on [`Instant`] (spec §16.6).
///
/// Sleep-until rather than sleep-for so the interval does not accumulate the drift of however
/// long a publish took.
fn tick_loop(app: AppHandle) {
    let mut next = Instant::now();
    let mut emit_failed = false;

    loop {
        next += TICK_INTERVAL;
        let now = Instant::now();
        if next > now {
            std::thread::sleep(next - now);
        } else if now - next > TICK_INTERVAL {
            // More than a whole period behind — a suspended app, or a device that slept. Re-base
            // rather than firing the burst of catch-up ticks a naive sleep-until would produce:
            // the tick is a *snapshot*, so the missed ones have nothing to say that this one does
            // not, and the data a late tick would otherwise drop rides in `columns` instead.
            next = now;
        }

        let now = Instant::now();
        let payload = collect(&app.state::<AppState>(), now);

        match app.emit(EVENT, &payload) {
            Ok(()) => emit_failed = false,
            // Logged once per run of failures rather than at 10 Hz. A stalled webview is not a
            // failure here — wry queues the script — so this only fires on something structural.
            Err(e) if !emit_failed => {
                emit_failed = true;
                eprintln!("tick: emit failed: {e}");
            }
            Err(_) => {}
        }
    }
}

// ─── the six commands ───────────────────────────────────────────────────────────────────────
//
// `rename_all = "snake_case"` on every one of them: Tauri's default is camelCase argument names,
// and spec §9.2 wants a Rust field name and its TS name to be literally the same string.
//
// There is deliberately **no `get_settings`** — the first tick arrives ≤100 ms after the listener
// registers — and **no clear-calibration command**: uncalibrated is the initial state, and a wrong
// offset is retyped (spec §9.1). `get_spectrogram` is the seventh and belongs to `b10`.
//
// Return values exist for **feel, not truth** (spec §9.2): a picker tap updates from the
// authoritative return rather than waiting up to 100 ms for the next tick.
//
// **Each command is a one-line delegate to an inherent method on [`AppState`].** The bodies are
// where spec §6.11's table is actually composed — two or three calls apiece, because `b04`
// deliberately kept the side effects out of `settings.rs` — and `State<'_, _>` cannot be built
// outside a running Tauri app, so keeping the composition off the command fn is what lets the
// whole table be tested with no app, no hardware and no clock.
//
// What that leaves untested, stated rather than glossed: **the argument names**. Tauri's default
// is camelCase and `rename_all = "snake_case"` overrides it, and a mismatch there is a runtime
// rejection rather than a compile error — the same class of cost §9.2 already accepts for field
// names. It surfaces on the first press of a button.

impl AppState {
    /// C / A / Z — spec §6.11's second row, **all three columns**, in the order `b04` finding 2
    /// requires.
    ///
    /// The callback picks up the new chain at its next block boundary, so blocks weighted the old
    /// way may already be sitting in the queue. Draining and discarding them *between* the switch
    /// and the window clear is what stops that energy landing in the fresh window — invisible on
    /// screen if it goes wrong, which is why the order is spelled out rather than left to read
    /// naturally. The `metrics` lock is held across the whole sequence so the tick thread cannot
    /// deposit in the middle of it, and one block still in flight in the callback is the ±21 ms
    /// spec §6.3 already tolerates.
    pub fn apply_weighting(&self, weighting: Weighting) -> WireSettings {
        let mut metrics = self.metrics.lock().unwrap();
        self.capture.set_weighting(weighting);
        self.capture.drain(|_| {});
        metrics.on_weighting_change();
        self.settings
            .lock()
            .unwrap()
            .set_weighting(weighting)
            .into()
    }

    /// F / S. Clears the max hold and **not** the window (spec §6.11): `L_CFmax` and `L_CSmax`
    /// differ by several dB on speech and neither is visible in the number itself, while the
    /// `L_eq` accumulator is untouched, not even as a flicker.
    pub fn apply_time_weighting(&self, time_weighting: TimeWeighting) -> WireSettings {
        let mut metrics = self.metrics.lock().unwrap();
        metrics.set_time_weighting(time_weighting);
        self.settings
            .lock()
            .unwrap()
            .set_time_weighting(time_weighting)
            .into()
    }

    /// 10 / 30 / 60 / 120 s. **Clears nothing** — the ring re-slices (spec §6.5), so `60 → 120`
    /// reads `60s of 120s` rather than starting from empty.
    ///
    /// Validated by the store *first*: the ring is only re-sliced once the value is known good, so
    /// a rejected length leaves both halves untouched rather than half-applied.
    pub fn apply_window_length(&self, window_s: u32) -> Result<WireSettings, String> {
        let mut metrics = self.metrics.lock().unwrap();
        let settings = self
            .settings
            .lock()
            .unwrap()
            .set_window_s(window_s)
            .map_err(|e| e.to_string())?;
        metrics.set_window_s(window_s);
        Ok(settings.into())
    }

    /// Type what the proper meter reads, and store the difference (spec §8.5).
    ///
    /// Matched against a **fixed 10 s slice of the ring, independent of the display's window**
    /// (spec §8.3), and bounds-checked at 0–140 dB on the *typed* value — `683` for `68.3` would
    /// otherwise store a ~600 dB offset. The derived offset itself is unclamped on purpose: a
    /// legitimate offset near +100 dB would sit inside any plausible bound only by luck.
    ///
    /// **Clears nothing** (spec §6.11's bottom row), which is what makes the calibration gesture —
    /// repeatedly nudging while watching the number — interactive rather than a reset each time.
    pub fn apply_calibration_from_reference(
        &self,
        reference_db: f64,
    ) -> Result<WireSettings, String> {
        let raw_slice_leq = self.metrics.lock().unwrap().leq_over(CAL_SLICE_S).0;
        self.settings
            .lock()
            .unwrap()
            .set_offset_from_reference(reference_db, raw_slice_leq)
            .map(WireSettings::from)
            .map_err(|e| e.to_string())
    }

    /// The offset typed directly, or trimmed by ±0.1 dB. Clears nothing.
    ///
    /// Directly editable is not decoration: free provisioning expires every 7 days, so the app is
    /// re-signed and reinstalled weekly and delete-then-install loses the data container. A
    /// visible, editable offset makes recovery a sticky note and a retype (spec §8.5).
    pub fn apply_calibration_offset(&self, offset_db: f64) -> Result<WireSettings, String> {
        self.settings
            .lock()
            .unwrap()
            .set_offset_db(offset_db)
            .map(WireSettings::from)
            .map_err(|e| e.to_string())
    }

    /// *Start measuring this talk*: clears the window **and** the max hold (spec §6.8), and **not**
    /// the picture (spec §9.3) or the smoother (a live value, not history).
    pub fn apply_reset(&self) -> WireSettings {
        self.metrics.lock().unwrap().reset();
        self.settings.lock().unwrap().settings().into()
    }
}

#[tauri::command(rename_all = "snake_case")]
pub fn set_weighting(state: State<'_, AppState>, weighting: Weighting) -> WireSettings {
    state.apply_weighting(weighting)
}

#[tauri::command(rename_all = "snake_case")]
pub fn set_time_weighting(
    state: State<'_, AppState>,
    time_weighting: TimeWeighting,
) -> WireSettings {
    state.apply_time_weighting(time_weighting)
}

#[tauri::command(rename_all = "snake_case")]
pub fn set_window_length(
    state: State<'_, AppState>,
    window_s: u32,
) -> Result<WireSettings, String> {
    state.apply_window_length(window_s)
}

#[tauri::command(rename_all = "snake_case")]
pub fn set_calibration_from_reference(
    state: State<'_, AppState>,
    reference_db: f64,
) -> Result<WireSettings, String> {
    state.apply_calibration_from_reference(reference_db)
}

#[tauri::command(rename_all = "snake_case")]
pub fn set_calibration_offset(
    state: State<'_, AppState>,
    offset_db: f64,
) -> Result<WireSettings, String> {
    state.apply_calibration_offset(offset_db)
}

/// Returns the settings — unchanged, since Reset is not a setting — only so the frontend has one
/// shape for all six commands. The refill is what actually reports the result, honestly and within
/// a tick: `0s of 60s` climbing.
#[tauri::command(rename_all = "snake_case")]
pub fn reset(state: State<'_, AppState>) -> WireSettings {
    state.apply_reset()
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::atomic::AtomicU32;

    use super::*;

    use crate::capture::{BlockSummary, CaptureFacts};

    /// Raw dBFS levels chosen so that, at spec §9.1's own +101.4 dB offset, they land exactly on
    /// spec §9.1's own published numbers.
    fn levels() -> Levels {
        Levels {
            now: Some(-30.2),
            leq: Some(-33.0),
            max: Some(-26.5),
            coverage_s: 41.3,
        }
    }

    fn calibrated() -> Settings {
        Settings {
            offset_db: Some(101.4),
            ..Settings::default()
        }
    }

    fn running() -> CaptureState {
        CaptureState::Running(CaptureFacts {
            device: "test".into(),
            sample_format: "f32".into(),
            sample_rate: 48_000,
            channels: 1,
            buffer_frames: Some(1024),
            session: None,
        })
    }

    /// The wire contract is **verbatim** in spec §9.1, and `vue-tsc` cannot check it — a renamed
    /// field is a runtime `undefined` in `src/bridge.ts`, not a compile error. This pins the Rust
    /// half: every key, its spelling, its nesting and its order.
    ///
    /// The values are §9.1's own example, which is why the expected string can be read straight
    /// against the spec.
    #[test]
    fn the_wire_shape_is_section_9_1_verbatim() {
        let tick = tick_payload(
            418_752,
            levels(),
            (Some(-33.3), 10.0),
            calibrated(),
            Input::Capturing,
        );

        assert_eq!(
            serde_json::to_string(&tick).unwrap(),
            r#"{"now_slot":418752,"meter":{"leq":68.4,"inst":71.2,"max":74.9,"coverage_s":41.3,"cal_leq":68.1,"cal_coverage_s":10.0,"input":"capturing"},"settings":{"weighting":"C","time_weighting":"S","window_s":60,"offset_db":101.4,"unit":"dB"},"columns":[]}"#
        );
    }

    /// ≈460 bytes is not decoration: above 8192 Tauri's IPC leaves its fast path, and this design
    /// publishes 10 times a second forever.
    #[test]
    fn the_tick_stays_on_the_ipc_fast_path() {
        let tick = tick_payload(
            418_752,
            levels(),
            (Some(-33.3), 10.0),
            calibrated(),
            Input::Capturing,
        );
        let bytes = serde_json::to_string(&tick).unwrap().len();
        assert!(
            bytes < 8192,
            "tick is {bytes} bytes, over the bulk-path threshold"
        );
    }

    /// `--` is `null`, never a sentinel like `-999` (spec §9.2). The coverage figure is **not**
    /// optional and rides along at zero, because an indicator that appears only when something is
    /// wrong is a warning.
    #[test]
    fn an_absent_number_is_null_and_coverage_is_published_anyway() {
        let nothing = Levels {
            now: None,
            leq: None,
            max: None,
            coverage_s: 0.0,
        };
        let tick = tick_payload(7, nothing, (None, 0.0), Settings::default(), Input::Denied);
        let json = serde_json::to_string(&tick).unwrap();

        assert_eq!(
            json,
            r#"{"now_slot":7,"meter":{"leq":null,"inst":null,"max":null,"coverage_s":0.0,"cal_leq":null,"cal_coverage_s":0.0,"input":"denied"},"settings":{"weighting":"C","time_weighting":"S","window_s":60,"offset_db":null,"unit":"dBFS"},"columns":[]}"#
        );
        assert!(!json.contains("-999"));
    }

    /// Spec §8.2: the offset is applied to **all three** published levels and to the calibration
    /// slice, post-log, in Rust. It is emphatically **not** applied to the coverage figures, which
    /// are seconds.
    #[test]
    fn every_db_value_crossing_the_bridge_is_calibrated() {
        let raw = tick_payload(
            0,
            levels(),
            (Some(-33.3), 10.0),
            Settings::default(),
            Input::Capturing,
        );
        let cal = tick_payload(
            0,
            levels(),
            (Some(-33.3), 10.0),
            calibrated(),
            Input::Capturing,
        );

        assert_eq!(raw.meter.leq, Some(-33.0));
        assert_eq!(raw.meter.inst, Some(-30.2));
        assert_eq!(raw.meter.max, Some(-26.5));
        assert_eq!(raw.meter.cal_leq, Some(-33.3));

        assert_eq!(cal.meter.leq, Some(68.4));
        assert_eq!(cal.meter.inst, Some(71.2));
        assert_eq!(cal.meter.max, Some(74.9));
        assert_eq!(cal.meter.cal_leq, Some(68.1));

        assert_eq!(cal.meter.coverage_s, raw.meter.coverage_s);
        assert_eq!(cal.meter.cal_coverage_s, raw.meter.cal_coverage_s);
    }

    /// The unit travels **with** the values in the same payload (spec §9.2), so a number can never
    /// be painted under the wrong label. `dBFS` is a correctly-named different quantity, not an
    /// error state (spec §8.6).
    #[test]
    fn the_unit_travels_with_the_numbers() {
        let uncalibrated = tick_payload(
            0,
            levels(),
            (None, 0.0),
            Settings::default(),
            Input::Capturing,
        );
        assert_eq!(uncalibrated.settings.unit, Unit::DbFs);
        assert_eq!(uncalibrated.settings.offset_db, None);

        let calibrated = tick_payload(0, levels(), (None, 0.0), calibrated(), Input::Capturing);
        assert_eq!(calibrated.settings.unit, Unit::Db);
    }

    /// 0.1 dB, spec §9.1's resolution, including the offset itself.
    #[test]
    fn values_are_rounded_to_a_tenth_of_a_decibel() {
        let noisy = Levels {
            now: Some(-30.246_913),
            leq: Some(-33.049_999),
            max: Some(-26.55),
            coverage_s: 41.279_998,
        };
        let settings = Settings {
            offset_db: Some(101.438_71),
            ..Settings::default()
        };
        let tick = tick_payload(0, noisy, (None, 9.964), settings, Input::Capturing);

        assert_eq!(tick.meter.inst, Some(71.2));
        assert_eq!(tick.meter.leq, Some(68.4));
        assert_eq!(tick.meter.max, Some(74.9));
        assert_eq!(tick.meter.coverage_s, 41.3);
        assert_eq!(tick.meter.cal_coverage_s, 10.0);
        assert_eq!(tick.settings.offset_db, Some(101.4));
    }

    /// Spec §9.4's mapping, all six combinations that can arise.
    ///
    /// The load-bearing row is the first: a **denied microphone still builds a stream** and
    /// delivers exact zeros, so `Running` must not be allowed to report `capturing` over the top
    /// of an authoritative denial. Getting this backwards is invisible on the desk — macOS never
    /// answers `Denied` at all — and on the phone it is the one failure the user can fix.
    #[test]
    fn the_input_state_reads_the_permission_before_the_stream() {
        use MicPermission::{Denied, Granted, Undetermined};

        assert_eq!(input_state(Some(Denied), &running()), Input::Denied);
        assert_eq!(
            input_state(Some(Denied), &CaptureState::Starting),
            Input::Denied
        );

        assert_eq!(input_state(Some(Granted), &running()), Input::Capturing);
        assert_eq!(
            input_state(Some(Granted), &CaptureState::Starting),
            Input::Unavailable
        );
        assert_eq!(
            input_state(
                Some(Granted),
                &CaptureState::Failed {
                    reason: "no default input device".into()
                }
            ),
            Input::Unavailable
        );

        // The prompt is still on screen: no category, no stream, no input — `unavailable` is
        // literally true while it is up.
        assert_eq!(
            input_state(Some(Undetermined), &CaptureState::Starting),
            Input::Unavailable
        );
    }

    /// Spec §9.4's desktop caveat, asserted rather than merely noted: off iOS there is no
    /// `recordPermission` to read, so a running stream reports `capturing` and `denied` can never
    /// appear. This is why `b06` has to drive the input-state line from a stub to see it.
    #[test]
    fn without_a_permission_answer_a_running_stream_is_capturing() {
        assert_eq!(input_state(None, &running()), Input::Capturing);
        assert_eq!(
            input_state(None, &CaptureState::Starting),
            Input::Unavailable
        );
    }

    // ── the command site: spec §6.11's table, where it is actually composed ──────────────────
    //
    // Every one of these runs through [`collect`], the real publish path, so what is asserted is
    // what the frontend would have been sent — not an intermediate the screen never sees.

    const FS: f64 = 48_000.0;
    /// The block the granted route actually delivers (`b01` finding 3).
    const FRAMES: u32 = 1024;
    /// 1024 / 48 000 s = 21.333 ms — deliberately not a whole number of slots.
    const BLOCK: Duration = Duration::from_nanos(21_333_333);
    /// A full-scale sine's mean square, and its level under spec §16.4's convention.
    const TONE_MS: f64 = 0.5;
    const TONE_DB: f64 = -3.010_299_956_639_812;
    /// One slot plus one block: the real quantisation of the coverage figure (`b03`).
    const COVERAGE_TOLERANCE: f64 = 0.2;

    /// The app, minus Tauri and minus the microphone.
    ///
    /// [`Capture::new`] gives a capture side with no thread, no session and no stream, which is
    /// everything the commands touch — so the whole table below runs with **no hardware and no
    /// sleeping**, the property `b03` designed the metrics pipeline around. The construction
    /// mirrors `lib.rs`'s `setup`.
    struct Rig {
        state: AppState,
        now: Instant,
    }

    impl Rig {
        fn new() -> Rig {
            Rig::with_store(SettingsStore::in_memory(Settings::default()))
        }

        fn with_store(store: SettingsStore) -> Rig {
            let start = Instant::now();
            let settings = store.settings();
            let mut metrics = Metrics::new(FS, start);
            metrics.set_window_s(settings.window_s);
            metrics.set_time_weighting(settings.time_weighting);
            Rig {
                state: AppState::new(Capture::new(settings.weighting), metrics, store),
                now: start,
            }
        }

        /// Deposits `secs` of steady tone on a synthetic clock, one block at a time, exactly as
        /// the tick thread does.
        fn run(&mut self, secs: f64, mean_square: f64) {
            let end = self.now + Duration::from_secs_f64(secs);
            let mut metrics = self.state.metrics.lock().unwrap();
            while self.now + BLOCK <= end {
                self.now += BLOCK;
                metrics.deposit(
                    self.now,
                    BlockSummary {
                        sum_sq: mean_square * FRAMES as f64,
                        n: FRAMES,
                        t: self.now,
                    },
                );
            }
            drop(metrics);
            self.now = end;
        }

        /// What the frontend would have been sent, right now.
        fn tick(&self) -> Tick {
            collect(&self.state, self.now)
        }
    }

    fn assert_coverage(actual: f64, expected_s: f64) {
        assert!(
            (actual - expected_s).abs() <= COVERAGE_TOLERANCE,
            "coverage {actual} s, expected {expected_s} s"
        );
    }

    /// **The tick is a timer, not audio-driven** (spec §16.6). With no audio at all it still
    /// publishes a complete snapshot — which is what makes settings-in-every-tick and the input
    /// state safe on a stream that never started.
    #[test]
    fn the_tick_publishes_a_complete_snapshot_with_no_audio_at_all() {
        let tick = Rig::new().tick();
        assert_eq!(tick.meter.leq, None);
        assert_eq!(tick.meter.inst, None);
        assert_eq!(tick.meter.max, None);
        assert_eq!(tick.meter.coverage_s, 0.0);
        assert_eq!(tick.meter.input, Input::Unavailable);
        assert_eq!(tick.settings.window_s, 60);
        assert_eq!(tick.settings.unit, Unit::DbFs);
        assert!(tick.columns.is_empty());
    }

    /// **`now_slot` is what makes the picture's right edge honest** (spec §9.2). With no audio at
    /// all, only the clock advances the ring — and without that index a run of gap slots up to the
    /// present is invisible, because no columns arrive and so nothing says the silence is *current*.
    ///
    /// It is also the ordering assertion for [`collect`]: `now_slot` is read **after** `levels`,
    /// which is what advances the ring. Read before, and the right edge is a tick stale — and a
    /// whole 7 s of silence would be reported as the present.
    #[test]
    fn the_ring_index_advances_on_the_clock_with_no_audio() {
        let mut rig = Rig::new();
        assert_eq!(rig.tick().now_slot, 0);

        rig.now += Duration::from_secs(7);
        let tick = rig.tick();
        assert_eq!(tick.now_slot, 70, "100 ms slots, so 7 s is 70 of them");
        assert_eq!(tick.meter.coverage_s, 0.0);
        assert!(tick.columns.is_empty());
    }

    /// Spec §6.11's second row, all three columns, as `b04` finding 7 corrected it — including the
    /// smoother, without which the max-hold clear is defeated by the very next block and the hold
    /// stays ~7 dB high for the rest of the session.
    #[test]
    fn a_weighting_change_clears_the_window_the_hold_and_the_smoother() {
        let mut rig = Rig::new();
        rig.run(20.0, TONE_MS);

        let before = rig.tick();
        assert!(before.meter.leq.is_some());
        assert!(before.meter.inst.is_some());
        assert!(before.meter.max.is_some());
        assert_coverage(before.meter.coverage_s, 20.0);

        let settings = rig.state.apply_weighting(Weighting::A);
        assert_eq!(settings.weighting, Weighting::A);
        // The callback's own view — the thing that actually filters the next block.
        assert_eq!(rig.state.capture.weighting(), Weighting::A);

        let after = rig.tick();
        assert_eq!(after.meter.leq, None, "the window was not cleared");
        assert_eq!(after.meter.max, None, "the max hold was not cleared");
        assert_eq!(after.meter.inst, None, "the smoother was not cleared");
        assert_eq!(after.meter.coverage_s, 0.0);
        assert_eq!(after.settings.weighting, Weighting::A);
    }

    /// Spec §6.11's third row: the max hold and **nothing else**. `L_CFmax` and `L_CSmax` differ by
    /// several dB on speech, while the `L_eq` accumulator is untouched — not even as a flicker —
    /// and §6.6 keeps the smoother state, because it is the same quantity at a different τ.
    #[test]
    fn a_time_weighting_change_clears_the_hold_and_leaves_the_window_and_smoother_alone() {
        let mut rig = Rig::new();
        rig.run(20.0, TONE_MS);
        let before = rig.tick();

        let settings = rig.state.apply_time_weighting(TimeWeighting::Fast);
        assert_eq!(settings.time_weighting, TimeWeighting::Fast);

        let after = rig.tick();
        assert_eq!(after.meter.max, None, "the max hold was not cleared");
        assert_eq!(after.meter.leq, before.meter.leq, "the window was cleared");
        assert_eq!(after.meter.coverage_s, before.meter.coverage_s);
        assert_eq!(
            after.meter.inst, before.meter.inst,
            "the smoother was reset"
        );
    }

    /// Spec §6.5: the ring holds valid energy at any length, so a window change is nothing but
    /// *how far back do I sum*. The `10 → 120` leg is the one that matters — data the ring still
    /// held has to come back, which a reset-on-change would have thrown away.
    #[test]
    fn a_window_change_re_slices_the_ring_rather_than_resetting_it() {
        let mut rig = Rig::new();
        rig.run(30.0, TONE_MS);
        assert_coverage(rig.tick().meter.coverage_s, 30.0);

        rig.state
            .apply_window_length(10)
            .expect("10 s is permitted");
        let at_10 = rig.tick();
        assert_coverage(at_10.meter.coverage_s, 10.0);
        assert!(at_10.meter.leq.is_some(), "re-slicing emptied the ring");
        assert_eq!(at_10.settings.window_s, 10);

        rig.state
            .apply_window_length(120)
            .expect("120 s is permitted");
        assert_coverage(rig.tick().meter.coverage_s, 30.0);
    }

    /// The picker can only ever send 10 / 30 / 60 / 120, so this arrives from a hand-edited file or
    /// a mistaken command. It must leave **both** halves untouched rather than half-applied — the
    /// reason the store validates before the ring is re-sliced.
    ///
    /// The ring is filled past the rejected length on purpose. With less audio than 45 s in it, a
    /// ring wrongly re-sliced to 45 s reports exactly the coverage a 60 s one does, and the test
    /// passes over the top of the bug — which is what it did before this line was written.
    #[test]
    fn an_unsupported_window_length_changes_nothing_on_either_side() {
        let mut rig = Rig::new();
        rig.run(50.0, TONE_MS);

        let error = rig
            .state
            .apply_window_length(45)
            .expect_err("45 s is not one of the four");
        assert!(error.contains("45"), "{error}");

        let tick = rig.tick();
        assert_eq!(tick.settings.window_s, 60);
        assert_coverage(tick.meter.coverage_s, 50.0);
    }

    /// Spec §8.5's bounds check, on the value that is **typed**: `683` for `68.3` would otherwise
    /// store a ~600 dB offset, and every number on screen would be authoritative-looking and wrong
    /// by an unknown amount.
    #[test]
    fn the_683_typo_is_rejected_and_leaves_the_meter_uncalibrated() {
        let mut rig = Rig::new();
        rig.run(12.0, TONE_MS);

        let error = rig
            .state
            .apply_calibration_from_reference(683.0)
            .expect_err("683 dB is a typo for 68.3");
        assert!(error.contains("683"), "{error}");

        let tick = rig.tick();
        assert_eq!(tick.settings.offset_db, None);
        assert_eq!(tick.settings.unit, Unit::DbFs);
        // Refusing it must not also throw away the audio it was going to be matched against.
        assert_coverage(tick.meter.cal_coverage_s, 10.0);
    }

    /// Arithmetic, not policy (spec §8.3): with nothing in the 10 s slice the `L_eq` is undefined,
    /// so there is no offset to derive. Below *full* coverage nothing is refused — the figure is
    /// shown and the reader judges.
    #[test]
    fn calibrating_with_nothing_to_match_is_refused_rather_than_invented() {
        let error = Rig::new()
            .state
            .apply_calibration_from_reference(68.3)
            .expect_err("an empty slice cannot be matched");
        assert!(error.contains("nothing to match"), "{error}");
    }

    /// **Calibrating is the one settings act that changes every number on screen and clears
    /// nothing** (spec §8.2, §6.11's bottom row) — which is what makes the gesture, nudging while
    /// watching the number, interactive rather than a reset each time.
    #[test]
    fn typing_the_reference_moves_every_number_at_once_and_clears_nothing() {
        let mut rig = Rig::new();
        rig.run(20.0, TONE_MS);
        let before = rig.tick();

        let settings = rig
            .state
            .apply_calibration_from_reference(68.3)
            .expect("a full 10 s slice");
        let offset = settings.offset_db.expect("an offset was stored");
        assert!((offset - (68.3 - TONE_DB)).abs() < 0.05, "offset {offset}");

        let after = rig.tick();
        assert_eq!(after.settings.unit, Unit::Db);
        assert!((after.meter.leq.expect("a level") - 68.3).abs() < 0.05);

        // All four, by the same amount: `max(xᵢ + c) = max(xᵢ) + c` is why the hold is included.
        for (was, now) in [
            (before.meter.leq, after.meter.leq),
            (before.meter.inst, after.meter.inst),
            (before.meter.max, after.meter.max),
            (before.meter.cal_leq, after.meter.cal_leq),
        ] {
            let moved = now.expect("a level") - was.expect("a level");
            assert!(
                (moved - offset).abs() < 0.11,
                "moved by {moved}, not {offset}"
            );
        }

        assert_eq!(after.meter.coverage_s, before.meter.coverage_s);
        assert_eq!(after.meter.cal_coverage_s, before.meter.cal_coverage_s);
    }

    /// Spec §6.8. One button, because it is not "clear the max", it is *start measuring this talk*.
    ///
    /// It deliberately does **not** touch the smoother — NOW is a live value, not history — and
    /// does not touch the calibration, which would make a mis-tapped Reset expensive.
    #[test]
    fn reset_clears_the_window_and_the_hold_and_nothing_else() {
        let mut rig = Rig::new();
        rig.run(20.0, TONE_MS);
        rig.state
            .apply_calibration_offset(101.4)
            .expect("a finite offset");
        let before = rig.tick();

        assert_eq!(rig.state.apply_reset().offset_db, Some(101.4));

        let after = rig.tick();
        assert_eq!(after.meter.leq, None);
        assert_eq!(after.meter.max, None);
        assert_eq!(after.meter.coverage_s, 0.0);
        assert_eq!(
            after.meter.inst, before.meter.inst,
            "the smoother was reset"
        );
        assert_eq!(after.settings.offset_db, Some(101.4));
        assert_eq!(after.settings.unit, Unit::Db);
    }

    /// **A command's answer and the next tick's answer are the same object**, field for field.
    ///
    /// This is what this ticket's dev-loop pass caught: the commands were returning
    /// [`crate::settings::Settings`] — the same four values with **no `unit`** and an unrounded
    /// offset — while the tick carried [`WireSettings`]. Nothing failed loudly, because `vue-tsc`
    /// cannot see across the bridge and a missing field is `undefined` at runtime. It matters
    /// because `b06` takes picker feedback from the return value rather than the next tick, so the
    /// one act that changes the unit would have painted its number under no unit at all.
    #[test]
    fn a_command_answers_with_the_same_settings_the_next_tick_carries() {
        let mut rig = Rig::new();
        rig.run(20.0, TONE_MS);

        let returned = rig
            .state
            .apply_calibration_from_reference(68.3)
            .expect("a full 10 s slice");
        let ticked = rig.tick().settings;

        assert_eq!(returned, ticked);
        assert_eq!(
            serde_json::to_string(&returned).unwrap(),
            serde_json::to_string(&ticked).unwrap(),
            "one settings shape on the wire, not two"
        );
        // The field that was silently absent, and the one that was silently unrounded.
        assert_eq!(returned.unit, Unit::Db);
        assert_eq!(returned.offset_db, Some(round_01(68.3 - TONE_DB)));
    }

    /// Spec §10's write-through, from the command site rather than from the store's own tests: the
    /// app dies on backgrounding, possibly without running shutdown code, so a deferred write is a
    /// lost setting — and the offset is recoverable only by standing next to the reference meter
    /// again.
    #[test]
    fn every_setting_a_command_changes_is_on_disk_before_the_next_launch() {
        let dir = TempDir::new();
        let rig = Rig::with_store(SettingsStore::load(Some(dir.file())));

        rig.state.apply_weighting(Weighting::A);
        rig.state.apply_time_weighting(TimeWeighting::Fast);
        rig.state
            .apply_window_length(120)
            .expect("120 s is permitted");
        rig.state
            .apply_calibration_offset(101.4)
            .expect("a finite offset");

        // Exactly what the next launch reads.
        let reloaded = SettingsStore::load(Some(dir.file())).settings();
        assert_eq!(reloaded.weighting, Weighting::A);
        assert_eq!(reloaded.time_weighting, TimeWeighting::Fast);
        assert_eq!(reloaded.window_s, 120);
        assert_eq!(reloaded.offset_db, Some(101.4));

        // And a fresh start comes up on them, rather than on the four defaults.
        let restarted = Rig::with_store(SettingsStore::load(Some(dir.file())));
        assert_eq!(restarted.state.capture.weighting(), Weighting::A);
        let tick = restarted.tick();
        assert_eq!(tick.settings.window_s, 120);
        assert_eq!(tick.settings.time_weighting, TimeWeighting::Fast);
        assert_eq!(tick.settings.unit, Unit::Db);
    }

    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> TempDir {
            static NEXT: AtomicU32 = AtomicU32::new(0);
            let dir = std::env::temp_dir().join(format!(
                "decibel-meter-b05-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir_all(&dir).expect("create temp dir");
            TempDir(dir)
        }

        fn file(&self) -> PathBuf {
            self.0.join(crate::settings::FILE_NAME)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
}
