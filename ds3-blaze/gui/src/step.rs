//! A step of a mode as the window shows it: the data a row reads, and the
//! clicks it can give back.

use ds3_blaze_diag::netcheck::{Finding, Fix};
use std::net::Ipv4Addr;

/// Why a button is disabled while a step runs or a prompt is awaited.
pub const BUSY: &str = "Wait until the current step ends";

/// Why the button that ends a mode's changes is disabled before any change.
pub const NOTHING_TO_UNDO: &str = "Nothing to undo yet";

/// Where a step stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Waiting,
    Running,
    Ok,
    Warning,
    Failed,
}

/// One line of a step's details.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Detail {
    Text(String),
    /// A check's result, with its fix button when it has one.
    Finding(Finding),
}

/// A button the window draws.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Button {
    pub label: &'static str,
    pub click: Click,
    /// Whether pressing it brings up the administrator prompt.
    pub elevates: bool,
    /// Why the button is disabled, `None` while it is enabled.
    pub disabled: Option<&'static str>,
}

impl Button {
    /// Returns an enabled button that runs nothing elevated.
    pub fn new(label: &'static str, click: Click) -> Button {
        Button {
            label,
            click,
            elevates: false,
            disabled: None,
        }
    }

    /// Returns an enabled button that brings up the administrator prompt.
    pub fn elevated(label: &'static str, click: Click) -> Button {
        Button {
            elevates: true,
            ..Button::new(label, click)
        }
    }

    /// Returns the button that applies `fix`.
    pub fn fix(fix: &Fix) -> Button {
        let label = match fix {
            Fix::GameRule | Fix::ServerRule(_) => "Add rule",
            Fix::PrivateProfile(_) => "Make private",
        };
        Button::elevated(label, Click::Fix(fix.clone()))
    }

    /// Returns the button disabled for `reason` when one is given.
    pub fn disabled_by(self, reason: Option<&'static str>) -> Button {
        Button {
            disabled: reason,
            ..self
        }
    }

    /// Returns whether the button can be pressed.
    pub fn enabled(&self) -> bool {
        self.disabled.is_none()
    }
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
    /// The sentence the row shows: what the step will do while it waits,
    /// what is now true once it is done.
    pub summary: String,
    pub status: Status,
    pub details: Vec<Detail>,
    /// The line beside the buttons under the details.
    pub note: Option<String>,
    pub buttons: Vec<Button>,
}

impl Step {
    /// Returns a waiting step saying `summary`, with no details or buttons.
    pub fn new(summary: &str) -> Step {
        Step {
            summary: summary.to_string(),
            status: Status::Waiting,
            details: Vec::new(),
            note: None,
            buttons: Vec::new(),
        }
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
