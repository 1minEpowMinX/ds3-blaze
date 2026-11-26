//! The row a step is drawn as: a marker, the step's sentence and a chevron
//! on one line, and the details in a panel below it while the row is open.

use crate::step::{Button, Click, Detail, Status, Step};
use ds3_blaze_diag::netcheck::{Finding, Severity};
use eframe::egui::{
    self, Align, Align2, Color32, CursorIcon, FontId, Label, Layout, Margin, Pos2, RichText, Sense,
    Stroke, Vec2, vec2,
};

/// Height of a row's line, in points.
const LINE_HEIGHT: f32 = 44.0;

/// Diameter of the marker at the start of a line.
const MARKER_SIZE: f32 = 24.0;

/// Space before the marker and between the marker and the sentence.
const MARKER_GAP: f32 = 10.0;

pub const GREEN: Color32 = Color32::from_rgb(95, 185, 105);
pub const ORANGE: Color32 = Color32::from_rgb(230, 150, 50);
pub const RED: Color32 = Color32::from_rgb(225, 85, 75);
const BLUE: Color32 = Color32::from_rgb(95, 155, 235);
pub const GREY: Color32 = Color32::from_gray(140);

/// Returns the colour a status is drawn in.
fn status_colour(status: Status) -> Color32 {
    match status {
        Status::Waiting => GREY,
        Status::Running => BLUE,
        Status::Ok => GREEN,
        Status::Warning => ORANGE,
        Status::Failed => RED,
    }
}

/// Draws `step` as the row numbered `number` and returns the click a button
/// or a field in its details gave.
///
/// A click anywhere on the line toggles `open` when the step has details,
/// a note or buttons; they show in a panel below the line while `open` is
/// true.
pub fn show(ui: &mut egui::Ui, number: usize, step: &Step, open: &mut bool) -> Option<Click> {
    let openable = !step.details.is_empty() || step.note.is_some() || !step.buttons.is_empty();
    let line = ui
        .allocate_ui_with_layout(
            vec2(ui.available_width(), LINE_HEIGHT),
            Layout::left_to_right(Align::Center),
            |ui| {
                lead(ui, Some(number), step.status);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    chevron(ui, openable.then_some(*open));
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
                egui::Id::unique(("row line", number)),
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

/// Returns the sentence as a line shows it: weak while the step waits,
/// coloured while it needs attention.
fn summary_text(step: &Step, ui: &egui::Ui) -> RichText {
    let text = RichText::new(&step.summary);
    match step.status {
        Status::Waiting => text.color(ui.visuals().weak_text_color()),
        Status::Warning | Status::Failed => text.color(status_colour(step.status)),
        Status::Running | Status::Ok => text.color(ui.visuals().strong_text_color()),
    }
}

/// Draws the marker with the space around it.
fn lead(ui: &mut egui::Ui, number: Option<usize>, status: Status) {
    ui.add_space(MARKER_GAP);
    marker(ui, number, status);
    ui.add_space(MARKER_GAP);
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

/// Draws `button` at least `height` points high and returns whether it was
/// pressed; a disabled button tells why on hover.
pub fn action(ui: &mut egui::Ui, button: &Button, height: f32) -> bool {
    let widget = egui::Button::new(button.label).min_size(vec2(0.0, height));
    let response = ui.add_enabled(button.enabled(), widget);
    let response = match button.disabled {
        Some(reason) => response.on_disabled_hover_text(reason),
        None => response,
    };
    response.clicked()
}

/// Draws the step's buttons right to left and returns the click one gave.
fn buttons(ui: &mut egui::Ui, step: &Step) -> Option<Click> {
    let mut click = None;
    for button in step.buttons.iter().rev() {
        if action(ui, button, 30.0) {
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
                let button = Button::fix(finding.fix.as_ref()?);
                action(ui, &button, 26.0).then_some(button.click)
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
