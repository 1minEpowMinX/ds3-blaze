# Work plan and risks

## Stages

| Stage | Result | How to verify |
|---|---|---|
| 0. Traffic capture | Done (captures #1 and #2 in [captures.md](../research/captures.md)): ports, SSL, QoS and demangler formats, P2P port 3659, cause of the disconnects (symmetric NAT/CGNAT). The content of Blaze messages can only be seen on our own server | — |
| 1. Redirector + SSL | Done on 2026-11-24 (see "Own redirector" in [protocol.md](../research/protocol.md)): the handshake with the bypass certificate succeeds, the game moves to the main server over plain TCP and sends `preAuth` | `getServerInstance` with `deadspace-3-pc` appears in the emulator log |
| 2. Login | Done on 2026-11-24 (see "Login" in [protocol.md](../research/protocol.md)): `preAuth` → `originLogin` → `postAuth` go through, the game enters the online co-op menu and reaches `createGame` | The co-op menu item opens without a connection error |
| 3. Game session | Host: done on 2026-11-24 (see "Check on the emulator" in [protocol.md](../research/protocol.md)); the room is created, the game runs, quitting with a save is clean. Guest: joining a public room through quick match works in game over VPN on 2026-11-25 (see "Second player" in [protocol.md](../research/protocol.md)). An EA App invite into a private room also works in game (2026-11-25) | Both players in one session, P2P UDP visible between 26.x addresses |
| 4. Stabilization | Handling disconnects, reconnecting, returning to the menu cleanly. Done: a guest whose mesh link does not come up within `JOIN_TIMEOUT` (`blaze.rs`) is removed with `PLAYER_JOIN_TIMEOUT`, so a failed join no longer keeps the room full | Several chapters in a row without drops |
| 5. Convenience | One-click start: emulator + `hosts` edit + firewall rules | The second player is set up in a minute |

Stages 1–3 are done with our own server, built on the message layouts of Pocket Relay Server and the definitions from Aim4kill/BlazeSDK (see [architecture.md](architecture.md)).

## Risks and open questions

1. **SSL bypass.** Resolved at stage 1: the game accepts the certificate from `blaze-ssl-async`, no ProtoSSL patch is needed.
2. **`fetchClientConfig` response.** Resolved at stage 2: the game requests only `BlazeSDK` (in `preAuth`) and `MessageOfTheDay`, and empty responses are enough.
3. **Game attributes and matchmaking.** DS3's matchmaking rules (chapter, difficulty, DLC) are unknown; at first the emulator may pair any two sessions.
4. **Initial data.** It is worth checking in advance that the game does not require progress or inventory from Blaze (`Locker`, `userSettingsLoadAll`): if it does, empty responses could damage local saves. Back up your saves before testing.
5. **Firewall.** Radmin VPN often lands in the "Public network" profile, and Windows blocks inbound UDP. A rule is needed for `deadspace3.exe` and for the emulator. `net-check` shows the network profile and the firewall state ([network-checks.md](../guides/network-checks.md)).
6. **EA App updates.** They do not affect Blaze, but launching the game still depends on the EA App (DRM and the Origin SDK).
