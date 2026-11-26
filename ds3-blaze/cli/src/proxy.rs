//! Pass-through mode: the game's Blaze traffic goes to EA's servers through this
//! process, which logs every frame in clear text.
//!
//! The game reaches the local redirector through the hosts file. Its
//! `getServerInstance` request is forwarded to EA's redirector, the answer is
//! remembered as the upstream instance, and the game is sent to the local main
//! port in plain TCP. Each connection to that port is relayed byte for byte to
//! the upstream instance.

use blaze_ssl_async::{BlazeServerContext, BlazeStream};
use bytes::BytesMut;
use ds3_blaze_core::fire::{FireCodec, Frame};
use ds3_blaze_core::log;
use ds3_blaze_core::redirector::{self, COMPONENT, GET_SERVER_INSTANCE, Instance};
use futures::{SinkExt, TryStreamExt};
use std::io::{Error, ErrorKind};
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use tdf::{TdfSerialize, TdfSerializer};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio_util::codec::{Decoder, Framed};

/// Address of EA's redirector `gosredirector.ea.com`, which the hosts file maps
/// to this machine.
pub const EA_REDIRECTOR: SocketAddr =
    SocketAddr::new(std::net::IpAddr::V4(Ipv4Addr::new(159, 153, 51, 18)), 42127);

/// Upstream instance learned from EA's redirector, shared by both listeners.
type Upstream = Arc<Mutex<Option<Instance>>>;

/// Runs the local redirector on `redirector_bind` and the relay on `relay_bind`
/// until a listener fails.
pub async fn run(
    redirector_bind: SocketAddr,
    relay_bind: SocketAddr,
    ea_redirector: SocketAddr,
) -> std::io::Result<()> {
    let upstream: Upstream = Arc::default();
    let local = Instance {
        ip: match relay_bind.ip() {
            std::net::IpAddr::V4(ip) => ip,
            std::net::IpAddr::V6(_) => Ipv4Addr::LOCALHOST,
        },
        port: relay_bind.port(),
        secure: false,
    };
    tokio::try_join!(
        run_redirector(redirector_bind, ea_redirector, local, upstream.clone()),
        run_relay(relay_bind, upstream),
    )?;
    Ok(())
}

async fn run_redirector(
    bind: SocketAddr,
    ea_redirector: SocketAddr,
    local: Instance,
    upstream: Upstream,
) -> std::io::Result<()> {
    let listener = TcpListener::bind(bind).await?;
    let context = Arc::new(BlazeServerContext::default());
    log!(
        "redirector",
        "listening on {bind}, forwarding getServerInstance to EA at {ea_redirector}"
    );
    loop {
        let (tcp, peer) = listener.accept().await?;
        let context = context.clone();
        let upstream = upstream.clone();
        tokio::spawn(async move {
            if let Err(err) = redirect(tcp, context, ea_redirector, local, upstream).await {
                log!("redirector", "{peer}: {err}");
            }
        });
    }
}

/// Answers the game's `getServerInstance` requests on `tcp` with `local` after
/// storing EA's answer in `upstream`.
async fn redirect(
    tcp: TcpStream,
    context: Arc<BlazeServerContext>,
    ea_redirector: SocketAddr,
    local: Instance,
    upstream: Upstream,
) -> std::io::Result<()> {
    let mut game = Framed::new(
        BlazeStream::accept(tcp, context).await?,
        FireCodec::default(),
    );
    while let Some(request) = game.try_next().await? {
        log!("redirector", "<- {request}");
        let header = request.header.clone();
        if header.component != COMPONENT || header.command != GET_SERVER_INSTANCE {
            game.send(Frame::response_empty(&header)).await?;
            continue;
        }
        let answer = ask_ea(ea_redirector, request).await?;
        log!("redirector", "EA -> {answer}");
        let instance = redirector::read_instance(&answer.contents)
            .ok_or_else(|| Error::other("EA's getServerInstance answer has no ADDR.VALU"))?;
        log!(
            "redirector",
            "upstream instance {}:{} {}",
            instance.ip,
            instance.port,
            if instance.secure {
                "over SSLv3"
            } else {
                "in plain TCP"
            }
        );
        *upstream.lock().unwrap_or_else(|e| e.into_inner()) = Some(instance);
        let reply = redirector::instance_frame(&header, local);
        log!("redirector", "-> {reply}");
        game.send(reply).await?;
    }
    Ok(())
}

/// Sends `request` to EA's redirector at `addr` and returns its response.
async fn ask_ea(addr: SocketAddr, request: Frame) -> std::io::Result<Frame> {
    let mut ea = Framed::new(BlazeStream::connect(addr).await?, FireCodec::default());
    ea.send(request).await?;
    ea.try_next().await?.ok_or_else(|| {
        Error::new(
            ErrorKind::UnexpectedEof,
            "EA's redirector closed the connection",
        )
    })
}

async fn run_relay(bind: SocketAddr, upstream: Upstream) -> std::io::Result<()> {
    let listener = TcpListener::bind(bind).await?;
    log!("proxy", "relay listening on {bind}");
    let counter = AtomicU32::new(0);
    loop {
        let (tcp, peer) = listener.accept().await?;
        let tag: &'static str = Box::leak(
            format!("proxy#{}", counter.fetch_add(1, Ordering::Relaxed) + 1).into_boxed_str(),
        );
        let target = *upstream.lock().unwrap_or_else(|e| e.into_inner());
        tokio::spawn(async move {
            let Some(target) = target else {
                log!(tag, "{peer}: no upstream instance yet, closing");
                return;
            };
            match relay(tcp, target, tag).await {
                Ok(()) => log!(tag, "closed"),
                Err(err) => log!(tag, "closed: {err}"),
            }
        });
    }
}

/// Relays `game` to `target` until either side closes, logging the frames of both
/// directions under `tag`.
async fn relay(game: TcpStream, target: Instance, tag: &'static str) -> std::io::Result<()> {
    let addr = SocketAddr::from((target.ip, target.port));
    log!(tag, "connecting to EA at {addr}");
    let (game_rx, game_tx) = game.into_split();
    if target.secure {
        let (ea_rx, ea_tx) = tokio::io::split(BlazeStream::connect(addr).await?);
        log!(tag, "connected over SSLv3");
        pump_both(game_rx, game_tx, ea_rx, ea_tx, tag).await
    } else {
        let (ea_rx, ea_tx) = TcpStream::connect(addr).await?.into_split();
        log!(tag, "connected in plain TCP");
        pump_both(game_rx, game_tx, ea_rx, ea_tx, tag).await
    }
}

async fn pump_both<GR, GW, ER, EW>(
    game_rx: GR,
    game_tx: GW,
    ea_rx: ER,
    ea_tx: EW,
    tag: &'static str,
) -> std::io::Result<()>
where
    GR: AsyncRead + Unpin,
    GW: AsyncWrite + Unpin,
    ER: AsyncRead + Unpin,
    EW: AsyncWrite + Unpin,
{
    tokio::select! {
        r = pump(game_rx, ea_tx, tag, "<-") => r,
        r = pump(ea_rx, game_tx, tag, "->") => r,
    }
}

/// Copies `from` to `to` and logs each complete frame with `arrow` (`<-` for the
/// game's frames, `->` for EA's).
async fn pump<R, W>(
    mut from: R,
    mut to: W,
    tag: &'static str,
    arrow: &'static str,
) -> std::io::Result<()>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let mut buf = vec![0u8; 16 * 1024];
    let mut pending = BytesMut::new();
    let mut codec = FireCodec::default();
    loop {
        let n = from.read(&mut buf).await?;
        if n == 0 {
            let _ = to.shutdown().await;
            return Ok(());
        }
        to.write_all(&buf[..n]).await?;
        to.flush().await?;
        pending.extend_from_slice(&buf[..n]);
        while let Some(frame) = codec.decode(&mut pending)? {
            log!(tag, "{arrow} {frame}");
        }
    }
}

/// `getServerInstance` request with the fields Dead Space 3 identifies itself by.
struct GameInstanceRequest;

impl TdfSerialize for GameInstanceRequest {
    fn serialize<S: TdfSerializer>(&self, w: &mut S) {
        w.tag_str(b"BSDK", "3.19.7.0 (3.19.07.0)");
        w.tag_str(b"BTIM", "Nov 27 2012 13:44:59");
        w.tag_str(b"CLNT", "/PC/DEADSPACE-3");
        w.tag_u8(b"CLTP", 0);
        w.tag_u8(b"CPLT", 4);
        w.tag_str(b"CSKU", "DS3");
        w.tag_str(b"CVER", "1.0");
        w.tag_str(b"DSDK", "8.18.8.0");
        w.tag_str(b"ENV", "prod");
        w.tag_u32(b"LOC", u32::from_be_bytes(*b"enUS"));
        w.tag_str(b"NAME", "deadspace-3-pc");
        w.tag_str(b"PLAT", "Windows");
        w.tag_str(b"PROF", "standardSecure_v3");
    }
}

/// Asks EA's redirector at `addr` for the Dead Space 3 instance and prints the
/// answer; fails when the SSLv3 handshake or the exchange fails.
pub async fn check(addr: SocketAddr) -> std::io::Result<()> {
    println!("EA redirector {addr} over SSLv3 (RC4)");
    let answer = ask_ea(
        addr,
        Frame::request(0, COMPONENT, GET_SERVER_INSTANCE, GameInstanceRequest),
    )
    .await?;
    println!("{answer}");
    let instance = redirector::read_instance(&answer.contents)
        .ok_or_else(|| Error::other("answer has no ADDR.VALU"))?;
    let main = SocketAddr::from((instance.ip, instance.port));
    println!("main server {main} secure={}", instance.secure);
    if instance.secure {
        BlazeStream::connect(main).await?;
        println!("SSLv3 handshake with the main server OK");
    }
    Ok(())
}
