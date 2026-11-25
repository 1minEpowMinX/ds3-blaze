# Documentation

Subject: Dead Space 3 for PC (Steam version, launched through the EA App). Analysis started on 2026-11-24.

## Research

What the game and EA's servers do, as observed.

| File | Contents |
|---|---|
| [game.md](research/game.md) | What the exe reveals: BlazeSDK version, trusted SSL roots, the P2P network stack, code encryption |
| [captures.md](research/captures.md) | Traffic captures against EA's servers: login, QoS, the player's NAT, demangler, UDP between players |
| [protocol.md](research/protocol.md) | What the game requires from a Blaze server, stage by stage: redirector, login, host's room, second player |

## Design

How ds3-blaze answers that research, and in what stages.

| File | Contents |
|---|---|
| [architecture.md](design/architecture.md) | Layout with the emulator and a VPN, existing work, sources |
| [plan.md](design/plan.md) | Work stages and their status, risks and open questions |
| [gui.md](design/gui.md) | The players' window, the workspace split and the command line that serves tests |

## Guides

| File | Contents |
|---|---|
| [network-checks.md](guides/network-checks.md) | Checking the network between players before launching the game |

## Images

| File | Contents |
|---|---|
| [how-it-works.svg](images/how-it-works.svg) | Official co-op compared with ds3-blaze, shown at the top of the README and used for the Nexus gallery |

## Publishing

| File | Contents |
|---|---|
| [nexus/](nexus/description.bbcode.txt) | Text of the Nexus Mods page, in BBCode. It repeats the main path of "Running" in the top-level README, so a change to those commands goes into both |
