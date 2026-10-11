//! egui editor per SPEC.md "GUI": a 760x560 rack panel drawn with the shared `rr-ui` kit.
//!
//! The whole UI is the function [`ui`] over the small [`Ctl`] trait, so the real editor can be
//! rendered and tested outside a host (`examples/editor_preview.rs`, `tests/editor.rs`). The
//! plugin implements [`Ctl`] over its params + `ParamSetter` ([`PluginCtl`]); [`MapCtl`] is the
//! host-free implementation over a plain value map.

use std::cell::RefCell;
use std::collections::{BTreeSet, HashMap};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use nih_plug::prelude::*;
use nih_plug_egui::{create_egui_editor, egui, EguiState};
use rr_ui::theme::metrics::{GAP, GROUP_CHROME_H, KNOB_D_LG};
use rr_ui::widgets::{
    caption, combo_stacked, group_ext, hslider_resp, knob_row, knob_styled, step_grid_ex, KnobResponse, KnobStyle,
};

use crate::params::VssParams;

pub const WINDOW: (u32, u32) = (760, 560);
const RATES: [&str; 5] = ["1/4", "1/8", "1/8T", "1/16", "1/16T"];
const VOWELS: [&str; 6] = ["A", "E", "I", "O", "U", "Hold"];
const MAX_STEPS: usize = 16;

/// Voice knobs left to right: (param id, label). Formant Shift is the only bipolar one.
pub const KNOBS: [(&str, &str); 7] = [
    ("formant_shift", "Formant"),
    ("resonance", "Reson."),
    ("drive", "Drive"),
    ("glide", "Glide"),
    ("swing", "Swing"),
    ("mix", "Mix"),
    ("output_gain", "Output"),
];
pub const STEP_IDS: [&str; MAX_STEPS] = [
    "step_1", "step_2", "step_3", "step_4", "step_5", "step_6", "step_7", "step_8", "step_9", "step_10",
    "step_11", "step_12", "step_13", "step_14", "step_15", "step_16",
];

/// What the editor needs from a parameter backend. Values are normalised 0..1; enum params are
/// addressed by variant index. `begin`/`set*`/`end` follow the host automation gesture rules.
pub trait Ctl {
    fn get01(&self, id: &str) -> f32;
    fn default01(&self, id: &str) -> f32;
    fn readout(&self, id: &str) -> String;
    fn enum_idx(&self, id: &str) -> usize;
    fn begin(&self, id: &str);
    fn set01(&self, id: &str, v: f32);
    fn set_enum(&self, id: &str, i: usize);
    fn end(&self, id: &str);
}

type PtrMap = HashMap<String, ParamPtr>;

fn ptr_map(params: &VssParams) -> PtrMap {
    params.param_map().into_iter().map(|(id, p, _)| (id, p)).collect()
}

fn ptr(map: &PtrMap, id: &str) -> ParamPtr {
    *map.get(id).unwrap_or_else(|| panic!("editor used unknown param id {id:?}"))
}

fn enum_to01(p: ParamPtr, i: usize) -> f32 {
    let n = unsafe { p.step_count() }.unwrap_or(1).max(1);
    i.min(n) as f32 / n as f32
}

fn to_enum_idx(p: ParamPtr, v01: f32) -> usize {
    let n = unsafe { p.step_count() }.unwrap_or(1).max(1);
    (v01 * n as f32).round() as usize
}

/// [`Ctl`] over the live params and the host's `ParamSetter`.
pub struct PluginCtl<'a> {
    pub map: &'a PtrMap,
    pub setter: &'a ParamSetter<'a>,
}

impl Ctl for PluginCtl<'_> {
    fn get01(&self, id: &str) -> f32 {
        unsafe { ptr(self.map, id).unmodulated_normalized_value() }
    }
    fn default01(&self, id: &str) -> f32 {
        unsafe { ptr(self.map, id).default_normalized_value() }
    }
    fn readout(&self, id: &str) -> String {
        unsafe { ptr(self.map, id).normalized_value_to_string(self.get01(id), true) }
    }
    fn enum_idx(&self, id: &str) -> usize {
        to_enum_idx(ptr(self.map, id), self.get01(id))
    }
    fn begin(&self, id: &str) {
        unsafe { self.setter.raw_context.raw_begin_set_parameter(ptr(self.map, id)) }
    }
    fn set01(&self, id: &str, v: f32) {
        unsafe { self.setter.raw_context.raw_set_parameter_normalized(ptr(self.map, id), v) }
    }
    fn set_enum(&self, id: &str, i: usize) {
        self.set01(id, enum_to01(ptr(self.map, id), i));
    }
    fn end(&self, id: &str) {
        unsafe { self.setter.raw_context.raw_end_set_parameter(ptr(self.map, id)) }
    }
}

/// Host-free [`Ctl`] over a map of normalised values (initialised to the param defaults). Also
/// records every id the editor reads and every gesture call, so tests can prove the editor binds
/// every parameter.
pub struct MapCtl {
    _params: Arc<VssParams>, // keeps the ParamPtrs in `map` valid
    map: PtrMap,
    vals: RefCell<HashMap<String, f32>>,
    pub touched: RefCell<BTreeSet<String>>,
    pub log: RefCell<Vec<String>>,
}

impl MapCtl {
    pub fn new(params: Arc<VssParams>) -> Self {
        let map = ptr_map(&params);
        let vals = map.iter().map(|(k, p)| (k.clone(), unsafe { p.default_normalized_value() })).collect();
        Self { _params: params, map, vals: RefCell::new(vals), touched: Default::default(), log: Default::default() }
    }
    pub fn ids(&self) -> BTreeSet<String> {
        self.map.keys().cloned().collect()
    }
    /// The parameter's display name, e.g. "Formant Shift".
    pub fn name(&self, id: &str) -> String {
        unsafe { ptr(&self.map, id).name() }.to_string()
    }
}

impl Ctl for MapCtl {
    fn get01(&self, id: &str) -> f32 {
        self.touched.borrow_mut().insert(id.into());
        self.vals.borrow()[id]
    }
    fn default01(&self, id: &str) -> f32 {
        unsafe { ptr(&self.map, id).default_normalized_value() }
    }
    fn readout(&self, id: &str) -> String {
        unsafe { ptr(&self.map, id).normalized_value_to_string(self.get01(id), true) }
    }
    fn enum_idx(&self, id: &str) -> usize {
        to_enum_idx(ptr(&self.map, id), self.get01(id))
    }
    fn begin(&self, id: &str) {
        self.log.borrow_mut().push(format!("begin {id}"));
    }
    fn set01(&self, id: &str, v: f32) {
        self.log.borrow_mut().push(format!("set {id}"));
        self.vals.borrow_mut().insert(id.into(), v);
    }
    fn set_enum(&self, id: &str, i: usize) {
        self.set01(id, enum_to01(ptr(&self.map, id), i));
    }
    fn end(&self, id: &str) {
        self.log.borrow_mut().push(format!("end {id}"));
    }
}

/// Maps a widget's gesture onto begin / set / end. The gesture stays open from the first change
/// (or drag start) until the drag ends or the primary button is up, so a plain click on a slider
/// is still a complete begin/set/end. A reset (double click) is its own begin/set/end.
fn drive(ui: &egui::Ui, ctl: &dyn Ctl, id: &str, r: KnobResponse, snap: impl Fn(f32) -> f32) {
    let key = egui::Id::new(("vss_gesture", id));
    let mut open = ui.data(|d| d.get_temp::<bool>(key)).unwrap_or(false);
    if r.reset {
        if let Some(v) = r.changed {
            ctl.begin(id);
            ctl.set01(id, snap(v));
            ctl.end(id);
        }
        return;
    }
    if (r.drag_started || r.changed.is_some()) && !open {
        ctl.begin(id);
        open = true;
    }
    if let Some(v) = r.changed {
        ctl.set01(id, snap(v));
    }
    if open && (r.drag_ended || !ui.input(|i| i.pointer.primary_down())) {
        ctl.end(id);
        open = false;
    }
    ui.data_mut(|d| d.insert_temp(key, open));
}

/// Shortens a param readout to fit a 50 px knob cell: values with a unit lose a decimal
/// ("-12.00 st" -> "-12.0 st", "20.0 ms" -> "20 ms"); unitless ones ("0.50") pass through.
pub fn compact_readout(s: &str) -> String {
    match s.split_once(' ') {
        Some((num, unit)) => match num.parse::<f32>() {
            Ok(v) if unit == "ms" => format!("{v:.0} {unit}"),
            Ok(v) => format!("{v:.1} {unit}"),
            Err(_) => s.to_string(),
        },
        None => s.to_string(),
    }
}

fn voice(ui: &mut egui::Ui, ctl: &dyn Ctl) {
    let lg = KnobStyle::default().size(KNOB_D_LG).cell(50.0);
    knob_row(ui, |ui| {
        for (id, label) in KNOBS {
            let style = if id == "formant_shift" { lg.bipolar() } else { lg };
            let r = knob_styled(ui, label, ctl.get01(id), ctl.default01(id), &compact_readout(&ctl.readout(id)), style);
            drive(ui, ctl, id, r, |v| v);
        }
    });
}

fn sequence(ui: &mut egui::Ui, ctl: &dyn Ctl) {
    ui.spacing_mut().item_spacing.y = 4.0;
    if let Some(i) = combo_stacked(ui, "Rate", ctl.enum_idx("rate"), &RATES) {
        ctl.begin("rate");
        ctl.set_enum("rate", i);
        ctl.end("rate");
    }
    let r = hslider_resp(ui, 76.0, "Steps", ctl.get01("steps"), &ctl.readout("steps"));
    // Quantise to whole steps; the snapped value is what the param (and so the slider) sees.
    drive(ui, ctl, "steps", r, |v| (v * (MAX_STEPS - 1) as f32).round() / (MAX_STEPS - 1) as f32);
}

fn pattern(ui: &mut egui::Ui, ctl: &dyn Ctl, playhead: Option<usize>, body_h: f32) {
    let steps = 1 + (ctl.get01("steps") * (MAX_STEPS - 1) as f32).round() as usize;
    let cells: [u8; MAX_STEPS] = std::array::from_fn(|c| ctl.enum_idx(STEP_IDS[c]) as u8);
    let cur = playhead.map(|s| s.min(MAX_STEPS - 1));
    let cap = cur.map_or(String::new(), |c| format!("Step {} \u{b7} {}", c + 1, VOWELS[cells[c] as usize]));
    group_ext(ui, egui::vec2(672.0, body_h), "Vowel pattern", |ui| caption(ui, &cap), |ui| {
        let h = body_h - GROUP_CHROME_H;
        if let Some((c, r)) = step_grid_ex(ui, &VOWELS, MAX_STEPS, steps, cur, &cells, Some(5), Some(h)) {
            ctl.begin(STEP_IDS[c]);
            ctl.set_enum(STEP_IDS[c], r);
            ctl.end(STEP_IDS[c]);
        }
    });
}

/// The complete editor UI. `playhead` is the sequencer's current step (it free-runs when the
/// transport is stopped, so it is always meaningful).
pub fn ui(ctx: &egui::Context, ctl: &dyn Ctl, playhead: Option<usize>) {
    egui::CentralPanel::default().frame(egui::Frame::NONE).show(ctx, |ui| {
        let status = format!("16-step formant \u{b7} v{}", env!("CARGO_PKG_VERSION"));
        rr_ui::panel::rack_panel(ui, "VSS", "Vowel Step Sequencer", &status, egui::vec2(168.0, 124.0), |hui, bui| {
            hui.spacing_mut().item_spacing = egui::vec2(GAP, GAP);
            hui.horizontal_top(|ui| {
                group_ext(ui, egui::vec2(376.0, 124.0), "Voice", |_| {}, |ui| voice(ui, ctl));
                group_ext(ui, egui::vec2(100.0, 124.0), "Sequence", |_| {}, |ui| sequence(ui, ctl));
            });
            let body_h = bui.max_rect().height();
            pattern(bui, ctl, playhead, body_h);
        });
    });
}

pub fn create(params: Arc<VssParams>, current_step: Arc<AtomicUsize>) -> Option<Box<dyn Editor>> {
    let map = ptr_map(&params);
    create_egui_editor(
        EguiState::from_size(WINDOW.0, WINDOW.1),
        (),
        |ctx, _| rr_ui::theme::apply(ctx),
        move |ctx, setter, _state| {
            // `params` is held here so the ParamPtrs in `map` stay valid for the editor's lifetime.
            let _keep = &params;
            ui(ctx, &PluginCtl { map: &map, setter }, Some(current_step.load(Ordering::Relaxed)));
            ctx.request_repaint_after(std::time::Duration::from_millis(33));
        },
    )
}
