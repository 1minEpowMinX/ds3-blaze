//! Drawing of the host's session.

use crate::flow::session::{PlayerRow, SESSION_LOG_LINES, SessionView};
use crate::flow::step::Click;
use crate::ui::theme::{self, GREEN, GREY, ORANGE, RED, STRONG, TEXT, WEAK};
use eframe::egui::{
    self, Align, Align2, Color32, CursorIcon, Label, Layout, Margin, RichText, Sense, TextStyle,
    Vec2, vec2,
};

/// Width of a guest's name field, in points.
const NAME_WIDTH: f32 = 170.0;

/// Height of a player's row, in points.
const PLAYER_HEIGHT: f32 = 42.0;

/// Space between a card's edge and its content, in points.
const PADDING: i8 = 14;

/// Draws `view` and returns the click a field or a button gave.
pub fn show(ui: &mut egui::Ui, view: &SessionView) -> Option<Click> {
    let mut click = None;
    theme::section(ui, "This game");
    tiles(
        ui,
        [
            ("Room", Some(view.room.as_str())),
            ("Progress", view.progress.as_deref()),
            ("Matchmaking", view.matchmaking),
        ],
    );

    theme::section(ui, "Players");
    theme::card()
        .inner_margin(Margin::symmetric(PADDING, 0))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 0.0;
            for (index, player) in view.players.iter().enumerate() {
                if index > 0 {
                    theme::divider(ui);
                }
                if let Some(row_click) = player_row(ui, player) {
                    click = Some(row_click);
                }
            }
        });
    if let Some(refused) = &view.refused_name {
        ui.add_space(4.0);
        ui.label(RichText::new(refused).color(RED));
    }
    ui.add_space(8.0);
    let mut on = view.only_allowed;
    if ui
        .checkbox(&mut on, "Only let in players I approve")
        .changed()
    {
        click = Some(Click::OnlyAllowed(!view.only_allowed));
    }

    ui.add_space(18.0);
    let (_, open_logs) = egui::Sides::new().show(
        ui,
        |ui| theme::section_label(ui, "Server log"),
        |ui| theme::button(ui, "Open logs folder").clicked(),
    );
    if open_logs {
        click = Some(Click::OpenLogs);
    }
    ui.add_space(4.0);
    let start = view.log.len().saturating_sub(SESSION_LOG_LINES);
    theme::inset(PADDING).show(ui, |ui| {
        ui.set_width(ui.available_width());
        if view.log.is_empty() {
            ui.label(RichText::new("Nothing yet").color(WEAK));
        }
        for line in &view.log[start..] {
            ui.add(
                Label::new(
                    RichText::new(line)
                        .text_style(TextStyle::Monospace)
                        .color(WEAK),
                )
                .truncate(),
            );
        }
    });
    click
}

/// Draws `tiles` side by side in one card, each a caption over its value or
/// "–", with a line between two.
fn tiles(ui: &mut egui::Ui, tiles: [(&str, Option<&str>); 3]) {
    theme::card()
        .inner_margin(Margin::symmetric(0, PADDING))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            let width = ui.available_width() / 3.0;
            ui.spacing_mut().item_spacing.x = 0.0;
            ui.horizontal_top(|ui| {
                for (index, (caption, value)) in tiles.into_iter().enumerate() {
                    let tile = ui.allocate_ui_with_layout(
                        vec2(width, 0.0),
                        Layout::top_down(Align::Min),
                        |ui| {
                            ui.set_width(width);
                            egui::Frame::new()
                                .inner_margin(Margin::symmetric(PADDING + 2, 0))
                                .show(ui, |ui| {
                                    ui.set_width(ui.available_width());
                                    ui.spacing_mut().item_spacing.y = 4.0;
                                    ui.label(RichText::new(caption).small().color(WEAK));
                                    let value = RichText::new(value.unwrap_or("–"))
                                        .size(15.0)
                                        .color(if value.is_some() { STRONG } else { WEAK });
                                    ui.add(Label::new(value).wrap());
                                });
                        },
                    );
                    if index > 0 {
                        let rect = tile.response.rect;
                        ui.painter().vline(
                            rect.left(),
                            rect.y_range(),
                            egui::Stroke::new(1.0, theme::LINE),
                        );
                    }
                }
            });
        });
}

/// Draws a round dot of `colour` that marks a player's state.
fn dot(ui: &mut egui::Ui, colour: Color32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(12.0), Sense::hover());
    ui.painter().circle_filled(rect.center(), 4.0, colour);
}

/// Draws a player and returns the click its field or a button gave.
///
/// A guest's name is text with Rename beside it; Rename or a click on the
/// name opens a field in its place. The field keeps what is typed in egui's
/// memory while it is open, and closing it gives [`Click::Rename`] when the
/// name changed, unless Escape closed it. A refused address has no name.
fn player_row(ui: &mut egui::Ui, player: &PlayerRow) -> Option<Click> {
    let mut click = None;
    let size = vec2(ui.available_width(), PLAYER_HEIGHT);
    ui.allocate_ui_with_layout(size, Layout::left_to_right(Align::Center), |ui| {
        ui.set_min_size(size);
        if player.you {
            dot(ui, GREEN);
            ui.label(RichText::new(&player.name).color(STRONG));
            ui.label(RichText::new("you").color(WEAK));
            return;
        }
        let id = egui::Id::unique(("player name", player.address));
        let mut draft = ui.data_mut(|data| data.get_temp::<String>(id));
        if player.refused {
            dot(ui, ORANGE);
            ui.label(
                RichText::new(format!("Someone at {} was turned away", player.address)).color(TEXT),
            );
        } else {
            dot(ui, if player.connected { GREEN } else { GREY });
            match &mut draft {
                Some(draft) => {
                    if let Some(rename) = name_field(ui, id, draft, player) {
                        click = Some(rename);
                    }
                }
                None => {
                    let name = ui
                        .add(
                            Label::new(RichText::new(&player.name).color(STRONG))
                                .selectable(false)
                                .sense(Sense::click()),
                        )
                        .on_hover_cursor(CursorIcon::Text);
                    if name.clicked() {
                        ui.data_mut(|data| data.insert_temp(id, player.name.clone()));
                    }
                }
            }
            ui.label(RichText::new(player.address.to_string()).color(WEAK));
            if !player.connected {
                ui.label(RichText::new("offline").color(WEAK));
            }
        }
        if let Some(offered) = &player.offered {
            ui.label(RichText::new(format!("calls itself {offered}")).color(WEAK));
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if player.refused {
                if theme::button(ui, "Let in").clicked() {
                    click = Some(Click::Allow(player.address));
                }
                return;
            }
            // While the field is open, Done takes the focus from it and so
            // closes it.
            let rename = if draft.is_some() { "Done" } else { "Rename" };
            if theme::button(ui, rename).clicked() && draft.is_none() {
                ui.data_mut(|data| data.insert_temp(id, player.name.clone()));
            }
            if player.offered.is_some() && theme::button(ui, "Use this name").clicked() {
                click = Some(Click::UseOffered(player.address));
            }
        });
    });
    click
}

/// Draws the open name field of `player`, keeping `draft` under `id`, and
/// returns the rename closing it gave.
fn name_field(
    ui: &mut egui::Ui,
    id: egui::Id,
    draft: &mut String,
    player: &PlayerRow,
) -> Option<Click> {
    let field = ui.add(
        egui::TextEdit::singleline(draft)
            .desired_width(NAME_WIDTH)
            .align(Align2::LEFT_CENTER)
            .margin(vec2(6.0, 4.0)),
    );
    if field.lost_focus() {
        ui.data_mut(|data| data.remove::<String>(id));
        let escaped = ui.input(|input| input.key_pressed(egui::Key::Escape));
        return (!escaped && *draft != player.name)
            .then(|| Click::Rename(player.address, draft.trim().to_string()));
    }
    if !field.has_focus() {
        // The field has just opened.
        field.request_focus();
    }
    ui.data_mut(|data| data.insert_temp(id, draft.clone()));
    None
}
