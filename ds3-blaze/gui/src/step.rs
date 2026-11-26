//! A step of a mode as the window shows it: the data a row reads, and the
//! clicks it can give back.

use ds3_blaze_diag::netcheck::{Finding, Fix};
use std::net::Ipv4Addr;

/// Where a step stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Waiting,
    Running,
    Ok,
    Warning,
    Failed,
}

impl Status {
    /// Returns the word the status pill shows.
    pub fn label(self) -> &'static str {
        match self {
            Status::Waiting => "waiting",
            Status::Running => "running",
            Status::Ok => "ok",
            Status::Warning => "warning",
            Status::Failed => "failed",
        }
    }
}

/// One line of a step's details.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Detail {
    Text(String),
    /// A check's result, with its fix button when it has one.
    Finding(Finding),
    /// A player the host's server knows.
    Player(PlayerRow),
}

/// A player in the host's players step.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlayerRow {
    pub address: Ipv4Addr,
    /// The name the host's server gives the player.
    pub name: String,
    /// The name the player sent, when it differs from `name`.
    pub offered: Option<String>,
    pub connected: bool,
    /// Whether the server refuses the player's connections.
    pub refused: bool,
}

/// A button on a step's line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Button {
    pub label: &'static str,
    pub click: Click,
    pub enabled: bool,
}

/// Every action a row's buttons and fields can ask for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Click {
    Connect,
    StartServer,
    RestoreHosts,
    Fix(Fix),
    CheckAgain,
    Allow(Ipv4Addr),
    /// A name typed for a player; an empty one clears the host's name.
    Rename(Ipv4Addr, String),
    /// The player's own name taken in place of the host's.
    UseOffered(Ipv4Addr),
    OnlyAllowed(bool),
    OpenLogs,
}

/// A step of a mode.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Step {
    pub title: &'static str,
    /// The line a collapsed row shows.
    pub summary: String,
    pub status: Status,
    pub details: Vec<Detail>,
    pub buttons: Vec<Button>,
}

impl Step {
    /// Returns a waiting step titled `title`, with no summary, details or
    /// buttons.
    pub fn new(title: &'static str) -> Step {
        Step {
            title,
            summary: String::new(),
            status: Status::Waiting,
            details: Vec::new(),
            buttons: Vec::new(),
        }
    }
}
