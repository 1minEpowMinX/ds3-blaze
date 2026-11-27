//! `reach` against a server started in this process on free loopback ports.

use ds3_blaze_core::players::Roster;
use ds3_blaze_core::server::{Config, Ports, Server, ServerHandle};
use ds3_blaze_diag::reach::{self, Stage};
use std::net::Ipv4Addr;

/// Starts a server on free loopback ports that hands out `advertise`.
async fn started(advertise: Ipv4Addr) -> ServerHandle {
    Server::start(Config {
        bind: Ipv4Addr::LOCALHOST,
        advertise,
        ports: Ports::ANY,
        players: Roster {
            own_name: "Host".into(),
            ..Default::default()
        },
    })
    .await
    .expect("the server starts")
}

#[tokio::test]
async fn server_on_the_checked_address_passes_every_stage() {
    let server = started(Ipv4Addr::LOCALHOST).await;
    let reports = reach::server(Ipv4Addr::LOCALHOST, server.ports(), None).await;
    let stages: Vec<Stage> = reports.iter().map(|report| report.stage).collect();
    assert_eq!(
        stages,
        [
            Stage::Redirector,
            Stage::Blaze,
            Stage::QosHttp,
            Stage::QosUdp
        ]
    );
    for report in &reports {
        assert!(report.result.is_ok(), "{report:?}");
    }
}

#[tokio::test]
async fn server_advertising_another_address_fails_the_redirector_and_skips_blaze() {
    // A documentation address, which nothing answers.
    let elsewhere = Ipv4Addr::new(192, 0, 2, 1);
    let server = started(elsewhere).await;
    let reports = reach::server(Ipv4Addr::LOCALHOST, server.ports(), None).await;
    let redirector = reports[0].result.as_ref().expect_err("a failure");
    assert!(redirector.contains("192.0.2.1"), "{redirector}");
    let blaze = reports[1].result.as_ref().expect_err("not tried");
    assert!(blaze.starts_with("not tried"), "{blaze}");
    assert!(reports[2].result.is_ok(), "{:?}", reports[2]);
    assert!(reports[3].result.is_ok(), "{:?}", reports[3]);
}

#[tokio::test]
async fn a_name_adds_the_nickname_stage() {
    let server = started(Ipv4Addr::LOCALHOST).await;
    let reports = reach::server(Ipv4Addr::LOCALHOST, server.ports(), Some("Carver")).await;
    let nickname = reports.last().expect("a report");
    assert_eq!(reports.len(), 5);
    assert_eq!(nickname.stage, Stage::Nickname);
    // The check runs from loopback, which the server names own_name.
    let text = nickname.result.as_ref().expect("a pass");
    assert!(text.starts_with("the host calls you Host;"), "{text}");
}
