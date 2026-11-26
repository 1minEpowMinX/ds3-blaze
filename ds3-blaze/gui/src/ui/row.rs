//! The row a step is drawn as: a marker, the step's sentence and a chevron
//! on one line, and the details in a panel below it while the row is open.

use crate::flow::checks::sentence;
use crate::flow::step::{Button, Click, Detail, Status, Step};
use crate::ui::theme::{self, ACCENT, GREEN, GREY, ON_ACCENT, ORANGE, RED, STRONG, TEXT, WEAK};
use ds3_blaze_diag::netcheck::{Finding, Severity};
use eframe::egui::text::{LayoutJob, TextFormat};
use eframe::egui::{
    self, Align, Align2, Color32, CursorIcon, FontId, Label, Layout, Margin, Pos2, RichText, Sense,
    Stroke, Vec2, pos2, vec2,
};

/// Height of a row's line, in points.
const LINE_HEIGHT: f32 = 44.0;

/// Diameter of the marker at the start of a line.
const MARKER_SIZE: f32 = 24.0;

/// Space before the marker and between the marker and the sentence.
const MARKER_GAP: f32 = 10.0;

/// The shield an elevating button's label starts with, U+1F6E1, which
/// egui takes from the system's emoji font (Segoe UI Emoji on Windows) and
/// draws in that font's own colours.
const SHIELD: &str = "\u{1F6E1}";

/// Space between the shield and the label, in points.
const SHIELD_GAP: f32 = 6.0;

/// Space between the shield and the edge of the disc it sits on in a main
/// button, in points.
const DISC_PADDING: f32 = 3.5;

/// What an elevating button says on hover.
const PERMISSION: &str = "Windows asks for permission";

/// Space between a card's edge and what a row draws in it.
const CARD_PADDING: f32 = 6.0;

/// Returns the colour a status is drawn in.
fn status_colour(status: Status) -> Color32 {
    match status {
        Status::Waiting => GREY,
        Status::Running => ACCENT,
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
fn show(ui: &mut egui::Ui, number: usize, step: &Step, open: &mut bool) -> Option<Click> {
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
                        ui.add(Label::new(summary_text(step)).truncate().selectable(false));
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
    if openable && *open {
        panel(ui, step)
    } else {
        None
    }
}

/// Returns the sentence as a line shows it: weak while the step waits,
/// plain once it is done, bright while it runs and coloured while it needs
/// attention.
fn summary_text(step: &Step) -> RichText {
    let text = RichText::new(&step.summary);
    match step.status {
        Status::Waiting => text.color(WEAK),
        Status::Ok => text.color(TEXT),
        Status::Running => text.color(STRONG),
        Status::Warning | Status::Failed => text.color(status_colour(step.status)),
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
                WEAK,
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
        Stroke::new(1.5, WEAK),
    );
}

/// Draws `button` at least `height` points high and returns whether it was
/// pressed; a disabled button tells why on hover.
///
/// A button that brings up the administrator prompt has a shield before its
/// label and says so on hover.
pub fn action(ui: &mut egui::Ui, button: &Button, height: f32) -> bool {
    draw(ui, button, height, false)
}

/// Draws `button` as the one thing to press next, filled with the accent,
/// and returns whether it was pressed.
pub fn main_action(ui: &mut egui::Ui, button: &Button, height: f32) -> bool {
    draw(ui, button, height, true)
}

/// Draws `button`, filled with the accent when `main`, and returns whether
/// it was pressed.
fn draw(ui: &mut egui::Ui, button: &Button, height: f32, main: bool) -> bool {
    let font_id = egui::TextStyle::Button.resolve(ui.style());
    let shield = button.elevates.then(|| {
        let colour = if button.enabled() { ACCENT } else { GREY };
        // The colour applies only where the system has no colour emoji font.
        ui.painter()
            .layout_no_wrap(SHIELD.to_owned(), font_id.clone(), colour)
    });
    // On the accent the glyph's own blue is lost, so a main button sets it
    // on a dark disc.
    let disc = |ink: egui::Rect| ink.width().max(ink.height()) + 2.0 * DISC_PADDING;
    let lead = shield.as_ref().map_or(0.0, |shield| {
        let ink = shield.mesh_bounds;
        (if main { disc(ink) } else { ink.width() }) + SHIELD_GAP
    });
    let mut job = LayoutJob::default();
    job.append(
        button.label,
        lead,
        TextFormat {
            font_id,
            // The placeholder takes the button's own text colour.
            color: if main {
                ON_ACCENT
            } else {
                Color32::PLACEHOLDER
            },
            ..TextFormat::default()
        },
    );
    let label_width = ui.painter().layout_job(job.clone()).size().x;
    let padding = theme::button_padding(ui, label_width);
    let mut widget = egui::Button::new(job).min_size(vec2(0.0, height));
    if main {
        widget = widget.fill(ACCENT).stroke(Stroke::NONE);
    }
    let response = ui
        .scope(|ui| {
            ui.spacing_mut().button_padding.x = padding;
            ui.add_enabled(button.enabled(), widget)
        })
        .inner;
    if main && response.hovered() && button.enabled() {
        ui.painter()
            .rect_filled(response.rect, 8, Color32::WHITE.gamma_multiply(0.08));
    }
    if let Some(shield) = shield {
        let mut painter = ui.painter().clone();
        if !button.enabled() {
            painter.multiply_opacity(ui.visuals().disabled_alpha());
        }
        // The emoji font's line is taller than the label's, so the glyph is
        // centred on its own ink rather than on its line.
        let ink = shield.mesh_bounds;
        let left = response.rect.left() + padding;
        let width = if main { disc(ink) } else { ink.width() };
        let centre = pos2(left + width / 2.0, response.rect.center().y);
        if main {
            painter.circle_filled(centre, width / 2.0, ON_ACCENT);
        }
        painter.galley(centre - ink.center().to_vec2(), shield, GREY);
    }
    let response = match button.disabled {
        Some(reason) => response.on_disabled_hover_text(reason),
        None if button.elevates => response.on_hover_text(PERMISSION),
        None => response,
    };
    response.clicked()
}

/// Draws the steps as rows of one card and returns the click one of them
/// gave; while `done`, they fold into one "Setup done" line that `expanded`
/// opens.
pub fn setup(
    ui: &mut egui::Ui,
    steps: &[Step],
    open: &mut [bool],
    done: bool,
    expanded: &mut bool,
) -> Option<Click> {
    theme::card()
        .inner_margin(Margin::symmetric(CARD_PADDING as i8, 0))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            // Rows sit edge to edge; the dividers mark where one ends.
            ui.spacing_mut().item_spacing.y = 0.0;
            rows(ui, steps, open, done, expanded)
        })
        .inner
}

/// Draws the inside of the setup card and returns the click a row gave.
fn rows(
    ui: &mut egui::Ui,
    steps: &[Step],
    open: &mut [bool],
    done: bool,
    expanded: &mut bool,
) -> Option<Click> {
    if done {
        let line = ui
            .allocate_ui_with_layout(
                vec2(ui.available_width(), LINE_HEIGHT),
                Layout::left_to_right(Align::Center),
                |ui| {
                    lead(ui, None, Status::Ok);
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        chevron(ui, Some(*expanded));
                        ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                            let text = RichText::new("Setup done").color(TEXT);
                            ui.add(Label::new(text).selectable(false));
                        });
                    });
                },
            )
            .response;
        let line = ui
            .interact(line.rect, egui::Id::unique("setup done"), Sense::click())
            .on_hover_cursor(CursorIcon::PointingHand);
        line.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, "Setup done"));
        if line.clicked() {
            *expanded = !*expanded;
        }
        if !*expanded {
            return None;
        }
        theme::divider(ui);
    }
    let mut click = None;
    for (index, step) in steps.iter().enumerate() {
        if index > 0 {
            theme::divider(ui);
        }
        if let Some(row_click) = show(ui, index + 1, step, &mut open[index]) {
            click = Some(row_click);
        }
    }
    click
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
    let indent = (MARKER_GAP * 2.0 + MARKER_SIZE) as i8;
    theme::inset(12)
        .outer_margin(Margin {
            left: indent,
            right: CARD_PADDING as i8,
            top: 0,
            bottom: 12,
        })
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 6.0;
            for detail in &step.details {
                if let Some(detail_click) = show_detail(ui, detail) {
                    click = Some(detail_click);
                }
            }
            if step.note.is_none() && step.buttons.is_empty() {
                return;
            }
            if !step.details.is_empty() {
                ui.add_space(4.0);
                theme::divider(ui);
                ui.add_space(4.0);
            }
            let (_, button_click) = egui::Sides::new().shrink_left().show(
                ui,
                |ui| {
                    if let Some(note) = &step.note {
                        ui.add(Label::new(RichText::new(note).color(WEAK)).wrap());
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
            ui.add(Label::new(RichText::new(sentence(text)).color(TEXT)).wrap());
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
                ui.add(Label::new(RichText::new(sentence(&finding.text)).color(TEXT)).wrap());
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
