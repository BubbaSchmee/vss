//! Per-channel 3-band formant filter. Stub: a later agent replaces the internals with the RBJ
//! bandpass biquad bank described in SPEC.md "DSP". Keep this surface small and stable.

/// Three-band formant filter for one audio channel.
pub struct FormantFilter {
    sample_rate: f32,
}

impl FormantFilter {
    pub fn new(sample_rate: f32) -> Self {
        Self { sample_rate }
    }

    /// Clears all filter state (e.g. on transport reposition or plugin activation).
    pub fn reset(&mut self) {
        // ponytail: nothing to reset yet, no filter state exists until the biquads land.
    }

    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        self.sample_rate = sample_rate;
    }

    /// Process one sample. Pass-through until the biquad bank is implemented.
    #[inline]
    pub fn process(&mut self, input: f32) -> f32 {
        let _ = self.sample_rate;
        input
    }
}
