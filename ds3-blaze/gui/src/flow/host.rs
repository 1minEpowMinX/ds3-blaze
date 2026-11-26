//! The host's steps: this PC's network, the hosts entry, the server and the
//! direct link to the guest; and the session the server's state makes up.

use crate::flow::banner::{self, Banner, Tone};
use crate::flow::checks;
use crate::flow::guest::PROMPT;
use crate::flow::link::Link;
use crate::flow::network::Network;
use crate::flow::session::{self, PlayerRow, SessionView};
use crate::flow::step::{self, BUSY, Button, Click, Detail, NOTHING_TO_UNDO, Status, Step};
use ds3_blaze_core::hosts::{self, State};
use ds3_blaze_core::players::{Roster, check_nickname};
use ds3_blaze_core::server::Ports;
use ds3_blaze_core::snapshot::Snapshot;
use ds3_blaze_diag::netcheck::{Finding, Fix};
use ds3_blaze_diag::p2p::{Outcome, Progress};
use ds3_blaze_diag::reach::Report;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::io;
use std::net::Ipv4Addr;
use std::time::Instant;

/// Why the player can't go back while the server runs.
const STOP_FIRST: &str = "Stop hosting first";

const NETWORK: usize = 0;
const HOSTS: usize = 1;
const SERVER: usize = 2;
const LINK: usize = 3;

/// What each step says while it waits, in the order the steps run.
const WAITING: [&str; 4] = [
    "Check this PC's network",
    "Point the game at this PC",
    "Start the server",
    "Check the direct link to the guest",
];

/// What each step says once it is done.
const DONE: [&str; 4] = [
    "This PC's network is ready",
    "The game goes to this PC",
    "The server is running",
    "The direct link works",
];

/// Number of recent log lines the flow keeps.
pub const LOG_LINES: usize = 100;

/// What the open link step says beside "Check again".
const LINK_NOTE: &str = "The guest's Connect starts the same check here";

/// What the host's link step adds when nothing arrived either way.
const SILENT_HINT: &str = "the guest's check runs when the guest presses Connect; \
                           to run both checks at once again, both press Check again";

/// The addresses an exchange has started with, so that each starts one.
#[derive(Clone, Debug, Default)]
pub struct Trigger {
    seen: BTreeSet<Ipv4Addr>,
}

impl Trigger {
    /// Returns whether a connection from `ip` is the first from that address,
    /// never for this machine or `own`.
    pub fn first(&mut self, ip: Ipv4Addr, own: Ipv4Addr) -> bool {
        !ip.is_loopback() && ip != own && self.seen.insert(ip)
    }
}

/// What reaches the host's steps.
#[derive(Debug)]
pub enum HostInput {
    Click(Click),
    /// The check of this PC.
    Network(io::Result<Network>),
    /// The check of the route to the guest.
    Route(io::Result<Network>),
    Hosts(io::Result<State>),
    /// An elevated command line run ended, with its exit code.
    Elevated(io::Result<i32>),
    Started(io::Result<Ports>),
    SelfReach(Vec<Report>),
    /// A Blaze connection arrived from this address.
    Connection(Ipv4Addr),
    Snapshot(Snapshot),
    /// A log line as the console shows it.
    Log(String),
    Progress(Progress),
    Exchange(io::Result<Outcome>),
    /// Time passed; the countdown is drawn again.
    Tick,
}

/// Work the host's steps ask the window to carry out.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HostAction {
    CheckNetwork {
        own: Ipv4Addr,
    },
    Route {
        own: Ipv4Addr,
        peer: Ipv4Addr,
    },
    ReadHosts,
    /// Points the game at this PC through the hosts file.
    WriteHosts,
    StartServer {
        advertise: Ipv4Addr,
        roster: Roster,
    },
    SelfReach {
        own: Ipv4Addr,
        ports: Ports,
    },
    Exchange {
        peer: Ipv4Addr,
    },
    Rename(Ipv4Addr, Option<String>),
    Allow(Ipv4Addr),
    OnlyAllowed(bool),
    /// Keeps the names the host gave.
    SaveNames(BTreeMap<Ipv4Addr, String>),
    /// Applies the fixes in one elevated run.
    ApplyFixes(Vec<Fix>),
    OpenLogs,
    /// Stops the server and points the game back at the EA servers.
    StopAndRestore,
}

/// Which elevated command line run is awaited.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Elevation {
    Hosts,
    Fix,
    Restore,
}

/// The host's steps and what they wait for.
pub struct HostFlow {
    own: Ipv4Addr,
    name: String,
    /// Names the host gave players, by address.
    known: BTreeMap<Ipv4Addr, String>,
    steps: Vec<Step>,
    trigger: Trigger,
    /// The guest the last exchange ran with.
    guest: Option<Ipv4Addr>,
    /// What the network check found on this PC.
    local: Vec<Finding>,
    route: Vec<Finding>,
    /// The error of applying the last fix.
    fix_error: Option<String>,
    link: Link,
    other_vpn: Vec<String>,
    elevation: Option<Elevation>,
    elevation_error: Option<String>,
    /// Whether "Start server" runs its chain of steps.
    starting: bool,
    /// Whether the hosts entry was written during this start.
    wrote_hosts: bool,
    /// Whether the next hosts state is read after restoring the EA servers.
    restoring: bool,
    /// The running server's ports, `None` while no server runs.
    ports: Option<Ports>,
    snapshot: Snapshot,
    log: VecDeque<String>,
    /// Why the name typed last for a player was refused.
    refused_name: Option<String>,
}

impl HostFlow {
    /// Returns the steps of a host at `own` named `name`, who gave the
    /// players in `known` their names, all waiting.
    pub fn new(own: Ipv4Addr, name: String, known: BTreeMap<Ipv4Addr, String>) -> HostFlow {
        let mut flow = HostFlow {
            own,
            name,
            known,
            steps: WAITING.iter().map(|waiting| Step::new(waiting)).collect(),
            trigger: Trigger::default(),
            guest: None,
            local: Vec::new(),
            route: Vec::new(),
            fix_error: None,
            link: Link::default(),
            other_vpn: Vec::new(),
            elevation: None,
            elevation_error: None,
            starting: false,
            wrote_hosts: false,
            restoring: false,
            ports: None,
            snapshot: Snapshot::default(),
            log: VecDeque::new(),
            refused_name: None,
        };
        flow.draw_link(Instant::now());
        flow
    }

    /// Returns every step to waiting and forgets the guest and what the
    /// checks found, as before the first start.
    fn reset(&mut self, now: Instant) {
        self.steps = WAITING.iter().map(|waiting| Step::new(waiting)).collect();
        self.trigger = Trigger::default();
        self.guest = None;
        self.local.clear();
        self.route.clear();
        self.fix_error = None;
        self.link = Link::default();
        self.draw_link(now);
    }

    /// Returns this PC's VPN address.
    pub fn own(&self) -> Ipv4Addr {
        self.own
    }

    /// Returns the title of the host's screen.
    pub fn title(&self) -> String {
        "Hosting".into()
    }

    /// Returns why the player can't go back to the start screen, `None`
    /// while they can: a step runs, or the server runs and has to be
    /// stopped first.
    pub fn stay(&self) -> Option<&'static str> {
        if self.busy() {
            Some(BUSY)
        } else if self.serving() {
            Some(STOP_FIRST)
        } else {
            None
        }
    }

    /// Returns the button that stops the server and restores the EA servers.
    pub fn exit(&self) -> Button {
        Button::elevated("Stop hosting", Click::RestoreHosts).disabled_by(if self.busy() {
            Some(BUSY)
        } else if !self.serving() && self.steps[HOSTS].status == Status::Waiting {
            Some(NOTHING_TO_UNDO)
        } else {
            None
        })
    }

    /// Returns the banner for where the steps and the server stand: the first
    /// rule that matches, in the order of the spec's banner table.
    pub fn banner(&self) -> Banner {
        let start = || Button::elevated("Start server", Click::StartServer);
        let copy = || Button::new("Copy address", Click::CopyAddress);
        if self.link.running() {
            return Banner::new(Tone::Neutral, self.steps[LINK].summary.clone())
                .detail("Don't start the game until the check ends")
                .spinning();
        }
        if self.busy() {
            let doing = self
                .steps
                .iter()
                .find(|step| step.status == Status::Running)
                .map_or(PROMPT, |step| step.summary.as_str());
            return Banner::new(Tone::Neutral, "Starting the server")
                .detail(doing)
                .spinning();
        }
        let failed = [HOSTS, SERVER]
            .into_iter()
            .find(|index| self.steps[*index].status == Status::Failed);
        if let Some(index) = failed {
            let text = match index {
                HOSTS => "The game can't be pointed at this PC",
                _ if self.serving() => "Friends can't reach the server",
                _ => "The server did not start",
            };
            let banner = Banner::new(Tone::Failure, text).detail(self.steps[index].summary.clone());
            return if self.serving() {
                banner
            } else {
                banner.action(start())
            };
        }
        if !self.serving() {
            return Banner::new(Tone::Neutral, "Start the server so friends can join")
                .action(start());
        }
        let players = self.players();
        if let Some(refused) = players.iter().find(|player| player.refused) {
            let detail = match &refused.offered {
                Some(offered) => format!("{}, calls itself {offered}", refused.address),
                None => refused.address.to_string(),
            };
            return Banner::new(Tone::Warning, "Someone wants to join")
                .detail(detail)
                .action(Button::new("Let in", Click::Allow(refused.address)));
        }
        let fixes = step::fixes(&self.steps);
        if !fixes.is_empty() {
            return Banner::new(Tone::Warning, "Almost there")
                .detail(banner::things_to_fix(fixes.len()))
                .action(Button::elevated("Fix it", Click::FixAll));
        }
        let names: Vec<String> = players
            .into_iter()
            .filter(|player| !player.you && player.connected)
            .map(|player| player.name)
            .collect();
        if names.is_empty() {
            return Banner::new(Tone::Neutral, "Waiting for friends")
                .detail(format!("They join at {}", self.own))
                .action(copy());
        }
        Banner::new(
            Tone::Success,
            format!("Playing with {}", names.join(" and ")),
        )
        .detail(format!("Friends join at {}", self.own))
        .action(copy())
    }

    /// Returns whether the setup can fold into one line: the server runs
    /// with every step before the link done, and the link done or waiting
    /// for a guest.
    pub fn setup_done(&self) -> bool {
        self.serving()
            && self.steps[..LINK]
                .iter()
                .all(|step| step.status == Status::Ok)
            && matches!(self.steps[LINK].status, Status::Ok | Status::Waiting)
    }

    /// Returns the steps in the order they run.
    pub fn steps(&self) -> &[Step] {
        &self.steps
    }

    /// Returns what the session shows, `None` while no server runs.
    pub fn session(&self) -> Option<SessionView> {
        self.ports.as_ref()?;
        let room = self.snapshot.rooms.first();
        let room_text = match room {
            // The game started during the check would take its port.
            _ if self.link.running() => "Wait until the check with the guest ends".into(),
            Some(room) => format!(
                "{}, {} of {}",
                session::state_word(room.state),
                room.members.len(),
                room.capacity
            ),
            None => "Start Dead Space 3 and create a game".into(),
        };
        Some(SessionView {
            room: room_text,
            progress: room.and_then(|room| room.checkpoint).map(|checkpoint| {
                format!(
                    "Chapter {}, restore point {}",
                    checkpoint.chapter, checkpoint.restore_point
                )
            }),
            matchmaking: room.map(|room| {
                if room.open_to_matchmaking {
                    "Open"
                } else {
                    "Closed"
                }
            }),
            players: self.players(),
            only_allowed: self.snapshot.roster.only_allowed,
            refused_name: self.refused_name.clone(),
            log: self.log.iter().cloned().collect(),
        })
    }

    /// Returns the host's own row, then every other address the server
    /// knows: connected, then neither, then refused, each group by address.
    fn players(&self) -> Vec<PlayerRow> {
        let snapshot = &self.snapshot;
        let roster = &snapshot.roster;
        let others = |ip: &Ipv4Addr| !ip.is_loopback() && *ip != self.own;
        let mut addresses: BTreeSet<Ipv4Addr> = snapshot
            .sessions
            .iter()
            .map(|session| session.address)
            .filter(others)
            .collect();
        addresses.extend(snapshot.refused.iter().copied().filter(others));
        addresses.extend(roster.offered.keys().copied().filter(others));
        let mut rows: Vec<PlayerRow> = addresses
            .into_iter()
            .map(|address| {
                let name = roster.name_for(address, self.own);
                let refused = snapshot.refused.contains(&address);
                // A refused row shows no name, so it keeps the one sent.
                let offered = roster
                    .offered
                    .get(&address)
                    .filter(|offered| refused || **offered != name)
                    .cloned();
                PlayerRow {
                    address,
                    you: false,
                    name,
                    offered,
                    connected: snapshot.sessions.iter().any(|s| s.address == address),
                    refused,
                }
            })
            .collect();
        rows.sort_by_key(|row| (row.refused, !row.connected, row.address));
        rows.insert(
            0,
            PlayerRow {
                address: self.own,
                you: true,
                name: self.name.clone(),
                offered: None,
                connected: true,
                refused: false,
            },
        );
        rows
    }

    /// Returns whether a step runs or an elevated run is awaited.
    pub fn busy(&self) -> bool {
        self.starting
            || self.elevation.is_some()
            || self.steps.iter().any(|step| step.status == Status::Running)
    }

    /// Returns whether the server runs.
    pub fn serving(&self) -> bool {
        self.ports.is_some()
    }

    /// Takes `input` at `now` and returns the work it asks for.
    pub fn apply(&mut self, input: HostInput, now: Instant) -> Vec<HostAction> {
        match input {
            HostInput::Click(click) => self.click(click, now),
            HostInput::Network(network) => self.network(network, now),
            HostInput::Route(network) => {
                self.route = match network {
                    Ok(network) => {
                        self.other_vpn = network.other_vpn;
                        network.route
                    }
                    Err(err) => vec![Finding {
                        severity: ds3_blaze_diag::netcheck::Severity::Warning,
                        text: format!("the route check failed to run: {err}"),
                        fix: None,
                    }],
                };
                self.draw_link(now);
                Vec::new()
            }
            HostInput::Hosts(state) => self.hosts(state, now),
            HostInput::Elevated(result) => self.elevated(result, now),
            HostInput::Started(result) => self.started(result),
            HostInput::SelfReach(reports) => {
                self.self_reach(&reports);
                Vec::new()
            }
            HostInput::Connection(ip) => self.connection(ip, now),
            HostInput::Snapshot(snapshot) => {
                self.snapshot = snapshot;
                // The link's title names the guest by the server's name.
                self.draw_link(now);
                Vec::new()
            }
            HostInput::Log(line) => {
                if self.log.len() == LOG_LINES {
                    self.log.pop_front();
                }
                self.log.push_back(line);
                Vec::new()
            }
            HostInput::Progress(progress) => {
                self.link.progress(progress);
                self.draw_link(now);
                Vec::new()
            }
            HostInput::Exchange(result) => {
                self.link.finish(result, &self.other_vpn);
                self.draw_link(now);
                Vec::new()
            }
            HostInput::Tick => {
                self.draw_link(now);
                Vec::new()
            }
        }
    }

    fn click(&mut self, click: Click, now: Instant) -> Vec<HostAction> {
        match click {
            Click::StartServer => {
                for (index, waiting) in WAITING.iter().enumerate().take(SERVER + 1) {
                    self.steps[index] = Step::new(waiting);
                }
                self.starting = true;
                self.wrote_hosts = false;
                self.elevation_error = None;
                self.check_network()
            }
            Click::CheckAgain => match self.guest {
                Some(guest) => self.start_exchange(guest, now),
                None => Vec::new(),
            },
            Click::Fix(fix) => self.apply_fixes(vec![fix]),
            Click::FixAll => self.apply_fixes(step::fixes(&self.steps)),
            Click::RestoreHosts => {
                self.elevation = Some(Elevation::Restore);
                self.ports = None;
                self.steps[SERVER] = Step::new(WAITING[SERVER]);
                let hosts = &mut self.steps[HOSTS];
                hosts.status = Status::Running;
                hosts.summary = PROMPT.into();
                vec![HostAction::StopAndRestore]
            }
            Click::Rename(ip, typed) => self.rename(ip, typed),
            Click::UseOffered(ip) => {
                self.known.remove(&ip);
                self.refused_name = None;
                vec![
                    HostAction::Rename(ip, None),
                    HostAction::SaveNames(self.known.clone()),
                ]
            }
            Click::Allow(ip) => vec![HostAction::Allow(ip)],
            Click::OnlyAllowed(on) => vec![HostAction::OnlyAllowed(on)],
            Click::OpenLogs => vec![HostAction::OpenLogs],
            Click::Connect | Click::CopyAddress => Vec::new(),
        }
    }

    /// Takes a name typed for the player at `ip`: an empty one clears the
    /// host's name, which brings the player's own back.
    fn rename(&mut self, ip: Ipv4Addr, typed: String) -> Vec<HostAction> {
        let typed = typed.trim().to_string();
        let name = if typed.is_empty() {
            None
        } else if let Err(reason) = check_nickname(&typed) {
            self.refused_name = Some(format!("{typed:?} is refused: {reason}"));
            return Vec::new();
        } else {
            Some(typed)
        };
        self.refused_name = None;
        match &name {
            Some(name) => self.known.insert(ip, name.clone()),
            None => self.known.remove(&ip),
        };
        vec![
            HostAction::Rename(ip, name),
            HostAction::SaveNames(self.known.clone()),
        ]
    }

    fn check_network(&mut self) -> Vec<HostAction> {
        let step = &mut self.steps[NETWORK];
        step.status = Status::Running;
        step.summary = "Checking this PC's network".into();
        vec![HostAction::CheckNetwork { own: self.own }]
    }

    fn network(&mut self, network: io::Result<Network>, now: Instant) -> Vec<HostAction> {
        let step = &mut self.steps[NETWORK];
        match network {
            Err(err) => {
                step.status = Status::Warning;
                step.summary = format!("The network check failed to run: {err}");
            }
            Ok(network) => {
                step.status = Status::Ok;
                step.summary = DONE[NETWORK].into();
                step.details = vec![Detail::Text(match network.adapter {
                    Some(adapter) => format!("{} on {adapter}", self.own),
                    None => self.own.to_string(),
                })];
                self.local = network.local;
                self.other_vpn = network.other_vpn;
                self.draw_link(now);
            }
        }
        if !self.starting {
            return Vec::new();
        }
        let hosts = &mut self.steps[HOSTS];
        hosts.status = Status::Running;
        hosts.summary = "Reading the hosts file".into();
        vec![HostAction::ReadHosts]
    }

    fn hosts(&mut self, state: io::Result<State>, now: Instant) -> Vec<HostAction> {
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
            let error = self.elevation_error.take();
            if matches!(state, Ok(State::EaServers)) {
                self.reset(now);
                return Vec::new();
            }
            // A failed restore leaves the game going to ds3-blaze, so what the
            // run found still holds.
            step.status = Status::Failed;
            step.summary = checks::sentence(&error.unwrap_or(text));
            return Vec::new();
        }
        if !self.starting {
            return Vec::new();
        }
        match state {
            Ok(State::Redirected(address)) if address == Ipv4Addr::LOCALHOST => {
                step.status = Status::Ok;
                step.summary = DONE[HOSTS].into();
                step.details
                    .insert(0, Detail::Text(format!("hosts → {address}")));
                let server = &mut self.steps[SERVER];
                server.status = Status::Running;
                server.summary = "Starting the server".into();
                vec![HostAction::StartServer {
                    advertise: self.own,
                    roster: Roster {
                        own_name: self.name.clone(),
                        names: self.known.clone(),
                        ..Roster::default()
                    },
                }]
            }
            _ if !self.wrote_hosts => {
                self.wrote_hosts = true;
                self.elevation = Some(Elevation::Hosts);
                step.status = Status::Running;
                step.summary = PROMPT.into();
                vec![HostAction::WriteHosts]
            }
            _ => {
                self.starting = false;
                step.status = Status::Failed;
                step.summary = checks::sentence(&self.elevation_error.take().unwrap_or(text));
                Vec::new()
            }
        }
    }

    fn elevated(&mut self, result: io::Result<i32>, now: Instant) -> Vec<HostAction> {
        self.elevation_error = result.err().map(|err| err.to_string());
        match self.elevation.take() {
            Some(Elevation::Hosts) => vec![HostAction::ReadHosts],
            Some(Elevation::Fix) => match self.elevation_error.take() {
                Some(err) => {
                    self.fix_error = Some(err);
                    self.draw_link(now);
                    Vec::new()
                }
                None => self.check_network(),
            },
            Some(Elevation::Restore) => {
                self.restoring = true;
                vec![HostAction::ReadHosts]
            }
            None => Vec::new(),
        }
    }

    fn started(&mut self, result: io::Result<Ports>) -> Vec<HostAction> {
        self.starting = false;
        let step = &mut self.steps[SERVER];
        match result {
            Err(err) => {
                step.status = Status::Failed;
                step.summary = format!("The server did not start: {err}");
                Vec::new()
            }
            Ok(ports) => {
                self.ports = Some(ports);
                step.status = Status::Running;
                step.summary = "Checking the server".into();
                step.details = vec![Detail::Text(format!(
                    "redirector on TCP {}, Blaze on TCP {}, QoS on TCP {} and UDP {}",
                    ports.redirector, ports.blaze, ports.qos.http, ports.qos.udp
                ))];
                vec![HostAction::SelfReach {
                    own: self.own,
                    ports,
                }]
            }
        }
    }

    fn self_reach(&mut self, reports: &[Report]) {
        let step = &mut self.steps[SERVER];
        step.details.truncate(1);
        step.details.extend(
            reports
                .iter()
                .map(|report| Detail::Finding(checks::report_finding(report))),
        );
        match checks::first_failure(reports) {
            Some(failed) => {
                step.status = Status::Failed;
                step.summary = checks::sentence(&checks::report_finding(failed).text);
            }
            None => {
                step.status = Status::Ok;
                step.summary = DONE[SERVER].into();
            }
        }
    }

    fn connection(&mut self, ip: Ipv4Addr, now: Instant) -> Vec<HostAction> {
        if !self.trigger.first(ip, self.own) {
            return Vec::new();
        }
        self.route.clear();
        let mut actions = vec![HostAction::Route {
            own: self.own,
            peer: ip,
        }];
        actions.extend(self.start_exchange(ip, now));
        actions
    }

    fn apply_fixes(&mut self, fixes: Vec<Fix>) -> Vec<HostAction> {
        if fixes.is_empty() {
            return Vec::new();
        }
        self.elevation = Some(Elevation::Fix);
        self.fix_error = None;
        vec![HostAction::ApplyFixes(fixes)]
    }

    fn start_exchange(&mut self, guest: Ipv4Addr, now: Instant) -> Vec<HostAction> {
        self.guest = Some(guest);
        self.link.start();
        self.draw_link(now);
        vec![HostAction::Exchange { peer: guest }]
    }

    fn draw_link(&mut self, now: Instant) {
        let name = self
            .guest
            .map(|guest| self.snapshot.roster.name_for(guest, self.own));
        let waiting = match &name {
            Some(name) => format!("Check the direct link to {name}"),
            None => WAITING[LINK].into(),
        };
        let mut step = self.link.step(&waiting, DONE[LINK], now, SILENT_HINT);
        let mut findings = self.local.clone();
        findings.extend(self.route.iter().cloned());
        checks::add_findings(&mut step, &findings);
        if self.guest.is_some() || !step.details.is_empty() {
            step.note = Some(LINK_NOTE.into());
        }
        if let (Some(guest), Some(name)) = (self.guest, name) {
            step.details
                .insert(0, Detail::Text(format!("{name} at {guest}")));
        }
        if let Some(error) = &self.fix_error {
            step.details
                .push(Detail::Text(format!("the fix failed: {error}")));
        }
        self.steps[LINK] = step;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ds3_blaze_core::game::SetupReason;
    use ds3_blaze_core::server::Ports;
    use ds3_blaze_core::snapshot::{Checkpoint, Member, Room, RoomState, Session};
    use ds3_blaze_diag::reach::Stage;

    const OWN: Ipv4Addr = Ipv4Addr::new(26, 0, 0, 1);
    const GUEST: Ipv4Addr = Ipv4Addr::new(26, 0, 0, 2);
    const STRANGER: Ipv4Addr = Ipv4Addr::new(26, 0, 0, 3);

    fn flow() -> HostFlow {
        HostFlow::new(OWN, "Host".into(), BTreeMap::new())
    }

    fn session(id: u32, address: Ipv4Addr, name: &str) -> Session {
        Session {
            id,
            address,
            name: name.into(),
            player_id: Some(id),
        }
    }

    /// Returns a flow whose server runs.
    fn serving() -> HostFlow {
        let mut flow = flow();
        flow.apply(HostInput::Started(Ok(Ports::default())), Instant::now());
        flow
    }

    /// Returns a flow whose server runs and answered its own check.
    fn ready() -> HostFlow {
        let mut flow = serving();
        flow.apply(HostInput::SelfReach(Vec::new()), Instant::now());
        flow
    }

    fn refused_stranger() -> HostInput {
        HostInput::Snapshot(Snapshot {
            refused: vec![STRANGER],
            roster: Roster {
                offered: BTreeMap::from([(STRANGER, "Stranger".into())]),
                ..Roster::default()
            },
            ..Snapshot::default()
        })
    }

    fn network_with_a_fix() -> HostInput {
        HostInput::Network(Ok(Network {
            local: vec![Finding {
                severity: ds3_blaze_diag::netcheck::Severity::Problem,
                text: "no game rule".into(),
                fix: Some(Fix::GameRule),
            }],
            ..Network::default()
        }))
    }

    #[test]
    fn host_idle_asks_to_start() {
        let banner = flow().banner();
        assert_eq!(banner.tone, Tone::Neutral);
        assert_eq!(banner.text, "Start the server so friends can join");
        let action = banner.action.expect("an action");
        assert_eq!(
            (action.label, action.click),
            ("Start server", Click::StartServer)
        );
        assert!(action.elevates);
    }

    #[test]
    fn back_waits_for_a_step_and_for_the_server_to_stop() {
        let mut idle = flow();
        assert_eq!(idle.stay(), None);
        idle.apply(HostInput::Click(Click::StartServer), Instant::now());
        assert_eq!(idle.stay(), Some(BUSY));
        assert_eq!(ready().stay(), Some(STOP_FIRST));
    }

    #[test]
    fn host_running_says_what_it_does() {
        let mut flow = flow();
        flow.apply(HostInput::Click(Click::StartServer), Instant::now());
        let banner = flow.banner();
        assert_eq!(banner.text, "Starting the server");
        assert_eq!(banner.detail.as_deref(), Some("Checking this PC's network"));
        assert!(banner.spinner && banner.action.is_none());
    }

    #[test]
    fn host_failure_names_it() {
        let mut flow = flow();
        flow.apply(
            HostInput::Started(Err(io::Error::other("port taken"))),
            Instant::now(),
        );
        let banner = flow.banner();
        assert_eq!(banner.tone, Tone::Failure);
        assert_eq!(banner.text, "The server did not start");
        assert_eq!(banner.detail, Some(flow.steps()[SERVER].summary.clone()));
        assert_eq!(banner.action.expect("an action").click, Click::StartServer);
    }

    #[test]
    fn host_refused_address_asks_to_let_in() {
        let mut flow = ready();
        flow.apply(refused_stranger(), Instant::now());
        let banner = flow.banner();
        assert_eq!(banner.tone, Tone::Warning);
        assert_eq!(banner.text, "Someone wants to join");
        assert_eq!(
            banner.detail,
            Some(format!("{STRANGER}, calls itself Stranger"))
        );
        let action = banner.action.expect("an action");
        assert_eq!(
            (action.label, action.click),
            ("Let in", Click::Allow(STRANGER))
        );
    }

    #[test]
    fn host_fixes_lead_to_fix_it() {
        let mut flow = ready();
        flow.apply(network_with_a_fix(), Instant::now());
        let banner = flow.banner();
        assert_eq!(banner.text, "Almost there");
        assert_eq!(banner.action.expect("an action").click, Click::FixAll);
    }

    #[test]
    fn the_setup_folds_once_the_server_answers() {
        let now = Instant::now();
        let mut flow = flow();
        assert!(!flow.setup_done());
        for input in [
            HostInput::Click(Click::StartServer),
            HostInput::Network(Ok(Network::default())),
            HostInput::Hosts(Ok(State::Redirected(Ipv4Addr::LOCALHOST))),
            HostInput::Started(Ok(Ports::default())),
            HostInput::SelfReach(Vec::new()),
        ] {
            flow.apply(input, now);
        }
        assert!(flow.setup_done());
        flow.apply(HostInput::Connection(GUEST), now);
        assert!(!flow.setup_done());
    }

    #[test]
    fn host_waits_for_friends() {
        let banner = ready().banner();
        assert_eq!(banner.tone, Tone::Neutral);
        assert_eq!(banner.text, "Waiting for friends");
        assert_eq!(banner.detail, Some(format!("They join at {OWN}")));
        assert_eq!(banner.action.expect("an action").click, Click::CopyAddress);
    }

    #[test]
    fn host_plays_with_the_guests() {
        let mut flow = ready();
        flow.apply(
            HostInput::Snapshot(Snapshot {
                sessions: vec![session(2, GUEST, "Carver")],
                roster: Roster {
                    offered: BTreeMap::from([(GUEST, "Carver".into())]),
                    ..Roster::default()
                },
                ..Snapshot::default()
            }),
            Instant::now(),
        );
        let banner = flow.banner();
        assert_eq!(banner.tone, Tone::Success);
        assert_eq!(banner.text, "Playing with Carver");
        assert_eq!(banner.detail, Some(format!("Friends join at {OWN}")));
    }

    #[test]
    fn host_refusal_comes_before_fixes() {
        let mut flow = ready();
        let now = Instant::now();
        flow.apply(network_with_a_fix(), now);
        flow.apply(refused_stranger(), now);
        assert_eq!(flow.banner().text, "Someone wants to join");
    }

    /// Returns the session's rows other than the host's own.
    fn players(flow: &HostFlow) -> Vec<PlayerRow> {
        let session = flow.session().expect("the server runs");
        session.players.into_iter().filter(|row| !row.you).collect()
    }

    #[test]
    fn a_trigger_fires_once_per_guest_address() {
        let mut trigger = Trigger::default();
        assert!(trigger.first(GUEST, OWN));
        assert!(!trigger.first(GUEST, OWN));
        assert!(!trigger.first(Ipv4Addr::LOCALHOST, OWN));
        assert!(!trigger.first(OWN, OWN));
        assert!(trigger.first(STRANGER, OWN));
    }

    #[test]
    fn start_server_checks_the_network_and_the_hosts_entry_first() {
        let mut flow = flow();
        let now = Instant::now();
        assert_eq!(
            flow.apply(HostInput::Click(Click::StartServer), now),
            [HostAction::CheckNetwork { own: OWN }]
        );
        assert_eq!(
            flow.apply(HostInput::Network(Ok(Network::default())), now),
            [HostAction::ReadHosts]
        );
        assert_eq!(
            flow.apply(HostInput::Hosts(Ok(State::EaServers)), now),
            [HostAction::WriteHosts]
        );
        assert_eq!(
            flow.apply(HostInput::Elevated(Ok(0)), now),
            [HostAction::ReadHosts]
        );
        let start = flow.apply(
            HostInput::Hosts(Ok(State::Redirected(Ipv4Addr::LOCALHOST))),
            now,
        );
        let [HostAction::StartServer { advertise, roster }] = start.as_slice() else {
            panic!("{start:?}");
        };
        assert_eq!(*advertise, OWN);
        assert_eq!(roster.own_name, "Host");
        let ports = Ports::default();
        assert_eq!(
            flow.apply(HostInput::Started(Ok(ports)), now),
            [HostAction::SelfReach { own: OWN, ports }]
        );
        assert!(flow.serving());
        flow.apply(
            HostInput::SelfReach(vec![Report {
                stage: Stage::Redirector,
                result: Ok("ok".into()),
            }]),
            now,
        );
        assert_eq!(flow.steps()[SERVER].status, Status::Ok);
        assert_eq!(flow.steps()[SERVER].summary, DONE[SERVER]);
        assert!(flow.steps()[NETWORK].buttons.is_empty());
        assert!(!flow.busy());
    }

    #[test]
    fn waiting_steps_say_what_they_will_do() {
        let summaries: Vec<String> = flow()
            .steps()
            .iter()
            .map(|step| step.summary.clone())
            .collect();
        assert_eq!(summaries, WAITING);
        assert_eq!(flow().exit().disabled, Some(NOTHING_TO_UNDO));
    }

    #[test]
    fn the_first_connection_of_a_guest_starts_one_exchange() {
        let mut flow = serving();
        let now = Instant::now();
        assert_eq!(
            flow.apply(HostInput::Connection(GUEST), now),
            [
                HostAction::Route {
                    own: OWN,
                    peer: GUEST
                },
                HostAction::Exchange { peer: GUEST }
            ]
        );
        assert!(flow.apply(HostInput::Connection(GUEST), now).is_empty());
        assert!(
            flow.apply(HostInput::Connection(Ipv4Addr::LOCALHOST), now)
                .is_empty()
        );
        assert_eq!(
            flow.session().unwrap().room,
            "Wait until the check with the guest ends"
        );
    }

    #[test]
    fn the_session_exists_only_while_the_server_runs() {
        let mut flow = flow();
        let now = Instant::now();
        assert_eq!(flow.session(), None);
        flow.apply(HostInput::Started(Ok(Ports::default())), now);
        assert!(flow.session().is_some());
        flow.apply(HostInput::Click(Click::RestoreHosts), now);
        assert_eq!(flow.session(), None);
    }

    #[test]
    fn the_session_lists_the_connected_before_the_refused() {
        let mut flow = serving();
        flow.apply(
            HostInput::Snapshot(Snapshot {
                sessions: vec![
                    session(1, Ipv4Addr::LOCALHOST, "Host"),
                    session(3, STRANGER, "Partner"),
                ],
                refused: vec![GUEST],
                roster: Roster {
                    offered: BTreeMap::from([(STRANGER, "Partner".into())]),
                    ..Roster::default()
                },
                ..Snapshot::default()
            }),
            Instant::now(),
        );
        let rows = players(&flow);
        assert_eq!(rows.len(), 2);
        assert!(rows[0].connected && !rows[0].refused && rows[0].address == STRANGER);
        assert_eq!(rows[0].offered, None);
        assert!(rows[1].refused && !rows[1].connected && rows[1].address == GUEST);
    }

    #[test]
    fn a_name_set_over_the_guests_own_shows_it_with_use_this_name() {
        let mut flow = HostFlow::new(
            OWN,
            "Host".into(),
            BTreeMap::from([(GUEST, "Partner".into())]),
        );
        let now = Instant::now();
        flow.apply(HostInput::Started(Ok(Ports::default())), now);
        flow.apply(
            HostInput::Snapshot(Snapshot {
                sessions: vec![session(2, GUEST, "Partner")],
                roster: Roster {
                    names: BTreeMap::from([(GUEST, "Partner".into())]),
                    offered: BTreeMap::from([(GUEST, "Carver".into())]),
                    ..Roster::default()
                },
                ..Snapshot::default()
            }),
            now,
        );
        let rows = players(&flow);
        assert_eq!(rows[0].name, "Partner");
        assert_eq!(rows[0].offered.as_deref(), Some("Carver"));
        assert_eq!(
            flow.apply(HostInput::Click(Click::UseOffered(GUEST)), now),
            [
                HostAction::Rename(GUEST, None),
                HostAction::SaveNames(BTreeMap::new())
            ]
        );
    }

    #[test]
    fn renames_save_the_hosts_names_and_an_empty_one_clears_it() {
        let mut flow = serving();
        let now = Instant::now();
        assert_eq!(
            flow.apply(
                HostInput::Click(Click::Rename(GUEST, "Partner".into())),
                now
            ),
            [
                HostAction::Rename(GUEST, Some("Partner".into())),
                HostAction::SaveNames(BTreeMap::from([(GUEST, "Partner".into())]))
            ]
        );
        assert_eq!(
            flow.apply(HostInput::Click(Click::Rename(GUEST, " ".into())), now),
            [
                HostAction::Rename(GUEST, None),
                HostAction::SaveNames(BTreeMap::new())
            ]
        );
        assert!(
            flow.apply(
                HostInput::Click(Click::Rename(GUEST, "Isaac Clarke".into())),
                now
            )
            .is_empty()
        );
        let refused = flow.session().unwrap().refused_name.unwrap();
        assert!(refused.contains("is refused"), "{refused}");
    }

    #[test]
    fn the_link_names_the_guest_by_the_servers_name() {
        let mut flow = serving();
        let now = Instant::now();
        flow.apply(HostInput::Connection(GUEST), now);
        flow.apply(
            HostInput::Snapshot(Snapshot {
                sessions: vec![session(2, GUEST, "Partner")],
                roster: Roster {
                    names: BTreeMap::from([(GUEST, "Partner".into())]),
                    ..Roster::default()
                },
                ..Snapshot::default()
            }),
            now,
        );
        assert!(
            flow.steps()[LINK]
                .details
                .contains(&Detail::Text(format!("Partner at {GUEST}")))
        );
    }

    #[test]
    fn a_running_server_lists_the_host_first() {
        let mut flow = serving();
        let now = Instant::now();
        let session = flow.session().unwrap();
        assert!(session.players[0].you && session.players[0].address == OWN);
        assert!(!session.only_allowed);
        assert_eq!(flow.exit().label, "Stop hosting");
        assert!(!flow.exit().enabled());
        flow.apply(HostInput::SelfReach(Vec::new()), now);
        assert!(flow.exit().enabled());
    }

    #[test]
    fn the_room_reads_the_snapshot() {
        let mut flow = serving();
        let now = Instant::now();
        let view = flow.session().unwrap();
        assert_eq!(view.room, "Start Dead Space 3 and create a game");
        assert_eq!((view.progress, view.matchmaking), (None, None));
        let member = |player_id, name: &str, joined_by| Member {
            player_id,
            name: name.into(),
            connected: true,
            joined_by,
        };
        flow.apply(
            HostInput::Snapshot(Snapshot {
                rooms: vec![Room {
                    id: 1,
                    name: "room".into(),
                    state: RoomState::InGame,
                    open_to_matchmaking: true,
                    capacity: 2,
                    members: vec![
                        member(1, "Host", SetupReason::Create),
                        member(2, "Carver", SetupReason::Matchmaking { session_id: 1 }),
                    ],
                    checkpoint: Some(Checkpoint {
                        chapter: 4,
                        restore_point: 2,
                    }),
                }],
                ..Snapshot::default()
            }),
            now,
        );
        let view = flow.session().unwrap();
        assert_eq!(view.room, "In game, 2 of 2");
        assert_eq!(view.progress.as_deref(), Some("Chapter 4, restore point 2"));
        assert_eq!(view.matchmaking, Some("Open"));
    }

    #[test]
    fn the_session_keeps_the_recent_log_lines() {
        let mut flow = serving();
        let now = Instant::now();
        for number in 0..LOG_LINES + 5 {
            flow.apply(HostInput::Log(format!("line {number}")), now);
        }
        let log = flow.session().unwrap().log;
        assert_eq!(log.len(), LOG_LINES);
        assert_eq!(log.last(), Some(&format!("line {}", LOG_LINES + 4)));
        assert_eq!(
            flow.apply(HostInput::Click(Click::OpenLogs), now),
            [HostAction::OpenLogs]
        );
    }

    #[test]
    fn fix_all_applies_the_fixes_the_rows_offer() {
        let mut flow = flow();
        let now = Instant::now();
        assert!(flow.apply(HostInput::Click(Click::FixAll), now).is_empty());
        flow.apply(
            HostInput::Network(Ok(Network {
                local: vec![Finding {
                    severity: ds3_blaze_diag::netcheck::Severity::Problem,
                    text: "no game rule".into(),
                    fix: Some(Fix::GameRule),
                }],
                ..Network::default()
            })),
            now,
        );
        assert_eq!(
            flow.apply(HostInput::Click(Click::Fix(Fix::GameRule)), now),
            [HostAction::ApplyFixes(vec![Fix::GameRule])]
        );
        assert_eq!(
            flow.apply(HostInput::Click(Click::FixAll), now),
            [HostAction::ApplyFixes(vec![Fix::GameRule])]
        );
    }

    #[test]
    fn stop_and_restore_stops_the_server_and_reads_the_hosts_state() {
        let mut flow = flow();
        let now = Instant::now();
        flow.apply(HostInput::Started(Ok(Ports::default())), now);
        assert_eq!(
            flow.apply(HostInput::Click(Click::RestoreHosts), now),
            [HostAction::StopAndRestore]
        );
        assert!(!flow.serving());
        assert_eq!(
            flow.apply(HostInput::Elevated(Ok(0)), now),
            [HostAction::ReadHosts]
        );
        flow.apply(HostInput::Hosts(Ok(State::EaServers)), now);
        let mut fresh = self::flow();
        fresh.apply(HostInput::Tick, now);
        assert_eq!(flow.steps(), fresh.steps(), "every step waits again");
        assert!(!flow.busy());
    }

    #[test]
    fn a_failed_restore_keeps_what_the_run_found() {
        let mut flow = flow();
        let now = Instant::now();
        flow.apply(HostInput::Started(Ok(Ports::default())), now);
        flow.apply(HostInput::Click(Click::RestoreHosts), now);
        flow.apply(HostInput::Elevated(Ok(0)), now);
        flow.apply(
            HostInput::Hosts(Ok(State::Redirected(Ipv4Addr::LOCALHOST))),
            now,
        );
        assert_eq!(flow.steps()[HOSTS].status, Status::Failed);
    }
}
