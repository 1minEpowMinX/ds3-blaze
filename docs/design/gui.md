# GUI and workspace split

Design agreed on 2026-11-26. It turns the test-era command line into a window that an ordinary player can follow, and splits the crate so that the window and the command line share one core.

## Goal

A player today has to find their own VPN address, type the other player's address into `--player`, run `hosts.ps1` from an elevated PowerShell, run `p2p-check` at the same moment as the other player, and remember `hosts.ps1 -Disable` afterwards. The GUI removes each of these steps or does it for the player. The command line stays for tests and troubleshooting.

## Workspace

`ds3-blaze/` becomes a Cargo workspace:

| Crate | Kind | Contents |
|---|---|---|
| `ds3-blaze-core` | lib | `fire`, `components`, `models`, `game`, `blaze`, `redirector`, `qos`, `log`, and the new `events`, `hosts`, `vpn` |
| `ds3-blaze-diag` | lib | `netcheck`, `p2p`, the new `reach`; the protocol test that replaces `selftest` lives in its tests |
| `ds3-blaze-cli` | bin `ds3-blaze-cli.exe` | argument parsing with `clap` derive, console output; `proxy` and `ea-check` behind the feature `research` |
| `ds3-blaze-gui` | bin `ds3-blaze.exe` | the window, built with `eframe` |

The players' executable is the GUI, so it takes the name players already know, `ds3-blaze.exe`. The GUI is built with `windows_subsystem = "windows"` and has no console, which is why the command line is a separate executable rather than a mode of the same one. Renaming the command line breaks existing scripts and instructions: the commit that does it is marked `feat!` with a `BREAKING CHANGE:` footer.

## Core

### Server lifecycle

`Server::start(config)` binds the redirector, Blaze and QoS listeners and returns a `ServerHandle`. The handle stops all three through a `CancellationToken`, exposes the event stream and a state snapshot, and owns the `PlayerBook`. The configuration carries every port, so a test can start a server on free ports inside its own process. The defaults stay the named constants in the core (`REDIRECTOR_PORT` and the QoS and Blaze port constants that `main.rs` holds today).

### State and events

The GUI renders from `ServerHandle::snapshot()`: sessions with address, name and login state, refused addresses, and rooms with their state, members and the chapter and restore point read from `GAME_CHECKPOINT`. The snapshot is the source of truth for what the window shows, so a missed event never leaves the window wrong.

Events go out on a `tokio::sync::broadcast` channel, one per process like the log file. They carry log lines (`Event::Log` with the time, tag and text) and notices that state changed, which make the GUI repaint. `log!` keeps writing the session file under `logs/`; the core no longer prints to stdout. The CLI subscribes to the channel and prints log lines, which keeps today's console output.

### Player book

`PlayerBook` holds the names by address, the allowed addresses and the "only allowed players" switch behind a `RwLock`. It replaces `Config::player_name`, `player_names` and `only_listed`, so renaming a player or allowing a refused address takes effect for the next connection without restarting the server. `Config::admits` and `Config::name_for` move onto it with their current rules.

### hosts

The `hosts` module carries the behaviour of `scripts/hosts.ps1` over to Rust: the section between `# Added by ds3-blaze` and `# End of ds3-blaze section`, removal of lines ending in the legacy marker, refusal when the section has no end line, the file's own encoding (a byte order mark if present, the ANSI code page otherwise) and line endings, the backup `hosts.ds3-blaze.bak` written without any ds3-blaze entries, and a DNS cache flush afterwards.

The text transformation is a pure function, `rewrite(file, action)`, which takes the file's bytes and returns the updated file and the backup, so it is tested without touching the system file. Reading the current state (`EaServers`, `Redirected(address)`, `Broken`) needs no rights; writing needs an elevated process.

Windows uses the first entry it finds for a name, so an entry for a redirected name outside the section, written by hand or by another program, wins over the section when it comes before it and takes the game elsewhere. The state reports such an entry and its line, so the GUI and `hosts status` can say which line to remove; the module never removes it, since it is not ds3-blaze's. The script is deleted once the module and the CLI command below replace it.

### VPN adapters

The `vpn` module lists IPv4 adapters through `GetAdaptersAddresses`, with the same VPN detection `netcheck` uses today (`is_vpn`). The start screen shows the list at once, without the few seconds a PowerShell call takes.

## Diagnostics

### Network findings and fixes

`netcheck` returns `Vec<Finding { severity, text, fix }>` instead of printing. It is split in two: a local part (VPN adapter present, other VPN adapters that are up, its network profile, firewall rules for the game and for ds3-blaze), which needs no peer, and a route part for a given peer address. Both work on parsed records, apart from the PowerShell call that reads them, so their rules have unit tests without the machine's network state.

### Route to the peer

The route part takes the peer's address and the player's own VPN address, the one chosen on the start screen. Its checks are today's `check_route`, tied to the chosen address:

- Windows has a route to the peer.
- The route leaves through the adapter that holds the chosen address, and the peer is on that adapter's network, with no gateway in between. The comparison is with the chosen adapter rather than with any adapter whose name looks like a VPN (`is_vpn`), so another tunnel that takes over the peer's addresses is caught whatever its adapter is called.
- A UDP socket connected to the peer picks the chosen address as its source.
- Other VPN adapters that are up are listed: as a note when the route passes, as the likely cause when it does not.

The route part sees only routing. A VPN client that drops traffic with filters while the routes stay intact (a kill switch) shows only in the exchange, so a `p2p` run that gets no answer while another VPN adapter is up names that adapter as the likely cause.

`fix` names an action the GUI can offer as a button: `Fix::GameRule` (inbound UDP on `GAME_PORT`), `Fix::ServerRule` (inbound for the executable that runs the server, needed on the host) and `Fix::PrivateProfile` (the VPN adapter's network set to Private). Each is carried out by the CLI command `fix`, which the GUI starts elevated. After a fix the GUI runs the local part again, and the new findings show whether it worked.

### P2P check

`p2p` is today's `p2pcheck` exchange with its output turned into events: both sides ping each other's known address on `GAME_PORT` and answer the other's pings, as the game does. Both players run the same exchange, so the GUI adds no protocol of its own and the result means what `p2p-check` means today.

The two runs start at the same moment without the players arranging it, because the guest's `reach` is the signal:

- The guest's window starts the exchange with the host as soon as `reach` passes.
- The host's window starts it with an address the first time a Blaze connection arrives from that address, which is the guest's `reach` a moment earlier. Refused connections count too, so the check runs while the host decides whether to allow the guest.

Each run lasts at most `DEFAULT_SECONDS`, the existing limit, and the row counts it down. Neither window starts it again by itself; "Check again" runs it on demand, and the other player presses theirs too.

The exchange binds `GAME_PORT`, which the game holds while it runs. A failed bind is reported as "close the game to run the check", which the bind error already says today, so the GUI needs no knowledge of the game's process.

### Server reachability

`reach` is the guest's check of the host's server. It resolves `gosredirector.ea.com`, which confirms that the hosts entry is in effect, and compares the result with the host's address; asks the redirector for the server instance over SSLv3; sends one `preAuth` to the Blaze server; and queries QoS over HTTP and UDP. It creates no room and starts no matchmaking, so it is safe against a host who is already playing.

The expected hosts address and the server address are separate parameters. The guest expects the host's address and checks the host's server; the host expects 127.0.0.1 and checks its own server through its VPN address, which shows that the listeners answer on the VPN interface.

### Protocol test

The full client scenario of today's `selftest` (login, room creation, the second client's join, matchmaking timeout, QoS) becomes an integration test of `ds3-blaze-diag`. It starts the server with `Server::start` on free ports in the test process. The `selftest` command goes away: against a live host its matchmaking would join the host's real public room, and its two logins break invite matching, which works only while exactly two players are logged in.

## GUI

### Start screen

The start screen asks for the player's name and VPN address (from `vpn`, VPN adapters first) and offers two modes: host a game, or join a friend. Every other VPN adapter that is up is marked, with a note that another VPN can take over the other player's traffic. It shows the state of the hosts file. When ds3-blaze entries are present while no server or connection is active, as after a crash, it offers to restore the EA servers.

### Step rows

Both modes are built from one widget: a row with a step marker, a title, a one-line summary, a status pill and a chevron, laid out left to right like a table row. A click expands the row to its details, where a warning lists its findings with their fix buttons. Rows stay as the player left them: the GUI does not expand or collapse them by itself.

A mode is a list of steps. Each step owns its state (waiting, running, ok, warning, failed), its summary, its pill and its expanded view; the rendering only reads them. The step logic therefore has unit tests without a window.

The summary carries the information a player watches during play, so a collapsed row is enough: "Carver connected · 1 waiting", "Public · 2/2 · Chapter 4".

### Host steps

| Step | Summary when done | Expanded |
|---|---|---|
| Your VPN address | address and adapter; "Start server" | the local part's findings with fixes |
| Point game at this PC | hosts → 127.0.0.1 | the entry, the backup, "Restore EA servers" |
| Server | listening on the VPN address | the three listeners; `reach` against the VPN address |
| Players | who is connected, how many are waiting | names editable in place, "Allow" for refused addresses, "Only players I allow" |
| Direct link to the guest | both directions and latency | the route part for the guest's address, the exchange, findings with fixes, "Check again" |
| Room | visibility, players, chapter | chapter and restore point, how the guest joined; before a room exists, the step tells the host to start the game and create one |
| Log | the latest line | recent lines, "Open logs folder" |

"Stop and restore hosts" stops the server and removes the hosts entry.

### Guest steps

| Step | Summary when done | Expanded |
|---|---|---|
| Host's VPN address | the address and the adapter the route takes; "Connect" | the local part and the route part for the host's address, with fixes |
| Point game at host | hosts → the host's address | the entry, the backup, "Restore EA servers" |
| Reach host's server | all services answer | each `reach` stage with its result |
| Direct link to host | both directions and latency | the exchange, findings with fixes, "Check again" |
| Start Dead Space 3 | online co-op | quick match for a public game, the EA App invite for a private one |

"Disconnect and restore hosts" removes the hosts entry.

### Order of the steps

The guest's "Connect" runs the route part before the hosts file changes. A route problem stops the steps after it, since the hosts entry, `reach` and the exchange would all go through the same wrong route; the row says what to change and "Connect" runs it again. The host learns the guest's address only from the guest's first connection, so the host's route part runs alongside the exchange that connection starts, and the exchange still begins when the guest's does.

The direct link step comes before the game step in both modes because the check needs `GAME_PORT`. While it runs, the row says not to start the game until the countdown ends. The host's room step says the same until the check with the guest has finished.

When the host's game is already running as the guest connects, the host's run cannot bind the port and the guest's pings go unanswered. The guest's row then says that a host whose game is running closes it, and both press "Check again".

The GUI does not block the game from starting during a check. A passing run ends `LINGER` after both directions work, so the window is a few seconds, and the ways to block a launch (an `Image File Execution Options` debugger entry, or ending the game's process) need administrator rights, look like malware to antivirus software, and leave the game unable to start if the window crashes. How the game behaves when it starts while a check holds `GAME_PORT` has not been observed; a check in game decides whether the row's warning needs to be stronger.

### Elevation

Writing the hosts file and applying fixes need administrator rights; the GUI itself runs without them, so the server never runs elevated. The GUI starts `ds3-blaze-cli.exe` next to itself through `ShellExecuteExW` with the verb `runas` (one UAC prompt per action), waits for it, and then checks the outcome by reading the state again: the hosts state needs no rights, and the network findings come from the local check.

### Settings

`ds3-blaze.toml` next to the executable keeps the player's name, the last mode, the last host address and the names of known players. The archive is meant to be unpacked into a folder of its own, where the logs already go.

### Closing

Closing the window while the hosts file points at ds3-blaze asks whether to restore the EA servers, with restoring as the default.

## Command line

`ds3-blaze-cli` uses `clap` derive, so the help and the option errors come from the code:

| Command | Purpose |
|---|---|
| `serve` | the server, with today's options |
| `hosts enable --address IP`, `hosts disable`, `hosts status` | the hosts entry; `enable` and `disable` need an elevated console |
| `fix game-rule`, `fix server-rule`, `fix network-profile` | the network fixes the GUI offers |
| `net-check --peer IP [--address IP]` | the network findings; `--address` is the player's own VPN address, and without it the route is checked against any adapter `is_vpn` recognises |
| `p2p-check --peer IP [--address IP]` | the P2P exchange, after the same findings |
| `reach --host IP` | the guest's server check |
| `proxy`, `ea-check` | research only, built with the feature `research` |

The GUI's settings file does not affect the command line, which stays predictable for tests.

## Packaging and CI

- CI runs from the workspace root: `cargo fmt --check`, `cargo clippy --workspace --all-targets --all-features -D warnings` (so the `research` code keeps compiling), `cargo build`, `cargo test --workspace`. The step that starts `serve` and runs `selftest` from PowerShell is replaced by the protocol test.
- `tools/package.ps1` packs both executables, the README and the license. `scripts/hosts.ps1` leaves the archive and the repository. Release builds leave out `research`.

## Documentation roles

The GUI owns the procedure: its step rows say what to do and how to fix what is wrong. The documents stop retelling it.

| Document | Holds | Leaves out |
|---|---|---|
| Nexus page | what the mod is and why, requirements, a short start ("run `ds3-blaze.exe`, choose a mode, follow the steps"), going back to EA, limitations, log privacy | commands, options, ports |
| README | the same short start, the command line with a pointer to `ds3-blaze-cli --help`, the login token warning, development | the option list, which `clap` generates |
| [network-checks.md](../guides/network-checks.md) | manual checks with the command line for when the GUI is not enough | |

The README and the Nexus page still share the few sentences of the short start, because the Nexus page has to stand on its own. The note in [docs/README.md](../README.md) about keeping the two in step stays, now covering only those sentences. The limitation "names in game come from `--name` and `--player`" changes to the start screen and the host's players table.

## Tests

- `hosts::rewrite`: with and without a byte order mark, ANSI text, CRLF and LF, an empty file, a file without a final line break, legacy-marker lines, a section without an end line, repeated `enable`.
- `hosts::state`: an entry for a redirected name outside the section, before the section and after it, and a commented one.
- `PlayerBook`: admission and naming rules as `Config::admits` and `Config::name_for` have them now, and changes while sessions are open.
- The route part on parsed records: no route; the route through an adapter other than the chosen one, including one that `is_vpn` recognises; a gateway in between; a source address other than the chosen one; another VPN adapter up beside a passing route.
- `p2p`: two exchanges on the loopback address with different ports, started a moment apart; one exchange whose port is already taken; the failure text of an exchange without answers, with and without another VPN adapter up.
- The host's trigger: a first connection from an address starts one run, a second connection from it starts none, a refused connection starts one.
- `reach` against a server started in the test.
- The protocol test described above.
- The steps of each mode as state machines, without a window.

## Stages

Each stage is a branch of its own, merged into `main` when CI passes. A stage that removes or changes a command also updates the README, the Nexus page and the guides that use it, so `main` never documents a command that is gone. Until stage 6 the command line keeps the name `ds3-blaze.exe`.

1. Workspace split with no change in behaviour: the crates, the moved modules, CI and `package.ps1` pointed at the workspace.
2. `Server::start` with ports in the configuration, the protocol test, the `selftest` command and its CI step removed.
3. Events, snapshot and `PlayerBook`; the core stops printing and the CLI prints the events.
4. `clap`, the `hosts` module and `hosts` command, `scripts/hosts.ps1` removed.
5. Diagnostics: findings and fixes, the route part tied to the chosen address, the `fix` command, `p2p` events, `reach`, and the hosts state reporting entries for the redirector outside the section.
6. The GUI, the guest's nickname sent to the host, and the renaming of the executables.
7. README, Nexus page, network-checks and the docs index rewritten for the GUI.

## Guest nickname

The guest tells the host its own nickname, so the host no longer has to type it. The way the guest sends it (for example a request to the host's QoS HTTP server) and how the host confirms it are still to be thought through; they are settled at the start of stage 6, which builds them with the GUI. The same channel would let the guest's window show the name the host sees, which `reach` cannot learn without a full login.
