//! The row a step is drawn as: one line with a marker, the title, the
//! summary, the buttons, a status pill and a chevron, and the details below
//! it while the row is open.

use crate::step::{Click, Detail, PlayerRow, Status, Step};
use ds3_blaze_diag::netcheck::{Finding, Severity};
use eframe::egui::{self, Align, Color32, Label, Layout, RichText, Sense};

/// Returns the colour a status is drawn in.
fn status_colour(status: Status) -> Color32 {
    match status {
        Status::Waiting => Color32::GRAY,
        Status::Running => Color32::from_rgb(90, 150, 230),
        Status::Ok => Color32::from_rgb(80, 180, 100),
        Status::Warning => Color32::from_rgb(220, 170, 50),
        Status::Failed => Color32::from_rgb(220, 80, 70),
    }
}

/// Returns the colour and the mark a finding is drawn with.
fn severity_look(severity: Severity) -> (Color32, &'static str) {
    match severity {
        Severity::Ok => (status_colour(Status::Ok), "ok"),
        Severity::Note => (Color32::GRAY, "note"),
        Severity::Warning => (status_colour(Status::Warning), "warning"),
        Severity::Problem => (status_colour(Status::Failed), "problem"),
    }
}

/// Draws `step` as the row numbered `number` and returns the click one of its
/// buttons or fields gave.
///
/// A click on the marker, the title, the summary, the pill or the chevron
/// toggles `open`; the details show below the line while `open` is true.
pub fn show(ui: &mut egui::Ui, number: usize, step: &Step, open: &mut bool) -> Option<Click> {
    let mut click = None;
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.set_width(ui.available_width());
        let colour = status_colour(step.status);
        let toggled = ui
            .horizontal(|ui| {
                let toggle =
                    |text: RichText| Label::new(text).sense(Sense::click()).selectable(false);
                let mut toggled = false;
                toggled |= ui
                    .add(toggle(
                        RichText::new(number.to_string()).color(colour).strong(),
                    ))
                    .clicked();
                toggled |= ui.add(toggle(RichText::new(step.title).strong())).clicked();
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    toggled |= ui
                        .add(toggle(RichText::new(if *open { "▾" } else { "▸" })))
                        .clicked();
                    toggled |= ui
                        .add(toggle(RichText::new(step.status.label()).color(colour)))
                        .clicked();
                    for button in step.buttons.iter().rev() {
                        if ui
                            .add_enabled(button.enabled, egui::Button::new(button.label))
                            .clicked()
                        {
                            click = Some(button.click.clone());
                        }
                    }
                    ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                        toggled |= ui
                            .add(toggle(RichText::new(&step.summary)).truncate())
                            .clicked();
                    });
                });
                toggled
            })
            .inner;
        if toggled {
            *open = !*open;
        }
        if *open && !step.details.is_empty() {
            ui.separator();
            for detail in &step.details {
                if let Some(detail_click) = show_detail(ui, detail) {
                    click = Some(detail_click);
                }
            }
        }
    });
    click
}

/// Draws one line of the details and returns the click it gave.
fn show_detail(ui: &mut egui::Ui, detail: &Detail) -> Option<Click> {
    match detail {
        Detail::Text(text) => {
            ui.label(text);
            None
        }
        Detail::Finding(finding) => show_finding(ui, finding),
        Detail::Player(player) => show_player(ui, player),
    }
}

/// Draws a finding with its fix button.
fn show_finding(ui: &mut egui::Ui, finding: &Finding) -> Option<Click> {
    let (colour, mark) = severity_look(finding.severity);
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new(mark).color(colour));
        ui.label(&finding.text);
        let fix = finding.fix.as_ref()?;
        ui.button("Fix").clicked().then(|| Click::Fix(fix.clone()))
    })
    .inner
}

/// Draws a player with the name field, the name the player sent and
/// "Allow" for a refused address.
///
/// The field keeps what is typed in egui's memory and gives
/// [`Click::Rename`] when it loses focus with a changed name.
fn show_player(ui: &mut egui::Ui, player: &PlayerRow) -> Option<Click> {
    let mut click = None;
    ui.horizontal_wrapped(|ui| {
        let id = egui::Id::unique(("player name", player.address));
        let mut draft = ui
            .data_mut(|data| data.get_temp::<String>(id))
            .unwrap_or_else(|| player.name.clone());
        let field = ui.add(egui::TextEdit::singleline(&mut draft).desired_width(140.0));
        if field.lost_focus() {
            if draft != player.name {
                click = Some(Click::Rename(player.address, draft.trim().to_string()));
            }
            ui.data_mut(|data| data.remove::<String>(id));
        } else if field.has_focus() {
            ui.data_mut(|data| data.insert_temp(id, draft));
        }
        ui.label(player.address.to_string());
        let state = match (player.connected, player.refused) {
            (true, _) => "connected",
            (false, true) => "waiting for you to allow",
            (false, false) => "not connected",
        };
        ui.label(RichText::new(state).color(Color32::GRAY));
        if let Some(offered) = &player.offered {
            ui.label(format!("calls itself {offered}"));
            if ui.button("Use this name").clicked() {
                click = Some(Click::UseOffered(player.address));
            }
        }
        if player.refused && ui.button("Allow").clicked() {
            click = Some(Click::Allow(player.address));
        }
    });
    click
}
