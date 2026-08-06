//! The four settings, their JSON file, and the calibration arithmetic — spec §8 and §10.
//!
//! **All four settings are Rust-owned; the frontend issues commands and holds nothing**
//! (spec §10). This module owns the values and the file. It deliberately owns *neither* of the
//! two side effects a setting change has:
//!
//! - clearing the window and the max hold belongs to [`crate::metrics`] (spec §6.11), and
//! - zeroing the filter state belongs to the audio callback in [`crate::capture`].
//!
//! A setting change is therefore two or three calls at the command site, which `b05` wires. The
//! alternative — a god object owning the ring, the stream and the file — would make the §6.11
//! table untestable, which is the one table this design most needs tested.
//!
//! **The calibration offset is applied here, post-log** (spec §8.2):
//! `published_dB = 10·log₁₀(Σp²/Σn) + c`. As a gain before squaring, the ring would hold
//! calibrated energy and changing the offset would invalidate everything already accumulated —
//! a third reset cause firing at the worst possible moment, since the calibration gesture *is*
//! repeatedly nudging the offset while watching the number. Post-log the number is identical and
//! **nothing resets**, which is what makes the gesture interactive.
//!
//! It stays in **Rust** rather than Vue so an uncalibrated number can never cross the bridge and
//! get rendered by mistake.

use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::metrics::{TimeWeighting, WINDOW_LENGTHS_S};
use crate::weighting::Weighting;

/// The file, inside `app.path().app_config_dir()` (spec §10).
///
/// One file for all four settings: one write, one read at startup. `serde` and `serde_json` are
/// already dependencies and `app_config_dir()` is core Tauri called from Rust rather than over
/// the bridge, so this needs **no new dependency, no npm package and no capability entry** —
/// `CLAUDE.md`'s four-step plugin ceremony does not apply.
pub const FILE_NAME: &str = "settings.json";

/// The default window length, in seconds (spec §10).
///
/// [`crate::metrics::Metrics`] starts here too; the two are not wired together, so `b05` sets
/// the loaded value at startup rather than relying on the coincidence.
pub const DEFAULT_WINDOW_S: u32 = 60;

/// Bounds on the **typed reference value** (spec §8.5).
///
/// A fat-fingered `683` for `68.3` would otherwise store a ~600 dB offset. The bound is on what
/// is typed, not on what is derived — see [`offset_from_reference`].
const REFERENCE_MIN_DB: f64 = 0.0;
const REFERENCE_MAX_DB: f64 = 140.0;

/// The unit the three numbers are published under (spec §9.1).
///
/// This rides in **every** tick beside the values, which extends spec §8.2's footgun-denial from
/// values to labels: a number can never be painted under the wrong unit because the unit travels
/// with it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Unit {
    /// Calibrated: an offset is stored, so the numbers are dB SPL.
    #[serde(rename = "dB")]
    Db,
    /// Uncalibrated — **a designed state, not an error** (spec §8.6). Not a wrong SPL but a
    /// correctly-named different quantity, with the unit carrying the honesty.
    #[serde(rename = "dBFS")]
    DbFs,
}

impl fmt::Display for Unit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Unit::Db => f.write_str("dB"),
            Unit::DbFs => f.write_str("dBFS"),
        }
    }
}

/// The four settings, and the whole of what is persisted (spec §10).
///
/// **Four values and nothing about the calibration itself** (spec §8.7) — no date, no record of
/// the reference value, no mode. The reasoning is sharper than resisting complexity: the offset
/// does not go stale with time, it goes stale with a **hardware change**. A displayed date would
/// direct attention to elapsed time — the wrong variable — while detecting nothing.
///
/// `#[serde(default)]` per field rather than a strict parse: an absent field falls back to its
/// default while the rest of the file survives. The value that matters is `offset_db`, which is
/// recoverable only by standing next to the reference meter again (spec §8.5), so a future added
/// field must not be able to take it down with it.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default = "default_weighting")]
    pub weighting: Weighting,
    #[serde(default)]
    pub time_weighting: TimeWeighting,
    #[serde(default = "default_window_s")]
    pub window_s: u32,
    /// `Option<f64>`, **not** a default of 0 (spec §8.6). A shipped default of ~+100 dB was
    /// rejected firmly: it would read roughly right out of the box at the cost of an
    /// authoritative-looking number wrong by an unknown amount, which is the precise failure
    /// this whole design is organised against.
    ///
    /// Serializes to `null` when unset — never a sentinel (spec §9.2).
    #[serde(default)]
    pub offset_db: Option<f64>,
}

fn default_weighting() -> Weighting {
    Weighting::C
}

fn default_window_s() -> u32 {
    DEFAULT_WINDOW_S
}

impl Default for Settings {
    /// `(None, C, S, 60)` — spelled out rather than derived, because **the default time
    /// weighting is `S`, not `F`**. `06 d5`'s own text still says `F`; it was superseded by
    /// `09 d5` and the correction is listed in spec §17.
    fn default() -> Self {
        Settings {
            weighting: Weighting::C,
            time_weighting: TimeWeighting::Slow,
            window_s: DEFAULT_WINDOW_S,
            offset_db: None,
        }
    }
}

impl Settings {
    /// `dB` once an offset is stored, `dBFS` while there is none (spec §8.6).
    pub fn unit(&self) -> Unit {
        match self.offset_db {
            Some(_) => Unit::Db,
            None => Unit::DbFs,
        }
    }

    /// Adds the offset **after the log**, to every dB value crossing the bridge (spec §8.2).
    ///
    /// `None` passes through as `None`: `--` is the instrument reporting that it has nothing to
    /// say, and calibrating a silence would produce a number out of nothing. Uncalibrated
    /// (`offset_db == None`) passes the raw value through unchanged — labelled `dBFS` by
    /// [`Settings::unit`], which is where the honesty lives.
    ///
    /// This is correct for **all three** published numbers, the max hold included, because
    /// `max(xᵢ + c) = max(xᵢ) + c`.
    pub fn calibrated(&self, raw_db: Option<f64>) -> Option<f64> {
        match (raw_db, self.offset_db) {
            (Some(raw), Some(offset)) => Some(raw + offset),
            (Some(raw), None) => Some(raw),
            (None, _) => None,
        }
    }
}

/// Why a setting was refused. Every variant is a value the user typed, never an internal fault:
/// a **write** failure is logged rather than surfaced (spec §10).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SettingsError {
    /// The typed reference value is outside ~0–140 dB (spec §8.5). This is the `683` case.
    ReferenceOutOfRange(f64),
    /// Zero coverage in the 10 s match slice, so the L_eq is undefined and the offset cannot be
    /// computed (spec §8.3). **Arithmetic, not policy** — below zero coverage nothing is
    /// refused, the figure is displayed and the reader judges.
    NoSignalToMatch,
    /// A non-finite offset or reference. The *derived* offset is deliberately unclamped, but it
    /// still has to be a number.
    NotFinite,
    /// A window length that is not one of 10 / 30 / 60 / 120 s (spec §6.5). The surface is a
    /// picker, so this can only arrive from a hand-edited file or a mistaken command.
    UnsupportedWindow(u32),
}

impl fmt::Display for SettingsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SettingsError::ReferenceOutOfRange(value) => write!(
                f,
                "reference {value} dB is outside {REFERENCE_MIN_DB}–{REFERENCE_MAX_DB} dB"
            ),
            SettingsError::NoSignalToMatch => {
                f.write_str("no audio in the 10 s calibration slice, so there is nothing to match")
            }
            SettingsError::NotFinite => f.write_str("not a finite number"),
            SettingsError::UnsupportedWindow(value) => {
                write!(
                    f,
                    "window length {value} s is not one of {WINDOW_LENGTHS_S:?}"
                )
            }
        }
    }
}

impl std::error::Error for SettingsError {}

/// `reference_db − raw_slice_leq`: the offset that makes the app read what the proper meter
/// reads (spec §8.5).
///
/// **Typing the reference value is the primary affordance.** You read 68.3 off the proper meter
/// and type 68.3; no mental arithmetic, one step from cold, and identical whether the offset is
/// already set or not. Nudge-only was ruled out by arithmetic rather than preference: `02`
/// measured an idle room at −59.6 dBFS in `Measurement` mode, which puts the offset on the order
/// of **+100 dB** — 1 000 taps at 0.1 dB from cold.
///
/// `raw_slice_leq` is [`crate::metrics::Metrics::leq_over`]`(10)` — a **fixed 10 s slice of the
/// ring, independent of the display's window setting** (spec §8.3). Its `None` is zero coverage
/// in that slice, and it is the only reason this returns [`SettingsError::NoSignalToMatch`].
///
/// **The typed reference is bounds-checked and the derived offset is not.** Those are different
/// claims about different numbers: `683` is a typo, whereas a legitimate offset near +100 dB
/// would sit inside any plausible bound only by luck.
pub fn offset_from_reference(
    reference_db: f64,
    raw_slice_leq: Option<f64>,
) -> Result<f64, SettingsError> {
    if !reference_db.is_finite() {
        return Err(SettingsError::NotFinite);
    }
    if !(REFERENCE_MIN_DB..=REFERENCE_MAX_DB).contains(&reference_db) {
        return Err(SettingsError::ReferenceOutOfRange(reference_db));
    }
    let raw = raw_slice_leq.ok_or(SettingsError::NoSignalToMatch)?;
    if !raw.is_finite() {
        return Err(SettingsError::NotFinite);
    }
    Ok(reference_db - raw)
}

/// The four settings plus the file they live in, writing through on **every** change.
///
/// Write-through rather than write-on-exit (spec §10): the app dies on backgrounding, possibly
/// without running shutdown code (`11` probe 2b), so a deferred write is a lost setting. Changes
/// are deliberate settings acts and the file is four values.
pub struct SettingsStore {
    settings: Settings,
    /// `None` when the config directory could not be resolved at all. The settings still apply
    /// in memory; they simply do not survive a restart. Refusing them would be worse.
    path: Option<PathBuf>,
}

impl SettingsStore {
    /// Reads `path`, falling back to `(None, C, S, 60)` on **missing or corrupt** with a log line
    /// and nothing in the UI (spec §10).
    ///
    /// Nothing is written here. A missing file is not a change, so the file appears on the first
    /// real settings act.
    pub fn load(path: Option<PathBuf>) -> Self {
        let settings = match &path {
            Some(path) => read(path),
            None => {
                eprintln!("settings: no config directory; settings will not survive a restart");
                Settings::default()
            }
        };
        SettingsStore { settings, path }
    }

    /// A store with no file at all, for tests and for the case where the config directory is
    /// unavailable.
    pub fn in_memory(settings: Settings) -> Self {
        SettingsStore {
            settings,
            path: None,
        }
    }

    pub fn settings(&self) -> Settings {
        self.settings
    }

    /// C / A / Z. The caller must **also** clear the window and the max hold
    /// ([`crate::metrics::Metrics::on_weighting_change`]) and switch the callback's chain
    /// ([`crate::capture::Capture::set_weighting`], which zeroes the filter state) — spec
    /// §6.11's second row, all three columns.
    pub fn set_weighting(&mut self, weighting: Weighting) -> Settings {
        self.settings.weighting = weighting;
        self.write_through();
        self.settings
    }

    /// F / S. The caller must also clear the max hold
    /// ([`crate::metrics::Metrics::set_time_weighting`] does it) — and **not** the window.
    pub fn set_time_weighting(&mut self, time_weighting: TimeWeighting) -> Settings {
        self.settings.time_weighting = time_weighting;
        self.write_through();
        self.settings
    }

    /// 10 / 30 / 60 / 120 s. Clears **nothing** — the ring re-slices
    /// ([`crate::metrics::Metrics::set_window_s`]).
    pub fn set_window_s(&mut self, window_s: u32) -> Result<Settings, SettingsError> {
        if !WINDOW_LENGTHS_S.contains(&window_s) {
            return Err(SettingsError::UnsupportedWindow(window_s));
        }
        self.settings.window_s = window_s;
        self.write_through();
        Ok(self.settings)
    }

    /// The offset, typed directly (spec §8.5's third affordance) or trimmed by ±0.1 dB.
    ///
    /// Directly editable is **not decoration**: free provisioning expires every 7 days, so the
    /// app is re-signed and reinstalled weekly, and delete-then-install loses the data
    /// container. With the offset visible and editable, recovery is a sticky note and a retype.
    ///
    /// Unclamped on purpose — see [`offset_from_reference`] — but it must be a number.
    ///
    /// **Clears nothing**: not the window, not the max hold, not the filter state, not the
    /// spectrogram ring (spec §6.11's bottom row). Since no offset ever enters
    /// [`crate::metrics`], that row holds by construction rather than by a line of code.
    pub fn set_offset_db(&mut self, offset_db: f64) -> Result<Settings, SettingsError> {
        if !offset_db.is_finite() {
            return Err(SettingsError::NotFinite);
        }
        self.settings.offset_db = Some(offset_db);
        self.write_through();
        Ok(self.settings)
    }

    /// Stores `reference_db − raw_slice_leq` (spec §8.5). Clears nothing.
    pub fn set_offset_from_reference(
        &mut self,
        reference_db: f64,
        raw_slice_leq: Option<f64>,
    ) -> Result<Settings, SettingsError> {
        let offset = offset_from_reference(reference_db, raw_slice_leq)?;
        self.set_offset_db(offset)
    }

    /// **A write failure is logged, not surfaced** (spec §10). The setting has already been
    /// applied in memory; refusing it would be worse.
    ///
    /// Plain `fs::write` rather than write-a-temp-and-rename. A torn write is indistinguishable
    /// from a corrupt file, which already has a designed outcome — defaults and a log line — and
    /// the recovery for the one value worth anything is the retype spec §8.5 keeps available.
    fn write_through(&self) {
        let Some(path) = &self.path else {
            return;
        };
        if let Err(reason) = write(path, &self.settings) {
            eprintln!("settings: could not write {}: {reason}", path.display());
        }
    }
}

fn write(path: &Path, settings: &Settings) -> Result<(), String> {
    let json = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?;
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    fs::write(path, json).map_err(|e| e.to_string())
}

fn read(path: &Path) -> Settings {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(reason) => {
            // Missing is the ordinary first-run case and is not distinguished in the log's tone;
            // either way the outcome is the same and there is nothing in the UI about it.
            eprintln!(
                "settings: could not read {} ({reason}); using defaults",
                path.display()
            );
            return Settings::default();
        }
    };

    match serde_json::from_str::<Settings>(&text) {
        Ok(settings) => sanitise(settings),
        Err(reason) => {
            eprintln!(
                "settings: {} is not readable settings ({reason}); using defaults",
                path.display()
            );
            Settings::default()
        }
    }
}

/// Repairs values that parse but cannot be honoured.
///
/// The file is on disk where anything can edit it, and both of these would otherwise be silent:
/// an unsupported window length would slice a span no picker can express, and a non-finite
/// offset would put `NaN` or `inf` on the wire dressed as a calibrated reading.
fn sanitise(mut settings: Settings) -> Settings {
    if !WINDOW_LENGTHS_S.contains(&settings.window_s) {
        eprintln!(
            "settings: window length {} s is not one of {:?}; using {DEFAULT_WINDOW_S} s",
            settings.window_s, WINDOW_LENGTHS_S
        );
        settings.window_s = DEFAULT_WINDOW_S;
    }
    if settings.offset_db.is_some_and(|offset| !offset.is_finite()) {
        eprintln!("settings: stored offset is not a finite number; uncalibrated");
        settings.offset_db = None;
    }
    settings
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::time::{Duration, Instant};

    use super::*;
    use crate::capture::BlockSummary;
    use crate::metrics::Metrics;

    // ── the four defaults ────────────────────────────────────────────────────────────────

    /// The trap named in this ticket: `06 d5`'s own text still says `F`, and it is wrong.
    #[test]
    fn the_defaults_are_uncalibrated_c_s_and_60_s() {
        let settings = Settings::default();
        assert_eq!(settings.offset_db, None);
        assert_eq!(settings.weighting, Weighting::C);
        assert_eq!(settings.time_weighting, TimeWeighting::Slow);
        assert_eq!(settings.window_s, 60);
    }

    // ── §8.6: the unit carries the honesty ───────────────────────────────────────────────

    #[test]
    fn uncalibrated_publishes_raw_values_under_dbfs() {
        let settings = Settings::default();
        assert_eq!(settings.unit(), Unit::DbFs);
        assert_eq!(settings.unit().to_string(), "dBFS");
        // Raw, not blanked: an uncalibrated meter is not broken, it is un-scaled, and blanking
        // it would mean you could not tell capture works at all.
        assert_eq!(settings.calibrated(Some(-45.2)), Some(-45.2));
    }

    #[test]
    fn a_stored_offset_switches_the_unit_to_db() {
        let mut store = SettingsStore::in_memory(Settings::default());
        let settings = store.set_offset_db(101.4).expect("finite offset");
        assert_eq!(settings.unit(), Unit::Db);
        assert_eq!(settings.unit().to_string(), "dB");
    }

    #[test]
    fn the_wire_form_of_the_settings_is_the_contract_in_9_1() {
        let settings = Settings {
            weighting: Weighting::A,
            time_weighting: TimeWeighting::Fast,
            window_s: 120,
            offset_db: Some(101.4),
        };
        let json = serde_json::to_value(settings).expect("serialize");
        assert_eq!(json["weighting"], "A");
        assert_eq!(json["time_weighting"], "F");
        assert_eq!(json["window_s"], 120);
        assert_eq!(json["offset_db"], 101.4);

        // `--` and uncalibrated are both `null`, never a sentinel (spec §9.2).
        let bare = serde_json::to_value(Settings::default()).expect("serialize");
        assert_eq!(bare["offset_db"], serde_json::Value::Null);
        assert_eq!(bare["weighting"], "C");
        assert_eq!(bare["time_weighting"], "S");
    }

    // ── §8.2: the offset is added post-log ───────────────────────────────────────────────

    #[test]
    fn calibrating_shifts_a_value_by_exactly_the_offset() {
        let settings = Settings {
            offset_db: Some(101.4),
            ..Settings::default()
        };
        assert_eq!(settings.calibrated(Some(-33.0)), Some(68.4));
    }

    #[test]
    fn nothing_to_say_stays_nothing_to_say_however_it_is_calibrated() {
        let uncalibrated = Settings::default();
        let calibrated = Settings {
            offset_db: Some(101.4),
            ..Settings::default()
        };
        assert_eq!(uncalibrated.calibrated(None), None);
        assert_eq!(calibrated.calibrated(None), None);
    }

    // ── §8.5: the arithmetic and its one bound ───────────────────────────────────────────

    #[test]
    fn the_offset_is_the_reference_minus_what_the_app_reads() {
        // The scale this design actually lives at: a room reading −33.1 dBFS against a proper
        // meter showing 68.3 dB(C).
        let offset = offset_from_reference(68.3, Some(-33.1)).expect("in range");
        assert!((offset - 101.4).abs() < 1e-12, "{offset}");
    }

    #[test]
    fn a_fat_fingered_683_for_68_3_is_rejected() {
        assert_eq!(
            offset_from_reference(683.0, Some(-33.1)),
            Err(SettingsError::ReferenceOutOfRange(683.0))
        );
    }

    #[test]
    fn the_reference_bound_is_inclusive_and_only_rejects_what_is_outside_it() {
        assert!(offset_from_reference(0.0, Some(-33.1)).is_ok());
        assert!(offset_from_reference(140.0, Some(-33.1)).is_ok());
        assert!(offset_from_reference(-0.1, Some(-33.1)).is_err());
        assert!(offset_from_reference(140.1, Some(-33.1)).is_err());
        assert_eq!(
            offset_from_reference(f64::NAN, Some(-33.1)),
            Err(SettingsError::NotFinite)
        );
    }

    /// The bound is on the **typed reference**; the derived offset is unclamped, and both of
    /// these land far outside 0–140 legitimately.
    #[test]
    fn the_derived_offset_itself_is_not_clamped() {
        assert_eq!(offset_from_reference(0.0, Some(-140.0)), Ok(140.0));
        let big = offset_from_reference(140.0, Some(-120.0)).expect("in range");
        assert!((big - 260.0).abs() < 1e-12, "{big}");
        let negative = offset_from_reference(0.0, Some(6.0)).expect("in range");
        assert!((negative - -6.0).abs() < 1e-12, "{negative}");
    }

    /// §8.3: arithmetic, not policy. At *zero* coverage the L_eq is undefined, so there is
    /// nothing to subtract from — the command fails rather than inventing a number. Below that,
    /// coverage is displayed and the reader judges; nothing here refuses a thin slice.
    #[test]
    fn zero_coverage_in_the_slice_errors_rather_than_returning_a_number() {
        assert_eq!(
            offset_from_reference(68.3, None),
            Err(SettingsError::NoSignalToMatch)
        );
    }

    #[test]
    fn a_non_finite_offset_is_refused_even_though_the_range_is_open() {
        let mut store = SettingsStore::in_memory(Settings::default());
        assert_eq!(
            store.set_offset_db(f64::INFINITY),
            Err(SettingsError::NotFinite)
        );
        assert_eq!(store.settings().offset_db, None);
    }

    #[test]
    fn only_the_four_permitted_window_lengths_are_accepted() {
        let mut store = SettingsStore::in_memory(Settings::default());
        for length in WINDOW_LENGTHS_S {
            assert_eq!(store.set_window_s(length).map(|s| s.window_s), Ok(length));
        }
        assert_eq!(
            store.set_window_s(45),
            Err(SettingsError::UnsupportedWindow(45))
        );
        // Refused means unchanged, not partially applied.
        assert_eq!(store.settings().window_s, 120);
    }

    // ── §10: persistence ─────────────────────────────────────────────────────────────────

    /// A private directory per test, without a `tempfile` dev-dependency for four values.
    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> TempDir {
            static NEXT: AtomicU32 = AtomicU32::new(0);
            let dir = std::env::temp_dir().join(format!(
                "decibel-meter-b04-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&dir).expect("create temp dir");
            TempDir(dir)
        }

        fn file(&self) -> PathBuf {
            self.0.join(FILE_NAME)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn all_four_settings_survive_a_round_trip_through_the_file() {
        let dir = TempDir::new();
        let mut store = SettingsStore::load(Some(dir.file()));

        store.set_weighting(Weighting::A);
        store.set_time_weighting(TimeWeighting::Fast);
        store.set_window_s(120).expect("permitted length");
        store
            .set_offset_from_reference(68.3, Some(-33.1))
            .expect("in range");
        let written = store.settings();

        let reloaded = SettingsStore::load(Some(dir.file())).settings();
        assert_eq!(reloaded.weighting, Weighting::A);
        assert_eq!(reloaded.time_weighting, TimeWeighting::Fast);
        assert_eq!(reloaded.window_s, 120);
        assert_eq!(reloaded, written);
        // The offset survives to the last bit, not to the display's 0.1 dB.
        assert_eq!(reloaded.offset_db, Some(68.3 - -33.1));
    }

    /// Write-through, not write-on-exit: the app dies on backgrounding, possibly without running
    /// shutdown code, so the file must be current after *each* change rather than after the
    /// last one.
    #[test]
    fn every_single_change_is_on_disk_before_the_next_one() {
        let dir = TempDir::new();
        let mut store = SettingsStore::load(Some(dir.file()));

        let on_disk = || {
            serde_json::from_str::<Settings>(&fs::read_to_string(dir.file()).expect("read"))
                .expect("parse")
        };

        store.set_weighting(Weighting::Z);
        assert_eq!(on_disk().weighting, Weighting::Z);
        store.set_time_weighting(TimeWeighting::Fast);
        assert_eq!(on_disk().time_weighting, TimeWeighting::Fast);
        store.set_window_s(10).expect("permitted length");
        assert_eq!(on_disk().window_s, 10);
        store.set_offset_db(101.4).expect("finite");
        assert_eq!(on_disk().offset_db, Some(101.4));
    }

    #[test]
    fn nothing_is_written_before_the_first_settings_act() {
        let dir = TempDir::new();
        let store = SettingsStore::load(Some(dir.file()));
        assert_eq!(store.settings(), Settings::default());
        assert!(!dir.file().exists(), "load must not write");
    }

    #[test]
    fn a_missing_settings_file_falls_back_to_the_defaults() {
        let dir = TempDir::new();
        let store = SettingsStore::load(Some(dir.file()));
        assert_eq!(store.settings(), Settings::default());
    }

    #[test]
    fn a_corrupt_settings_file_falls_back_to_the_defaults() {
        let dir = TempDir::new();
        // A torn write is the realistic corruption, not random bytes.
        fs::write(dir.file(), "{\"weighting\": \"A\", \"time_wei").expect("write");
        let store = SettingsStore::load(Some(dir.file()));
        assert_eq!(store.settings(), Settings::default());
    }

    #[test]
    fn a_file_naming_a_mode_that_does_not_exist_falls_back_to_the_defaults() {
        let dir = TempDir::new();
        fs::write(dir.file(), r#"{"weighting":"Q"}"#).expect("write");
        assert_eq!(
            SettingsStore::load(Some(dir.file())).settings(),
            Settings::default()
        );
    }

    /// A partially written *valid* file keeps what it has. The one value worth anything is the
    /// offset, recoverable only beside the reference meter, so an added field must never be able
    /// to take it down with it.
    #[test]
    fn a_file_missing_a_field_keeps_the_fields_it_has() {
        let dir = TempDir::new();
        fs::write(dir.file(), r#"{"offset_db": 101.4}"#).expect("write");
        let settings = SettingsStore::load(Some(dir.file())).settings();
        assert_eq!(settings.offset_db, Some(101.4));
        assert_eq!(settings.weighting, Weighting::C);
        assert_eq!(settings.time_weighting, TimeWeighting::Slow);
        assert_eq!(settings.window_s, 60);
    }

    #[test]
    fn a_hand_edited_window_length_no_picker_can_express_is_repaired() {
        let dir = TempDir::new();
        fs::write(dir.file(), r#"{"window_s": 45, "offset_db": 101.4}"#).expect("write");
        let settings = SettingsStore::load(Some(dir.file())).settings();
        assert_eq!(settings.window_s, 60);
        // Repaired, not discarded: the rest of the file still holds.
        assert_eq!(settings.offset_db, Some(101.4));
    }

    #[test]
    fn a_setting_applies_in_memory_even_with_nowhere_to_write_it() {
        // A directory where the file should be: `fs::write` fails, the setting still takes.
        let dir = TempDir::new();
        fs::create_dir_all(dir.file()).expect("create a directory in the file's place");
        let mut store = SettingsStore::load(Some(dir.file()));
        assert_eq!(store.settings(), Settings::default());
        assert_eq!(store.set_weighting(Weighting::A).weighting, Weighting::A);
        assert_eq!(store.settings().weighting, Weighting::A);
    }

    #[test]
    fn no_config_directory_at_all_still_leaves_a_working_meter() {
        let mut store = SettingsStore::load(None);
        assert_eq!(store.settings(), Settings::default());
        assert_eq!(
            store.set_offset_db(101.4).map(|s| s.offset_db),
            Ok(Some(101.4))
        );
    }

    // ── §14.2's remaining case, and §8.3 against the real ring ───────────────────────────

    const FS: f64 = 48_000.0;
    const FRAMES: u32 = 1024;
    const BLOCK: Duration = Duration::from_nanos(21_333_333);

    /// Runs `secs` of audio at `mean_square` into a real [`Metrics`], returning the clock.
    fn run(metrics: &mut Metrics, from: Instant, secs: f64, mean_square: f64) -> Instant {
        let end = from + Duration::from_secs_f64(secs);
        let mut now = from;
        while now + BLOCK <= end {
            now += BLOCK;
            metrics.deposit(
                now,
                BlockSummary {
                    sum_sq: mean_square * FRAMES as f64,
                    n: FRAMES,
                    t: now,
                },
            );
        }
        end
    }

    /// §14.2's last case, and the one row of §6.11 that had no home until this ticket: **an
    /// offset change resets nothing and shifts all three numbers by exactly the delta.**
    ///
    /// It holds by construction rather than by a code path — no offset enters [`Metrics`] — and
    /// this test is what makes that claim checkable rather than merely stated.
    #[test]
    fn an_offset_change_resets_nothing_and_shifts_all_three_numbers_by_the_delta() {
        let start = Instant::now();
        let mut metrics = Metrics::new(FS, start);
        // Loud then quiet, so the max hold is genuinely above both the L_eq and NOW and a reset
        // of it would be visible rather than coincidentally equal.
        let now = run(&mut metrics, start, 5.0, 0.5);
        let now = run(&mut metrics, now, 5.0, 0.005);
        let raw = metrics.levels(now);

        let mut store = SettingsStore::in_memory(Settings::default());
        let uncalibrated = store.settings();
        assert_eq!(uncalibrated.unit(), Unit::DbFs);

        let first = store.set_offset_db(101.4).expect("finite");
        let after_first = metrics.levels(now);

        // Nothing reset. Not the window, not the max hold, not the smoother — the *raw* numbers
        // are bit-identical across a calibration change, which is the strongest form of the
        // claim available.
        assert_eq!(after_first, raw, "a calibration change touched the metrics");

        // All three shift by exactly the offset, the historical maximum included, because
        // `max(xᵢ + c) = max(xᵢ) + c`.
        for (calibrated, raw) in [
            (first.calibrated(raw.now), raw.now),
            (first.calibrated(raw.leq), raw.leq),
            (first.calibrated(raw.max), raw.max),
        ] {
            let calibrated = calibrated.expect("a level");
            let raw = raw.expect("a level");
            assert!(
                (calibrated - (raw + 101.4)).abs() < 1e-12,
                "{calibrated} is not {raw} + 101.4"
            );
        }
        assert_eq!(first.unit(), Unit::Db);

        // And a *trim* — the ±0.1 dB gesture — moves them by the delta and nothing else, which
        // is what makes the gesture interactive rather than destructive.
        let trimmed = store.set_offset_db(101.5).expect("finite");
        assert_eq!(metrics.levels(now), raw);
        let before = first.calibrated(raw.leq).expect("a level");
        let after = trimmed.calibrated(raw.leq).expect("a level");
        assert!((after - before - 0.1).abs() < 1e-12, "{before} → {after}");
    }

    /// §8.3: the match slice is **the last 100 slots, whatever the window setting is**. Free,
    /// because §6.5 allocates the ring at the maximum length — no second accumulator, nothing to
    /// override, nothing to restore, and the main meter is never disturbed.
    #[test]
    fn the_10_s_match_slice_is_unaffected_by_the_window_length_setting() {
        let start = Instant::now();
        let mut metrics = Metrics::new(FS, start);
        // 110 s of loud, then the 10 s the calibration must see, at a level 20 dB below it.
        let ten_seconds_ago = run(&mut metrics, start, 110.0, 0.5);
        run(&mut metrics, ten_seconds_ago, 10.0, 0.005);

        let expected = 10.0 * 0.005f64.log10();
        for window_s in WINDOW_LENGTHS_S {
            metrics.set_window_s(window_s);
            let (slice_leq, slice_coverage) = metrics.leq_over(10);
            let slice_leq = slice_leq.expect("10 s of audio");
            assert!(
                (slice_leq - expected).abs() < 0.02,
                "at a {window_s} s window the 10 s slice read {slice_leq} dB, expected {expected}"
            );
            assert!(
                (slice_coverage - 10.0).abs() < 0.15,
                "at a {window_s} s window the slice covered {slice_coverage} s"
            );
            // And the offset derived from it is the same number at every window length.
            let offset = offset_from_reference(68.3, Some(slice_leq)).expect("in range");
            assert!((offset - (68.3 - expected)).abs() < 0.02, "{offset}");
        }

        // Meanwhile the display's own window is reading what it was asked to read: at 120 s it
        // sees the loud 110 s too, so the two figures genuinely differ.
        metrics.set_window_s(120);
        let (window_leq, _) = metrics.leq_over(metrics.window_s());
        let window_leq = window_leq.expect("audio");
        assert!(
            window_leq > expected + 10.0,
            "the 120 s window read {window_leq} dB, no different from the 10 s slice"
        );
    }

    /// The gesture spec §8.4 describes, end to end: read the proper meter, type the number, and
    /// every displayed value moves at once with nothing disturbed.
    #[test]
    fn typing_what_the_proper_meter_reads_makes_the_app_read_it_too() {
        let start = Instant::now();
        let mut metrics = Metrics::new(FS, start);
        // A steady sound — §8.4's requirement is load-bearing, not advisory: the reference
        // meter's quantity is unknown, so the two agree only when the sound is not changing.
        let now = run(&mut metrics, start, 12.0, 0.005);

        let (slice_leq, coverage) = metrics.leq_over(10);
        assert!((coverage - 10.0).abs() < 0.15, "{coverage}");

        let mut store = SettingsStore::in_memory(Settings::default());
        let settings = store
            .set_offset_from_reference(68.3, slice_leq)
            .expect("in range");

        // The 10 s slice now reads exactly what was typed.
        let shown = settings.calibrated(slice_leq).expect("a level");
        assert!((shown - 68.3).abs() < 1e-12, "{shown}");
        assert_eq!(settings.unit(), Unit::Db);

        // And the 60 s meter, over the same steady sound, agrees to within its own coverage.
        let leq = settings
            .calibrated(metrics.levels(now).leq)
            .expect("a level");
        assert!((leq - 68.3).abs() < 0.02, "{leq}");
    }
}
