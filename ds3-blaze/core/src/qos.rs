//! QoS service: HTTP `/qos/qos`, `/qos/firewall`, `/qos/firetype` and the UDP
//! echo probe the client measures latency and its external address with.
//!
//! Response bodies follow the XML captured from EA's QoS servers.

use crate::log;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream, UdpSocket};
use tokio_util::sync::CancellationToken;

/// Ports of the QoS service.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ports {
    pub http: u16,
    pub udp: u16,
}

/// Value of `<firetype>`; Pocket Relay answers Mass Effect 3 clients with it.
const FIRETYPE: u8 = 2;
/// Firewall probe ports announced in `/qos/firewall`; nothing listens on them.
const FIREWALL_PORTS: [u16; 2] = [17500, 17501];
const MAX_REQUEST: usize = 8 * 1024;
/// Size of a latency probe (`qtyp=1`); bandwidth probes (`qtyp=2`) are `probesize` bytes.
const LATENCY_PROBE_SIZE: usize = 20;
/// Bytes 20..28 of every bandwidth probe reply from EA's QoS server in capture 2,
/// reused verbatim; their meaning is unknown.
const BANDWIDTH_REPLY_TAIL: [u8; 8] = [0x05, 0xb8, 0xd8, 0x00, 0x53, 0xad, 0x00, 0x00];

/// Serves the HTTP part on `http` and the UDP probe echo on `udp` until either
/// fails; each HTTP connection ends when `stop` is cancelled.
///
/// `advertise` is the address written into the XML as the probe target.
pub async fn serve(
    http: TcpListener,
    udp: UdpSocket,
    advertise: Ipv4Addr,
    stop: CancellationToken,
) -> std::io::Result<()> {
    let udp_addr = udp.local_addr()?;
    log!(
        "qos",
        "listening on {} (HTTP) and {udp_addr} (UDP)",
        http.local_addr()?
    );
    tokio::try_join!(
        accept_http(http, advertise, udp_addr.port(), stop),
        echo(udp)
    )?;
    Ok(())
}

async fn accept_http(
    listener: TcpListener,
    advertise: Ipv4Addr,
    udp_port: u16,
    stop: CancellationToken,
) -> std::io::Result<()> {
    loop {
        let (tcp, peer) = listener.accept().await?;
        tokio::spawn(
            stop.clone()
                .run_until_cancelled_owned(serve_http(tcp, peer, advertise, udp_port)),
        );
    }
}

async fn echo(socket: UdpSocket) -> std::io::Result<()> {
    let mut buffer = [0u8; 2048];
    loop {
        let (count, peer) = socket.recv_from(&mut buffer).await?;
        let SocketAddr::V4(peer_v4) = peer else {
            continue;
        };
        let probe = &buffer[..count];
        let reply = if count > LATENCY_PROBE_SIZE {
            bandwidth_reply(probe)
        } else {
            latency_reply(probe, peer_v4)
        };
        log!(
            "qos",
            "{peer}: UDP probe {count} bytes, replied with {} bytes",
            reply.len()
        );
        socket.send_to(&reply, peer).await?;
    }
}

/// Returns the reply to a latency probe: the probe as received, then the sender's
/// IPv4 address and port as seen here, then four zero bytes.
fn latency_reply(probe: &[u8], peer: SocketAddrV4) -> Vec<u8> {
    let mut reply = Vec::with_capacity(probe.len() + 10);
    reply.extend_from_slice(probe);
    reply.extend_from_slice(&peer.ip().octets());
    reply.extend_from_slice(&peer.port().to_be_bytes());
    reply.extend_from_slice(&[0; 4]);
    reply
}

/// Returns the reply to a bandwidth probe, laid out as EA's replies in capture 2.
///
/// A probe holds, as big-endian u32: an id, `requestid`, `reqsecret`, the probe
/// index from 0 and the probe count, then zero padding. The reply has the probe's
/// length and keeps the first three words; then follow the index plus one (big
/// endian), the probe count (little endian) and [`BANDWIDTH_REPLY_TAIL`]. A probe
/// too short for that layout is echoed unchanged.
fn bandwidth_reply(probe: &[u8]) -> Vec<u8> {
    let mut reply = vec![0u8; probe.len()];
    if probe.len() < 20 + BANDWIDTH_REPLY_TAIL.len() {
        reply.copy_from_slice(probe);
        return reply;
    }
    let word = |at: usize| u32::from_be_bytes(probe[at..at + 4].try_into().expect("4 bytes"));
    reply[..12].copy_from_slice(&probe[..12]);
    reply[12..16].copy_from_slice(&word(12).wrapping_add(1).to_be_bytes());
    reply[16..20].copy_from_slice(&word(16).to_le_bytes());
    reply[20..28].copy_from_slice(&BANDWIDTH_REPLY_TAIL);
    reply
}

async fn serve_http(mut tcp: TcpStream, peer: SocketAddr, advertise: Ipv4Addr, udp_port: u16) {
    let mut request = Vec::new();
    let mut chunk = [0u8; 1024];
    while !request.windows(4).any(|w| w == b"\r\n\r\n") {
        match tcp.read(&mut chunk).await {
            Ok(0) | Err(_) => return,
            Ok(n) => request.extend_from_slice(&chunk[..n]),
        }
        if request.len() > MAX_REQUEST {
            return;
        }
    }
    let text = String::from_utf8_lossy(&request);
    let target = text.split_whitespace().nth(1).unwrap_or("").to_string();
    log!("qos", "{peer}: GET {target}");

    let (path, query) = target.split_once('?').unwrap_or((target.as_str(), ""));
    let param = |name: &str| {
        query
            .split('&')
            .filter_map(|pair| pair.split_once('='))
            .find(|(key, _)| *key == name)
            .map(|(_, value)| value)
    };
    let ip = u32::from(advertise);
    let (command, body) = match path {
        "/qos/qos" if param("qtyp") == Some("2") => (
            "qos",
            format!(
                "<qos><numprobes>10</numprobes><qosport>{udp_port}</qosport><probesize>1200</probesize>\
                 <qosip>{ip}</qosip><requestid>2</requestid><reqsecret>2</reqsecret></qos>"
            ),
        ),
        "/qos/qos" => (
            "qos",
            format!(
                "<qos><numprobes>0</numprobes><qosport>{udp_port}</qosport><probesize>0</probesize>\
                 <qosip>{ip}</qosip><requestid>1</requestid><reqsecret>0</reqsecret></qos>"
            ),
        ),
        "/qos/firewall" => (
            "firewall",
            format!(
                "<firewall><ips><ips>{ip}</ips><ips>{ip}</ips></ips><numinterfaces>2</numinterfaces>\
                 <ports><ports>{}</ports><ports>{}</ports></ports>\
                 <requestid>3</requestid><reqsecret>3</reqsecret></firewall>",
                FIREWALL_PORTS[0], FIREWALL_PORTS[1]
            ),
        ),
        "/qos/firetype" => (
            "firetype",
            format!("<firetype><firetype>{FIRETYPE}</firetype></firetype>"),
        ),
        _ => {
            log!("qos", "{peer}: unknown path, answering 404");
            let _ = tcp
                .write_all(
                    b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                )
                .await;
            return;
        }
    };
    let body = format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n{body}\n");
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/xml\r\nContent-Length: {}\r\n\
         X-BLAZE-COMPONENT: qos\r\nX-BLAZE-COMMAND: {command}\r\nX-BLAZE-SEQNO: 0\r\n\
         Connection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = tcp.write_all(response.as_bytes()).await;
}
