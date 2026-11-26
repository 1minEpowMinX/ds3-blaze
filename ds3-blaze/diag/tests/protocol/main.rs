//! Runs the game's client sequence against a server started in this process.

mod client;

use ds3_blaze_core::server::{Config, Ports, Server};
use std::net::Ipv4Addr;

#[tokio::test]
async fn client_sequence_passes() {
    let server = Server::start(Config {
        bind: Ipv4Addr::LOCALHOST,
        advertise: Ipv4Addr::LOCALHOST,
        ports: Ports::ANY,
        player_name: "Host".into(),
        player_names: Vec::new(),
        only_listed: false,
    })
    .await
    .expect("the server starts");
    let ports = server.ports();
    client::run(
        Ipv4Addr::LOCALHOST,
        ports.redirector,
        ports.qos.http,
        ports.qos.udp,
    )
    .await
    .expect("the client sequence passes");
}
