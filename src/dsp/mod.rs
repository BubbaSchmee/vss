//! DSP engine surface. Stub: passes audio through unchanged. A later agent wires up
//! `FormantFilter` + `Sequencer` per SPEC.md "DSP" / "Sequencer". Keep this surface small.

pub mod formant;
pub mod sequencer;

use formant::FormantFilter;
use sequencer::Sequencer;

/// Converts a decibel value to a linear gain factor.
#[inline]
pub fn db_to_gain(db: f32) -> f32 {
    10f32.powf(db / 20.0)
}

/// Converts a linear gain factor to decibels.
#[inline]
pub fn gain_to_db(gain: f32) -> f32 {
    20.0 * gain.max(1e-8).log10()
}

/// Soft clip / saturation helper: `tanh(x * g) / tanh(g)`, unity gain at small `x`.
#[inline]
pub fn drive(x: f32, g: f32) -> f32 {
    if g <= 0.0 {
        x
    } else {
        (x * g).tanh() / g.tanh()
    }
}

/// The whole per-instance DSP engine: one formant filter per channel plus the sequencer.
/// `process()` currently passes audio through unchanged; a later agent replaces the body.
pub struct Engine {
    filters: Vec<FormantFilter>,
    sequencer: Sequencer,
    sample_rate: f32,
}

impl Engine {
    pub fn new() -> Self {
        Self {
            filters: Vec::new(),
            sequencer: Sequencer::new(),
            sample_rate: 44100.0,
        }
    }

    pub fn reset(&mut self) {
        for filter in &mut self.filters {
            filter.reset();
        }
        self.sequencer.reset();
    }

    pub fn set_sample_rate(&mut self, sample_rate: f32, num_channels: usize) {
        self.sample_rate = sample_rate;
        self.filters = (0..num_channels).map(|_| FormantFilter::new(sample_rate)).collect();
    }

    /// Process one buffer in place. Pass-through until the real DSP lands; `channel_samples` is
    /// one sample per channel, in channel order, matching `nih_plug::buffer::Buffer::iter_samples`.
    #[inline]
    pub fn process(&mut self, channel_samples: &mut [&mut f32]) {
        let _ = self.sample_rate;
        for (channel, filter) in channel_samples.iter_mut().zip(self.filters.iter_mut()) {
            **channel = filter.process(**channel);
        }
    }
}

impl Default for Engine {
    fn default() -> Self {
        Self::new()
    }
}
