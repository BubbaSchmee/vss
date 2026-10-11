//! RR-BUS integration (riddim-rack SPEC "RR-BUS", "Device tests"). Every test opens its own
//! uniquely named bus, so none of them touches a bus a running DAW is using.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use nih_plug::prelude::*;
use rr_bus::{Bus, Kind, Subscription};
use vss::bus::{modulate, target_offsets, Link, Modulator};
use vss::dsp::{BlockParams, Engine, FrameParams};
use vss::params::{CvTarget, Rate, Vowel, VssParams, DEFAULT_PATTERN};

const SR: f32 = 48_000.0;
const BLOCK: usize = 64;

fn bus() -> Bus {
    static N: AtomicUsize = AtomicUsize::new(0);
    let b = Bus::open_named(&format!("Local\\VssBusTest.{}.{}", std::process::id(), N.fetch_add(1, Ordering::Relaxed)));
    assert!(b.is_enabled());
    b
}

struct Rig {
    params: Arc<VssParams>,
    link: Arc<Link>,
    m: Modulator,
    // What the host would report for the cvN_target / cvN_amount params (nih-plug offers no public
    // setter outside a GUI context, and `begin_block` takes plain values anyway).
    targets: [CvTarget; 3],
    amounts: [f32; 3],
}

fn rig(bus: &Bus) -> Rig {
    let params = Arc::new(VssParams::default());
    let link = Link::new(params.clone());
    let mut m = Modulator::new(link.clone());
    m.initialize(bus.clone());
    Rig { params, link, m, targets: [CvTarget::Off; 3], amounts: [0.0; 3] }
}

impl Rig {
    /// One block the way `Vss::process` does it: read the slots, then advance the smoothing per
    /// sample. Returns the Resonance offset after the block's last sample.
    fn block(&mut self, frames: usize) -> f32 {
        let (targets, amounts) = (self.targets, self.amounts);
        self.m.begin_block(targets, amounts, SR);
        let mut off = 0.0;
        for _ in 0..frames {
            off = target_offsets(targets, self.m.next())[CvTarget::Resonance.to_index()];
        }
        off
    }

    fn run(&mut self, frames: usize) -> f32 {
        let mut off = 0.0;
        for _ in 0..frames / BLOCK {
            off = self.block(BLOCK);
        }
        off
    }
}

#[test]
fn bound_slot_moves_the_target_within_one_block_plus_smoothing() {
    let bus = bus();
    let publisher = bus.register("GTS");
    let ramp = publisher.publish("Ramp", Kind::Unipolar);
    let mut rig = rig(&bus);
    rig.link.bind(0, "GTS 1 \u{b7} Ramp");
    assert!(rig.link.is_bound(0));
    rig.targets[0] = CvTarget::Resonance;
    rig.amounts[0] = 0.25;

    // Zero before the publisher moves.
    assert_eq!(rig.block(BLOCK), 0.0);

    // The ramp jumps to 1.0: within one block the offset has moved by the 5 ms one-pole's share
    // of the goal (0.25), and it settles on the goal after a few time constants.
    ramp.set(1.0);
    let one_block = rig.block(BLOCK);
    let expected = 0.25 * (1.0 - (-(BLOCK as f32) / (0.005 * SR)).exp());
    assert!(one_block > 0.0 && (one_block - expected).abs() < 1e-3, "{one_block} vs {expected}");
    let settled = rig.run((0.005 * SR) as usize * 6);
    assert!((settled - 0.25).abs() < 1e-3, "{settled}");

    // In plain units: resonance 0.5 (normalized 0.5) + 0.25 -> 0.75; clamped at the top.
    let p = &rig.params.resonance;
    assert!((modulate(p, 0.5, settled) - 0.75).abs() < 2e-3);
    assert_eq!(modulate(p, 0.9, 0.5), 1.0);
    assert_eq!(modulate(p, 0.1, -0.5), 0.0);

    // Negative amount lowers it again, and the host value is untouched.
    rig.amounts[0] = -0.25;
    let down = rig.run(6000);
    assert!((down + 0.25).abs() < 1e-3, "{down}");
    assert_eq!(rig.params.resonance.value(), 0.5, "modulation must never write the host parameter");
}

#[test]
fn unbound_lost_or_off_applies_zero() {
    let bus = bus();
    let publisher = bus.register("GTS");
    let src = publisher.publish("Gate", Kind::Gate);
    src.set(1.0);
    let mut rig = rig(&bus);
    rig.targets[0] = CvTarget::Resonance;
    rig.amounts[0] = 0.5;

    // Unbound.
    assert_eq!(rig.run(2000), 0.0);

    // Bound, then the publisher goes away: lost, offset decays to exactly zero.
    rig.link.bind(0, "GTS 1 \u{b7} Gate");
    assert!((rig.run(6000) - 0.5).abs() < 1e-3);
    drop(src);
    drop(publisher);
    assert_eq!(rig.run(8000), 0.0);
    assert_eq!(modulate(&rig.params.resonance, 0.37, 0.0), 0.37);
    assert!(!rig.link.is_bound(0), "the slot flags itself lost after a failed read");

    // Target Off: bound and live, still zero.
    let publisher = bus.register("GTS");
    let src = publisher.publish("Gate", Kind::Gate);
    src.set(1.0);
    rig.link.refresh();
    assert!(rig.link.is_bound(0), "rebinds by label + name when the publisher is back");
    assert!((rig.run(6000) - 0.5).abs() < 1e-3);
    rig.targets[0] = CvTarget::Off;
    assert_eq!(rig.run(8000), 0.0);
}

#[test]
fn slots_on_the_same_target_add_and_other_targets_stay_put() {
    let o = target_offsets([CvTarget::Drive, CvTarget::Drive, CvTarget::Mix], [0.25, -0.1, 0.5]);
    assert!((o[CvTarget::Drive.to_index()] - 0.15).abs() < 1e-6);
    assert_eq!(o[CvTarget::Mix.to_index()], 0.5);
    assert_eq!(o[CvTarget::Resonance.to_index()], 0.0);
    assert_eq!(target_offsets([CvTarget::Off; 3], [1.0; 3]), [0.0; 6]);
    // Normalized-space offsets on a non-linear range: Glide 0..500 ms is skewed, so +0.1
    // normalized is not +50 ms, and Formant Shift -12..12 moves 2.4 st for +0.1.
    let p = VssParams::default();
    assert!((modulate(&p.formant_shift, 0.0, 0.1) - 2.4).abs() < 1e-4);
    let g = modulate(&p.glide, 20.0, 0.1);
    assert!(g > 20.0 && (g - 70.0).abs() > 1.0, "{g}");
}

#[test]
fn persisted_fields_round_trip_through_serde() {
    let params = VssParams::default();
    *params.cv_sources.write().unwrap() = ["GTS 1 \u{b7} Mod A".into(), String::new(), "Lead bass \u{b7} Step Gate".into()];
    *params.instance_label.write().unwrap() = "Lead bass".into();
    let saved: BTreeMap<String, String> = params.serialize_fields();
    assert!(saved.contains_key("cv_sources") && saved.contains_key("instance_label"), "{saved:?}");

    let restored = VssParams::default();
    assert_eq!(*restored.cv_sources.read().unwrap(), [String::new(), String::new(), String::new()]);
    restored.deserialize_fields(&saved);
    assert_eq!(*restored.cv_sources.read().unwrap(), *params.cv_sources.read().unwrap());
    assert_eq!(*restored.instance_label.read().unwrap(), "Lead bass");
}

#[test]
fn loading_state_rebinds_the_sources_and_the_label() {
    let bus = bus();
    let publisher = bus.register("GTS");
    let src = publisher.publish("Mod A", Kind::Bipolar);
    src.set(-0.5);

    // "Save": a session with a patched slot and a renamed instance.
    let first = rig(&bus);
    first.link.bind(1, "GTS 1 \u{b7} Mod A");
    first.link.set_label("Lead bass");
    let saved = first.params.serialize_fields();
    drop(first);

    // "Load" into a fresh instance: deserialize, then `initialize` re-registers under the saved
    // label and binds the saved sources.
    let params = Arc::new(VssParams::default());
    params.deserialize_fields(&saved);
    let link = Link::new(params.clone());
    let mut m = Modulator::new(link.clone());
    m.initialize(bus.clone());
    assert_eq!(link.label(), "Lead bass");
    assert!(link.is_bound(1) && !link.is_bound(0));
    m.begin_block([CvTarget::Mix; 3], [1.0; 3], SR);
    assert_eq!(link.activity(1), 0.5);

    // The device's own sources are listed under the saved label, and re-registering without a
    // saved label persists the default so a saved set re-resolves it.
    let names: Vec<String> = bus.list().iter().map(|s| s.persisted()).collect();
    for n in ["Lead bass \u{b7} Step Gate", "Lead bass \u{b7} Vowel", "Lead bass \u{b7} Step"] {
        assert!(names.contains(&n.to_string()), "{names:?}");
    }
    let fresh = rig(&bus);
    assert_eq!(*fresh.params.instance_label.read().unwrap(), "VSS 1");
}

#[test]
fn reclaimed_instance_registers_again_from_reset() {
    let bus = bus();
    let mut rig = rig(&bus);
    assert_eq!(bus.list().len(), 3);
    rig.m.ensure_registered(); // healthy: no churn
    assert_eq!(bus.list().len(), 3);
    // A host that stops calling `process` for over 2 s gets its record reclaimed by the next
    // register on the bus (real wall clock; there is no way to age a heartbeat).
    std::thread::sleep(std::time::Duration::from_millis(2100));
    let other = bus.register("GTS"); // reclaims every record with a stale heartbeat
    assert!(bus.list().is_empty(), "the stale VSS record and its sources were reclaimed");
    rig.m.ensure_registered();
    let names: Vec<String> = bus.list().iter().map(|s| s.persisted()).collect();
    assert_eq!(names.len(), 3, "{names:?}");
    drop(other);
}

#[test]
fn publishes_step_gate_vowel_and_step() {
    let bus = bus();
    let rig = rig(&bus);
    let sub = |n: &str| Subscription::bind(&bus, &rig.link.label(), n);
    let (gate, vowel, step) = (sub("Step Gate"), sub("Vowel"), sub("Step"));
    assert_eq!((gate.read(), vowel.read(), step.read()), (Some(0.0), Some(0.0), Some(0.0)));

    let mut pattern = DEFAULT_PATTERN;
    pattern[1] = Vowel::U;
    pattern[2] = Vowel::Hold; // keeps U
    pattern[3] = Vowel::E;
    let mut engine = Engine::new();
    engine.set_sample_rate(SR, 2);
    let mut at = 0usize;
    // 150 BPM @ 48 kHz, 1/8: one step is exactly 9600 samples. Run to `n` and publish.
    let mut run_to = |n: usize, swing: f32| {
        let block = BlockParams { steps: 8, rate: Rate::Eighth, swing, glide_ms: 0.0, pattern };
        while at < n {
            engine.begin_block(false, None, Some(150.0), None, &block);
            for _ in 0..BLOCK.min(n - at) {
                let mut frame = [0.0f32; 2];
                engine.process_frame(frame.iter_mut(), &FrameParams { formant_shift: 0.0, resonance: 0.5, drive_db: 0.0, mix: 1.0, output_gain_db: 0.0 });
                at += 1;
            }
        }
        rig.m.publish(engine.bus_signals());
    };

    run_to(100, 0.0); // step 0 (A), first half
    assert_eq!((gate.read(), vowel.read(), step.read()), (Some(1.0), Some(0.0), Some(0.0)));
    run_to(6000, 0.0); // step 0, second half
    assert_eq!(gate.read(), Some(0.0));
    run_to(9600 + 100, 0.0); // step 1 (U)
    assert_eq!((gate.read(), vowel.read(), step.read()), (Some(1.0), Some(1.0), Some(0.125)));
    run_to(2 * 9600 + 100, 0.0); // step 2 is Hold: the vowel stays U
    assert_eq!((vowel.read(), step.read()), (Some(1.0), Some(0.25)));
    run_to(3 * 9600 + 7000, 0.0); // step 3 (E), second half
    assert_eq!((gate.read(), vowel.read(), step.read()), (Some(0.0), Some(0.25), Some(0.375)));
}

#[test]
fn step_gate_follows_swing() {
    // Swing 1.0 stretches the even step to 1 + 1/3 step units (12800 samples), so its gate is
    // still high at 6200 samples and low at 7000 (the first half ends at 6400).
    let mut seq = vss::dsp::sequencer::Sequencer::new();
    seq.begin_block(false, None, Some(150.0), None, SR);
    let spb = vss::dsp::sequencer::steps_per_beat(Rate::Eighth);
    for _ in 0..6200 {
        seq.next_step(spb, 1.0, 8);
    }
    assert!(seq.step_phase() < 0.5, "{}", seq.step_phase());
    for _ in 0..800 {
        seq.next_step(spb, 1.0, 8);
    }
    assert!(seq.step_phase() >= 0.5, "{}", seq.step_phase());
    assert_eq!(seq.steps_active(), 8);
}
