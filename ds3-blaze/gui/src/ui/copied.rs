//! Feedback that a control has just copied something to the clipboard.

use eframe::egui;

/// What a control says for a moment after it copies.
pub const COPIED: &str = "Copied";

/// How long a control says [`COPIED`], in seconds.
const COPIED_FOR: f64 = 1.5;

/// Records under `id` that the control copied just now.
pub fn mark(ui: &egui::Ui, id: egui::Id) {
    let now = ui.input(|input| input.time);
    ui.data_mut(|data| data.insert_temp(id, now));
    ui.ctx().request_repaint_after_secs(COPIED_FOR as f32);
}

/// Returns whether the copy recorded under `id` is less than [`COPIED_FOR`]
/// old.
pub fn shown(ui: &egui::Ui, id: egui::Id) -> bool {
    let now = ui.input(|input| input.time);
    let left = ui
        .data(|data| data.get_temp::<f64>(id))
        .map_or(0.0, |at| COPIED_FOR - (now - at));
    if left > 0.0 {
        // Nothing else may repaint the window when the moment ends.
        ui.ctx().request_repaint_after_secs(left as f32);
    }
    left > 0.0
}
