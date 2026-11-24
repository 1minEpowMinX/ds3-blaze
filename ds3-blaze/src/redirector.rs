//! SSLv3 redirector answering `getServerInstance` with the probe's address.

use crate::fire::{FireCodec, Frame, FrameHeader};
use crate::log;
use blaze_ssl_async::{BlazeServerContext, BlazeStream};
use futures::{SinkExt, TryStreamExt};
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;
use tdf::prelude::{DecodeResult, GroupSlice, TaggedUnion, TdfDeserializeOwned, TdfType, TdfTyped};
use tdf::{TdfDeserializer, TdfSerialize, TdfSerializer};
use tokio::net::{TcpListener, TcpStream};
use tokio::time::timeout;
use tokio_util::codec::Framed;

/// Redirector component id.
pub const COMPONENT: u16 = 0x5;
/// `getServerInstance` command id.
pub const GET_SERVER_INSTANCE: u16 = 0x1;

const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(15);
const IDLE_TIMEOUT: Duration = Duration::from_secs(60);

/// Address the redirector hands out as the main Blaze server.
#[derive(Clone, Copy)]
pub struct Instance {
    pub ip: Ipv4Addr,
    pub port: u16,
    /// Value of `SECU`: whether the client must speak SSLv3 to the instance.
    pub secure: bool,
}

/// `getServerInstance` response body: `ADDR` union (0 = address), `SECU`, `XDNS`.
struct InstanceResponse(Instance);

impl TdfSerialize for InstanceResponse {
    fn serialize<S: TdfSerializer>(&self, w: &mut S) {
        w.tag_union_start(b"ADDR", 0x0);
        w.group(b"VALU", |w| {
            w.tag_u32(b"IP", u32::from(self.0.ip));
            w.tag_u16(b"PORT", self.0.port);
        });
        w.tag_bool(b"SECU", self.0.secure);
        w.tag_bool(b"XDNS", false);
    }
}

/// Accepts redirector connections on `bind` until the listener fails.
pub async fn run(bind: SocketAddr, instance: Instance) -> std::io::Result<()> {
    let listener = TcpListener::bind(bind).await?;
    let context = Arc::new(BlazeServerContext::default());
    log!(
        "redirector",
        "listening on {bind} (SSLv3, bypass certificate)"
    );
    loop {
        let (tcp, peer) = listener.accept().await?;
        tokio::spawn(handle(tcp, peer, context.clone(), instance));
    }
}

async fn handle(
    tcp: TcpStream,
    peer: SocketAddr,
    context: Arc<BlazeServerContext>,
    instance: Instance,
) {
    log!("redirector", "{peer}: TCP accepted, starting SSL handshake");

    let stream = match timeout(HANDSHAKE_TIMEOUT, BlazeStream::accept(tcp, context)).await {
        Ok(Ok(stream)) => stream,
        Ok(Err(err)) => {
            log!(
                "redirector",
                "{peer}: SSL handshake FAILED: {err} ({:?})",
                err.kind()
            );
            return;
        }
        Err(_) => {
            log!("redirector", "{peer}: SSL handshake timed out");
            return;
        }
    };
    log!("redirector", "{peer}: SSL handshake OK");

    let mut framed = Framed::new(stream, FireCodec::default());
    loop {
        let frame = match timeout(IDLE_TIMEOUT, framed.try_next()).await {
            Ok(Ok(Some(frame))) => frame,
            Ok(Ok(None)) => {
                log!("redirector", "{peer}: closed by client");
                return;
            }
            Ok(Err(err)) => {
                log!("redirector", "{peer}: read error: {err}");
                return;
            }
            Err(_) => {
                log!("redirector", "{peer}: idle timeout");
                return;
            }
        };
        log!("redirector", "{peer}: <- {frame}");

        let header = &frame.header;
        let reply = if header.component == COMPONENT && header.command == GET_SERVER_INSTANCE {
            log_service_name(peer, &frame);
            instance_frame(header, instance)
        } else {
            Frame::response_empty(header)
        };
        if let Err(err) = send(&mut framed, peer, reply).await {
            log!("redirector", "{peer}: write error: {err}");
            return;
        }
    }
}

fn log_service_name(peer: SocketAddr, frame: &Frame) {
    let mut r = TdfDeserializer::new(&frame.contents);
    match r.try_tag::<String>(b"NAME") {
        Ok(Some(name)) => log!("redirector", "{peer}: getServerInstance NAME=\"{name}\""),
        Ok(None) => log!("redirector", "{peer}: getServerInstance without NAME tag"),
        Err(err) => log!(
            "redirector",
            "{peer}: getServerInstance NAME unreadable: {err:?}"
        ),
    }
}

async fn send<T>(
    framed: &mut Framed<T, FireCodec>,
    peer: SocketAddr,
    frame: Frame,
) -> std::io::Result<()>
where
    T: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    log!("redirector", "{peer}: -> {frame}");
    framed.send(frame).await
}

/// Returns whether `header` belongs to a `getServerInstance` exchange.
pub fn is_instance_response(header: &FrameHeader) -> bool {
    header.component == COMPONENT && header.command == GET_SERVER_INSTANCE
}

/// Returns the `getServerInstance` response to `request` that points at `instance`.
pub fn instance_frame(request: &FrameHeader, instance: Instance) -> Frame {
    Frame::response(request, InstanceResponse(instance))
}

/// `ADDR.VALU` group of a `getServerInstance` response.
struct InstanceAddress(Ipv4Addr, u16);

impl TdfTyped for InstanceAddress {
    const TYPE: TdfType = TdfType::Group;
}

impl TdfDeserializeOwned for InstanceAddress {
    fn deserialize_owned(r: &mut TdfDeserializer<'_>) -> DecodeResult<Self> {
        let ip: u32 = r.tag(b"IP")?;
        let port: u16 = r.tag(b"PORT")?;
        GroupSlice::deserialize_content_skip(r)?;
        Ok(Self(Ipv4Addr::from(ip), port))
    }
}

/// Returns the instance a `getServerInstance` response body points at, or `None`
/// when `ADDR.VALU` is missing or unreadable.
pub fn read_instance(contents: &[u8]) -> Option<Instance> {
    let mut r = TdfDeserializer::new(contents);
    let addr: TaggedUnion<InstanceAddress> = r.tag(b"ADDR").ok()?;
    let InstanceAddress(ip, port) = Option::<InstanceAddress>::from(addr)?;
    let secure = r.tag::<bool>(b"SECU").unwrap_or(false);
    Some(Instance { ip, port, secure })
}
