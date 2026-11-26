//! Check, from a guest's PC, of every connection the game makes to the host's
//! server before it joins: the hosts entry, the redirector, the main Blaze
//! server and both QoS services.
//!
//! No room and no matchmaking: one `getServerInstance`, one `preAuth` and one
//! QoS request of each kind.

use blaze_ssl_async::BlazeStream;
use ds3_blaze_core::components::util;
use ds3_blaze_core::fire::{FireCodec, Frame, FrameType};
use ds3_blaze_core::hosts::REDIRECTED;
use ds3_blaze_core::redirector::{self, COMPONENT, GET_SERVER_INSTANCE};
use ds3_blaze_core::server::Ports;
use futures::{SinkExt, TryStreamExt};
use std::fmt;
use std::io::{self, ErrorKind};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::Duration;
use tdf::prelude::{TdfDeserializer, TdfSerialize, TdfSerializer};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::{TcpStream, UdpSocket};
use tokio_util::codec::Framed;

/// Longest time one stage waits, connecting included.
// A packet dropped over the VPN then costs seconds rather than the system's
// TCP connect timeout.
pub const STAGE_TIMEOUT: Duration = Duration::from_secs(5);

/// Part of the game's way to the server, in the order the game takes them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    /// The redirector's name resolved through the hosts file.
    Resolve,
    /// `getServerInstance` at the redirector over SSLv3.
    Redirector,
    /// `preAuth` at the main server the redirector hands out.
    Blaze,
    /// The QoS request over HTTP.
    QosHttp,
    /// The QoS bandwidth probe over UDP.
    QosUdp,
}

impl fmt::Display for Stage {
    /// Writes the stage's name as the console shows it.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Stage::Resolve => "hosts entry",
            Stage::Redirector => "redirector",
            Stage::Blaze => "Blaze server",
            Stage::QosHttp => "QoS HTTP",
            Stage::QosUdp => "QoS UDP",
        })
    }
}

/// Result of one stage: what was seen, or what failed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Report {
    pub stage: Stage,
    pub result: Result<String, String>,
}

/// Returns whether `addresses`, what the redirector's name resolves to,
/// starts with `expected`, the address the game should reach.
pub fn resolved(addresses: &[Ipv4Addr], expected: Ipv4Addr) -> Result<String, String> {
    let name = REDIRECTED[0];
    // The game connects to the first address the resolver returns.
    match addresses.first() {
        Some(&first) if first == expected => Ok(format!("{name} resolves to {expected}")),
        Some(_) => {
            let list: Vec<String> = addresses.iter().map(Ipv4Addr::to_string).collect();
            Err(format!(
                "{name} resolves to {} instead of {expected}: the hosts entry is missing \
                 or points elsewhere",
                list.join(", ")
            ))
        }
        None => Err(format!(
            "{name} resolves to no IPv4 address; the hosts entry should give {expected}"
        )),
    }
}

/// Runs every stage: the redirector's name against `expected`, then the
/// server at `address` on `ports`.
pub async fn run(expected: Ipv4Addr, address: Ipv4Addr, ports: Ports) -> Vec<Report> {
    let resolve = Report {
        stage: Stage::Resolve,
        result: lookup(ports.redirector)
            .await
            .and_then(|addresses| resolved(&addresses, expected)),
    };
    // The server stages connect to `address` directly, so they run whatever
    // the name resolves to.
    let mut reports = vec![resolve];
    reports.extend(server(address, ports).await);
    reports
}

/// Runs the stages after resolving against the server at `address`: the
/// redirector on `ports.redirector`, the main server it hands out, and QoS on
/// `ports.qos`.
///
/// The redirector stage fails when the server hands out an address other
/// than `address`; the Blaze stage is then not tried.
pub async fn server(address: Ipv4Addr, ports: Ports) -> Vec<Report> {
    let at = |port| SocketAddr::from((address, port));
    let redirector = within(at(ports.redirector), instance(at(ports.redirector)))
        .await
        .and_then(|target| {
            if target.ip() == IpAddr::V4(address) {
                Ok(target)
            } else {
                Err(format!(
                    "sends the game to {target} instead of {address}: \
                     the host's server advertises another address"
                ))
            }
        });
    let blaze = match &redirector {
        Ok(target) => within(*target, pre_auth(*target)).await,
        Err(_) => Err("not tried: the redirector stage failed".into()),
    };
    let qos_http = within(at(ports.qos.http), qos_http(at(ports.qos.http))).await;
    let qos_udp = within(at(ports.qos.udp), qos_udp(at(ports.qos.udp))).await;
    vec![
        Report {
            stage: Stage::Redirector,
            result: redirector.map(|target| format!("sends the game to {target}")),
        },
        Report {
            stage: Stage::Blaze,
            result: blaze,
        },
        Report {
            stage: Stage::QosHttp,
            result: qos_http,
        },
        Report {
            stage: Stage::QosUdp,
            result: qos_udp,
        },
    ]
}

/// Returns the IPv4 addresses the redirector's name resolves to, through the
/// system resolver the game uses as well.
async fn lookup(port: u16) -> Result<Vec<Ipv4Addr>, String> {
    let name = REDIRECTED[0];
    let addresses = tokio::time::timeout(STAGE_TIMEOUT, tokio::net::lookup_host((name, port)))
        .await
        .map_err(|_| {
            format!(
                "{name} did not resolve within {} s",
                STAGE_TIMEOUT.as_secs()
            )
        })?
        .map_err(|err| format!("{name} does not resolve: {err}"))?;
    Ok(addresses
        .filter_map(|address| match address.ip() {
            IpAddr::V4(ip) => Some(ip),
            IpAddr::V6(_) => None,
        })
        .collect())
}

/// Runs `stage` against `address` for [`STAGE_TIMEOUT`] at most and returns
/// its result, or the error after `address`.
async fn within<T>(
    address: SocketAddr,
    stage: impl Future<Output = io::Result<T>>,
) -> Result<T, String> {
    match tokio::time::timeout(STAGE_TIMEOUT, stage).await {
        Ok(Ok(value)) => Ok(value),
        Ok(Err(err)) => Err(format!("{address}: {err}")),
        Err(_) => Err(format!(
            "{address} gave no answer within {} s",
            STAGE_TIMEOUT.as_secs()
        )),
    }
}

/// Asks the redirector at `redirector` for the main server and returns the
/// address it hands out.
async fn instance(redirector: SocketAddr) -> io::Result<SocketAddr> {
    let stream = BlazeStream::connect(redirector).await?;
    let mut framed = Framed::new(stream, FireCodec::default());
    let reply = call(
        &mut framed,
        Frame::request(0, COMPONENT, GET_SERVER_INSTANCE, InstanceRequest),
    )
    .await?;
    if !redirector::is_instance_response(&reply.header) {
        return Err(io::Error::other(
            "the reply is not a getServerInstance response",
        ));
    }
    redirector::read_instance(&reply.contents)
        .map(|instance| SocketAddr::from((instance.ip, instance.port)))
        .ok_or_else(|| io::Error::other("the reply names no server"))
}

/// Sends `preAuth` to the main server at `target` and returns what it
/// answered.
async fn pre_auth(target: SocketAddr) -> io::Result<String> {
    let stream = TcpStream::connect(target).await?;
    let mut framed = Framed::new(stream, FireCodec::default());
    let reply = call(
        &mut framed,
        Frame::request(1, util::COMPONENT, util::PRE_AUTH, PreAuthRequest),
    )
    .await?;
    let instance: String = TdfDeserializer::new(&reply.contents)
        .tag(b"INST")
        .map_err(|_| io::Error::other("the preAuth reply has no INST"))?;
    Ok(format!("{target} answers preAuth as \"{instance}\""))
}

/// Requests the QoS page at `qos` and returns what it answered.
async fn qos_http(qos: SocketAddr) -> io::Result<String> {
    let body = http_get(qos, "/qos/qos?vers=1&qtyp=1&prpt=3659").await?;
    if !body.contains("<qosport>") {
        return Err(io::Error::other("the reply is not a QoS page"));
    }
    Ok(format!("{qos} answers the QoS request"))
}

/// Sends the bandwidth probe the game sends to `qos` and returns what it
/// answered.
async fn qos_udp(qos: SocketAddr) -> io::Result<String> {
    let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).await?;
    socket.connect(qos).await?;
    let mut probe = vec![0u8; 1200];
    for (offset, word) in [(0, 6u32), (4, 965), (8, 867), (12, 3), (16, 10)] {
        probe[offset..offset + 4].copy_from_slice(&word.to_be_bytes());
    }
    socket.send(&probe).await?;
    let mut reply = [0u8; 2048];
    let size = socket.recv(&mut reply).await?;
    if size != probe.len() || reply[..12] != probe[..12] {
        return Err(io::Error::other("the reply does not echo the probe"));
    }
    Ok(format!("{qos} answers the bandwidth probe"))
}

/// Sends `request` and returns the response with the same sequence number.
async fn call<T: AsyncRead + AsyncWrite + Unpin>(
    framed: &mut Framed<T, FireCodec>,
    request: Frame,
) -> io::Result<Frame> {
    let seq = request.header.seq;
    framed.send(request).await?;
    let reply = framed.try_next().await?.ok_or_else(|| {
        io::Error::new(ErrorKind::UnexpectedEof, "the server closed the connection")
    })?;
    if reply.header.ty != FrameType::Response || reply.header.seq != seq {
        return Err(io::Error::other(format!(
            "expected response {seq}, got {reply}"
        )));
    }
    Ok(reply)
}

/// Sends a GET for `target` to `address` and returns the whole reply.
async fn http_get(address: SocketAddr, target: &str) -> io::Result<String> {
    let mut tcp = TcpStream::connect(address).await?;
    tcp.write_all(
        format!("GET {target} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\n\r\n").as_bytes(),
    )
    .await?;
    let mut response = String::new();
    tcp.read_to_string(&mut response).await?;
    Ok(response)
}

/// `getServerInstance` request with the fields the game identifies itself by.
struct InstanceRequest;

impl TdfSerialize for InstanceRequest {
    fn serialize<S: TdfSerializer>(&self, w: &mut S) {
        w.tag_str(b"BSDK", "3.19.7.0 (3.19.07.0)");
        w.tag_str(b"NAME", "deadspace-3-pc");
        w.tag_str(b"PLAT", "Windows");
    }
}

/// `preAuth` request reduced to the locale the server reads.
struct PreAuthRequest;

impl TdfSerialize for PreAuthRequest {
    fn serialize<S: TdfSerializer>(&self, w: &mut S) {
        w.group(b"CINF", |w| w.tag_u32(b"LOC", u32::from_be_bytes(*b"enUS")));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOST: Ipv4Addr = Ipv4Addr::new(26, 0, 0, 1);

    #[test]
    fn resolving_to_the_expected_address_passes() {
        let text = resolved(&[HOST], HOST).expect("a pass");
        assert!(text.contains("resolves to 26.0.0.1"), "{text}");
    }

    #[test]
    fn resolving_elsewhere_names_both_addresses() {
        let elsewhere = Ipv4Addr::new(159, 153, 64, 175);
        let text = resolved(&[elsewhere], HOST).expect_err("a failure");
        assert!(
            text.contains("159.153.64.175 instead of 26.0.0.1"),
            "{text}"
        );
        // The game takes the first address, so a later match does not count.
        assert!(resolved(&[elsewhere, HOST], HOST).is_err());
    }

    #[test]
    fn resolving_to_nothing_fails() {
        let text = resolved(&[], HOST).expect_err("a failure");
        assert!(text.contains("no IPv4 address"), "{text}");
    }
}
