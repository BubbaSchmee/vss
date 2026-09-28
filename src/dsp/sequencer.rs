//! Beat-position → step-index sequencer. Stub: a later agent replaces the internals with the
//! swing/free-run logic described in SPEC.md "Sequencer". Keep this surface small and stable.

/// Tracks the current step index from a beat position.
pub struct Sequencer {
    current_step: usize,
}

impl Sequencer {
    pub fn new() -> Self {
        Self { current_step: 0 }
    }

    pub fn reset(&mut self) {
        self.current_step = 0;
    }

    /// Advance by one sample given the current transport state. Returns the step index.
    /// Always step 0 until the real beat-position math lands.
    #[inline]
    pub fn advance(
        &mut self,
        _playing: bool,
        _pos_beats: Option<f64>,
        _tempo: Option<f64>,
        _sample_rate: f32,
        _step_len_beats: f64,
        _swing: f32,
        _steps: usize,
    ) -> usize {
        self.current_step
    }
}

impl Default for Sequencer {
    fn default() -> Self {
        Self::new()
    }
}
