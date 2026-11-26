//! The direct link step: the exchange with the other player on the game's
//! port, its countdown and its outcome.

use crate::step::{Button, Click, Detail, Status, Step};
use ds3_blaze_diag::p2p::{self, Outcome, Progress};
use std::io;
use std::time::Instant;

/// Where the exchange with the other player stands.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Link {
    /// Whether an exchange runs.
    running: bool,
    /// When the running exchange stops at the latest.
    until: Option<Instant>,
    peer_reaches_this_pc: bool,
    this_pc_reaches_peer: bool,
    /// The last exchange's counters, or the error that kept it from running.
    result: Option<Result<Outcome, String>>,
    /// What the last exchange found wrong, `None` when both directions work.
    failure: Option<String>,
}

impl Link {
    /// Marks an exchange as started.
    pub fn start(&mut self) {
        *self = Link {
            running: true,
            ..Link::default()
        };
    }

    /// Returns whether an exchange runs.
    pub fn running(&self) -> bool {
        self.running
    }

    /// Returns whether the last exchange ended with both directions working.
    pub fn works(&self) -> bool {
        !self.running && matches!(self.result, Some(Ok(_))) && self.failure.is_none()
    }

    /// Returns whether the last exchange ended without both directions
    /// working.
    pub fn failed(&self) -> bool {
        !self.running && self.result.is_some() && !self.works()
    }

    /// Takes a progress report of the running exchange.
    pub fn progress(&mut self, progress: Progress) {
        match progress {
            Progress::Pinging { until, .. } => self.until = Some(until.into_std()),
            Progress::PeerReachesThisPc => self.peer_reaches_this_pc = true,
            Progress::ThisPcReachesPeer => self.this_pc_reaches_peer = true,
        }
    }

    /// Takes the end of the exchange; `other_vpn` names the VPN adapters
    /// that may drop its traffic.
    pub fn finish(&mut self, result: io::Result<Outcome>, other_vpn: &[String]) {
        self.running = false;
        self.until = None;
        let names: Vec<&str> = other_vpn.iter().map(String::as_str).collect();
        self.failure = match &result {
            Ok(outcome) => outcome.failure(&names),
            Err(err) => Some(err.to_string()),
        };
        self.result = Some(result.map_err(|err| err.to_string()));
    }

    /// Returns the step as the exchange stands at `now`: saying `waiting`
    /// before any exchange and `done` once both directions work, with the
    /// counters and a failure's text in the details.
    ///
    /// `silent_hint` is added when nothing arrived either way.
    pub fn step(&self, waiting: &str, done: &str, now: Instant, silent_hint: &str) -> Step {
        let mut step = Step::new(waiting);
        let mut directions = Vec::new();
        if self.peer_reaches_this_pc {
            directions.push(Detail::Text("the other player reaches this PC".into()));
        }
        if self.this_pc_reaches_peer {
            directions.push(Detail::Text("this PC reaches the other player".into()));
        }
        if self.running {
            step.status = Status::Running;
            step.summary = match self.until {
                Some(until) => format!(
                    "Checking the direct link, {} s left",
                    until.saturating_duration_since(now).as_secs()
                ),
                None => "Checking the direct link".into(),
            };
            step.details = directions;
            return step;
        }
        let Some(result) = &self.result else {
            return step;
        };
        step.buttons
            .push(Button::new("Check again", Click::CheckAgain));
        step.details = directions;
        if let Ok(outcome) = result {
            step.details.push(Detail::Text(format!(
                "sent {} pings, {} answered; {} pings received from the other player",
                outcome.sent, outcome.answered, outcome.received
            )));
            for stranger in &outcome.strangers {
                step.details.push(Detail::Text(format!(
                    "packets also came from {stranger}, not from the other player"
                )));
            }
        }
        match &self.failure {
            None => {
                step.status = Status::Ok;
                step.summary = done.into();
                step.details
                    .insert(0, Detail::Text(format!("UDP {} both ways", p2p::GAME_PORT)));
            }
            Some(failure) => {
                step.status = Status::Failed;
                step.summary = "The direct link failed".into();
                step.details.insert(0, Detail::Text(failure.clone()));
                let silent =
                    matches!(result, Ok(outcome) if outcome.answered == 0 && outcome.received == 0);
                if silent {
                    step.details.push(Detail::Text(silent_hint.into()));
                }
            }
        }
        step
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn the_running_step_counts_down() {
        let now = Instant::now();
        let mut link = Link::default();
        link.start();
        link.progress(Progress::Pinging {
            target: "26.0.0.1:3659".parse().unwrap(),
            until: tokio::time::Instant::from_std(now + Duration::from_secs(20)),
        });
        let step = link.step("Check", "Works", now, "");
        assert_eq!(step.status, Status::Running);
        assert!(step.summary.contains("20 s left"), "{}", step.summary);
        let later = link.step("Check", "Works", now + Duration::from_secs(5), "");
        assert!(later.summary.contains("15 s left"), "{}", later.summary);
    }

    #[test]
    fn a_silent_exchange_fails_with_the_hint() {
        let mut link = Link::default();
        link.start();
        link.finish(
            Ok(Outcome {
                sent: 30,
                ..Outcome::default()
            }),
            &[],
        );
        let step = link.step("Check", "Works", Instant::now(), "close the game");
        assert_eq!(step.status, Status::Failed);
        assert_eq!(step.summary, "The direct link failed");
        assert!(
            step.details
                .contains(&Detail::Text("close the game".into()))
        );
        assert_eq!(step.buttons[0].click, Click::CheckAgain);
        assert!(!link.works());
    }

    #[test]
    fn both_directions_make_a_working_link() {
        let mut link = Link::default();
        link.start();
        link.finish(
            Ok(Outcome {
                sent: 5,
                answered: 5,
                received: 5,
                strangers: Vec::new(),
            }),
            &[],
        );
        assert!(link.works());
        let step = link.step("Check", "Works", Instant::now(), "");
        assert_eq!(step.status, Status::Ok);
        assert_eq!(step.summary, "Works");
        assert_eq!(
            step.details[0],
            Detail::Text(format!("UDP {} both ways", p2p::GAME_PORT))
        );
        assert_eq!(
            Link::default()
                .step("Check", "Works", Instant::now(), "")
                .summary,
            "Check"
        );
    }
}
