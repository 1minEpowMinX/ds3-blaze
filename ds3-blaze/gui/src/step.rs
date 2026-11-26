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
    /// A step that only shows what happened, with no outcome of its own.
    Info,
}

impl Status {
    /// Returns the word of the status pill when the step names none.
    pub fn label(self) -> &'static str {
        match self {
            Status::Waiting => "",
            Status::Running => "running",
            Status::Ok => "ok",
            Status::Warning => "needs fix",
            Status::Failed => "failed",
            Status::Info => "",
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
    /// A checkbox that gives `click` when it is ticked or cleared.
    Toggle {
        label: &'static str,
        on: bool,
        click: Click,
    },
}

/// A player in the host's players step.
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
    /// Every fix the steps' findings offer, behind one prompt.
    FixAll,
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
    pub title: String,
    /// The line a collapsed row shows.
    pub summary: String,
    pub status: Status,
    /// The word of the status pill, in place of the status's own.
    pub pill: Option<&'static str>,
    pub details: Vec<Detail>,
    /// The line beside the buttons under the details.
    pub note: Option<String>,
    pub buttons: Vec<Button>,
}

impl Step {
    /// Returns a waiting step titled `title`, with no summary, details or
    /// buttons.
    pub fn new(title: &str) -> Step {
        Step {
            title: title.to_string(),
            summary: String::new(),
            status: Status::Waiting,
            pill: None,
            details: Vec::new(),
            note: None,
            buttons: Vec::new(),
        }
    }

    /// Returns the word the status pill shows.
    pub fn pill(&self) -> &'static str {
        self.pill.unwrap_or(self.status.label())
    }
}

/// Returns the fix of every finding in `steps` that has one, in the order
/// the rows show them, each fix once.
pub fn fixes(steps: &[Step]) -> Vec<Fix> {
    let mut fixes = Vec::new();
    for step in steps {
        for detail in &step.details {
            if let Detail::Finding(Finding { fix: Some(fix), .. }) = detail
                && !fixes.contains(fix)
            {
                fixes.push(fix.clone());
            }
        }
    }
    fixes
}

/// The line under a mode's steps: what to tell the other player, and the
/// button that ends the mode's changes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Footer {
    pub text: String,
    pub button: Button,
}
