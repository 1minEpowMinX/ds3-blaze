# Checking the network before launching the game

None of the checks needs the game.

1. `ds3-blaze net-check --peer <other PC's VPN address>` reads the Windows settings through PowerShell (NetTCPIP, NetAdapter, NetSecurity) and checks:
   - which adapter Windows will use to send traffic to the peer (`Find-NetRoute`) and whether it is a known VPN;
   - whether the peer's address lies in that adapter's network (a route without a gateway);
   - which other VPN adapters are up and whether they capture the peer's address;
   - which category the network belongs to ("Public" or "Private") and whether the firewall is on in that profile;
   - which rules for `deadspace3.exe` (inbound UDP) and `ds3-blaze.exe` (inbound TCP and UDP) apply in that profile, including blocking rules: they take precedence over allowing ones.

   Third-party firewalls are only named; the command cannot see their rules.
2. `ds3-blaze p2p-check --peer <other PC's VPN address>` runs `net-check` first and is then started on both PCs at the same time. Each side sends UDP pings to port 3659 from port 3659, as the game does, and answers the other side's pings. The result shows in which direction packets do not arrive.
3. `ds3-blaze selftest --host <host's VPN address>` runs on the guest's PC while the host runs `serve`. It goes through the guest's whole path over the VPN: redirector, login, joining a room through matchmaking and `joinGame`, QoS over HTTP and UDP.
