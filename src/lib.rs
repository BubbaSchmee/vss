//! VSS — Vowel Step Sequencer. A 3-band formant filter whose vowel is chosen by a tempo-synced
//! 16-step sequencer. See SPEC.md for the full contract.

pub mod dsp;
mod editor;
pub mod params;

use nih_plug::prelude::*;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use params::VssParams;

pub struct Vss {
    params: Arc<VssParams>,
    current_step: Arc<AtomicUsize>,
    engine: dsp::Engine,
}

impl Default for Vss {
    fn default() -> Self {
        Self {
            params: Arc::new(VssParams::default()),
            current_step: Arc::new(AtomicUsize::new(0)),
            engine: dsp::Engine::new(),
        }
    }
}

impl Plugin for Vss {
    const NAME: &'static str = "VSS";
    const VENDOR: &'static str = "VSS Open Source";
    const URL: &'static str = "https://github.com/BubbaSchmee/vss";
    const EMAIL: &'static str = "info@example.com";

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
        editor::create(self.params.clone(), self.current_step.clone())
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

        true
    }

    fn reset(&mut self) {
        self.engine.reset();
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
        let block = dsp::BlockParams {
            steps: params.steps.value() as usize,
            rate: params.rate.value(),
            swing: params.swing.value(),
            glide_ms: params.glide.value(),
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
            let frame_params = dsp::FrameParams {
                formant_shift: params.formant_shift.smoothed.next(),
                resonance: params.resonance.smoothed.next(),
                drive_db: params.drive.smoothed.next(),
                mix: params.mix.smoothed.next(),
                output_gain_db: params.output_gain.smoothed.next(),
            };
            self.engine.process_frame(frame, &frame_params);
        }

        self.current_step.store(self.engine.current_step(), Ordering::Relaxed);
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
