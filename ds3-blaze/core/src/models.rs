//! TDF bodies of the login-stage requests, responses and notifications.
//!
//! Field layouts follow Pocket Relay Server's Mass Effect 3 models
//! (`src/session/models`, MIT, Jacobtread), adapted to Dead Space 3.

use std::net::Ipv4Addr;
use std::time::Duration;
use tdf::prelude::{
    DecodeResult, GroupSlice, ObjectId, TaggedUnion, TdfDeserializeOwned, TdfDeserializer,
    TdfSerialize, TdfSerializer, TdfType, TdfTyped,
};

/// Service name the client asks the redirector for and the server reports in `INST`.
pub const SERVICE_NAME: &str = "deadspace-3-pc";
/// Server version string reported in `preAuth`.
pub const SERVER_VERSION: &str = "Blaze 3.19.07.0 (ds3-blaze)\n";
/// Auth and resource source id reported in `preAuth` (`ASRC`, `RSRC`) and `postAuth`.
pub const AUTH_SOURCE: &str = "303107";
/// Ping site alias the QoS configuration and session data refer to.
pub const PING_SITE: &str = "ea-sjc";
/// Ping period announced in `preAuth`; the client sends `ping` at this rate.
pub const PING_PERIOD: Duration = Duration::from_secs(15);

/// Client platform value for PC, as the client reports it in `CINF.CPFT`.
const PLATFORM_PC: u8 = 4;

/// Components the server reports as available in `preAuth` (`CIDS`).
const COMPONENT_IDS: &[u16] = &[
    0x1, 0x19, 0x4, 0x1c, 0x7, 0x9, 0xf802, 0x7800, 0xf, 0x7801, 0x7802, 0x7803, 0x7805, 0x7806,
    0x7d0,
];

/// Account data handed out to a client after `originLogin`.
#[derive(Clone)]
pub struct Player {
    pub id: u32,
    pub name: String,
    /// Locale from the client's `preAuth` (`CINF.LOC`), four ASCII bytes.
    pub locale: u32,
}

impl Player {
    /// Returns a player whose id is derived from `name`, so the same name keeps its id
    /// across connections.
    pub fn from_name(name: &str, locale: u32) -> Self {
        // FNV-1a, masked to a positive non-zero i32 range.
        let mut hash: u32 = 0x811c_9dc5;
        for byte in name.bytes() {
            hash ^= u32::from(byte);
            hash = hash.wrapping_mul(0x0100_0193);
        }
        let id = (hash & 0x7fff_ffff).max(1);
        Self {
            id,
            name: name.to_string(),
            locale,
        }
    }

    fn session_key(&self) -> String {
        format!("{:X}", self.id)
    }
}

/// Address of the QoS HTTP service announced in `preAuth`.
#[derive(Clone, Copy)]
pub struct QosHttp {
    pub host: Ipv4Addr,
    pub port: u16,
}

/// `preAuth` response.
pub struct PreAuthResponse {
    pub qos: QosHttp,
}

impl TdfSerialize for PreAuthResponse {
    fn serialize<S: TdfSerializer>(&self, w: &mut S) {
        let qos_host = self.qos.host.to_string();
        let site = |w: &mut S| {
            w.tag_str(b"PSA", &qos_host);
            w.tag_u16(b"PSP", self.qos.port);
            w.tag_str(b"SNA", "prod-sjc");
        };
        w.tag_zero(b"ANON");
        w.tag_str(b"ASRC", AUTH_SOURCE);
        w.tag_list_slice(b"CIDS", COMPONENT_IDS);
        w.tag_str_empty(b"CNGN");
        let ping_period = format!("{}s", PING_PERIOD.as_secs());
        w.group(b"CONF", |w| {
            w.tag_map_tuples(
                b"CONF",
                &[
                    ("pingPeriod", ping_period.as_str()),
                    ("voipHeadsetUpdateRate", "1000"),
                    ("xlspConnectionIdleTimeout", "300"),
                ],
            );
        });
        w.tag_str(b"INST", SERVICE_NAME);
        w.tag_bool(b"MINR", false);
        w.tag_str(b"NASP", "cem_ea_id");
        w.tag_str_empty(b"PILD");
        w.tag_str(b"PLAT", "pc");
        w.tag_str_empty(b"PTAG");
        w.group(b"QOSS", |w| {
            w.group(b"BWPS", |w| site(w));
            w.tag_u8(b"LNP", 1);
            w.tag_map_start(b"LTPS", TdfType::String, TdfType::Group, 1);
            PING_SITE.serialize(w);
            w.group_body(|w| site(w));
            w.tag_u32(b"SVID", 0x4541_0805);
        });
        w.tag_str(b"RSRC", AUTH_SOURCE);
        w.tag_str(b"SVER", SERVER_VERSION);
    }
}

/// `TELE` group: telemetry endpoint. Nothing listens there; the client sends nothing
/// essential to it.
struct Telemetry {
    host: Ipv4Addr,
}

impl TdfSerialize for Telemetry {
    fn serialize<S: TdfSerializer>(&self, w: &mut S) {
        w.group(b"TELE", |w| {
            w.tag_str(b"ADRS", &self.host.to_string());
            w.tag_zero(b"ANON");
            w.tag_str_empty(b"DISA");
            w.tag_str_empty(b"FILT");
            w.tag_u32(b"LOC", u32::from_be_bytes(*b"enUS"));
            w.tag_str_empty(b"NOOK");
            w.tag_u16(b"PORT", 9988);
            w.tag_u16(b"SDLY", 15000);
            w.tag_str_empty(b"SESS");
            w.tag_str_empty(b"SKEY");
            w.tag_u8(b"SPCT", 0);
            w.tag_str_empty(b"STIM");
        });
    }
}

/// `TICK` group: ticker endpoint, unused like [`Telemetry`].
struct Ticker {
    host: Ipv4Addr,
}

impl TdfSerialize for Ticker {
    fn serialize<S: TdfSerializer>(&self, w: &mut S) {
        w.group(b"TICK", |w| {
            w.tag_str(b"ADRS", &self.host.to_string());
            w.tag_u16(b"PORT", 8999);
            w.tag_str(
                b"SKEY",
                &format!("1,{}:8999,{SERVICE_NAME},10,50,50,50,50,0,12", self.host),
            );
        });
    }
}

/// `postAuth` response.
pub struct PostAuthResponse {
    pub host: Ipv4Addr,
    pub player_id: u32,
}

impl TdfSerialize for PostAuthResponse {
    fn serialize<S: TdfSerializer>(&self, w: &mut S) {
        w.group(b"PSS", |w| {
            w.tag_str(b"ADRS", "playersyncservice.ea.com");
            w.tag_blob_empty(b"CSIG");
            w.tag_str(b"PJID", AUTH_SOURCE);
            w.tag_u16(b"PORT", 443);
            w.tag_u8(b"RPRT", 0xF);
            w.tag_u8(b"TIID", 0);
        });
        Telemetry { host: self.host }.serialize(w);
        Ticker { host: self.host }.serialize(w);
        w.group(b"UROP", |w| {
            w.tag_u8(b"TMOP", 1);
            w.tag_u32(b"UID", self.player_id);
        });
    }
}

/// `getTelemetryServer` response.
pub struct TelemetryResponse {
    pub host: Ipv4Addr,
}

impl TdfSerialize for TelemetryResponse {
    fn serialize<S: TdfSerializer>(&self, w: &mut S) {
        Telemetry { host: self.host }.serialize(w);
    }
}

/// `getTickerServer` response.
pub struct TickerResponse {
    pub host: Ipv4Addr,
}

impl TdfSerialize for TickerResponse {
    fn serialize<S: TdfSerializer>(&self, w: &mut S) {
        Ticker { host: self.host }.serialize(w);
    }
}

fn write_persona<S: TdfSerializer>(w: &mut S, player: &Player) {
    w.group_body(|w| {
        w.tag_str(b"DSNM", &player.name);
        w.tag_zero(b"LAST");
        w.tag_u32(b"PID", player.id);
        w.tag_zero(b"STAS");
        w.tag_zero(b"XREF");
        w.tag_zero(b"XTYP");
    });
}

fn email(player: &Player) -> String {
    format!("{}@ds3-blaze.local", player.id)
}

fn write_session<S: TdfSerializer>(w: &mut S, player: &Player) {
    w.tag_u32(b"BUID", player.id);
    w.tag_zero(b"FRST");
    w.tag_str(b"KEY", &player.session_key());
    w.tag_zero(b"LLOG");
    w.tag_str(b"MAIL", &email(player));
    w.tag_group(b"PDTL");
    write_persona(w, player);
    w.tag_u32(b"UID", player.id);
}

/// `originLogin` / `silentLogin` response in its "silent" form with an embedded session.
pub struct AuthResponse<'a> {
    pub player: &'a Player,
}

impl TdfSerialize for AuthResponse<'_> {
    fn serialize<S: TdfSerializer>(&self, w: &mut S) {
        w.tag_zero(b"AGUP");
        w.tag_str_empty(b"LDHT");
        w.tag_zero(b"NTOS");
        w.tag_str(b"PCTK", &self.player.session_key());
        w.tag_str_empty(b"PRIV");
        w.group(b"SESS", |w| write_session(w, self.player));
        w.tag_zero(b"SPAM");
        w.tag_str_empty(b"THST");
        w.tag_str_empty(b"TSUI");
        w.tag_str_empty(b"TURI");
    }
}

/// `loginPersona` response.
pub struct PersonaResponse<'a> {
    pub player: &'a Player,
}

impl TdfSerialize for PersonaResponse<'_> {
    fn serialize<S: TdfSerializer>(&self, w: &mut S) {
        write_session(w, self.player);
    }
}

/// `getAuthToken` response.
pub struct AuthTokenResponse<'a> {
    pub player: &'a Player,
}

impl TdfSerialize for AuthTokenResponse<'_> {
    fn serialize<S: TdfSerializer>(&self, w: &mut S) {
        w.tag_str(b"AUTH", &self.player.session_key());
    }
}

/// `listUserEntitlements2` response with no entitlements.
pub struct EmptyEntitlements;

impl TdfSerialize for EmptyEntitlements {
    fn serialize<S: TdfSerializer>(&self, w: &mut S) {
        w.tag_list_empty(b"NLST", TdfType::Group);
    }
}

/// Entitlement group the online pass is listed under; the client asks for it in
/// `ListEntitlements` (`GNLS`).
pub const ENTITLEMENT_GROUP: &str = "DeadSpace3PC";
/// Entitlement tags of Dead Space 3's online pass, as named in the game's strings.
const ONLINE_PASS_TAGS: &[&str] = &["ONLINE_ACCESS", "PROJECT10_CODE_CONSUMED"];
/// Entitlement `TYPE` that Mass Effect 3's online access entitlements carry.
const ENTITLEMENT_TYPE_ONLINE_ACCESS: u8 = 1;
/// Entitlement `STAT` of an active entitlement.
const ENTITLEMENT_ACTIVE: u8 = 1;

/// `ListEntitlements` request reduced to the requested group names.
pub struct ListEntitlementsRequest {
    pub groups: Vec<String>,
}

impl TdfDeserializeOwned for ListEntitlementsRequest {
    fn deserialize_owned(r: &mut TdfDeserializer<'_>) -> DecodeResult<Self> {
        Ok(Self {
            groups: r.tag(b"GNLS")?,
        })
    }
}

/// `ListEntitlements` response holding the online pass when its group was requested.
pub struct OnlinePassEntitlements<'a> {
    pub player: &'a Player,
    /// Whether [`ENTITLEMENT_GROUP`] is among the requested groups.
    pub requested: bool,
}

impl OnlinePassEntitlements<'_> {
    /// Returns the tags this response lists.
    pub fn tags(&self) -> &'static [&'static str] {
        if self.requested {
            ONLINE_PASS_TAGS
        } else {
            &[]
        }
    }
}

impl TdfSerialize for OnlinePassEntitlements<'_> {
    fn serialize<S: TdfSerializer>(&self, w: &mut S) {
        let tags = self.tags();
        w.tag_list_start(b"NLST", TdfType::Group, tags.len());
        for (index, tag) in tags.iter().enumerate() {
            w.group_body(|w| {
                w.tag_str_empty(b"DEVI");
                w.tag_str(b"GDAY", "2013-02-05T00:00Z");
                w.tag_str(b"GNAM", ENTITLEMENT_GROUP);
                w.tag_u64(b"ID", index as u64 + 1);
                w.tag_u8(b"ISCO", 0);
                w.tag_u32(b"PID", self.player.id);
                w.tag_str(b"PJID", AUTH_SOURCE);
                w.tag_u8(b"PRCA", 2);
                w.tag_str_empty(b"PRID");
                w.tag_u8(b"STAT", ENTITLEMENT_ACTIVE);
                w.tag_u8(b"STRC", 0);
                w.tag_str(b"TAG", tag);
                w.tag_str_empty(b"TDAY");
                w.tag_u8(b"TYPE", ENTITLEMENT_TYPE_ONLINE_ACCESS);
                w.tag_u8(b"UCNT", 0);
                w.tag_u8(b"VER", 0);
            });
        }
    }
}

/// `userSettingsLoad` response.
pub struct UserSettingResponse<'a> {
    pub key: &'a str,
    pub data: &'a str,
}

impl TdfSerialize for UserSettingResponse<'_> {
    fn serialize<S: TdfSerializer>(&self, w: &mut S) {
        w.tag_str(b"DATA", self.data);
        w.tag_str(b"KEY", self.key);
    }
}

/// Association list id (`LID`) of a `getLists` request entry.
pub struct AssociationListId {
    pub name: String,
    pub ty: u8,
}

impl TdfTyped for AssociationListId {
    const TYPE: TdfType = TdfType::Group;
}

impl TdfDeserializeOwned for AssociationListId {
    fn deserialize_owned(r: &mut TdfDeserializer<'_>) -> DecodeResult<Self> {
        let (name, ty) = r.group(b"LID", |r| {
            Ok((r.tag::<String>(b"LNM")?, r.tag::<u8>(b"TYPE")?))
        })?;
        GroupSlice::deserialize_content_skip(r)?;
        Ok(Self { name, ty })
    }
}

/// `getLists` request reduced to the requested list ids.
pub struct GetListsRequest {
    pub lists: Vec<AssociationListId>,
}

impl TdfDeserializeOwned for GetListsRequest {
    fn deserialize_owned(r: &mut TdfDeserializer<'_>) -> DecodeResult<Self> {
        Ok(Self {
            lists: r.tag(b"ALST")?,
        })
    }
}

/// Object type of an association list (`BOID`).
const ASSOCIATION_LIST_OBJECT_TYPE: u16 = 1;
/// `FLGS` and `LMS` (maximum size) of the association lists Pocket Relay returns.
const ASSOCIATION_LIST_FLAGS: u8 = 4;
const ASSOCIATION_LIST_MAX_SIZE: u8 = 0xC8;

/// `getLists` response with one empty list per requested id.
pub struct GetListsResponse<'a> {
    pub player: &'a Player,
    pub lists: &'a [AssociationListId],
}

impl TdfSerialize for GetListsResponse<'_> {
    fn serialize<S: TdfSerializer>(&self, w: &mut S) {
        w.tag_list_start(b"LMAP", TdfType::Group, self.lists.len());
        for list in self.lists {
            w.group_body(|w| {
                w.group(b"INFO", |w| {
                    w.tag_alt(
                        b"BOID",
                        ObjectId::new_raw(
                            crate::components::association_lists::COMPONENT,
                            ASSOCIATION_LIST_OBJECT_TYPE,
                            u64::from(self.player.id),
                        ),
                    );
                    w.tag_u8(b"FLGS", ASSOCIATION_LIST_FLAGS);
                    w.group(b"LID", |w| {
                        w.tag_str(b"LNM", &list.name);
                        w.tag_u8(b"TYPE", list.ty);
                    });
                    w.tag_u8(b"LMS", ASSOCIATION_LIST_MAX_SIZE);
                    w.tag_u8(b"PRID", 0);
                });
                w.tag_zero(b"OFRC");
                w.tag_zero(b"TOCT");
            });
        }
    }
}

/// `ping` response.
pub struct PingResponse {
    pub server_time: u64,
}

impl TdfSerialize for PingResponse {
    fn serialize<S: TdfSerializer>(&self, w: &mut S) {
        w.tag_owned(b"STIM", self.server_time);
    }
}

/// `fetchClientConfig` / `userSettingsLoadAll` response: one string map under `tag`.
pub struct StringMapResponse<'a> {
    pub tag: &'static [u8],
    pub entries: &'a [(String, String)],
}

impl TdfSerialize for StringMapResponse<'_> {
    fn serialize<S: TdfSerializer>(&self, w: &mut S) {
        w.tag_map_start(
            self.tag,
            TdfType::String,
            TdfType::String,
            self.entries.len(),
        );
        for (key, value) in self.entries {
            key.as_str().serialize(w);
            value.as_str().serialize(w);
        }
    }
}

/// IPv4 address and port as Blaze encodes them (`IP` as u32, `PORT`).
#[derive(Clone, Copy, Debug)]
pub struct IpPort {
    pub ip: Ipv4Addr,
    pub port: u16,
}

impl Default for IpPort {
    fn default() -> Self {
        Self {
            ip: Ipv4Addr::UNSPECIFIED,
            port: 0,
        }
    }
}

impl IpPort {
    fn read(r: &mut TdfDeserializer<'_>, tag: &[u8]) -> DecodeResult<Self> {
        r.group(tag, |r| {
            let ip: u32 = r.tag(b"IP")?;
            let port: u16 = r.tag(b"PORT")?;
            Ok(Self {
                ip: Ipv4Addr::from(ip),
                port,
            })
        })
    }

    fn write<S: TdfSerializer>(&self, w: &mut S, tag: &[u8]) {
        w.group(tag, |w| {
            w.tag_u32(b"IP", u32::from(self.ip));
            w.tag_u16(b"PORT", self.port);
        });
    }
}

/// External and internal address pair (`NetworkAddress` union member 2, `VALU`).
#[derive(Clone, Copy, Default, Debug)]
pub struct AddressPair {
    pub external: IpPort,
    pub internal: IpPort,
}

impl TdfTyped for AddressPair {
    const TYPE: TdfType = TdfType::Group;
}

impl TdfDeserializeOwned for AddressPair {
    fn deserialize_owned(r: &mut TdfDeserializer<'_>) -> DecodeResult<Self> {
        let external = IpPort::read(r, b"EXIP")?;
        let internal = IpPort::read(r, b"INIP")?;
        GroupSlice::deserialize_content_skip(r)?;
        Ok(Self { external, internal })
    }
}

/// `NetworkAddress` union key of an [`AddressPair`].
const ADDRESS_PAIR_KEY: u8 = 0x2;

/// Client QoS summary (`NQOS` in requests, `QDAT` in session data).
#[derive(Clone, Copy, Default, Debug)]
pub struct QosData {
    pub downstream_bps: u32,
    pub nat_type: u8,
    pub upstream_bps: u32,
}

impl QosData {
    fn read(r: &mut TdfDeserializer<'_>, tag: &[u8]) -> DecodeResult<Self> {
        r.group(tag, |r| {
            Ok(Self {
                downstream_bps: r.tag(b"DBPS")?,
                nat_type: r.tag(b"NATT")?,
                upstream_bps: r.tag(b"UBPS")?,
            })
        })
    }

    /// Writes the summary as group `tag`.
    pub fn write<S: TdfSerializer>(&self, w: &mut S, tag: &[u8]) {
        w.group(tag, |w| {
            w.tag_u32(b"DBPS", self.downstream_bps);
            w.tag_u8(b"NATT", self.nat_type);
            w.tag_u32(b"UBPS", self.upstream_bps);
        });
    }
}

/// `updateNetworkInfo` request.
pub struct UpdateNetworkRequest {
    /// `None` when `ADDR` is unset or not an address pair.
    pub address: Option<AddressPair>,
    pub qos: QosData,
}

impl TdfDeserializeOwned for UpdateNetworkRequest {
    fn deserialize_owned(r: &mut TdfDeserializer<'_>) -> DecodeResult<Self> {
        let address = match r.tag::<TaggedUnion<AddressPair>>(b"ADDR")? {
            TaggedUnion::Set { key, value, .. } if key == ADDRESS_PAIR_KEY => Some(value),
            _ => None,
        };
        let qos = QosData::read(r, b"NQOS")?;
        Ok(Self { address, qos })
    }
}

/// Per-session network state published in user session notifications.
#[derive(Clone, Default)]
pub struct NetData {
    pub address: Option<AddressPair>,
    pub qos: QosData,
    pub hardware_flags: u8,
}

/// Writes `address` under `tag` as a `NetworkAddress` union: an [`AddressPair`]
/// under `VALU`, or unset for `None`.
pub fn write_network_address<S: TdfSerializer>(
    w: &mut S,
    tag: &[u8],
    address: Option<&AddressPair>,
) {
    match address {
        Some(pair) => {
            w.tag_union_start(tag, ADDRESS_PAIR_KEY);
            w.group(b"VALU", |w| {
                pair.external.write(w, b"EXIP");
                pair.internal.write(w, b"INIP");
            });
        }
        None => w.tag_union_unset(tag),
    }
}

/// Writes `address` as one element of a group list holding `NetworkAddress` unions.
///
/// Such elements carry the union key in place of a group start and the pair's
/// fields directly after it, ended like a group; the client decodes `HNET` in
/// this form only.
pub fn write_network_address_element<S: TdfSerializer>(w: &mut S, address: Option<&AddressPair>) {
    match address {
        Some(pair) => {
            w.write_byte(ADDRESS_PAIR_KEY);
            pair.external.write(w, b"EXIP");
            pair.internal.write(w, b"INIP");
            w.tag_group_end();
        }
        None => {
            w.write_byte(UNION_UNSET_KEY);
            w.tag_group_end();
        }
    }
}

/// Union key of an unset `NetworkAddress`.
const UNION_UNSET_KEY: u8 = 0x7F;

/// Object type of a game in a user session's object list (`ULST`).
const GAME_OBJECT_TYPE: u16 = 1;

/// `UserSessionExtendedData` group: `DATA` of user session notifications, `EDAT`
/// of a `lookupUsers` entry.
struct ExtendedData<'a> {
    tag: &'static [u8],
    net: &'a NetData,
    /// Ids of the games the user is a member of, listed in `ULST` when not empty.
    games: &'a [u32],
}

impl TdfSerialize for ExtendedData<'_> {
    fn serialize<S: TdfSerializer>(&self, w: &mut S) {
        w.group(self.tag, |w| {
            write_network_address(w, b"ADDR", self.net.address.as_ref());
            w.tag_str(b"BPS", PING_SITE);
            w.tag_str_empty(b"CTY");
            w.tag_var_int_list_empty(b"CVAR");
            w.tag_map_empty(b"DMAP", TdfType::VarInt, TdfType::VarInt);
            w.tag_u8(b"HWFG", self.net.hardware_flags);
            w.tag_list_empty(b"PSLM", TdfType::VarInt);
            self.net.qos.write(w, b"QDAT");
            w.tag_u8(b"UATT", 0);
            if !self.games.is_empty() {
                let objects: Vec<ObjectId> = self
                    .games
                    .iter()
                    .map(|&id| {
                        ObjectId::new_raw(
                            crate::components::game_manager::COMPONENT,
                            GAME_OBJECT_TYPE,
                            u64::from(id),
                        )
                    })
                    .collect();
                w.tag_list_slice(b"ULST", &objects);
            }
        });
    }
}

/// `UserAdded` notification.
pub struct UserAdded<'a> {
    pub player: &'a Player,
    pub net: &'a NetData,
}

impl TdfSerialize for UserAdded<'_> {
    fn serialize<S: TdfSerializer>(&self, w: &mut S) {
        ExtendedData {
            tag: b"DATA",
            net: self.net,
            games: &[],
        }
        .serialize(w);
        write_user_identification(w, self.player, u64::from(self.player.id));
    }
}

/// Writes the `USER` group (`UserIdentification`) of `player` with account id
/// `account_id`.
fn write_user_identification<S: TdfSerializer>(w: &mut S, player: &Player, account_id: u64) {
    w.group(b"USER", |w| {
        w.tag_u64(b"AID", account_id);
        w.tag_u32(b"ALOC", player.locale);
        w.tag_blob_empty(b"EXBB");
        w.tag_zero(b"EXID");
        w.tag_u32(b"ID", player.id);
        w.tag_str(b"NAME", &player.name);
    });
}

/// `LookupUsersRequest.LookupType` of a lookup by Blaze user id.
pub const LOOKUP_BY_BLAZE_ID: u8 = 0;
/// `LookupUsersRequest.LookupType` of a lookup by persona name.
pub const LOOKUP_BY_PERSONA_NAME: u8 = 1;
/// `LookupUsersRequest.LookupType` of a lookup by EA account id.
pub const LOOKUP_BY_ACCOUNT_ID: u8 = 3;

/// One `UserIdentification` entry of a `lookupUsers` request.
pub struct UserIdentification {
    /// EA account id (`AID`).
    pub account_id: u64,
    /// Blaze user id (`ID`).
    pub blaze_id: u64,
    /// Persona name (`NAME`).
    pub name: String,
}

impl TdfTyped for UserIdentification {
    const TYPE: TdfType = TdfType::Group;
}

impl TdfDeserializeOwned for UserIdentification {
    fn deserialize_owned(r: &mut TdfDeserializer<'_>) -> DecodeResult<Self> {
        // Tags are read in ascending order: the reader only scans forward.
        let account_id: u64 = r.tag(b"AID")?;
        let blaze_id: u64 = r.tag(b"ID")?;
        let name: String = r.tag(b"NAME")?;
        GroupSlice::deserialize_content_skip(r)?;
        Ok(Self {
            account_id,
            blaze_id,
            name,
        })
    }
}

/// `lookupUsers` request (`LookupUsersRequest`).
pub struct LookupUsersRequest {
    /// `LookupType` (`LTYP`), one of the `LOOKUP_BY_*` values.
    pub lookup_type: u8,
    /// Users to look up (`ULST`).
    pub users: Vec<UserIdentification>,
}

impl TdfDeserializeOwned for LookupUsersRequest {
    fn deserialize_owned(r: &mut TdfDeserializer<'_>) -> DecodeResult<Self> {
        let lookup_type: u8 = r.tag(b"LTYP")?;
        let users = r.tag(b"ULST")?;
        Ok(Self { lookup_type, users })
    }
}

/// One user found by `lookupUsers` (`UserData`).
pub struct UserData<'a> {
    pub player: &'a Player,
    /// Account id reported in `USER.AID`.
    pub account_id: u64,
    pub net: &'a NetData,
    /// Ids of the games the user is a member of.
    pub games: Vec<u32>,
}

/// `lookupUsers` response (`UserDataResponse`).
pub struct UserDataResponse<'a> {
    pub users: Vec<UserData<'a>>,
}

impl TdfSerialize for UserDataResponse<'_> {
    fn serialize<S: TdfSerializer>(&self, w: &mut S) {
        w.tag_list_start(b"ULST", TdfType::Group, self.users.len());
        for user in &self.users {
            w.group_body(|w| {
                ExtendedData {
                    tag: b"EDAT",
                    net: user.net,
                    games: &user.games,
                }
                .serialize(w);
                w.tag_u8(b"FLGS", USER_SUBSCRIBED | USER_ONLINE);
                write_user_identification(w, user.player, user.account_id);
            });
        }
    }
}

/// `UserUpdated` flag: the receiver is subscribed to the user.
pub const USER_SUBSCRIBED: u8 = 0x1;
/// `UserUpdated` flag: the user is online.
pub const USER_ONLINE: u8 = 0x2;

/// `UserUpdated` notification.
pub struct UserUpdated {
    pub flags: u8,
    pub player_id: u32,
}

impl TdfSerialize for UserUpdated {
    fn serialize<S: TdfSerializer>(&self, w: &mut S) {
        w.tag_u8(b"FLGS", self.flags);
        w.tag_u32(b"ID", self.player_id);
    }
}

/// Object type of a user session id (`CGID` of [`UserAuthenticated`]).
const USER_OBJECT_TYPE: u16 = 1;

/// `UserAuthenticated` notification announcing the logged-in user to its own connection.
///
/// Carries the `SESS` fields of [`AuthResponse`] together with the flattened persona
/// fields of the layout other Blaze 3 titles receive; the decoder skips the tags it
/// does not know.
pub struct UserAuthenticated<'a> {
    pub player: &'a Player,
}

impl TdfSerialize for UserAuthenticated<'_> {
    fn serialize<S: TdfSerializer>(&self, w: &mut S) {
        let player = self.player;
        // Tags stay in ascending order: the client decoder scans forward only.
        w.tag_u32(b"ALOC", player.locale);
        w.tag_u32(b"BUID", player.id);
        w.tag_alt(
            b"CGID",
            ObjectId::new_raw(
                crate::components::user_sessions::COMPONENT,
                USER_OBJECT_TYPE,
                u64::from(player.id),
            ),
        );
        w.tag_str(b"DSNM", &player.name);
        w.tag_zero(b"FRST");
        w.tag_str(b"KEY", &player.session_key());
        w.tag_zero(b"LAST");
        w.tag_zero(b"LLOG");
        w.tag_str(b"MAIL", &email(player));
        w.tag_str(b"NASP", "cem_ea_id");
        w.tag_group(b"PDTL");
        write_persona(w, player);
        w.tag_u32(b"PID", player.id);
        w.tag_u8(b"PLAT", PLATFORM_PC);
        w.tag_u32(b"UID", player.id);
        w.tag_zero(b"USTP");
        w.tag_zero(b"XREF");
    }
}

/// `UserSessionExtendedDataUpdate` notification for the user's own session.
pub struct ExtendedDataUpdate<'a> {
    pub player_id: u32,
    pub net: &'a NetData,
    /// Ids of the games the user is a member of.
    pub games: &'a [u32],
}

impl TdfSerialize for ExtendedDataUpdate<'_> {
    fn serialize<S: TdfSerializer>(&self, w: &mut S) {
        ExtendedData {
            tag: b"DATA",
            net: self.net,
            games: self.games,
        }
        .serialize(w);
        w.tag_u8(b"SUBS", USER_SUBSCRIBED);
        w.tag_u32(b"USID", self.player_id);
    }
}
