//! VSS — Vowel Step Sequencer. A 3-band formant filter whose vowel is chosen by a tempo-synced
//! 16-step sequencer. See SPEC.md for the full contract.

pub mod bus;
pub mod dsp;
pub mod editor;
pub mod params;

use nih_plug::prelude::*;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use bus::{modulate, target_offsets, Link, Modulator};
use params::{CvTarget, VssParams};

pub struct Vss {
    params: Arc<VssParams>,
    current_step: Arc<AtomicUsize>,
    engine: dsp::Engine,
    modulator: Modulator,
    sample_rate: f32,
}

impl Default for Vss {
    fn default() -> Self {
        let params = Arc::new(VssParams::default());
        Self {
            modulator: Modulator::new(Link::new(params.clone())),
            params,
            current_step: Arc::new(AtomicUsize::new(0)),
            engine: dsp::Engine::new(),
            sample_rate: 44100.0,
        }
    }
}

impl Plugin for Vss {
    const NAME: &'static str = "VSS";
    const VENDOR: &'static str = "VSS Open Source";
    const URL: &'static str = "https://github.com/BubbaSchmee/vss";
    const EMAIL: &'static str = "https://github.com/BubbaSchmee/vss/issues";

    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[AudioIOLayout {
        main_input_channels: NonZeroU32::new(2),
        main_output_channels: NonZeroU32::new(2),
        ..AudioIOLayout::const_default()
    }];

    const MIDI_INPUT: MidiConfig = MidiConfig::None;
    const SAMPLE_ACCURATE_AUTOMATION: bool = true;

    type SysExMessage = ();
    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn editor(&mut self, _async_executor: AsyncExecutor<Self>) -> Option<Box<dyn Editor>> {
        editor::create(self.params.clone(), self.current_step.clone(), self.modulator.link().clone())
    }

    fn initialize(
        &mut self,
        audio_io_layout: &AudioIOLayout,
        buffer_config: &BufferConfig,
        _context: &mut impl InitContext<Self>,
    ) -> bool {
        nih_log!(
            "initialize: sample_rate={} max_buffer_size={}",
            buffer_config.sample_rate,
            buffer_config.max_buffer_size
        );

        let num_channels = audio_io_layout
            .main_output_channels
            .map(|c| c.get() as usize)
            .unwrap_or(2);
        self.engine.set_sample_rate(buffer_config.sample_rate, num_channels);
        self.engine.log_steps = std::env::var_os("NIH_LOG").is_some();
        self.sample_rate = buffer_config.sample_rate;
        // The one place the session bus mapping is opened (never from `process`).
        self.modulator.initialize(rr_bus::Bus::open());

        true
    }

    fn reset(&mut self) {
        self.engine.reset();
        // A host that paused `process` for over 2 s had its record reclaimed; register again.
        // Allocates only in that case, which `reset` may do.
        nih_plug::util::permit_alloc(|| self.modulator.ensure_registered());
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        let params = &self.params;
        let transport = context.transport();
        let step_params = params.steps_array();
        let targets = params.cv_targets().map(|t| t.value());
        let amounts = params.cv_amounts().map(|a| a.value());
        // Read the CV-in slots once per block; offsets are smoothed per sample below.
        self.modulator.begin_block(targets, amounts, self.sample_rate);
        let glide_off = target_offsets(targets, self.modulator.current())[CvTarget::Glide.to_index()];
        let block = dsp::BlockParams {
            steps: params.steps.value() as usize,
            rate: params.rate.value(),
            swing: params.swing.value(),
            glide_ms: modulate(&params.glide, params.glide.value(), glide_off),
            pattern: std::array::from_fn(|i| step_params[i].value()),
        };
        self.engine.begin_block(
            transport.playing,
            transport.pos_beats(),
            transport.tempo,
            transport.loop_range_beats(),
            &block,
        );

        for frame in buffer.iter_samples() {
            let off = target_offsets(targets, self.modulator.next());
            let frame_params = dsp::FrameParams {
                formant_shift: modulate(&params.formant_shift, params.formant_shift.smoothed.next(), off[CvTarget::FormantShift.to_index()]),
                resonance: modulate(&params.resonance, params.resonance.smoothed.next(), off[CvTarget::Resonance.to_index()]),
                drive_db: modulate(&params.drive, params.drive.smoothed.next(), off[CvTarget::Drive.to_index()]),
                mix: modulate(&params.mix, params.mix.smoothed.next(), off[CvTarget::Mix.to_index()]),
                output_gain_db: params.output_gain.smoothed.next(),
            };
            self.engine.process_frame(frame, &frame_params);
        }

        self.current_step.store(self.engine.current_step(), Ordering::Relaxed);
        self.modulator.publish(self.engine.bus_signals());
        ProcessStatus::Normal
    }
}

impl ClapPlugin for Vss {
    const CLAP_ID: &'static str = "com.github.bubbaschmee.vss";
    const CLAP_DESCRIPTION: Option<&'static str> =
        Some("A tempo-synced vowel step sequencer / formant filter");
    const CLAP_MANUAL_URL: Option<&'static str> = Some(Self::URL);
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::AudioEffect,
        ClapFeature::Filter,
        ClapFeature::Stereo,
    ];
}

impl Vst3Plugin for Vss {
    const VST3_CLASS_ID: [u8; 16] = *b"VssVowelStepSeq!";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Fx, Vst3SubCategory::Filter];
}

nih_export_clap!(Vss);
nih_export_vst3!(Vss);
