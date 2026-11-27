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

`PlayerBook` holds the names by address, the allowed addresses and the "only allowed players" switch behind a `RwLock`. It replaces `Config::player_name`, `player_names` and `only_listed`, so renaming a player or allowing a refused address takes effect for the next connection without restarting the server. `Config::admits` and `Config::name_for` move onto it with their current rules. From stage 6 it also holds the names guests send themselves, below the names the host sets (see "Guest nickname").

### hosts

The `hosts` module carries the behaviour of `scripts/hosts.ps1` over to Rust: the section between `# Added by ds3-blaze` and `# End of ds3-blaze section`, removal of lines ending in the legacy marker, refusal when the section has no end line, the file's own encoding (a byte order mark if present, the ANSI code page otherwise) and line endings, the backup `hosts.ds3-blaze.bak` written without any ds3-blaze entries, and a DNS cache flush afterwards.

The text transformation is a pure function, `rewrite(file, action)`, which takes the file's bytes and returns the updated file and the backup, so it is tested without touching the system file. Reading the current state (`EaServers`, `Redirected(address)`, `Broken`) needs no rights; writing needs an elevated process.

Windows uses the first entry it finds for a name, so an entry for a redirected name outside the section, written by hand or by another program, wins over the section when it comes before it and takes the game elsewhere. The state reports such an entry and its line, so the GUI and `hosts status` can say which line to remove; the module never removes it, since it is not ds3-blaze's. The script is deleted once the module and the CLI command below replace it.

### VPN adapters

The `vpn` module lists IPv4 adapters through `GetAdaptersAddresses`, with the same VPN detection `netcheck` uses today (`is_vpn`). The start screen shows the list at once, without the few seconds a PowerShell call takes.

## Diagnostics

### Network findings and fixes

`netcheck` returns `Vec<Finding { severity, text, fix }>` instead of printing. It is split in two: a local part (VPN adapter present, other VPN adapters that are up as a note, its network profile, firewall rules for the game and for ds3-blaze), which needs no peer and runs on the host's PC and the guest's alike, and a route part for a given peer address. Both work on parsed records, apart from the PowerShell call that reads them, so their rules have unit tests without the machine's network state.

### Route to the peer

The route part takes the peer's address and the player's own VPN address, the one chosen on the start screen. Its checks are today's `check_route`, tied to the chosen address:

- Windows has a route to the peer.
- The route leaves through the adapter that holds the chosen address, and the peer is on that adapter's network, with no gateway in between. The comparison is with the chosen adapter rather than with any adapter whose name looks like a VPN (`is_vpn`), so another tunnel that takes over the peer's addresses is caught whatever its adapter is called.
- A UDP socket connected to the peer picks the chosen address as its source.
- Other VPN adapters that are up, which the local part already notes, are named again only when the route fails, as the likely cause; a passing route does not repeat them.

On the command line the own address is optional. Without it, any adapter `is_vpn` recognises counts as the player's: the route passes when it leaves through one of them, and a failing route names no other adapter as the cause, since every VPN adapter is a candidate; each one is noted instead as the network the peer's address should be on.

The route part sees only routing. A VPN client that drops traffic with filters while the routes stay intact (a kill switch) shows only in the exchange, so a `p2p` run that gets no answer while another VPN adapter is up names that adapter as the likely cause.

`fix` names an action the GUI can offer as a button, carrying what the action needs: `Fix::GameRule` (inbound UDP on `GAME_PORT`, whatever the game's path), `Fix::ServerRule(path)` (inbound for the executable at `path` that runs the server, needed on the host) and `Fix::PrivateProfile(address)` (the network of the adapter holding `address` set to Private). The firewall rules are named by `fix::rule_name` after one pattern, and a fix replaces a rule of its own name, so running it twice leaves one rule. Each is carried out by the CLI command `fix`, which the GUI starts elevated. After a fix the GUI runs the local part again, and the new findings show whether it worked.

### P2P check

`p2p` is today's `p2pcheck` exchange with its output turned into events: both sides ping each other's known address on `GAME_PORT` and answer the other's pings, as the game does. Both players run the same exchange, so the GUI adds no protocol of its own and the result means what `p2p-check` means today.

The two runs start at the same moment without the players arranging it, because the guest's `reach` is the signal:

- The guest's window starts the exchange with the host as soon as `reach` passes.
- The host's window starts it with an address the first time a Blaze connection arrives from that address, which is the guest's `reach` a moment earlier. Refused connections count too, so the check runs while the host decides whether to allow the guest.

Each run lasts at most `DEFAULT_SECONDS`, the existing limit, and the row counts it down. Neither window starts it again by itself; "Check again" runs it on demand, and the other player presses theirs too.

The exchange binds `GAME_PORT`, which the game holds while it runs. A failed bind is reported as "close the game to run the check", which the bind error already says today, so the GUI needs no knowledge of the game's process.

### Server reachability

`reach` is the guest's check of the host's server. It resolves `gosredirector.ea.com`, which confirms that the hosts entry is in effect, and compares the result with the host's address; asks the redirector for the server instance over SSLv3; sends one `preAuth` to the Blaze server; and queries QoS over HTTP and UDP. It creates no room and starts no matchmaking, so it is safe against a host who is already playing.

The redirector has to hand out the address being checked: a server advertising another address sends the guest's game elsewhere, so that fails the redirector stage, and `preAuth` is then not tried. The other stages connect to the checked address directly and run whatever the name resolves to. Each stage has a timeout of its own (`reach::STAGE_TIMEOUT`), so a packet dropped over the VPN costs seconds rather than the TCP connect timeout.

Given the player's name, `reach` also sends it to the host's server as a last stage, described under "Guest nickname".

The expected hosts address and the server address are separate parameters (`reach --host IP [--expect IP]`, where `--expect` defaults to `--host`). The guest expects the host's address and checks the host's server; the host expects 127.0.0.1 and checks its own server through its VPN address, which shows that the listeners answer on the VPN interface.

### Protocol test

The full client scenario of today's `selftest` (login, room creation, the second client's join, matchmaking timeout, QoS) becomes an integration test of `ds3-blaze-diag`. It starts the server with `Server::start` on free ports in the test process. The `selftest` command goes away: against a live host its matchmaking would join the host's real public room, and its two logins break invite matching, which works only while exactly two players are logged in.

## GUI

### Start screen

The start screen asks for the player's name and VPN address in one card and offers the two modes as large cards below it: host a game, or join a friend.

The player's own VPN address is chosen here once, for both modes. The first address `vpn::choices` returns is taken and shown as one line with its adapter's name and "Change", which opens the list of VPN adapters, and of the adapter holding the chosen address, with the others behind "Show all adapters". Without an adapter to choose, the line asks the player to connect the VPN and offers "Read adapters again". When another VPN adapter is up beside the chosen one, one line under the address says that another VPN can take over the other player's traffic.

A name the rule refuses says why under the name field, and the cards stay disabled. Each card is one button, for the keyboard and screen readers as well as the mouse. When the hosts file holds ds3-blaze entries while no server or connection is active, as after a crash, a notice offers "Restore EA servers"; otherwise the screen says nothing about the hosts file.

### Mode screen

A mode's screen holds, from top to bottom:

- **Header:** Back as an arrow, disabled with its reason while a step runs or the server runs, and "Hosting" or "Joining" with the host's address ("Joining a friend" before one is typed). The window title is "ds3-blaze"; the player's name shows on the start screen and in the host's session only.
- **Banner:** one sentence on where things stand, an optional second line, one main button and a tone (neutral, success, warning, failure). A spinner takes the button's place while a step runs.
- **Setup:** the steps as quiet rows. Once the mode is set up (`setup_done`: every guest step done; the host's server running with the steps before the link done and the link done or waiting for a guest) they collapse into one line, "Setup done", which the player may open; when a step runs, needs a fix or fails, the list opens by itself. What the player opened belongs to that mode's screen: starting, connecting and stopping close every row and the line again, and the other mode starts with all of them closed. Once Stop hosting or Disconnect has restored the EA servers, every step waits again as before the first start; a restore that fails keeps what the run found, since the game still goes to ds3-blaze.
- **Session**, host only: the room, the players and the server log.
- **Footer:** the button that ends the mode's changes, "Stop hosting" or "Disconnect".

### Look

Every screen is one column of cards, centred in the window and no wider than `theme::COLUMN`, on a background darker than the cards. A title in small capitals opens each part: Setup, This game, Players, Server log on a mode's screen; You and Play on the start screen. The setup rows, the players and the room's three values each sit in one card with thin lines between their parts, and an open row's details, the log and the fields sit in wells darker than the card.

Colour carries one meaning each. The accent fills the banner's button, the one thing to press next, marks the shield and a running step, and colours the bar of a neutral banner; nothing else uses it. Green, orange and red mark how a step, a finding or a player stands, and tint the banner with its bar on the left. A done row is plain text, a waiting one weak, so the eye goes to the banner first and then to whatever needs attention. The palette, the type sizes and the spacing are defined once, in `gui/src/ui/theme.rs`.

### Banner

The banner is a pure function of a mode's state (`GuestFlow::banner`, `HostFlow::banner`): it reads the steps, whether the server runs and whether a step is busy, and stores nothing, so it never disagrees with the rows. The first rule that matches wins, which puts a cause before its effect: a firewall fix waits until the host's server answers.

| Guest | Host |
|---|---|
| the hosts entry not in place and nothing failed, as before Connect and after Disconnect: "Join a friend", the host's address field, "Connect" | the direct link checked: the countdown, and not to start the game until the check ends |
| the direct link checked: the countdown, and not to start the game until the check ends | a step running: "Starting the server", with what the step does below |
| a step running: "Connecting to" the host's address, with what the step does below | a step failed: what failed, the step's sentence, "Start server"; a server that runs but fails its own check says friends can't reach it |
| a step failed: what failed, the step's sentence, "Check again" (Connect), and the address field to change a wrong address | server stopped: "Start the server so friends can join", "Start server" |
| findings with fixes: "Almost there", how many things to fix, "Fix it" | an address turned away: "Someone wants to join" with the address and the nickname it sent, "Let in" |
| the direct link failed with nothing to fix: what that means, "Check again" | findings with fixes: as the guest |
| everything done: "Ready: start Dead Space 3 and choose online co-op" | serving, nobody connected: "Waiting for friends", the address, "Copy address" |
| | players connected: "Playing with" their names, "Copy address" |

"Fix it" applies every fix the findings offer in one elevated run of `fix several`, so the player sees one administrator prompt. The banner's button is the only place "Connect", "Start server" and "Fix it" appear; the rows do not repeat it. "Copy address" says "Copied" for a moment after a click, at its own width.

### Step rows

A row is a sign (waiting, running, done, needs a fix, failed), one sentence in the player's words and, when it has details, a chevron. A waiting row says what the step will do; a done row says what is now true. Clicking a row opens its panel: the technical lines (the hosts entry, the listeners, the `reach` stages, the exchange counters), the findings with one button per fix, and the step's buttons such as "Check again". Rows stay as the player left them, apart from the setup list opening by itself. The direct link's row takes the findings of the network check before its own check only when one of them needs a fix, so a row that has not run shows no results.

A mode is a list of steps. Each step owns its state, its sentence and its panel; the rendering only reads them, so the step logic has unit tests without a window.

The crate keeps the two apart by folder. `gui/src/flow` holds the modes' state machines and everything the window shows of them (the banner, the steps, the session, the start screen's form, the closing dialog's state) and never names egui; a test fails if one of its modules does. `gui/src/ui` draws what a flow holds and returns the clicks, changing nothing but the fields it binds. `app.rs` carries the clicks to the flows and the flows' actions to the background work, and owns the I/O: the hosts file, the adapters, the server, the elevated runs.

### Host steps

| Step | Waiting | Done | Panel |
|---|---|---|---|
| Network | Check this PC's network | This PC's network is ready | the adapter and the local findings |
| Game | Point the game at this PC | The game goes to this PC | the hosts entry and the backup |
| Server | Start the server | The server is running | the three listeners; `reach` against the VPN address |
| Direct link, named after the guest once known | Check the direct link to the guest | The direct link works | the local findings and the route part for the guest's address, with fixes; the exchange; "Check again" |

### Host session

| Part | Shows |
|---|---|
| Room | the room's state and how many of its places are taken; before a room exists, "Start Dead Space 3 and create a game"; while the check with the guest runs, to wait before starting the game |
| Progress | the chapter and restore point read from `GAME_CHECKPOINT` |
| Matchmaking | whether the room is open to matchmaking |
| Players | the host's own row; each connected guest with its name and address, Rename opening a field in the name's place, an emptied name returning to the guest's own, and beside a name the host set the guest's own nickname with "Use this name"; each refused address as someone turned away, with the nickname it sent and "Let in"; the switch "Only let in players I approve" |
| Log | the last `SESSION_LOG_LINES` lines, "Open logs folder" |

A refused address has no name to change; Rename comes once it is let in.

### Guest steps

| Step | Waiting | Done | Panel |
|---|---|---|---|
| Network | Check this PC's network | This PC's network is ready | the adapter, the route to the host and the local findings |
| Game | Point the game at the host's PC | The game goes to the host's PC | the hosts entry and the backup |
| Host's server | Ask the host's server | The host's server answers, with the name the host sees | each `reach` stage with its result |
| Direct link | Check the direct link to the host | The direct link works | the local part and the route part for the host's address, with fixes; the exchange; "Check again" |
| Start Dead Space 3 | Start Dead Space 3 and choose online co-op | | quick match for a public game, the EA App invite for a private one |

### Order of the steps

The guest's "Connect" runs the route part before the hosts file changes. A route problem stops the steps after it, since the hosts entry, `reach` and the exchange would all go through the same wrong route; the banner says what to change and "Connect" runs it again. The host learns the guest's address only from the guest's first connection, so the host's route part runs alongside the exchange that connection starts, and the exchange still begins when the guest's does.

The direct link step comes before the game step in both modes because the check needs `GAME_PORT`. While it runs, the guest's banner says not to start the game until the countdown ends, and the host's room tile says the same until the check with the guest has finished.

When the host's game is already running as the guest connects, the host's run cannot bind the port and the guest's pings go unanswered. The guest's banner then says that a host whose game is running closes it, and both press "Check again".

The GUI does not block the game from starting during a check. A passing run ends `LINGER` after both directions work, so the window is a few seconds, and the ways to block a launch (an `Image File Execution Options` debugger entry, or ending the game's process) need administrator rights, look like malware to antivirus software, and leave the game unable to start if the window crashes. How the game behaves when it starts while a check holds `GAME_PORT` has not been observed; a check in game decides whether the banner's warning needs to be stronger.

### Elevation

Writing the hosts file and applying fixes need administrator rights; the GUI itself runs without them, so the server never runs elevated. The GUI starts `ds3-blaze-cli.exe` next to itself through `ShellExecuteExW` with the verb `runas` (one UAC prompt per action), waits for it, and then checks the outcome by reading the state again: the hosts state needs no rights, and the network findings come from the local check.

A button that starts such a run carries a shield before its label and the hover text "Windows asks for permission"; the flow that builds the button marks it (`Button::elevates`), and `row::action`, the one function that draws a `Button`, draws the shield. The main button sets it on a dark disc, since the glyph's own blue is lost on the accent. A disabled button gives the reason on hover (`Button::disabled`).

### Settings

`ds3-blaze.toml` next to the executable keeps the player's name, the last mode, the last host address and the names the host gave known players. The archive is meant to be unpacked into a folder of its own, where the logs already go.

### Closing

Closing the window while the hosts file points at ds3-blaze asks whether to restore the EA servers, with restoring as the default.

## Command line

`ds3-blaze-cli` uses `clap` derive, so the help and the option errors come from the code:

| Command | Purpose |
|---|---|
| `serve` | the server, with today's options; `--name` and `--player` names follow `players::check_nickname` |
| `hosts enable --address IP`, `hosts disable`, `hosts status` | the hosts entry; `enable` and `disable` need an elevated console |
| `fix game-rule`, `fix server-rule [--program PATH]`, `fix network-profile --address IP` | the network fixes the GUI offers; `--program` defaults to the running executable; all need an elevated console |
| `fix several [--game-rule] [--server-rule PATH] [--network-profile IP]` | the fixes named, one after another in one run; a failed fix does not stop the others, each prints its outcome, and the run fails when any fix failed |
| `net-check --peer IP [--address IP]` | the network findings, each wrong setting with the `fix` command that changes it; `--address` is the player's own VPN address, and without it the route is checked against any adapter `is_vpn` recognises |
| `p2p-check --peer IP [--address IP] [--port PORT] [--seconds N]` | the P2P exchange, after the same findings |
| `reach --host IP [--expect IP] [--name NICK]` | the guest's server check; `--name` sends the guest's nickname as its last stage |
| `proxy`, `ea-check` | research only, built with the feature `research` |

The GUI's settings file does not affect the command line, which stays predictable for tests.

## Packaging and CI

- CI runs from the workspace root: `cargo fmt --check`, `cargo clippy --workspace --all-targets --all-features -D warnings` (so the `research` code keeps compiling), `cargo build`, `cargo test --workspace`. The step that starts `serve` and runs `selftest` from PowerShell is replaced by the protocol test.
- `tools/package.ps1` packs both executables, the README and the license. `scripts/hosts.ps1` leaves the archive and the repository. Release builds leave out `research`.

## Documentation roles

The GUI owns the procedure: its banner and step rows say what to do and how to fix what is wrong. The documents stop retelling it.

| Document | Holds | Leaves out |
|---|---|---|
| Nexus page | what the mod is and why, requirements, a short start ("run `ds3-blaze.exe`, choose a mode, follow the steps"), going back to EA, limitations, log privacy | commands, options, ports |
| README | the same short start, the command line with a pointer to `ds3-blaze-cli --help`, the login token warning, development | the option list, which `clap` generates |
| [network-checks.md](../guides/network-checks.md) | manual checks with the command line for when the GUI is not enough | |

The README and the Nexus page still share the few sentences of the short start, because the Nexus page has to stand on its own. The note in [docs/README.md](../README.md) about keeping the two in step stays, now covering only those sentences. The limitation "names in game come from `--name` and `--player`" changes to the start screen and the players in the host's session.

## Tests

- `hosts::rewrite`: with and without a byte order mark, ANSI text, CRLF and LF, an empty file, a file without a final line break, legacy-marker lines, a section without an end line, repeated `enable`.
- `hosts::state`: an entry for a redirected name outside the section, before the section and after it, and a commented one.
- `PlayerBook`: admission and naming rules as `Config::admits` and `Config::name_for` have them now, and changes while sessions are open; a guest's sent name below a name the host set, below `own_name`, back after the host's name is cleared, and kept for a refused address.
- `players::check_nickname`: both ends of `NICKNAME_LENGTH`, `-` and `_`, and refusals of Cyrillic letters, a space and a line break.
- The nickname request: an accepted name, a refused one leaving the book unchanged, a name the host set answered in place of the sent one; `nickname_reply` for each answer; `reach` with a name against a server started in the test.
- The route part on parsed records: no route; the route through an adapter other than the chosen one, including one that `is_vpn` recognises; a gateway in between; a source address other than the chosen one; another VPN adapter up beside a passing route (no route finding) and beside a failing one (the likely cause).
- The local part on parsed records: no adapter holds the chosen address; a Public network with its fix; a block rule; a missing rule with inbound blocked and allowed; no server rule on a guest's PC; a port rule without a program counting as the game's.
- `fix::script`: the port from `GAME_PORT`, a program path with a single quote, the adapter holding the given address, the administrator check.
- `p2p`: two exchanges on the loopback address with different ports, started a moment apart; one exchange whose port is already taken; the failure text of an exchange without answers, with and without another VPN adapter up.
- The host's trigger: a first connection from an address starts one run, a second connection from it starts none, a refused connection starts one.
- `reach`: the resolved addresses against the expected one; a server started in the test, checked at its own address and while advertising another one.
- The protocol test described above.
- The steps of each mode as state machines, without a window.
- The banner of each mode: one case per row of its table, the mode driven into that state by inputs, and the order of the rules where two match.
- `fix several`: the arguments parsed into the fixes they name, and a run with no fix named refused.

A debug build fills the screens with made-up states when the variable `preview::VARIABLE` names one of the states `preview.rs` defines, so each banner and the session can be looked at without a VPN, an administrator prompt or a second PC.

## Stages

Each stage is a branch of its own, merged into `main` when CI passes. A stage that removes or changes a command also updates the README, the Nexus page and the guides that use it, so `main` never documents a command that is gone. Until stage 6 the command line keeps the name `ds3-blaze.exe`.

1. Workspace split with no change in behaviour: the crates, the moved modules, CI and `package.ps1` pointed at the workspace.
2. `Server::start` with ports in the configuration, the protocol test, the `selftest` command and its CI step removed.
3. Events, snapshot and `PlayerBook`; the core stops printing and the CLI prints the events.
4. `clap`, the `hosts` module and `hosts` command, `scripts/hosts.ps1` removed.
5. Diagnostics: findings and fixes, the route part tied to the chosen address, the `fix` command, `p2p` events, `reach`, and the hosts state reporting entries for the redirector outside the section.
6. The GUI, the guest's nickname sent to the host, and the renaming of the executables. The mode screens are then rebuilt around the banner, the quiet setup rows and the host's session, with `fix several` behind "Fix it".
7. README, Nexus page, network-checks and the docs index rewritten for the GUI.

## Guest nickname

The guest's window sends the player's name to the host's server, so the host no longer types it, and the guest's window shows the name the host's game will use.

**Rule.** `players::check_nickname` holds the rule, modelled on EA persona names: a length within `players::NICKNAME_LENGTH`, and ASCII letters, digits, `-` and `_` only. The start screen's name field, `serve --name` and `--player`, and the server's handling of a sent name all apply it, so a name needs no escaping in a URL, a log line or the settings file.

**Request.** `GET /ds3-blaze/name?n=<name>` on the QoS HTTP listener. The server takes the guest's address from the connection, so a guest can name only itself. It answers 200 with the name its game will show for that address (`name_for` after the change), or 400 with the reason when the rule refuses the name. A server without nicknames answers the path with its usual 404.

**Precedence.** `Roster` gains `offered`, the names guests sent, by address. `name_for` takes, in order: the host's entry in `names` (from `--player`, the settings file or the players in the host's session); `own_name` for this machine and the advertised address; the guest's entry in `offered`; `Player-` and the last octet. A name the host set therefore wins without a confirmation step, and clearing it in the session (`set_name(ip, None)`) brings the guest's name back. A sent name is kept for a refused address too, so the host sees whom "Let in" admits. `offered` lives as long as the server: the settings file keeps only the host's names, and a guest sends its name at each connection, so a nickname changed since the last session is not overruled by an old copy. Each sent name raises `event::changed` and logs `<address> calls itself <name>`.

**Timing.** A player already logged in keeps the name it logged in with, so the name has to arrive before the guest's game logs in. In the guest's steps it does, since `reach` comes before the game step.

**Guest side.** `reach` gains an optional last stage, `Stage::Nickname`, run when it is given a name: `reach::run(expected, address, ports, name)`, and `reach --name NICK` on the command line. `nickname_reply(status, body)`, a pure function, reads the answer: the name that was sent passes as "the host sees you as …"; another name passes as "the host calls you …", since the game works under it and the host can change it; 404 fails as a host whose ds3-blaze takes no nicknames; 400 fails with the server's reason. The GUI shows a failed nickname stage as a warning, not a failure: the game still works under the name the host's server gives, and the direct link step does not wait for it.

**Saves.** Campaign progress lives in local saves only, and neither player's save holds a player name, the id `Player::from_name` derives or an EA account id: checked on 2026-11-26 on the host's and the guest's saves from the co-op of 2026-11-25. A nickname may therefore change between sessions without a warning. The walk-through between two PCs in stage 6 repeats the check after a restore point reached together.
