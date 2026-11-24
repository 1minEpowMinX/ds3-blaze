# The game: what the exe reveals

Analyzed on 2026-11-24. Subject: `D:\Games\Steam\steamapps\common\Dead Space 3\deadspace3.exe` (Steam version, launched through the EA App, AppID 1238060).

## Protocol: yes, it is Blaze

Strings from the `.rdata` section (which is not encrypted):

| Indicator | Value |
|---|---|
| SDK version | `BlazeSDK 3.19.07.0`, built `Nov 27 2012` (path `C:\ears\DL\006\packages\BlazeSDK\3.19.07.0\...`) |
| Service name for the redirector | `deadspace-3-pc` |
| Redirector | `gosredirector.online.ea.com`, `gosredirector.ea.com` (`.scert`/`.stest` for test environments) |
| Authentication | `Blaze::Authentication::OriginLoginRequest` (`originLogin`); the token comes from the EA App through the Origin SDK (LSX, `127.0.0.1`) |
| Components | `Authentication`, `Util` (`preAuth`, `postAuth`, `fetchClientConfig`, `userSettings*`, `getTelemetryServer`, `getTickerServer`), `UserSessions` (`updateNetworkInfo`, `lookupUser*`), `GameManager` (`createGame`, `joinGame`, `startMatchmaking`, `advanceGameState`, `updateMeshConnection`, `getGameListSnapshot`), `Locker`, `Playgroups`, stats/leaderboards |
| QoS | HTTP pings `http://%s:%u/qos/qos`, `/qos/firetype`, `/qos/firewall` |

The command set and command names match Mass Effect 3 (BlazeSDK 3.15.x, also from 2012). This means the TDF codec and packet format from the ME3 emulators should fit without rework.

Checked on 2026-11-24: `gosredirector.ea.com` resolves to `159.153.51.18`, port `42127` is open. The port is not found in the binary itself because the code is encrypted (see below), but 42127 is the standard port for Blaze 3.

## SSL: the same trusted CA as in ME3

The built-in table of trusted roots in DirtySock ProtoSSL (file offset around `0xB31300`):

- `OTG3 Certificate Authority` (Electronic Arts, Online Technology Group)
- `GOS 2011 Certificate Authority` (Global Online Studio)
- `VeriSign Class 3 Secure Server CA`

`OTG3` is the very root that the bypass certificate in `blaze-ssl-async` (Pocket Relay for ME3) targets: "certificate built to bypass verification of older EA ProtoSSL implementations". DirtySock in DS3 is of the same generation, so the bypass is likely to work without a client patch. This is the first thing to verify by experiment.

## Game traffic is P2P; Blaze only brings players together

The binary contains the whole DirtySock network stack: `netgamelink`, `commudp-global`, `prototunnel-tunnel` (`EA Tunnel`), UPnP (`AddPortMapping`, the `-noupnp` flag), `demangler.ea.com` (ProtoMangle, NAT traversal through an EA server), `peerport=`, as well as the topology enums `PEER_TO_PEER_FULL_MESH`, `PEER_TO_PEER_PARTIAL_MESH`, `PEER_TO_PEER_DIRTYCAST_FAILOVER` and the NAT types `NAT_TYPE_OPEN/MODERATE/STRICT`.

How it works:

1. The client connects to Blaze (through the redirector) and logs in with an Origin token.
2. The host creates a game (`createGame`); the second player finds it (matchmaking, the friends' game list or an EA App invite) and sends `joinGame`.
3. Blaze sends both of them `NotifyGameSetup` with each other's network addresses (internal and external IP:port).
4. From then on the players talk directly over UDP through ProtoTunnel; if that fails, they try to get through with EA's demangler.

Steps 3 and 4 are exactly where it breaks for most players: the addresses come from EA's QoS, and NAT traversal depends on the demangler. If the Blaze server is our own and hands out addresses from a virtual network (Radmin VPN, ZeroTier, Tailscale), the NAT problem disappears entirely: there is nothing to traverse.

## The executable is encrypted

The `.text` section has an entropy of 8.0, and the entry point lies in an unnamed seventh section (the EA DRM stub, `Core\Activation.dll`). Ghidra needs a process dump taken after decryption (x64dbg + Scylla, or a memory dump of the running game). Statically, only `.rdata`, `.data` and `.rsrc` are available. This also constrains patches: code bytes cannot be changed in the file, only in memory after startup, through a proxy DLL (the game imports `VERSION.dll`, `DINPUT8.dll`, `WSOCK32.dll`, `d3dx9_43.dll`; there is no `binkw32.dll` as in ME3).
