//! UDP reachability check between two players on the game's P2P port.
//!
//! Both machines run the exchange at the same time with each other's address.
//! Each side pings the other every [`PING_INTERVAL`] and answers the other's
//! pings, so one run shows both directions: pings received mean the other
//! player reaches this machine, answers received mean this machine reaches the
//! other player.
//!
//! Both sides send from and to the same port, as the game does, so a firewall
//! that tracks UDP flows lets the other side's packets in once this side has
//! sent. Firewall rules bound to the game's executable are not exercised.

use std::io;
use std::net::{Ipv4Addr, SocketAddr};
use std::time::Duration;
use tokio::net::UdpSocket;
use tokio::sync::mpsc;
use tokio::time::{Instant, interval};

/// Port Dead Space 3 uses for its P2P traffic.
pub const GAME_PORT: u16 = 3659;
/// Run length used when `--seconds` is not given.
pub const DEFAULT_SECONDS: u64 = 30;

const PING: &[u8] = b"DS3P2P PING ";
const PONG: &[u8] = b"DS3P2P PONG ";
const PING_INTERVAL: Duration = Duration::from_millis(500);
/// Time the exchange keeps answering after both directions work, so the other
/// side's exchange sees its answers too.
const LINGER: Duration = Duration::from_secs(5);

/// How long an exchange runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Timing {
    /// Longest run time.
    pub limit: Duration,
    /// Time the exchange goes on after both directions work.
    pub linger: Duration,
}

impl Default for Timing {
    /// Returns [`DEFAULT_SECONDS`] as the limit and [`LINGER`].
    fn default() -> Self {
        Self {
            limit: Duration::from_secs(DEFAULT_SECONDS),
            linger: LINGER,
        }
    }
}

/// Something an exchange learned while it runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Progress {
    /// The exchange started pinging `target` and stops at `until` at the
    /// latest.
    Pinging { target: SocketAddr, until: Instant },
    /// The first ping of the other player arrived.
    PeerReachesThisPc,
    /// The first answer to this machine's pings arrived.
    ThisPcReachesPeer,
}

/// Counters of a finished exchange.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Outcome {
    /// Pings this machine sent.
    pub sent: u32,
    /// Answers to them: this machine reaches the other player.
    pub answered: u32,
    /// Pings from the other player: the other player reaches this machine.
    pub received: u32,
    /// Senders of packets from addresses other than the other player's.
    pub strangers: Vec<SocketAddr>,
}

impl Outcome {
    /// Returns what failed and its likely cause, or `None` when both
    /// directions work.
    ///
    /// When no answer came back, the VPN adapters in `other_vpn_adapters`
    /// are named as a possible cause: a VPN client's kill switch drops traffic
    /// while the routes stay intact, so only the exchange shows it.
    pub fn failure(&self, other_vpn_adapters: &[&str]) -> Option<String> {
        let text = match (self.received > 0, self.answered > 0) {
            (true, true) => return None,
            (true, false) => {
                "the other player's pings arrive but this PC's pings never reach them: \
                 the other player's firewall drops incoming UDP"
            }
            (false, true) => {
                "this PC reaches the other player but their pings never arrived: \
                 this PC's firewall drops incoming UDP"
            }
            (false, false) => {
                "nothing arrived either way: check the VPN connection, the other player's address \
                 and both firewalls, and that the check runs on both PCs at once"
            }
        };
        if self.answered > 0 || other_vpn_adapters.is_empty() {
            return Some(text.to_string());
        }
        let names: Vec<String> = other_vpn_adapters
            .iter()
            .map(|name| format!("\"{name}\""))
            .collect();
        Some(format!(
            "{text}; another VPN is up on this PC ({}) and may drop the traffic, \
             disconnect it and run the check again",
            names.join(", ")
        ))
    }
}

/// Binds UDP `port` on every interface for an exchange.
///
/// # Errors
///
/// Returns the bind error with the advice to close the game, which holds the
/// game's port while it runs.
pub async fn bind(port: u16) -> io::Result<UdpSocket> {
    UdpSocket::bind((Ipv4Addr::UNSPECIFIED, port))
        .await
        .map_err(|err| {
            io::Error::new(
                err.kind(),
                format!("cannot bind UDP port {port}: {err} (close the game, it holds this port)"),
            )
        })
}

/// Pings `peer` from `socket` and answers its pings until both directions
/// have worked for `timing.linger`, or for `timing.limit` at most, reporting
/// each first on `progress`.
///
/// Sending stops nothing when `progress` has no receiver.
pub async fn exchange(
    socket: UdpSocket,
    peer: SocketAddr,
    timing: Timing,
    progress: mpsc::UnboundedSender<Progress>,
) -> Outcome {
    let deadline = Instant::now() + timing.limit;
    let _ = progress.send(Progress::Pinging {
        target: peer,
        until: deadline,
    });
    let mut both_since: Option<Instant> = None;
    let mut outcome = Outcome::default();
    let mut ticker = interval(PING_INTERVAL);
    let mut buf = [0u8; 64];
    loop {
        let now = Instant::now();
        if now >= deadline || both_since.is_some_and(|since| now >= since + timing.linger) {
            return outcome;
        }
        tokio::select! {
            _ = ticker.tick() => {
                outcome.sent += 1;
                // A send error on an unreachable route shows up as missing answers.
                let _ = socket.send_to(&[PING, &outcome.sent.to_be_bytes()].concat(), peer).await;
            }
            // Wakes the loop at the deadline even when nothing arrives.
            _ = tokio::time::sleep_until(deadline) => {}
            received = socket.recv_from(&mut buf) => {
                // Windows reports an ICMP port-unreachable for an earlier send as a
                // receive error; the other side is just not listening yet.
                let Ok((size, from)) = received else { continue };
                let packet = &buf[..size];
                if from.ip() != peer.ip() {
                    if !outcome.strangers.contains(&from) {
                        outcome.strangers.push(from);
                    }
                    continue;
                }
                if let Some(seq) = packet.strip_prefix(PING) {
                    if outcome.received == 0 {
                        let _ = progress.send(Progress::PeerReachesThisPc);
                    }
                    outcome.received += 1;
                    let _ = socket.send_to(&[PONG, seq].concat(), from).await;
                } else if packet.starts_with(PONG) {
                    if outcome.answered == 0 {
                        let _ = progress.send(Progress::ThisPcReachesPeer);
                    }
                    outcome.answered += 1;
                }
                if both_since.is_none() && outcome.received > 0 && outcome.answered > 0 {
                    both_since = Some(Instant::now());
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn outcome(answered: u32, received: u32) -> Outcome {
        Outcome {
            sent: 10,
            answered,
            received,
            strangers: Vec::new(),
        }
    }

    #[test]
    fn both_directions_working_is_no_failure() {
        assert_eq!(outcome(3, 4).failure(&["ProtonVPN"]), None);
    }

    #[test]
    fn failure_names_the_direction_that_fails() {
        let theirs = outcome(0, 4).failure(&[]).expect("a failure");
        assert!(theirs.contains("the other player's firewall"), "{theirs}");
        let ours = outcome(3, 0).failure(&[]).expect("a failure");
        assert!(ours.contains("this PC's firewall"), "{ours}");
        let none = outcome(0, 0).failure(&[]).expect("a failure");
        assert!(none.contains("nothing arrived"), "{none}");
        assert!(!none.contains("another VPN"), "{none}");
    }

    #[test]
    fn missing_answers_name_other_vpn_adapters() {
        let adapters = ["ProtonVPN", "WireGuard"];
        for (answered, received) in [(0, 0), (0, 4)] {
            let text = outcome(answered, received)
                .failure(&adapters)
                .expect("a failure");
            assert!(
                text.contains("another VPN is up on this PC (\"ProtonVPN\", \"WireGuard\")"),
                "{text}"
            );
        }
        // Answers arrive, so this PC's outgoing traffic passes the other VPN.
        let text = outcome(3, 0).failure(&adapters).expect("a failure");
        assert!(!text.contains("another VPN"), "{text}");
    }
}
