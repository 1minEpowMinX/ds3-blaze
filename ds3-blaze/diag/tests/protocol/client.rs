//! Client side of the game's sequence against a ds3-blaze server: redirector,
//! login, a room with a second client joining, matchmaking, QoS over HTTP and UDP.

use blaze_ssl_async::BlazeStream;
use ds3_blaze_core::components::{
    association_lists as assoc, authentication as auth, game_manager as gm, user_sessions as us,
    util,
};
use ds3_blaze_core::fire::{FireCodec, Frame, FrameType, hex};
use ds3_blaze_core::redirector::{self, COMPONENT, GET_SERVER_INSTANCE};
use futures::{SinkExt, TryStreamExt};
use std::io::{Error, ErrorKind};
use std::net::{Ipv4Addr, SocketAddr};
use std::time::Duration;
use tdf::TdfStringifier;
use tdf::prelude::{
    DecodeResult, GroupSlice, ObjectId, TdfDeserializeOwned, TdfDeserializer, TdfSerialize,
    TdfSerializer, TdfType, TdfTyped,
};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::{TcpStream, UdpSocket};
use tokio_util::codec::Framed;

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
        w.group(b"CINF", |w| w.tag_u32(b"LOC", u32::from_be_bytes(*b"ruRU")));
    }
}

/// `originLogin` request with a placeholder token.
struct OriginLoginRequest;

impl TdfSerialize for OriginLoginRequest {
    fn serialize<S: TdfSerializer>(&self, w: &mut S) {
        w.tag_str(b"AUTH", "selftest-token");
        w.tag_u8(b"TYPE", 1);
    }
}

/// `updateNetworkInfo` request with a loopback address pair.
struct NetworkRequest;

impl TdfSerialize for NetworkRequest {
    fn serialize<S: TdfSerializer>(&self, w: &mut S) {
        w.tag_union_start(b"ADDR", 0x2);
        w.group(b"VALU", |w| {
            for tag in [b"EXIP", b"INIP"] {
                w.group(tag, |w| {
                    w.tag_u32(b"IP", u32::from(Ipv4Addr::LOCALHOST));
                    w.tag_u16(b"PORT", 3659);
                });
            }
        });
        w.group(b"NQOS", |w| {
            w.tag_u32(b"DBPS", 0);
            w.tag_u8(b"NATT", 0);
            w.tag_u32(b"UBPS", 0);
        });
    }
}

/// `listEntitlements` request for the groups the game asks for.
struct ListEntitlementsRequest;

impl TdfSerialize for ListEntitlementsRequest {
    fn serialize<S: TdfSerializer>(&self, w: &mut S) {
        w.tag_list_slice(b"GNLS", &["DeadSpace", "DeadSpace3", "DeadSpace3PC"]);
    }
}

/// `getLists` request for one association list of type 1.
struct GetListsRequest;

impl TdfSerialize for GetListsRequest {
    fn serialize<S: TdfSerializer>(&self, w: &mut S) {
        w.tag_list_start(b"ALST", TdfType::Group, 1);
        w.group_body(|w| {
            w.tag_u8(b"FLGS", 1);
            w.group(b"LID", |w| {
                w.tag_str_empty(b"LNM");
                w.tag_u8(b"TYPE", 1);
            });
        });
    }
}

/// Game attributes of a `createGame` request, in the case-insensitive key order
/// Dead Space 3 sends them in.
const CREATE_GAME_ATTRIBUTES: [(&str, &str); 4] = [
    ("__StreamListKey", "ds_2"),
    ("chapterNumber", "1"),
    ("COOP_REVIVE", "1"),
    ("GAME_MODE", "0"),
];

/// `createGame` request with the fields Dead Space 3 sends for a public co-op game.
struct CreateGameRequest;

impl TdfSerialize for CreateGameRequest {
    fn serialize<S: TdfSerializer>(&self, w: &mut S) {
        w.tag_map_tuples(b"ATTR", &CREATE_GAME_ATTRIBUTES);
        w.tag_str(b"GNAM", "Dead Space 3 (public)");
        w.tag_u16(b"GSET", 287);
        w.tag_u8(b"NTOP", 130);
        w.tag_list_slice::<u16>(b"PCAP", &[2, 0]);
        w.tag_u16(b"PMAX", 8);
        w.tag_u8(b"PRES", 1);
        w.tag_u8(b"VOIP", 2);
        w.tag_str(b"VSTR", "Feb 15 2013 11:50:11");
    }
}

/// `advanceGameState` request moving game `0` to `PRE_GAME`.
struct AdvanceStateRequest(u32);

impl TdfSerialize for AdvanceStateRequest {
    fn serialize<S: TdfSerializer>(&self, w: &mut S) {
        w.tag_u32(b"GID", self.0);
        w.tag_u8(b"GSTA", 0x82);
    }
}

/// Request body holding only a game id (`finalizeGameCreation`).
struct GameRequest(u32);

impl TdfSerialize for GameRequest {
    fn serialize<S: TdfSerializer>(&self, w: &mut S) {
        w.tag_u32(b"GID", self.0);
    }
}

/// `setPlayerAttributes` request as Dead Space 3 sends it for the host: game id,
/// then player id.
struct PlayerAttributesRequest(u32, u32);

impl TdfSerialize for PlayerAttributesRequest {
    fn serialize<S: TdfSerializer>(&self, w: &mut S) {
        w.tag_map_tuples(b"ATTR", &[("UNIFORM_ATTR", "suit")]);
        w.tag_u32(b"GID", self.0);
        w.tag_u32(b"PID", self.1);
    }
}

/// `removePlayer` request for a player leaving (`PLAYER_LEFT`): game id, then
/// player id.
struct RemovePlayerRequest(u32, u32);

impl TdfSerialize for RemovePlayerRequest {
    fn serialize<S: TdfSerializer>(&self, w: &mut S) {
        w.tag_u32(b"GID", self.0);
        w.tag_u32(b"PID", self.1);
        w.tag_u8(b"REAS", 6);
    }
}

/// `startMatchmaking` request reduced to its duration in milliseconds.
struct MatchmakingRequest(u32);

impl TdfSerialize for MatchmakingRequest {
    fn serialize<S: TdfSerializer>(&self, w: &mut S) {
        w.tag_u32(b"DUR", self.0);
        w.tag_str(b"GVER", "Feb 15 2013 11:50:11");
    }
}

/// `joinGame` request for game `0` by invitation.
struct JoinRequest(u32);

impl TdfSerialize for JoinRequest {
    fn serialize<S: TdfSerializer>(&self, w: &mut S) {
        w.tag_u32(b"GID", self.0);
        w.tag_u8(b"JMET", 4);
        w.tag_u8(b"SLOT", 0);
    }
}

/// `lookupUsers` request for Blaze user `0` by id.
struct LookupRequest(u32);

impl TdfSerialize for LookupRequest {
    fn serialize<S: TdfSerializer>(&self, w: &mut S) {
        w.tag_u8(b"LTYP", 0);
        w.tag_list_start(b"ULST", TdfType::Group, 1);
        w.group_body(|w| {
            w.tag_zero(b"AID");
            w.tag_zero(b"ALOC");
            w.tag_blob_empty(b"EXBB");
            w.tag_zero(b"EXID");
            w.tag_u32(b"ID", self.0);
            w.tag_str_empty(b"NAME");
        });
    }
}

/// `updateMeshConnection` request reporting the link to player `1` in game `0`
/// as connected.
struct MeshConnectedRequest(u32, u32);

impl TdfSerialize for MeshConnectedRequest {
    fn serialize<S: TdfSerializer>(&self, w: &mut S) {
        w.tag_u32(b"GID", self.0);
        w.tag_list_start(b"TARG", TdfType::Group, 1);
        w.group_body(|w| {
            w.tag_u8(b"FLGS", 0);
            w.tag_u32(b"PID", self.1);
            w.tag_u8(b"STAT", 2);
        });
    }
}

/// Empty request body.
struct Empty;

impl TdfSerialize for Empty {
    fn serialize<S: TdfSerializer>(&self, _: &mut S) {}
}

/// Runs redirect, login and QoS checks against the server at `host`.
///
/// Returns an error describing the first step that did not behave as the
/// game expects.
pub async fn run(
    host: Ipv4Addr,
    redirector_port: u16,
    qos_http_port: u16,
    qos_udp_port: u16,
) -> std::io::Result<()> {
    let redirector_addr = SocketAddr::from((host, redirector_port));
    println!("1. redirector {redirector_addr} over SSLv3");
    let mut framed = Framed::new(
        BlazeStream::connect(redirector_addr).await?,
        FireCodec::default(),
    );
    let reply = call(
        &mut framed,
        Frame::request(0, COMPONENT, GET_SERVER_INSTANCE, InstanceRequest),
    )
    .await?;
    if !redirector::is_instance_response(&reply.header) {
        return Err(Error::other("reply is not a getServerInstance response"));
    }
    let target = redirector::read_instance(&reply.contents)
        .map(|i| SocketAddr::from((i.ip, i.port)))
        .ok_or_else(|| Error::other("cannot read ADDR.VALU from the redirector reply"))?;
    println!("   redirected to {target}");

    println!("2. main server {target}");
    let (mut framed, player_id) = log_in(target).await?;

    let reply = call(
        &mut framed,
        Frame::request(
            5,
            auth::COMPONENT,
            auth::LIST_ENTITLEMENTS,
            ListEntitlementsRequest,
        ),
    )
    .await?;
    let tags: Vec<EntitlementTag> = TdfDeserializer::new(&reply.contents)
        .tag(b"NLST")
        .map_err(|e| Error::other(format!("listEntitlements reply without NLST: {e:?}")))?;
    let tags: Vec<String> = tags.into_iter().map(|t| t.0).collect();
    if tags.is_empty() {
        return Err(Error::other("listEntitlements granted nothing"));
    }
    println!("   listEntitlements OK: {tags:?}");

    let reply = call(
        &mut framed,
        Frame::request(6, assoc::COMPONENT, assoc::GET_LISTS, GetListsRequest),
    )
    .await?;
    if reply.contents.is_empty() {
        return Err(Error::other("getLists reply is empty"));
    }
    println!("   getLists OK");

    let reply = call(
        &mut framed,
        Frame::request(7, gm::COMPONENT, gm::CREATE_GAME, CreateGameRequest),
    )
    .await?;
    let game_id: u32 = TdfDeserializer::new(&reply.contents)
        .tag(b"GID")
        .map_err(|e| Error::other(format!("createGame reply without GID: {e:?}")))?;
    let setup = next(&mut framed).await?;
    if setup.header.ty != FrameType::Notify || setup.header.command != gm::GAME_SETUP {
        return Err(Error::other(format!(
            "expected GameSetup after the createGame reply, got {setup}"
        )));
    }
    let (text, complete) =
        TdfStringifier::<String>::new_string(TdfDeserializer::new(&setup.contents));
    if !complete {
        return Err(Error::other(format!("GameSetup does not decode: {setup}")));
    }
    let positions: Vec<Option<usize>> = CREATE_GAME_ATTRIBUTES
        .iter()
        .map(|(key, _)| text.find(&format!("\"{key}\"")))
        .collect();
    if positions.iter().any(Option::is_none) || !positions.is_sorted() {
        return Err(Error::other(format!(
            "GameSetup ATTR keys out of request order: {setup}"
        )));
    }
    let update = next(&mut framed).await?;
    if update.header.command != us::USER_SESSION_EXTENDED_DATA_UPDATE
        || game_list(&update) != [u64::from(game_id)]
    {
        return Err(Error::other(format!(
            "expected ULST [{game_id}] after GameSetup, got {update}"
        )));
    }
    println!("   createGame OK: reply, GameSetup with ATTR in request order, ULST [{game_id}]");

    call(
        &mut framed,
        Frame::request(
            8,
            gm::COMPONENT,
            gm::FINALIZE_GAME_CREATION,
            GameRequest(game_id),
        ),
    )
    .await?;
    call(
        &mut framed,
        Frame::request(
            9,
            gm::COMPONENT,
            gm::ADVANCE_GAME_STATE,
            AdvanceStateRequest(game_id),
        ),
    )
    .await?;
    expect_notify(&mut framed, gm::COMPONENT, gm::GAME_STATE_CHANGE).await?;
    println!("   finalizeGameCreation, advanceGameState OK");

    call(
        &mut framed,
        Frame::request(
            10,
            gm::COMPONENT,
            gm::SET_PLAYER_ATTRIBUTES,
            PlayerAttributesRequest(game_id, player_id),
        ),
    )
    .await?;
    expect_notify(&mut framed, gm::COMPONENT, gm::PLAYER_ATTRIB_CHANGE).await?;
    println!("   setPlayerAttributes OK");

    println!("   second client:");
    let (mut guest, guest_id) = log_in(target).await?;
    if guest_id == player_id {
        return Err(Error::other("second client got the host's player id"));
    }
    join_and_leave(&mut framed, &mut guest, game_id, guest_id).await?;

    call(
        &mut framed,
        Frame::request(
            13,
            gm::COMPONENT,
            gm::REMOVE_PLAYER,
            RemovePlayerRequest(game_id, player_id),
        ),
    )
    .await?;
    expect_notify(&mut framed, gm::COMPONENT, gm::PLAYER_REMOVED).await?;
    let update = expect_notify(
        &mut framed,
        us::COMPONENT,
        us::USER_SESSION_EXTENDED_DATA_UPDATE,
    )
    .await?;
    if !game_list(&update).is_empty() {
        return Err(Error::other(format!(
            "ULST still lists a game after removePlayer: {update}"
        )));
    }
    expect_sequence(
        &mut guest,
        &[
            (gm::COMPONENT, gm::PLAYER_REMOVED),
            (gm::COMPONENT, gm::GAME_REMOVED),
        ],
    )
    .await?;
    let update = expect_notify(
        &mut guest,
        us::COMPONENT,
        us::USER_SESSION_EXTENDED_DATA_UPDATE,
    )
    .await?;
    if !game_list(&update).is_empty() {
        return Err(Error::other(format!(
            "guest ULST still lists the game after the host left: {update}"
        )));
    }
    println!("   removePlayer OK: the guest got PlayerRemoved, GameRemoved and an empty ULST");

    let reply = call(
        &mut framed,
        Frame::request(
            14,
            gm::COMPONENT,
            gm::START_MATCHMAKING,
            MatchmakingRequest(200),
        ),
    )
    .await?;
    let session: u32 = TdfDeserializer::new(&reply.contents)
        .tag(b"MSID")
        .map_err(|e| Error::other(format!("startMatchmaking reply without MSID: {e:?}")))?;
    let failed = expect_notify(&mut framed, gm::COMPONENT, gm::MATCHMAKING_FAILED).await?;
    if TdfDeserializer::new(&failed.contents)
        .tag::<u32>(b"MSID")
        .ok()
        != Some(session)
    {
        return Err(Error::other(format!(
            "MatchmakingFailed for another session: {failed}"
        )));
    }
    println!("   startMatchmaking OK: session {session} timed out");

    println!("3. QoS HTTP {host}:{qos_http_port}");
    let body = http_get(
        SocketAddr::from((host, qos_http_port)),
        "/qos/qos?vers=1&qtyp=1&prpt=3659",
    )
    .await?;
    if !body.contains("<qosport>") {
        return Err(Error::other(format!("unexpected QoS body: {body}")));
    }
    println!("   /qos/qos OK");

    println!("4. QoS UDP {host}:{qos_udp_port}");
    let udp = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).await?;
    udp.connect((host, qos_udp_port)).await?;
    let mut probe = vec![0u8; 1200];
    for (at, word) in [(0, 6u32), (4, 965), (8, 867), (12, 3), (16, 10)] {
        probe[at..at + 4].copy_from_slice(&word.to_be_bytes());
    }
    udp.send(&probe).await?;
    let mut reply = [0u8; 2048];
    let size = tokio::time::timeout(Duration::from_secs(2), udp.recv(&mut reply))
        .await
        .map_err(|_| Error::other("no reply to the bandwidth probe"))??;
    if size != probe.len()
        || reply[..12] != probe[..12]
        || reply[12..20] != [0, 0, 0, 4, 10, 0, 0, 0]
    {
        return Err(Error::other(format!(
            "bandwidth reply {}",
            hex(&reply[..size.min(32)])
        )));
    }
    println!("   bandwidth probe OK");
    println!("all checks passed");
    Ok(())
}

/// Logs a client in at the main server `target` (`preAuth` to
/// `updateNetworkInfo`) and returns its connection and player id.
async fn log_in(target: SocketAddr) -> std::io::Result<(Framed<TcpStream, FireCodec>, u32)> {
    let mut framed = Framed::new(TcpStream::connect(target).await?, FireCodec::default());
    let reply = call(
        &mut framed,
        Frame::request(1, util::COMPONENT, util::PRE_AUTH, PreAuthRequest),
    )
    .await?;
    let instance: String = TdfDeserializer::new(&reply.contents)
        .tag(b"INST")
        .map_err(|e| Error::other(format!("preAuth reply without INST: {e:?}")))?;
    println!("   preAuth OK, INST=\"{instance}\"");

    framed
        .send(Frame::request(
            2,
            auth::COMPONENT,
            auth::ORIGIN_LOGIN,
            OriginLoginRequest,
        ))
        .await?;
    let mut notifications = Vec::new();
    loop {
        let frame = next(&mut framed).await?;
        match frame.header.ty {
            FrameType::Notify => notifications.push(frame.header.command),
            FrameType::Response if frame.header.seq == 2 => break,
            _ => return Err(Error::other(format!("unexpected frame: {frame}"))),
        }
    }
    if notifications != [us::USER_ADDED, us::USER_UPDATED, us::USER_AUTHENTICATED] {
        return Err(Error::other(format!(
            "originLogin notifications {notifications:?}"
        )));
    }
    println!("   originLogin OK with UserAdded, UserUpdated and UserAuthenticated");

    call(
        &mut framed,
        Frame::request(3, util::COMPONENT, util::POST_AUTH, Empty),
    )
    .await?;
    println!("   postAuth OK");

    framed
        .send(Frame::request(
            4,
            us::COMPONENT,
            us::UPDATE_NETWORK_INFO,
            NetworkRequest,
        ))
        .await?;
    let update = next(&mut framed).await?;
    let response = next(&mut framed).await?;
    if update.header.command != us::USER_SESSION_EXTENDED_DATA_UPDATE || response.header.seq != 4 {
        return Err(Error::other(
            "updateNetworkInfo did not publish session data",
        ));
    }
    let player_id: u32 = TdfDeserializer::new(&update.contents)
        .tag(b"USID")
        .map_err(|e| Error::other(format!("session data without USID: {e:?}")))?;
    println!("   updateNetworkInfo OK, player {player_id}");
    Ok((framed, player_id))
}

async fn next<T: AsyncRead + AsyncWrite + Unpin>(
    framed: &mut Framed<T, FireCodec>,
) -> std::io::Result<Frame> {
    framed
        .try_next()
        .await?
        .ok_or_else(|| Error::new(ErrorKind::UnexpectedEof, "server closed the connection"))
}

/// Sends `request` and returns the response with the same sequence number.
async fn call<T: AsyncRead + AsyncWrite + Unpin>(
    framed: &mut Framed<T, FireCodec>,
    request: Frame,
) -> std::io::Result<Frame> {
    let seq = request.header.seq;
    framed.send(request).await?;
    let reply = next(framed).await?;
    if reply.header.ty != FrameType::Response || reply.header.seq != seq {
        return Err(Error::other(format!(
            "expected response {seq}, got {reply}"
        )));
    }
    Ok(reply)
}

/// Reads the next frame and returns it when it is notification `command` of
/// `component`.
async fn expect_notify<T: AsyncRead + AsyncWrite + Unpin>(
    framed: &mut Framed<T, FireCodec>,
    component: u16,
    command: u16,
) -> std::io::Result<Frame> {
    let frame = next(framed).await?;
    if frame.header.ty != FrameType::Notify
        || frame.header.component != component
        || frame.header.command != command
    {
        return Err(Error::other(format!(
            "expected notification {component:#x}/{command:#x}, got {frame}"
        )));
    }
    Ok(frame)
}

/// Places the client `guest` (player `guest_id`) in the host's game `game_id`
/// twice, through matchmaking and through `lookupUsers` of the host followed by
/// `joinGame`, completing the first join over the mesh and leaving after it;
/// fails on the first notification out of the expected order on either
/// connection.
async fn join_and_leave<T: AsyncRead + AsyncWrite + Unpin>(
    host: &mut Framed<T, FireCodec>,
    guest: &mut Framed<T, FireCodec>,
    game_id: u32,
    guest_id: u32,
) -> std::io::Result<()> {
    let introduce = [
        (us::COMPONENT, us::USER_ADDED),
        (us::COMPONENT, us::USER_UPDATED),
    ];

    call(
        guest,
        Frame::request(
            5,
            gm::COMPONENT,
            gm::START_MATCHMAKING,
            MatchmakingRequest(5000),
        ),
    )
    .await?;
    let frames = expect_sequence(
        guest,
        &[introduce[0], introduce[1], (gm::COMPONENT, gm::GAME_SETUP)],
    )
    .await?;
    let (setup, _) =
        TdfStringifier::<String>::new_string(TdfDeserializer::new(&frames[2].contents));
    if !setup.contains("\"MSID\"") || !setup.contains("\"FIT\"") {
        return Err(Error::other(format!(
            "matchmaking GameSetup without a matchmaking context: {}",
            frames[2]
        )));
    }
    let update = expect_notify(guest, us::COMPONENT, us::USER_SESSION_EXTENDED_DATA_UPDATE).await?;
    if game_list(&update) != [u64::from(game_id)] {
        return Err(Error::other(format!(
            "guest ULST is not [{game_id}]: {update}"
        )));
    }
    expect_sequence(
        host,
        &[
            introduce[0],
            introduce[1],
            (gm::COMPONENT, gm::PLAYER_JOINING),
        ],
    )
    .await?;
    println!("   startMatchmaking OK: joined game {game_id}, host got PlayerJoining");

    let host_id = {
        let (text, _) =
            TdfStringifier::<String>::new_string(TdfDeserializer::new(&frames[0].contents));
        text.lines()
            .find_map(|line| {
                line.trim()
                    .strip_prefix("\"ID\": ")
                    .map(|v| v.trim_end_matches(',').to_string())
            })
            .and_then(|v| v.parse::<u32>().ok())
            .ok_or_else(|| Error::other(format!("UserAdded without USER.ID: {}", frames[0])))?
    };
    call(
        guest,
        Frame::request(
            6,
            gm::COMPONENT,
            gm::UPDATE_MESH_CONNECTION,
            MeshConnectedRequest(game_id, host_id),
        ),
    )
    .await?;
    let completed = [
        (gm::COMPONENT, gm::GAME_PLAYER_STATE_CHANGE),
        (gm::COMPONENT, gm::PLAYER_JOIN_COMPLETED),
    ];
    expect_sequence(guest, &completed).await?;
    expect_sequence(host, &completed).await?;
    println!("   updateMeshConnection OK: PlayerJoinCompleted on both sides");

    call(
        guest,
        Frame::request(
            7,
            gm::COMPONENT,
            gm::REMOVE_PLAYER,
            RemovePlayerRequest(game_id, guest_id),
        ),
    )
    .await?;
    expect_notify(guest, gm::COMPONENT, gm::PLAYER_REMOVED).await?;
    expect_notify(guest, us::COMPONENT, us::USER_SESSION_EXTENDED_DATA_UPDATE).await?;
    expect_notify(host, gm::COMPONENT, gm::PLAYER_REMOVED).await?;
    println!("   guest removePlayer OK");

    let reply = call(
        guest,
        Frame::request(8, us::COMPONENT, us::LOOKUP_USERS, LookupRequest(host_id)),
    )
    .await?;
    let users: Vec<LookedUpUser> = TdfDeserializer::new(&reply.contents)
        .tag(b"ULST")
        .map_err(|e| Error::other(format!("lookupUsers reply without ULST: {e:?} {reply}")))?;
    if users.len() != 1
        || users[0].id != u64::from(host_id)
        || users[0].games != [u64::from(game_id)]
    {
        return Err(Error::other(format!(
            "lookupUsers did not return the host in game {game_id}: {reply}"
        )));
    }
    println!("   lookupUsers OK: the host is in game {game_id}");

    let reply = call(
        guest,
        Frame::request(9, gm::COMPONENT, gm::JOIN_GAME, JoinRequest(game_id)),
    )
    .await?;
    if TdfDeserializer::new(&reply.contents)
        .tag::<u32>(b"GID")
        .ok()
        != Some(game_id)
    {
        return Err(Error::other(format!(
            "joinGame reply for another game: {reply}"
        )));
    }
    let frames = expect_sequence(
        guest,
        &[introduce[0], introduce[1], (gm::COMPONENT, gm::GAME_SETUP)],
    )
    .await?;
    let (setup, _) =
        TdfStringifier::<String>::new_string(TdfDeserializer::new(&frames[2].contents));
    if !setup.contains("\"DCTX\": 1") {
        return Err(Error::other(format!(
            "joinGame GameSetup without the join context: {}",
            frames[2]
        )));
    }
    expect_notify(guest, us::COMPONENT, us::USER_SESSION_EXTENDED_DATA_UPDATE).await?;
    expect_sequence(
        host,
        &[
            introduce[0],
            introduce[1],
            (gm::COMPONENT, gm::PLAYER_JOINING),
        ],
    )
    .await?;
    println!("   joinGame OK");
    Ok(())
}

/// Reads one notification per entry of `expected` (component, command) and
/// returns them; fails at the first one that differs.
async fn expect_sequence<T: AsyncRead + AsyncWrite + Unpin>(
    framed: &mut Framed<T, FireCodec>,
    expected: &[(u16, u16)],
) -> std::io::Result<Vec<Frame>> {
    let mut frames = Vec::with_capacity(expected.len());
    for &(component, command) in expected {
        frames.push(expect_notify(framed, component, command).await?);
    }
    Ok(frames)
}

/// Returns the game ids in `DATA.ULST` of a `UserSessionExtendedDataUpdate`,
/// empty when the list is absent.
fn game_list(update: &Frame) -> Vec<u64> {
    let mut r = TdfDeserializer::new(&update.contents);
    r.group(b"DATA", |r| r.try_tag::<Vec<ObjectId>>(b"ULST"))
        .ok()
        .flatten()
        .map(|ids| ids.iter().map(|id| id.id).collect())
        .unwrap_or_default()
}

async fn http_get(addr: SocketAddr, target: &str) -> std::io::Result<String> {
    let mut tcp = TcpStream::connect(addr).await?;
    tcp.write_all(format!("GET {target} HTTP/1.1\r\nHost: {addr}\r\n\r\n").as_bytes())
        .await?;
    let mut response = String::new();
    tcp.read_to_string(&mut response).await?;
    Ok(response)
}

/// `TAG` of an entitlement in a `listEntitlements` response.
struct EntitlementTag(String);

impl TdfTyped for EntitlementTag {
    const TYPE: TdfType = TdfType::Group;
}

impl TdfDeserializeOwned for EntitlementTag {
    fn deserialize_owned(r: &mut TdfDeserializer<'_>) -> DecodeResult<Self> {
        let tag: String = r.tag(b"TAG")?;
        GroupSlice::deserialize_content_skip(r)?;
        Ok(Self(tag))
    }
}

/// One `UserData` entry of a `lookupUsers` response: its games and Blaze id.
struct LookedUpUser {
    games: Vec<u64>,
    id: u64,
}

impl TdfTyped for LookedUpUser {
    const TYPE: TdfType = TdfType::Group;
}

impl TdfDeserializeOwned for LookedUpUser {
    fn deserialize_owned(r: &mut TdfDeserializer<'_>) -> DecodeResult<Self> {
        let games = r
            .group(b"EDAT", |r| r.try_tag::<Vec<ObjectId>>(b"ULST"))?
            .unwrap_or_default()
            .iter()
            .map(|id| id.id)
            .collect();
        let id = r.group(b"USER", |r| r.tag::<u64>(b"ID"))?;
        GroupSlice::deserialize_content_skip(r)?;
        Ok(Self { games, id })
    }
}
