# Architecture


```
 Player A (host)                               Player B
 ┌───────────────────────────┐                 ┌──────────────────────┐
 │ deadspace3.exe            │                 │ deadspace3.exe       │
 │  └ hosts:                 │                 │  └ hosts:            │
 │    gosredirector → 26.x.A │                 │    gosredirector →   │
 │ ds3-blaze (emulator)      │◄── Blaze/TCP ───┤    26.x.A            │
 │  :42127 redirector+SSLv3  │    over VPN     │                      │
 │  :Blaze main, :QoS HTTP   │                 │                      │
 └────────────┬──────────────┘                 └──────────┬───────────┘
              └──────────── P2P UDP (ProtoTunnel) ────────┘
                         directly over Radmin VPN 26.x.x.x
```

- The emulator runs on the host's PC (or on any PC in the same VPN). No external VPS is needed.
- For each client's `NetworkAddress`, the emulator substitutes the address the TCP connection came from (that is, the VPN address) as both the internal and the external one. This guarantees that P2P goes through the VPN, and EA's demangler is not needed.
- The emulator accepts any Origin token and never asks EA to verify it. The player's name is set by the `--name` and `--player IP=NAME` options.
- The emulator serves QoS itself (HTTP and UDP echo) in the format from capture #2 ([captures.md](captures.md)), giving its own address. EA's demangler is not redirected: the peers' addresses are VPN addresses anyway, and in game P2P is established without it.
- The game's UDP port 3659 must be open for inbound traffic on the VPN adapter of both players; `p2p-check` verifies this ([network-checks.md](network-checks.md)).
- The guest gets into the host's room through quick match (a public room) or through an EA App invite (a private room). Both paths are confirmed in game; details are in the "Second player" section of [protocol.md](protocol.md). The emulator keeps no game lists.

## Existing work

| Project | What it provides | Relevance to DS3 |
|---|---|---|
| [PocketRelay/Server](https://github.com/PocketRelay/Server) (Rust) | A complete ME3 Blaze emulator: redirector, SSLv3, TDF, GameManager, matchmaking, QoS, LAN/WAN operation, optional Origin token verification with EA | Source of message and component layouts; the server is written anew for DS3, and borrowings are credited in the source file headers |
| [jacobtread/blaze-ssl-async](https://github.com/jacobtread/blaze-ssl-async) | SSLv3 (RC4-SHA/MD5) and a bypass certificate for old ProtoSSL | Used as a dependency; DS3 accepts the bypass certificate |
| [PocketRelay/Client](https://github.com/PocketRelay/Client) | Redirects the ME3 client to a custom server; `/legacy` holds the certificate check patch from [masseffect-binkw32](https://github.com/Erik-JS/masseffect-binkw32) | Not needed: a `hosts` edit is enough for DS3 |
| [Aim4kill/BlazeSDK](https://github.com/Aim4kill/BlazeSDK) (C#) | A library for Blaze servers: `Blaze2SDK`, `Blaze3SDK`, `EATDF`, `ProtoFire`, a sample Blaze 3 redirector | Source of Blaze 3 command and TDF definitions; command numbers match the strings in the DS3 exe |
| [PrivateServerEmulator/ME3PSE](https://github.com/PrivateServerEmulator/ME3PSE) (C#) | An old ME3 emulator | For reference |

No ready-made emulator for Dead Space 3 could be found. The official DS3 servers appear to still be up (unlike DS2, shut down on 2023-12-08), but players widely complain about `Failed to join game`, and discussions recommend Hamachi and public sessions instead of private ones.

## Sources

- [PocketRelay](https://github.com/PocketRelay), [blaze-ssl-async](https://github.com/jacobtread/blaze-ssl-async), [Aim4kill/BlazeSDK](https://github.com/Aim4kill/BlazeSDK), [ME3PSE](https://github.com/PrivateServerEmulator/ME3PSE)
- [EA Forums: Dead Space 3 (PC) Failed To Join Game](https://forums.ea.com/discussions/dead-space-franchise-discussion-en/dead-space-3-pc-failed-to-join-game-error/7229623)
- [Steam: Does coop still work?](https://steamcommunity.com/app/1238060/discussions/0/634541622122515974/)
- [PC Gamer: EA closing servers incl. Dead Space 2](https://www.pcgamer.com/electronic-arts-is-closing-the-servers-for-even-more-old-games-including-dead-space-2-crysis-3-and-mirrors-edge-catalyst/)
