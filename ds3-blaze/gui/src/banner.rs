//! The sentence on top of a mode's screen: where things stand and the one
//! thing to do next.

use crate::row;
use crate::step::{Button, Click};
use crate::theme::{self, ACCENT, GREEN, ORANGE, RED, STRONG, TEXT};
use eframe::egui::{self, Color32, Label, Margin, RichText, pos2};

/// How a banner is coloured.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    Neutral,
    Success,
    Warning,
    Failure,
}

/// The sentence on top of a mode's screen and its one action.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Banner {
    pub tone: Tone,
    pub text: String,
    /// The second line, `None` when one line says it all.
    pub detail: Option<String>,
    /// The main button; `None` while a step runs or when nothing is to do.
    pub action: Option<Button>,
    /// Whether a spinner stands in for the button.
    pub spinner: bool,
    /// Whether the banner holds the guest's address field.
    pub input: bool,
}

impl Banner {
    /// Returns a banner of `tone` saying `text`, with no detail, action,
    /// spinner or input.
    pub fn new(tone: Tone, text: impl Into<String>) -> Banner {
        Banner {
            tone,
            text: text.into(),
            detail: None,
            action: None,
            spinner: false,
            input: false,
        }
    }

    /// Returns the banner with `detail` as its second line.
    pub fn detail(self, detail: impl Into<String>) -> Banner {
        Banner {
            detail: Some(detail.into()),
            ..self
        }
    }

    /// Returns the banner with `button` as its action.
    pub fn action(self, button: Button) -> Banner {
        Banner {
            action: Some(button),
            ..self
        }
    }

    /// Returns the banner with a spinner in place of an action.
    pub fn spinning(self) -> Banner {
        Banner {
            spinner: true,
            action: None,
            ..self
        }
    }

    /// Returns the banner holding the guest's address field.
    pub fn with_input(self) -> Banner {
        Banner {
            input: true,
            ..self
        }
    }
}

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

/// Returns how many things to fix `count` names, in the player's words.
pub fn things_to_fix(count: usize) -> String {
    match count {
        1 => "1 thing to fix on this PC".into(),
        count => format!("{count} things to fix on this PC"),
    }
}
