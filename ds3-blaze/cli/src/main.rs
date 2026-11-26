//! Command line of ds3-blaze: the local Blaze server for Dead Space 3 co-op,
//! the hosts entry that points the game at it, and the network checks between
//! two players.
//!
//! The commands and their options are declared in [`Command`].

#[cfg(feature = "research")]
mod proxy;

use clap::{Args, Parser, Subcommand};
use ds3_blaze_core::event::{self, Event};
use ds3_blaze_core::hosts::{self, Action, State};
use ds3_blaze_core::log;
use ds3_blaze_core::players::{self, Roster};
use ds3_blaze_core::server::{self, DEFAULT_BLAZE_PORT, Server};
use ds3_blaze_diag::fix;
use ds3_blaze_diag::netcheck::{self, Finding, Fix, Own, Setup, Severity};
use ds3_blaze_diag::p2p::{self, Progress, Timing};
use ds3_blaze_diag::reach;

use std::io;
use std::net::{Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;
use tokio::sync::broadcast::error::{RecvError, TryRecvError};
use tokio::sync::mpsc;

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
    /// Shows or changes the hosts entry that points the game at a ds3-blaze
    /// server.
    #[command(subcommand)]
    Hosts(HostsCommand),
    /// Shows this PC's VPN adapter, other VPN adapters, the network profile,
    /// the Windows Firewall rules for the game and this server, and the adapter
    /// traffic to the peer takes.
    NetCheck {
        /// The other player's VPN address.
        #[arg(long, value_name = "IP")]
        peer: Ipv4Addr,
        /// This PC's VPN address; without it any adapter that looks like a VPN
        /// counts as this PC's.
        #[arg(long, value_name = "IP")]
        address: Option<Ipv4Addr>,
    },
    /// Runs net-check, then exchanges UDP pings with the other player; run it
    /// on both PCs at once.
    P2pCheck(P2pArgs),
    /// Changes a Windows network setting that net-check finds wrong; needs a
    /// console run as administrator.
    #[command(subcommand)]
    Fix(FixCommand),
    /// Checks every connection the game makes to the host's server before it
    /// joins: the hosts entry, the redirector, the Blaze server and QoS; with
    /// --name, also sends this player's nickname to the host's server.
    Reach {
        /// The host's VPN address; on the host's own PC, its own VPN address.
        #[arg(long, value_name = "IP")]
        host: Ipv4Addr,
        /// The address the hosts entry should give; the default is --host.
        #[arg(long, value_name = "IP")]
        expect: Option<Ipv4Addr>,
        /// This player's nickname, sent to the host's server as the last stage.
        #[arg(long, value_name = "NICK", value_parser = nickname)]
        name: Option<String>,
    },
    /// Relays the game to EA's Blaze servers and logs every message.
    #[cfg(feature = "research")]
    Proxy(EaArgs),
    /// Checks the SSLv3 connection to EA's redirector and main server.
    #[cfg(feature = "research")]
    EaCheck(EaArgs),
}

#[derive(Subcommand)]
enum FixCommand {
    /// Adds a Windows Firewall rule allowing inbound UDP on the game's P2P port.
    GameRule,
    /// Adds a Windows Firewall rule allowing inbound traffic to the program
    /// that runs the server.
    ServerRule {
        /// The program; the default is this executable.
        #[arg(long, value_name = "PATH")]
        program: Option<PathBuf>,
    },
    /// Sets the network of the adapter holding --address to Private.
    NetworkProfile {
        /// This PC's VPN address.
        #[arg(long, value_name = "IP")]
        address: Ipv4Addr,
    },
}

#[derive(Subcommand)]
enum HostsCommand {
    /// Points the game at the server at --address; needs a console run as
    /// administrator.
    Enable {
        /// Address of the server: 127.0.0.1 on the host's PC, the host's VPN
        /// address on a guest's.
        #[arg(long, value_name = "IP", default_value_t = Ipv4Addr::LOCALHOST)]
        address: Ipv4Addr,
    },
    /// Points the game back at the EA servers; needs a console run as
    /// administrator.
    Disable,
    /// Tells which servers the game reaches.
    Status,
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
    #[arg(long, value_name = "NAME", default_value = DEFAULT_NAME, value_parser = nickname)]
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
    /// This PC's VPN address, as net-check takes it.
    #[arg(long, value_name = "IP")]
    address: Option<Ipv4Addr>,
    /// UDP port; the default is the game's P2P port.
    #[arg(long, value_name = "PORT", default_value_t = p2p::GAME_PORT)]
    port: u16,
    /// Longest run time in seconds.
    #[arg(long, value_name = "N", default_value_t = p2p::DEFAULT_SECONDS)]
    seconds: u64,
}

#[cfg(feature = "research")]
#[derive(Args)]
struct EaArgs {
    /// EA's redirector; the default is the address gosredirector.ea.com had.
    #[arg(long, value_name = "IP:PORT", default_value_t = proxy::EA_REDIRECTOR)]
    ea_redirector: std::net::SocketAddr,
}

/// Returns `name` when [`players::check_nickname`] accepts it, or the reason
/// it is refused.
fn nickname(name: &str) -> Result<String, String> {
    players::check_nickname(name)
        .map(|()| name.to_string())
        .map_err(|reason| format!("the name is refused: {reason}"))
}

/// Returns the address and the name of an `IP=NAME` entry.
fn player(entry: &str) -> Result<(Ipv4Addr, String), String> {
    let (ip, name) = entry.split_once('=').ok_or("expected IP=NAME")?;
    let ip = ip.parse().map_err(|err| format!("{ip}: {err}"))?;
    Ok((ip, nickname(name)?))
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
            offered: Default::default(),
            only_allowed: args.only_listed,
        },
    };
    Server::start(config).await?.finished().await
}

/// Returns what `state` means for the game, or its problem as an error.
fn describe(state: State) -> io::Result<String> {
    match state {
        State::Broken(problem) => Err(io::Error::other(problem)),
        state => Ok(state.to_string()),
    }
}

/// Runs a `hosts` subcommand.
fn hosts(command: HostsCommand) -> io::Result<()> {
    let action = match command {
        HostsCommand::Enable { address } => Action::Enable(address),
        HostsCommand::Disable => Action::Disable,
        HostsCommand::Status => {
            println!("{}", describe(hosts::status()?)?);
            return Ok(());
        }
    };
    hosts::apply(action).map_err(needing_administrator)?;
    println!(
        "previous hosts file without ds3-blaze entries saved as {}",
        hosts::backup_path().display()
    );
    if let Err(err) = hosts::flush_dns() {
        eprintln!(
            "warning: {err}; the game may keep the old address until the DNS cache \
             expires or Windows restarts"
        );
    }
    // An entry outside the section that comes first still wins after the write.
    if let Ok(state @ State::Overridden { .. }) = hosts::status() {
        eprintln!("warning: {}", describe(state)?);
        return Ok(());
    }
    match action {
        Action::Enable(address) => println!(
            "{} now leads to the ds3-blaze server at {address}",
            hosts::REDIRECTED.join(", ")
        ),
        Action::Disable => println!("ds3-blaze entries removed; the game reaches the EA servers"),
    }
    Ok(())
}

/// What net-check found that the commands after it use.
struct NetCheck {
    problems: usize,
    /// Names of the VPN adapters that are up besides this PC's.
    other_vpn_adapters: Vec<String>,
}

/// Prints the local and route findings for traffic to `peer` from the adapter
/// holding `address`, or from any VPN adapter without one.
fn net_check(peer: Ipv4Addr, address: Option<Ipv4Addr>) -> io::Result<NetCheck> {
    println!("network check for {peer} (reading Windows settings, a few seconds)");
    let records = netcheck::read(Some(peer))?;
    let own = address.map_or(Own::AnyVpn, Own::Address);
    let setup = Setup {
        own,
        // This executable runs `serve`, so its rules are the server's.
        server: std::env::current_exe().ok(),
    };
    println!("this PC:");
    let mut findings = netcheck::local(&records, &setup);
    print_findings(&findings);
    println!("route to {peer}:");
    let route = netcheck::route(&records, peer, &own, netcheck::source_ip(peer));
    print_findings(&route);
    findings.extend(route);
    let count = |severity| findings.iter().filter(|f| f.severity == severity).count();
    let problems = count(Severity::Problem);
    println!(
        "network check: {problems} problem(s), {} warning(s)",
        count(Severity::Warning)
    );
    Ok(NetCheck {
        problems,
        other_vpn_adapters: netcheck::other_vpn_adapters(&records, &own)
            .into_iter()
            .map(|adapter| adapter.name.clone())
            .collect(),
    })
}

/// Runs net-check and fails when a finding is a problem.
fn net_check_command(peer: Ipv4Addr, address: Option<Ipv4Addr>) -> io::Result<()> {
    match net_check(peer, address)?.problems {
        0 => Ok(()),
        n => Err(io::Error::other(format!("{n} problem(s) found"))),
    }
}

/// Runs net-check, then the exchange with the other player, printing its
/// progress and outcome; fails when a direction does not work.
async fn p2p_check(args: P2pArgs) -> io::Result<()> {
    // Problems net-check finds do not stop the exchange, which shows their
    // effect.
    let other_vpn_adapters = match net_check(args.peer, args.address) {
        Ok(found) => found.other_vpn_adapters,
        Err(err) => {
            println!("network check failed to run: {err}");
            Vec::new()
        }
    };
    println!();
    let socket = p2p::bind(args.port).await?;
    let peer = SocketAddr::from((args.peer, args.port));
    let timing = Timing {
        limit: Duration::from_secs(args.seconds),
        ..Timing::default()
    };
    let (sender, mut progress) = mpsc::unbounded_channel();
    let print = async {
        while let Some(event) = progress.recv().await {
            match event {
                Progress::Pinging { target, until } => println!(
                    "UDP {}: pinging {target} for up to {} s; run the same check on the other PC now",
                    target.port(),
                    until
                        .saturating_duration_since(tokio::time::Instant::now())
                        .as_secs_f32()
                        .round()
                ),
                Progress::PeerReachesThisPc => {
                    println!("  ping from {peer} arrived: the other player reaches this PC")
                }
                Progress::ThisPcReachesPeer => {
                    println!("  answer from {peer} arrived: this PC reaches the other player")
                }
            }
        }
    };
    let (outcome, ()) = tokio::join!(p2p::exchange(socket, peer, timing, sender), print);
    println!(
        "sent {} pings, {} answered; {} pings received from the other player",
        outcome.sent, outcome.answered, outcome.received
    );
    for stranger in &outcome.strangers {
        println!(
            "  packets also came from {stranger}, not from the other player: check their address"
        );
    }
    let names: Vec<&str> = other_vpn_adapters.iter().map(String::as_str).collect();
    match outcome.failure(&names) {
        None => {
            println!("both directions work over UDP {}", args.port);
            Ok(())
        }
        Some(failure) => Err(io::Error::other(failure)),
    }
}

/// Returns `err` with the hint to run the command as administrator when its
/// kind is [`io::ErrorKind::PermissionDenied`], and `err` itself otherwise.
fn needing_administrator(err: io::Error) -> io::Error {
    match err.kind() {
        io::ErrorKind::PermissionDenied => io::Error::new(
            err.kind(),
            format!("{err}; run this command in a console run as administrator"),
        ),
        _ => err,
    }
}

/// Carries out the fix `command` names.
fn run_fix(command: FixCommand) -> io::Result<()> {
    let fix = match command {
        FixCommand::GameRule => Fix::GameRule,
        FixCommand::ServerRule { program } => {
            Fix::ServerRule(program.map_or_else(std::env::current_exe, Ok)?)
        }
        FixCommand::NetworkProfile { address } => Fix::PrivateProfile(address),
    };
    fix::apply(&fix).map_err(needing_administrator)?;
    println!("done; net-check shows the new state");
    Ok(())
}

/// Checks the server at `host` with the hosts entry expected to give
/// `expected`, sending `name` last when it is given, and prints one line per
/// stage; fails when a stage other than the nickname fails.
async fn reach_command(host: Ipv4Addr, expected: Ipv4Addr, name: Option<String>) -> io::Result<()> {
    println!("checking the way to the server at {host}:");
    let reports = reach::run(expected, host, server::Ports::default(), name.as_deref()).await;
    let mut failed = 0;
    for report in &reports {
        match &report.result {
            Ok(text) => println!("  [ok] {}: {text}", report.stage),
            // The game still works under the name the host's server gives.
            Err(text) if report.stage == reach::Stage::Nickname => {
                println!("  [warning] {}: {text}", report.stage)
            }
            Err(text) => {
                failed += 1;
                println!("  [problem] {}: {text}", report.stage);
                if report.stage == reach::Stage::Resolve {
                    println!(
                        "          fix: {} hosts enable --address {expected}",
                        shell_exe(&invoked_exe())
                    );
                }
            }
        }
    }
    if failed > 0 {
        return Err(io::Error::other(format!(
            "{failed} of {} stages failed",
            reports.len()
        )));
    }
    println!("the game reaches the server at {host}");
    Ok(())
}

/// Returns the path this executable was started by, as the console typed it.
fn invoked_exe() -> String {
    std::env::args()
        .next()
        .unwrap_or_else(|| env!("CARGO_BIN_NAME").into())
}

/// Returns `invoked` in the form PowerShell runs it in.
fn shell_exe(invoked: &str) -> String {
    // PowerShell runs a quoted path only through the call operator.
    if invoked.contains(' ') {
        format!("& \"{invoked}\"")
    } else {
        invoked.to_string()
    }
}

/// Returns the command line that carries out `fix`.
fn fix_command(fix: &Fix) -> String {
    let exe = std::env::current_exe().ok();
    fix_command_from(fix, &invoked_exe(), exe.as_deref())
}

/// Returns the command line that carries out `fix` with the executable
/// started as `invoked`; the server rule names its program only when it is
/// not `current_exe`, which `fix server-rule` takes by default.
fn fix_command_from(fix: &Fix, invoked: &str, current_exe: Option<&std::path::Path>) -> String {
    let exe = shell_exe(invoked);
    match fix {
        Fix::GameRule => format!("{exe} fix game-rule"),
        Fix::ServerRule(program) if Some(program.as_path()) == current_exe => {
            format!("{exe} fix server-rule")
        }
        Fix::ServerRule(program) => {
            format!("{exe} fix server-rule --program \"{}\"", program.display())
        }
        Fix::PrivateProfile(address) => format!("{exe} fix network-profile --address {address}"),
    }
}

/// Prints each finding on a line of its own, marked with its severity.
fn print_findings(findings: &[Finding]) {
    for finding in findings {
        let mark = match finding.severity {
            Severity::Ok => "ok",
            Severity::Note => "note",
            Severity::Warning => "warning",
            Severity::Problem => "problem",
        };
        println!("  [{mark}] {}", finding.text);
        if let Some(fix) = &finding.fix {
            println!("          fix: {}", fix_command(fix));
        }
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
        Command::Hosts(command) => hosts(command),
        Command::Fix(command) => run_fix(command),
        Command::NetCheck { peer, address } => net_check_command(peer, address),
        Command::P2pCheck(args) => p2p_check(args).await,
        Command::Reach { host, expect, name } => {
            reach_command(host, expect.unwrap_or(host), name).await
        }
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
        assert!(player("26.0.0.2=Isaac Clarke").is_err());
        assert!(player("26.0.2=Carver").is_err());
    }

    #[test]
    fn fix_commands_run_as_printed() {
        let exe = std::path::Path::new(r"C:\My Games\ds3-blaze-cli.exe");
        assert_eq!(
            fix_command_from(&Fix::GameRule, r".\ds3-blaze-cli.exe", Some(exe)),
            r".\ds3-blaze-cli.exe fix game-rule"
        );
        assert_eq!(
            fix_command_from(
                &Fix::ServerRule(exe.into()),
                &exe.display().to_string(),
                Some(exe)
            ),
            r#"& "C:\My Games\ds3-blaze-cli.exe" fix server-rule"#
        );
        assert_eq!(
            fix_command_from(
                &Fix::ServerRule(r"D:\gui.exe".into()),
                "ds3-blaze",
                Some(exe)
            ),
            r#"ds3-blaze fix server-rule --program "D:\gui.exe""#
        );
        assert_eq!(
            fix_command_from(
                &Fix::PrivateProfile(Ipv4Addr::new(26, 0, 0, 1)),
                "ds3-blaze",
                None
            ),
            "ds3-blaze fix network-profile --address 26.0.0.1"
        );
    }

    #[test]
    fn default_name_is_a_nickname() {
        // clap parses a string default with the value parser, so a refused
        // default would fail every `serve` started without --name.
        assert_eq!(nickname(DEFAULT_NAME), Ok(DEFAULT_NAME.to_string()));
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
