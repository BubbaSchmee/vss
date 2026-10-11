//! The complete parameter contract for VSS. See `SPEC.md` "Parameter contract" and "Vowel table".
//!
//! This file is consumed by later agents (DSP, GUI, tests) — the shapes here are the contract,
//! keep them exact. Parameter ids are stable and must never be renamed.

use nih_plug::prelude::*;
use std::sync::RwLock;

/// Tempo-synced step rate. Display strings match the SPEC exactly.
#[derive(Enum, Debug, PartialEq, Clone, Copy)]
pub enum Rate {
    #[name = "1/4"]
    Quarter,
    #[name = "1/8"]
    Eighth,
    #[name = "1/8T"]
    EighthTriplet,
    #[name = "1/16"]
    Sixteenth,
    #[name = "1/16T"]
    SixteenthTriplet,
}

impl Rate {
    /// Step length in beats (quarter notes), per SPEC "Sequencer".
    pub fn step_len_beats(self) -> f64 {
        match self {
            Rate::Quarter => 1.0,
            Rate::Eighth => 0.5,
            Rate::EighthTriplet => 1.0 / 3.0,
            Rate::Sixteenth => 0.25,
            Rate::SixteenthTriplet => 1.0 / 6.0,
        }
    }
}

/// A sequencer step's vowel target. `Hold` keeps the previous step's target (no new glide); if
/// step 1 is `Hold`, treat it as `A` (see SPEC "Vowel table").
#[derive(Enum, Debug, PartialEq, Clone, Copy)]
pub enum Vowel {
    A,
    E,
    I,
    O,
    U,
    Hold,
}

/// One vowel's three formant band centers (Hz) and relative band gains (dB), from Peterson &
/// Barney 1952 (male). Indices line up 1:1 with `f1`/`f2`/`f3` and `gains`.
#[derive(Debug, Clone, Copy)]
pub struct VowelFormants {
    pub f1: f32,
    pub f2: f32,
    pub f3: f32,
    /// Relative band gains in dB, applied to [f1, f2, f3] respectively.
    pub gains: [f32; 3],
}

/// Formant table for the five sung vowels. `Vowel::Hold` has no entry here — it never reaches the
/// DSP as a target, it's resolved to the previous step's vowel (or `A`) by the sequencer.
pub const VOWELS: [(Vowel, VowelFormants); 5] = [
    (
        Vowel::A,
        VowelFormants { f1: 730.0, f2: 1090.0, f3: 2440.0, gains: [0.0, -6.0, -12.0] },
    ),
    (
        Vowel::E,
        VowelFormants { f1: 530.0, f2: 1840.0, f3: 2480.0, gains: [0.0, -6.0, -12.0] },
    ),
    (
        Vowel::I,
        VowelFormants { f1: 270.0, f2: 2290.0, f3: 3010.0, gains: [0.0, -6.0, -12.0] },
    ),
    (
        Vowel::O,
        VowelFormants { f1: 570.0, f2: 840.0, f3: 2410.0, gains: [0.0, -6.0, -12.0] },
    ),
    (
        Vowel::U,
        VowelFormants { f1: 300.0, f2: 870.0, f3: 2240.0, gains: [0.0, -6.0, -12.0] },
    ),
];

/// Look up a vowel's formants. Panics on `Vowel::Hold` — callers must resolve `Hold` to a concrete
/// vowel first (see SPEC "Vowel table").
pub fn vowel_formants(vowel: Vowel) -> VowelFormants {
    VOWELS
        .iter()
        .find(|(v, _)| *v == vowel)
        .map(|(_, f)| *f)
        .expect("Vowel::Hold has no formant entry; resolve Hold before calling vowel_formants()")
}

/// Default step pattern (steps 1..16), per SPEC "Parameter contract".
pub const DEFAULT_PATTERN: [Vowel; 16] = [
    Vowel::A,
    Vowel::O,
    Vowel::A,
    Vowel::E,
    Vowel::U,
    Vowel::O,
    Vowel::A,
    Vowel::I,
    Vowel::A,
    Vowel::O,
    Vowel::A,
    Vowel::E,
    Vowel::U,
    Vowel::O,
    Vowel::I,
    Vowel::A,
];

/// What a CV-in slot modulates (RR-BUS, SPEC "Bus"). `Off` must stay first: it is the default.
#[derive(Enum, Debug, PartialEq, Clone, Copy)]
pub enum CvTarget {
    Off,
    #[name = "Formant Shift"]
    FormantShift,
    Resonance,
    Drive,
    Glide,
    Mix,
}

/// Short names for the PATCH strip target combo, in `CvTarget` order.
pub const CV_TARGET_LABELS: [&str; 6] = ["Off", "Formant", "Reson.", "Drive", "Glide", "Mix"];

#[derive(Params)]
pub struct VssParams {
    #[id = "formant_shift"]
    pub formant_shift: FloatParam,

    #[id = "resonance"]
    pub resonance: FloatParam,

    #[id = "drive"]
    pub drive: FloatParam,

    #[id = "steps"]
    pub steps: IntParam,

    #[id = "rate"]
    pub rate: EnumParam<Rate>,

    #[id = "swing"]
    pub swing: FloatParam,

    #[id = "glide"]
    pub glide: FloatParam,

    #[id = "step_1"]
    pub step_1: EnumParam<Vowel>,
    #[id = "step_2"]
    pub step_2: EnumParam<Vowel>,
    #[id = "step_3"]
    pub step_3: EnumParam<Vowel>,
    #[id = "step_4"]
    pub step_4: EnumParam<Vowel>,
    #[id = "step_5"]
    pub step_5: EnumParam<Vowel>,
    #[id = "step_6"]
    pub step_6: EnumParam<Vowel>,
    #[id = "step_7"]
    pub step_7: EnumParam<Vowel>,
    #[id = "step_8"]
    pub step_8: EnumParam<Vowel>,
    #[id = "step_9"]
    pub step_9: EnumParam<Vowel>,
    #[id = "step_10"]
    pub step_10: EnumParam<Vowel>,
    #[id = "step_11"]
    pub step_11: EnumParam<Vowel>,
    #[id = "step_12"]
    pub step_12: EnumParam<Vowel>,
    #[id = "step_13"]
    pub step_13: EnumParam<Vowel>,
    #[id = "step_14"]
    pub step_14: EnumParam<Vowel>,
    #[id = "step_15"]
    pub step_15: EnumParam<Vowel>,
    #[id = "step_16"]
    pub step_16: EnumParam<Vowel>,

    #[id = "mix"]
    pub mix: FloatParam,

    #[id = "output_gain"]
    pub output_gain: FloatParam,

    // RR-BUS CV-in slots. Ids are stable. Amount is bipolar -1..1 (default 0 = no modulation).
    #[id = "cv1_target"]
    pub cv1_target: EnumParam<CvTarget>,
    #[id = "cv1_amount"]
    pub cv1_amount: FloatParam,
    #[id = "cv2_target"]
    pub cv2_target: EnumParam<CvTarget>,
    #[id = "cv2_amount"]
    pub cv2_amount: FloatParam,
    #[id = "cv3_target"]
    pub cv3_target: EnumParam<CvTarget>,
    #[id = "cv3_amount"]
    pub cv3_amount: FloatParam,

    /// Persisted `"<label> · <name>"` per CV-in slot; empty = no source.
    #[persist = "cv_sources"]
    pub cv_sources: RwLock<[String; 3]>,
    /// The bus instance label (empty until the first registration names it).
    #[persist = "instance_label"]
    pub instance_label: RwLock<String>,
}

impl VssParams {
    /// The three CV-in target params in slot order.
    pub fn cv_targets(&self) -> [&EnumParam<CvTarget>; 3] {
        [&self.cv1_target, &self.cv2_target, &self.cv3_target]
    }

    /// The three CV-in amount params in slot order.
    pub fn cv_amounts(&self) -> [&FloatParam; 3] {
        [&self.cv1_amount, &self.cv2_amount, &self.cv3_amount]
    }

    /// The 16 step params in order, for the sequencer/GUI to index into without a 16-arm match.
    pub fn steps_array(&self) -> [&EnumParam<Vowel>; 16] {
        [
            &self.step_1,
            &self.step_2,
            &self.step_3,
            &self.step_4,
            &self.step_5,
            &self.step_6,
            &self.step_7,
            &self.step_8,
            &self.step_9,
            &self.step_10,
            &self.step_11,
            &self.step_12,
            &self.step_13,
            &self.step_14,
            &self.step_15,
            &self.step_16,
        ]
    }
}

impl Default for VssParams {
    fn default() -> Self {
        let pattern = DEFAULT_PATTERN;

        Self {
            formant_shift: FloatParam::new(
                "Formant Shift",
                0.0,
                FloatRange::Linear { min: -12.0, max: 12.0 },
            )
            .with_smoother(SmoothingStyle::Linear(20.0))
            .with_unit(" st")
            .with_value_to_string(formatters::v2s_f32_rounded(2)),

            resonance: FloatParam::new(
                "Resonance",
                0.5,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_smoother(SmoothingStyle::Linear(20.0))
            .with_value_to_string(formatters::v2s_f32_rounded(2)),

            drive: FloatParam::new("Drive", 0.0, FloatRange::Linear { min: 0.0, max: 24.0 })
                .with_smoother(SmoothingStyle::Linear(10.0))
                .with_unit(" dB")
                .with_value_to_string(formatters::v2s_f32_rounded(2)),

            steps: IntParam::new("Steps", 8, IntRange::Linear { min: 1, max: 16 }),

            rate: EnumParam::new("Rate", Rate::Eighth),

            swing: FloatParam::new("Swing", 0.0, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_value_to_string(formatters::v2s_f32_rounded(2)),

            // 0 ms = jump (no glide); log-skewed so the useful low end isn't crushed into a
            // handful of pixels. Not itself smoothed — it's a time constant the DSP reads.
            glide: FloatParam::new(
                "Glide",
                20.0,
                FloatRange::Skewed {
                    min: 0.0,
                    max: 500.0,
                    factor: FloatRange::skew_factor(-2.0),
                },
            )
            .with_unit(" ms")
            .with_value_to_string(formatters::v2s_f32_rounded(1)),

            step_1: EnumParam::new("Step 1", pattern[0]),
            step_2: EnumParam::new("Step 2", pattern[1]),
            step_3: EnumParam::new("Step 3", pattern[2]),
            step_4: EnumParam::new("Step 4", pattern[3]),
            step_5: EnumParam::new("Step 5", pattern[4]),
            step_6: EnumParam::new("Step 6", pattern[5]),
            step_7: EnumParam::new("Step 7", pattern[6]),
            step_8: EnumParam::new("Step 8", pattern[7]),
            step_9: EnumParam::new("Step 9", pattern[8]),
            step_10: EnumParam::new("Step 10", pattern[9]),
            step_11: EnumParam::new("Step 11", pattern[10]),
            step_12: EnumParam::new("Step 12", pattern[11]),
            step_13: EnumParam::new("Step 13", pattern[12]),
            step_14: EnumParam::new("Step 14", pattern[13]),
            step_15: EnumParam::new("Step 15", pattern[14]),
            step_16: EnumParam::new("Step 16", pattern[15]),

            mix: FloatParam::new("Mix", 1.0, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_smoother(SmoothingStyle::Linear(10.0))
                .with_value_to_string(formatters::v2s_f32_rounded(2)),

            output_gain: FloatParam::new(
                "Output",
                0.0,
                FloatRange::Linear { min: -24.0, max: 12.0 },
            )
            .with_smoother(SmoothingStyle::Linear(10.0))
            .with_unit(" dB")
            .with_value_to_string(formatters::v2s_f32_rounded(2)),

            cv1_target: EnumParam::new("CV 1 Target", CvTarget::Off),
            cv1_amount: FloatParam::new("CV 1 Amount", 0.0, FloatRange::Linear { min: -1.0, max: 1.0 })
                .with_value_to_string(formatters::v2s_f32_rounded(2)),

            cv2_target: EnumParam::new("CV 2 Target", CvTarget::Off),
            cv2_amount: FloatParam::new("CV 2 Amount", 0.0, FloatRange::Linear { min: -1.0, max: 1.0 })
                .with_value_to_string(formatters::v2s_f32_rounded(2)),

            cv3_target: EnumParam::new("CV 3 Target", CvTarget::Off),
            cv3_amount: FloatParam::new("CV 3 Amount", 0.0, FloatRange::Linear { min: -1.0, max: 1.0 })
                .with_value_to_string(formatters::v2s_f32_rounded(2)),

            cv_sources: RwLock::new(Default::default()),
            instance_label: RwLock::new(String::new()),
        }
    }
}
