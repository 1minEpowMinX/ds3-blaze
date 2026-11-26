//! The sentence on top of a mode's screen: where things stand and the one
//! thing to do next.

use crate::flow::step::Button;

/// How a banner is coloured.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    Neutral,
    Success,
    Warning,
    Failure,
}

/// The sentence on top of a mode's screen and its one action.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Banner {
    pub tone: Tone,
    pub text: String,
    /// The second line, `None` when one line says it all.
    pub detail: Option<String>,
    /// The main button; `None` while a step runs or when nothing is to do.
    pub action: Option<Button>,
    /// Whether a spinner stands in for the button.
    pub spinner: bool,
    /// Whether the banner holds the guest's address field.
    pub input: bool,
}

impl Banner {
    /// Returns a banner of `tone` saying `text`, with no detail, action,
    /// spinner or input.
    pub fn new(tone: Tone, text: impl Into<String>) -> Banner {
        Banner {
            tone,
            text: text.into(),
            detail: None,
            action: None,
            spinner: false,
            input: false,
        }
    }

    /// Returns the banner with `detail` as its second line.
    pub fn detail(self, detail: impl Into<String>) -> Banner {
        Banner {
            detail: Some(detail.into()),
            ..self
        }
    }

    /// Returns the banner with `button` as its action.
    pub fn action(self, button: Button) -> Banner {
        Banner {
            action: Some(button),
            ..self
        }
    }

    /// Returns the banner with a spinner in place of an action.
    pub fn spinning(self) -> Banner {
        Banner {
            spinner: true,
            action: None,
            ..self
        }
    }

    /// Returns the banner holding the guest's address field.
    pub fn with_input(self) -> Banner {
        Banner {
            input: true,
            ..self
        }
    }
}

/// Returns how many things to fix `count` names, in the player's words.
pub fn things_to_fix(count: usize) -> String {
    match count {
        1 => "1 thing to fix on this PC".into(),
        count => format!("{count} things to fix on this PC"),
    }
}
