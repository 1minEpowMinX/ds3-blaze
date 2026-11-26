//! The host's session while the server runs: the room, the players and the
//! server's recent log lines.

use crate::step::Click;
use crate::theme::{self, GREEN, GREY, ORANGE, RED, STRONG, TEXT, WEAK};
use ds3_blaze_core::snapshot::RoomState;
use eframe::egui::{
    self, Align, Align2, Color32, Label, Layout, Margin, RichText, Sense, TextStyle, Vec2, vec2,
};
use std::net::Ipv4Addr;

/// Number of log lines the session shows.
pub const SESSION_LOG_LINES: usize = 6;

/// Width of a guest's name field, in points.
const NAME_WIDTH: f32 = 170.0;

/// Height of a player's row, in points.
const PLAYER_HEIGHT: f32 = 42.0;

/// Space between a card's edge and its content, in points.
const PADDING: i8 = 14;

/// What the host's session shows while the server runs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionView {
    /// The room's state and how many of its places are taken, or what to do
    /// before a room exists or while the check with the guest runs.
    pub room: String,
    /// The chapter and restore point, `None` before the game reports one.
    pub progress: Option<String>,
    /// Whether the room is open to matchmaking, `None` before a room exists.
    pub matchmaking: Option<&'static str>,
    /// The host's own row first, then the guests, connected before refused.
    pub players: Vec<PlayerRow>,
    pub only_allowed: bool,
    /// Why the name typed last for a player was refused.
    pub refused_name: Option<String>,
    /// The recent log lines, oldest first.
    pub log: Vec<String>,
}

/// A player in the host's session.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlayerRow {
    pub address: Ipv4Addr,
    /// Whether the row is the host's own.
    pub you: bool,
    /// The name the host's server gives the player.
    pub name: String,
    /// The name the player sent, when it differs from `name` or the server
    /// refuses the player.
    pub offered: Option<String>,
    pub connected: bool,
    /// Whether the server refuses the player's connections.
    pub refused: bool,
}

/// Returns the word a room's state is shown as.
pub fn state_word(state: RoomState) -> String {
    match state {
        RoomState::Initializing => "Setting up".into(),
        RoomState::PreGame => "In the lobby".into(),
        RoomState::InGame => "In game".into(),
        RoomState::Other(value) => format!("State {value}"),
    }
}

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
        |ui| ui.small_button("Open logs folder").clicked(),
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
/// A guest's name field keeps what is typed in egui's memory and gives
/// [`Click::Rename`] when it loses focus with a changed name. A refused
/// address has no name field.
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
        if player.refused {
            dot(ui, ORANGE);
            ui.label(
                RichText::new(format!("Someone at {} was turned away", player.address)).color(TEXT),
            );
        } else {
            dot(ui, if player.connected { GREEN } else { GREY });
            let id = egui::Id::unique(("player name", player.address));
            let mut draft = ui
                .data_mut(|data| data.get_temp::<String>(id))
                .unwrap_or_else(|| player.name.clone());
            let field = ui.add(
                egui::TextEdit::singleline(&mut draft)
                    .desired_width(NAME_WIDTH)
                    .align(Align2::LEFT_CENTER)
                    .margin(vec2(6.0, 4.0)),
            );
            if field.lost_focus() {
                if draft != player.name {
                    click = Some(Click::Rename(player.address, draft.trim().to_string()));
                }
                ui.data_mut(|data| data.remove::<String>(id));
            } else if field.has_focus() {
                ui.data_mut(|data| data.insert_temp(id, draft));
            }
            ui.label(RichText::new(player.address.to_string()).color(WEAK));
            if !player.connected {
                ui.label(RichText::new("offline").color(WEAK));
            }
        }
        if let Some(offered) = &player.offered {
            ui.label(RichText::new(format!("calls itself {offered}")).color(WEAK));
            if !player.refused && ui.small_button("Use this name").clicked() {
                click = Some(Click::UseOffered(player.address));
            }
        }
        if player.refused {
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.button("Let in").clicked() {
                    click = Some(Click::Allow(player.address));
                }
            });
        }
    });
    click
}
