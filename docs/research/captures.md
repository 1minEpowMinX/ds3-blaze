# Traffic captures against EA's servers

## Capture #1 (2026-11-24, `DS3_Net_Packages\Capture#1.pcapng`)

The file holds 106 packets: only DNS and one TCP connection to the redirector (it looks as if it was saved with the filter `dns || tcp.port == 42127`). The online login sequence:

| Time, s | Event |
|---|---|
| 0–52 | EA App traffic (`*.juno.ea.com`, `accounts.ea.com`, `friends.gs.ea.com`, `gateway.ea.com`): authorization, friends, cloud saves. Unrelated to Blaze |
| 65.2–67.5 | Three DNS queries `gosredirector.ea.com` → `159.153.51.18` |
| 67.6–71.9 | TCP to `159.153.51.18:42127`: three SYN retries, the SYN-ACK arrives after 4.3 s |
| 71.9–77.4 | SSLv3 handshake and one request/response exchange (245 and 117 bytes of payload), then the client closes the connection |
| 77.4 | DNS `gsprodblapp-03.ea.com` → `159.153.73.13`: the main Blaze server from the redirector's response |
| 78.0 | DNS for four QoS servers: `gosnrtprod-qos01`, `gosgvaprod-qos01`, `gosiadprod-qos01`, `gossjcprod-qos01` (`*.ea.com`). So the login to the main server succeeded and the client received the QoS configuration |
| 89.1 | DNS `gos.ea.com` → `159.153.71.19` (probably telemetry or the ticker from `postAuth`) |

SSL parameters from the handshake:

- Protocol SSL 3.0. The client offers `TLS_RSA_WITH_RC4_128_SHA`, `TLS_RSA_WITH_RC4_128_MD5`, `TLS_RSA_WITH_AES_128_CBC_SHA`, `TLS_RSA_WITH_AES_256_CBC_SHA`; EA's server picks AES-256. ProtoSSL here is newer than in ME3 (which has only RC4), but RC4 is still in the list, so `blaze-ssl-async` fits without changes to its ciphers.
- Server certificate: `CN=gosredirector.ea.com, OU=Global Online Studio`, issued by `OTG3 Certificate Authority` (`dirtysock-contact@ea.com`), signature `md5WithRSAEncryption`, valid from 2015-05-07 to 2035-05-02, serial number `0x03C2`. A single-link chain; the OTG3 root is built into the game.

A side observation: establishing the connection to the official redirector took about 4 s because of lost SYN/SYN-ACK packets. This directly confirms that EA's infrastructure is unstable from the very first step.

The content of the redirector's Blaze messages is encrypted (RSA key exchange) and cannot be decrypted without EA's server private key. The request structure is known from ME3 (`getServerInstance` with `deadspace-3-pc`); our own redirector will confirm it.

## Capture #2 (2026-11-24, `DS3_Net_Packages\Capture#2.pcapng`, unfiltered)

Scenario: online login, matchmaking into a random host's game, kick after about 20 s. Unrelated traffic (RDP from `192.168.1.3`, Steam, Teredo, EA App over 443) is excluded. DirtySDK version from the `User-Agent` header: `ProtoHttp 1.3/DS 8.18.8.0`.

| Time, s | Event |
|---|---|
| 44.6–46.4 | UPnP: SSDP discovery, `GetExternalIPAddress` → the router answers **`0.0.0.0`**; then `GetSpecificPortMappingEntry`, `DeletePortMapping`, `AddPortMapping` UDP 3659 → `192.168.1.4:3659`, description `EA Tunnel`, lease 14400 s |
| 46.0 | Redirector `159.153.51.18:42127` (SSLv3) |
| 47.3 | Main Blaze: `gsprodblapp-03.ea.com` = **`159.153.73.13:10323`, also SSLv3**. Blaze messages are encrypted and unreadable in the capture |
| 47.8–55.2 | QoS (plain HTTP and UDP, see below) |
| 55.9–57.0 | `http://gos.ea.com/easo/editorial/Deadspace/3/MessageOfDay/pc/mod_ru.txt`, `mod_mp_ru.txt`, `mod_onlinepass_ru.txt`: messages of the day, plain HTTP |
| 66.6 | A 743-byte client request (matchmaking or joining a game), a server response of about 3 KB (by its size it looks like `NotifyGameSetup` with the host's data) |
| 66.8–86.8 | UDP `192.168.1.4:3659` → `<host IP>:3659` (the host): 40 packets, **not a single reply** |
| 76.9–78.4 | Demangler (ProtoMangle) over plain HTTP, see below. Outcome: `status=success`, `peerIP=<host IP>`, `peerPort=3659`, yet there is still no connection |
| 86.6 | A new exchange with Blaze, followed by the kick |

### QoS

Ports: HTTP `17502` (TCP), echo probes `17499` (UDP), firewall check `17500`, `17501` (UDP). Responses are XML with the headers `X-BLAZE-COMPONENT: qos`, `X-BLAZE-COMMAND: qos|firewall|firetype`, `X-BLAZE-SEQNO`:

```
GET /qos/qos?vers=1&qtyp=1&prpt=3659       → <qos><numprobes>0</numprobes><qosport>17499</qosport><probesize>0</probesize><qosip>…</qosip><requestid>1</requestid><reqsecret>0</reqsecret></qos>
GET /qos/qos?vers=1&qtyp=2&prpt=3659       → numprobes=10, probesize=1200, requestid=965, reqsecret=867 (bandwidth measurement)
GET /qos/firewall?vers=1&nint=2            → <firewall><ips><ips>…</ips>×2</ips><numinterfaces>2</numinterfaces><ports><ports>17500</ports><ports>17501</ports></ports><requestid>1437</requestid><reqsecret>406</reqsecret></firewall>
GET /qos/firetype?vers=1&rqid=1437&rqsc=406&inip=-1062731516&inpt=3659 → <firetype><firetype>4</firetype></firetype>
```

`qosip` and `inip` are sent as 32-bit integers (`inip=-1062731516` = `192.168.1.4`).

State of EA's QoS servers: of the four sites, only two respond (`gosiadprod-qos01` `159.153.52.14` and `gossjcprod-qos01` `162.244.53.174`). `gosnrtprod-qos01` and `gosgvaprod-qos01` do not answer SYN. The probes to `17500`/`17501` got no reply, so the firewall test did not finish and the NAT type came out as `4` (by the order of the Blaze enum, probably `NAT_TYPE_UNKNOWN`).

### The player's network: symmetric NAT behind the ISP

The QoS UDP replies (38 bytes) contain the client's external address as the server sees it:

| QoS server | Client's external address |
|---|---|
| `159.153.52.14` | `<player's external IP>:22937` |
| `162.244.53.174` | `<player's external IP>:9653` |

The same local port 3659 maps to different external ports for different servers. This is symmetric NAT (the mapping depends on the destination). Together with `NewExternalIPAddress = 0.0.0.0` from the router, it is almost certainly the ISP's CGNAT: a UPnP port mapping opens the port only on the home router and never reaches the internet. ProtoMangle's method (guessing the port) cannot get through such a NAT, which is exactly what the capture shows: the host did not receive a single packet, and after 20 s the game kicked the client.

This is the main cause of the connection problems. The VPN layout removes it completely, because UDP travels inside the Radmin tunnel and the ISP's NAT does not interfere with the tunnel.

### Demangler (ProtoMangle)

Plain HTTP on `demangler.ea.com:3658` (CNAME `demangler.prod.legacy.gs.ea.com`), cookie `sessionID=<session id>`:

```
GET /getPeerAddress?myIP=192.168.1.4&myPort=3659&version=1.0
→ status=probe / targetIP-N, targetPort-N, sourcePort-N, tag-N=probe1|probe2|probe3.x|prober:<id>:0, sendCount-N
→ … (6 rounds, UDP probes to 3.83.166.41:10000 and 54.152.64.208:10000 from different source ports)
→ status=success / peerIP=<host IP> / peerPort=3659
```

The protocol is simple and text-based, so it is easy to emulate if needed: answer `status=success` with the peer's VPN address right away.

### UDP between players

Game port: **3659** (on both ends: locally and on the host). Two packet types:

- 74 bytes, every 0.5 s: a header, then `434f6500`, the session id (the same as in the demangler cookie), the string `$0401a8c0` (the internal IP `192.168.1.4` in hex, reversed byte order) and `-1554058313`. By its format this is a ProtoTunnel/NetGameLink connection request.
- 24 bytes, every 1 s: encrypted (Blaze hands out the tunnel key).
