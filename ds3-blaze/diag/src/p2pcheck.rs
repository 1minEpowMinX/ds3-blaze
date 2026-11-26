//! UDP reachability check between two players on the game's P2P port.
//!
//! Both machines run the check at the same time with each other's address. Each
//! side binds the port, pings the peer every [`PING_INTERVAL`] and answers the
//! peer's pings, so one run shows both directions: pings received mean the peer
//! reaches this machine, answers received mean this machine reaches the peer.
//!
//! Both sides send from and to the same port, as the game does, so a firewall
//! that tracks UDP flows lets the peer's packets in once this side has sent.
//! Firewall rules bound to the game's executable are not exercised.

use std::net::{Ipv4Addr, SocketAddr};
use std::time::Duration;
use tokio::net::UdpSocket;
use tokio::time::{Instant, interval};

/// Port Dead Space 3 uses for its P2P traffic.
pub const GAME_PORT: u16 = 3659;
/// Run length used when `--seconds` is not given.
pub const DEFAULT_SECONDS: u64 = 30;

const PING: &[u8] = b"DS3P2P PING ";
const PONG: &[u8] = b"DS3P2P PONG ";
const PING_INTERVAL: Duration = Duration::from_millis(500);
/// Time the check keeps answering after both directions work, so the peer's own
/// check sees its answers too.
const LINGER: Duration = Duration::from_secs(5);

/// Counters of one run.
#[derive(Default)]
struct Tally {
    sent: u32,
    /// Pings from the peer: the peer reaches this machine.
    pings_received: u32,
    /// Answers to this machine's pings: this machine reaches the peer.
    pongs_received: u32,
    /// Packets from addresses other than the peer, by sender.
    strangers: Vec<SocketAddr>,
}

/// Exchanges pings with `peer` on UDP `port` for up to `seconds` seconds and
/// prints the result; fails when the port cannot be bound or a direction does
/// not work.
pub async fn run(peer: Ipv4Addr, port: u16, seconds: u64) -> std::io::Result<()> {
    let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, port))
        .await
        .map_err(|err| {
            std::io::Error::new(
                err.kind(),
                format!("cannot bind UDP port {port}: {err} (close the game, it holds this port)"),
            )
        })?;
    let target = SocketAddr::from((peer, port));
    println!(
        "UDP {port}: pinging {target} for up to {seconds} s; run the same check on the other PC now"
    );

    let deadline = Instant::now() + Duration::from_secs(seconds);
    let mut both_since: Option<Instant> = None;
    let mut tally = Tally::default();
    let mut ticker = interval(PING_INTERVAL);
    let mut buf = [0u8; 64];
    loop {
        let now = Instant::now();
        if now >= deadline || both_since.is_some_and(|since| now >= since + LINGER) {
            break;
        }
        tokio::select! {
            _ = ticker.tick() => {
                tally.sent += 1;
                // A send error on an unreachable route shows up as missing answers.
                let _ = socket.send_to(&[PING, &tally.sent.to_be_bytes()].concat(), target).await;
            }
            received = socket.recv_from(&mut buf) => {
                // Windows reports an ICMP port-unreachable for an earlier send as a
                // receive error; the peer is just not listening yet.
                let Ok((size, from)) = received else { continue };
                let packet = &buf[..size];
                if from.ip() != target.ip() {
                    if !tally.strangers.contains(&from) {
                        tally.strangers.push(from);
                    }
                    continue;
                }
                if let Some(seq) = packet.strip_prefix(PING) {
                    if tally.pings_received == 0 {
                        println!("  ping from {from} arrived: the peer reaches this PC");
                    }
                    tally.pings_received += 1;
                    let _ = socket.send_to(&[PONG, seq].concat(), from).await;
                } else if packet.starts_with(PONG) {
                    if tally.pongs_received == 0 {
                        println!("  answer from {from} arrived: this PC reaches the peer");
                    }
                    tally.pongs_received += 1;
                }
                if both_since.is_none() && tally.pings_received > 0 && tally.pongs_received > 0 {
                    both_since = Some(Instant::now());
                }
            }
        }
    }
    report(&tally, target)
}

/// Prints `tally` for the exchange with `target` and returns an error naming the
/// direction that failed.
fn report(tally: &Tally, target: SocketAddr) -> std::io::Result<()> {
    println!(
        "sent {} pings, {} answered; {} pings received from the peer",
        tally.sent, tally.pongs_received, tally.pings_received
    );
    for stranger in &tally.strangers {
        println!("  packets also came from {stranger}, not from the peer: check the peer address");
    }
    match (tally.pings_received > 0, tally.pongs_received > 0) {
        (true, true) => {
            println!("both directions work over UDP {}", target.port());
            Ok(())
        }
        (true, false) => Err(std::io::Error::other(
            "the peer's pings arrive but this PC's pings never reach the peer: the peer's firewall drops incoming UDP",
        )),
        (false, true) => Err(std::io::Error::other(
            "this PC reaches the peer but the peer's pings never arrived: this PC's firewall drops incoming UDP",
        )),
        (false, false) => Err(std::io::Error::other(
            "nothing arrived either way: check the VPN connection, the peer address and both firewalls, \
             and that the check runs on both PCs at once",
        )),
    }
}
