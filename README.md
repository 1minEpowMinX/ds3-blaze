# Dead Space 3 co-op without EA servers

`ds3-blaze` is a local Blaze server for Dead Space 3 co-op on PC. It stands in for the EA servers that bring players together: the redirector, login, game rooms, matchmaking and QoS. Game traffic flows directly between the players over Radmin VPN (or any other VPN that puts them on one subnet), so the ISP's NAT no longer gets in the way.

The game is still launched through the EA App; the server replaces Blaze only.

![Official co-op fails when one player is behind a strict NAT; with ds3-blaze both players share a VPN, the server runs on the host's PC, and game traffic flows directly between them](docs/images/how-it-works.svg)

## Status

| Stage | Status |
|---|---|
| Redirector (SSLv3) | works in game |
| Online login | works in game |
| Host's room | works in game: create, play, quit with a save |
| Second player | works in game through quick match and through an EA App invite |

Protocol analysis, traffic captures and the work plan: [docs/](docs/README.md).

## Getting it

Release archives are on the repository's Releases page. Each holds `ds3-blaze.exe`, the window that walks both players through hosting and joining, `ds3-blaze-cli.exe`, the command line, this README, the license and the third-party notices; the commands below run from the unpacked folder.

GitHub Actions builds every release archive and signs a build provenance attestation for it and for both executables inside. With the [GitHub CLI](https://cli.github.com/) installed, either file can be checked against this repository's release workflow; the command fails for a file that workflow did not produce:

```powershell
gh attestation verify <archive or executable> --repo 1minEpowMinX/ds3-blaze --signer-workflow 1minEpowMinX/ds3-blaze/.github/workflows/release.yml
```

To build from source instead, install stable Rust on Windows (`net-check` reads the network settings through PowerShell) and run `tools\package.ps1`. It builds the release binaries and writes the same archive to `dist\`.

```powershell
powershell -ExecutionPolicy Bypass -File .\tools\package.ps1
```

`.\ds3-blaze-cli.exe --help` lists the commands, and `.\ds3-blaze-cli.exe <command> --help` the options of one.

## Running

Both players join the same Radmin VPN network, and both run the network check at the same time, each with the other PC's VPN address:

```powershell
.\ds3-blaze-cli.exe p2p-check --peer <other PC's VPN address>
```

Each Windows setting the check finds wrong comes with the `fix` command that changes it. [docs/guides/network-checks.md](docs/guides/network-checks.md) describes every check.

Back up your saves before the first test sessions.

### Host

The server runs on the host's PC.

1. Start the server on all interfaces with your VPN address. `--player` names the guest; without it the guest shows up as `Player-<last octet of the guest's address>`:

   ```powershell
   .\ds3-blaze-cli.exe serve --bind 0.0.0.0 --advertise <host's VPN address> --name <host's name> --player <guest's VPN address>=<guest's name>
   ```

   On a VPN network open to strangers, add `--only-listed`: the server then accepts only your own PC and the `--player` addresses.

2. Point `gosredirector.ea.com` at your own PC from a PowerShell run as administrator, in the same folder:

   ```powershell
   .\ds3-blaze-cli.exe hosts enable
   ```

3. Start the game, go online in co-op and create a game. A **public** game takes the guest through quick match; a private one takes invites only, so invite the guest through the EA App from inside it.

If the guest does not get in, the `logs\` folder shows how far the server got. The logs hold friend lists and addresses, so share them only privately.

### Guest

1. Point `gosredirector.ea.com` at the host from a PowerShell run as administrator, in the unpacked folder:

   ```powershell
   .\ds3-blaze-cli.exe hosts enable --address <host's VPN address>
   ```

2. Start the game and go online in co-op. Then either:

   - pick quick match to join the host's public game. The server puts you into it as soon as it exists, retrying for as long as the game's search lasts, so you may start searching a little before the host's game is up;
   - or accept the host's invite in the EA App. The server knows players by the addresses they connect from, not by EA account, so it matches the invite to the host only while the two of you are the only players on it.

If the game cannot get in, check every connection it makes to the host's server while the server runs:

```powershell
.\ds3-blaze-cli.exe reach --host <host's VPN address>
```

It prints one line per stage (the hosts entry, the redirector, the Blaze server, QoS) and says what is wrong with each one that fails.

### Afterwards

Both players go back to the EA servers from a PowerShell run as administrator:

```powershell
.\ds3-blaze-cli.exe hosts disable
```

`.\ds3-blaze-cli.exe hosts status` tells which servers the game reaches, and needs no administrator rights. Each `enable` and `disable` first saves the hosts file as it was, minus any ds3-blaze entries, to `hosts.ds3-blaze.bak` beside it; restoring that copy also returns to the EA servers.

## Logs and privacy

The server writes its logs to `logs/` under the directory it is started from. Logs of the `proxy` command contain EA account data, the persona name, the friend list and IP addresses. Do not publish them. `.gitignore` keeps logs, traffic captures, memory dumps and game files out of the repository.

When a player logs in, the game sends the server the EA login token it got from the EA App. `ds3-blaze` ignores the token and leaves it out of the logs, but it still reaches the host's PC in plain text, and while it is valid it can sign in to EA as that player. Encryption cannot prevent this, since the host's PC is the recipient. Join only a host you trust.

## Development

Each feature or fix is developed on its own branch named `<type>/<name>`, where `<type>` is one of the commit types below, and lands on `main` through a merge (`git merge --no-ff`); nothing is committed to `main` directly.

CI on GitHub Actions checks every push to any branch and every pull request into `main`: `tools\check-commits.ps1` over the branch name and the subjects of the commits the branch adds, `tools\check-clean.ps1` over the tracked files, then `cargo fmt`, `cargo clippy` with every feature on and warnings as errors, a build and the tests, among them a protocol test that runs the game's client sequence against a server started inside the test. A newer push to the same branch cancels the check still running for it. Pushing an annotated tag `v<version>` that matches `ds3-blaze/Cargo.toml` runs the same checks, packs the archive, attests its build provenance and opens a draft release. Its notes are rendered by [git-cliff](https://git-cliff.org) from `cliff.toml`: the tag's message comes first as the summary, followed by the commits since the previous tag. Commit messages follow [Conventional Commits](https://www.conventionalcommits.org/), with the types listed in `$Types` of `tools\check-commits.ps1`; a `Deploy:` trailer lands under "Action required" in the notes. With git-cliff installed, `git cliff --unreleased` previews the next release's notes.

The `proxy` and `ea-check` commands, used for protocol research, exist only in a build with the `research` feature, which release archives leave out:

```powershell
cargo build --features research --manifest-path ds3-blaze\Cargo.toml
```

The same checks run locally once the hooks are enabled in a clone: the repository check before each commit, and the naming check on each commit message, which also refuses a commit on `main` other than one that concludes a merge:

```powershell
git config core.hooksPath .githooks
```

Personal values that must never be committed, such as an EA persona name or a home IP address, go one per line into `.git\info\sensitive.txt`. The check searches staged files for them; the file itself stays inside `.git`.

## Credits

- [Pocket Relay](https://github.com/PocketRelay) (Jacobtread, MIT): Blaze component ids and message field layouts, adapted to Dead Space 3. The headers of `components.rs`, `models.rs` and `game.rs` cite the sources.
- [blaze-ssl-async](https://github.com/jacobtread/blaze-ssl-async) and [tdf](https://crates.io/crates/tdf) (Jacobtread, MIT): SSLv3 and the TDF format, used as dependencies.
- [Aim4kill/BlazeSDK](https://github.com/Aim4kill/BlazeSDK): Blaze 3 command and notification definitions.

Reference copies of the upstream projects go into `vendor/`, which is not tracked:

```powershell
git clone https://github.com/PocketRelay/Server vendor/pocket-relay-server
git clone https://github.com/PocketRelay/Client vendor/pocket-relay-client
git clone https://github.com/PocketRelay/PocketRelayClientShared vendor/pocket-relay-client-shared
git clone https://github.com/jacobtread/blaze-ssl-async vendor/blaze-ssl-async
```

## License

[MIT](LICENSE). Dead Space 3 and Blaze belong to Electronic Arts. This project is not affiliated with EA and contains no game files.

[THIRD_PARTY_NOTICES](THIRD_PARTY_NOTICES) holds the licenses of the adapted Pocket Relay code and of every crate compiled into the executables. `tools\notices.ps1` regenerates it from `Cargo.lock` with [cargo-about](https://github.com/EmbarkStudios/cargo-about) after a dependency change; CI fails while it is out of date.
