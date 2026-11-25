# The Blaze protocol in DS3: what the game requires

Observations from the logs of `ds3-blaze serve` and `ds3-blaze proxy`, by work stage.

## Own redirector (stage 1, 2026-11-24)

Setup: `ds3-blaze serve` (the `ds3-blaze/` crate); `hosts` points `gosredirector.ea.com` to `127.0.0.1`. The redirector uses the default certificate from `blaze-ssl-async`, and its `getServerInstance` response names a main-server probe on `127.0.0.1:42128` with `SECU=0`. Log: `ds3-blaze/logs/ds3-blaze-20261124-130718.log`.

Result:

- **The bypass certificate works on DS3 without a client patch.** The SSLv3 handshake completes in 0.1 s.
- The `getServerInstance` request (component `0x5`, command `0x1`) arrives with the fields `BSDK "3.19.7.0 (3.19.07.0)"`, `BTIM "Nov 27 2012 13:44:59"`, `CLNT "/PC/DEADSPACE-3"`, `CLTP 0`, `CPLT 4`, `CSKU "DS3"`, `CVER "1.0"`, `DSDK "8.18.8.0"`, `ENV "prod"`, `FPID` (union, unset), `LOC` (`ruRU` as 4 ASCII bytes), `NAME "deadspace-3-pc"`, `PLAT "Windows"`, `PROF "standardSecure_v3"`.
- The game accepts a minimal response in Pocket Relay's format (`ADDR`/`VALU` with `IP` and `PORT`, `SECU`, `XDNS`).
- With `SECU=0` the client connects to the main server over **plain TCP**, without SSL. So the emulator's main server does not need SSLv3.
- The first request to the main server is `preAuth` (component `0x9` Util, command `0x7`) with the groups `CDAT` (`IITO`, `LANG`, `SVCN "deadspace-3-pc"`, `TYPE`), `CINF` (`BSDK`, `BTIM`, `CLNT`, `CPFT`, `CSKU`, `CVER`, `DSDK`, `ENV`, `HWID`, `LOC`, `MAC`) and `FCCR` (`CFID "BlazeSDK"`). Without a response, the client closes the connection after 10 s and starts over from the redirector.
- Request `seq` numbering starts at 0 and continues across connections.

## Login (stage 2, 2026-11-24)

Logs: `ds3-blaze/logs/ds3-blaze-20261124-134540.log`, `…-135814.log`, `…-140859.log`. The last run reached the online menu and room creation.

What DS3 requires beyond the ME3 model in Pocket Relay:

- **The client sends `postAuth` only after the notification `UserSessions::UserAuthenticated` (`0x7802`/`0x8`).** Given an `originLogin` response in ME3's "silent" format, the game only pings the server. The server sends `UserAdded`, `UserUpdated` and `UserAuthenticated` before the `originLogin` response.
- **The bandwidth QoS** (`qtyp=2`, 1200-byte probes) expects replies in EA's format: the same length, the first 12 bytes of the probe, then the probe number plus one and the probe count. The game does not accept a plain echo with the address at the end and never gets to `/qos/firewall`.
- **Online pass.** After login the game requests `listEntitlements` (`0x1`/`0x20`) for the groups `DeadSpace`, `DeadSpace3`, `DeadSpace3PC`. The exe strings include `ONLINE_ACCESS` and `PROJECT10_CODE_CONSUMED`; with an empty list the game hangs while connecting, and with these two entitlements in the `DeadSpace3PC` group it proceeds to the menu.
- After `postAuth` come `updateNetworkInfo` (with `NLMP`, latencies to the sites), `getTelemetryServer`, `getLists` (types 1 and 2), `getAccount`, `updateHardwareFlags`, `fetchClientConfig CFID="MessageOfTheDay"`, `userSettingsLoad KEY="mtid"`, `setUsersToList` (the friends list from the EA App, type 1, and an empty type 4), `setClientMetrics`. Empty responses to `getAccount`, `MessageOfTheDay`, `mtid` and `setUsersToList` are enough for the game.
- `createGame` (`0x4`/`0x1`) arrives with `GNAM "Dead Space 3 (public)"`, `GSET 287`, `NTOP 130` (full mesh), `PCAP [2, 0]`, `PMAX 8`, `VSTR` with the build date, and chapter and difficulty attributes (`GAME_MODE`, `GAME_CHECKPOINT`, `MAP_DISPLAY_NAME` and so on).

## Room creation: reference from EA's servers (stage 3, 2026-11-24)

A room built after Pocket Relay's pattern was destroyed by DS3 30 ms after `GameSetup` (`destroyGame`, `REAS 3`), and the game reported a lost connection to the EA server. The `GameSetup` fields matched Pocket Relay and the Blaze 3 TDF definitions from Aim4kill/BlazeSDK, and the exe has no debug output. So the reference was taken from EA's live servers with `ds3-blaze proxy`: the game connects to the emulator over plain TCP, the emulator speaks SSLv3 to `gsprodblapp-03.ea.com:10323` and logs every message. EA accepts the `blaze-ssl-async` handshake with RC4 (check: `ds3-blaze ea-check`). Proxy log: `ds3-blaze/logs/ds3-blaze-20261124-144446.log`. It contains account data and must not be published.

What the reference showed:

- **Order.** EA first answers `createGame` (`GID`), then sends `GameSetup`, then `UserSessionExtendedDataUpdate`, where the game's `ObjectId` (component `0x4`, type `1`) appears in `DATA.ULST` and `SUBS 1` is set. Pocket Relay sends `GameSetup` before the response. `GameStateChange` and `PlayerAttribChange` likewise come after their responses.
- **The hosting game then sends** `finalizeGameCreation` (`GID`, empty `XNNC`/`XSES`), `advanceGameState` to `PRE_GAME` (`0x82`) and `setPlayerAttributes` (`UNIFORM_ATTR`, the suit). It leaves the room through `removePlayer` with `REAS 6`; in reply come `PlayerRemoved` (`CNTX`, `GID`, `PID`, `REAS`) and a session update without `ULST`.
- **EA's `GameSetup` fields:** the same as in the request, plus `GURL ""`, an empty `PSAS`, the player's `TIME` in microseconds since 1970, `UID`/`HSES` (a session id separate from `PID`). `GPVH` is the same for both rooms but matches no FNV-1/1a variant; the emulator takes EA's value for the known version (`KNOWN_VERSION_HASHES` in `game.rs`).
- **Enums from the exe strings** (listed in reverse declaration order): `GameDestructionReason` has `SYS_GAME_RECREATE` at number 2, so `REAS 3` is `HOST_LEAVING`; in `PlayerRemovedReason`, `6` is `PLAYER_LEFT` and `2` is `BLAZESERVER_CONN_LOST`.
- **Matchmaking (quick match).** The `startMatchmaking` response contains `MSID`. Then `MatchmakingAsyncStatus` arrives once a second (the state of the `gameDS*MatchRule` rules, the number of games in the pool), and once the request's `DUR` (27.5 s) runs out, `MatchmakingFailed` arrives with `RSLT 3` (`TIMED_OUT`) and `MAXF 13000`.
- For reference: EA answers `userSettingsLoad KEY="mtid"` with error `0xC8` if the setting was never saved; `UserAuthenticated` arrives before the `originLogin` response, and `UserAdded` and `UserUpdated` after it.

**Check on the emulator (log `ds3-blaze-20261124-145421.log`).** With EA's order, the single-host scenario runs in `serve` from start to finish: quick match ends in `MatchmakingFailed`, the room is created, the game moves it to `IN_GAME` (`advanceGameState`, `GSTA 0x83`), updates the attributes `GAME_STATUS`, `UNIFORM_GUID`, `UNIFORM2_GUID` and the player's suit, and on quitting with a save sends `removePlayer` with `REAS 6`. Only `getAccount` and `setUsersToList` remain without a handler, and empty responses are enough for the game. The EA App meanwhile stays online, shows the game to friends and offers to invite them; what the invite carries and which server the invitee goes to has not been checked yet.

Important for the second player: a host running the emulator on the same PC reports `EXIP` as `127.0.0.1` (QoS sees it from loopback) and `INIP` from its local network. Neither address is usable by a partner over the VPN.

## Second player (stage 3, without an EA reference)

A second player's join cannot be captured from EA's servers: co-op through EA does not come together for these players, which is the whole reason for the emulator. So the join is built from the Blaze 3 definitions in Aim4kill/BlazeSDK (command and notification numbers match the exe strings) and from Pocket Relay's flow, while the "response first, then notifications" order is taken from the room creation reference.

- **Names and addresses.** The player on the emulator's PC and on the `--advertise` address gets the `--name` name, a player at an address from `--player IP=NAME` gets the given name, and everyone else gets `Player-<last octet>`. A name already in use is suffixed with `-2`, `-3`. In `updateNetworkInfo` both of the player's addresses are replaced with the one their connection came from (loopback is replaced with `--advertise`), and the port is taken from the internal address. If that address is loopback as well, the addresses stay unchanged.
- **How the guest gets into a room.** Quick match, at once or within `DUR` (searching once a second), finds the oldest room with bit `0x2` in `GSET` (open to matchmaking), in the lobby or in game with bit `0x100` (join in progress), that has a free slot according to `PCAP`. A public DS3 room (`0x11F`) qualifies; a private one (`0x114`, invite only) does not. `joinGame` leads into a private room: by `GID` or, if no such game exists, by the user in `USER.ID`.
- **Notifications on join.** The host receives the guest's `UserAdded` and `UserUpdated`, then `PlayerJoining`. The guest receives the same pair for the host, `GameSetup` (matchmaking context: `FIT`, `MAXF`, `MSID`, `RSLT 2`, `USID`; for `joinGame` the context is `DCTX 1`) and a session update with `ULST`. The guest joins in the `ACTIVE_CONNECTING` state.
- **P2P.** When either of the two reports a `CONNECTED` link in `updateMeshConnection`, the guest moves to `ACTIVE_CONNECTED`, and both receive `GamePlayerStateChange` and `PlayerJoinCompleted`. A `DISCONNECTED` link between host and guest removes the guest.
- **EA App invite.** The player who accepts the invite leaves their own room (`removePlayer`, `REAS 6`) and looks up the inviter through `lookupUsers` with `LTYP 3`: by the EA account id (`AID`) the game takes from the EA App. The game repeats an empty response several times a second without end, which is what an endless "connecting" looks like. The server does not see account ids at login, so an unknown id is bound to the only other player on the server and stays with them while they are logged in (a lookup that finds them offline binds the id again); with two or more such players the response is `USER_ERR_USER_NOT_FOUND`. The `UserDataResponse` carries the player's session with the `ULST` of their rooms and `USER.AID` equal to the requested id. The game then sends `joinGame` with `GID 0`, the inviter's `USER.ID`, `JMET 4` and `SLOT 1` (private slot), and the server seats it in that player's room. Confirmed in game over VPN on 2026-11-25 (log `ds3-blaze-20261125-030406.log`): the guest accepted an invite into a private room (`0x114`), both sides reported a `CONNECTED` link, and the room moved to `IN_GAME`.
- **Host leaving.** There is no host migration: the guest receives the host's `PlayerRemoved` and `GameRemoved` (`HOST_LEAVING`), and the room is destroyed.

The self-test exercises all of this with a second client. In game with two PCs over VPN (log `ds3-blaze-20261125-021400.log`), quick match went through completely: the guest joined a public room through `startMatchmaking`, both sides reported a `CONNECTED` link in `updateMeshConnection`, the game ran for about six minutes, and both left through `removePlayer`. An EA App invite in the same run hung on `lookupUsers`, which had no handler yet; with the handler it goes through (see above). Before a two-PC run, check the network as described in [network-checks.md](../guides/network-checks.md).

## DLC (2026-11-24)

The server does not report DLC ownership; the online pass is the only entitlement the game needs from Blaze.

- **EA's reply.** In the proxy log `ds3-blaze-20261124-144446.log`, EA answers `listEntitlements` for the groups `DeadSpace`, `DeadSpace3`, `DeadSpace3PC` with a single entitlement: `ONLINE_ACCESS` in `DeadSpace3PC` (`TYPE 1`, `STAT 1`). `PROJECT10_CODE_CONSUMED` is not in it either, although the emulator grants it.
- **Where the game checks DLC.** The exe strings hold Origin SDK LSX requests (`QueryEntitlements`, `QueryOffers`, `ConsumeEntitlement`) and the item tags `DS3:PDLC_DLC1`, `DS3:PDLC_TUNDRA`, `DS3:PDLC_MARAUDER` and others. Ownership is asked of the EA App on the same PC, and the content is loaded from local files (`dlc\%s\%s.toc`). Errors such as `$load_failed_dlc_unauthorized` and `$dlc_join_failed_chapter` are the client's own.
- **The only DLC field in Blaze traffic** is the matchmaking rule `gameDSMapPackRule` in `startMatchmaking`; without DLC it comes with no `VALU`. The emulator ignores matchmaking rules (it compares only the game version), so quick match may place a player without a DLC chapter in a room that is in one, and the client then refuses the join itself.

Not verified: the captured account owns no DLC, so EA's `listEntitlements` reply and the `gameDSMapPackRule` values for a DLC owner are unknown.
