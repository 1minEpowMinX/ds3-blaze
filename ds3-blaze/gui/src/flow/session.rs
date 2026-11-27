//! What the host's session shows while the server runs: the room, the
//! players and the server's recent log lines.

use ds3_blaze_core::snapshot::RoomState;
use std::net::Ipv4Addr;

/// Number of log lines the session shows.
pub const SESSION_LOG_LINES: usize = 6;

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
