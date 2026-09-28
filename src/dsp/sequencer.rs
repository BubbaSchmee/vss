//! Beat position -> step index (SPEC.md "Sequencer").
//!
//! The beat of every sample is computed as `anchor + n * tempo / (60 * sr)` from an anchor, never
//! by accumulating a per-sample increment, so boundaries land on the exact sample (e.g. 150 BPM at
//! 48 kHz puts 1/8 boundaries on multiples of 9600 with no drift). While the host plays, the
//! anchor is the host's `pos_beats` at each block start (this also covers the stopped->playing
//! snap and loop jumps); while stopped, the internal counter free-runs at host tempo. With an
//! active host loop, beats extrapolated past `loop_end` within a block wrap to `loop_start`.

use crate::params::Rate;

/// Nudge in step units so a sample sitting exactly on a boundary never falls one ulp short of it.
/// One sample is >= 1.7e-6 step units even at 20 BPM / 192 kHz / 1/4, and f64 rounding of beat
/// positions below ~1e5 beats stays under 1e-10, so this can't move a boundary by a whole sample.
const BOUNDARY_EPS: f64 = 1e-9;

/// Steps per beat for each rate. All rates are 1/n of a beat, so working in step units is a
/// multiplication by a small integer (exact) rather than a division by 1/3 or 1/6 (inexact).
pub fn steps_per_beat(rate: Rate) -> f64 {
    match rate {
        Rate::Quarter => 1.0,
        Rate::Eighth => 2.0,
        Rate::EighthTriplet => 3.0,
        Rate::Sixteenth => 4.0,
        Rate::SixteenthTriplet => 6.0,
    }
}

pub struct Sequencer {
    anchor_beat: f64,
    /// Samples evaluated since `anchor_beat`.
    since_anchor: u64,
    tempo: f64,
    sample_rate: f64,
    /// Beat position of the most recently evaluated sample.
    beat: f64,
    /// Absolute (un-wrapped) step number of the last evaluated sample; `None` after reset.
    last_abs_step: Option<i64>,
    /// Pattern length in effect; a new `steps` value is latched only at a step boundary.
    steps_active: i64,
    current: usize,
    /// Host loop `(start, end)` in beats, only while playing with the block start before `end`.
    loop_range: Option<(f64, f64)>,
}

impl Sequencer {
    pub fn new() -> Self {
        Self {
            anchor_beat: 0.0,
            since_anchor: 0,
            tempo: 120.0,
            sample_rate: 44100.0,
            beat: 0.0,
            last_abs_step: None,
            steps_active: 1,
            current: 0,
            loop_range: None,
        }
    }

    /// Back to beat 0; the next evaluated sample is treated as a fresh step.
    pub fn reset(&mut self) {
        self.anchor_beat = 0.0;
        self.since_anchor = 0;
        self.beat = 0.0;
        self.last_abs_step = None;
        self.current = 0;
    }

    /// Call once at the start of every processing block with the host transport.
    /// `loop_range` is `Transport::loop_range_beats()` (`Some` only while the host loop is active).
    pub fn begin_block(
        &mut self,
        playing: bool,
        pos_beats: Option<f64>,
        tempo: Option<f64>,
        loop_range: Option<(f64, f64)>,
        sample_rate: f32,
    ) {
        let tempo = tempo.filter(|t| t.is_finite() && *t > 0.0).unwrap_or(120.0);
        let sample_rate = sample_rate as f64;
        match pos_beats.filter(|p| playing && p.is_finite()) {
            Some(host) => {
                self.anchor(host);
                // A playhead already past the loop end plays straight through (no wrap).
                self.loop_range = loop_range
                    .filter(|&(start, end)| start.is_finite() && end > start && host < end);
            }
            None => {
                if tempo != self.tempo
                    || sample_rate != self.sample_rate
                    || self.loop_range.is_some()
                {
                    // Keep the (wrapped) position, change the speed from here on.
                    let now = self.beat_at(self.since_anchor);
                    self.anchor(now);
                    self.loop_range = None;
                }
            }
        }
        self.tempo = tempo;
        self.sample_rate = sample_rate;
    }

    /// Evaluate the next sample. Returns the wrapped step index in `0..steps`.
    #[inline]
    pub fn next_step(&mut self, steps_per_beat: f64, swing: f32, steps: usize) -> usize {
        let beat = self.beat_at(self.since_anchor);
        self.since_anchor += 1;
        self.beat = beat;

        // Step units; each 2-step pair has its inner boundary at 1 + swing/3 instead of 1.
        let s = beat * steps_per_beat + BOUNDARY_EPS;
        let pair = (s * 0.5).floor();
        let offset = s - 2.0 * pair;
        let odd = offset >= 1.0 + swing.clamp(0.0, 1.0) as f64 / 3.0;
        let abs_step = 2 * pair as i64 + odd as i64;

        if self.last_abs_step != Some(abs_step) {
            self.last_abs_step = Some(abs_step);
            self.steps_active = steps.clamp(1, 16) as i64;
            self.current = abs_step.rem_euclid(self.steps_active) as usize;
        }
        self.current
    }

    /// Step index of the most recently evaluated sample.
    pub fn current(&self) -> usize {
        self.current
    }

    /// Beat position of the most recently evaluated sample.
    pub fn beat(&self) -> f64 {
        self.beat
    }

    fn anchor(&mut self, beat: f64) {
        self.anchor_beat = beat;
        self.since_anchor = 0;
    }

    #[inline]
    fn beat_at(&self, n: u64) -> f64 {
        let beat = self.anchor_beat + n as f64 * self.tempo / (60.0 * self.sample_rate);
        match self.loop_range {
            Some((start, end)) if beat >= end => start + (beat - end).rem_euclid(end - start),
            _ => beat,
        }
    }
}

impl Default for Sequencer {
    fn default() -> Self {
        Self::new()
    }
}
