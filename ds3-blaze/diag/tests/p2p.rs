//! Exchanges between sockets of this process on the loopback address.

use ds3_blaze_diag::p2p::{self, Outcome, Progress, Timing};
use std::net::{Ipv4Addr, SocketAddr};
use std::time::Duration;
use tokio::net::UdpSocket;
use tokio::sync::mpsc;

/// Timing short enough for a test; the linger still spans a few pings.
const QUICK: Timing = Timing {
    limit: Duration::from_secs(5),
    linger: Duration::from_millis(1200),
};

/// Binds a free UDP port through `p2p::bind` and returns the socket with the
/// loopback address of that port.
async fn bound() -> (UdpSocket, SocketAddr) {
    let socket = p2p::bind(0).await.expect("a free port binds");
    let port = socket.local_addr().expect("a local address").port();
    (socket, SocketAddr::from((Ipv4Addr::LOCALHOST, port)))
}

/// Runs one exchange and returns its outcome with every progress event.
async fn run(socket: UdpSocket, peer: SocketAddr, timing: Timing) -> (Outcome, Vec<Progress>) {
    let (sender, mut receiver) = mpsc::unbounded_channel();
    let outcome = p2p::exchange(socket, peer, timing, sender).await;
    let mut events = Vec::new();
    while let Ok(event) = receiver.try_recv() {
        events.push(event);
    }
    (outcome, events)
}

#[tokio::test]
async fn two_exchanges_started_apart_see_both_directions() {
    let (first, first_address) = bound().await;
    let (second, second_address) = bound().await;
    let early = run(first, second_address, QUICK);
    let late = async {
        tokio::time::sleep(Duration::from_millis(700)).await;
        run(second, first_address, QUICK).await
    };
    let ((first, first_events), (second, second_events)) = tokio::join!(early, late);
    for (outcome, events) in [(first, first_events), (second, second_events)] {
        assert_eq!(outcome.failure(&[]), None, "{outcome:?}");
        assert!(outcome.strangers.is_empty());
        assert!(matches!(events[0], Progress::Pinging { .. }), "{events:?}");
        assert!(events.contains(&Progress::PeerReachesThisPc), "{events:?}");
        assert!(events.contains(&Progress::ThisPcReachesPeer), "{events:?}");
    }
}

#[tokio::test]
async fn taken_port_fails_to_bind_with_the_advice_to_close_the_game() {
    let (_holder, address) = bound().await;
    let err = p2p::bind(address.port())
        .await
        .expect_err("the port is taken");
    assert!(err.to_string().contains("close the game"), "{err}");
}

#[tokio::test]
async fn exchange_without_an_answer_ends_at_its_limit() {
    let (socket, _) = bound().await;
    // A port that was free a moment ago and is closed again.
    let silent = bound().await.1;
    let limit = Duration::from_millis(1200);
    let started = tokio::time::Instant::now();
    let (outcome, events) = run(socket, silent, Timing { limit, ..QUICK }).await;
    let took = started.elapsed();
    assert!(
        took >= limit && took < limit + Duration::from_secs(1),
        "{took:?}"
    );
    assert!(outcome.sent >= 2, "{outcome:?}");
    assert_eq!((outcome.answered, outcome.received), (0, 0));
    assert_eq!(events.len(), 1, "{events:?}");
    assert!(outcome.failure(&[]).is_some());
}
