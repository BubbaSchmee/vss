//! DSP engine: sequencer -> vowel target -> per-channel formant filter -> drive -> mix -> output.
//! See SPEC.md "DSP" / "Sequencer" / "Logging". Nothing in the per-sample path allocates.

pub mod formant;
pub mod sequencer;

use crate::params::{vowel_formants, Rate, Vowel};
use formant::FormantFilter;
use sequencer::{steps_per_beat, Sequencer};

/// `nih_log!` formats through the logger, which may allocate; allowed explicitly so the debug
/// build's `assert_process_allocs` only catches allocations we didn't mean.
macro_rules! rt_log {
    ($($args:tt)*) => {
        nih_plug::util::permit_alloc(|| nih_plug::nih_log!($($args)*))
    };
}

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

/// `tanh(x * g) / tanh(g)` with `g = db_to_gain(drive_db)`; exact pass-through at 0 dB.
///
/// The SPEC formula alone jumps from unity to `1/tanh(1)` (+2.4 dB small-signal) the moment drive
/// leaves 0, which clicks when the smoothed drive crosses 0. So over the first 1 dB the shaped
/// signal is crossfaded in from the dry one; from 1 dB up it is the SPEC formula exactly.
#[derive(Clone, Copy)]
pub struct Drive {
    db: f32,
    g: f32,
    inv_tanh_g: f32,
    blend: f32,
}

impl Drive {
    pub fn new() -> Self {
        let mut drive = Self {
            db: f32::NAN,
            g: 1.0,
            inv_tanh_g: 1.0,
            blend: 0.0,
        };
        drive.set_db(0.0);
        drive
    }

    /// Cheap when unchanged, so it can be called every sample with the smoothed value.
    #[inline]
    pub fn set_db(&mut self, db: f32) {
        if db != self.db {
            self.db = db;
            self.g = db_to_gain(db.max(0.0));
            self.inv_tanh_g = 1.0 / self.g.tanh();
            self.blend = db.clamp(0.0, 1.0);
        }
    }

    #[inline]
    pub fn process(&self, x: f32) -> f32 {
        if self.blend <= 0.0 {
            return x;
        }
        let shaped = (x * self.g).tanh() * self.inv_tanh_g;
        x + (shaped - x) * self.blend
    }
}

impl Default for Drive {
    fn default() -> Self {
        Self::new()
    }
}

/// Parameters read once per block (not smoothed).
#[derive(Clone, Copy)]
pub struct BlockParams {
    pub steps: usize,
    pub rate: Rate,
    pub swing: f32,
    pub glide_ms: f32,
    pub pattern: [Vowel; 16],
}

/// Smoothed parameters, one value per sample.
#[derive(Clone, Copy)]
pub struct FrameParams {
    pub formant_shift: f32,
    pub resonance: f32,
    pub drive_db: f32,
    pub mix: f32,
    pub output_gain_db: f32,
}

/// Resolve `Hold` to the nearest earlier non-Hold step; a Hold chain reaching step 1 means `A`.
fn resolve_vowel(pattern: &[Vowel; 16], index: usize) -> Vowel {
    pattern[..=index]
        .iter()
        .rev()
        .copied()
        .find(|v| *v != Vowel::Hold)
        .unwrap_or(Vowel::A)
}

/// The whole per-instance DSP engine: one formant filter per channel plus the sequencer.
pub struct Engine {
    filters: Vec<FormantFilter>,
    sequencer: Sequencer,
    sample_rate: f32,
    block: BlockParams,
    steps_per_beat: f64,
    drive: Drive,
    output_db: f32,
    output_gain: f32,
    /// Step index whose vowel is currently targeted; `None` after reset.
    last_step: Option<usize>,
    target: Option<Vowel>,
    // Last logged transport state, for change-only logging.
    logged_playing: Option<bool>,
    logged_tempo: Option<f64>,
}

impl Engine {
    pub fn new() -> Self {
        Self {
            filters: Vec::new(),
            sequencer: Sequencer::new(),
            sample_rate: 44100.0,
            block: BlockParams {
                steps: 8,
                rate: Rate::Eighth,
                swing: 0.0,
                glide_ms: 20.0,
                pattern: crate::params::DEFAULT_PATTERN,
            },
            steps_per_beat: 2.0,
            drive: Drive::new(),
            output_db: 0.0,
            output_gain: 1.0,
            last_step: None,
            target: None,
            logged_playing: None,
            logged_tempo: None,
        }
    }

    pub fn reset(&mut self) {
        for filter in &mut self.filters {
            filter.reset();
        }
        self.sequencer.reset();
        self.last_step = None;
        self.target = None;
    }

    /// Allocates; call from `initialize()`, never from `process()`.
    pub fn set_sample_rate(&mut self, sample_rate: f32, num_channels: usize) {
        self.sample_rate = sample_rate;
        self.filters = (0..num_channels)
            .map(|_| FormantFilter::new(sample_rate))
            .collect();
        self.reset();
    }

    /// Call once at the start of every processing block.
    pub fn begin_block(
        &mut self,
        playing: bool,
        pos_beats: Option<f64>,
        tempo: Option<f64>,
        block: &BlockParams,
    ) {
        if self.logged_playing != Some(playing) {
            self.logged_playing = Some(playing);
            rt_log!(
                "transport: {} tempo={:?} pos_beats={:?}",
                if playing { "playing" } else { "stopped" },
                tempo,
                pos_beats
            );
        }
        if let Some(t) = tempo {
            // 0.1 BPM hysteresis keeps a tempo ramp to a few lines per second.
            if self
                .logged_tempo
                .map_or(true, |last| (t - last).abs() >= 0.1)
            {
                self.logged_tempo = Some(t);
                rt_log!(
                    "tempo: {:.3} BPM playing={} pos_beats={:?}",
                    t,
                    playing,
                    pos_beats
                );
            }
        }

        self.sequencer
            .begin_block(playing, pos_beats, tempo, self.sample_rate);
        self.block = *block;
        self.steps_per_beat = steps_per_beat(block.rate);
        for filter in &mut self.filters {
            filter.set_glide_ms(block.glide_ms);
        }
        // Pick up edits to the step that is currently playing.
        if let Some(step) = self.last_step {
            self.retarget(step);
        }
    }

    /// Process one frame (one sample per channel, in channel order) in place.
    #[inline]
    pub fn process_frame<'a>(
        &mut self,
        frame: impl IntoIterator<Item = &'a mut f32>,
        p: &FrameParams,
    ) {
        let step =
            self.sequencer
                .next_step(self.steps_per_beat, self.block.swing, self.block.steps);
        if self.last_step != Some(step) {
            self.last_step = Some(step);
            self.retarget(step);
            rt_log!(
                "step: index={} vowel={:?} ({:?}) pos_beats={:.4}",
                step,
                self.block.pattern[step],
                self.target.unwrap_or(Vowel::A),
                self.sequencer.beat()
            );
        }

        self.drive.set_db(p.drive_db);
        if p.output_gain_db != self.output_db {
            self.output_db = p.output_gain_db;
            self.output_gain = db_to_gain(p.output_gain_db);
        }
        let mix = p.mix.clamp(0.0, 1.0);

        for (sample, filter) in frame.into_iter().zip(self.filters.iter_mut()) {
            let dry = *sample;
            let wet = self
                .drive
                .process(filter.process(dry, p.formant_shift, p.resonance));
            *sample = (dry * (1.0 - mix) + wet * mix) * self.output_gain;
        }
    }

    /// Step index of the most recent sample (for the GUI).
    pub fn current_step(&self) -> usize {
        self.sequencer.current()
    }

    fn retarget(&mut self, step: usize) {
        let vowel = resolve_vowel(&self.block.pattern, step);
        if self.target != Some(vowel) {
            self.target = Some(vowel);
            let formants = vowel_formants(vowel);
            for filter in &mut self.filters {
                filter.set_target(&formants);
            }
        }
    }
}

impl Default for Engine {
    fn default() -> Self {
        Self::new()
    }
}
