//! Screens filled with made-up results, which a debug build shows when
//! `DS3_BLAZE_PREVIEW` is set, so that each banner can be looked at without
//! a VPN, an administrator prompt or a second PC.

use crate::flow::guest::{GuestFlow, GuestInput};
use crate::flow::host::{HostFlow, HostInput};
use crate::flow::network::Network;
use crate::flow::start::Form;
use crate::flow::step::Click;
use ds3_blaze_core::game::SetupReason;
use ds3_blaze_core::hosts::State;
use ds3_blaze_core::players::Roster;
use ds3_blaze_core::server::Ports;
use ds3_blaze_core::snapshot::{Checkpoint, Member, Room, RoomState, Session, Snapshot};
use ds3_blaze_core::vpn::Adapter;
use ds3_blaze_diag::netcheck::{Finding, Fix, Severity};
use ds3_blaze_diag::p2p::Outcome;
use ds3_blaze_diag::reach::{Report, Stage};
use std::collections::BTreeMap;
use std::net::Ipv4Addr;
use std::time::Instant;

/// Name of the environment variable that picks the preview.
pub const VARIABLE: &str = "DS3_BLAZE_PREVIEW";

pub const HOST: Ipv4Addr = Ipv4Addr::new(26, 0, 0, 1);
pub const GUEST: Ipv4Addr = Ipv4Addr::new(26, 0, 0, 2);
const STRANGER: Ipv4Addr = Ipv4Addr::new(26, 0, 0, 54);

/// The screen a preview shows, named by the value of [`VARIABLE`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Preview {
    /// The start screen with a VPN adapter and Wi-Fi.
    Start,
    /// The host's steps before Start server.
    HostIdle,
    /// The host's server running with a guest in a game and a stranger
    /// refused.
    Host,
    /// The guest's steps before Connect.
    GuestIdle,
    /// The guest's direct link failed on two issues of this PC.
    Guest,
    /// The guest's steps all done.
    GuestReady,
}

impl Preview {
    /// Returns the preview `value` names, the start screen for a value that
    /// names none.
    pub fn parse(value: &str) -> Preview {
        match value {
            "host-idle" => Preview::HostIdle,
            "host" => Preview::Host,
            "guest-idle" => Preview::GuestIdle,
            "guest" => Preview::Guest,
            "guest-ready" => Preview::GuestReady,
            _ => Preview::Start,
        }
    }
}

/// Returns the start screen of a player named Isaac on Radmin VPN, with
/// Wi-Fi beside it.
pub fn start() -> Form {
    let adapter = |name: &str, description: &str, ip| Adapter {
        name: name.into(),
        description: description.into(),
        ipv4: vec![ip],
        up: true,
    };
    Form {
        name: "Isaac".into(),
        address: Some(HOST),
        adapters: vec![
            adapter("Radmin VPN", "Famatech Radmin VPN Ethernet Adapter", HOST),
            adapter("Wi-Fi", "Wireless adapter", Ipv4Addr::new(192, 168, 1, 20)),
        ],
        hosts: Ok(State::EaServers),
        choosing: false,
        all: false,
    }
}

fn finding(severity: Severity, text: &str, fix: Option<Fix>) -> Finding {
    Finding {
        severity,
        text: text.into(),
        fix,
    }
}

fn network(local: Vec<Finding>, peer: Option<Ipv4Addr>) -> Network {
    Network {
        local,
        route: peer
            .map(|peer| {
                let text = format!("the route to {peer} goes through Radmin VPN");
                vec![finding(Severity::Ok, &text, None)]
            })
            .unwrap_or_default(),
        adapter: Some("Radmin VPN".into()),
        other_vpn: Vec::new(),
    }
}

fn reports(nickname: &str) -> Vec<Report> {
    let mut reports: Vec<Report> = [
        Stage::Resolve,
        Stage::Redirector,
        Stage::Blaze,
        Stage::QosHttp,
        Stage::QosUdp,
    ]
    .into_iter()
    .map(|stage| Report {
        stage,
        result: Ok(format!("{stage} answers")),
    })
    .collect();
    reports.push(Report {
        stage: Stage::Nickname,
        result: Ok(nickname.into()),
    });
    reports
}

/// Returns the host's steps with the server running, a guest in a game and a
/// stranger refused.
pub fn host() -> HostFlow {
    let now = Instant::now();
    let mut flow = HostFlow::new(HOST, "Isaac".into(), BTreeMap::new());
    let inputs = [
        HostInput::Click(Click::StartServer),
        HostInput::Network(Ok(network(Vec::new(), None))),
        HostInput::Hosts(Ok(State::Redirected(Ipv4Addr::LOCALHOST))),
        HostInput::Started(Ok(Ports::default())),
        HostInput::SelfReach(reports("").split_off(1)),
        HostInput::Snapshot(Snapshot {
            sessions: vec![Session {
                id: 2,
                address: GUEST,
                name: "Carver".into(),
                player_id: Some(2),
            }],
            refused: vec![STRANGER],
            rooms: vec![Room {
                id: 1,
                name: "Isaac".into(),
                state: RoomState::InGame,
                open_to_matchmaking: true,
                capacity: 2,
                members: vec![
                    Member {
                        player_id: 1,
                        name: "Isaac".into(),
                        connected: true,
                        joined_by: SetupReason::Create,
                    },
                    Member {
                        player_id: 2,
                        name: "Carver".into(),
                        connected: true,
                        joined_by: SetupReason::Matchmaking { session_id: 2 },
                    },
                ],
                checkpoint: Some(Checkpoint {
                    chapter: 4,
                    restore_point: 2,
                }),
            }],
            roster: Roster {
                own_name: "Isaac".into(),
                offered: BTreeMap::from([(GUEST, "Carver".into())]),
                only_allowed: true,
                ..Roster::default()
            },
        }),
        HostInput::Connection(GUEST),
        HostInput::Route(Ok(network(Vec::new(), Some(GUEST)))),
        HostInput::Exchange(Ok(Outcome {
            sent: 30,
            answered: 30,
            received: 30,
            strangers: Vec::new(),
        })),
        HostInput::Log("21:04:33 mesh connected: Isaac and Carver".into()),
    ];
    for input in inputs {
        flow.apply(input, now);
    }
    flow
}

/// Returns the guest's steps with every step done.
pub fn guest_ready() -> GuestFlow {
    let now = Instant::now();
    let mut flow = GuestFlow::new(GUEST, "Carver".into(), HOST);
    let inputs = [
        GuestInput::Click(Click::Connect),
        GuestInput::Network(Ok(network(Vec::new(), Some(HOST)))),
        GuestInput::Hosts(Ok(State::Redirected(HOST))),
        GuestInput::Reach(reports("the host sees you as Carver")),
        GuestInput::Exchange(Ok(Outcome {
            sent: 30,
            answered: 30,
            received: 30,
            strangers: Vec::new(),
        })),
    ];
    for input in inputs {
        flow.apply(input, now);
    }
    flow
}

/// Returns the guest's steps with the host reached and the direct link
/// failed on two issues of this PC.
pub fn guest() -> GuestFlow {
    let now = Instant::now();
    let mut flow = GuestFlow::new(GUEST, "Carver".into(), HOST);
    let local = vec![
        finding(
            Severity::Warning,
            "no inbound rule allows UDP 3659 on Radmin VPN",
            Some(Fix::GameRule),
        ),
        finding(
            Severity::Warning,
            "the Radmin VPN network is Public",
            Some(Fix::PrivateProfile(GUEST)),
        ),
    ];
    let inputs = [
        GuestInput::Click(Click::Connect),
        GuestInput::Network(Ok(network(local, Some(HOST)))),
        GuestInput::Hosts(Ok(State::Redirected(HOST))),
        GuestInput::Reach(reports("the host sees you as Carver")),
        GuestInput::Exchange(Ok(Outcome {
            sent: 30,
            ..Outcome::default()
        })),
    ];
    for input in inputs {
        flow.apply(input, now);
    }
    flow
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_preview_has_a_name_and_others_show_the_start_screen() {
        for (value, preview) in [
            ("host-idle", Preview::HostIdle),
            ("host", Preview::Host),
            ("guest-idle", Preview::GuestIdle),
            ("guest", Preview::Guest),
            ("guest-ready", Preview::GuestReady),
            ("start", Preview::Start),
            ("nonsense", Preview::Start),
        ] {
            assert_eq!(Preview::parse(value), preview, "{value}");
        }
        assert!(guest_ready().setup_done());
        assert!(host().setup_done());
    }
}
