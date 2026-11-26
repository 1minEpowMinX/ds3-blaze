//! State of a running server at one moment, as a window shows it.

use crate::game::SetupReason;
use crate::players::Roster;
use std::net::Ipv4Addr;

/// Sessions, refused addresses, rooms and player book of a server.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Snapshot {
    /// Open Blaze connections in the order they were accepted.
    pub sessions: Vec<Session>,
    /// Addresses whose connections were refused and that the player book still
    /// does not admit, in the order of their first refusal.
    pub refused: Vec<Ipv4Addr>,
    /// Games by id.
    pub rooms: Vec<Room>,
    pub roster: Roster,
}

/// One open Blaze connection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Session {
    /// Number of the connection, as in the `blaze#N` log tag.
    pub id: u32,
    pub address: Ipv4Addr,
    /// Name the player logged in with, or the name the player book gives the
    /// address before login.
    pub name: String,
    /// Id of the logged-in player, `None` before login.
    pub player_id: Option<u32>,
}

/// One game.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Room {
    pub id: u32,
    /// Game name (`GNAM`).
    pub name: String,
    pub state: RoomState,
    /// Whether the settings let quick match place players in the game; the
    /// game's public rooms set it and its private rooms do not.
    pub open_to_matchmaking: bool,
    /// Number of player slots.
    pub capacity: usize,
    /// Members, the host first.
    pub members: Vec<Member>,
    /// Chapter and restore point from the `GAME_CHECKPOINT` attribute.
    pub checkpoint: Option<Checkpoint>,
}

/// Game state (`GSTA`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RoomState {
    Initializing,
    PreGame,
    InGame,
    /// A state value this server does not name.
    Other(u8),
}

/// One member of a game.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Member {
    pub player_id: u32,
    pub name: String,
    /// Whether the member reported its mesh connection; a joining member is
    /// still connecting until then.
    pub connected: bool,
    /// How the member entered the game.
    pub joined_by: SetupReason,
}

/// Position in the campaign a game was created or last updated at.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Checkpoint {
    pub chapter: u32,
    pub restore_point: u32,
}

impl Checkpoint {
    /// Returns the chapter and restore point a `GAME_CHECKPOINT` value starts
    /// with, as two eight-digit hexadecimal numbers, or `None` when it does not.
    pub fn parse(value: &str) -> Option<Self> {
        let number = |range| {
            let digits = value.get(range)?;
            u32::from_str_radix(digits, 16).ok()
        };
        Some(Self {
            chapter: number(0..8)?,
            restore_point: number(8..16)?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checkpoint_reads_chapter_and_restore_point() {
        assert_eq!(
            Checkpoint::parse("000000010000000AX_0000000_0000000_v3"),
            Some(Checkpoint {
                chapter: 1,
                restore_point: 10
            })
        );
    }

    #[test]
    fn checkpoint_without_two_numbers_is_none() {
        assert_eq!(Checkpoint::parse(""), None);
        assert_eq!(Checkpoint::parse("00000001"), None);
        assert_eq!(Checkpoint::parse("0000000100000Z0A"), None);
        assert_eq!(Checkpoint::parse("00000001000000\u{e9}"), None);
    }
}
