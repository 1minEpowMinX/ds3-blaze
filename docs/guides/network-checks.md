# Checking the network before launching the game

None of the checks needs the game. `ds3-blaze <command> --help` lists each command's options.

## `net-check`

```powershell
.\ds3-blaze-cli.exe net-check --peer <other PC's VPN address> --address <this PC's VPN address>
```

The command reads the Windows settings through PowerShell (NetTCPIP, NetAdapter, NetSecurity) and prints its findings in two parts, each line marked `[ok]`, `[note]`, `[warning]` or `[problem]`. It fails when a finding is a problem.

`--address` names this PC's VPN address. Without it, any adapter whose name looks like a VPN client's counts as this PC's; with it, only the adapter holding that address does, so another VPN that takes over the other player's address is caught whatever its adapter is called.

**This PC:**

- the adapter holding this PC's VPN address is up;
- other VPN adapters that are up, as a note: another VPN client can take over or drop the traffic;
- the category of the VPN adapter's network ("Public" or "Private") and whether the firewall is on in its profile;
- third-party firewalls, which are only named, since their rules are not visible to Windows;
- the Windows Firewall rules for the game (rules for `deadspace3.exe`, or rules that open the game's UDP port to any program) and for the `ds3-blaze-cli.exe` that runs the check. Only the host's PC needs the server's rule; a guest can leave a warning about it. A blocking rule is a problem, since it takes precedence over allowing ones. A missing allowing rule is a warning only where the profile blocks inbound traffic by default.

**Route to the other player:**

- Windows has a route to the other player's address;
- it leaves through this PC's VPN adapter, and the address lies in that adapter's network, with no gateway in between;
- a UDP socket sending to the other player picks this PC's VPN address as its source.

When the route fails while another VPN adapter is up, that adapter is named as the likely cause.

## Fixes

A finding that a Windows setting can resolve is followed by a `fix:` line with the command that changes it. Each runs in a PowerShell started as administrator:

| Command | Change |
|---|---|
| `fix game-rule` | allows inbound UDP on the game's port, for any program, since ds3-blaze does not know where the game is installed |
| `fix server-rule [--program PATH]` | allows inbound traffic to the program that runs the server, the running `ds3-blaze-cli.exe` by default |
| `fix network-profile --address IP` | sets the network of the adapter holding the address to Private |

The firewall rules are named `ds3-blaze: allow …`, so they sort together in the Windows Firewall list. Running a fix again replaces its rule rather than adding a second one. `Remove-NetFirewallRule -DisplayName '<name>'` removes a rule that is no longer wanted. Running `net-check` again shows whether the fix worked.

## `p2p-check`

```powershell
.\ds3-blaze-cli.exe p2p-check --peer <other PC's VPN address> --address <this PC's VPN address>
```

The command runs `net-check` first and is then started on both PCs at the same time. Each side sends UDP pings to the game's port from the same port, as the game does, and answers the other side's pings. The result shows in which direction packets do not arrive. When no answers come back while another VPN adapter is up, the result names that adapter: a VPN client's kill switch drops traffic without changing the routes, so only the exchange shows it.

The game holds its port while it runs, so the check needs the game closed.

## `reach`

The guest runs it while the host's server is running:

```powershell
.\ds3-blaze-cli.exe reach --host <host's VPN address>
```

It goes through every connection the game makes before it joins, one line per stage:

1. **hosts entry**: `gosredirector.ea.com` resolves to the host's address. When it does not, the line is followed by the `hosts enable` command that sets the entry.
2. **redirector**: the host's redirector answers over SSLv3 and sends the game to the host's address. A server that sends it elsewhere was started with another `--advertise` address.
3. **Blaze server**: the server the redirector names answers `preAuth`. It is not tried when the redirector stage fails.
4. **QoS HTTP** and **QoS UDP**: the host's QoS services answer.

The check creates no room and starts no matchmaking, so it is safe while the host is already playing.

The host can check its own server through the VPN, which shows that the listeners answer on the VPN interface:

```powershell
.\ds3-blaze-cli.exe reach --host <host's own VPN address> --expect 127.0.0.1
```

When the host's own check passes but the guest's stages give no answer, the host's firewall drops the guest's connections; `net-check` on the host's PC names the missing rule.
