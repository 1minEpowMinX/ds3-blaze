//! GameManager games: creation, joining, state, settings and attributes, and the
//! notifications that keep every member's client in step.
//!
//! Field layouts follow Pocket Relay Server's Mass Effect 3 game manager
//! (`src/session/models/game_manager.rs`, `src/services/game`, MIT, Jacobtread)
//! and the Blaze 3 TDF definitions of Aim4kill/BlazeSDK; values the Dead Space 3
//! client sends in `createGame` are echoed back instead of Mass Effect 3 constants.

use crate::components::{game_manager as gm, user_sessions as us};
use crate::fire::Frame;
use crate::models::{self, NetData, Player};
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};
use tdf::prelude::{
    DecodeResult, GroupSlice, ObjectId, TdfDeserializeOwned, TdfDeserializer, TdfMap, TdfSerialize,
    TdfSerializer, TdfType, TdfTyped,
};
use tokio::sync::mpsc::UnboundedSender;

/// Game state the server assigns to a new game (`INITIALIZING`).
const STATE_INITIALIZING: u8 = 0x1;
/// Game state of a game in its lobby (`PRE_GAME`).
const STATE_PRE_GAME: u8 = 0x82;
/// Game state of a game being played (`IN_GAME`).
const STATE_IN_GAME: u8 = 0x83;
/// Game setting bit: matchmaking may place players in the game.
const SETTING_OPEN_TO_MATCHMAKING: u16 = 0x2;
/// Game setting bit: players may join while the game is `IN_GAME`.
const SETTING_JOIN_IN_PROGRESS: u16 = 0x100;
/// Player state of a member still connecting to the mesh (`ACTIVE_CONNECTING`).
const PLAYER_ACTIVE_CONNECTING: u8 = 0x2;
/// Player state of a member whose connection to the mesh is established.
const PLAYER_ACTIVE_CONNECTED: u8 = 0x4;
/// Team index meaning "any team with room".
const UNSPECIFIED_TEAM_INDEX: u16 = 0xFFFF;
/// `DatalessSetupContext` value: the receiver created the game.
const CREATE_GAME_SETUP: u8 = 0x0;
/// `DatalessSetupContext` value: the receiver joined the game.
const JOIN_GAME_SETUP: u8 = 0x1;
/// `GameSetupReason` union key of a `DatalessSetupContext`.
const SETUP_REASON_DATALESS: u8 = 0x0;
/// `GameSetupReason` union key of a `MatchmakingSetupContext`.
const SETUP_REASON_MATCHMAKING: u8 = 0x3;
/// Slot type of a public participant.
pub const SLOT_PUBLIC_PARTICIPANT: u8 = 0x0;
/// `PlayerNetConnectionStatus` of a mesh link that is down.
const LINK_DISCONNECTED: u8 = 0x0;
/// `PlayerNetConnectionStatus` of an established mesh link.
const LINK_CONNECTED: u8 = 0x2;

/// `PlayerRemoved` reason for a member that never finished connecting
/// (`PLAYER_JOIN_TIMEOUT`).
const REMOVE_REASON_JOIN_TIMEOUT: u8 = 0x0;
/// `PlayerRemoved` reason for a member whose mesh link broke (`PLAYER_CONN_LOST`).
const REMOVE_REASON_PLAYER_CONNECTION_LOST: u8 = 0x1;
/// `PlayerRemoved` reason for a member whose Blaze connection closed
/// (`BLAZESERVER_CONN_LOST`).
const REMOVE_REASON_CONNECTION_LOST: u8 = 0x2;
/// `GameRemoved` reason sent to the remaining members when the host leaves
/// (`HOST_LEAVING`).
const DESTROY_REASON_HOST_LEAVING: u8 = 0x3;

/// GameManager error `GAMEMANAGER_ERR_INVALID_GAME_ID`.
pub const ERR_INVALID_GAME_ID: u16 = 2;
/// GameManager error `GAMEMANAGER_ERR_GAME_FULL`.
pub const ERR_GAME_FULL: u16 = 4;
/// GameManager error `GAMEMANAGER_ERR_PLAYER_NOT_FOUND`.
pub const ERR_PLAYER_NOT_FOUND: u16 = 101;
/// GameManager error `GAMEMANAGER_ERR_ALREADY_GAME_MEMBER`.
pub const ERR_ALREADY_GAME_MEMBER: u16 = 103;

/// Parameters of a `createGame` request the server keeps.
pub struct CreateGameRequest {
    pub attributes: Vec<(String, String)>,
    pub name: String,
    pub settings: u16,
    pub game_type: String,
    pub topology: u8,
    /// Slot capacities (`PCAP`): public, then private.
    pub capacity: Vec<u16>,
    pub max_players: u16,
    pub presence: u8,
    pub voip: u8,
    /// Game protocol version string; members must report the same one.
    pub version: String,
}

impl CreateGameRequest {
    /// Returns the request read from `contents`, with defaults for missing fields.
    pub fn read(contents: &[u8]) -> Self {
        fn tag<T: for<'de> tdf::prelude::TdfDeserialize<'de> + tdf::prelude::TdfTyped>(
            contents: &[u8],
            tag: &[u8],
        ) -> Option<T> {
            TdfDeserializer::new(contents).tag(tag).ok()
        }
        Self {
            attributes: read_attributes(contents),
            name: tag(contents, b"GNAM").unwrap_or_default(),
            settings: tag(contents, b"GSET").unwrap_or_default(),
            game_type: tag(contents, b"GTYP").unwrap_or_default(),
            topology: tag(contents, b"NTOP").unwrap_or_default(),
            capacity: tag(contents, b"PCAP").unwrap_or_default(),
            max_players: tag(contents, b"PMAX").unwrap_or_default(),
            presence: tag(contents, b"PRES").unwrap_or_default(),
            voip: tag(contents, b"VOIP").unwrap_or_default(),
            version: tag(contents, b"VSTR").unwrap_or_default(),
        }
    }
}

/// Returns the `ATTR` string map of a request body in attribute order, empty when absent.
pub fn read_attributes(contents: &[u8]) -> Vec<(String, String)> {
    // `TdfMap` orders keys case-sensitively, so its order is discarded here.
    let mut attributes: Vec<(String, String)> = TdfDeserializer::new(contents)
        .tag::<TdfMap<String, String>>(b"ATTR")
        .map(|map| map.iter().cloned().collect())
        .unwrap_or_default();
    sort_attributes(&mut attributes);
    attributes
}

/// Sorts the key and value pairs of `attributes` in place by key, with ASCII case
/// folded to lower case.
fn sort_attributes(attributes: &mut [(String, String)]) {
    // BlazeSDK attribute maps compare keys case-insensitively and look them up by
    // binary search over the order received: in any other order the client cannot
    // find keys such as `chapterNumber`.
    attributes.sort_by(|(a, _), (b, _)| {
        a.bytes()
            .map(|c| c.to_ascii_lowercase())
            .cmp(b.bytes().map(|c| c.to_ascii_lowercase()))
    });
}

/// One entry of an `updateMeshConnection` request: the state of the reporter's
/// link to another member.
pub struct MeshLink {
    /// Member at the other end of the link (`PID`).
    pub player_id: u32,
    /// `PlayerNetConnectionStatus` (`STAT`).
    pub status: u8,
}

impl TdfTyped for MeshLink {
    const TYPE: TdfType = TdfType::Group;
}

impl TdfDeserializeOwned for MeshLink {
    fn deserialize_owned(r: &mut TdfDeserializer<'_>) -> DecodeResult<Self> {
        let player_id: u32 = r.tag(b"PID")?;
        let status: u8 = r.tag(b"STAT")?;
        GroupSlice::deserialize_content_skip(r)?;
        Ok(Self { player_id, status })
    }
}

/// Returns the links listed under `TARG` in an `updateMeshConnection` body, empty
/// when the list is missing or unreadable.
pub fn read_mesh_links(contents: &[u8]) -> Vec<MeshLink> {
    TdfDeserializer::new(contents)
        .tag(b"TARG")
        .unwrap_or_default()
}

/// Notifications held back until the response to the request that caused them
/// has been sent, in the order the EA server sends them.
#[derive(Default)]
pub struct Outbox(Vec<(UnboundedSender<Frame>, Frame)>);

impl Outbox {
    /// Queues `frame` for the connection behind `tx`.
    pub fn push(&mut self, tx: &UnboundedSender<Frame>, frame: Frame) {
        self.0.push((tx.clone(), frame));
    }

    /// Sends every queued frame in order.
    pub fn flush(self) {
        for (tx, frame) in self.0 {
            // A closed connection leaves its games through `Games::leave_all`.
            let _ = tx.send(frame);
        }
    }
}

/// A logged-in user: account, current network state and connection.
struct User {
    player: Player,
    net: NetData,
    tx: UnboundedSender<Frame>,
}

/// How a member came to receive `GameSetup` (`REAS`).
#[derive(Clone, Copy)]
pub enum SetupReason {
    /// The member created the game.
    Create,
    /// The member joined with `joinGame`.
    Join,
    /// Matchmaking session `session_id` placed the member in the game.
    Matchmaking { session_id: u32 },
}

/// A member of a game and the channel its connection's frames go out on.
struct GamePlayer {
    player: Player,
    net: NetData,
    state: u8,
    /// Slot type (`SLOT`): public or private participant.
    slot_type: u8,
    tx: UnboundedSender<Frame>,
    /// Player attributes (`PATT`), sorted like game attributes.
    attributes: Vec<(String, String)>,
    /// Join time in microseconds since the Unix epoch (`TIME`).
    joined: u64,
}

impl GamePlayer {
    /// Returns `user` as a member in player state `state` and slot type `slot_type`.
    fn new(user: &User, state: u8, slot_type: u8) -> Self {
        let joined = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_micros() as u64)
            .unwrap_or(0);
        Self {
            player: user.player.clone(),
            net: user.net.clone(),
            state,
            slot_type,
            tx: user.tx.clone(),
            attributes: Vec::new(),
            joined,
        }
    }

    /// Writes the member as one `PROS`/`PDAT` group body.
    fn write<S: TdfSerializer>(&self, w: &mut S, game_id: u32, slot: usize) {
        w.group_body(|w| {
            w.tag_blob_empty(b"BLOB");
            w.tag_zero(b"EXID");
            w.tag_u32(b"GID", game_id);
            w.tag_u32(b"LOC", self.player.locale);
            w.tag_str(b"NAME", &self.player.name);
            if !self.attributes.is_empty() {
                w.tag_map_tuples(b"PATT", &self.attributes);
            }
            w.tag_u32(b"PID", self.player.id);
            models::write_network_address(w, b"PNET", self.net.address.as_ref());
            w.tag_owned(b"SID", slot);
            w.tag_u8(b"SLOT", self.slot_type);
            w.tag_u8(b"STAT", self.state);
            w.tag_u16(b"TIDX", UNSPECIFIED_TEAM_INDEX);
            w.tag_u64(b"TIME", self.joined);
            w.tag_alt(b"UGID", ObjectId::new_raw(0, 0, 0));
            w.tag_u32(b"UID", self.player.id);
        });
    }
}

/// One game: its settings and its members, the host first.
pub struct Game {
    pub id: u32,
    pub state: u8,
    pub settings: u16,
    attributes: Vec<(String, String)>,
    request: CreateGameRequest,
    seed: u32,
    players: Vec<GamePlayer>,
}

impl Game {
    /// Queues `value` as notification `command` for every member.
    fn notify_all<V: TdfSerialize + Clone>(&self, out: &mut Outbox, command: u16, value: V) {
        for member in &self.players {
            out.push(
                &member.tx,
                Frame::notify(gm::COMPONENT, command, value.clone()),
            );
        }
    }

    fn member(&self, player_id: u32) -> Option<&GamePlayer> {
        self.players.iter().find(|m| m.player.id == player_id)
    }

    /// Returns the number of slots of every type (`PCAP`), at least one.
    fn capacity(&self) -> usize {
        let total: usize = self.request.capacity.iter().map(|&c| usize::from(c)).sum();
        total.max(1)
    }

    /// Returns whether matchmaking may place another player in the game.
    fn open_to_matchmaking(&self) -> bool {
        let joinable_state = self.state == STATE_PRE_GAME
            || (self.state == STATE_IN_GAME && self.settings & SETTING_JOIN_IN_PROGRESS != 0);
        joinable_state
            && self.settings & SETTING_OPEN_TO_MATCHMAKING != 0
            && self.players.len() < self.capacity()
    }

    /// Sets member `player_id` to `ACTIVE_CONNECTED` and queues the change and
    /// `PlayerJoinCompleted` for every member; returns `false` when the member was
    /// not connecting.
    fn complete_join(&mut self, player_id: u32, out: &mut Outbox) -> bool {
        let Some(member) = self.players.iter_mut().find(|m| m.player.id == player_id) else {
            return false;
        };
        if member.state != PLAYER_ACTIVE_CONNECTING {
            return false;
        }
        member.state = PLAYER_ACTIVE_CONNECTED;
        let player = PlayerEvent {
            game_id: self.id,
            player_id,
        };
        self.notify_all(
            out,
            gm::GAME_PLAYER_STATE_CHANGE,
            PlayerStateChange {
                player,
                state: PLAYER_ACTIVE_CONNECTED,
            },
        );
        self.notify_all(out, gm::PLAYER_JOIN_COMPLETED, player);
        true
    }
}

/// All users, games and matchmaking sessions on this server.
#[derive(Default)]
pub struct Games {
    next_id: u32,
    next_matchmaking_id: u32,
    users: HashMap<u32, User>,
    games: HashMap<u32, Game>,
    /// EA account ids that `lookupUsers` named, bound to player ids.
    accounts: HashMap<u64, u32>,
}

/// A logged-in player that `lookupUsers` names.
pub struct FoundUser {
    pub player_id: u32,
    /// Whether this lookup bound the requested account id to the player.
    pub bound: bool,
}

impl Games {
    /// Returns a player named `name`, or `name` followed by `-2`, `-3` and so on
    /// when a logged-in user already holds the id derived from it.
    pub fn unique_player(&self, name: &str, locale: u32) -> Player {
        let mut player = Player::from_name(name, locale);
        let mut suffix = 1;
        while self.users.contains_key(&player.id) {
            suffix += 1;
            player = Player::from_name(&format!("{name}-{suffix}"), locale);
        }
        player
    }

    /// Registers `player` as logged in on the connection behind `tx`.
    pub fn log_in(&mut self, player: Player, net: NetData, tx: UnboundedSender<Frame>) {
        self.users.insert(player.id, User { player, net, tx });
    }

    /// Unregisters player `player_id` if it is still logged in on the connection
    /// behind `tx`.
    pub fn log_out(&mut self, player_id: u32, tx: &UnboundedSender<Frame>) {
        if self
            .users
            .get(&player_id)
            .is_some_and(|user| user.tx.same_channel(tx))
        {
            self.users.remove(&player_id);
        }
    }

    /// Stores `net` as the network state of player `player_id`, in its user
    /// record and in every game it is a member of.
    pub fn update_net(&mut self, player_id: u32, net: &NetData) {
        if let Some(user) = self.users.get_mut(&player_id) {
            user.net = net.clone();
        }
        for game in self.games.values_mut() {
            if let Some(member) = game.players.iter_mut().find(|m| m.player.id == player_id) {
                member.net = net.clone();
            }
        }
    }

    /// Returns `UserSessionExtendedDataUpdate` for player `player_id` with its
    /// network state and game memberships, or `None` when it is not logged in.
    pub fn session_update(&self, player_id: u32) -> Option<Frame> {
        let user = self.users.get(&player_id)?;
        let games = self.games_of(player_id);
        Some(Frame::notify(
            us::COMPONENT,
            us::USER_SESSION_EXTENDED_DATA_UPDATE,
            models::ExtendedDataUpdate {
                player_id,
                net: &user.net,
                games: &games,
            },
        ))
    }

    /// Queues [`Games::session_update`] for player `player_id` on its own connection.
    fn queue_session_update(&self, player_id: u32, out: &mut Outbox) {
        if let (Some(user), Some(frame)) =
            (self.users.get(&player_id), self.session_update(player_id))
        {
            out.push(&user.tx, frame);
        }
    }

    /// Creates a game hosted by player `host_id`, queues its `GameSetup` and the
    /// host's session update, and returns the game id; returns `None` when the
    /// host is not logged in.
    pub fn create(
        &mut self,
        request: CreateGameRequest,
        host_id: u32,
        out: &mut Outbox,
    ) -> Option<u32> {
        let host = GamePlayer::new(
            self.users.get(&host_id)?,
            PLAYER_ACTIVE_CONNECTED,
            SLOT_PUBLIC_PARTICIPANT,
        );
        self.next_id += 1;
        let id = self.next_id;
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.subsec_nanos())
            .unwrap_or(0);
        let game = Game {
            id,
            state: STATE_INITIALIZING,
            settings: request.settings,
            attributes: request.attributes.clone(),
            request,
            seed,
            players: vec![host],
        };
        let setup = GameSetup {
            game: &game,
            reason: SetupReason::Create,
            receiver: host_id,
        };
        out.push(
            &game.players[0].tx,
            Frame::notify(gm::COMPONENT, gm::GAME_SETUP, setup),
        );
        self.games.insert(id, game);
        self.queue_session_update(host_id, out);
        Some(id)
    }

    /// Returns the logged-in player that `identity` names under lookup type
    /// `lookup_type` (a `LOOKUP_BY_*` value), or `None`.
    ///
    /// An account id that is neither a player id nor bound yet is bound to the
    /// only logged-in player other than `requester` without a bound account id;
    /// the binding lasts until the server stops.
    pub fn find_user(
        &mut self,
        requester: u32,
        lookup_type: u8,
        identity: &models::UserIdentification,
    ) -> Option<FoundUser> {
        let known = |player_id: u32| {
            Some(FoundUser {
                player_id,
                bound: false,
            })
        };
        match lookup_type {
            models::LOOKUP_BY_BLAZE_ID => u32::try_from(identity.blaze_id)
                .ok()
                .filter(|id| self.users.contains_key(id))
                .and_then(known),
            models::LOOKUP_BY_PERSONA_NAME => self
                .users
                .values()
                .find(|user| user.player.name.eq_ignore_ascii_case(&identity.name))
                .and_then(|user| known(user.player.id)),
            models::LOOKUP_BY_ACCOUNT_ID => {
                let account_id = identity.account_id;
                if let Some(&player_id) = self.accounts.get(&account_id) {
                    return self.users.contains_key(&player_id).then_some(FoundUser {
                        player_id,
                        bound: false,
                    });
                }
                if let Some(id) = u32::try_from(account_id)
                    .ok()
                    .filter(|id| self.users.contains_key(id))
                {
                    return known(id);
                }
                // The client takes its friends' EA account ids from the EA App, and
                // login never shows the server a player's own one, so an unknown id
                // is matched by elimination.
                let mut candidates = self.users.keys().copied().filter(|&id| {
                    id != requester && !self.accounts.values().any(|&bound| bound == id)
                });
                let player_id = candidates.next()?;
                if candidates.next().is_some() {
                    return None;
                }
                self.accounts.insert(account_id, player_id);
                Some(FoundUser {
                    player_id,
                    bound: true,
                })
            }
            _ => None,
        }
    }

    /// Returns the `lookupUsers` entry of logged-in player `player_id`, or `None`.
    ///
    /// `USER.AID` carries the account id bound to the player, or the player id
    /// when none is.
    pub fn user_data(&self, player_id: u32) -> Option<models::UserData<'_>> {
        let user = self.users.get(&player_id)?;
        let account_id = self
            .accounts
            .iter()
            .find(|&(_, &id)| id == player_id)
            .map_or(u64::from(player_id), |(&account_id, _)| account_id);
        Some(models::UserData {
            player: &user.player,
            account_id,
            net: &user.net,
            games: self.games_of(player_id),
        })
    }

    /// Returns the name of logged-in player `player_id`, or `None`.
    pub fn player_name(&self, player_id: u32) -> Option<&str> {
        self.users
            .get(&player_id)
            .map(|user| user.player.name.as_str())
    }

    /// Returns whether game `id` exists.
    pub fn contains(&self, id: u32) -> bool {
        self.games.contains_key(&id)
    }

    /// Returns the ids of the games player `player_id` is a member of, ascending.
    pub fn games_of(&self, player_id: u32) -> Vec<u32> {
        let mut ids: Vec<u32> = self
            .games
            .values()
            .filter(|game| game.member(player_id).is_some())
            .map(|game| game.id)
            .collect();
        ids.sort_unstable();
        ids
    }

    /// Returns the oldest game matchmaking may place player `player_id` in, or
    /// `None` when there is none.
    pub fn find_match(&self, player_id: u32) -> Option<u32> {
        self.games
            .values()
            .filter(|game| game.open_to_matchmaking() && game.member(player_id).is_none())
            .map(|game| game.id)
            .min()
    }

    /// Adds player `player_id` to game `id` in slot type `slot_type` and queues
    /// the notifications that introduce it, or returns a GameManager error code.
    ///
    /// Existing members receive the newcomer's `UserAdded` and `UserUpdated` and
    /// then `PlayerJoining`; the newcomer receives the same pair for each existing
    /// member, then `GameSetup` with `reason`, then its session update.
    pub fn join(
        &mut self,
        id: u32,
        player_id: u32,
        slot_type: u8,
        reason: SetupReason,
        out: &mut Outbox,
    ) -> Result<(), u16> {
        let Some(user) = self.users.get(&player_id) else {
            return Err(ERR_PLAYER_NOT_FOUND);
        };
        let Some(game) = self.games.get_mut(&id) else {
            return Err(ERR_INVALID_GAME_ID);
        };
        if game.member(player_id).is_some() {
            return Err(ERR_ALREADY_GAME_MEMBER);
        }
        if game.players.len() >= game.capacity() {
            return Err(ERR_GAME_FULL);
        }
        let newcomer = GamePlayer::new(user, PLAYER_ACTIVE_CONNECTING, slot_type);
        let subscribed = models::USER_SUBSCRIBED | models::USER_ONLINE;
        for member in &game.players {
            let added = models::UserAdded {
                player: &newcomer.player,
                net: &newcomer.net,
            };
            out.push(
                &member.tx,
                Frame::notify(us::COMPONENT, us::USER_ADDED, added),
            );
            let updated = models::UserUpdated {
                flags: subscribed,
                player_id,
            };
            out.push(
                &member.tx,
                Frame::notify(us::COMPONENT, us::USER_UPDATED, updated),
            );
            let added = models::UserAdded {
                player: &member.player,
                net: &member.net,
            };
            out.push(
                &newcomer.tx,
                Frame::notify(us::COMPONENT, us::USER_ADDED, added),
            );
            let updated = models::UserUpdated {
                flags: subscribed,
                player_id: member.player.id,
            };
            out.push(
                &newcomer.tx,
                Frame::notify(us::COMPONENT, us::USER_UPDATED, updated),
            );
        }
        let slot = game.players.len();
        let joining = PlayerJoining {
            game_id: id,
            player: &newcomer,
            slot,
        };
        game.notify_all(out, gm::PLAYER_JOINING, joining);
        let tx = newcomer.tx.clone();
        game.players.push(newcomer);
        let setup = GameSetup {
            game,
            reason,
            receiver: player_id,
        };
        out.push(&tx, Frame::notify(gm::COMPONENT, gm::GAME_SETUP, setup));
        self.queue_session_update(player_id, out);
        Ok(())
    }

    /// Returns a new matchmaking session id.
    pub fn next_matchmaking_id(&mut self) -> u32 {
        self.next_matchmaking_id += 1;
        self.next_matchmaking_id
    }

    /// Sets the state of game `id` and queues the change for its members; returns
    /// `false` for an unknown game.
    pub fn advance_state(&mut self, id: u32, state: u8, out: &mut Outbox) -> bool {
        let Some(game) = self.games.get_mut(&id) else {
            return false;
        };
        game.state = state;
        game.notify_all(
            out,
            gm::GAME_STATE_CHANGE,
            StateChange { game_id: id, state },
        );
        true
    }

    /// Sets the settings of game `id` and queues the change for its members;
    /// returns `false` for an unknown game.
    pub fn set_settings(&mut self, id: u32, settings: u16, out: &mut Outbox) -> bool {
        let Some(game) = self.games.get_mut(&id) else {
            return false;
        };
        game.settings = settings;
        game.notify_all(
            out,
            gm::GAME_SETTINGS_CHANGE,
            SettingsChange {
                game_id: id,
                settings,
            },
        );
        true
    }

    /// Merges `changes` into the attributes of game `id` and queues the changed
    /// entries for its members; returns `false` for an unknown game.
    pub fn set_attributes(
        &mut self,
        id: u32,
        changes: Vec<(String, String)>,
        out: &mut Outbox,
    ) -> bool {
        let Some(game) = self.games.get_mut(&id) else {
            return false;
        };
        merge_attributes(&mut game.attributes, &changes);
        game.notify_all(
            out,
            gm::GAME_ATTRIB_CHANGE,
            AttributesChange {
                game_id: id,
                attributes: changes,
            },
        );
        true
    }

    /// Merges `changes` into the attributes of player `player_id` in game `id` and
    /// queues the changed entries for the game's members; returns `false` when the
    /// player is not in that game.
    pub fn set_player_attributes(
        &mut self,
        id: u32,
        player_id: u32,
        changes: Vec<(String, String)>,
        out: &mut Outbox,
    ) -> bool {
        let Some(game) = self.games.get_mut(&id) else {
            return false;
        };
        let Some(member) = game.players.iter_mut().find(|m| m.player.id == player_id) else {
            return false;
        };
        merge_attributes(&mut member.attributes, &changes);
        let change = PlayerAttributesChange {
            game_id: id,
            player_id,
            attributes: changes,
        };
        game.notify_all(out, gm::PLAYER_ATTRIB_CHANGE, change);
        true
    }

    /// Applies the mesh links player `reporter` reports for game `id` and returns
    /// a log line per change made.
    ///
    /// A connected link completes the join of whichever end is still connecting.
    /// A link reported down removes the guest at one end: the target when the host
    /// reports it, the reporter when it is a guest's link to the host.
    pub fn update_mesh(
        &mut self,
        id: u32,
        reporter: u32,
        links: &[MeshLink],
        out: &mut Outbox,
    ) -> Vec<String> {
        let Some(game) = self.games.get_mut(&id) else {
            return vec![format!("game {id} unknown")];
        };
        let host_id = game.players[0].player.id;
        let mut events = Vec::new();
        let mut dropped = Vec::new();
        for link in links {
            match link.status {
                LINK_CONNECTED => {
                    for end in [reporter, link.player_id] {
                        if game.complete_join(end, out) {
                            events.push(format!("player {end} finished joining game {id}"));
                        }
                    }
                }
                LINK_DISCONNECTED => {
                    let guest = if reporter == host_id {
                        link.player_id
                    } else if link.player_id == host_id {
                        reporter
                    } else {
                        continue;
                    };
                    if let Some(member) = game.member(guest) {
                        let reason = if member.state == PLAYER_ACTIVE_CONNECTING {
                            REMOVE_REASON_JOIN_TIMEOUT
                        } else {
                            REMOVE_REASON_PLAYER_CONNECTION_LOST
                        };
                        dropped.push((guest, reason));
                    }
                }
                _ => {}
            }
        }
        for (guest, reason) in dropped {
            if self.remove_player(id, guest, reason, out) {
                events.push(format!(
                    "player {guest} removed from game {id}: mesh link down, reason {reason}"
                ));
            }
        }
        events
    }

    /// Removes player `player_id` from game `id` for `reason` and returns `false`
    /// when the player is not in that game.
    ///
    /// Every member including the removed one receives `PlayerRemoved`, and the
    /// removed one its session update. When the host leaves, the remaining members
    /// receive `GameRemoved` and their session updates and the game is deleted, as
    /// it is once empty.
    pub fn remove_player(&mut self, id: u32, player_id: u32, reason: u8, out: &mut Outbox) -> bool {
        let Some(game) = self.games.get_mut(&id) else {
            return false;
        };
        let Some(index) = game.players.iter().position(|m| m.player.id == player_id) else {
            return false;
        };
        game.notify_all(
            out,
            gm::PLAYER_REMOVED,
            PlayerRemoved {
                game_id: id,
                player_id,
                reason,
            },
        );
        game.players.remove(index);
        let destroy = index == 0 || game.players.is_empty();
        if destroy {
            self.destroy(id, DESTROY_REASON_HOST_LEAVING, out);
        }
        self.queue_session_update(player_id, out);
        true
    }

    /// Deletes game `id`, queues `GameRemoved` with `reason` and the session
    /// update for each member left in it, and returns `false` for an unknown game.
    pub fn destroy(&mut self, id: u32, reason: u8, out: &mut Outbox) -> bool {
        let Some(game) = self.games.remove(&id) else {
            return false;
        };
        let removed = GameRemoved {
            game_id: id,
            reason,
        };
        for member in &game.players {
            out.push(
                &member.tx,
                Frame::notify(gm::COMPONENT, gm::GAME_REMOVED, removed),
            );
            self.queue_session_update(member.player.id, out);
        }
        true
    }

    /// Removes player `player_id` from every game, as when its connection closes.
    pub fn leave_all(&mut self, player_id: u32, out: &mut Outbox) {
        for id in self.games_of(player_id) {
            self.remove_player(id, player_id, REMOVE_REASON_CONNECTION_LOST, out);
        }
    }
}

/// Merges `changes` into `attributes`, matching keys without regard to ASCII case,
/// and keeps `attributes` in attribute order.
fn merge_attributes(attributes: &mut Vec<(String, String)>, changes: &[(String, String)]) {
    for (key, value) in changes {
        match attributes
            .iter_mut()
            .find(|(k, _)| k.eq_ignore_ascii_case(key))
        {
            Some(entry) => entry.1 = value.clone(),
            None => attributes.push((key.clone(), value.clone())),
        }
    }
    sort_attributes(attributes);
}

/// `GameSetup` notification describing `game` to member `receiver`.
struct GameSetup<'a> {
    game: &'a Game,
    reason: SetupReason,
    receiver: u32,
}

impl TdfSerialize for GameSetup<'_> {
    fn serialize<S: TdfSerializer>(&self, w: &mut S) {
        let game = self.game;
        let request = &game.request;
        let host = &game.players[0];
        w.group(b"GAME", |w| {
            w.tag_list_iter_owned(b"ADMN", std::iter::once(host.player.id));
            w.tag_map_tuples(b"ATTR", &game.attributes);
            w.tag_list_slice(b"CAP", &request.capacity);
            w.tag_u32(b"GID", game.id);
            w.tag_str(b"GNAM", &request.name);
            w.tag_u64(b"GPVH", version_hash(&request.version));
            w.tag_u16(b"GSET", game.settings);
            w.tag_u64(b"GSID", u64::from(game.id));
            w.tag_u8(b"GSTA", game.state);
            w.tag_str(b"GTYP", &request.game_type);
            w.tag_str_empty(b"GURL");
            w.tag_list_start(b"HNET", TdfType::Group, 1);
            models::write_network_address_element(w, host.net.address.as_ref());
            w.tag_u32(b"HSES", host.player.id);
            w.tag_zero(b"IGNO");
            w.tag_u16(b"MCAP", request.max_players);
            host.net.qos.write(w, b"NQOS");
            w.tag_zero(b"NRES");
            w.tag_u8(b"NTOP", request.topology);
            w.tag_str_empty(b"PGID");
            w.tag_blob_empty(b"PGSR");
            w.group(b"PHST", |w| {
                w.tag_u32(b"HPID", host.player.id);
                w.tag_zero(b"HSLT");
            });
            w.tag_u8(b"PRES", request.presence);
            // EA's server leaves the ping site alias of a Dead Space 3 game empty.
            w.tag_str_empty(b"PSAS");
            w.tag_zero(b"QCAP");
            w.tag_u32(b"SEED", game.seed);
            w.tag_zero(b"TCAP");
            w.group(b"THST", |w| {
                w.tag_u32(b"HPID", host.player.id);
                w.tag_zero(b"HSLT");
            });
            w.tag_str(
                b"UUID",
                &format!("00000000-0000-0000-0000-{:012x}", game.id),
            );
            w.tag_u8(b"VOIP", request.voip);
            w.tag_str(b"VSTR", &request.version);
            w.tag_blob_empty(b"XNNC");
            w.tag_blob_empty(b"XSES");
        });
        w.tag_list_start(b"PROS", TdfType::Group, game.players.len());
        for (slot, member) in game.players.iter().enumerate() {
            member.write(w, game.id, slot);
        }
        match self.reason {
            SetupReason::Create | SetupReason::Join => {
                let context = if matches!(self.reason, SetupReason::Create) {
                    CREATE_GAME_SETUP
                } else {
                    JOIN_GAME_SETUP
                };
                w.tag_union_start(b"REAS", SETUP_REASON_DATALESS);
                w.group(b"VALU", |w| w.tag_u8(b"DCTX", context));
            }
            SetupReason::Matchmaking { session_id } => {
                w.tag_union_start(b"REAS", SETUP_REASON_MATCHMAKING);
                w.group(b"VALU", |w| {
                    w.tag_u32(b"FIT", MATCHMAKING_MAX_FIT);
                    w.tag_u32(b"MAXF", MATCHMAKING_MAX_FIT);
                    w.tag_u32(b"MSID", session_id);
                    w.tag_u8(b"RSLT", MATCHMAKING_JOINED_EXISTING_GAME);
                    w.tag_u32(b"USID", self.receiver);
                });
            }
        }
    }
}

/// `createGame` response.
pub struct CreateGameResponse {
    pub game_id: u32,
}

impl TdfSerialize for CreateGameResponse {
    fn serialize<S: TdfSerializer>(&self, w: &mut S) {
        w.tag_u32(b"GID", self.game_id);
    }
}

/// `joinGame` response for a player placed in game `game_id` (`JGS` `JOINED_GAME`).
pub struct JoinGameResponse {
    pub game_id: u32,
}

impl TdfSerialize for JoinGameResponse {
    fn serialize<S: TdfSerializer>(&self, w: &mut S) {
        w.tag_u32(b"GID", self.game_id);
        w.tag_zero(b"JGS");
    }
}

/// `PlayerJoining` notification announcing `player` in slot `slot`.
#[derive(Clone)]
struct PlayerJoining<'a> {
    game_id: u32,
    player: &'a GamePlayer,
    slot: usize,
}

impl TdfSerialize for PlayerJoining<'_> {
    fn serialize<S: TdfSerializer>(&self, w: &mut S) {
        w.tag_u32(b"GID", self.game_id);
        w.tag_group(b"PDAT");
        self.player.write(w, self.game_id, self.slot);
    }
}

/// Game and player ids: the `PlayerJoinCompleted` notification and the head of
/// `GamePlayerStateChange`.
#[derive(Clone, Copy)]
struct PlayerEvent {
    game_id: u32,
    player_id: u32,
}

impl TdfSerialize for PlayerEvent {
    fn serialize<S: TdfSerializer>(&self, w: &mut S) {
        w.tag_u32(b"GID", self.game_id);
        w.tag_u32(b"PID", self.player_id);
    }
}

/// `GamePlayerStateChange` notification.
#[derive(Clone, Copy)]
struct PlayerStateChange {
    player: PlayerEvent,
    state: u8,
}

impl TdfSerialize for PlayerStateChange {
    fn serialize<S: TdfSerializer>(&self, w: &mut S) {
        self.player.serialize(w);
        w.tag_u8(b"STAT", self.state);
    }
}

/// `GameRemoved` notification.
#[derive(Clone, Copy)]
struct GameRemoved {
    game_id: u32,
    reason: u8,
}

impl TdfSerialize for GameRemoved {
    fn serialize<S: TdfSerializer>(&self, w: &mut S) {
        w.tag_u32(b"GID", self.game_id);
        w.tag_u8(b"REAS", self.reason);
    }
}

/// `GameStateChange` notification.
#[derive(Clone)]
struct StateChange {
    game_id: u32,
    state: u8,
}

impl TdfSerialize for StateChange {
    fn serialize<S: TdfSerializer>(&self, w: &mut S) {
        w.tag_u32(b"GID", self.game_id);
        w.tag_u8(b"GSTA", self.state);
    }
}

/// `GameSettingsChange` notification; the settings travel under `ATTR`.
#[derive(Clone)]
struct SettingsChange {
    game_id: u32,
    settings: u16,
}

impl TdfSerialize for SettingsChange {
    fn serialize<S: TdfSerializer>(&self, w: &mut S) {
        w.tag_u16(b"ATTR", self.settings);
        w.tag_u32(b"GID", self.game_id);
    }
}

/// `GameAttribChange` notification with the changed attributes.
#[derive(Clone)]
struct AttributesChange {
    game_id: u32,
    attributes: Vec<(String, String)>,
}

impl TdfSerialize for AttributesChange {
    fn serialize<S: TdfSerializer>(&self, w: &mut S) {
        w.tag_map_tuples(b"ATTR", &self.attributes);
        w.tag_u32(b"GID", self.game_id);
    }
}

/// `PlayerAttribChange` notification with the changed attributes of one member.
#[derive(Clone)]
struct PlayerAttributesChange {
    game_id: u32,
    player_id: u32,
    attributes: Vec<(String, String)>,
}

impl TdfSerialize for PlayerAttributesChange {
    fn serialize<S: TdfSerializer>(&self, w: &mut S) {
        w.tag_map_tuples(b"ATTR", &self.attributes);
        w.tag_u32(b"GID", self.game_id);
        w.tag_u32(b"PID", self.player_id);
    }
}

/// `PlayerRemoved` notification.
#[derive(Clone)]
struct PlayerRemoved {
    game_id: u32,
    player_id: u32,
    reason: u8,
}

impl TdfSerialize for PlayerRemoved {
    fn serialize<S: TdfSerializer>(&self, w: &mut S) {
        w.tag_zero(b"CNTX");
        w.tag_u32(b"GID", self.game_id);
        w.tag_u32(b"PID", self.player_id);
        w.tag_u8(b"REAS", self.reason);
    }
}

/// `GPVH` values EA's server sends for game protocol versions, keyed by `VSTR`.
const KNOWN_VERSION_HASHES: [(&str, u64); 1] = [("Feb 15 2013 11:50:11", 0x78b4_a18f_1da6_fe53)];

/// Returns the game protocol version hash (`GPVH`) of `version`.
///
/// Versions in [`KNOWN_VERSION_HASHES`] get EA's value. Other versions get FNV-1
/// over the bytes with 64-bit arithmetic and the 32-bit offset and prime, as
/// Pocket Relay computes it for Mass Effect 3 (credited there to Aim4kill).
fn version_hash(version: &str) -> u64 {
    if let Some((_, hash)) = KNOWN_VERSION_HASHES.iter().find(|(v, _)| *v == version) {
        return *hash;
    }
    const OFFSET: u64 = 2_166_136_261;
    const PRIME: u64 = 16_777_619;
    version.bytes().fold(OFFSET, |hash, byte| {
        hash.wrapping_mul(PRIME) ^ u64::from(byte)
    })
}

/// Matchmaking result `SUCCESS_JOINED_EXISTING_GAME`.
const MATCHMAKING_JOINED_EXISTING_GAME: u8 = 0x2;
/// Matchmaking result `TIMED_OUT`.
pub const MATCHMAKING_TIMED_OUT: u8 = 0x3;
/// Highest fit score EA's server reports for Dead Space 3 matchmaking (`MAXF`).
const MATCHMAKING_MAX_FIT: u32 = 13000;

/// `startMatchmaking` response.
pub struct StartMatchmakingResponse {
    pub session_id: u32,
}

impl TdfSerialize for StartMatchmakingResponse {
    fn serialize<S: TdfSerializer>(&self, w: &mut S) {
        w.tag_u32(b"MSID", self.session_id);
    }
}

/// `MatchmakingFailed` notification ending matchmaking session `session_id`.
pub struct MatchmakingFailed {
    pub session_id: u32,
    pub player_id: u32,
    pub result: u8,
}

impl TdfSerialize for MatchmakingFailed {
    fn serialize<S: TdfSerializer>(&self, w: &mut S) {
        w.tag_u32(b"MAXF", MATCHMAKING_MAX_FIT);
        w.tag_u32(b"MSID", self.session_id);
        w.tag_u8(b"RSLT", self.result);
        w.tag_u32(b"USID", self.player_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{LOOKUP_BY_ACCOUNT_ID, LOOKUP_BY_PERSONA_NAME, UserIdentification};
    use tokio::sync::mpsc::unbounded_channel;

    /// Logs in a player named `name` and returns its id.
    fn log_in(games: &mut Games, name: &str) -> u32 {
        let player = Player::from_name(name, 0);
        let id = player.id;
        games.log_in(player, NetData::default(), unbounded_channel().0);
        id
    }

    fn by_account(account_id: u64) -> UserIdentification {
        UserIdentification {
            account_id,
            blaze_id: 0,
            name: String::new(),
        }
    }

    #[test]
    fn unknown_account_binds_to_the_only_other_player() {
        let mut games = Games::default();
        let host = log_in(&mut games, "host");
        let guest = log_in(&mut games, "guest");
        let found = games
            .find_user(guest, LOOKUP_BY_ACCOUNT_ID, &by_account(1_000_000_000_001))
            .unwrap();
        assert_eq!((found.player_id, found.bound), (host, true));
        let again = games
            .find_user(guest, LOOKUP_BY_ACCOUNT_ID, &by_account(1_000_000_000_001))
            .unwrap();
        assert_eq!((again.player_id, again.bound), (host, false));
        assert_eq!(games.user_data(host).unwrap().account_id, 1_000_000_000_001);
        // The host's account is taken, so the host's own lookup of the guest binds the guest.
        let back = games
            .find_user(host, LOOKUP_BY_ACCOUNT_ID, &by_account(1_000_000_000_002))
            .unwrap();
        assert_eq!(back.player_id, guest);
    }

    #[test]
    fn unknown_account_with_two_candidates_is_not_found() {
        let mut games = Games::default();
        let requester = log_in(&mut games, "a");
        log_in(&mut games, "b");
        log_in(&mut games, "c");
        assert!(
            games
                .find_user(requester, LOOKUP_BY_ACCOUNT_ID, &by_account(7_000_000_000))
                .is_none()
        );
    }

    #[test]
    fn persona_name_ignores_ascii_case() {
        let mut games = Games::default();
        let requester = log_in(&mut games, "a");
        let host = log_in(&mut games, "Host");
        let identity = UserIdentification {
            account_id: 0,
            blaze_id: 0,
            name: "host".into(),
        };
        let found = games
            .find_user(requester, LOOKUP_BY_PERSONA_NAME, &identity)
            .unwrap();
        assert_eq!(found.player_id, host);
    }
}
