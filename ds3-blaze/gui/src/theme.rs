//! The window's look: its colours, type sizes and the surfaces every screen
//! is built from.
//!
//! The accent marks the main button, the administrator shield, a running
//! step and a neutral banner; green, orange and red mark how a step, a
//! finding or a player stands.

use eframe::egui::text::{LayoutJob, TextFormat};
use eframe::egui::{
    self, Color32, CornerRadius, FontFamily, FontId, Margin, Stroke, TextStyle, Visuals, vec2,
};

/// Window background, behind every card.
pub const BACKGROUND: Color32 = Color32::from_rgb(21, 23, 27);
/// Fill of a card.
pub const SURFACE: Color32 = Color32::from_rgb(30, 33, 38);
/// Fill of a well sunk into a card: fields, an open row's details, the log.
pub const INSET: Color32 = Color32::from_rgb(17, 19, 22);
/// Fill of a button at rest.
const RAISED: Color32 = Color32::from_rgb(43, 47, 54);
/// Fill of a button under the pointer.
const HOVERED: Color32 = Color32::from_rgb(53, 58, 66);
/// Fill of a button being pressed.
const PRESSED: Color32 = Color32::from_rgb(62, 68, 78);
/// Lines between rows and around fields.
pub const LINE: Color32 = Color32::from_rgb(45, 49, 56);

pub const TEXT: Color32 = Color32::from_rgb(200, 205, 213);
pub const STRONG: Color32 = Color32::from_rgb(242, 244, 247);
pub const WEAK: Color32 = Color32::from_rgb(132, 139, 150);

/// What to press next: the main button and the shield on a button.
pub const ACCENT: Color32 = Color32::from_rgb(92, 190, 232);
/// Text drawn on the accent.
pub const ON_ACCENT: Color32 = Color32::from_rgb(10, 24, 32);

pub const GREEN: Color32 = Color32::from_rgb(92, 192, 120);
pub const ORANGE: Color32 = Color32::from_rgb(234, 162, 66);
pub const RED: Color32 = Color32::from_rgb(232, 94, 86);
pub const GREY: Color32 = Color32::from_rgb(110, 116, 126);

/// Widest the content of a screen gets, in points; a wider window leaves the
/// rest as margins on both sides.
pub const COLUMN: f32 = 640.0;

/// Corner radius of a card.
pub const CARD_RADIUS: u8 = 12;

/// Sets the colours, type sizes and spacing every screen is drawn with.
pub fn apply(ctx: &egui::Context) {
    ctx.set_visuals(visuals());
    ctx.all_styles_mut(|style| {
        style.visuals = visuals();
        style.text_styles = [
            (
                TextStyle::Small,
                FontId::new(11.5, FontFamily::Proportional),
            ),
            (TextStyle::Body, FontId::new(14.0, FontFamily::Proportional)),
            (
                TextStyle::Button,
                FontId::new(14.0, FontFamily::Proportional),
            ),
            (
                TextStyle::Heading,
                FontId::new(20.0, FontFamily::Proportional),
            ),
            (
                TextStyle::Monospace,
                FontId::new(12.5, FontFamily::Monospace),
            ),
        ]
        .into();
        style.spacing.item_spacing = vec2(8.0, 6.0);
        style.spacing.button_padding = vec2(14.0, 6.0);
        style.spacing.interact_size.y = 30.0;
    });
}

/// Returns the dark visuals the palette above makes.
fn visuals() -> Visuals {
    let mut visuals = Visuals::dark();
    visuals.panel_fill = BACKGROUND;
    visuals.window_fill = SURFACE;
    visuals.window_stroke = Stroke::new(1.0, LINE);
    visuals.faint_bg_color = SURFACE;
    visuals.extreme_bg_color = INSET;
    visuals.text_edit_bg_color = Some(INSET);
    visuals.code_bg_color = INSET;
    visuals.weak_text_color = Some(WEAK);
    visuals.hyperlink_color = ACCENT;
    visuals.warn_fg_color = ORANGE;
    visuals.error_fg_color = RED;
    visuals.selection.bg_fill = ACCENT.gamma_multiply(0.35);
    visuals.selection.stroke = Stroke::new(1.0, ACCENT);
    visuals.text_cursor.stroke = Stroke::new(2.0, ACCENT);

    let radius = CornerRadius::same(8);
    let widgets = &mut visuals.widgets;
    widgets.noninteractive.bg_fill = SURFACE;
    widgets.noninteractive.weak_bg_fill = SURFACE;
    widgets.noninteractive.bg_stroke = Stroke::new(1.0, LINE);
    widgets.noninteractive.fg_stroke = Stroke::new(1.0, TEXT);
    widgets.noninteractive.corner_radius = radius;
    for (state, fill, text) in [
        (&mut widgets.inactive, RAISED, TEXT),
        (&mut widgets.hovered, HOVERED, STRONG),
        (&mut widgets.active, PRESSED, STRONG),
        (&mut widgets.open, PRESSED, STRONG),
    ] {
        state.bg_fill = fill;
        state.weak_bg_fill = fill;
        state.bg_stroke = Stroke::new(1.0, LINE);
        state.fg_stroke = Stroke::new(1.0, text);
        state.corner_radius = radius;
        state.expansion = 0.0;
    }
    widgets.hovered.bg_stroke = Stroke::new(1.0, HOVERED.lerp_to_gamma(TEXT, 0.25));
    visuals
}

/// Returns the frame of a card, with no inner margin of its own.
pub fn card() -> egui::Frame {
    egui::Frame::new().fill(SURFACE).corner_radius(CARD_RADIUS)
}

/// Returns the frame of a well sunk into a card, with `margin` inside.
pub fn inset(margin: i8) -> egui::Frame {
    egui::Frame::new()
        .fill(INSET)
        .corner_radius(8)
        .inner_margin(Margin::same(margin))
}

/// Draws `add` in a column at most [`COLUMN`] wide, centred in the space
/// left, and returns what `add` returned.
pub fn column<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let width = ui.available_width().min(COLUMN);
    let side = ((ui.available_width() - width) / 2.0).max(0.0);
    ui.horizontal_top(|ui| {
        ui.add_space(side);
        ui.vertical(|ui| {
            ui.set_width(width);
            add(ui)
        })
        .inner
    })
    .inner
}

/// Draws the small capitals that open a part of a screen, with the space
/// that sets the part apart from the one before.
pub fn section(ui: &mut egui::Ui, text: &str) {
    ui.add_space(18.0);
    section_label(ui, text);
    ui.add_space(4.0);
}

/// Draws `text` in the small capitals of a part's title.
pub fn section_label(ui: &mut egui::Ui, text: &str) {
    let mut job = LayoutJob::default();
    job.append(
        &text.to_uppercase(),
        0.0,
        TextFormat {
            font_id: FontId::proportional(11.5),
            color: WEAK,
            extra_letter_spacing: 1.2,
            ..TextFormat::default()
        },
    );
    ui.add(egui::Label::new(job).selectable(false));
}

/// Draws a thin line across the width left, taking one point of height.
pub fn divider(ui: &mut egui::Ui) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), egui::Sense::hover());
    ui.painter()
        .hline(rect.x_range(), rect.center().y, Stroke::new(1.0, LINE));
}
