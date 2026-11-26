//! Local Blaze server for Dead Space 3 co-op.
//!
//! `serve` runs the SSLv3 redirector, the main Blaze server and the QoS
//! service; `proxy` relays the game to EA's servers and logs the traffic;
//! `ea-check` tests the SSLv3 connection to EA's redirector; `net-check` reports
//! the route and firewall state for a VPN peer; `p2p-check` adds a UDP exchange
//! between two players on the game's port.

mod proxy;

use ds3_blaze_core::event::{self, Event};
use ds3_blaze_core::log;
use ds3_blaze_core::players::Roster;
use ds3_blaze_core::server::{self, DEFAULT_BLAZE_PORT, REDIRECTOR_PORT, Server};
use ds3_blaze_diag::{netcheck, p2pcheck};

use std::net::{Ipv4Addr, SocketAddr};
use std::process::ExitCode;
use tokio::sync::broadcast::error::{RecvError, TryRecvError};

/// Display name used when `--name` is not given.
const DEFAULT_NAME: &str = "DS3Player";

const USAGE: &str = "\
usage:
  ds3-blaze serve [--bind IP] [--advertise IP] [--blaze-port PORT] [--name NAME]
                  [--player IP=NAME]... [--only-listed]
      --bind        address all listeners bind to (default 127.0.0.1)
      --advertise   IP the game is told to reach this server at (default 127.0.0.1)
      --blaze-port  main Blaze server port (default 42128)
      --name        display name of the player on this machine (default DS3Player)
      --player      display name of the player connecting from IP; others get
                    Player-<last octet of their IP>
      --only-listed refuse Blaze connections from addresses other than this
                    machine, --advertise and the --player entries
  ds3-blaze proxy [--ea-redirector IP:PORT]
      relays the game to EA's Blaze servers and logs every message
      --ea-redirector  EA's redirector (default: gosredirector.ea.com's address)
  ds3-blaze ea-check [--ea-redirector IP:PORT]
      checks the SSLv3 connection to EA's redirector and main server
  ds3-blaze net-check --peer IP
      shows the adapter traffic to IP takes, other VPN adapters, the network
      profile and the Windows Firewall rules for the game and this server
  ds3-blaze p2p-check --peer IP [--port PORT] [--seconds N]
      runs net-check, then exchanges UDP pings with the other player;
      run it on both PCs at once
      --peer        the other player's VPN address
      --port        UDP port (default 3659, the game's P2P port)
      --seconds     longest run time (default 30)";

struct ServeArgs {
    bind: Ipv4Addr,
    advertise: Ipv4Addr,
    blaze_port: u16,
    name: String,
    players: Vec<(Ipv4Addr, String)>,
    only_listed: bool,
}

fn parse_serve(args: &[String]) -> Result<ServeArgs, String> {
    let mut out = ServeArgs {
        bind: Ipv4Addr::LOCALHOST,
        advertise: Ipv4Addr::LOCALHOST,
        blaze_port: DEFAULT_BLAZE_PORT,
        name: DEFAULT_NAME.to_string(),
        players: Vec::new(),
        only_listed: false,
    };
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        let mut value = || it.next().ok_or_else(|| format!("{arg} needs a value"));
        match arg.as_str() {
            "--bind" => out.bind = value()?.parse().map_err(|e| format!("--bind: {e}"))?,
            "--advertise" => {
                out.advertise = value()?.parse().map_err(|e| format!("--advertise: {e}"))?
            }
            "--blaze-port" => {
                out.blaze_port = value()?.parse().map_err(|e| format!("--blaze-port: {e}"))?
            }
            "--name" => out.name = value()?.clone(),
            "--player" => {
                let entry = value()?;
                let (ip, name) = entry.split_once('=').ok_or("--player takes IP=NAME")?;
                let ip = ip.parse().map_err(|e| format!("--player {entry}: {e}"))?;
                if name.trim().is_empty() {
                    return Err(format!("--player {entry}: empty name"));
                }
                out.players.push((ip, name.to_string()));
            }
            "--only-listed" => out.only_listed = true,
            other => return Err(format!("unknown option {other}")),
        }
    }
    if out.name.trim().is_empty() {
        return Err("--name must not be empty".into());
    }
    Ok(out)
}

fn parse_ea_redirector(args: &[String]) -> Result<SocketAddr, String> {
    match args {
        [] => Ok(proxy::EA_REDIRECTOR),
        [flag, addr] if flag == "--ea-redirector" => {
            addr.parse().map_err(|e| format!("--ea-redirector: {e}"))
        }
        _ => Err("only --ea-redirector IP:PORT is accepted".into()),
    }
}

async fn run_proxy(ea_redirector: SocketAddr) -> std::io::Result<()> {
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

struct P2pArgs {
    peer: Ipv4Addr,
    port: u16,
    seconds: u64,
}

fn parse_p2p_check(args: &[String]) -> Result<P2pArgs, String> {
    let mut peer = None;
    let mut out = P2pArgs {
        peer: Ipv4Addr::UNSPECIFIED,
        port: p2pcheck::GAME_PORT,
        seconds: p2pcheck::DEFAULT_SECONDS,
    };
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        let mut value = || it.next().ok_or_else(|| format!("{arg} needs a value"));
        match arg.as_str() {
            "--peer" => peer = Some(value()?.parse().map_err(|e| format!("--peer: {e}"))?),
            "--port" => out.port = value()?.parse().map_err(|e| format!("--port: {e}"))?,
            "--seconds" => out.seconds = value()?.parse().map_err(|e| format!("--seconds: {e}"))?,
            other => return Err(format!("unknown option {other}")),
        }
    }
    out.peer = peer.ok_or("p2p-check needs --peer IP")?;
    Ok(out)
}

async fn serve(args: ServeArgs) -> std::io::Result<()> {
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

/// Runs `command`, printing the log lines written meanwhile by any task of
/// this process, and returns its result after the lines still queued.
async fn printing_log(command: impl Future<Output = std::io::Result<()>>) -> std::io::Result<()> {
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
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.split_first() {
        Some((cmd, rest)) if cmd == "serve" => match parse_serve(rest) {
            Ok(a) => printing_log(serve(a)).await,
            Err(e) => return usage_error(&e),
        },
        Some((cmd, rest)) if cmd == "proxy" => match parse_ea_redirector(rest) {
            Ok(addr) => printing_log(run_proxy(addr)).await,
            Err(e) => return usage_error(&e),
        },
        Some((cmd, rest)) if cmd == "ea-check" => match parse_ea_redirector(rest) {
            Ok(addr) => proxy::check(addr).await,
            Err(e) => return usage_error(&e),
        },
        Some((cmd, rest)) if cmd == "net-check" => match parse_p2p_check(rest) {
            Ok(a) => netcheck::diagnose(a.peer).and_then(|verdict| match verdict.problems {
                0 => Ok(()),
                n => Err(std::io::Error::other(format!("{n} problem(s) found"))),
            }),
            Err(e) => return usage_error(&e),
        },
        Some((cmd, rest)) if cmd == "p2p-check" => match parse_p2p_check(rest) {
            Ok(a) => p2pcheck::run(a.peer, a.port, a.seconds).await,
            Err(e) => return usage_error(&e),
        },
        _ => return usage_error("missing command"),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("error: {err}");
            ExitCode::FAILURE
        }
    }
}

fn usage_error(message: &str) -> ExitCode {
    eprintln!("{message}\n{USAGE}");
    ExitCode::from(2)
}
