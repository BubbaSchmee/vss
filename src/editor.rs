//! egui editor per SPEC.md "GUI": fixed 760x380 dark window, a top row of parameter sliders
//! plus a Rate combo box, and a 16x6 step grid (columns = steps, rows = A E I O U Hold) driven
//! by the shared `current_step` atomic.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use nih_plug::prelude::*;
use nih_plug_egui::{create_egui_editor, egui, widgets::ParamSlider, EguiState};

use crate::params::{Rate, VssParams, Vowel};

const RATES: [(Rate, &str); 5] = [
    (Rate::Quarter, "1/4"),
    (Rate::Eighth, "1/8"),
    (Rate::EighthTriplet, "1/8T"),
    (Rate::Sixteenth, "1/16"),
    (Rate::SixteenthTriplet, "1/16T"),
];

const VOWEL_ROWS: [(Vowel, &str); 6] = [
    (Vowel::A, "A"),
    (Vowel::E, "E"),
    (Vowel::I, "I"),
    (Vowel::O, "O"),
    (Vowel::U, "U"),
    (Vowel::Hold, "Hold"),
];

/// Builds the editor. `EguiState` can't be persisted on `VssParams` (that struct is fixed
/// contract owned by another agent), so it's just a fresh local size here — no window state
/// to restore across sessions, which is fine for a fixed 760x380 window.
pub fn create(params: Arc<VssParams>, current_step: Arc<AtomicUsize>) -> Option<Box<dyn Editor>> {
    let egui_state = EguiState::from_size(760, 380);

    create_egui_editor(
        egui_state,
        (),
        |ctx, _| ctx.set_visuals(egui::Visuals::dark()),
        move |ctx, setter, _state| {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.heading(format!(
                    "VSS — Vowel Step Sequencer  v{}",
                    env!("CARGO_PKG_VERSION")
                ));
                ui.separator();

                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.label("Formant Shift");
                        ui.add(ParamSlider::for_param(&params.formant_shift, setter));
                    });
                    ui.vertical(|ui| {
                        ui.label("Resonance");
                        ui.add(ParamSlider::for_param(&params.resonance, setter));
                    });
                    ui.vertical(|ui| {
                        ui.label("Drive");
                        ui.add(ParamSlider::for_param(&params.drive, setter));
                    });
                    ui.vertical(|ui| {
                        ui.label("Glide");
                        ui.add(ParamSlider::for_param(&params.glide, setter));
                    });
                    ui.vertical(|ui| {
                        ui.label("Swing");
                        ui.add(ParamSlider::for_param(&params.swing, setter));
                    });
                    ui.vertical(|ui| {
                        ui.label("Mix");
                        ui.add(ParamSlider::for_param(&params.mix, setter));
                    });
                    ui.vertical(|ui| {
                        ui.label("Output");
                        ui.add(ParamSlider::for_param(&params.output_gain, setter));
                    });
                    ui.vertical(|ui| {
                        ui.label("Steps");
                        ui.add(ParamSlider::for_param(&params.steps, setter));
                    });
                    ui.vertical(|ui| {
                        ui.label("Rate");
                        let current_rate = params.rate.value();
                        let current_label = RATES
                            .iter()
                            .find(|(rate, _)| *rate == current_rate)
                            .map(|(_, label)| *label)
                            .unwrap_or("");
                        egui::ComboBox::from_id_salt("rate_combo")
                            .selected_text(current_label)
                            .show_ui(ui, |ui| {
                                for (rate, label) in RATES {
                                    if ui.selectable_label(current_rate == rate, label).clicked() {
                                        setter.begin_set_parameter(&params.rate);
                                        setter.set_parameter(&params.rate, rate);
                                        setter.end_set_parameter(&params.rate);
                                    }
                                }
                            });
                    });
                });

                ui.separator();

                let steps = params.steps.value() as usize;
                let step = current_step.load(Ordering::Relaxed);
                let step_params = params.steps_array();

                egui::Grid::new("step_grid")
                    .spacing([3.0, 3.0])
                    .show(ui, |ui| {
                        for (vowel, label) in VOWEL_ROWS {
                            for (col, param) in step_params.iter().enumerate() {
                                let selected = param.value() == vowel;
                                let dimmed = col >= steps;
                                let is_current_col = col == step && !dimmed;

                                let text = if dimmed {
                                    egui::RichText::new(label).weak()
                                } else {
                                    egui::RichText::new(label)
                                };

                                let response =
                                    ui.add(egui::SelectableLabel::new(selected, text));
                                if is_current_col {
                                    ui.painter().rect_stroke(
                                        response.rect,
                                        2.0,
                                        egui::Stroke::new(2.0_f32, egui::Color32::YELLOW),
                                        egui::StrokeKind::Outside,
                                    );
                                }
                                if response.clicked() {
                                    setter.begin_set_parameter(*param);
                                    setter.set_parameter(*param, vowel);
                                    setter.end_set_parameter(*param);
                                }
                            }
                            ui.end_row();
                        }
                    });
            });

            ctx.request_repaint_after(std::time::Duration::from_millis(33));
        },
    )
}
