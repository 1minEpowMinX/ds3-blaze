//! The sentence on top of a mode's screen: where things stand and the one
//! thing to do next.

use crate::row::{self, GREEN, ORANGE, RED};
use crate::step::{Button, Click};
use eframe::egui::{self, Label, Margin, RichText, Stroke};

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

/// Draws `banner` across the width and returns the click its button gave;
/// `input` draws the guest's address field under the text when the banner
/// holds it.
pub fn show(
    ui: &mut egui::Ui,
    banner: &Banner,
    input: impl FnOnce(&mut egui::Ui),
) -> Option<Click> {
    let background = ui.visuals().faint_bg_color;
    let (fill, stroke) = match banner.tone {
        Tone::Neutral => (background, ui.visuals().widgets.noninteractive.bg_stroke),
        Tone::Success => tinted(background, GREEN),
        Tone::Warning => tinted(background, ORANGE),
        Tone::Failure => tinted(background, RED),
    };
    let mut click = None;
    egui::Frame::new()
        .fill(fill)
        .stroke(stroke)
        .corner_radius(10.0)
        .inner_margin(Margin::symmetric(18, 16))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            let (_, button) = egui::Sides::new().shrink_left().show(
                ui,
                |ui| {
                    ui.vertical(|ui| {
                        let text = RichText::new(&banner.text)
                            .size(18.0)
                            .color(ui.visuals().strong_text_color());
                        ui.add(Label::new(text).wrap());
                        if let Some(detail) = &banner.detail {
                            ui.add(Label::new(RichText::new(detail).weak()).wrap());
                        }
                        if banner.input {
                            ui.add_space(6.0);
                            input(ui);
                        }
                    });
                },
                |ui| {
                    if banner.spinner {
                        ui.add(egui::Spinner::new().size(22.0));
                        return None;
                    }
                    let action = banner.action.as_ref()?;
                    row::action(ui, action, 36.0).then(|| action.click.clone())
                },
            );
            click = button;
        });
    click
}

/// Returns the fill and the border of a banner coloured `colour`.
fn tinted(background: egui::Color32, colour: egui::Color32) -> (egui::Color32, Stroke) {
    (
        background.lerp_to_gamma(colour, 0.14),
        Stroke::new(1.0, colour.gamma_multiply(0.6)),
    )
}

/// Returns how many things to fix `count` names, in the player's words.
pub fn things_to_fix(count: usize) -> String {
    match count {
        1 => "1 thing to fix on this PC".into(),
        count => format!("{count} things to fix on this PC"),
    }
}
