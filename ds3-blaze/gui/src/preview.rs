//! Steps filled with made-up results, which a debug build shows when
//! `DS3_BLAZE_PREVIEW` names a mode, so that the rows can be looked at
//! without a VPN, an administrator prompt or a second PC.

use crate::guest::{GuestFlow, GuestInput};
use crate::host::{HostFlow, HostInput};
use crate::network::Network;
use crate::step::Click;
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

/// Returns the VPN adapter the preview's player chooses from.
pub fn adapters(own: Ipv4Addr) -> Vec<Adapter> {
    vec![Adapter {
        name: "Radmin VPN".into(),
        description: "Famatech Radmin VPN Ethernet Adapter".into(),
        ipv4: vec![own],
        up: true,
    }]
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
