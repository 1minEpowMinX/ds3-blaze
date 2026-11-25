//! Main Blaze server over plain TCP: login stage (`preAuth` to `postAuth`),
//! session upkeep and GameManager games.
//!
//! Requests without a handler are logged as `UNHANDLED` and answered with an
//! empty response, so a client run shows which commands still need one.

use crate::components::{
    association_lists as assoc, authentication as auth, game_manager as gm, user_sessions as us,
    util,
};
use crate::fire::{FireCodec, Frame, FrameType};
use crate::game::{self, Games, Outbox};
use crate::log;
use crate::models::{self, NetData, Player, QosHttp};
use futures::{SinkExt, StreamExt};
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tdf::prelude::{TdfDeserializeOwned, TdfDeserializer, TdfSerialize};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio::task::AbortHandle;
use tokio_util::codec::Framed;

/// Server-wide settings and state shared by all sessions.
pub struct Config {
    /// Address the client is told to reach this machine at (telemetry, ticker, QoS).
    pub advertise: Ipv4Addr,
    pub qos: QosHttp,
    /// Display name of a player connecting from this machine or from `advertise`.
    pub player_name: String,
    /// Display names of players connecting from other addresses.
    pub player_names: Vec<(Ipv4Addr, String)>,
    /// Whether connections are accepted only from addresses [`Config::admits`] lists.
    pub only_listed: bool,
    pub games: Mutex<Games>,
}

impl Config {
    /// Returns whether a connection from `ip` may open a session: always without
    /// `only_listed`, otherwise only from this machine, `advertise` or an address
    /// in `player_names`.
    fn admits(&self, ip: Ipv4Addr) -> bool {
        !self.only_listed
            || ip.is_loopback()
            || ip == self.advertise
            || self.player_names.iter().any(|(addr, _)| *addr == ip)
    }

    /// Returns the display name of a player connecting from `ip`.
    ///
    /// Addresses listed in `player_names` get their entry; this machine and
    /// `advertise` get `player_name`; any other address gets `Player-` followed by
    /// its last octet.
    fn name_for(&self, ip: Ipv4Addr) -> String {
        if let Some((_, name)) = self.player_names.iter().find(|(addr, _)| *addr == ip) {
            return name.clone();
        }
        if ip.is_loopback() || ip == self.advertise {
            return self.player_name.clone();
        }
        format!("Player-{}", ip.octets()[3])
    }

    /// Returns the address other players reach a client connected from `ip` at:
    /// `advertise` for this machine, `ip` otherwise.
    fn reachable_ip(&self, ip: Ipv4Addr) -> Ipv4Addr {
        if ip.is_loopback() { self.advertise } else { ip }
    }
}

/// Accepts sessions on `bind` until the listener fails, closing connections
/// from addresses the configuration does not admit.
pub async fn run(bind: SocketAddr, config: Arc<Config>) -> std::io::Result<()> {
    let listener = TcpListener::bind(bind).await?;
    log!("blaze", "listening on {bind} (plain TCP)");
    if config.only_listed {
        log!(
            "blaze",
            "accepting only this machine, {} and the --player addresses",
            config.advertise
        );
    }
    let next_id = Arc::new(AtomicU32::new(1));
    loop {
        let (tcp, peer) = listener.accept().await?;
        if !config.admits(peer_ipv4(peer)) {
            log!("blaze", "{peer}: refused, address not listed");
            continue;
        }
        let id = next_id.fetch_add(1, Ordering::Relaxed);
        tokio::spawn(serve_session(tcp, peer, id, config.clone()));
    }
}

/// State of one client connection.
struct Session {
    tag: String,
    /// IPv4 address the connection comes from (IPv4-mapped form for IPv6).
    peer_ip: Ipv4Addr,
    config: Arc<Config>,
    tx: mpsc::UnboundedSender<Frame>,
    locale: u32,
    player: Option<Player>,
    net: NetData,
    /// Settings saved with `userSettingsSave` during this connection.
    settings: Vec<(String, String)>,
    /// Notifications sent after the response to the request being handled.
    outbox: Outbox,
    /// Running matchmaking session: its id and the timer that ends it.
    matchmaking: Option<(u32, AbortHandle)>,
}

impl Drop for Session {
    /// Stops matchmaking, removes the session's player from every game it is in
    /// and logs it out.
    fn drop(&mut self) {
        self.stop_matchmaking();
        self.log_out();
    }
}

async fn serve_session(tcp: TcpStream, peer: SocketAddr, id: u32, config: Arc<Config>) {
    let tag = format!("blaze#{id}");
    log!(&tag, "{peer}: connected");
    let (mut sink, mut stream) = Framed::new(tcp, FireCodec::default()).split();
    let (tx, mut rx) = mpsc::unbounded_channel::<Frame>();

    let writer_tag = tag.clone();
    let writer = tokio::spawn(async move {
        while let Some(frame) = rx.recv().await {
            log!(&writer_tag, "-> {frame}");
            if let Err(err) = sink.send(frame).await {
                log!(&writer_tag, "write error: {err}");
                return;
            }
        }
    });

    let mut session = Session {
        tag: tag.clone(),
        peer_ip: peer_ipv4(peer),
        config,
        tx,
        locale: u32::from_be_bytes(*b"enUS"),
        player: None,
        net: NetData::default(),
        settings: Vec::new(),
        outbox: Outbox::default(),
        matchmaking: None,
    };
    loop {
        match stream.next().await {
            Some(Ok(frame)) => {
                log!(&tag, "<- {frame}");
                if frame.header.ty == FrameType::Request {
                    let reply = session.handle(&frame);
                    session.send(reply);
                    // EA's server sends GameManager notifications after the response
                    // that caused them; GameSetup ahead of the createGame response
                    // makes the game abandon the new game.
                    std::mem::take(&mut session.outbox).flush();
                }
            }
            Some(Err(err)) => {
                log!(&tag, "read error: {err}");
                break;
            }
            None => {
                log!(&tag, "closed by client");
                break;
            }
        }
    }
    drop(session);
    let _ = writer.await;
}

impl Session {
    fn games(&self) -> MutexGuard<'_, Games> {
        lock_games(&self.config)
    }

    fn send(&self, frame: Frame) {
        // The writer only stops after the socket fails, which the reader reports too.
        let _ = self.tx.send(frame);
    }

    fn notify<V: TdfSerialize>(&self, component: u16, command: u16, value: V) {
        self.send(Frame::notify(component, command, value));
    }

    /// Returns the response to `frame` after sending any notifications that precede it.
    fn handle(&mut self, frame: &Frame) -> Frame {
        if let Some(denied) = self.deny_game_change(frame) {
            return denied;
        }
        let h = &frame.header;
        let host = self.config.advertise;
        match (h.component, h.command) {
            (util::COMPONENT, util::PRE_AUTH) => {
                if let Some(locale) = read(frame, |r| r.group(b"CINF", |r| r.tag::<u32>(b"LOC"))) {
                    self.locale = locale;
                }
                Frame::response(
                    h,
                    models::PreAuthResponse {
                        qos: self.config.qos,
                    },
                )
            }
            (util::COMPONENT, util::PING) => {
                let now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map(|d| d.as_secs());
                Frame::response(
                    h,
                    models::PingResponse {
                        server_time: now.unwrap_or(0),
                    },
                )
            }
            (util::COMPONENT, util::FETCH_CLIENT_CONFIG) => {
                let id = read(frame, |r| r.tag::<String>(b"CFID")).unwrap_or_default();
                log!(
                    &self.tag,
                    "fetchClientConfig CFID=\"{id}\": answering with an empty map"
                );
                Frame::response(
                    h,
                    models::StringMapResponse {
                        tag: b"CONF",
                        entries: &[],
                    },
                )
            }
            (util::COMPONENT, util::GET_TELEMETRY_SERVER) => {
                Frame::response(h, models::TelemetryResponse { host })
            }
            (util::COMPONENT, util::GET_TICKER_SERVER) => {
                Frame::response(h, models::TickerResponse { host })
            }
            (util::COMPONENT, util::USER_SETTINGS_SAVE) => {
                let key = read(frame, |r| r.tag::<String>(b"KEY"));
                let data = read(frame, |r| r.tag::<String>(b"DATA"));
                if let (Some(key), Some(data)) = (key, data) {
                    log!(
                        &self.tag,
                        "userSettingsSave KEY=\"{key}\" ({} bytes)",
                        data.len()
                    );
                    self.settings.retain(|(k, _)| *k != key);
                    self.settings.push((key, data));
                    self.settings.sort();
                }
                Frame::response_empty(h)
            }
            (util::COMPONENT, util::USER_SETTINGS_LOAD) => {
                let key = read(frame, |r| r.tag::<String>(b"KEY")).unwrap_or_default();
                let data = self
                    .settings
                    .iter()
                    .find(|(k, _)| *k == key)
                    .map(|(_, v)| v.as_str());
                let state = if data.is_some() {
                    "found"
                } else {
                    "not saved, answering empty"
                };
                log!(&self.tag, "userSettingsLoad KEY=\"{key}\": {state}");
                Frame::response(
                    h,
                    models::UserSettingResponse {
                        key: &key,
                        data: data.unwrap_or(""),
                    },
                )
            }
            (util::COMPONENT, util::USER_SETTINGS_LOAD_ALL) => Frame::response(
                h,
                models::StringMapResponse {
                    tag: b"SMAP",
                    entries: &self.settings,
                },
            ),
            (util::COMPONENT, util::POST_AUTH) => match &self.player {
                Some(player) => Frame::response(
                    h,
                    models::PostAuthResponse {
                        host,
                        player_id: player.id,
                    },
                ),
                None => {
                    log!(&self.tag, "postAuth before login");
                    Frame::response_empty(h)
                }
            },
            (auth::COMPONENT, auth::ORIGIN_LOGIN | auth::SILENT_LOGIN) => {
                self.log_out();
                let player = self
                    .games()
                    .unique_player(&self.config.name_for(self.peer_ip), self.locale);
                log!(
                    &self.tag,
                    "login accepted without verification: \"{}\" id={}",
                    player.name,
                    player.id
                );
                // The client sends postAuth only after UserAuthenticated, which a Blaze
                // server queues while handling the login, ahead of the login response.
                self.notify(
                    us::COMPONENT,
                    us::USER_ADDED,
                    models::UserAdded {
                        player: &player,
                        net: &self.net,
                    },
                );
                self.notify(
                    us::COMPONENT,
                    us::USER_UPDATED,
                    models::UserUpdated {
                        flags: models::USER_SUBSCRIBED | models::USER_ONLINE,
                        player_id: player.id,
                    },
                );
                self.notify(
                    us::COMPONENT,
                    us::USER_AUTHENTICATED,
                    models::UserAuthenticated { player: &player },
                );
                let reply = Frame::response(h, models::AuthResponse { player: &player });
                self.games()
                    .log_in(player.clone(), self.net.clone(), self.tx.clone());
                self.player = Some(player);
                reply
            }
            (auth::COMPONENT, auth::LOGIN_PERSONA) => match &self.player {
                Some(player) => Frame::response(h, models::PersonaResponse { player }),
                None => Frame::response_empty(h),
            },
            (auth::COMPONENT, auth::GET_AUTH_TOKEN) => match &self.player {
                Some(player) => Frame::response(h, models::AuthTokenResponse { player }),
                None => Frame::response_empty(h),
            },
            (auth::COMPONENT, auth::LIST_ENTITLEMENTS) => match &self.player {
                Some(player) => {
                    let groups = read(frame, models::ListEntitlementsRequest::deserialize_owned)
                        .map(|request| request.groups)
                        .unwrap_or_default();
                    let reply = models::OnlinePassEntitlements {
                        player,
                        requested: groups.iter().any(|g| g == models::ENTITLEMENT_GROUP),
                    };
                    log!(
                        &self.tag,
                        "listEntitlements groups {groups:?}: granting {:?}",
                        reply.tags()
                    );
                    Frame::response(h, reply)
                }
                None => Frame::response(h, models::EmptyEntitlements),
            },
            (assoc::COMPONENT, assoc::GET_LISTS) => match &self.player {
                Some(player) => {
                    let lists = read(frame, models::GetListsRequest::deserialize_owned)
                        .map(|request| request.lists)
                        .unwrap_or_default();
                    Frame::response(
                        h,
                        models::GetListsResponse {
                            player,
                            lists: &lists,
                        },
                    )
                }
                None => Frame::response_empty(h),
            },
            (auth::COMPONENT, auth::LIST_USER_ENTITLEMENTS_2) => {
                Frame::response(h, models::EmptyEntitlements)
            }
            (auth::COMPONENT, auth::LOGOUT) => {
                self.stop_matchmaking();
                self.log_out();
                Frame::response_empty(h)
            }
            (us::COMPONENT, us::UPDATE_NETWORK_INFO) => {
                match read(frame, models::UpdateNetworkRequest::deserialize_owned) {
                    Some(request) => {
                        log!(
                            &self.tag,
                            "network info: address {:?}, NAT type {}",
                            request.address,
                            request.qos.nat_type
                        );
                        self.net.address = request.address.map(|pair| self.reachable_pair(pair));
                        self.net.qos = request.qos;
                    }
                    None => log!(&self.tag, "updateNetworkInfo body not understood"),
                }
                self.store_net();
                self.publish_net();
                Frame::response_empty(h)
            }
            (us::COMPONENT, us::UPDATE_HARDWARE_FLAGS) => {
                if let Some(flags) = read(frame, |r| r.tag::<u8>(b"HWFG")) {
                    self.net.hardware_flags = flags;
                }
                self.store_net();
                self.publish_net();
                Frame::response_empty(h)
            }
            (util::COMPONENT, util::SET_CLIENT_METRICS) => Frame::response_empty(h),
            (gm::COMPONENT, gm::CREATE_GAME) => match self.player.as_ref().map(|p| p.id) {
                Some(player_id) => {
                    let request = game::CreateGameRequest::read(&frame.contents);
                    let summary = format!(
                        "\"{}\" version \"{}\", settings {:#x}, {} attributes",
                        request.name,
                        request.version,
                        request.settings,
                        request.attributes.len()
                    );
                    match lock_games(&self.config).create(request, player_id, &mut self.outbox) {
                        Some(game_id) => {
                            log!(&self.tag, "createGame: game {game_id} {summary}");
                            Frame::response(h, game::CreateGameResponse { game_id })
                        }
                        None => {
                            log!(&self.tag, "createGame: player {player_id} is not logged in");
                            Frame::error(h, game::ERR_PLAYER_NOT_FOUND)
                        }
                    }
                }
                None => {
                    log!(&self.tag, "createGame before login");
                    Frame::response_empty(h)
                }
            },
            (gm::COMPONENT, gm::ADVANCE_GAME_STATE) => {
                let id = read(frame, |r| r.tag::<u32>(b"GID")).unwrap_or_default();
                let state = read(frame, |r| r.tag::<u8>(b"GSTA")).unwrap_or_default();
                let known = lock_games(&self.config).advance_state(id, state, &mut self.outbox);
                log!(
                    &self.tag,
                    "advanceGameState game {id} -> {state:#x}{}",
                    unknown_suffix(known)
                );
                Frame::response_empty(h)
            }
            (gm::COMPONENT, gm::SET_GAME_SETTINGS) => {
                let id = read(frame, |r| r.tag::<u32>(b"GID")).unwrap_or_default();
                let settings = read(frame, |r| r.tag::<u16>(b"GSET")).unwrap_or_default();
                let known = lock_games(&self.config).set_settings(id, settings, &mut self.outbox);
                log!(
                    &self.tag,
                    "setGameSettings game {id} -> {settings:#x}{}",
                    unknown_suffix(known)
                );
                Frame::response_empty(h)
            }
            (gm::COMPONENT, gm::SET_GAME_ATTRIBUTES) => {
                let id = read(frame, |r| r.tag::<u32>(b"GID")).unwrap_or_default();
                let changes = game::read_attributes(&frame.contents);
                let count = changes.len();
                let known = lock_games(&self.config).set_attributes(id, changes, &mut self.outbox);
                log!(
                    &self.tag,
                    "setGameAttributes game {id}: {count} changed{}",
                    unknown_suffix(known)
                );
                Frame::response_empty(h)
            }
            (gm::COMPONENT, gm::SET_PLAYER_ATTRIBUTES) => {
                let id = read(frame, |r| r.tag::<u32>(b"GID")).unwrap_or_default();
                let player_id = read(frame, |r| r.tag::<u32>(b"PID")).unwrap_or_default();
                let changes = game::read_attributes(&frame.contents);
                let count = changes.len();
                let known = lock_games(&self.config).set_player_attributes(
                    id,
                    player_id,
                    changes,
                    &mut self.outbox,
                );
                log!(
                    &self.tag,
                    "setPlayerAttributes game {id} player {player_id}: {count} changed{}",
                    unknown_suffix(known)
                );
                Frame::response_empty(h)
            }
            (gm::COMPONENT, gm::REMOVE_PLAYER) => {
                let id = read(frame, |r| r.tag::<u32>(b"GID")).unwrap_or_default();
                let player_id = read(frame, |r| r.tag::<u32>(b"PID")).unwrap_or_default();
                let reason = read(frame, |r| r.tag::<u8>(b"REAS")).unwrap_or_default();
                let removed =
                    lock_games(&self.config).remove_player(id, player_id, reason, &mut self.outbox);
                log!(
                    &self.tag,
                    "removePlayer {player_id} from game {id}, reason {reason}: {}",
                    if removed { "removed" } else { "not a member" }
                );
                Frame::response_empty(h)
            }
            (gm::COMPONENT, gm::DESTROY_GAME) => {
                let id = read(frame, |r| r.tag::<u32>(b"GID")).unwrap_or_default();
                let reason = read(frame, |r| r.tag::<u8>(b"REAS")).unwrap_or_default();
                let known = lock_games(&self.config).destroy(id, reason, &mut self.outbox);
                log!(
                    &self.tag,
                    "destroyGame {id}, reason {reason}{}",
                    unknown_suffix(known)
                );
                Frame::response_empty(h)
            }
            (gm::COMPONENT, gm::START_MATCHMAKING) => match self.player.as_ref().map(|p| p.id) {
                Some(player_id) => {
                    let duration =
                        read(frame, |r| r.tag::<u32>(b"DUR")).unwrap_or(DEFAULT_MATCHMAKING_MS);
                    let version = read(frame, |r| r.tag::<String>(b"GVER"));
                    self.stop_matchmaking();
                    self.start_matchmaking(player_id, duration, version, h)
                }
                None => {
                    log!(&self.tag, "startMatchmaking before login");
                    Frame::response_empty(h)
                }
            },
            (gm::COMPONENT, gm::JOIN_GAME) => match self.player.as_ref().map(|p| p.id) {
                Some(player_id) => {
                    let requested = read(frame, |r| r.tag::<u32>(b"GID")).unwrap_or_default();
                    let slot_type = read(frame, |r| r.tag::<u8>(b"SLOT"))
                        .unwrap_or(game::SLOT_PUBLIC_PARTICIPANT);
                    let target_user = read(frame, |r| r.group(b"USER", |r| r.tag::<u32>(b"ID")));
                    let mut games = lock_games(&self.config);
                    // A join by player names the host's user instead of a game.
                    let game_id = match target_user {
                        Some(user) if !games.contains(requested) => {
                            games.games_of(user).first().copied().unwrap_or(requested)
                        }
                        _ => requested,
                    };
                    let joined = games.join(
                        game_id,
                        player_id,
                        slot_type,
                        game::SetupReason::Join,
                        &mut self.outbox,
                    );
                    drop(games);
                    match joined {
                        Ok(ticket) => {
                            log!(
                                &self.tag,
                                "joinGame: joined game {game_id} (requested {requested}, user {target_user:?})"
                            );
                            watch_join(self.config.clone(), self.tag.clone(), ticket);
                            Frame::response(h, game::JoinGameResponse { game_id })
                        }
                        Err(code) => {
                            log!(
                                &self.tag,
                                "joinGame: game {game_id} (requested {requested}, user {target_user:?}) refused with error {code}"
                            );
                            Frame::error(h, code)
                        }
                    }
                }
                None => {
                    log!(&self.tag, "joinGame before login");
                    Frame::response_empty(h)
                }
            },
            (gm::COMPONENT, gm::CANCEL_MATCHMAKING) => {
                let id = read(frame, |r| r.tag::<u32>(b"MSID")).unwrap_or_default();
                match self.matchmaking.as_ref().map(|(session, _)| *session) {
                    Some(running) => log!(
                        &self.tag,
                        "cancelMatchmaking {id}: stopping session {running}"
                    ),
                    None => log!(&self.tag, "cancelMatchmaking {id}: no session running"),
                }
                self.stop_matchmaking();
                Frame::response_empty(h)
            }
            (gm::COMPONENT, gm::UPDATE_MESH_CONNECTION) => {
                let id = read(frame, |r| r.tag::<u32>(b"GID")).unwrap_or_default();
                let links = game::read_mesh_links(&frame.contents);
                let summary: Vec<String> = links
                    .iter()
                    .map(|link| format!("{}:{}", link.player_id, link.status))
                    .collect();
                log!(
                    &self.tag,
                    "updateMeshConnection game {id}: links {summary:?}"
                );
                if let Some(reporter) = self.player.as_ref().map(|p| p.id) {
                    let events = lock_games(&self.config).update_mesh(
                        id,
                        reporter,
                        &links,
                        &mut self.outbox,
                    );
                    for event in events {
                        log!(&self.tag, "{event}");
                    }
                }
                Frame::response_empty(h)
            }
            (gm::COMPONENT, gm::FINALIZE_GAME_CREATION) => Frame::response_empty(h),
            (us::COMPONENT, us::LOOKUP_USERS) => self.lookup_users(frame),
            _ => {
                log!(
                    &self.tag,
                    "UNHANDLED {:#06x}/{:#06x}: answering with an empty response",
                    h.component,
                    h.command
                );
                Frame::response_empty(h)
            }
        }
    }

    /// Returns error `GAMEMANAGER_ERR_PERMISSION_DENIED` for a GameManager request
    /// that changes an existing game on behalf of a player without the right to,
    /// or `None` for any other request.
    ///
    /// The game's state, settings and attributes and the game itself belong to
    /// its host; a member's attributes and membership belong to that member and
    /// to the host. A request naming an unknown game passes, and its handler
    /// answers it as before.
    fn deny_game_change(&self, frame: &Frame) -> Option<Frame> {
        let h = &frame.header;
        if h.component != gm::COMPONENT {
            return None;
        }
        let target = match h.command {
            gm::ADVANCE_GAME_STATE
            | gm::SET_GAME_SETTINGS
            | gm::SET_GAME_ATTRIBUTES
            | gm::DESTROY_GAME => None,
            gm::SET_PLAYER_ATTRIBUTES | gm::REMOVE_PLAYER => read(frame, |r| r.tag::<u32>(b"PID")),
            _ => return None,
        };
        let id = read(frame, |r| r.tag::<u32>(b"GID")).unwrap_or_default();
        let host = self.games().host_of(id)?;
        let requester = self.player.as_ref().map(|p| p.id);
        if requester == Some(host) || (requester.is_some() && requester == target) {
            return None;
        }
        log!(
            &self.tag,
            "{} on game {id} refused: player {requester:?} is not its host{}",
            crate::components::command_name(h.component, h.command, false).unwrap_or("request"),
            target.map_or(String::new(), |t| format!(" nor player {t}"))
        );
        Some(Frame::error(h, game::ERR_PERMISSION_DENIED))
    }

    /// Returns the `lookupUsers` response to `frame`: the logged-in users it names,
    /// or error `USER_ERR_USER_NOT_FOUND` when it names none.
    fn lookup_users(&self, frame: &Frame) -> Frame {
        let h = &frame.header;
        let Some(request) = read(frame, models::LookupUsersRequest::deserialize_owned) else {
            log!(&self.tag, "lookupUsers body not understood");
            return Frame::error(h, USER_ERR_USER_NOT_FOUND);
        };
        let requester = self.player.as_ref().map_or(0, |p| p.id);
        let mut games = self.games();
        let mut found = Vec::new();
        for identity in &request.users {
            let what = format!(
                "type {} (AID {}, ID {}, NAME \"{}\")",
                request.lookup_type, identity.account_id, identity.blaze_id, identity.name
            );
            match games.find_user(requester, request.lookup_type, identity) {
                Some(user) => {
                    let name = games.player_name(user.player_id).unwrap_or_default();
                    let how = if user.bound {
                        ", account id bound to the only other player"
                    } else {
                        ""
                    };
                    log!(
                        &self.tag,
                        "lookupUsers {what}: player {} \"{name}\"{how}",
                        user.player_id
                    );
                    if !found.contains(&user.player_id) {
                        found.push(user.player_id);
                    }
                }
                None => log!(&self.tag, "lookupUsers {what}: not found"),
            }
        }
        if found.is_empty() {
            return Frame::error(h, USER_ERR_USER_NOT_FOUND);
        }
        let users = found.iter().filter_map(|&id| games.user_data(id)).collect();
        Frame::response(h, models::UserDataResponse { users })
    }

    /// Returns `UserSessionExtendedDataUpdate` with the current network state and
    /// game memberships, or `None` before login.
    fn net_update(&self) -> Option<Frame> {
        let player = self.player.as_ref()?;
        self.games().session_update(player.id)
    }

    /// Sends `UserSessionExtendedDataUpdate` now.
    fn publish_net(&self) {
        if let Some(frame) = self.net_update() {
            self.send(frame);
        }
    }

    /// Copies the session's network state into the server's user and game records.
    fn store_net(&self) {
        if let Some(player) = &self.player {
            self.games().update_net(player.id, &self.net);
        }
    }

    /// Returns `pair` with both addresses set to the one other players reach this
    /// client at, keeping the internal port.
    ///
    /// Players meet over a VPN, where the client's LAN address and the external
    /// address QoS saw (this machine's loopback for a local client) are both
    /// unreachable. A reachable address on loopback leaves `pair` unchanged.
    fn reachable_pair(&self, pair: models::AddressPair) -> models::AddressPair {
        let ip = self.config.reachable_ip(self.peer_ip);
        if ip.is_loopback() || ip.is_unspecified() {
            return pair;
        }
        let address = models::IpPort {
            ip,
            port: pair.internal.port,
        };
        log!(
            &self.tag,
            "network address rewritten to {}:{} for players over the VPN",
            address.ip,
            address.port
        );
        models::AddressPair {
            external: address,
            internal: address,
        }
    }

    /// Removes the session's player from every game and from the logged-in users.
    fn log_out(&mut self) {
        let Some(player) = self.player.take() else {
            return;
        };
        let mut out = Outbox::default();
        let mut games = self.games();
        games.leave_all(player.id, &mut out);
        games.log_out(player.id, &self.tx);
        drop(games);
        out.flush();
    }

    /// Starts matchmaking session for player `player_id` lasting `duration`
    /// milliseconds and returns the `startMatchmaking` response to `request`.
    ///
    /// Only games created with game protocol version `version` (`GVER`) qualify,
    /// or any game when the request carried none. A qualifying game open to
    /// matchmaking is joined right after the response; otherwise a task looks for
    /// one every [`MATCHMAKING_RETRY`] and sends `MatchmakingFailed` once
    /// `duration` has passed.
    fn start_matchmaking(
        &mut self,
        player_id: u32,
        duration: u32,
        version: Option<String>,
        request: &crate::fire::FrameHeader,
    ) -> Frame {
        let mut games = lock_games(&self.config);
        let session_id = games.next_matchmaking_id();
        let reason = game::SetupReason::Matchmaking { session_id };
        let found = games.find_match(player_id, version.as_deref());
        if let Some(ticket) = found.and_then(|game_id| {
            games
                .join(
                    game_id,
                    player_id,
                    game::SLOT_PUBLIC_PARTICIPANT,
                    reason,
                    &mut self.outbox,
                )
                .ok()
        }) {
            log!(
                &self.tag,
                "startMatchmaking: session {session_id} joined game {}",
                ticket.game_id
            );
            watch_join(self.config.clone(), self.tag.clone(), ticket);
            return Frame::response(request, game::StartMatchmakingResponse { session_id });
        }
        drop(games);
        log!(
            &self.tag,
            "startMatchmaking: session {session_id}, no open game yet, searching for {duration} ms"
        );
        let config = self.config.clone();
        let tx = self.tx.clone();
        let tag = self.tag.clone();
        let timer = tokio::spawn(async move {
            let deadline = tokio::time::Instant::now() + Duration::from_millis(u64::from(duration));
            while tokio::time::Instant::now() < deadline {
                tokio::time::sleep(
                    MATCHMAKING_RETRY
                        .min(deadline.saturating_duration_since(tokio::time::Instant::now())),
                )
                .await;
                let mut out = Outbox::default();
                let mut games = lock_games(&config);
                let found = games.find_match(player_id, version.as_deref());
                let joined = found.and_then(|game_id| {
                    games
                        .join(
                            game_id,
                            player_id,
                            game::SLOT_PUBLIC_PARTICIPANT,
                            reason,
                            &mut out,
                        )
                        .ok()
                });
                drop(games);
                out.flush();
                if let Some(ticket) = joined {
                    log!(
                        &tag,
                        "matchmaking session {session_id} joined game {}",
                        ticket.game_id
                    );
                    watch_join(config, tag, ticket);
                    return;
                }
            }
            log!(&tag, "matchmaking session {session_id} timed out");
            let failed = game::MatchmakingFailed {
                session_id,
                player_id,
                result: game::MATCHMAKING_TIMED_OUT,
            };
            let _ = tx.send(Frame::notify(gm::COMPONENT, gm::MATCHMAKING_FAILED, failed));
        });
        self.matchmaking = Some((session_id, timer.abort_handle()));
        Frame::response(request, game::StartMatchmakingResponse { session_id })
    }

    /// Stops the running matchmaking session, if any, without notifying the client.
    fn stop_matchmaking(&mut self) {
        if let Some((_, timer)) = self.matchmaking.take() {
            timer.abort();
        }
    }
}

/// Returns the IPv4 address of `peer`: its IPv4-mapped form for IPv6, or the
/// unspecified address for any other IPv6 address.
fn peer_ipv4(peer: SocketAddr) -> Ipv4Addr {
    match peer.ip() {
        std::net::IpAddr::V4(ip) => ip,
        std::net::IpAddr::V6(ip) => ip.to_ipv4_mapped().unwrap_or(Ipv4Addr::UNSPECIFIED),
    }
}

/// Returns the server's games, locked.
fn lock_games(config: &Config) -> MutexGuard<'_, Games> {
    // A panic while holding the lock leaves plain data behind; keep serving it.
    config
        .games
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Removes the member `ticket` names with reason `PLAYER_JOIN_TIMEOUT` once
/// [`JOIN_TIMEOUT`] has passed, unless its join has completed by then.
fn watch_join(config: Arc<Config>, tag: String, ticket: game::JoinTicket) {
    tokio::spawn(async move {
        tokio::time::sleep(JOIN_TIMEOUT).await;
        let mut out = Outbox::default();
        let expired = lock_games(&config).expire_join(&ticket, &mut out);
        out.flush();
        if expired {
            log!(
                &tag,
                "player {} removed from game {}: no mesh connection within {} s",
                ticket.player_id,
                ticket.game_id,
                JOIN_TIMEOUT.as_secs()
            );
        }
    });
}

/// Time a new member has to report a connected mesh link before it is removed.
///
/// In two-PC runs the link comes up within a second of the join; a member still
/// connecting after this long holds a slot that a working player cannot take.
const JOIN_TIMEOUT: Duration = Duration::from_secs(30);

/// UserSessions error code `USER_ERR_USER_NOT_FOUND`.
const USER_ERR_USER_NOT_FOUND: u16 = 1;

/// Matchmaking duration used when `startMatchmaking` carries no `DUR`, in
/// milliseconds; Dead Space 3 sends this value.
const DEFAULT_MATCHMAKING_MS: u32 = 27_500;
/// Interval between searches for an open game while matchmaking runs.
const MATCHMAKING_RETRY: Duration = Duration::from_secs(1);

/// Returns the log suffix for an operation on a game id the server does not know.
fn unknown_suffix(known: bool) -> &'static str {
    if known { "" } else { " (unknown game)" }
}

/// Returns the value `read_fn` extracts from the body of `frame`, or `None` when
/// the body does not contain it.
fn read<T>(
    frame: &Frame,
    read_fn: impl FnOnce(&mut TdfDeserializer<'_>) -> tdf::prelude::DecodeResult<T>,
) -> Option<T> {
    read_fn(&mut TdfDeserializer::new(&frame.contents)).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(only_listed: bool) -> Config {
        Config {
            advertise: Ipv4Addr::new(26, 0, 0, 1),
            qos: QosHttp {
                host: Ipv4Addr::LOCALHOST,
                port: 0,
            },
            player_name: "host".into(),
            player_names: vec![(Ipv4Addr::new(26, 0, 0, 2), "guest".into())],
            only_listed,
            games: Default::default(),
        }
    }

    #[test]
    fn only_listed_admits_this_machine_and_listed_players() {
        let config = config(true);
        assert!(config.admits(Ipv4Addr::LOCALHOST));
        assert!(config.admits(Ipv4Addr::new(26, 0, 0, 1)));
        assert!(config.admits(Ipv4Addr::new(26, 0, 0, 2)));
        assert!(!config.admits(Ipv4Addr::new(26, 0, 0, 3)));
    }

    #[test]
    fn without_only_listed_every_address_is_admitted() {
        assert!(config(false).admits(Ipv4Addr::new(26, 0, 0, 3)));
    }
}
