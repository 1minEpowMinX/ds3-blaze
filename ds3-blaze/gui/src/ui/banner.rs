//! Drawing of a mode's banner.

use crate::flow::banner::{Banner, Tone};
use crate::flow::step::Click;
use crate::ui::row;
use crate::ui::theme::{self, ACCENT, GREEN, ORANGE, RED, STRONG, TEXT};
use eframe::egui::{self, Color32, Label, Margin, RichText, pos2};

/// Width of the bar down a banner's left side, in points.
const STRIPE: f32 = 4.0;

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
                    row::main_action(ui, action, 38.0).then(|| action.click.clone())
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

/// Returns the colour of a banner's bar and tint.
fn tone_colour(tone: Tone) -> Color32 {
    match tone {
        Tone::Neutral => ACCENT,
        Tone::Success => GREEN,
        Tone::Warning => ORANGE,
        Tone::Failure => RED,
    }
}
