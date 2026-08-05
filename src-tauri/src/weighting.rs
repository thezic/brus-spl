//! IEC 61672-1 frequency weighting: A, C and Z.
//!
//! Spec §5.3. The full derivation, the coefficient fixtures and the 34-row validation
//! table are in `.scratch/spl-meter-mvp/research/03-iec-weighting-filters.md`.
//!
//! Three things about this module are load-bearing:
//!
//! 1. **Always a cascade of biquads, never the expanded high-order polynomial.** Research
//!    `03` P1 measured a single 6th-order direct-form section overflowing to NaN in f32.
//!    `S_hp1` — the section whose poles sit nearest the unit circle — goes last.
//! 2. **Coefficients are derived at construction from the actual stream sample rate**, never
//!    compiled in. The bilinear recipe depends on the rate only through `c = 2·f_s`, so this
//!    is ~30 flops once per stream.
//! 3. **The digital cascade is normalised at 1 kHz**, not the analogue prototype. The
//!    standard's `A₁₀₀₀`/`C₁₀₀₀` are dropped entirely. 1 kHz is *the* calibration anchor, and
//!    normalising there makes clause 5.4.14 (`|L_C − L_A| ≤ 0.4 dB` at 1 kHz) exactly zero by
//!    construction rather than merely small.
//!
//! Realisation is transposed direct form II with f64 coefficients *and* f64 state. f32 state
//! is usually fine and sometimes isn't: research `03` P2 measured up to 3.9 dB of error on A
//! at 192 kHz with infrasonic content present, which is exactly what a phone microphone in a
//! venue delivers.

use std::f64::consts::TAU;

/// f₁ — double pole, shared by A and C (clauses 5.4.9–5.4.11).
const F1_HZ: f64 = 20.598_997_057_568_143;
/// f₂ — single pole, A only.
const F2_HZ: f64 = 107.652_648_643_046_27;
/// f₃ — single pole, A only.
const F3_HZ: f64 = 737.862_230_736_289_9;
/// f₄ — double pole, shared by A and C.
const F4_HZ: f64 = 12_194.217_147_998_01;

/// The reference frequency f_r at which the cascade is normalised (clause 5.4.8).
const REFERENCE_HZ: f64 = 1000.0;

/// The three weighting modes (spec §5). Z is the bypass path, not a filter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Weighting {
    A,
    C,
    Z,
}

/// One weighting realised as a biquad cascade at one sample rate, with persistent state.
///
/// Build one per stream; rebuild it rather than mutate it when the rate or the mode changes,
/// since §5.2 restarts the rolling window at the same moment anyway.
#[derive(Debug, Clone)]
pub struct WeightingChain {
    weighting: Weighting,
    sample_rate: f64,
    /// In cascade order. Empty for Z, two sections for C, three for A.
    sections: Vec<Biquad>,
}

impl WeightingChain {
    /// Derive the cascade for `weighting` at `sample_rate_hz`.
    ///
    /// # Panics
    ///
    /// If `sample_rate_hz` is not above 2 kHz — the 1 kHz normalisation point has to be
    /// below Nyquist for the filter to mean anything.
    pub fn new(sample_rate_hz: f64, weighting: Weighting) -> Self {
        assert!(
            sample_rate_hz > 2.0 * REFERENCE_HZ,
            "sample rate {sample_rate_hz} Hz puts the 1 kHz reference at or above Nyquist"
        );

        // The bilinear transform's only dependence on the sample rate.
        let c = 2.0 * sample_rate_hz;

        let mut sections = match weighting {
            Weighting::Z => Vec::new(),
            Weighting::C => vec![section_lp4(c), section_hp1(c)],
            Weighting::A => vec![section_lp4(c), section_hp23(c), section_hp1(c)],
        };

        // Normalise the *digital* cascade at 1 kHz (research `03` §2.3). An
        // analogue-normalised filter reads +0.004 dB at 1 kHz, because the bilinear
        // transform shifts the response there slightly.
        if let Some((first, rest)) = sections.split_first_mut() {
            let omega = TAU * REFERENCE_HZ / sample_rate_hz;
            let gain = rest
                .iter()
                .fold(first.magnitude_at(omega), |g, s| g * s.magnitude_at(omega));
            first.scale_numerator(1.0 / gain);
        }

        Self {
            weighting,
            sample_rate: sample_rate_hz,
            sections,
        }
    }

    /// The mode this chain realises.
    pub fn weighting(&self) -> Weighting {
        self.weighting
    }

    /// The sample rate the coefficients were derived for.
    pub fn sample_rate(&self) -> f64 {
        self.sample_rate
    }

    /// Filter one sample, advancing the state. For Z this returns `sample` unchanged.
    #[inline]
    pub fn process(&mut self, sample: f64) -> f64 {
        let mut y = sample;
        for section in &mut self.sections {
            y = section.process(y);
        }
        y
    }

    /// Filter a block in place.
    pub fn process_block(&mut self, block: &mut [f64]) {
        for sample in block {
            *sample = self.process(*sample);
        }
    }

    /// Zero the state, leaving the coefficients alone.
    pub fn reset(&mut self) {
        for section in &mut self.sections {
            section.reset();
        }
    }
}

/// `S_lp4` — low-pass, double pole at f₄. `ω₄² / (s + ω₄)²`.
fn section_lp4(c: f64) -> Biquad {
    let w4 = TAU * F4_HZ;
    Biquad::bilinear([0.0, 0.0, w4 * w4], [1.0, 2.0 * w4, w4 * w4], c)
}

/// `S_hp1` — high-pass, double pole at f₁. `s² / (s + ω₁)²`.
fn section_hp1(c: f64) -> Biquad {
    let w1 = TAU * F1_HZ;
    Biquad::bilinear([1.0, 0.0, 0.0], [1.0, 2.0 * w1, w1 * w1], c)
}

/// `S_hp23` — high-pass, single poles at f₂ and f₃. `s² / ((s + ω₂)(s + ω₃))`. A only.
fn section_hp23(c: f64) -> Biquad {
    let w2 = TAU * F2_HZ;
    let w3 = TAU * F3_HZ;
    Biquad::bilinear([1.0, 0.0, 0.0], [1.0, w2 + w3, w2 * w3], c)
}

/// A single biquad section in transposed direct form II. `a0` is implicitly 1.
#[derive(Debug, Clone)]
struct Biquad {
    b0: f64,
    b1: f64,
    b2: f64,
    a1: f64,
    a2: f64,
    z1: f64,
    z2: f64,
}

impl Biquad {
    /// Bilinear-transform the analogue section `N(s)/D(s)` with `c = 2·f_s`.
    ///
    /// `n` is `[n₂, n₁, n₀]` and `d` is `[d₂, d₁, d₀]`. Plain bilinear: no prewarping — it
    /// would trade high-frequency error for error in the 1–5 kHz band where the tolerance is
    /// four times tighter and where speech energy actually is — and no oversampling.
    fn bilinear(n: [f64; 3], d: [f64; 3], c: f64) -> Self {
        let cc = c * c;

        let b0 = n[0] * cc + n[1] * c + n[2];
        let b1 = 2.0 * n[2] - 2.0 * n[0] * cc;
        let b2 = n[0] * cc - n[1] * c + n[2];

        let a0 = d[0] * cc + d[1] * c + d[2];
        let a1 = 2.0 * d[2] - 2.0 * d[0] * cc;
        let a2 = d[0] * cc - d[1] * c + d[2];

        Self {
            b0: b0 / a0,
            b1: b1 / a0,
            b2: b2 / a0,
            a1: a1 / a0,
            a2: a2 / a0,
            z1: 0.0,
            z2: 0.0,
        }
    }

    /// Scale the numerator. Used once, to fold the 1 kHz normalisation into section 0.
    fn scale_numerator(&mut self, factor: f64) {
        self.b0 *= factor;
        self.b1 *= factor;
        self.b2 *= factor;
    }

    #[inline]
    fn process(&mut self, x: f64) -> f64 {
        let y = self.b0 * x + self.z1;
        self.z1 = self.b1 * x - self.a1 * y + self.z2;
        self.z2 = self.b2 * x - self.a2 * y;
        y
    }

    fn reset(&mut self) {
        self.z1 = 0.0;
        self.z2 = 0.0;
    }

    /// `|H(e^{jω})|`, evaluated in real arithmetic — every pole here is real, and the one
    /// place a complex number would otherwise appear is this magnitude, which factors into
    /// two hypots.
    fn magnitude_at(&self, omega: f64) -> f64 {
        let (s1, c1) = omega.sin_cos();
        let (s2, c2) = (2.0 * omega).sin_cos();

        let num_re = self.b0 + self.b1 * c1 + self.b2 * c2;
        let num_im = -(self.b1 * s1 + self.b2 * s2);
        let den_re = 1.0 + self.a1 * c1 + self.a2 * c2;
        let den_im = -(self.a1 * s1 + self.a2 * s2);

        num_re.hypot(num_im) / den_re.hypot(den_im)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::PI;

    // ---------------------------------------------------------------------------------
    // Research `03` §4.2, transcribed. Two parallel arrays purely to keep each row on one
    // line; they are the same table and are asserted to be the same length.
    // ---------------------------------------------------------------------------------

    /// `(n, f_exact, A exact, A tab, C exact, C tab, design band +, design band −)`
    type Row = (i32, f64, f64, f64, f64, f64, f64, f64);

    /// Research `03` §4.2, one tuple per row.
    ///
    /// `n` indexes the one-third-octave series; `f_exact` is the table's own printed value
    /// and is only cross-checked against `1000·10^(0.1(n−30))`, never used to filter.
    /// `A exact` / `C exact` are the analogue design-goal values from equations (6)/(7) —
    /// these are the tight assertion targets. `A tab` / `C tab` are the standard's published
    /// one-decimal values — the loose targets. The design band is the class-1 tolerance
    /// minus the Table A.1 expanded uncertainty, stated as two positive magnitudes.
    #[rustfmt::skip]
    const TABLE: [Row; 34] = [
        (10, 10.0,       -70.430, -70.4, -14.330, -14.3, 3.0, f64::INFINITY),
        (11, 12.5893,    -63.371, -63.4, -11.249, -11.2, 2.5, f64::INFINITY),
        (12, 15.8489,    -56.688, -56.7,  -8.531,  -8.5, 2.0,  4.0),
        (13, 19.9526,    -50.452, -50.5,  -6.240,  -6.2, 2.0,  2.0),
        (14, 25.1189,    -44.703, -44.7,  -4.405,  -4.4, 2.0,  1.5),
        (15, 31.6228,    -39.440, -39.4,  -3.010,  -3.0, 1.5,  1.5),
        (16, 39.8107,    -34.630, -34.6,  -1.999,  -2.0, 1.0,  1.0),
        (17, 50.1187,    -30.228, -30.2,  -1.294,  -1.3, 1.0,  1.0),
        (18, 63.0957,    -26.194, -26.2,  -0.818,  -0.8, 1.0,  1.0),
        (19, 79.4328,    -22.504, -22.5,  -0.504,  -0.5, 1.0,  1.0),
        (20, 100.0,      -19.143, -19.1,  -0.300,  -0.3, 1.0,  1.0),
        (21, 125.8925,   -16.098, -16.1,  -0.169,  -0.2, 1.0,  1.0),
        (22, 158.4893,   -13.350, -13.4,  -0.085,  -0.1, 1.0,  1.0),
        (23, 199.5262,   -10.870, -10.9,  -0.033,   0.0, 1.0,  1.0),
        (24, 251.1886,    -8.630,  -8.6,  -0.000,   0.0, 1.0,  1.0),
        (25, 316.2278,    -6.611,  -6.6,   0.019,   0.0, 1.0,  1.0),
        (26, 398.1072,    -4.808,  -4.8,   0.029,   0.0, 1.0,  1.0),
        (27, 501.1872,    -3.233,  -3.2,   0.033,   0.0, 1.0,  1.0),
        (28, 630.9573,    -1.900,  -1.9,   0.029,   0.0, 1.0,  1.0),
        (29, 794.3282,    -0.824,  -0.8,   0.019,   0.0, 1.0,  1.0),
        (30, 1000.0,       0.000,   0.0,   0.000,   0.0, 0.7,  0.7),
        (31, 1258.9254,    0.591,   0.6,  -0.033,   0.0, 1.0,  1.0),
        (32, 1584.8932,    0.981,   1.0,  -0.085,  -0.1, 1.0,  1.0),
        (33, 1995.2623,    1.200,   1.2,  -0.169,  -0.2, 1.0,  1.0),
        (34, 2511.8864,    1.271,   1.3,  -0.300,  -0.3, 1.0,  1.0),
        (35, 3162.2777,    1.199,   1.2,  -0.504,  -0.5, 1.0,  1.0),
        (36, 3981.0717,    0.970,   1.0,  -0.818,  -0.8, 1.0,  1.0),
        (37, 5011.8723,    0.549,   0.5,  -1.294,  -1.3, 1.5,  1.5),
        (38, 6309.5734,   -0.121,  -0.1,  -1.999,  -2.0, 1.5,  2.0),
        (39, 7943.2823,   -1.111,  -1.1,  -3.010,  -3.0, 1.5,  2.5),
        (40, 10000.0,     -2.492,  -2.5,  -4.405,  -4.4, 2.0,  3.0),
        (41, 12589.2541,  -4.318,  -4.3,  -6.240,  -6.2, 2.0,  5.0),
        (42, 15848.9319,  -6.603,  -6.6,  -8.531,  -8.5, 2.5, 16.0),
        (43, 19952.6231,  -9.317,  -9.3, -11.249, -11.2, 3.0, f64::INFINITY),
    ];

    /// The same rows in the same order: research `03` §4.2's last four columns, the
    /// *measured* plain-bilinear deviation from the analogue design goal, as
    /// `(A@44.1k, A@48k, C@44.1k, C@48k)`. Used only to cross-check this file's closed-form
    /// warping prediction against the numbers research `03` measured.
    #[rustfmt::skip]
    const BILINEAR_DEV: [(f64, f64, f64, f64); 34] = [
        ( -0.005,  -0.004,  0.000,   0.000),
        ( -0.005,  -0.004,  0.000,   0.000),
        ( -0.005,  -0.004,  0.000,   0.000),
        ( -0.005,  -0.004,  0.000,   0.000),
        ( -0.005,  -0.004,  0.000,   0.000),
        ( -0.005,  -0.004,  0.000,   0.000),
        ( -0.005,  -0.004,  0.000,   0.000),
        ( -0.005,  -0.004,  0.000,   0.000),
        ( -0.005,  -0.004,  0.000,   0.000),
        ( -0.005,  -0.004,  0.000,   0.000),
        ( -0.005,  -0.004,  0.000,   0.000),
        ( -0.005,  -0.004,  0.000,   0.000),
        ( -0.005,  -0.004,  0.000,   0.000),
        ( -0.004,  -0.004,  0.000,   0.000),
        ( -0.004,  -0.004,  0.000,   0.000),
        ( -0.004,  -0.003,  0.000,   0.000),
        ( -0.003,  -0.003,  0.000,   0.000),
        ( -0.002,  -0.002,  0.000,   0.000),
        ( -0.002,  -0.001,  0.000,   0.000),
        ( -0.001,  -0.001,  0.000,   0.000),
        ( -0.000,   0.000,  0.000,  -0.000),
        (  0.000,   0.000, -0.000,  -0.000),
        (  0.000,   0.000, -0.001,  -0.001),
        ( -0.001,  -0.001, -0.003,  -0.002),
        ( -0.005,  -0.004, -0.007,  -0.006),
        ( -0.016,  -0.014, -0.019,  -0.016),
        ( -0.044,  -0.037, -0.047,  -0.039),
        ( -0.111,  -0.093, -0.114,  -0.095),
        ( -0.272,  -0.225, -0.274,  -0.227),
        ( -0.645,  -0.530, -0.647,  -0.532),
        ( -1.501,  -1.216, -1.503,  -1.218),
        ( -3.459,  -2.738, -3.461,  -2.740),
        ( -8.214,  -6.214, -8.216,  -6.215),
        (-24.176, -15.668, -24.177, -15.668),
    ];

    /// The four rates the coefficients are derived and asserted at.
    const RATES: [f64; 4] = [16_000.0, 44_100.0, 48_000.0, 192_000.0];

    /// Above this rate the plain-bilinear filter holds the class-1 design band at every
    /// verification frequency below its own Nyquist (research `03` §2.5 bisected 39 012 Hz).
    /// Below it the filter leaves the band at high frequency by design, not by regression.
    const DESIGN_BAND_MIN_RATE: f64 = 39_012.0;

    /// Where a below-`DESIGN_BAND_MIN_RATE` rate stops being held to the band. Measured: at
    /// 16 kHz the 6.3 kHz row is already ~5.6 dB out, and 5 kHz sits on the limit.
    const LOW_RATE_BAND_LIMIT_HZ: f64 = 4000.0;

    /// A full-scale sine has this RMS, so a measured level is `SINE_RMS_DB + W(f)`.
    const SINE_RMS_DB: f64 = -3.010_299_956_639_812;

    /// Seconds of signal discarded before measuring. The f₁ double pole's time constant is
    /// ~7.7 ms, so this is ~195 of them (research `03` P5).
    const WARMUP_SECS: f64 = 1.5;
    /// Seconds measured, nudged to a whole number of half-cycles — see `measure_len`.
    const MEASURE_SECS: f64 = 2.0;

    // ---------------------------------------------------------------------------------
    // Helpers
    // ---------------------------------------------------------------------------------

    /// The standard's own frequency series, Table 2 footnote b: `f = 1000·10^(0.1(n−30))`.
    /// **Not** the nominal labels — 12.5 Hz instead of 12.5893 Hz produces a spurious
    /// 0.18 dB failure, and 16 Hz instead of 15.8489 Hz a 0.28 dB one.
    fn exact_third_octave(n: i32) -> f64 {
        1000.0 * 10f64.powf(0.1 * f64::from(n - 30))
    }

    /// The analogue prototype magnitude, equations (6) and (7), un-normalised.
    fn analogue_magnitude(weighting: Weighting, f: f64) -> f64 {
        let f1 = F1_HZ * F1_HZ;
        let f2 = F2_HZ * F2_HZ;
        let f3 = F3_HZ * F3_HZ;
        let f4 = F4_HZ * F4_HZ;
        let ff = f * f;
        match weighting {
            Weighting::Z => 1.0,
            Weighting::C => (f4 * ff) / ((ff + f1) * (ff + f4)),
            Weighting::A => {
                (f4 * ff * ff) / ((ff + f1) * (ff + f2).sqrt() * (ff + f3).sqrt() * (ff + f4))
            }
        }
    }

    /// The analogue design goal in dB relative to 1 kHz — equations (6)/(7) with the
    /// standard's `A₁₀₀₀`/`C₁₀₀₀` folded in exactly rather than rounded.
    fn analogue_db(weighting: Weighting, f: f64) -> f64 {
        20.0 * (analogue_magnitude(weighting, f) / analogue_magnitude(weighting, REFERENCE_HZ))
            .log10()
    }

    /// The bilinear transform is an exact frequency-warped mapping: the digital response at
    /// `f` equals the analogue response at `Ω(f) = (f_s/π)·tan(π f / f_s)`.
    fn warped_hz(f: f64, sample_rate: f64) -> f64 {
        (sample_rate / PI) * (PI * f / sample_rate).tan()
    }

    /// The response the *digital* filter should have at `f`, in dB relative to 1 kHz. Closed
    /// form, independent of this module's coefficient arithmetic.
    fn predicted_digital_db(weighting: Weighting, f: f64, sample_rate: f64) -> f64 {
        let at_f = analogue_magnitude(weighting, warped_hz(f, sample_rate));
        let at_ref = analogue_magnitude(weighting, warped_hz(REFERENCE_HZ, sample_rate));
        20.0 * (at_f / at_ref).log10()
    }

    /// Number of samples to measure over: about `MEASURE_SECS`, adjusted so the segment
    /// spans as close to a whole number of half-cycles as the sample grid allows.
    ///
    /// This is not fussiness. The mean of `sin²` over a partial cycle is biased, and at
    /// 10 Hz over a flat 2 s the bias is ~0.035 dB — three times the tight tolerance. A
    /// whole number of half-cycles cancels it exactly, so the search picks the candidate
    /// length that lands nearest an integer sample count.
    fn measure_len(f: f64, sample_rate: f64) -> usize {
        let per_half_cycle = sample_rate / (2.0 * f);
        let target = (MEASURE_SECS * sample_rate / per_half_cycle)
            .round()
            .max(1.0);
        let span = (target / 4.0).clamp(4.0, 500.0) as i32;

        let mut best = (f64::INFINITY, target);
        for k in -span..=span {
            let m = target + f64::from(k);
            if m < 1.0 {
                continue;
            }
            let len = m * per_half_cycle;
            let err = (len - len.round()).abs();
            if err < best.0 {
                best = (err, m);
            }
        }
        (best.1 * per_half_cycle).round() as usize
    }

    /// Feed a sine of `amplitude` at `f` through a freshly built chain and return the RMS of
    /// the measured tail, in dB relative to `amplitude` — so the expected value is
    /// `SINE_RMS_DB + W(f)`.
    fn measure_rms_db(weighting: Weighting, f: f64, sample_rate: f64, amplitude: f64) -> f64 {
        let mut chain = WeightingChain::new(sample_rate, weighting);
        let step = TAU * f / sample_rate;

        let warmup = (WARMUP_SECS * sample_rate).round() as usize;
        for n in 0..warmup {
            chain.process(amplitude * (step * n as f64).sin());
        }

        let len = measure_len(f, sample_rate);
        let mut sum_sq = 0.0;
        for n in warmup..warmup + len {
            let y = chain.process(amplitude * (step * n as f64).sin());
            sum_sq += y * y;
        }

        let rms = (sum_sq / len as f64).sqrt();
        20.0 * (rms / amplitude).log10()
    }

    /// Rows testable at this rate: the tone has to be below Nyquist to exist at all.
    fn testable(f: f64, sample_rate: f64) -> bool {
        f < 0.5 * sample_rate
    }

    /// `|H(e^{jω})|` of a whole cascade. The audio path never needs this, so it lives here
    /// rather than on `WeightingChain`; `new` composes the same product itself, over the
    /// sections it is in the middle of building.
    fn cascade_magnitude(chain: &WeightingChain, omega: f64) -> f64 {
        chain
            .sections
            .iter()
            .fold(1.0, |m, s| m * s.magnitude_at(omega))
    }

    // ---------------------------------------------------------------------------------
    // The table itself
    // ---------------------------------------------------------------------------------

    #[test]
    fn table_transcription_is_self_consistent() {
        assert_eq!(TABLE.len(), BILINEAR_DEV.len());
        for row in TABLE {
            let (n, f_printed, ..) = row;
            let f = exact_third_octave(n);
            assert!(
                (f - f_printed).abs() < 5e-4 * f_printed,
                "row n={n}: 1000·10^(0.1(n−30)) = {f}, table prints {f_printed}"
            );
        }
    }

    #[test]
    fn pole_frequencies_match_the_clause_5_4_9_derivation() {
        // Clauses 5.4.9–5.4.11, from f_L = 10^1.5, f_H = 10^3.9, f_A = 10^2.45, D = √½.
        let f_r: f64 = 1000.0;
        let f_l = 10f64.powf(1.5);
        let f_h = 10f64.powf(3.9);
        let f_a = 10f64.powf(2.45);
        let d = 0.5f64.sqrt();

        let b = (f_r * f_r + (f_l * f_l * f_h * f_h) / (f_r * f_r) - d * (f_l * f_l + f_h * f_h))
            / (1.0 - d);
        let c = f_l * f_l * f_h * f_h;

        // f₄² is well conditioned; f₁² comes from Vieta (`c / f₄²`) rather than the
        // cancelling form `(−b − √(b²−4c))/2`, which loses five digits (research `03` P6).
        let f4_sq = (-b + (b * b - 4.0 * c).sqrt()) / 2.0;
        let f4 = f4_sq.sqrt();
        let f1 = (c / f4_sq).sqrt();
        let f2 = (3.0 - 5f64.sqrt()) / 2.0 * f_a;
        let f3 = (3.0 + 5f64.sqrt()) / 2.0 * f_a;

        for (name, derived, constant) in [
            ("f1", f1, F1_HZ),
            ("f2", f2, F2_HZ),
            ("f3", f3, F3_HZ),
            ("f4", f4, F4_HZ),
        ] {
            let rel = ((derived - constant) / constant).abs();
            assert!(
                rel < 1e-12,
                "{name}: derived {derived}, constant {constant} (relative {rel:e})"
            );
        }
    }

    #[test]
    fn analogue_design_goals_match_the_research_table() {
        let mut worst = 0.0f64;
        for row in TABLE {
            let (n, _, a_exact, _, c_exact, ..) = row;
            let f = exact_third_octave(n);
            for (weighting, expected) in [(Weighting::A, a_exact), (Weighting::C, c_exact)] {
                let got = analogue_db(weighting, f);
                let dev = (got - expected).abs();
                worst = worst.max(dev);
                assert!(
                    dev <= 5e-4 + 1e-9,
                    "{weighting:?} at {f:.4} Hz: equations (6)/(7) give {got:.6} dB, \
                     research `03` §4.2 prints {expected:.3} dB"
                );
            }
        }
        println!("analogue design goal vs research §4.2: worst {worst:.6} dB");
    }

    #[test]
    fn warping_prediction_matches_the_measured_deviation_columns() {
        let mut worst = 0.0f64;
        for (row, dev) in TABLE.iter().zip(BILINEAR_DEV.iter()) {
            let (n, _, a_exact, _, c_exact, ..) = *row;
            let f = exact_third_octave(n);
            let cases = [
                (Weighting::A, a_exact, 44_100.0, dev.0),
                (Weighting::A, a_exact, 48_000.0, dev.1),
                (Weighting::C, c_exact, 44_100.0, dev.2),
                (Weighting::C, c_exact, 48_000.0, dev.3),
            ];
            for (weighting, goal, rate, published) in cases {
                let predicted = predicted_digital_db(weighting, f, rate) - goal;
                let diff = (predicted - published).abs();
                worst = worst.max(diff);
                assert!(
                    diff <= 1e-3 + 1e-9,
                    "{weighting:?} at {f:.4} Hz, {rate} Hz: closed-form warping predicts \
                     {predicted:.6} dB, research `03` §4.2 measured {published:.3} dB"
                );
            }
        }
        println!("warping prediction vs research §4.2 deviation columns: worst {worst:.6} dB");
    }

    // ---------------------------------------------------------------------------------
    // Tight — the regression test. Digital response vs the analogue design goal, allowing
    // for the bilinear warping the design goal cannot know about.
    // ---------------------------------------------------------------------------------

    fn check_tight(sample_rate: f64, amplitude: f64) -> (f64, usize) {
        let mut worst = 0.0f64;
        let mut count = 0usize;

        for row in TABLE {
            let (n, _, a_exact, _, c_exact, ..) = row;
            let f = exact_third_octave(n);
            if !testable(f, sample_rate) {
                continue;
            }
            for (weighting, goal) in [(Weighting::A, a_exact), (Weighting::C, c_exact)] {
                let warping = predicted_digital_db(weighting, f, sample_rate) - goal;
                let expected = SINE_RMS_DB + goal + warping;
                let measured = measure_rms_db(weighting, f, sample_rate, amplitude);
                let dev = (measured - expected).abs();
                worst = worst.max(dev);
                count += 1;
                assert!(
                    dev <= 0.01,
                    "TIGHT {weighting:?} at {f:.4} Hz, {sample_rate} Hz, amplitude \
                     {amplitude}: measured {measured:.6} dBFS, expected {expected:.6} dBFS \
                     (design goal {goal:.3} dB + {warping:.3} dB of bilinear warping); \
                     deviation {dev:.6} dB"
                );
            }
        }
        (worst, count)
    }

    #[test]
    fn tight_at_16_khz() {
        let (worst, n) = check_tight(16_000.0, 1.0);
        println!("tight @ 16 kHz: {n} assertions, worst {worst:.6} dB");
    }

    #[test]
    fn tight_at_44_1_khz() {
        let (worst, n) = check_tight(44_100.0, 1.0);
        println!("tight @ 44.1 kHz: {n} assertions, worst {worst:.6} dB");
    }

    #[test]
    fn tight_at_48_khz() {
        let (worst, n) = check_tight(48_000.0, 1.0);
        println!("tight @ 48 kHz: {n} assertions, worst {worst:.6} dB");
    }

    #[test]
    fn tight_at_192_khz() {
        let (worst, n) = check_tight(192_000.0, 1.0);
        println!("tight @ 192 kHz: {n} assertions, worst {worst:.6} dB");
    }

    /// Amplitude is irrelevant in f64; this is the pass that proves it.
    #[test]
    fn tight_at_48_khz_amplitude_1e_minus_3() {
        let (worst, n) = check_tight(48_000.0, 1e-3);
        println!("tight @ 48 kHz, amplitude 1e-3: {n} assertions, worst {worst:.6} dB");
    }

    // ---------------------------------------------------------------------------------
    // Loose — the standards-shaped test. Digital response vs the *published* one-decimal
    // table value, inside the class-1 design band.
    // ---------------------------------------------------------------------------------

    /// Returns `(worst |error| from the table value, smallest margin left to a band edge,
    /// assertions)`. The margin is the number that means something: several rows have a
    /// `−∞` lower limit, so a large error there is not a near miss.
    fn check_loose(sample_rate: f64) -> (f64, f64, usize) {
        let mut worst = 0.0f64;
        let mut margin = f64::INFINITY;
        let mut count = 0usize;

        for row in TABLE {
            let (n, _, _, a_tab, _, c_tab, band_plus, band_minus) = row;
            let f = exact_third_octave(n);
            if !testable(f, sample_rate) {
                continue;
            }
            // Research `03` §2.5: below ~39 kHz the plain-bilinear filter leaves the class-1
            // design band at high frequency. 16 kHz is in the rate list to exercise the
            // runtime coefficient derivation, not to claim conformance there.
            if sample_rate < DESIGN_BAND_MIN_RATE && f > LOW_RATE_BAND_LIMIT_HZ {
                continue;
            }
            for (weighting, published) in [(Weighting::A, a_tab), (Weighting::C, c_tab)] {
                let measured = measure_rms_db(weighting, f, sample_rate, 1.0) - SINE_RMS_DB;
                let error = measured - published;
                worst = worst.max(error.abs());
                margin = margin.min(band_plus - error).min(band_minus + error);
                count += 1;
                assert!(
                    error <= band_plus && -error <= band_minus,
                    "LOOSE {weighting:?} at {f:.4} Hz, {sample_rate} Hz: measured \
                     {measured:.4} dB, table says {published:.1} dB, design band \
                     +{band_plus}/-{band_minus} dB (error {error:.4} dB)"
                );
            }
        }
        (worst, margin, count)
    }

    fn report_loose(label: &str, sample_rate: f64) {
        let (worst, margin, n) = check_loose(sample_rate);
        println!(
            "loose @ {label}: {n} assertions, worst {worst:.4} dB from the table value, \
             {margin:.4} dB of margin left to the nearest band edge"
        );
    }

    #[test]
    fn loose_at_16_khz() {
        report_loose("16 kHz", 16_000.0);
    }

    #[test]
    fn loose_at_44_1_khz() {
        report_loose("44.1 kHz", 44_100.0);
    }

    #[test]
    fn loose_at_48_khz() {
        report_loose("48 kHz", 48_000.0);
    }

    #[test]
    fn loose_at_192_khz() {
        report_loose("192 kHz", 192_000.0);
    }

    // ---------------------------------------------------------------------------------
    // Invariants
    // ---------------------------------------------------------------------------------

    /// Clause 5.4.6 puts C down by exactly D² = ½ at f_L and at f_H. That is a property of
    /// the *analogue* prototype and therefore of f₁ and f₄ themselves, so it is asserted on
    /// this module's own constants at 1e-9 — this is the check that catches a transposed one.
    #[test]
    fn c_is_down_3_010_db_at_f_l_and_f_h() {
        let half_power_db = -10.0 * 2f64.log10();
        for (name, f) in [("f_L", 10f64.powf(1.5)), ("f_H", 10f64.powf(3.9))] {
            let got = analogue_db(Weighting::C, f);
            assert!(
                (got - half_power_db).abs() < 1e-9,
                "C at {name} = {f:.4} Hz is {got:.12} dB, expected {half_power_db:.12} dB"
            );
        }
    }

    /// The same two invariants end to end, through the actual filter. Note that the digital
    /// value at f_H is **not** −3.010 dB at any usable rate — the bilinear warping is
    /// −0.53 dB there at 48 kHz — so that half has to be stated against the warped
    /// prediction. At f_L the warping is nil, so the digital filter reads −3.010 dB flat.
    #[test]
    fn the_two_design_invariants_survive_discretisation() {
        for rate in RATES {
            for (name, f) in [("f_L", 10f64.powf(1.5)), ("f_H", 10f64.powf(3.9))] {
                if !testable(f, rate) {
                    continue;
                }
                let expected = SINE_RMS_DB + predicted_digital_db(Weighting::C, f, rate);
                let measured = measure_rms_db(Weighting::C, f, rate, 1.0);
                assert!(
                    (measured - expected).abs() <= 0.01,
                    "C at {name} = {f:.4} Hz, {rate} Hz: measured {measured:.6} dBFS, \
                     expected {expected:.6} dBFS"
                );
            }
        }
    }

    /// What the digital renormalisation buys: exactly unity at the calibration anchor.
    #[test]
    fn unity_gain_at_1_khz() {
        for rate in RATES {
            for weighting in [Weighting::A, Weighting::C, Weighting::Z] {
                let chain = WeightingChain::new(rate, weighting);
                let omega = TAU * REFERENCE_HZ / rate;
                let db = 20.0 * cascade_magnitude(&chain, omega).log10();
                assert!(
                    db.abs() < 1e-9,
                    "{weighting:?} at {rate} Hz: |H(1 kHz)| = {db:e} dB, expected 0"
                );
            }
        }
    }

    /// Clause 5.4.14 allows ±0.4 dB between a C- and an A-weighted indication at 1 kHz.
    /// Normalising both digitally makes it exactly zero, so assert it as such.
    #[test]
    fn clause_5_4_14_c_and_a_agree_at_1_khz() {
        for rate in RATES {
            let omega = TAU * REFERENCE_HZ / rate;
            let c_chain = WeightingChain::new(rate, Weighting::C);
            let a_chain = WeightingChain::new(rate, Weighting::A);
            let l_c = 20.0 * cascade_magnitude(&c_chain, omega).log10();
            let l_a = 20.0 * cascade_magnitude(&a_chain, omega).log10();
            assert!(
                (l_c - l_a).abs() < 1e-9,
                "{rate} Hz: |L_C - L_A| at 1 kHz = {:e} dB",
                (l_c - l_a).abs()
            );
        }
    }

    #[test]
    fn z_returns_the_input_unchanged() {
        for rate in RATES {
            let mut chain = WeightingChain::new(rate, Weighting::Z);
            assert_eq!(chain.weighting(), Weighting::Z);
            assert_eq!(chain.sample_rate(), rate);

            let mut block = [0.0, 1.0, -1.0, 0.5, -0.25, 1e-9, -1e-9, 0.123_456_789, 0.0];
            let original = block;
            for (i, x) in original.iter().enumerate() {
                assert_eq!(chain.process(*x), *x, "sample {i} at {rate} Hz");
            }
            chain.process_block(&mut block);
            assert_eq!(block, original, "process_block at {rate} Hz");
        }
    }

    #[test]
    fn the_cascade_is_the_shape_the_spec_asks_for() {
        for rate in RATES {
            assert_eq!(WeightingChain::new(rate, Weighting::Z).sections.len(), 0);
            assert_eq!(WeightingChain::new(rate, Weighting::C).sections.len(), 2);
            assert_eq!(WeightingChain::new(rate, Weighting::A).sections.len(), 3);
        }
    }

    #[test]
    fn reset_zeroes_the_state_but_not_the_coefficients() {
        let mut chain = WeightingChain::new(48_000.0, Weighting::A);
        for n in 0..4800 {
            chain.process((TAU * 1000.0 * f64::from(n) / 48_000.0).sin());
        }
        chain.reset();
        let fresh = WeightingChain::new(48_000.0, Weighting::A);
        for (after, before) in chain.sections.iter().zip(fresh.sections.iter()) {
            assert_eq!(after.z1, 0.0);
            assert_eq!(after.z2, 0.0);
            assert_eq!(after.b0, before.b0);
            assert_eq!(after.a1, before.a1);
        }
    }

    /// Research `03` §2.4's fixtures, compared as *responses* — the gain distribution across
    /// sections is arbitrary, so the coefficients themselves are not comparable, only what
    /// the cascade does with them.
    #[test]
    fn responses_match_the_research_2_4_coefficient_fixtures() {
        #[rustfmt::skip]
        let fixtures: [(f64, Weighting, &[[f64; 5]]); 4] = [
            (48_000.0, Weighting::C, &[
                [0.198_424_662_339_766_1, 0.396_849_324_679_532_2, 0.198_424_662_339_766_1,
                 -0.224_558_446_076_045_05, 0.012_606_623_926_022_036],
                [0.997_309_040_744_271_2, -1.994_618_081_488_542_4, 0.997_309_040_744_271_2,
                 -1.994_614_455_977_990_6, 0.994_621_706_999_094_1],
            ]),
            (48_000.0, Weighting::A, &[
                [0.247_889_203_110_647_16, 0.495_778_406_221_294_3, 0.247_889_203_110_647_16,
                 -0.224_558_446_076_045_05, 0.012_606_623_926_022_036],
                [0.947_257_565_994_427, -1.894_515_131_988_854, 0.947_257_565_994_427,
                 -1.893_870_494_810_513_6, 0.895_159_769_167_194_6],
                [0.997_309_040_744_271_2, -1.994_618_081_488_542_4, 0.997_309_040_744_271_2,
                 -1.994_614_455_977_990_6, 0.994_621_706_999_094_1],
            ]),
            (44_100.0, Weighting::C, &[
                [0.217_650_589_159_589_15, 0.435_301_178_319_178_3, 0.217_650_589_159_589_15,
                 -0.140_536_070_343_899_75, 0.004_937_596_766_926_394],
                [0.997_071_587_669_548_7, -1.994_143_175_339_097_5, 0.997_071_587_669_548_7,
                 -1.994_138_881_249_972_3, 0.994_147_469_428_222_7],
            ]),
            (44_100.0, Weighting::A, &[
                [0.271_881_787_501_086_27, 0.543_763_575_002_172_5, 0.271_881_787_501_086_27,
                 -0.140_536_070_343_899_75, 0.004_937_596_766_926_394],
                [0.942_830_672_354_703, -1.885_661_344_709_406, 0.942_830_672_354_703,
                 -1.884_901_217_524_506_1, 0.886_421_471_894_305_6],
                [0.997_071_587_669_548_7, -1.994_143_175_339_097_5, 0.997_071_587_669_548_7,
                 -1.994_138_881_249_972_3, 0.994_147_469_428_222_7],
            ]),
        ];

        let mut worst = 0.0f64;
        for (rate, weighting, coefficients) in fixtures {
            let published: Vec<Biquad> = coefficients
                .iter()
                .map(|c| Biquad {
                    b0: c[0],
                    b1: c[1],
                    b2: c[2],
                    a1: c[3],
                    a2: c[4],
                    z1: 0.0,
                    z2: 0.0,
                })
                .collect();
            let ours = WeightingChain::new(rate, weighting);

            for row in TABLE {
                let f = exact_third_octave(row.0);
                if !testable(f, rate) {
                    continue;
                }
                let omega = TAU * f / rate;
                let theirs = published.iter().fold(1.0, |m, s| m * s.magnitude_at(omega));
                let mine = cascade_magnitude(&ours, omega);
                let diff = (20.0 * (mine / theirs).log10()).abs();
                worst = worst.max(diff);
                assert!(
                    diff < 1e-6,
                    "{weighting:?} at {f:.4} Hz, {rate} Hz: our response differs from \
                     research `03` §2.4's fixture by {diff:e} dB"
                );
            }
        }
        println!("response vs research §2.4 fixtures: worst {worst:e} dB");
    }
}
