//! The guest's steps: from the route to the host, through the hosts entry
//! and the host's server, to the direct link and the game.

use crate::checks;
use crate::link::Link;
use crate::network::Network;
use crate::step::{Button, Click, Detail, Footer, Status, Step};
use ds3_blaze_core::hosts::{self, State};
use ds3_blaze_diag::netcheck::{Finding, Fix, Severity};
use ds3_blaze_diag::p2p::{Outcome, Progress};
use ds3_blaze_diag::reach::{Report, Stage};
use std::io;
use std::net::Ipv4Addr;
use std::time::Instant;

const ADDRESS: usize = 0;
const HOSTS: usize = 1;
const REACH: usize = 2;
const LINK: usize = 3;
const GAME: usize = 4;

/// Titles of the steps, in the order they run.
const TITLES: [&str; 5] = [
    "Host's VPN address",
    "Point game at host",
    "Reach host's server",
    "Direct link to host",
    "Start Dead Space 3",
];

/// What the open link step says beside "Check again".
const LINK_NOTE: &str = "The host's window runs the same check at the same time";

/// What the guest's link step adds when nothing arrived either way.
const SILENT_HINT: &str = "if the host's game is running, it holds the port the check needs: \
                           the host closes the game, then both press Check again";

/// What reaches the guest's steps.
#[derive(Debug)]
pub enum GuestInput {
    Click(Click),
    Network(io::Result<Network>),
    Hosts(io::Result<State>),
    /// An elevated command line run ended, with its exit code.
    Elevated(io::Result<i32>),
    Reach(Vec<Report>),
    Progress(Progress),
    Exchange(io::Result<Outcome>),
    /// Time passed; the countdown is drawn again.
    Tick,
}

/// Work the guest's steps ask the window to carry out.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GuestAction {
    CheckNetwork { own: Ipv4Addr, peer: Ipv4Addr },
    ReadHosts,
    WriteHosts(Ipv4Addr),
    Reach { host: Ipv4Addr, name: String },
    Exchange { peer: Ipv4Addr },
    ApplyFix(Fix),
    RestoreHosts,
}

/// Which elevated command line run is awaited.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Elevation {
    Hosts,
    Fix,
    Restore,
}

/// The guest's steps and what they wait for.
pub struct GuestFlow {
    own: Ipv4Addr,
    name: String,
    host: Ipv4Addr,
    steps: Vec<Step>,
    link: Link,
    /// What the network check found, shown with the direct link.
    findings: Vec<Finding>,
    /// The error of applying the last fix.
    fix_error: Option<String>,
    /// What the host's server said about the guest's name.
    nickname: Option<Result<String, String>>,
    other_vpn: Vec<String>,
    elevation: Option<Elevation>,
    /// The error of the last elevated run, shown if the state it was to
    /// change did not change.
    elevation_error: Option<String>,
    /// Whether the hosts entry was written during this connection.
    wrote_hosts: bool,
    /// Whether the next hosts state is read after restoring the EA servers.
    restoring: bool,
}

impl GuestFlow {
    /// Returns the steps of a guest at `own` named `name` joining the host at
    /// `host`, all waiting.
    pub fn new(own: Ipv4Addr, name: String, host: Ipv4Addr) -> GuestFlow {
        let mut flow = GuestFlow {
            own,
            name,
            host,
            steps: Vec::new(),
            link: Link::default(),
            findings: Vec::new(),
            fix_error: None,
            nickname: None,
            other_vpn: Vec::new(),
            elevation: None,
            elevation_error: None,
            wrote_hosts: false,
            restoring: false,
        };
        flow.reset(Instant::now());
        flow
    }

    /// Sets every step back to waiting.
    fn reset(&mut self, now: Instant) {
        self.steps = TITLES.iter().map(|title| Step::new(title)).collect();
        self.link = Link::default();
        self.findings.clear();
        self.fix_error = None;
        self.nickname = None;
        self.draw_link(now);
        self.draw_game();
        self.draw_buttons();
    }

    /// Returns the line under the steps: the name the host's server gives
    /// the guest, and the button that restores the EA servers.
    pub fn footer(&self) -> Footer {
        let text = match &self.nickname {
            Some(Ok(text)) => capitalised(text),
            _ => format!("You join as {}", self.name),
        };
        Footer {
            text,
            button: Button {
                label: "Disconnect and restore hosts",
                click: Click::RestoreHosts,
                enabled: !self.busy() && self.steps[HOSTS].status != Status::Waiting,
            },
        }
    }

    /// Returns the steps in the order they run.
    pub fn steps(&self) -> &[Step] {
        &self.steps
    }

    /// Returns whether a step runs or an elevated run is awaited.
    pub fn busy(&self) -> bool {
        self.elevation.is_some() || self.steps.iter().any(|step| step.status == Status::Running)
    }

    /// Takes `input` at `now` and returns the work it asks for.
    pub fn apply(&mut self, input: GuestInput, now: Instant) -> Vec<GuestAction> {
        let actions = self.take(input, now);
        self.draw_buttons();
        actions
    }

    fn take(&mut self, input: GuestInput, now: Instant) -> Vec<GuestAction> {
        match input {
            GuestInput::Click(click) => self.click(click, now),
            GuestInput::Network(network) => self.network(network, now),
            GuestInput::Hosts(state) => self.hosts(state),
            GuestInput::Elevated(result) => self.elevated(result, now),
            GuestInput::Reach(reports) => self.reach(&reports, now),
            GuestInput::Progress(progress) => {
                self.link.progress(progress);
                self.draw_link(now);
                Vec::new()
            }
            GuestInput::Exchange(result) => {
                self.link.finish(result, &self.other_vpn);
                self.draw_link(now);
                self.draw_game();
                Vec::new()
            }
            GuestInput::Tick => {
                self.draw_link(now);
                Vec::new()
            }
        }
    }

    fn click(&mut self, click: Click, now: Instant) -> Vec<GuestAction> {
        match click {
            Click::Connect => {
                self.reset(now);
                self.wrote_hosts = false;
                self.elevation_error = None;
                self.check_network()
            }
            Click::CheckAgain => self.start_exchange(now),
            Click::Fix(fix) => {
                self.elevation = Some(Elevation::Fix);
                self.fix_error = None;
                vec![GuestAction::ApplyFix(fix)]
            }
            Click::RestoreHosts => {
                self.elevation = Some(Elevation::Restore);
                self.steps[HOSTS].status = Status::Running;
                self.steps[HOSTS].summary = "waiting for the administrator prompt".into();
                vec![GuestAction::RestoreHosts]
            }
            _ => Vec::new(),
        }
    }

    fn check_network(&mut self) -> Vec<GuestAction> {
        let step = &mut self.steps[ADDRESS];
        step.status = Status::Running;
        step.summary = "reading this PC's network settings, a few seconds".into();
        vec![GuestAction::CheckNetwork {
            own: self.own,
            peer: self.host,
        }]
    }

    fn network(&mut self, network: io::Result<Network>, now: Instant) -> Vec<GuestAction> {
        let step = &mut self.steps[ADDRESS];
        match network {
            Err(err) => {
                // The later steps show the route's effect even unchecked.
                step.status = Status::Warning;
                step.summary = format!("the network check failed to run: {err}");
            }
            Ok(network) => {
                let mut findings = network.local.clone();
                findings.extend(network.route.iter().cloned());
                self.other_vpn = network.other_vpn;
                // A wrong route would carry the hosts entry, reach and the
                // exchange alike, so it stops them.
                if let Some(problem) = network
                    .route
                    .iter()
                    .find(|finding| finding.severity == Severity::Problem)
                {
                    step.status = Status::Failed;
                    step.summary = problem.text.clone();
                    self.findings = findings;
                    self.draw_link(now);
                    return Vec::new();
                }
                step.status = Status::Ok;
                step.summary = match network.adapter {
                    Some(adapter) => {
                        format!("{} reaches {} through {adapter}", self.own, self.host)
                    }
                    None => format!("{} reaches {}", self.own, self.host),
                };
                self.findings = findings;
                self.draw_link(now);
            }
        }
        let hosts = &mut self.steps[HOSTS];
        hosts.status = Status::Running;
        hosts.summary = "reading the hosts file".into();
        vec![GuestAction::ReadHosts]
    }

    fn hosts(&mut self, state: io::Result<State>) -> Vec<GuestAction> {
        let text = match &state {
            Ok(state) => state.to_string(),
            Err(err) => format!("the hosts file cannot be read: {err}"),
        };
        let step = &mut self.steps[HOSTS];
        step.details = vec![
            Detail::Text(text.clone()),
            Detail::Text(format!("backup: {}", hosts::backup_path().display())),
        ];
        if std::mem::take(&mut self.restoring) {
            let restored = matches!(state, Ok(State::EaServers));
            let error = self.elevation_error.take();
            step.status = if restored {
                Status::Waiting
            } else {
                Status::Failed
            };
            step.summary = match error {
                Some(error) if !restored => error,
                _ => text,
            };
            return Vec::new();
        }
        match state {
            Ok(State::Redirected(address)) if address == self.host => {
                step.status = Status::Ok;
                step.pill = Some("done");
                step.summary = format!("hosts → {}", self.host);
                let reach = &mut self.steps[REACH];
                reach.status = Status::Running;
                reach.summary = "asking the host's server".into();
                vec![GuestAction::Reach {
                    host: self.host,
                    name: self.name.clone(),
                }]
            }
            _ if !self.wrote_hosts => {
                self.wrote_hosts = true;
                self.elevation = Some(Elevation::Hosts);
                step.status = Status::Running;
                step.summary = "waiting for the administrator prompt".into();
                vec![GuestAction::WriteHosts(self.host)]
            }
            _ => {
                step.status = Status::Failed;
                step.summary = self.elevation_error.take().unwrap_or(text);
                Vec::new()
            }
        }
    }

    fn elevated(&mut self, result: io::Result<i32>, now: Instant) -> Vec<GuestAction> {
        self.elevation_error = result.err().map(|err| err.to_string());
        match self.elevation.take() {
            Some(Elevation::Hosts) => vec![GuestAction::ReadHosts],
            Some(Elevation::Fix) => match self.elevation_error.take() {
                Some(err) => {
                    self.fix_error = Some(err);
                    self.draw_link(now);
                    Vec::new()
                }
                None => self.check_network(),
            },
            Some(Elevation::Restore) => {
                // Restoring ends the connection: the later steps start over.
                self.steps[REACH] = Step::new(TITLES[REACH]);
                self.link = Link::default();
                self.nickname = None;
                self.draw_link(now);
                self.draw_game();
                self.restoring = true;
                vec![GuestAction::ReadHosts]
            }
            None => Vec::new(),
        }
    }

    fn reach(&mut self, reports: &[Report], now: Instant) -> Vec<GuestAction> {
        let findings: Vec<_> = reports.iter().map(checks::report_finding).collect();
        let step = &mut self.steps[REACH];
        step.details = checks::details(&findings);
        if let Some(failed) = checks::first_failure(reports) {
            step.status = Status::Failed;
            step.summary = checks::report_finding(failed).text;
            return Vec::new();
        }
        let nickname = reports
            .iter()
            .find(|report| report.stage == Stage::Nickname);
        self.nickname = nickname.map(|report| report.result.clone());
        step.status = match nickname {
            Some(Report { result: Err(_), .. }) => Status::Warning,
            _ => Status::Ok,
        };
        step.summary = match nickname.map(|report| &report.result) {
            Some(Ok(text) | Err(text)) => format!("all services answer; {text}"),
            None => "all services answer".into(),
        };
        self.start_exchange(now)
    }

    fn start_exchange(&mut self, now: Instant) -> Vec<GuestAction> {
        self.link.start();
        self.draw_link(now);
        self.draw_game();
        vec![GuestAction::Exchange { peer: self.host }]
    }

    fn draw_link(&mut self, now: Instant) {
        let mut step = self.link.step(TITLES[LINK], now, SILENT_HINT);
        checks::add_findings(&mut step, &self.findings);
        if self.link.running() || !step.details.is_empty() {
            step.note = Some(LINK_NOTE.into());
        }
        if let Some(error) = &self.fix_error {
            step.details
                .push(Detail::Text(format!("the fix failed: {error}")));
        }
        self.steps[LINK] = step;
    }

    /// Sets the button of the address step for the current state.
    fn draw_buttons(&mut self) {
        let idle = !self.busy();
        self.steps[ADDRESS].buttons = vec![Button {
            label: "Connect",
            click: Click::Connect,
            enabled: idle,
        }];
    }

    fn draw_game(&mut self) {
        let step = &mut self.steps[GAME];
        *step = Step::new(TITLES[GAME]);
        if self.link.running() {
            step.summary = "wait until the direct link check ends".into();
        } else if self.link.works() {
            step.status = Status::Ok;
            step.summary = "start the game and play online co-op".into();
            step.details = vec![
                Detail::Text("a public game: the host creates it, then choose quick match".into()),
                Detail::Text("a private game: accept the host's invite in the EA App".into()),
            ];
        } else if self.link.failed() {
            step.status = Status::Warning;
            step.pill = Some("at risk");
            step.summary = "the direct link failed, so co-op may not connect".into();
        } else {
            step.summary = "online co-op".into();
        }
    }
}

/// Returns `text` with its first letter in upper case.
fn capitalised(text: &str) -> String {
    let mut letters = text.chars();
    match letters.next() {
        Some(first) => first.to_uppercase().chain(letters).collect(),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ds3_blaze_diag::netcheck::Finding;
    use std::time::Duration;

    const OWN: Ipv4Addr = Ipv4Addr::new(26, 0, 0, 2);
    const HOST: Ipv4Addr = Ipv4Addr::new(26, 0, 0, 1);

    fn flow() -> GuestFlow {
        GuestFlow::new(OWN, "Carver".into(), HOST)
    }

    fn finding(severity: Severity, text: &str) -> Finding {
        Finding {
            severity,
            text: text.into(),
            fix: None,
        }
    }

    fn clean_network() -> GuestInput {
        GuestInput::Network(Ok(Network {
            local: vec![finding(Severity::Ok, "VPN adapter is up")],
            route: vec![finding(Severity::Ok, "the route takes the VPN")],
            adapter: Some("Radmin VPN".into()),
            other_vpn: Vec::new(),
        }))
    }

    fn passing(stage: Stage) -> Report {
        Report {
            stage,
            result: Ok(format!("{stage} answers")),
        }
    }

    fn reports(nickname: Result<String, String>) -> Vec<Report> {
        let mut reports: Vec<Report> = [
            Stage::Resolve,
            Stage::Redirector,
            Stage::Blaze,
            Stage::QosHttp,
            Stage::QosUdp,
        ]
        .into_iter()
        .map(passing)
        .collect();
        reports.push(Report {
            stage: Stage::Nickname,
            result: nickname,
        });
        reports
    }

    /// Returns a flow that went through Connect up to the reach stage.
    fn reaching(now: Instant) -> GuestFlow {
        let mut flow = flow();
        flow.apply(GuestInput::Click(Click::Connect), now);
        flow.apply(clean_network(), now);
        flow.apply(GuestInput::Hosts(Ok(State::Redirected(HOST))), now);
        flow
    }

    #[test]
    fn connect_checks_the_network_first() {
        let mut flow = flow();
        assert_eq!(
            flow.apply(GuestInput::Click(Click::Connect), Instant::now()),
            [GuestAction::CheckNetwork {
                own: OWN,
                peer: HOST
            }]
        );
        assert_eq!(flow.steps()[ADDRESS].status, Status::Running);
    }

    #[test]
    fn a_route_problem_stops_the_steps_after_it() {
        let mut flow = flow();
        let now = Instant::now();
        flow.apply(GuestInput::Click(Click::Connect), now);
        let actions = flow.apply(
            GuestInput::Network(Ok(Network {
                route: vec![finding(Severity::Problem, "the route takes Wi-Fi")],
                ..Network::default()
            })),
            now,
        );
        assert!(actions.is_empty());
        assert_eq!(flow.steps()[ADDRESS].status, Status::Failed);
        assert_eq!(flow.steps()[ADDRESS].summary, "the route takes Wi-Fi");
        assert_eq!(flow.steps()[HOSTS].status, Status::Waiting);
    }

    #[test]
    fn the_hosts_entry_is_written_once_and_read_back() {
        let mut flow = flow();
        let now = Instant::now();
        flow.apply(GuestInput::Click(Click::Connect), now);
        assert_eq!(flow.apply(clean_network(), now), [GuestAction::ReadHosts]);
        assert_eq!(
            flow.apply(GuestInput::Hosts(Ok(State::EaServers)), now),
            [GuestAction::WriteHosts(HOST)]
        );
        assert_eq!(
            flow.apply(GuestInput::Elevated(Ok(0)), now),
            [GuestAction::ReadHosts]
        );
        assert!(
            flow.apply(GuestInput::Hosts(Ok(State::EaServers)), now)
                .is_empty()
        );
        assert_eq!(flow.steps()[HOSTS].status, Status::Failed);
        assert_eq!(flow.steps()[HOSTS].summary, State::EaServers.to_string());
    }

    #[test]
    fn a_declined_prompt_is_what_the_hosts_step_says() {
        let mut flow = flow();
        let now = Instant::now();
        flow.apply(GuestInput::Click(Click::Connect), now);
        flow.apply(clean_network(), now);
        flow.apply(GuestInput::Hosts(Ok(State::EaServers)), now);
        let declined = io::Error::new(
            io::ErrorKind::PermissionDenied,
            "the administrator prompt was declined",
        );
        flow.apply(GuestInput::Elevated(Err(declined)), now);
        flow.apply(GuestInput::Hosts(Ok(State::EaServers)), now);
        assert_eq!(
            flow.steps()[HOSTS].summary,
            "the administrator prompt was declined"
        );
    }

    #[test]
    fn the_hosts_entry_in_place_asks_for_reach_with_the_name() {
        let mut flow = flow();
        let now = Instant::now();
        flow.apply(GuestInput::Click(Click::Connect), now);
        flow.apply(clean_network(), now);
        assert_eq!(
            flow.apply(GuestInput::Hosts(Ok(State::Redirected(HOST))), now),
            [GuestAction::Reach {
                host: HOST,
                name: "Carver".into()
            }]
        );
        assert_eq!(flow.steps()[HOSTS].summary, format!("hosts → {HOST}"));
    }

    #[test]
    fn a_failed_nickname_warns_and_the_exchange_still_starts() {
        let now = Instant::now();
        let mut flow = reaching(now);
        let actions = flow.apply(
            GuestInput::Reach(reports(Err("the host refused the name: x".into()))),
            now,
        );
        assert_eq!(actions, [GuestAction::Exchange { peer: HOST }]);
        assert_eq!(flow.steps()[REACH].status, Status::Warning);
        assert!(flow.steps()[REACH].summary.contains("refused the name"));
        assert!(
            flow.steps()[GAME]
                .summary
                .starts_with("wait until the direct link check ends")
        );
    }

    #[test]
    fn network_issues_show_with_the_direct_link() {
        let mut flow = flow();
        let now = Instant::now();
        flow.apply(GuestInput::Click(Click::Connect), now);
        flow.apply(
            GuestInput::Network(Ok(Network {
                local: vec![finding(Severity::Warning, "the network is Public")],
                route: vec![finding(Severity::Ok, "the route takes the VPN")],
                adapter: Some("Radmin VPN".into()),
                other_vpn: Vec::new(),
            })),
            now,
        );
        assert_eq!(flow.steps()[ADDRESS].status, Status::Ok);
        let link = &flow.steps()[LINK];
        assert_eq!(link.status, Status::Warning);
        assert_eq!(link.summary, "1 issue found");
        assert_eq!(link.details.len(), 2);
    }

    #[test]
    fn the_footer_names_the_guest_as_the_host_sees_it() {
        let now = Instant::now();
        let mut flow = reaching(now);
        assert_eq!(flow.footer().text, "You join as Carver");
        // The button waits for the running step.
        assert!(!flow.footer().button.enabled);
        flow.apply(
            GuestInput::Reach(reports(Ok("the host sees you as Carver".into()))),
            now,
        );
        assert_eq!(flow.footer().text, "The host sees you as Carver");
    }

    #[test]
    fn the_name_the_host_sees_is_in_the_summary() {
        let now = Instant::now();
        let mut flow = reaching(now);
        flow.apply(
            GuestInput::Reach(reports(Ok("the host sees you as Carver".into()))),
            now,
        );
        assert_eq!(flow.steps()[REACH].status, Status::Ok);
        assert_eq!(
            flow.steps()[REACH].summary,
            "all services answer; the host sees you as Carver"
        );
    }

    #[test]
    fn a_failed_stage_stops_the_exchange() {
        let now = Instant::now();
        let mut flow = reaching(now);
        let mut failing = reports(Ok(String::new()));
        failing[2].result = Err("no answer".into());
        assert!(flow.apply(GuestInput::Reach(failing), now).is_empty());
        assert_eq!(flow.steps()[REACH].status, Status::Failed);
        assert_eq!(flow.steps()[LINK].status, Status::Waiting);
    }

    #[test]
    fn the_link_counts_down_and_a_silent_end_says_to_close_the_game() {
        let now = Instant::now();
        let mut flow = reaching(now);
        flow.apply(GuestInput::Reach(reports(Ok(String::new()))), now);
        flow.apply(
            GuestInput::Progress(Progress::Pinging {
                target: (HOST, 3659).into(),
                until: tokio::time::Instant::from_std(now + Duration::from_secs(20)),
            }),
            now,
        );
        flow.apply(GuestInput::Tick, now + Duration::from_secs(5));
        assert!(flow.steps()[LINK].summary.contains("15 s left"));
        flow.apply(
            GuestInput::Exchange(Ok(Outcome {
                sent: 20,
                ..Outcome::default()
            })),
            now,
        );
        let link = &flow.steps()[LINK];
        assert_eq!(link.status, Status::Failed);
        assert!(link.details.contains(&Detail::Text(SILENT_HINT.into())));
        assert_eq!(flow.steps()[GAME].status, Status::Warning);
        assert_eq!(
            flow.apply(GuestInput::Click(Click::CheckAgain), now),
            [GuestAction::Exchange { peer: HOST }]
        );
    }

    #[test]
    fn a_working_link_names_both_ways_into_the_game() {
        let now = Instant::now();
        let mut flow = reaching(now);
        flow.apply(GuestInput::Reach(reports(Ok(String::new()))), now);
        flow.apply(
            GuestInput::Exchange(Ok(Outcome {
                sent: 5,
                answered: 5,
                received: 5,
                strangers: Vec::new(),
            })),
            now,
        );
        let game = &flow.steps()[GAME];
        assert_eq!(game.status, Status::Ok);
        assert_eq!(game.details.len(), 2);
    }

    #[test]
    fn restoring_the_ea_servers_ends_the_connection() {
        let now = Instant::now();
        let mut flow = reaching(now);
        flow.apply(GuestInput::Reach(reports(Ok(String::new()))), now);
        assert_eq!(
            flow.apply(GuestInput::Click(Click::RestoreHosts), now),
            [GuestAction::RestoreHosts]
        );
        assert_eq!(
            flow.apply(GuestInput::Elevated(Ok(0)), now),
            [GuestAction::ReadHosts]
        );
        assert!(
            flow.apply(GuestInput::Hosts(Ok(State::EaServers)), now)
                .is_empty()
        );
        assert_eq!(flow.steps()[HOSTS].summary, State::EaServers.to_string());
        assert_eq!(flow.steps()[HOSTS].status, Status::Waiting);
        assert_eq!(flow.steps()[LINK].status, Status::Waiting);
        assert!(!flow.busy());
    }

    #[test]
    fn a_fix_runs_elevated_then_the_check_again() {
        let mut flow = flow();
        let now = Instant::now();
        assert_eq!(
            flow.apply(GuestInput::Click(Click::Fix(Fix::GameRule)), now),
            [GuestAction::ApplyFix(Fix::GameRule)]
        );
        assert_eq!(
            flow.apply(GuestInput::Elevated(Ok(0)), now),
            [GuestAction::CheckNetwork {
                own: OWN,
                peer: HOST
            }]
        );
    }
}
