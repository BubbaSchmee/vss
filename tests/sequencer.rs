//! Sequencer timing against an independent integer reference (SPEC.md "Verification" 1). std only.
//!
//! Reference: at 150 BPM / 48 kHz one beat is exactly 19 200 samples, so every rate's step is a
//! whole number of samples `S`. Within each 2-step pair (2S samples) the odd step starts at
//! `S * (1 + swing/3)`; with swing = k/2 that is `offset * 6 >= S * (6 + k)`, all integers.

use vss::dsp::sequencer::{steps_per_beat, Sequencer};
use vss::params::Rate;

const SR: f32 = 48_000.0;
const SAMPLES_PER_BEAT: i64 = 19_200; // 150 BPM at 48 kHz
const RATES: [(Rate, i64); 5] = [
    (Rate::Quarter, 19_200),
    (Rate::Eighth, 9_600),
    (Rate::EighthTriplet, 6_400),
    (Rate::Sixteenth, 4_800),
    (Rate::SixteenthTriplet, 3_200),
];

/// Step index at absolute sample `n` (sample 0 = beat 0); `swing_halves` = swing * 2.
fn reference(n: i64, step_samples: i64, swing_halves: i64, steps: i64) -> usize {
    let pair = n.div_euclid(2 * step_samples);
    let offset = n.rem_euclid(2 * step_samples);
    let odd = (offset * 6 >= step_samples * (6 + swing_halves)) as i64;
    (2 * pair + odd).rem_euclid(steps) as usize
}

fn host_pos(n: i64) -> f64 {
    n as f64 / SAMPLES_PER_BEAT as f64
}

/// Drives the sequencer like a host: transport sampled at the start of odd-sized blocks.
fn run(
    seq: &mut Sequencer,
    playing: bool,
    first_sample: i64,
    len: i64,
    rate: Rate,
    swing: f32,
    steps: usize,
    mut check: impl FnMut(i64, usize),
) {
    const BLOCK: i64 = 437;
    let mut n = first_sample;
    while n < first_sample + len {
        seq.begin_block(playing, Some(host_pos(n)), Some(150.0), None, SR);
        for k in n..(n + BLOCK).min(first_sample + len) {
            check(k, seq.next_step(steps_per_beat(rate), swing, steps));
        }
        n += BLOCK;
    }
}

#[test]
fn playing_matches_reference_for_every_rate_and_swing() {
    for (rate, step_samples) in RATES {
        for swing_halves in [0i64, 1, 2] {
            for steps in [5usize, 16] {
                let swing = swing_halves as f32 / 2.0;
                let mut seq = Sequencer::new();
                let mut changes = 0;
                let mut last = usize::MAX;
                // Start slightly before 0 (pre-roll) and run 8 bars.
                run(
                    &mut seq,
                    true,
                    -1_000,
                    32 * SAMPLES_PER_BEAT,
                    rate,
                    swing,
                    steps,
                    |n, got| {
                        let want = reference(n, step_samples, swing_halves, steps as i64);
                        assert_eq!(
                            got, want,
                            "{rate:?} swing {swing} steps {steps} at sample {n}"
                        );
                        changes += (got != last) as usize;
                        last = got;
                    },
                );
                assert!(changes > 16, "{rate:?}: sequencer never moved");
            }
        }
    }
}

#[test]
fn free_runs_at_host_tempo_when_stopped() {
    for (rate, step_samples) in RATES {
        let mut seq = Sequencer::new();
        // Host position while stopped is ignored; the internal counter starts at beat 0.
        let mut seen = std::collections::HashSet::new();
        let mut n0 = 0i64;
        const BLOCK: i64 = 300;
        while n0 < 8 * SAMPLES_PER_BEAT {
            seq.begin_block(false, Some(1234.5), Some(150.0), None, SR);
            for n in n0..n0 + BLOCK {
                let got = seq.next_step(steps_per_beat(rate), 0.5, 16);
                assert_eq!(
                    got,
                    reference(n, step_samples, 1, 16),
                    "{rate:?} free-run at {n}"
                );
                seen.insert(got);
            }
            n0 += BLOCK;
        }
        assert!(seen.len() > 4, "{rate:?}: free-run did not advance");
    }
}

#[test]
fn free_run_falls_back_to_120_bpm() {
    let mut seq = Sequencer::new();
    seq.begin_block(false, None, None, None, SR);
    // 120 BPM at 48 kHz: 24 000 samples per beat, 12 000 per 1/8.
    for n in 0..48_000i64 {
        let got = seq.next_step(steps_per_beat(Rate::Eighth), 0.0, 16);
        assert_eq!(got, (n / 12_000) as usize, "at {n}");
    }
}

#[test]
fn snaps_to_host_position_on_play() {
    let (rate, step_samples) = (Rate::Sixteenth, 4_800);
    let mut seq = Sequencer::new();
    // Jam with the transport stopped for an arbitrary while ...
    run(&mut seq, false, 0, 10_007, rate, 0.0, 16, |_, _| {});
    // ... then press play at beat 8.25 + a bit: the very first sample must follow the host.
    let start = 8 * SAMPLES_PER_BEAT + 4_800 + 1_234;
    run(
        &mut seq,
        true,
        start,
        4 * SAMPLES_PER_BEAT,
        rate,
        0.0,
        16,
        |n, got| {
            assert_eq!(got, reference(n, step_samples, 0, 16), "after snap at {n}");
        },
    );
    // Stop again: free-run continues from where the host left off, no jump.
    let resume = start + 4 * SAMPLES_PER_BEAT;
    let mut n = resume;
    for _ in 0..40 {
        seq.begin_block(false, Some(0.0), Some(150.0), None, SR);
        for _ in 0..512 {
            let got = seq.next_step(steps_per_beat(rate), 0.0, 16);
            assert_eq!(
                got,
                reference(n, step_samples, 0, 16),
                "free-run after stop at {n}"
            );
            n += 1;
        }
    }
}

#[test]
fn steps_change_applies_at_next_boundary() {
    let mut seq = Sequencer::new();
    let spb = steps_per_beat(Rate::Eighth);
    seq.begin_block(true, Some(0.0), Some(150.0), None, SR);
    for _ in 0..(3 * 9_600 + 100) {
        seq.next_step(spb, 0.0, 8);
    }
    assert_eq!(seq.current(), 3);
    // Shrink to 2 steps mid-step: step 3 keeps playing until its end ...
    for _ in (3 * 9_600 + 100)..(4 * 9_600) {
        assert_eq!(seq.next_step(spb, 0.0, 2), 3);
    }
    // ... then absolute step 4 wraps with the new length.
    assert_eq!(seq.next_step(spb, 0.0, 2), 0);
    for _ in 0..9_600 {
        seq.next_step(spb, 0.0, 2);
    }
    assert_eq!(seq.current(), 1);
}

/// SPEC "Loop wrap": 3-beat host loop at 150 BPM / 48 kHz, 1/8, 8 steps, 512-sample blocks. The
/// loop end (57 600 samples) falls mid-block; samples past it must follow the wrapped position.
#[test]
fn loop_wrap_mid_block() {
    const LOOP_END: i64 = 3 * SAMPLES_PER_BEAT;
    const BLOCK: i64 = 512;
    assert_ne!(LOOP_END % BLOCK, 0, "loop end must fall inside a block");
    let spb = steps_per_beat(Rate::Eighth);
    let mut seq = Sequencer::new();
    let mut n = 0i64; // continuous sample count; the host reports the looped position
    while n < 4 * LOOP_END {
        let host = host_pos(n.rem_euclid(LOOP_END));
        seq.begin_block(true, Some(host), Some(150.0), Some((0.0, 3.0)), SR);
        for k in n..n + BLOCK {
            let looped = k.rem_euclid(LOOP_END);
            let got = seq.next_step(spb, 0.0, 8);
            assert_eq!(got, reference(looped, 9_600, 0, 8), "at {k} (loop pos {looped})");
            if looped == 0 && k > 0 {
                assert_eq!(got, 0, "first sample after loop end at {k}");
            }
        }
        n += BLOCK;
    }
}

/// SPEC "Loop wrap": a nih-plug sub-block split by sample-accurate automation can hand
/// `begin_block` a `pos_beats` that already extrapolated past `loop_end` even though the loop
/// is still active. That must wrap the anchor to `start + (host - end).rem_euclid(end - start)`
/// and keep the loop active, yielding the same steps as the already-wrapped position (checked
/// against the same independent integer reference `loop_wrap_mid_block` uses).
#[test]
fn begin_block_wraps_host_position_already_past_loop_end() {
    const LOOP_END: i64 = 3 * SAMPLES_PER_BEAT;
    const OVERSHOOT: i64 = 1_234; // mid-block, not a step boundary
    const BLOCK: i64 = 512;
    let spb = steps_per_beat(Rate::Eighth);

    let mut seq = Sequencer::new();
    seq.begin_block(true, Some(host_pos(LOOP_END + OVERSHOOT)), Some(150.0), Some((0.0, 3.0)), SR);
    for k in 0..BLOCK {
        let got = seq.next_step(spb, 0.0, 8);
        let want = reference(OVERSHOOT + k, 9_600, 0, 8);
        assert_eq!(got, want, "at {k} after wrap");
    }

    // Loop must stay active (not disabled): the next block, still reported past loop_end,
    // keeps wrapping too.
    seq.begin_block(
        true,
        Some(host_pos(LOOP_END + OVERSHOOT + BLOCK)),
        Some(150.0),
        Some((0.0, 3.0)),
        SR,
    );
    let got = seq.next_step(spb, 0.0, 8);
    let want = reference(OVERSHOOT + BLOCK, 9_600, 0, 8);
    assert_eq!(got, want, "loop must remain active across blocks");
}
