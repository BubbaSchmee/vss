//! Per-channel 3-band formant filter (SPEC.md "DSP" 1-3).
//!
//! Three RBJ constant-0-dB-peak bandpass biquads in Direct Form I (DF-I keeps no
//! coefficient-weighted internal state, so coefficient changes don't inject transients the way
//! TDF-II can). Centers glide toward the target vowel with a one-pole in log2-frequency. The glide
//! runs per sample; coefficients are recomputed every `UPDATE_INTERVAL` samples from the current
//! (gliding) centers and the *smoothed* shift/resonance, or immediately when a new target arrives.

use crate::params::VowelFormants;

/// Coefficient recompute cadence in samples (SPEC: every 32 samples).
const UPDATE_INTERVAL: u32 = 32;

/// Fixed makeup gain applied to the summed bands (SPEC DSP step 3): a full-scale (0 dBFS) 55 Hz
/// sawtooth through vowel `A`, resonance 0.5 (Q = 2*10^0.5 ~= 6.32), shift 0, glide 0, 48 kHz peaks
/// at about 1.0. Measured (see `tests/formant.rs::makeup_gives_unity_peak_on_full_scale_saw`): raw
/// summed-band peak 0.37330 after settling, so makeup = 1/0.37330 ~= 2.68 (+8.6 dB). Not re-derived
/// per Q: on this saw, higher resonance narrows the bands and peaks lower (resonance 1.0 -> ~0.35).
pub const MAKEUP: f64 = 2.68;

/// Band outputs below this magnitude are flushed to zero (denormal guard; nih-plug
/// also enables FTZ around `process()`, this covers the tests/demo and any host that doesn't).
const DENORMAL_FLOOR: f64 = 1e-30;

#[derive(Clone, Copy, Default)]
struct Bandpass {
    // Normalized RBJ BPF (constant 0 dB peak): b1 == 0 and b2 == -b0.
    b0: f64,
    a1: f64,
    a2: f64,
    x1: f64,
    x2: f64,
    y1: f64,
    y2: f64,
}

impl Bandpass {
    fn set(&mut self, freq: f64, q: f64, sample_rate: f64) {
        let w0 = std::f64::consts::TAU * freq / sample_rate;
        let (sin, cos) = w0.sin_cos();
        let alpha = sin / (2.0 * q);
        let a0 = 1.0 + alpha;
        self.b0 = alpha / a0;
        self.a1 = -2.0 * cos / a0;
        self.a2 = (1.0 - alpha) / a0;
    }

    #[inline]
    fn tick(&mut self, x: f64) -> f64 {
        let mut y = self.b0 * (x - self.x2) - self.a1 * self.y1 - self.a2 * self.y2;
        if y.abs() < DENORMAL_FLOOR {
            y = 0.0;
        }
        self.x2 = self.x1;
        self.x1 = x;
        self.y2 = self.y1;
        self.y1 = y;
        y
    }

    fn clear(&mut self) {
        self.x1 = 0.0;
        self.x2 = 0.0;
        self.y1 = 0.0;
        self.y2 = 0.0;
    }
}

/// Three-band formant filter for one audio channel.
pub struct FormantFilter {
    sample_rate: f64,
    bands: [Bandpass; 3],
    /// Current (gliding) and target band centers as log2(Hz), before formant shift.
    current: [f64; 3],
    target: [f64; 3],
    /// Linear band gains of the target vowel.
    gains: [f64; 3],
    /// Per-sample one-pole coefficient for the glide; 1.0 = jump.
    glide_coeff: f64,
    countdown: u32,
    /// Recompute coefficients on the next sample regardless of the countdown.
    dirty: bool,
    /// False until the first target after a reset; that one jumps instead of gliding.
    primed: bool,
}

impl FormantFilter {
    pub fn new(sample_rate: f32) -> Self {
        let mut filter = Self {
            sample_rate: sample_rate as f64,
            bands: [Bandpass::default(); 3],
            current: [0.0; 3],
            target: [0.0; 3],
            gains: [0.0; 3],
            glide_coeff: 1.0,
            countdown: 0,
            dirty: true,
            primed: false,
        };
        filter.set_target(&crate::params::vowel_formants(crate::params::Vowel::A));
        filter.reset();
        filter
    }

    /// Clears all filter state. The next target jumps instead of gliding.
    pub fn reset(&mut self) {
        for band in &mut self.bands {
            band.clear();
        }
        self.current = self.target;
        self.primed = false;
        self.dirty = true;
    }

    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        self.sample_rate = sample_rate as f64;
        self.reset();
    }

    /// Glide time constant in ms (0 = jump). Cheap to call every block.
    pub fn set_glide_ms(&mut self, ms: f32) {
        self.glide_coeff = if ms <= 0.0 {
            1.0
        } else {
            1.0 - (-1000.0 / (ms as f64 * self.sample_rate)).exp()
        };
    }

    /// New vowel target. Takes effect on the next `process()` call.
    pub fn set_target(&mut self, formants: &VowelFormants) {
        self.target = [
            (formants.f1 as f64).log2(),
            (formants.f2 as f64).log2(),
            (formants.f3 as f64).log2(),
        ];
        self.gains = formants.gains.map(|db| 10f64.powf(db as f64 / 20.0));
        if !self.primed {
            self.current = self.target;
            self.primed = true;
        }
        self.dirty = true;
    }

    /// Process one sample. `shift_st` and `resonance` must be the smoothed parameter values.
    #[inline]
    pub fn process(&mut self, input: f32, shift_st: f32, resonance: f32) -> f32 {
        for k in 0..3 {
            self.current[k] += (self.target[k] - self.current[k]) * self.glide_coeff;
        }

        if self.dirty || self.countdown == 0 {
            // ponytail: smoothed shift/resonance are sampled at the 32-sample cadence (<= 0.67 ms
            // late at 48 kHz); per-sample recompute during smoothing only if zipper noise shows up.
            self.update_coefficients(shift_st as f64, resonance as f64);
            self.dirty = false;
            self.countdown = UPDATE_INTERVAL;
        }
        self.countdown -= 1;

        let x = input as f64;
        let mut sum = 0.0;
        for k in 0..3 {
            sum += self.gains[k] * self.bands[k].tick(x);
        }
        (sum * MAKEUP) as f32
    }

    fn update_coefficients(&mut self, shift_st: f64, resonance: f64) {
        let q = 2.0 * 10f64.powf(resonance.clamp(0.0, 1.0));
        let max_freq = 0.45 * self.sample_rate;
        let shift_oct = shift_st / 12.0;
        for k in 0..3 {
            let freq = (self.current[k] + shift_oct).exp2().clamp(40.0, max_freq);
            self.bands[k].set(freq, q, self.sample_rate);
        }
    }
}
