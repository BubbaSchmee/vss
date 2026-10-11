//! Editor binding tests: run the real editor function headlessly over `MapCtl` and check that
//! every parameter id is bound, knobs sit on the right params, and gestures reach the right id
//! with begin / set / end pairing.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use rr_bus::{Bus, Kind};
use vss::bus::{Link, Modulator};

use nih_plug_egui::egui::{self, pos2, vec2, Event, Modifiers, PointerButton, RawInput, Rect};
use vss::editor::{self, compact_readout, Ctl, MapCtl, PatchView, CV_AMOUNT_IDS, CV_TARGET_IDS, KNOBS, STEP_IDS};
use vss::params::VssParams;

static NEXT: AtomicUsize = AtomicUsize::new(0);

struct Harness {
    ctx: egui::Context,
    ctl: MapCtl,
    link: Arc<Link>,
    view: PatchView,
    bus: Bus,
    // Keeps the device registered and two other devices' sources alive for the test's duration.
    _keep: (Modulator, rr_bus::Instance, rr_bus::Source, rr_bus::Source),
    t: f64,
}

impl Harness {
    fn new() -> Self {
        let ctx = egui::Context::default();
        rr_ui::theme::apply(&ctx);
        let params = Arc::new(VssParams::default());
        let bus = Bus::open_named(&format!("Local\\VssEditorTest.{}.{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
        let other = bus.register("GTS");
        let (mod_a, mod_b) = (other.publish("Mod A", Kind::Bipolar), other.publish("Mod B", Kind::Bipolar));
        let link = Link::new(params.clone());
        let mut m = Modulator::new(link.clone());
        m.initialize(bus.clone());
        let mut h = Self {
            ctx,
            ctl: MapCtl::new(params),
            link,
            view: PatchView::default(),
            bus,
            _keep: (m, other, mod_a, mod_b),
            t: 0.0,
        };
        for _ in 0..3 {
            h.frame(vec![]); // fonts install on the second pass; let layout settle
        }
        h
    }

    fn frame(&mut self, events: Vec<Event>) {
        self.t += 0.05;
        let raw = RawInput {
            screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(1064.0, 560.0))),
            time: Some(self.t),
            events,
            ..Default::default()
        };
        let (ctl, link, view) = (&self.ctl, &self.link, &mut self.view);
        let _ = self.ctx.run(raw, |ctx| editor::ui(ctx, ctl, link, view, Some(0)));
    }

    fn button(&mut self, x: f32, y: f32, pressed: bool) {
        self.frame(vec![Event::PointerButton {
            pos: pos2(x, y),
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        }]);
    }

    fn moved(&mut self, x: f32, y: f32) {
        self.frame(vec![Event::PointerMoved(pos2(x, y))]);
    }

    fn click(&mut self, x: f32, y: f32) {
        self.moved(x, y);
        self.button(x, y, true);
        self.button(x, y, false);
        self.frame(vec![]);
    }

    fn log(&self) -> Vec<String> {
        self.ctl.log.borrow().clone()
    }
}

#[test]
fn editor_binds_every_param_id() {
    let h = Harness::new();
    assert_eq!(*h.ctl.touched.borrow(), h.ctl.ids(), "editor must touch exactly the plugin's param ids");
    assert_eq!(h.ctl.ids().len(), 31);
}

#[test]
fn knob_labels_match_their_param() {
    let h = Harness::new();
    for (id, label) in KNOBS {
        let (name, label) = (h.ctl.name(id).to_lowercase(), label.trim_end_matches('.').to_lowercase());
        assert!(name.starts_with(&label), "knob {label:?} is bound to {id:?} ({name:?})");
    }
    for (i, id) in STEP_IDS.iter().enumerate() {
        assert_eq!(h.ctl.name(id), format!("Step {}", i + 1));
    }
}

#[test]
fn knob_drag_is_one_gesture_on_that_param() {
    let mut h = Harness::new();
    // Mix knob (6th of 7 in the Voice group).
    h.moved(516.0, 92.0);
    h.button(516.0, 92.0, true);
    h.moved(516.0, 70.0);
    h.moved(516.0, 50.0);
    h.button(516.0, 50.0, false);
    h.frame(vec![]);
    let log = h.log();
    assert_eq!(log.first().map(String::as_str), Some("begin mix"), "{log:?}");
    assert_eq!(log.last().map(String::as_str), Some("end mix"), "{log:?}");
    assert!(log.iter().all(|l| l.ends_with(" mix")), "{log:?}");
    assert_eq!(log.iter().filter(|l| l.starts_with("begin")).count(), 1);
    assert_eq!(log.iter().filter(|l| l.starts_with("end")).count(), 1);
}

#[test]
fn double_click_resets_in_one_gesture() {
    let mut h = Harness::new();
    h.ctl.set01("drive", 0.8); // knob 3
    h.frame(vec![]);
    h.click(365.0, 92.0);
    h.click(365.0, 92.0);
    let log = h.log();
    assert_eq!(&log[log.len() - 3..], ["begin drive", "set drive", "end drive"], "{log:?}");
    use vss::editor::Ctl;
    assert_eq!(h.ctl.get01("drive"), h.ctl.default01("drive"));
}

#[test]
fn grid_click_sets_that_step_to_that_row() {
    let mut h = Harness::new();
    use vss::editor::Ctl;
    // Column 2, row "E" (index 1); step_2 defaults to O (index 3).
    assert_eq!(h.ctl.enum_idx("step_2"), 3);
    h.click(192.0, 297.0);
    assert_eq!(h.log(), ["begin step_2", "set step_2", "end step_2"]);
    assert_eq!(h.ctl.enum_idx("step_2"), 1);
}

#[test]
fn steps_slider_snaps_to_whole_steps() {
    let mut h = Harness::new();
    use vss::editor::Ctl;
    h.click(680.0, 122.0);
    let log = h.log();
    assert_eq!(log.first().map(String::as_str), Some("begin steps"), "{log:?}");
    assert_eq!(log.last().map(String::as_str), Some("end steps"), "{log:?}");
    let v = h.ctl.get01("steps") * 15.0;
    assert!((v - v.round()).abs() < 1e-4, "steps not snapped: {v}");
    assert!(v.round() > 7.0, "click right of the default should raise steps: {v}");
}

#[test]
fn compact_readout_shortens_units() {
    assert_eq!(compact_readout("-12.00 st"), "-12.0 st");
    assert_eq!(compact_readout("20.0 ms"), "20 ms");
    assert_eq!(compact_readout("0.50"), "0.50");
}

// ---- PATCH strip (RR-BUS) ----------------------------------------------------------------
// Row centres y = 65 / 93 / 121; source combo x ~ 800, target combo x ~ 910, amount knob x ~ 977.
// An open combo lists its items below the combo, one 24 px row each.

const ROW_Y: [f32; 3] = [65.0, 93.0, 121.0];
const SRC_X: f32 = 800.0;
const TGT_X: f32 = 910.0;
const KNOB_X: f32 = 977.0;

/// Centre of item `k` of a combo whose centre is at `y`.
fn item_y(y: f32, k: usize) -> f32 {
    y + 12.0 + 6.0 + 24.0 * k as f32 + 12.0
}

#[test]
fn six_cv_params_are_bound_by_the_patch_strip() {
    let h = Harness::new();
    let touched = h.ctl.touched.borrow();
    for id in CV_TARGET_IDS.iter().chain(CV_AMOUNT_IDS.iter()) {
        assert!(touched.contains(*id), "{id} is not bound in the editor");
    }
}

#[test]
fn patch_target_pick_sets_that_cv_target() {
    let mut h = Harness::new();
    assert_eq!(h.ctl.enum_idx("cv2_target"), 0);
    h.click(TGT_X, ROW_Y[1]);
    h.click(TGT_X, item_y(ROW_Y[1], 4)); // Off, Formant, Reson., Drive, Glide, Mix
    assert_eq!(h.log(), ["begin cv2_target", "set cv2_target", "end cv2_target"]);
    assert_eq!(h.ctl.enum_idx("cv2_target"), 4);
    assert_eq!(h.ctl.enum_idx("cv1_target"), 0);
}

#[test]
fn patch_amount_drag_is_one_gesture_on_that_cv_amount() {
    let mut h = Harness::new();
    h.moved(KNOB_X, ROW_Y[2]);
    h.button(KNOB_X, ROW_Y[2], true);
    h.moved(KNOB_X, ROW_Y[2] - 15.0);
    h.moved(KNOB_X, ROW_Y[2] - 30.0);
    h.button(KNOB_X, ROW_Y[2] - 30.0, false);
    h.frame(vec![]);
    let log = h.log();
    assert_eq!(log.first().map(String::as_str), Some("begin cv3_amount"), "{log:?}");
    assert_eq!(log.last().map(String::as_str), Some("end cv3_amount"), "{log:?}");
    assert!(log.iter().all(|l| l.ends_with(" cv3_amount")), "{log:?}");
    assert!(h.ctl.get01("cv3_amount") > 0.5, "dragging up raises the bipolar amount");
}

#[test]
fn patch_source_pick_binds_and_persists() {
    let mut h = Harness::new();
    // Let the first refresh list the bus: GTS 1 Mod A, GTS 1 Mod B, then this device's three.
    let names: Vec<String> = h.bus.list().iter().map(|s| s.persisted()).collect();
    assert_eq!(names[..2], ["GTS 1 \u{b7} Mod A", "GTS 1 \u{b7} Mod B"]);
    assert!(names.contains(&"VSS 1 \u{b7} Step Gate".to_string()), "{names:?}");

    h.click(SRC_X, ROW_Y[0]);
    h.click(SRC_X, item_y(ROW_Y[0], 2)); // None, Mod A, Mod B, ...
    assert_eq!(h.link.persisted_sources()[0], "GTS 1 \u{b7} Mod B");
    assert!(h.link.is_bound(0));
    assert_eq!(h.link.persisted_sources()[1], "");

    // Back to None.
    h.click(SRC_X, ROW_Y[0]);
    h.click(SRC_X, item_y(ROW_Y[0], 0));
    assert_eq!(h.link.persisted_sources()[0], "");
    assert!(!h.link.is_bound(0));
}

#[test]
fn patch_label_edit_renames_the_instance() {
    let mut h = Harness::new();
    assert_eq!(h.link.label(), "VSS 1");
    h.click(950.0, 36.0);
    h.frame(vec![Event::Key {
        key: egui::Key::A,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::COMMAND,
    }]);
    h.frame(vec![Event::Text("Lead".into())]);
    let enter = |pressed| Event::Key { key: egui::Key::Enter, physical_key: None, pressed, repeat: false, modifiers: Modifiers::NONE };
    h.frame(vec![enter(true), enter(false)]); // the label commits on Enter/blur, not per keystroke
    h.frame(vec![]);
    assert_eq!(h.link.label(), "Lead");
    assert_eq!(h.link.persisted_sources(), [String::new(), String::new(), String::new()]);
    assert!(h.bus.list().iter().any(|s| s.persisted() == "Lead \u{b7} Step Gate"));
}
