//! Carrying out the work the steps ask for, off the window's thread.

use crate::elevate;
use crate::flow::guest::{GuestAction, GuestInput};
use crate::flow::host::{HostAction, HostInput};
use crate::flow::network;
use ds3_blaze_core::hosts;
use ds3_blaze_core::server::Ports;
use ds3_blaze_diag::p2p::{self, Outcome, Progress, Timing};
use ds3_blaze_diag::reach;
use std::io;
use std::net::{Ipv4Addr, SocketAddr};
use tokio::sync::mpsc;

/// Returns the result of `work` run on a blocking thread.
async fn blocking<T: Send + 'static>(work: impl FnOnce() -> T + Send + 'static) -> io::Result<T> {
    tokio::task::spawn_blocking(work)
        .await
        .map_err(io::Error::other)
}

/// Runs the exchange with `peer` on the game's port, handing each progress
/// report to `progress`, and returns its outcome.
///
/// # Errors
///
/// Returns the error of binding the game's port.
pub async fn exchange(peer: Ipv4Addr, progress: impl Fn(Progress)) -> io::Result<Outcome> {
    let socket = p2p::bind(p2p::GAME_PORT).await?;
    let (sender, mut receiver) = mpsc::unbounded_channel();
    let peer = SocketAddr::from((peer, p2p::GAME_PORT));
    let forward = async {
        while let Some(report) = receiver.recv().await {
            progress(report);
        }
    };
    let (outcome, ()) = tokio::join!(
        p2p::exchange(socket, peer, Timing::default(), sender),
        forward
    );
    Ok(outcome)
}

/// Carries out `action` for the guest's steps and hands each result to
/// `send`.
pub async fn guest(action: GuestAction, send: impl Fn(GuestInput) + Send + Sync) {
    let input = match action {
        GuestAction::CheckNetwork { own, peer } => GuestInput::Network(
            blocking(move || network::check(own, Some(peer), None))
                .await
                .and_then(|result| result),
        ),
        GuestAction::ReadHosts => {
            GuestInput::Hosts(blocking(hosts::status).await.and_then(|result| result))
        }
        GuestAction::WriteHosts(address) => {
            GuestInput::Elevated(elevate::run_cli(elevate::hosts_args(Some(address))).await)
        }
        GuestAction::RestoreHosts => {
            GuestInput::Elevated(elevate::run_cli(elevate::hosts_args(None)).await)
        }
        GuestAction::ApplyFixes(fixes) => {
            GuestInput::Elevated(elevate::run_cli(elevate::fixes_args(&fixes)).await)
        }
        GuestAction::Reach { host, name } => {
            GuestInput::Reach(reach::run(host, host, Ports::default(), Some(&name)).await)
        }
        GuestAction::Exchange { peer } => {
            GuestInput::Exchange(exchange(peer, |report| send(GuestInput::Progress(report))).await)
        }
    };
    send(input);
}

/// Carries out the host's `action` and hands each result to `send`; the
/// actions on the server and the settings are left to the window, which
/// holds them.
pub async fn host(action: HostAction, send: impl Fn(HostInput) + Send + Sync) {
    // The server runs inside the window's process, so its firewall rules are
    // the rules of the running executable.
    let server = std::env::current_exe().ok();
    let input = match action {
        HostAction::CheckNetwork { own } => HostInput::Network(
            blocking(move || network::check(own, None, server))
                .await
                .and_then(|result| result),
        ),
        HostAction::Route { own, peer } => HostInput::Route(
            blocking(move || network::check(own, Some(peer), server))
                .await
                .and_then(|result| result),
        ),
        HostAction::ReadHosts => {
            HostInput::Hosts(blocking(hosts::status).await.and_then(|result| result))
        }
        HostAction::WriteHosts => HostInput::Elevated(
            elevate::run_cli(elevate::hosts_args(Some(Ipv4Addr::LOCALHOST))).await,
        ),
        HostAction::ApplyFixes(fixes) => {
            HostInput::Elevated(elevate::run_cli(elevate::fixes_args(&fixes)).await)
        }
        // The host's game reaches its server through 127.0.0.1, and the
        // guest through the VPN address, which this checks.
        HostAction::SelfReach { own, ports } => {
            HostInput::SelfReach(reach::run(Ipv4Addr::LOCALHOST, own, ports, None).await)
        }
        HostAction::Exchange { peer } => {
            HostInput::Exchange(exchange(peer, |report| send(HostInput::Progress(report))).await)
        }
        HostAction::StartServer { .. }
        | HostAction::Rename(..)
        | HostAction::Allow(_)
        | HostAction::OnlyAllowed(_)
        | HostAction::SaveNames(_)
        | HostAction::OpenLogs
        | HostAction::StopAndRestore => return,
    };
    send(input);
}
