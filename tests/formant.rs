//! Formant filter, drive and demo-render checks (SPEC.md "Verification" 1 and 4). std only.

use vss::dsp::formant::FormantFilter;
use vss::dsp::Drive;
use vss::params::{vowel_formants, Vowel};

#[allow(dead_code)]
#[path = "../examples/demo.rs"]
mod demo;

const SR: f64 = 48_000.0;
const VOWELS: [Vowel; 5] = [Vowel::A, Vowel::E, Vowel::I, Vowel::O, Vowel::U];

/// Goertzel magnitude of `x` at `freq`, with an optional Hann window.
fn goertzel(x: &[f32], freq: f64, hann: bool) -> f64 {
    let n = x.len() as f64;
    let coeff = 2.0 * (std::f64::consts::TAU * freq / SR).cos();
    let (mut s1, mut s2) = (0.0f64, 0.0f64);
    for (i, &v) in x.iter().enumerate() {
        let w = if hann {
            0.5 - 0.5 * (std::f64::consts::TAU * i as f64 / n).cos()
        } else {
            1.0
        };
        let s0 = v as f64 * w + coeff * s1 - s2;
        s2 = s1;
        s1 = s0;
    }
    (s1 * s1 + s2 * s2 - coeff * s1 * s2).max(0.0).sqrt()
}

/// Frequency with the largest magnitude over `lo..=hi` in `step` Hz increments.
fn peak(x: &[f32], lo: f64, hi: f64, step: f64, hann: bool) -> f64 {
    let mut best = (lo, -1.0);
    let mut f = lo;
    while f <= hi {
        let m = goertzel(x, f, hann);
        if m > best.1 {
            best = (f, m);
        }
        f += step;
    }
    best.0
}

fn filter_for(vowel: Vowel, resonance: f32) -> impl FnMut(f32) -> f32 {
    let mut filter = FormantFilter::new(SR as f32);
    filter.set_glide_ms(0.0);
    filter.set_target(&vowel_formants(vowel));
    move |x| filter.process(x, 0.0, resonance)
}

/// SPEC.md vowel table (F1, F2, F3 Hz), hard-coded so a typo in `params.rs` fails the tests.
const SPEC_TABLE: [(Vowel, [f32; 3]); 5] = [
    (Vowel::A, [730.0, 1090.0, 2440.0]),
    (Vowel::E, [530.0, 1840.0, 2480.0]),
    (Vowel::I, [270.0, 2290.0, 3010.0]),
    (Vowel::O, [570.0, 840.0, 2410.0]),
    (Vowel::U, [300.0, 870.0, 2240.0]),
];

fn table(vowel: Vowel) -> [(f64, f64); 3] {
    let f = SPEC_TABLE.iter().find(|(v, _)| *v == vowel).unwrap().1;
    [(f[0] as f64, 0.05), (f[1] as f64, 0.05), (f[2] as f64, 0.08)]
}

#[test]
fn vowel_table_matches_spec() {
    for (vowel, [f1, f2, f3]) in SPEC_TABLE {
        let f = vowel_formants(vowel);
        assert_eq!([f.f1, f.f2, f.f3], [f1, f2, f3], "{vowel:?}");
        assert_eq!(f.gains, [0.0, -6.0, -12.0], "{vowel:?}");
    }
}

/// SPEC stimulus: 55 Hz saw, 2 s, 48 kHz, glide 0, shift 0, resonance 0.7; Goertzel over +-10 %.
///
/// A saw only has energy on 55 Hz harmonics, so the measured peak is always a harmonic. U's F1
/// (300 Hz) sits 25 Hz (8.3 %) from the nearest one (275 Hz), so the literal 5 % bound can't be
/// met by any filter there. Where no harmonic lies inside the tolerance, the test instead requires
/// the peak to be the harmonic nearest the formant. `formant_peaks_impulse_response` checks the
/// continuous response at every formant with the literal tolerances.
#[test]
fn formant_peaks_saw() {
    let saw = saw(2 * SR as usize);
    for vowel in VOWELS {
        let mut f = filter_for(vowel, 0.7);
        let y: Vec<f32> = saw.iter().map(|&x| f(x)).collect();
        for (target, tol) in table(vowel) {
            let p = peak(&y, (target * 0.9).floor(), (target * 1.1).ceil(), 1.0, true);
            let nearest_harmonic = (target / 55.0).round() * 55.0;
            let reachable = (nearest_harmonic - target).abs() / target <= tol;
            if reachable {
                assert!(
                    (p - target).abs() / target <= tol,
                    "{vowel:?}: peak {p} Hz not within {tol} of {target} Hz"
                );
            } else {
                assert!(
                    (p - nearest_harmonic).abs() <= 1.0,
                    "{vowel:?}: peak {p} Hz is not the harmonic nearest {target} Hz ({nearest_harmonic})"
                );
            }
        }
    }
}

/// Same sweep on the impulse response, i.e. the filter's continuous magnitude response.
#[test]
fn formant_peaks_impulse_response() {
    for vowel in VOWELS {
        let mut f = filter_for(vowel, 0.7);
        let ir: Vec<f32> = (0..32_768)
            .map(|n| f(if n == 0 { 1.0 } else { 0.0 }))
            .collect();
        for (target, tol) in table(vowel) {
            let p = peak(&ir, target * 0.9, target * 1.1, target * 0.001, false);
            assert!(
                (p - target).abs() / target <= tol,
                "{vowel:?}: IR peak {p:.1} Hz not within {tol} of {target} Hz"
            );
            assert!(
                (p - target * 0.9).abs() > 1.0 && (p - target * 1.1).abs() > 1.0,
                "{vowel:?}: IR peak {p:.1} Hz is at the sweep edge, not a local peak"
            );
        }
    }
}

fn rms(x: &[f32]) -> f64 {
    (x.iter().map(|&v| v as f64 * v as f64).sum::<f64>() / x.len() as f64).sqrt()
}

fn saw(len: usize) -> Vec<f32> {
    (0..len)
        .map(|n| (2.0 * (n as f64 * 55.0 / SR).fract() - 1.0) as f32)
        .collect()
}

/// Peak of a 0 dBFS 55 Hz saw (2 s) through vowel A, shift 0, glide 0, skipping 0.1 s of settling.
fn saw_peak(resonance: f32) -> f32 {
    let mut f = filter_for(Vowel::A, resonance);
    let y: Vec<f32> = saw(2 * SR as usize).iter().map(|&x| f(x)).collect();
    y[SR as usize / 10..].iter().fold(0.0, |m, v| m.max(v.abs()))
}

/// The documented makeup constant (SPEC DSP step 3): 0 dBFS saw at the defaults peaks at ~1.0.
/// Measured 1.0004 at resonance 0.5; resonance 1.0 narrows the bands and peaks at ~0.35.
#[test]
fn makeup_gives_unity_peak_on_full_scale_saw() {
    let (p05, p10) = (saw_peak(0.5), saw_peak(1.0));
    println!("0 dBFS saw peak: resonance 0.5 {p05:.4}, resonance 1.0 {p10:.4}");
    assert!((0.9..1.1).contains(&p05), "resonance 0.5 peak {p05}");
    assert!(p10 < 2.0, "resonance 1.0 peak {p10}");
}

#[test]
fn filter_is_finite_and_resets() {
    let mut filter = FormantFilter::new(SR as f32);
    filter.set_glide_ms(500.0);
    for (i, vowel) in VOWELS.iter().cycle().take(50).enumerate() {
        filter.set_target(&vowel_formants(*vowel));
        for n in 0..1000 {
            let x = if (n / 10) % 2 == 0 { 1.0 } else { -1.0 };
            let y = filter.process(x, (i % 25) as f32 - 12.0, (i % 11) as f32 / 10.0);
            assert!(y.is_finite());
        }
    }
    filter.reset();
    assert_eq!(filter.process(0.0, 0.0, 0.5), 0.0);
}

#[test]
fn drive_unity_at_zero_and_finite_at_max() {
    let mut drive = Drive::new();
    drive.set_db(0.0);
    for i in -1000..=1000 {
        let x = i as f32 / 100.0;
        assert_eq!(drive.process(x), x);
    }
    drive.set_db(24.0);
    for x in [10.0f32, -10.0, 1.0, -1.0, 0.0, 1e-30] {
        let y = drive.process(x);
        assert!(
            y.is_finite() && y.abs() <= 1.0 + 1e-6,
            "drive(24 dB)({x}) = {y}"
        );
    }
    // Continuous at 0: a hair of drive changes the level by a hair, not the +2.4 dB of 1/tanh(1).
    drive.set_db(1e-3);
    assert!((drive.process(0.1) / 0.1 - 1.0).abs() < 1e-3);
}

/// Listen-check stand-in: the demo is neither silent nor clipping, and an A step and an I step
/// have audibly different spectra (A: strong ~715 Hz, weak ~275 Hz; I: the reverse).
#[test]
fn demo_render_is_sane_and_talks() {
    let frames = demo::render();
    let left: Vec<f32> = frames.iter().map(|f| f[0]).collect();
    assert!(
        frames.iter().all(|f| f[0] == f[1]),
        "mono source should stay identical per channel"
    );

    let level_db = 20.0 * rms(&left).log10();
    let peak = left.iter().fold(0.0f32, |m, v| m.max(v.abs()));
    println!("demo RMS {level_db:.1} dBFS, peak {peak:.3}");
    assert!(
        (-30.0..=-6.0).contains(&level_db),
        "demo RMS {level_db} dBFS"
    );
    assert!(peak < 0.99, "demo peaks at {peak}");

    // Second bar (8 steps of 1/8 per bar with steps = 8): step 0 is A, step 7 is I. Skip the
    // first 30 ms of each note (glide), stop at the gate end.
    let bar = 8 * demo::EIGHTH;
    let note = |step: usize| {
        let start = bar + step * demo::EIGHTH + 1_440;
        &left[start..bar + step * demo::EIGHTH + demo::EIGHTH * 3 / 4]
    };
    let tilt = |x: &[f32]| 20.0 * (goertzel(x, 715.0, true) / goertzel(x, 275.0, true)).log10();
    let (a, i) = (tilt(note(0)), tilt(note(7)));
    println!("715/275 Hz balance: A {a:.1} dB, I {i:.1} dB");
    assert!(
        a - i > 20.0,
        "A and I steps don't differ enough: A {a:.1} dB, I {i:.1} dB"
    );
}
