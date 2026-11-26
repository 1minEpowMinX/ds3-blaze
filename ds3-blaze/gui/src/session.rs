//! The host's session while the server runs: the room, the players and the
//! server's recent log lines.

use crate::row::{GREEN, GREY, ORANGE, RED};
use crate::step::Click;
use ds3_blaze_core::snapshot::RoomState;
use eframe::egui::{self, Align2, Color32, Label, Margin, RichText, Sense, TextStyle, Vec2, vec2};
use std::net::Ipv4Addr;

/// Number of log lines the session shows.
pub const SESSION_LOG_LINES: usize = 6;

/// Width of a guest's name field, in points.
const NAME_WIDTH: f32 = 170.0;

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
    /// The name the player sent, when it differs from `name`.
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
    ui.add_space(16.0);
    heading(ui, "This game");
    tiles(
        ui,
        [
            ("Room", Some(view.room.as_str())),
            ("Progress", view.progress.as_deref()),
            ("Matchmaking", view.matchmaking),
        ],
    );
    ui.add_space(14.0);
    heading(ui, "Players");
    for player in &view.players {
        if let Some(row_click) = player_row(ui, player) {
            click = Some(row_click);
        }
    }
    if let Some(refused) = &view.refused_name {
        ui.label(RichText::new(refused).color(RED));
    }
    let mut on = view.only_allowed;
    if ui
        .checkbox(&mut on, "Only let in players I approve")
        .changed()
    {
        click = Some(Click::OnlyAllowed(!view.only_allowed));
    }
    ui.add_space(14.0);
    let (_, open_logs) = egui::Sides::new().show(
        ui,
        |ui| heading(ui, "Server log"),
        |ui| ui.button("Open logs folder").clicked(),
    );
    if open_logs {
        click = Some(Click::OpenLogs);
    }
    let start = view.log.len().saturating_sub(SESSION_LOG_LINES);
    egui::Frame::new()
        .fill(ui.visuals().faint_bg_color)
        .corner_radius(8.0)
        .inner_margin(Margin::same(10))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            if view.log.is_empty() {
                ui.label(RichText::new("Nothing yet").weak());
            }
            for line in &view.log[start..] {
                ui.add(
                    Label::new(RichText::new(line).text_style(TextStyle::Monospace).weak())
                        .truncate(),
                );
            }
        });
    click
}

/// Draws the title of a part of the session.
fn heading(ui: &mut egui::Ui, text: &str) {
    ui.label(
        RichText::new(text)
            .strong()
            .color(ui.visuals().strong_text_color()),
    );
    ui.add_space(4.0);
}

/// Draws `tiles` side by side, each a caption over its value or "–".
fn tiles(ui: &mut egui::Ui, tiles: [(&str, Option<&str>); 3]) {
    let spacing = ui.spacing().item_spacing.x;
    let width = (ui.available_width() - spacing * 2.0) / 3.0;
    ui.horizontal_top(|ui| {
        for (caption, value) in tiles {
            egui::Frame::new()
                .fill(ui.visuals().faint_bg_color)
                .corner_radius(8.0)
                .inner_margin(Margin::same(10))
                .show(ui, |ui| {
                    ui.set_width(width - 20.0);
                    ui.vertical(|ui| {
                        ui.label(RichText::new(caption).small().weak());
                        ui.add(Label::new(value.unwrap_or("–")).wrap());
                    });
                });
        }
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
    ui.horizontal(|ui| {
        ui.set_min_height(30.0);
        if player.you {
            dot(ui, GREEN);
            ui.label(RichText::new(&player.name).color(ui.visuals().strong_text_color()));
            ui.label(RichText::new("(you)").weak());
            return;
        }
        if player.refused {
            dot(ui, ORANGE);
            ui.label(format!("Someone at {} was turned away", player.address));
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
            ui.label(RichText::new(player.address.to_string()).weak());
            if !player.connected {
                ui.label(RichText::new("offline").weak());
            }
        }
        if let Some(offered) = &player.offered {
            ui.label(RichText::new(format!("calls itself {offered}")).weak());
            if !player.refused && ui.small_button("Use this name").clicked() {
                click = Some(Click::UseOffered(player.address));
            }
        }
        if player.refused && ui.button("Let in").clicked() {
            click = Some(Click::Allow(player.address));
        }
    });
    click
}
