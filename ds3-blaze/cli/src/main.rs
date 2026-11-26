//! Command line of ds3-blaze: the local Blaze server for Dead Space 3 co-op and
//! the network checks between two players.
//!
//! The commands and their options are declared in [`Command`].

#[cfg(feature = "research")]
mod proxy;

use clap::{Args, Parser, Subcommand};
use ds3_blaze_core::event::{self, Event};
use ds3_blaze_core::log;
use ds3_blaze_core::players::Roster;
use ds3_blaze_core::server::{self, DEFAULT_BLAZE_PORT, Server};
use ds3_blaze_diag::{netcheck, p2pcheck};

use std::io;
use std::net::Ipv4Addr;
use std::process::ExitCode;
use tokio::sync::broadcast::error::{RecvError, TryRecvError};

/// Display name used when `--name` is not given.
const DEFAULT_NAME: &str = "DS3Player";

/// Local Blaze server for Dead Space 3 co-op over VPN.
#[derive(Parser)]
#[command(name = env!("CARGO_BIN_NAME"), version, arg_required_else_help = true)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Runs the redirector, the Blaze server and the QoS service.
    Serve(ServeArgs),
    /// Shows the adapter traffic to the peer takes, other VPN adapters, the
    /// network profile and the Windows Firewall rules for the game and this
    /// server.
    NetCheck {
        /// The other player's VPN address.
        #[arg(long, value_name = "IP")]
        peer: Ipv4Addr,
    },
    /// Runs net-check, then exchanges UDP pings with the other player; run it
    /// on both PCs at once.
    P2pCheck(P2pArgs),
    /// Relays the game to EA's Blaze servers and logs every message.
    #[cfg(feature = "research")]
    Proxy(EaArgs),
    /// Checks the SSLv3 connection to EA's redirector and main server.
    #[cfg(feature = "research")]
    EaCheck(EaArgs),
}

#[derive(Args)]
struct ServeArgs {
    /// Address all listeners bind to.
    #[arg(long, value_name = "IP", default_value_t = Ipv4Addr::LOCALHOST)]
    bind: Ipv4Addr,
    /// Address the game is told to reach this server at.
    #[arg(long, value_name = "IP", default_value_t = Ipv4Addr::LOCALHOST)]
    advertise: Ipv4Addr,
    /// Port of the main Blaze server.
    #[arg(long, value_name = "PORT", default_value_t = DEFAULT_BLAZE_PORT)]
    blaze_port: u16,
    /// Display name of the player on this machine.
    #[arg(long, value_name = "NAME", default_value = DEFAULT_NAME, value_parser = non_empty)]
    name: String,
    /// Display name of the player connecting from IP; others get Player-<last
    /// octet of their IP>.
    #[arg(long = "player", value_name = "IP=NAME", value_parser = player)]
    players: Vec<(Ipv4Addr, String)>,
    /// Refuses Blaze connections from addresses other than this machine,
    /// --advertise and the --player entries.
    #[arg(long)]
    only_listed: bool,
}

#[derive(Args)]
struct P2pArgs {
    /// The other player's VPN address.
    #[arg(long, value_name = "IP")]
    peer: Ipv4Addr,
    /// UDP port; the default is the game's P2P port.
    #[arg(long, value_name = "PORT", default_value_t = p2pcheck::GAME_PORT)]
    port: u16,
    /// Longest run time in seconds.
    #[arg(long, value_name = "N", default_value_t = p2pcheck::DEFAULT_SECONDS)]
    seconds: u64,
}

#[cfg(feature = "research")]
#[derive(Args)]
struct EaArgs {
    /// EA's redirector; the default is the address gosredirector.ea.com had.
    #[arg(long, value_name = "IP:PORT", default_value_t = proxy::EA_REDIRECTOR)]
    ea_redirector: std::net::SocketAddr,
}

/// Returns `name` when it holds more than white space.
fn non_empty(name: &str) -> Result<String, String> {
    if name.trim().is_empty() {
        Err("the name is empty".into())
    } else {
        Ok(name.to_string())
    }
}

/// Returns the address and the name of an `IP=NAME` entry.
fn player(entry: &str) -> Result<(Ipv4Addr, String), String> {
    let (ip, name) = entry.split_once('=').ok_or("expected IP=NAME")?;
    let ip = ip.parse().map_err(|err| format!("{ip}: {err}"))?;
    Ok((ip, non_empty(name)?))
}

#[cfg(feature = "research")]
async fn run_proxy(ea_redirector: std::net::SocketAddr) -> io::Result<()> {
    use ds3_blaze_core::server::REDIRECTOR_PORT;
    use std::net::SocketAddr;

    let path = log::init()?;
    log!("main", "log file: {}", path.display());
    log!(
        "main",
        "proxy mode: the log holds the account's data from EA, keep it private"
    );
    proxy::run(
        SocketAddr::from((Ipv4Addr::LOCALHOST, REDIRECTOR_PORT)),
        SocketAddr::from((Ipv4Addr::LOCALHOST, DEFAULT_BLAZE_PORT)),
        ea_redirector,
    )
    .await
}

async fn serve(args: ServeArgs) -> io::Result<()> {
    let path = log::init()?;
    log!("main", "log file: {}", path.display());
    log!(
        "main",
        "advertising {} (Blaze port {}), player name \"{}\"",
        args.advertise,
        args.blaze_port,
        args.name
    );
    let config = server::Config {
        bind: args.bind,
        advertise: args.advertise,
        ports: server::Ports {
            blaze: args.blaze_port,
            ..Default::default()
        },
        players: Roster {
            own_name: args.name,
            allowed: args.players.iter().map(|(ip, _)| *ip).collect(),
            names: args.players.into_iter().collect(),
            only_allowed: args.only_listed,
        },
    };
    Server::start(config).await?.finished().await
}

/// Runs net-check against `peer` and fails when it finds a problem.
fn net_check(peer: Ipv4Addr) -> io::Result<()> {
    match netcheck::diagnose(peer)?.problems {
        0 => Ok(()),
        n => Err(io::Error::other(format!("{n} problem(s) found"))),
    }
}

/// Runs `command`, printing the log lines written meanwhile by any task of
/// this process, and returns its result after the lines still queued.
async fn printing_log(command: impl Future<Output = io::Result<()>>) -> io::Result<()> {
    let mut events = event::subscribe();
    let result = {
        let print = async {
            loop {
                match events.recv().await {
                    Ok(event) => print_event(event),
                    Err(RecvError::Lagged(missed)) => print_missed(missed),
                    // The sender is a static that is never dropped.
                    Err(RecvError::Closed) => std::future::pending().await,
                }
            }
        };
        tokio::select! {
            result = command => result,
            never = print => never,
        }
    };
    loop {
        match events.try_recv() {
            Ok(event) => print_event(event),
            Err(TryRecvError::Lagged(missed)) => print_missed(missed),
            Err(TryRecvError::Empty | TryRecvError::Closed) => return result,
        }
    }
}

/// Prints a log line on stdout; other events are not shown on the console.
fn print_event(event: Event) {
    if let Event::Log(line) = event {
        println!("{line}");
    }
}

/// Tells on stderr that `missed` events went by unprinted.
fn print_missed(missed: u64) {
    eprintln!("({missed} events not shown here; the log file has every line)");
}

#[tokio::main]
async fn main() -> ExitCode {
    let result = match Cli::parse().command {
        Command::Serve(args) => printing_log(serve(args)).await,
        Command::NetCheck { peer } => net_check(peer),
        Command::P2pCheck(args) => p2pcheck::run(args.peer, args.port, args.seconds).await,
        #[cfg(feature = "research")]
        Command::Proxy(args) => printing_log(run_proxy(args.ea_redirector)).await,
        #[cfg(feature = "research")]
        Command::EaCheck(args) => proxy::check(args.ea_redirector).await,
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("error: {err}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn command_line_is_consistent() {
        Cli::command().debug_assert();
    }

    #[test]
    fn player_entries_need_an_address_and_a_name() {
        assert_eq!(
            player("26.0.0.2=Carver"),
            Ok((Ipv4Addr::new(26, 0, 0, 2), "Carver".into()))
        );
        assert!(player("26.0.0.2").is_err());
        assert!(player("26.0.0.2= ").is_err());
        assert!(player("26.0.2=Carver").is_err());
    }

    #[test]
    fn serve_defaults_come_from_the_constants() {
        let Command::Serve(args) = Cli::parse_from(["ds3-blaze", "serve"]).command else {
            panic!("not serve");
        };
        assert_eq!(args.bind, Ipv4Addr::LOCALHOST);
        assert_eq!(args.blaze_port, DEFAULT_BLAZE_PORT);
        assert_eq!(args.name, DEFAULT_NAME);
        assert!(args.players.is_empty());
        assert!(!args.only_listed);
    }
}
