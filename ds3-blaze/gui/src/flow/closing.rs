//! Closing the window while the hosts file still sends the game to
//! ds3-blaze: the close waits until the player says whether to restore the
//! EA servers first, and until that restore ends.

use crate::flow::start::points_at_ds3_blaze;
use ds3_blaze_core::hosts::State;
use std::io;

/// Where closing the window stands.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Closing {
    /// No close was asked for.
    #[default]
    No,
    /// The dialog asks whether to restore the EA servers first.
    Asking,
    /// The EA servers are being restored before the window closes.
    Restoring,
    /// The next close request goes through.
    Allowed,
}

/// What the player answered in the closing dialog.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Answer {
    Restore,
    Close,
    Stay,
}

/// What an answer asks the window to do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Effect {
    /// Stop the server and restore the EA servers, elevated.
    RestoreHosts,
    Close,
}

/// What a restore of the EA servers left behind.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Restored {
    /// Whether the restore ran and the hosts file, as far as it can be read,
    /// no longer holds ds3-blaze entries.
    pub done: bool,
    /// What the window says about the restore, `None` when all went well.
    pub message: Option<String>,
}

impl Closing {
    /// Takes a close request and returns whether it is held back; `points`
    /// tells whether the hosts file holds ds3-blaze entries.
    pub fn request(&mut self, points: bool) -> bool {
        match *self {
            Closing::Allowed => false,
            Closing::Restoring => true,
            Closing::No if points => {
                *self = Closing::Asking;
                true
            }
            Closing::Asking => points,
            Closing::No => false,
        }
    }

    /// Takes the player's answer while the dialog asks and returns what it
    /// asks the window to do.
    pub fn answer(&mut self, answer: Answer) -> Option<Effect> {
        if *self != Closing::Asking {
            return None;
        }
        match answer {
            Answer::Restore => {
                *self = Closing::Restoring;
                Some(Effect::RestoreHosts)
            }
            Answer::Close => {
                *self = Closing::Allowed;
                Some(Effect::Close)
            }
            Answer::Stay => {
                *self = Closing::No;
                None
            }
        }
    }

    /// Takes the end of a restore and returns whether the window closes now:
    /// it does when a restore started by the dialog is `done`.
    pub fn restore_ended(&mut self, done: bool) -> bool {
        if *self != Closing::Restoring {
            return false;
        }
        *self = if done { Closing::Allowed } else { Closing::No };
        done
    }
}

/// Returns what a restore that ended with `result` left behind, `state`
/// being the hosts file read after it; the exit code alone is not trusted.
pub fn restored(result: &io::Result<i32>, state: &io::Result<State>) -> Restored {
    let still = state.as_ref().is_ok_and(points_at_ds3_blaze);
    let message = match (result, state) {
        (Err(err), _) => Some(err.to_string()),
        (Ok(_), Ok(state)) if still => Some(format!("The EA servers were not restored: {state}")),
        (Ok(_), Err(err)) => Some(format!("The hosts file cannot be read: {err}")),
        (Ok(_), Ok(_)) => None,
    };
    Restored {
        done: result.is_ok() && !still,
        message,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    const HOST: Ipv4Addr = Ipv4Addr::new(26, 0, 0, 1);

    #[test]
    fn a_close_goes_through_while_the_game_goes_to_ea() {
        let mut closing = Closing::No;
        assert!(!closing.request(false));
        assert_eq!(closing, Closing::No);
    }

    #[test]
    fn a_close_waits_for_the_answer_while_the_game_goes_to_ds3_blaze() {
        let mut closing = Closing::No;
        assert!(closing.request(true));
        assert_eq!(closing, Closing::Asking);
        assert!(closing.request(true), "a second request is held too");
        assert_eq!(closing.answer(Answer::Stay), None);
        assert_eq!(closing, Closing::No);
    }

    #[test]
    fn restoring_holds_the_close_until_it_ends() {
        let mut closing = Closing::No;
        closing.request(true);
        assert_eq!(closing.answer(Answer::Restore), Some(Effect::RestoreHosts));
        assert!(
            closing.request(false),
            "a restore under way holds any close"
        );
        assert!(closing.restore_ended(true));
        assert_eq!(closing, Closing::Allowed);
        assert!(!closing.request(true));
    }

    #[test]
    fn a_failed_restore_keeps_the_window_open() {
        let mut closing = Closing::No;
        closing.request(true);
        closing.answer(Answer::Restore);
        assert!(!closing.restore_ended(false));
        assert_eq!(closing, Closing::No);
    }

    #[test]
    fn closing_without_restoring_lets_the_next_request_through() {
        let mut closing = Closing::No;
        closing.request(true);
        assert_eq!(closing.answer(Answer::Close), Some(Effect::Close));
        assert!(!closing.request(true));
    }

    #[test]
    fn a_restore_from_the_start_screen_closes_nothing() {
        let mut closing = Closing::No;
        assert!(!closing.restore_ended(true));
        assert_eq!(closing.answer(Answer::Restore), None);
    }

    #[test]
    fn a_restore_is_judged_by_the_hosts_file() {
        let back = restored(&Ok(0), &Ok(State::EaServers));
        assert_eq!(
            back,
            Restored {
                done: true,
                message: None
            }
        );
        let still = restored(&Ok(0), &Ok(State::Redirected(HOST)));
        assert!(!still.done);
        assert!(
            still
                .message
                .unwrap()
                .starts_with("The EA servers were not restored")
        );
        let refused = restored(&Err(io::Error::other("cancelled")), &Ok(State::EaServers));
        assert_eq!(refused.message.as_deref(), Some("cancelled"));
        assert!(!refused.done);
    }
}
