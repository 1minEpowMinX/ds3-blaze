//! The row a step is drawn as: a marker, the title, the summary, a status
//! pill and a chevron on one line, and the details in a panel below it while
//! the row is open.

use crate::step::{Click, Detail, PlayerRow, Status, Step};
use ds3_blaze_diag::netcheck::{Finding, Fix, Severity};
use eframe::egui::{
    self, Align, Align2, Color32, CursorIcon, FontId, Label, Layout, Margin, Pos2, RichText, Sense,
    Stroke, Vec2, vec2,
};

/// Height of a row's line, in points.
const LINE_HEIGHT: f32 = 44.0;

/// Diameter of the marker at the start of a line.
const MARKER_SIZE: f32 = 24.0;

/// Space before the marker and between the marker and the title.
const MARKER_GAP: f32 = 10.0;

/// Width of the title column.
const TITLE_WIDTH: f32 = 210.0;

/// Widths of a player's name, address and state columns.
const PLAYER_COLUMNS: [f32; 3] = [230.0, 120.0, 90.0];

pub const GREEN: Color32 = Color32::from_rgb(95, 185, 105);
pub const ORANGE: Color32 = Color32::from_rgb(230, 150, 50);
pub const RED: Color32 = Color32::from_rgb(225, 85, 75);
const BLUE: Color32 = Color32::from_rgb(95, 155, 235);
const GREY: Color32 = Color32::from_gray(140);

/// Returns the colour a status is drawn in.
fn status_colour(status: Status) -> Color32 {
    match status {
        Status::Waiting | Status::Info => GREY,
        Status::Running => BLUE,
        Status::Ok => GREEN,
        Status::Warning => ORANGE,
        Status::Failed => RED,
    }
}

/// Returns the space a line gives the marker and the title, before the
/// summary or the input starts.
fn lead_width(ui: &egui::Ui) -> f32 {
    MARKER_GAP + MARKER_SIZE + MARKER_GAP + TITLE_WIDTH + ui.spacing().item_spacing.x * 3.0
}

/// Draws `step` as the row numbered `number` and returns the click a button
/// or a field in its details gave.
///
/// A click anywhere on the line toggles `open` when the step has details,
/// a note or buttons; they show in a panel below the line while `open` is
/// true.
pub fn show(ui: &mut egui::Ui, number: usize, step: &Step, open: &mut bool) -> Option<Click> {
    let colour = status_colour(step.status);
    let openable = !step.details.is_empty() || step.note.is_some() || !step.buttons.is_empty();
    let line = ui
        .allocate_ui_with_layout(
            vec2(ui.available_width(), LINE_HEIGHT),
            Layout::left_to_right(Align::Center),
            |ui| {
                lead(ui, Some(number), step);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    chevron(ui, openable.then_some(*open));
                    let word = step.pill();
                    if !word.is_empty() {
                        pill(ui, word, colour);
                    }
                    ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                        ui.add(
                            Label::new(summary_text(step, ui))
                                .truncate()
                                .selectable(false),
                        );
                    });
                });
            },
        )
        .response;
    if openable {
        let line = ui
            .interact(
                line.rect,
                egui::Id::unique(("row line", &step.title)),
                Sense::click(),
            )
            .on_hover_cursor(CursorIcon::PointingHand);
        if line.clicked() {
            *open = !*open;
        }
    }
    let click = if openable && *open {
        panel(ui, step)
    } else {
        None
    };
    ui.add(egui::Separator::default().spacing(0.0));
    click
}

/// Draws `step` as an unnumbered row with `input` in place of the summary
/// and the step's buttons after it, and returns the click a button gave.
///
/// The summary shows below the line while the step runs or went wrong, and
/// `error` below it.
pub fn show_input(
    ui: &mut egui::Ui,
    step: &Step,
    error: Option<&str>,
    input: impl FnOnce(&mut egui::Ui),
) -> Option<Click> {
    let mut click = None;
    ui.allocate_ui_with_layout(
        vec2(ui.available_width(), LINE_HEIGHT),
        Layout::left_to_right(Align::Center),
        |ui| {
            lead(ui, None, step);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                click = buttons(ui, step);
                ui.with_layout(Layout::left_to_right(Align::Center), input);
            });
        },
    );
    let worth_showing = matches!(
        step.status,
        Status::Running | Status::Warning | Status::Failed
    );
    let lines = [
        (worth_showing && !step.summary.is_empty()).then(|| summary_text(step, ui)),
        error.map(|error| RichText::new(error).color(RED)),
    ];
    for text in lines.into_iter().flatten() {
        ui.horizontal(|ui| {
            ui.add_space(lead_width(ui));
            ui.add(Label::new(text).wrap());
        });
    }
    ui.add_space(4.0);
    ui.add(egui::Separator::default().spacing(0.0));
    click
}

/// Returns the summary as a line shows it: coloured while the step needs
/// attention.
fn summary_text(step: &Step, ui: &egui::Ui) -> RichText {
    let text = RichText::new(&step.summary);
    match step.status {
        Status::Warning | Status::Failed => text.color(status_colour(step.status)),
        _ => text.color(ui.visuals().text_color()),
    }
}

/// Draws the marker and the title column.
fn lead(ui: &mut egui::Ui, number: Option<usize>, step: &Step) {
    ui.add_space(MARKER_GAP);
    marker(ui, number, step.status);
    ui.add_space(MARKER_GAP);
    ui.allocate_ui_with_layout(
        vec2(TITLE_WIDTH, LINE_HEIGHT),
        Layout::left_to_right(Align::Center),
        |ui| {
            ui.set_min_width(TITLE_WIDTH);
            let title = RichText::new(&step.title).color(ui.visuals().strong_text_color());
            ui.add(Label::new(title).truncate().selectable(false));
        },
    );
}

/// Draws the round marker of a step: its number, if it has one, while it
/// waits, a spinner while it runs and a sign for each outcome.
fn marker(ui: &mut egui::Ui, number: Option<usize>, status: Status) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(MARKER_SIZE), Sense::hover());
    let colour = status_colour(status);
    let centre = rect.center();
    ui.painter()
        .circle_filled(centre, MARKER_SIZE / 2.0, colour.gamma_multiply(0.22));
    match status {
        Status::Running => egui::Spinner::new()
            .size(14.0)
            .color(colour)
            .paint_at(ui, rect.shrink(5.0)),
        Status::Waiting => {
            let Some(number) = number else {
                return;
            };
            ui.painter().text(
                centre,
                Align2::CENTER_CENTER,
                number.to_string(),
                FontId::proportional(13.0),
                ui.visuals().text_color(),
            );
        }
        Status::Info => {
            for offset in [-4.0, 0.0, 4.0] {
                ui.painter().line_segment(
                    [centre + vec2(-5.0, offset), centre + vec2(5.0, offset)],
                    Stroke::new(1.5, ui.visuals().text_color()),
                );
            }
        }
        Status::Ok | Status::Warning | Status::Failed => sign(ui, centre, status, colour),
    }
}

/// Paints the sign of an outcome around `centre`: a tick, an exclamation
/// mark or a cross.
fn sign(ui: &egui::Ui, centre: Pos2, status: Status, colour: Color32) {
    let painter = ui.painter();
    let stroke = Stroke::new(2.0, colour);
    match status {
        Status::Ok => {
            painter.line(
                vec![
                    centre + vec2(-4.5, 0.0),
                    centre + vec2(-1.5, 3.0),
                    centre + vec2(4.5, -3.5),
                ],
                stroke,
            );
        }
        Status::Failed => {
            painter.line_segment([centre + vec2(-4.0, -4.0), centre + vec2(4.0, 4.0)], stroke);
            painter.line_segment([centre + vec2(-4.0, 4.0), centre + vec2(4.0, -4.0)], stroke);
        }
        _ => {
            painter.line_segment([centre + vec2(0.0, -5.0), centre + vec2(0.0, 1.5)], stroke);
            painter.circle_filled(centre + vec2(0.0, 4.5), 1.2, colour);
        }
    }
}

/// Draws the chevron that shows whether a row is open, or the space it takes
/// when `open` is `None` because the row has nothing to open.
fn chevron(ui: &mut egui::Ui, open: Option<bool>) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(16.0), Sense::hover());
    let Some(open) = open else {
        return;
    };
    let centre = rect.center();
    let points = if open {
        [vec2(-4.0, -2.0), vec2(0.0, 2.0), vec2(4.0, -2.0)]
    } else {
        [vec2(-2.0, -4.0), vec2(2.0, 0.0), vec2(-2.0, 4.0)]
    };
    ui.painter().line(
        points.iter().map(|point| centre + *point).collect(),
        Stroke::new(1.5, ui.visuals().weak_text_color()),
    );
}

/// Draws `word` in a rounded pill tinted with `colour`, as high as its text.
fn pill(ui: &mut egui::Ui, word: &str, colour: Color32) {
    let galley = ui
        .painter()
        .layout_no_wrap(word.to_string(), FontId::proportional(12.5), colour);
    let (rect, _) = ui.allocate_exact_size(galley.size() + vec2(18.0, 6.0), Sense::hover());
    ui.painter()
        .rect_filled(rect, 10.0, colour.gamma_multiply(0.18));
    ui.painter()
        .galley(rect.center() - galley.size() / 2.0, galley, colour);
}

/// Draws the step's buttons right to left and returns the click one gave.
fn buttons(ui: &mut egui::Ui, step: &Step) -> Option<Click> {
    let mut click = None;
    for button in step.buttons.iter().rev() {
        let widget = egui::Button::new(button.label).min_size(vec2(0.0, 30.0));
        if ui.add_enabled(button.enabled, widget).clicked() {
            click = Some(button.click.clone());
        }
    }
    click
}

/// Draws the panel of an open row: the details, then the note and the
/// buttons; returns the click one of them gave.
fn panel(ui: &mut egui::Ui, step: &Step) -> Option<Click> {
    let mut click = None;
    let indent = (MARKER_GAP + MARKER_SIZE) as i8;
    egui::Frame::new()
        .fill(ui.visuals().faint_bg_color)
        .stroke(ui.visuals().widgets.noninteractive.bg_stroke)
        .corner_radius(8.0)
        .inner_margin(Margin::same(12))
        .outer_margin(Margin {
            left: indent,
            right: 0,
            top: 0,
            bottom: 8,
        })
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            for detail in &step.details {
                if let Some(detail_click) = show_detail(ui, detail) {
                    click = Some(detail_click);
                }
            }
            if step.note.is_none() && step.buttons.is_empty() {
                return;
            }
            if !step.details.is_empty() {
                ui.separator();
            }
            let (_, button_click) = egui::Sides::new().shrink_left().show(
                ui,
                |ui| {
                    if let Some(note) = &step.note {
                        ui.add(Label::new(RichText::new(note).weak()).wrap());
                    }
                },
                |ui| buttons(ui, step),
            );
            if button_click.is_some() {
                click = button_click;
            }
        });
    click
}

/// Draws one line of the details and returns the click it gave.
fn show_detail(ui: &mut egui::Ui, detail: &Detail) -> Option<Click> {
    match detail {
        Detail::Text(text) => {
            ui.add(Label::new(text.as_str()).wrap());
            None
        }
        Detail::Finding(finding) => show_finding(ui, finding),
        Detail::Player(player) => show_player(ui, player),
        Detail::Toggle { label, on, click } => {
            let mut ticked = *on;
            ui.add_space(4.0);
            ui.checkbox(&mut ticked, *label)
                .changed()
                .then(|| click.clone())
        }
    }
}

/// Returns the label of the button that applies `fix`.
fn fix_label(fix: &Fix) -> &'static str {
    match fix {
        Fix::GameRule | Fix::ServerRule(_) => "Add rule",
        Fix::PrivateProfile(_) => "Make private",
    }
}

/// Draws a finding with its sign and its fix button.
fn show_finding(ui: &mut egui::Ui, finding: &Finding) -> Option<Click> {
    egui::Sides::new()
        .shrink_left()
        .show(
            ui,
            |ui| {
                severity_sign(ui, finding.severity);
                ui.add(Label::new(finding.text.as_str()).wrap());
            },
            |ui| {
                let fix = finding.fix.as_ref()?;
                ui.add(egui::Button::new(fix_label(fix)).min_size(vec2(0.0, 26.0)))
                    .clicked()
                    .then(|| Click::Fix(fix.clone()))
            },
        )
        .1
}

/// Draws the small sign a finding starts with.
fn severity_sign(ui: &mut egui::Ui, severity: Severity) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(16.0), Sense::hover());
    let centre = rect.center();
    let painter = ui.painter();
    match severity {
        Severity::Ok => sign(ui, centre, Status::Ok, GREEN),
        Severity::Note => {
            painter.circle_filled(centre, 2.5, GREY);
        }
        Severity::Warning => {
            let corners = vec![
                centre + vec2(0.0, -6.5),
                centre + vec2(7.0, 5.5),
                centre + vec2(-7.0, 5.5),
                centre + vec2(0.0, -6.5),
            ];
            painter.line(corners, Stroke::new(1.5, ORANGE));
            painter.line_segment(
                [centre + vec2(0.0, -2.0), centre + vec2(0.0, 1.5)],
                Stroke::new(1.5, ORANGE),
            );
            painter.circle_filled(centre + vec2(0.0, 3.6), 0.9, ORANGE);
        }
        Severity::Problem => sign(ui, centre, Status::Failed, RED),
    }
}

/// Draws `add` in a cell `width` points wide.
fn cell(ui: &mut egui::Ui, width: f32, add: impl FnOnce(&mut egui::Ui)) {
    ui.allocate_ui_with_layout(
        vec2(width, 30.0),
        Layout::left_to_right(Align::Center),
        |ui| {
            ui.set_min_width(width);
            add(ui);
        },
    );
}

/// Draws a player: the name, which the host types over for the others, the
/// address, the state and the buttons that apply to it.
///
/// The field keeps what is typed in egui's memory and gives
/// [`Click::Rename`] when it loses focus with a changed name.
fn show_player(ui: &mut egui::Ui, player: &PlayerRow) -> Option<Click> {
    let mut click = None;
    let [name_width, address_width, state_width] = PLAYER_COLUMNS;
    ui.horizontal(|ui| {
        cell(ui, name_width, |ui| {
            if player.you {
                ui.label(RichText::new(&player.name).color(ui.visuals().strong_text_color()));
                ui.label(RichText::new("(you)").weak());
                return;
            }
            let id = egui::Id::unique(("player name", player.address));
            let mut draft = ui
                .data_mut(|data| data.get_temp::<String>(id))
                .unwrap_or_else(|| player.name.clone());
            let field =
                ui.add(egui::TextEdit::singleline(&mut draft).desired_width(name_width - 16.0));
            if field.lost_focus() {
                if draft != player.name {
                    click = Some(Click::Rename(player.address, draft.trim().to_string()));
                }
                ui.data_mut(|data| data.remove::<String>(id));
            } else if field.has_focus() {
                ui.data_mut(|data| data.insert_temp(id, draft));
            }
        });
        cell(ui, address_width, |ui| {
            ui.label(player.address.to_string());
        });
        let (word, colour) = match (player.you, player.connected, player.refused) {
            (true, ..) => ("host", GREEN),
            (false, true, _) => ("online", GREEN),
            (false, false, true) => ("refused", ORANGE),
            (false, false, false) => ("offline", GREY),
        };
        cell(ui, state_width, |ui| pill(ui, word, colour));
        if player.refused && ui.button("Allow").clicked() {
            click = Some(Click::Allow(player.address));
        }
        if let Some(offered) = &player.offered {
            ui.label(RichText::new(format!("calls itself {offered}")).weak());
            if ui.small_button("Use this name").clicked() {
                click = Some(Click::UseOffered(player.address));
            }
        }
    });
    click
}
