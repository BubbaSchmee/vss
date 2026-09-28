//! egui editor. Stub: returns no editor yet. A later agent builds the GUI described in
//! SPEC.md "GUI" (760x380 fixed window, dark theme, top-row sliders, 16x6 vowel grid).
//! `nih_plug_egui` stays a dependency so it's compiled and cached now.

use nih_plug::prelude::*;
use std::sync::atomic::AtomicUsize;
use std::sync::Arc;

use crate::params::VssParams;

// TODO(editor): build the egui editor per SPEC.md "GUI" — ParamSlider row, 16x6 vowel grid
// driven by `current_step`, dark theme, fixed 760x380 window. Until then, no editor is shown.
pub fn create(_params: Arc<VssParams>, _current_step: Arc<AtomicUsize>) -> Option<Box<dyn Editor>> {
    None
}
