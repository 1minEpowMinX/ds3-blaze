//! Drawing of the start screen.

use crate::flow::start::{Form, StartClick};
use crate::flow::step::{BUSY, Button, Click};
use crate::settings::Mode;
use crate::ui::copied::{self, COPIED};
use crate::ui::row;
use crate::ui::theme::{self, ACCENT, ORANGE, RED, STRONG, TEXT, WEAK};
use eframe::egui::{
    self, Align, CursorIcon, FontId, Label, Layout, Margin, RichText, Sense, Stroke, TextStyle,
    pos2, vec2,
};

/// Draws `form` and returns what a click asked for; `busy` disables
/// "Restore EA servers" while a restore runs.
pub fn show(form: &mut Form, ui: &mut egui::Ui, busy: bool) -> Option<StartClick> {
    let mut click = None;
    ui.add_space(24.0);
    ui.label(RichText::new("ds3-blaze").size(30.0).color(STRONG));
    ui.label(RichText::new("Dead Space 3 co-op over a VPN").color(WEAK));

    theme::section(ui, "You");
    let mut name_problem = None;
    let mut address_problem = None;
    theme::card().inner_margin(Margin::same(18)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        caption(ui, "Name in the game");
        ui.add(
            egui::TextEdit::singleline(&mut form.name)
                .desired_width(f32::INFINITY)
                .margin(vec2(10.0, 7.0)),
        );
        name_problem = form.name_problem();
        if let Some(problem) = &name_problem {
            ui.label(RichText::new(problem).color(RED));
        }
        ui.add_space(12.0);
        caption(ui, "VPN address");
        address_problem = address(form, ui, &mut click);
    });
    ui.add_space(6.0);

    theme::section(ui, "Play");
    let ready = name_problem.is_none() && address_problem.is_none();
    let spacing = 12.0;
    let width = (ui.available_width() - spacing) / 2.0;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = spacing;
        if card(ui, width, "Host a game", "Friends join your PC", ready) {
            click = Some(StartClick::Mode(Mode::Host));
        }
        if card(
            ui,
            width,
            "Join a friend",
            "Type the host's address next",
            ready,
        ) {
            click = Some(StartClick::Mode(Mode::Guest));
        }
    });

    if form.offers_restore() {
        ui.add_space(16.0);
        let restore = Button::elevated("Restore EA servers", Click::RestoreHosts)
            .disabled_by(busy.then_some(BUSY));
        theme::card()
            .fill(theme::SURFACE.lerp_to_gamma(ORANGE, 0.07))
            .stroke(Stroke::new(1.0, ORANGE.gamma_multiply(0.35)))
            .inner_margin(Margin::same(16))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                let (_, pressed) = egui::Sides::new().shrink_left().show(
                    ui,
                    |ui| {
                        ui.add(
                            Label::new(
                                RichText::new(
                                    "The game still goes to ds3-blaze, not to the EA servers",
                                )
                                .color(TEXT),
                            )
                            .wrap(),
                        );
                    },
                    |ui| row::action(ui, &restore, 32.0),
                );
                if pressed {
                    click = Some(StartClick::RestoreHosts);
                }
            });
    }
    click
}

/// Draws the version and the commit the window was built from, with a link
/// to the source code; a click on the commit copies its full hash.
pub fn about(ui: &mut egui::Ui) {
    let url = env!("CARGO_PKG_REPOSITORY");
    egui::Sides::new().show(
        ui,
        |ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            let version = format!("v{}", env!("CARGO_PKG_VERSION"));
            ui.label(RichText::new(version).small().color(WEAK));
            if !env!("DS3_BLAZE_COMMIT").is_empty() {
                ui.label(RichText::new("·").small().color(WEAK));
                commit(ui);
            }
        },
        |ui| {
            ui.hyperlink_to(RichText::new("GitHub ↗").small(), url)
                .on_hover_text(url);
        },
    );
}

/// Draws the short hash of the commit the window was built from, which
/// copies the full hash when clicked and says [`COPIED`] for a moment after.
fn commit(ui: &mut egui::Ui) {
    let full = env!("DS3_BLAZE_COMMIT");
    let id = egui::Id::unique("commit copied at");
    let size = TextStyle::Small.resolve(ui.style()).size;
    // egui hides a tooltip on a click, so the label itself says it copied.
    let hash = if copied::shown(ui, id) {
        RichText::new(COPIED).color(WEAK)
    } else {
        RichText::new(env!("DS3_BLAZE_COMMIT_SHORT")).color(WEAK.gamma_multiply(0.5))
    };
    let date = env!("DS3_BLAZE_COMMIT_DATE");
    let response = ui
        .add(
            Label::new(hash.font(FontId::monospace(size)))
                .selectable(false)
                .sense(Sense::click()),
        )
        .on_hover_cursor(CursorIcon::PointingHand)
        .on_hover_text(format!(
            "{full}
Committed {date}
Click to copy"
        ));
    if response.clicked() {
        ui.ctx().copy_text(full.to_owned());
        copied::mark(ui, id);
    }
}

/// Draws the chosen address with Change, or why none is chosen, and the
/// list while it is open; returns why no address is chosen and puts a
/// request to read the adapters again in `click`.
fn address(form: &mut Form, ui: &mut egui::Ui, click: &mut Option<StartClick>) -> Option<String> {
    let address_problem = form.address_problem();
    match (&address_problem, form.holder()) {
        (None, Some(holder)) => {
            let name = holder.name.clone();
            let address = form.address.expect("a holder holds the address");
            ui.horizontal(|ui| {
                ui.label(RichText::new(address.to_string()).size(16.0).color(STRONG));
                ui.label(RichText::new(name).color(WEAK));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let label = if form.choosing { "Done" } else { "Change" };
                    if theme::button(ui, label).clicked() {
                        form.choosing = !form.choosing;
                    }
                });
            });
        }
        (problem, _) => {
            ui.horizontal(|ui| {
                if let Some(problem) = problem {
                    ui.label(RichText::new(problem).color(ORANGE));
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if theme::button(ui, "Read adapters again").clicked() {
                        *click = Some(StartClick::ReadAdapters);
                    }
                });
            });
        }
    }
    if form.choosing && address_problem.is_none() {
        ui.add_space(6.0);
        address_list(form, ui, click);
    }
    if let Some(other) = form.other_vpn() {
        ui.add_space(6.0);
        ui.add(
            Label::new(
                RichText::new(format!(
                    "{} is up too and can take over the other player's traffic",
                    other.name
                ))
                .color(ORANGE),
            )
            .wrap(),
        );
    }
    address_problem
}

/// Draws the open list of addresses with the buttons that widen it and ask
/// for it to be read again.
fn address_list(form: &mut Form, ui: &mut egui::Ui, click: &mut Option<StartClick>) {
    let mut chosen = form.address;
    theme::inset(10).show(ui, |ui| {
        ui.set_width(ui.available_width());
        for (ip, adapter) in form.shown() {
            ui.horizontal(|ui| {
                ui.radio_value(&mut chosen, Some(ip), ip.to_string());
                ui.label(RichText::new(&adapter.name).color(WEAK));
            });
        }
        ui.horizontal(|ui| {
            if !form.all && theme::button(ui, "Show all adapters").clicked() {
                form.all = true;
            }
            if theme::button(ui, "Read adapters again").clicked() {
                *click = Some(StartClick::ReadAdapters);
            }
        });
    });
    form.address = chosen;
}

/// Draws the caption over a field.
fn caption(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text).small().color(WEAK));
    ui.add_space(2.0);
}

/// Draws a mode's card `width` points wide and returns whether it was
/// clicked; a card that is not `enabled` is drawn faded and takes no click.
fn card(ui: &mut egui::Ui, width: f32, title: &str, subtitle: &str, enabled: bool) -> bool {
    ui.add_enabled_ui(enabled, |ui| {
        let id = egui::Id::unique(("mode card", title));
        let hovered = enabled && ui.ctx().read_response(id).is_some_and(|r| r.hovered());
        let frame = theme::card()
            .fill(if hovered {
                theme::SURFACE.lerp_to_gamma(ACCENT, 0.06)
            } else {
                theme::SURFACE
            })
            .stroke(Stroke::new(1.0, if hovered { ACCENT } else { theme::LINE }))
            .inner_margin(Margin::same(20))
            .show(ui, |ui| {
                // The margins and the border take the rest of `width`.
                ui.set_width(width - 42.0);
                ui.set_min_height(64.0);
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        let title = RichText::new(title).size(18.0).color(STRONG);
                        ui.add(Label::new(title).selectable(false));
                        ui.add_space(2.0);
                        ui.add(
                            Label::new(RichText::new(subtitle).color(WEAK))
                                .selectable(false)
                                .wrap(),
                        );
                    });
                });
            });
        let rect = frame.response.rect;
        let arrow = pos2(rect.right() - 26.0, rect.center().y);
        let colour = if hovered { ACCENT } else { WEAK };
        ui.painter().line(
            vec![
                arrow + vec2(-4.0, -5.0),
                arrow + vec2(1.0, 0.0),
                arrow + vec2(-4.0, 5.0),
            ],
            Stroke::new(2.0, colour),
        );
        let response = ui.interact(rect, id, Sense::click());
        // Screen readers and UI Automation see the card as a button.
        response
            .widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, ui.is_enabled(), title));
        response.on_hover_cursor(CursorIcon::PointingHand).clicked()
    })
    .inner
}
