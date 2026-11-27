//! Drawing of a mode's banner.

use crate::flow::banner::{Banner, Tone};
use crate::flow::step::{Button, Click};
use crate::ui::row;
use crate::ui::theme::{self, ACCENT, GREEN, ORANGE, RED, STRONG, TEXT};
use eframe::egui::{self, Color32, Label, Margin, RichText, pos2};

/// Width of the bar down a banner's left side, in points.
const STRIPE: f32 = 4.0;

/// What the button that copies the address says for a moment after a click.
const COPIED: &str = "Copied";

/// How long the button says [`COPIED`], in seconds.
const COPIED_FOR: f64 = 1.5;

/// Draws `banner` across the width and returns the click its button gave;
/// `input` draws the guest's address field under the text when the banner
/// holds it.
pub fn show(
    ui: &mut egui::Ui,
    banner: &Banner,
    input: impl FnOnce(&mut egui::Ui),
) -> Option<Click> {
    let colour = tone_colour(banner.tone);
    let mut click = None;
    let frame = theme::card()
        .fill(theme::SURFACE.lerp_to_gamma(colour, 0.07))
        .inner_margin(Margin {
            left: 22,
            right: 18,
            top: 18,
            bottom: 18,
        })
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            let (_, button) = egui::Sides::new().shrink_left().show(
                ui,
                |ui| {
                    ui.vertical(|ui| {
                        let text = RichText::new(&banner.text).size(20.0).color(STRONG);
                        ui.add(Label::new(text).wrap());
                        if let Some(detail) = &banner.detail {
                            ui.add_space(2.0);
                            ui.add(Label::new(RichText::new(detail).color(TEXT)).wrap());
                        }
                        if banner.input {
                            ui.add_space(10.0);
                            input(ui);
                        }
                    });
                },
                |ui| {
                    if banner.spinner {
                        ui.add(egui::Spinner::new().size(22.0).color(ACCENT));
                        return None;
                    }
                    let action = banner.action.as_ref()?;
                    action_button(ui, action).then(|| action.click.clone())
                },
            );
            click = button;
        });
    let rect = frame.response.rect;
    let stripe = egui::Rect::from_min_max(
        pos2(rect.left() + 9.0, rect.top() + 16.0),
        pos2(rect.left() + 9.0 + STRIPE, rect.bottom() - 16.0),
    );
    ui.painter().rect_filled(stripe, 2, colour);
    click
}

/// Draws a banner's button and returns whether it was pressed; the button
/// that copies the address says [`COPIED`] for [`COPIED_FOR`] after a click.
fn action_button(ui: &mut egui::Ui, action: &Button) -> bool {
    if action.click != Click::CopyAddress {
        return row::main_action(ui, action, 38.0);
    }
    let id = egui::Id::unique("address copied at");
    let now = ui.input(|input| input.time);
    let left = ui
        .data(|data| data.get_temp::<f64>(id))
        .map_or(0.0, |at| COPIED_FOR - (now - at));
    let label = if left > 0.0 { COPIED } else { action.label };
    let pressed = row::main_action_saying(ui, action, label, 38.0);
    if pressed {
        ui.data_mut(|data| data.insert_temp(id, now));
        ui.ctx().request_repaint_after_secs(COPIED_FOR as f32);
    } else if left > 0.0 {
        // Nothing else may repaint the window when the label goes back.
        ui.ctx().request_repaint_after_secs(left as f32);
    }
    pressed
}

/// Returns the colour of a banner's bar and tint.
fn tone_colour(tone: Tone) -> Color32 {
    match tone {
        Tone::Neutral => ACCENT,
        Tone::Success => GREEN,
        Tone::Warning => ORANGE,
        Tone::Failure => RED,
    }
}
