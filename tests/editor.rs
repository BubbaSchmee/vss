//! Editor binding tests: run the real editor function headlessly over `MapCtl` and check that
//! every parameter id is bound, knobs sit on the right params, and gestures reach the right id
//! with begin / set / end pairing.

use std::sync::Arc;

use nih_plug_egui::egui::{self, pos2, vec2, Event, Modifiers, PointerButton, RawInput, Rect};
use vss::editor::{self, compact_readout, MapCtl, KNOBS, STEP_IDS};
use vss::params::VssParams;

struct Harness {
    ctx: egui::Context,
    ctl: MapCtl,
    t: f64,
}

impl Harness {
    fn new() -> Self {
        let ctx = egui::Context::default();
        rr_ui::theme::apply(&ctx);
        let mut h = Self { ctx, ctl: MapCtl::new(Arc::new(VssParams::default())), t: 0.0 };
        for _ in 0..3 {
            h.frame(vec![]); // fonts install on the second pass; let layout settle
        }
        h
    }

    fn frame(&mut self, events: Vec<Event>) {
        self.t += 0.05;
        let raw = RawInput {
            screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(760.0, 560.0))),
            time: Some(self.t),
            events,
            ..Default::default()
        };
        let ctl = &self.ctl;
        let _ = self.ctx.run(raw, |ctx| editor::ui(ctx, ctl, Some(0)));
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
    assert_eq!(h.ctl.ids().len(), 25);
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
    h.click(166.0, 305.0);
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
