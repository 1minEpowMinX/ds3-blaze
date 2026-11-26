//! Windows network findings for co-op over a VPN: the local part (the player's
//! VPN adapter, other VPN adapters, the network profile and the Windows
//! Firewall rules for the game and the server) and the route part (how traffic
//! to the other player leaves this PC).
//!
//! The data comes from the NetTCPIP, NetAdapter, NetConnection and NetSecurity
//! PowerShell modules, read by [`SCRIPT`] as `|`-separated records and parsed
//! into [`Records`]; both parts work on the parsed records only.

use crate::p2p::GAME_PORT;
use crate::powershell;
use std::collections::BTreeSet;
use std::net::{Ipv4Addr, UdpSocket};
use std::path::{Path, PathBuf};

/// PowerShell script printing one record per line; `-Peer`, when given, is the
/// other player's address, and `-GamePort` the game's P2P port.
///
/// Records: `ROUTE|alias|description|local ip|prefix length|next hop|destination|ifindex`,
/// `ADAPTER|name|description|ifindex|ipv4 list|network category`,
/// `FIREWALL|profile|enabled|default inbound action`,
/// `RULE|program|name|direction|action|enabled|profiles|protocol|local ports`
/// for the rules bound to the game's or a ds3-blaze executable and for those
/// whose local ports name the game's port, `THIRDPARTY|product`.
const SCRIPT: &str = r#"param([string]$Peer, [string]$GamePort)
$ErrorActionPreference = 'SilentlyContinue'
[Console]::OutputEncoding = [Text.Encoding]::UTF8
function Clean($value) { "$value" -replace '[|\r\n]', ' ' }
if ($Peer) {
    $found = Find-NetRoute -RemoteIPAddress $Peer
    $address = $found | Where-Object { $_.CimClass.CimClassName -eq 'MSFT_NetIPAddress' } | Select-Object -First 1
    $route = $found | Where-Object { $_.CimClass.CimClassName -eq 'MSFT_NetRoute' } | Select-Object -First 1
    if ($route) {
        $nic = Get-NetAdapter -InterfaceIndex $route.ifIndex -IncludeHidden
        "ROUTE|$(Clean $route.InterfaceAlias)|$(Clean $nic.InterfaceDescription)|$(Clean $address.IPAddress)|$(Clean $address.PrefixLength)|$(Clean $route.NextHop)|$(Clean $route.DestinationPrefix)|$($route.ifIndex)"
    }
}
Get-NetAdapter | Where-Object Status -eq 'Up' | ForEach-Object {
    $ips = (Get-NetIPAddress -InterfaceIndex $_.ifIndex -AddressFamily IPv4).IPAddress -join ','
    $category = (Get-NetConnectionProfile -InterfaceIndex $_.ifIndex).NetworkCategory
    "ADAPTER|$(Clean $_.Name)|$(Clean $_.InterfaceDescription)|$($_.ifIndex)|$(Clean $ips)|$(Clean $category)"
}
Get-NetFirewallProfile | ForEach-Object { "FIREWALL|$($_.Name)|$($_.Enabled)|$($_.DefaultInboundAction)" }
Get-NetFirewallApplicationFilter | Where-Object { $_.Program -match '(?i)\\(deadspace3|ds3-blaze[^\\]*)\.exe$' } | ForEach-Object {
    $program = $_.Program
    $_ | Get-NetFirewallRule | ForEach-Object {
        $ports = $_ | Get-NetFirewallPortFilter
        "RULE|$(Clean $program)|$(Clean $_.DisplayName)|$($_.Direction)|$($_.Action)|$($_.Enabled)|$(Clean $_.Profile)|$(Clean $ports.Protocol)|$(Clean $ports.LocalPort)"
    }
}
Get-NetFirewallPortFilter -Protocol UDP | Where-Object { $_.LocalPort -contains $GamePort } | ForEach-Object {
    $ports = $_
    $_ | Get-NetFirewallRule | ForEach-Object {
        $program = ($_ | Get-NetFirewallApplicationFilter).Program
        "RULE|$(Clean $program)|$(Clean $_.DisplayName)|$($_.Direction)|$($_.Action)|$($_.Enabled)|$(Clean $_.Profile)|$(Clean $ports.Protocol)|$(Clean $ports.LocalPort)"
    }
}
Get-CimInstance -Namespace root/SecurityCenter2 -ClassName FirewallProduct | ForEach-Object { "THIRDPARTY|$(Clean $_.displayName)" }
"#;

/// Lower-case fragments of adapter names and descriptions that mark a VPN or
/// virtual tunnel adapter.
const VPN_MARKERS: &[&str] = &[
    "vpn",
    "radmin",
    "hamachi",
    "zerotier",
    "tailscale",
    "wireguard",
    "wintun",
    "openvpn",
    "tap-",
    "tap ",
    "tun",
    "nordlynx",
    "softether",
    "anyconnect",
    "fortinet",
    "pangp",
    "outline",
    "amnezia",
    "proton",
];

/// File name of the game's executable, whose rules are matched by name since
/// its folder differs between installs.
const GAME_EXE: &str = "deadspace3.exe";

/// Next hop of a route to an address on the adapter's own network.
const ON_LINK: &str = "0.0.0.0";

/// How much a finding matters, from least to most.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    /// A check passed.
    Ok,
    /// Something worth knowing that does not stop co-op.
    Note,
    /// Something that may stop co-op.
    Warning,
    /// Something that stops co-op.
    Problem,
}

/// Change that resolves a finding, carried out by the `fix` command.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Fix {
    /// Allows inbound UDP on the game's P2P port.
    GameRule,
    /// Allows inbound traffic to the server's executable at this path.
    ServerRule(PathBuf),
    /// Sets the network of the adapter holding this address to Private.
    PrivateProfile(Ipv4Addr),
}

/// One result of a check.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Finding {
    pub severity: Severity,
    pub text: String,
    /// The change that resolves it, when one exists.
    pub fix: Option<Fix>,
}

impl Finding {
    fn new(severity: Severity, text: impl Into<String>) -> Self {
        Self {
            severity,
            text: text.into(),
            fix: None,
        }
    }

    fn with_fix(mut self, fix: Fix) -> Self {
        self.fix = Some(fix);
        self
    }
}

/// The player's own VPN adapter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Own {
    /// The adapter that holds this address.
    Address(Ipv4Addr),
    /// Every adapter whose name or description looks like a VPN's.
    AnyVpn,
}

/// What the local part checks on this PC.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Setup {
    pub own: Own,
    /// The executable that runs the server on this PC, `None` on a guest's.
    pub server: Option<PathBuf>,
}

/// Route to the other player as Windows chooses it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Route {
    pub alias: String,
    pub description: String,
    /// Address the route sends from, `None` when Windows names none.
    pub local_ip: Option<Ipv4Addr>,
    pub prefix_length: String,
    pub next_hop: String,
    pub destination: String,
    pub if_index: String,
}

/// A network adapter that is up.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Adapter {
    pub name: String,
    pub description: String,
    pub if_index: String,
    pub ipv4: Vec<Ipv4Addr>,
    /// Network category of its connection (`Public`, `Private` or
    /// `DomainAuthenticated`), `None` when Windows reports none.
    pub category: Option<String>,
}

impl Adapter {
    /// Returns whether the name or the description looks like a VPN adapter's.
    pub fn is_vpn(&self) -> bool {
        is_vpn(&self.name, &self.description)
    }

    /// Returns the name and the description as a finding shows them.
    fn label(&self) -> String {
        format!("\"{}\" ({})", self.name, self.description)
    }

    /// Returns the IPv4 addresses separated by commas.
    fn addresses(&self) -> String {
        let addresses: Vec<String> = self.ipv4.iter().map(Ipv4Addr::to_string).collect();
        addresses.join(", ")
    }
}

/// Windows Firewall state of one profile.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FirewallProfile {
    pub name: String,
    pub enabled: bool,
    /// `true` unless the default inbound action is `Allow`.
    pub blocks_inbound: bool,
}

/// A Windows Firewall rule bound to the game's or a ds3-blaze executable.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Rule {
    pub program: String,
    pub name: String,
    pub inbound: bool,
    pub allow: bool,
    pub enabled: bool,
    pub profiles: String,
    /// `Any`, `TCP`, `UDP` or a protocol number.
    pub protocol: String,
    pub local_ports: String,
}

impl Rule {
    /// Returns whether the rule matches `protocol` (`TCP` or `UDP`).
    fn covers(&self, protocol: &str) -> bool {
        self.protocol.eq_ignore_ascii_case("any") || self.protocol.eq_ignore_ascii_case(protocol)
    }

    /// Returns whether the rule's local ports name `port` itself.
    fn opens(&self, port: u16) -> bool {
        let port = port.to_string();
        self.local_ports
            .split(|c: char| c == ',' || c.is_whitespace())
            .any(|listed| listed == port)
    }

    /// Returns whether the rule is an enabled inbound rule for firewall profile
    /// `profile`.
    fn applies_inbound(&self, profile: &str) -> bool {
        self.inbound
            && self.enabled
            && (self.profiles.eq_ignore_ascii_case("any") || self.profiles.contains(profile))
    }
}

/// Everything [`SCRIPT`] reports.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Records {
    pub route: Option<Route>,
    pub adapters: Vec<Adapter>,
    pub firewall: Vec<FirewallProfile>,
    pub rules: Vec<Rule>,
    pub third_party: Vec<String>,
}

impl Records {
    /// Returns the records in `text`, one per line; lines of other kinds are
    /// skipped and missing fields are empty.
    pub fn parse(text: &str) -> Self {
        let mut records = Self::default();
        for line in text.lines() {
            // PowerShell may start its UTF-8 output with a byte order mark.
            let line = line.trim_start_matches('\u{FEFF}');
            let fields: Vec<&str> = line.trim().split('|').map(str::trim).collect();
            let field = |i: usize| fields.get(i).copied().unwrap_or_default().to_string();
            let yes = |i: usize, word: &str| field(i).eq_ignore_ascii_case(word);
            match fields[0] {
                "ROUTE" => {
                    records.route = Some(Route {
                        alias: field(1),
                        description: field(2),
                        local_ip: field(3).parse().ok(),
                        prefix_length: field(4),
                        next_hop: field(5),
                        destination: field(6),
                        if_index: field(7),
                    })
                }
                "ADAPTER" => records.adapters.push(Adapter {
                    name: field(1),
                    description: field(2),
                    if_index: field(3),
                    ipv4: field(4)
                        .split(',')
                        .filter_map(|ip| ip.trim().parse().ok())
                        .collect(),
                    category: Some(field(5)).filter(|category| !category.is_empty()),
                }),
                "FIREWALL" => records.firewall.push(FirewallProfile {
                    name: field(1),
                    enabled: yes(2, "true"),
                    blocks_inbound: !yes(3, "allow"),
                }),
                "RULE" => records.rules.push(Rule {
                    program: field(1),
                    name: field(2),
                    inbound: yes(3, "inbound"),
                    allow: yes(4, "allow"),
                    enabled: yes(5, "true"),
                    profiles: field(6),
                    protocol: field(7),
                    local_ports: field(8),
                }),
                "THIRDPARTY" => records.third_party.push(field(1)),
                _ => {}
            }
        }
        records
    }

    /// Returns the adapters `own` names.
    fn own_adapters(&self, own: &Own) -> Vec<&Adapter> {
        self.adapters
            .iter()
            .filter(|adapter| match own {
                Own::Address(address) => adapter.ipv4.contains(address),
                Own::AnyVpn => adapter.is_vpn(),
            })
            .collect()
    }
}

/// Reads the network settings through PowerShell, with the route to `peer`
/// when one is given.
///
/// # Errors
///
/// Returns the error of running PowerShell.
pub fn read(peer: Option<Ipv4Addr>) -> std::io::Result<Records> {
    let mut args = vec![("GamePort", GAME_PORT.to_string())];
    args.extend(peer.map(|peer| ("Peer", peer.to_string())));
    Ok(Records::parse(&powershell::run(SCRIPT, &args)?))
}

/// Returns whether an adapter name or description looks like a VPN adapter.
fn is_vpn(name: &str, description: &str) -> bool {
    let text = format!("{name} {description}").to_lowercase();
    VPN_MARKERS.iter().any(|marker| text.contains(marker))
}

/// Returns the source address Windows picks for UDP to `peer`, without sending.
pub fn source_ip(peer: Ipv4Addr) -> Option<Ipv4Addr> {
    let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).ok()?;
    socket.connect((peer, 9)).ok()?;
    match socket.local_addr().ok()?.ip() {
        std::net::IpAddr::V4(ip) => Some(ip),
        std::net::IpAddr::V6(_) => None,
    }
}

/// Returns the VPN adapters that are up besides the ones `own` names.
pub fn other_vpn_adapters<'a>(records: &'a Records, own: &Own) -> Vec<&'a Adapter> {
    let own_adapters = records.own_adapters(own);
    records
        .adapters
        .iter()
        .filter(|adapter| adapter.is_vpn() && !own_adapters.contains(adapter))
        .collect()
}

/// Returns the findings about this PC: the player's VPN adapter, other VPN
/// adapters that are up, the network profile of the player's adapter, and the
/// firewall rules for the game and, with [`Setup::server`], the server in that
/// profile.
pub fn local(records: &Records, setup: &Setup) -> Vec<Finding> {
    let mut findings = Vec::new();
    let own_adapters = records.own_adapters(&setup.own);
    if own_adapters.is_empty() {
        findings.push(Finding::new(
            Severity::Problem,
            match setup.own {
                Own::Address(address) => format!(
                    "no adapter that is up holds {address}: connect the VPN, or choose the address it gives this PC"
                ),
                Own::AnyVpn => "no VPN adapter is up: connect the VPN".into(),
            },
        ));
    }
    for adapter in &own_adapters {
        findings.push(Finding::new(
            Severity::Ok,
            format!(
                "VPN adapter {} is up with {}",
                adapter.label(),
                adapter.addresses()
            ),
        ));
    }
    for adapter in other_vpn_adapters(records, &setup.own) {
        findings.push(Finding::new(
            Severity::Note,
            format!(
                "another VPN adapter is up: {} with {}; another VPN can take over the other player's traffic",
                adapter.label(),
                adapter.addresses()
            ),
        ));
    }
    for product in &records.third_party {
        findings.push(Finding::new(
            Severity::Warning,
            format!(
                "third-party firewall \"{product}\" is registered; its rules are not checked here"
            ),
        ));
    }
    let mut profiles = BTreeSet::new();
    for adapter in &own_adapters {
        match adapter.category.as_deref() {
            None => findings.push(Finding::new(
                Severity::Warning,
                format!(
                    "the network category of {} is unknown, so its firewall rules are not matched to a profile",
                    adapter.label()
                ),
            )),
            Some(category) => {
                let profile = profile_of(category);
                profiles.insert(profile);
                let address = match setup.own {
                    Own::Address(address) => Some(address),
                    Own::AnyVpn => adapter.ipv4.first().copied(),
                };
                findings.push(match (address, profile) {
                    (Some(address), "Public") => Finding::new(
                        Severity::Note,
                        format!(
                            "Windows files the network of {} as Public, where the firewall blocks \
                             the most inbound traffic; Private suits a network of friends",
                            adapter.label()
                        ),
                    )
                    .with_fix(Fix::PrivateProfile(address)),
                    _ => Finding::new(
                        Severity::Ok,
                        format!(
                            "Windows files the network of {} as {category}, firewall profile {profile}",
                            adapter.label()
                        ),
                    ),
                });
            }
        }
    }
    for profile in profiles {
        check_firewall(records, setup.server.as_deref(), profile, &mut findings);
    }
    findings
}

/// Returns the firewall profile name Windows applies to network category
/// `category`.
fn profile_of(category: &str) -> &'static str {
    match category {
        "Private" => "Private",
        "DomainAuthenticated" => "Domain",
        _ => "Public",
    }
}

/// Adds the findings about firewall profile `profile` and the rules for the
/// game and the server at `server` in it.
fn check_firewall(
    records: &Records,
    server: Option<&Path>,
    profile: &str,
    findings: &mut Vec<Finding>,
) {
    let Some(state) = records
        .firewall
        .iter()
        .find(|p| p.name.eq_ignore_ascii_case(profile))
    else {
        findings.push(Finding::new(
            Severity::Warning,
            format!("the Windows Firewall state of the {profile} profile is unreadable"),
        ));
        return;
    };
    if !state.enabled {
        findings.push(Finding::new(
            Severity::Ok,
            format!("Windows Firewall is off for the {profile} profile"),
        ));
        return;
    }
    findings.push(Finding::new(
        Severity::Ok,
        format!(
            "Windows Firewall is on for the {profile} profile, unsolicited inbound traffic is {}",
            if state.blocks_inbound {
                "blocked by default"
            } else {
                "allowed by default"
            }
        ),
    ));
    // The game rule `fix` adds opens the port for any program.
    let game = |rule: &Rule| {
        rule.program.to_lowercase().ends_with(GAME_EXE)
            || (rule.program.eq_ignore_ascii_case("any") && rule.opens(GAME_PORT))
    };
    check_program(
        records,
        state,
        Program {
            name: GAME_EXE.into(),
            role: "the game's P2P traffic",
            protocols: &["UDP"],
            fix: Fix::GameRule,
        },
        &game,
        findings,
    );
    if let Some(server) = server {
        let runs_server = |rule: &Rule| {
            Path::new(&rule.program)
                .as_os_str()
                .eq_ignore_ascii_case(server)
        };
        check_program(
            records,
            state,
            Program {
                name: server.display().to_string(),
                role: "Blaze and QoS",
                protocols: &["TCP", "UDP"],
                fix: Fix::ServerRule(server.to_path_buf()),
            },
            &runs_server,
            findings,
        );
    }
}

/// An executable whose inbound rules a finding is about.
struct Program {
    name: String,
    /// What the executable receives.
    role: &'static str,
    protocols: &'static [&'static str],
    /// The change that adds a rule allowing it.
    fix: Fix,
}

/// Adds the findings about the inbound rules for `program` that `matches`
/// picks, in the firewall profile `state`.
fn check_program(
    records: &Records,
    state: &FirewallProfile,
    program: Program,
    matches: &dyn Fn(&Rule) -> bool,
    findings: &mut Vec<Finding>,
) {
    let profile = &state.name;
    let rules: Vec<&Rule> = records
        .rules
        .iter()
        .filter(|rule| matches(rule) && rule.applies_inbound(profile))
        .collect();
    for protocol in program.protocols {
        let covering: Vec<&&Rule> = rules.iter().filter(|r| r.covers(protocol)).collect();
        // Block rules win over allow rules in Windows Firewall.
        if let Some(block) = covering.iter().find(|r| !r.allow) {
            findings.push(Finding::new(
                Severity::Problem,
                format!(
                    "rule \"{}\" blocks inbound {protocol} to {} ({}) in the {profile} profile; \
                     block rules win over allow rules, disable or delete it",
                    block.name, program.name, program.role
                ),
            ));
        } else if let Some(allow) = covering.first() {
            let ports =
                if allow.local_ports.eq_ignore_ascii_case("any") || allow.local_ports.is_empty() {
                    String::new()
                } else {
                    format!(", local ports {}", allow.local_ports)
                };
            findings.push(Finding::new(
                Severity::Ok,
                format!(
                    "rule \"{}\" allows inbound {protocol} to {} ({}){ports}",
                    allow.name, program.name, program.role
                ),
            ));
        } else if state.blocks_inbound {
            findings.push(
                Finding::new(
                    Severity::Warning,
                    format!(
                        "no rule allows inbound {protocol} to {} ({}) in the {profile} profile, \
                         where inbound traffic is blocked by default",
                        program.name, program.role
                    ),
                )
                .with_fix(program.fix.clone()),
            );
        }
    }
}

/// Returns the findings about traffic to `peer`: whether Windows has a route,
/// whether it leaves through the adapter `own` names with the other player on
/// that adapter's network, and whether `source`, the address a UDP socket
/// connected to `peer` sends from, is the player's own.
///
/// When a finding is a warning or a problem, the other VPN adapters that are up
/// are named as the likely cause.
pub fn route(
    records: &Records,
    peer: Ipv4Addr,
    own: &Own,
    source: Option<Ipv4Addr>,
) -> Vec<Finding> {
    let Some(route) = &records.route else {
        return vec![Finding::new(
            Severity::Problem,
            format!(
                "Windows has no route to {peer}: the VPN is not connected or the address is wrong"
            ),
        )];
    };
    let mut findings = Vec::new();
    let leaves = format!(
        "traffic to {peer} leaves through \"{}\" ({}) from {}/{}",
        route.alias,
        route.description,
        route.local_ip.map(|ip| ip.to_string()).unwrap_or_default(),
        route.prefix_length
    );
    let own_adapters = records.own_adapters(own);
    let through_own = match own {
        Own::Address(_) => own_adapters
            .iter()
            .any(|adapter| adapter.if_index == route.if_index),
        Own::AnyVpn => is_vpn(&route.alias, &route.description),
    };
    // Under Own::AnyVpn a route through a VPN adapter makes that adapter the
    // player's, so the other VPN adapters are the ones besides it. A route
    // through any other adapter leaves every VPN adapter a candidate for the
    // player's own, none of them another.
    let others: Vec<&Adapter> = match own {
        Own::Address(_) => other_vpn_adapters(records, own),
        Own::AnyVpn if through_own => records
            .adapters
            .iter()
            .filter(|adapter| adapter.is_vpn() && adapter.if_index != route.if_index)
            .collect(),
        Own::AnyVpn => Vec::new(),
    };
    if through_own {
        findings.push(Finding::new(Severity::Ok, leaves));
        if route.next_hop == ON_LINK {
            findings.push(Finding::new(
                Severity::Ok,
                "the other player is on the adapter's own network, with no gateway in between",
            ));
        } else {
            findings.push(Finding::new(
                Severity::Warning,
                format!(
                    "the route passes gateway {}: the other player is not on the same VPN network as this PC, \
                     or another VPN routes this address",
                    route.next_hop
                ),
            ));
        }
        let expected = match own {
            Own::Address(address) => Some(*address),
            Own::AnyVpn => route.local_ip,
        };
        if let (Some(source), Some(expected)) = (source, expected)
            && source != expected
        {
            findings.push(Finding::new(
                Severity::Warning,
                format!("a test socket picks source {source}, not {expected}: another tunnel may take the traffic"),
            ));
        }
    } else {
        findings.push(Finding::new(
            Severity::Problem,
            match own {
                Own::Address(address) => format!(
                    "{leaves}, not through the adapter holding {address}: the other player's address is not \
                     on your VPN network, or another VPN takes it over"
                ),
                Own::AnyVpn => format!(
                    "{leaves}, which is not a known VPN adapter: the other player's address is not on the VPN \
                     network, or the VPN adapter is down"
                ),
            },
        ));
        if *own == Own::AnyVpn {
            for adapter in &own_adapters {
                findings.push(Finding::new(
                    Severity::Note,
                    format!(
                        "VPN adapter {} is up with {}: the other player's address should be on its network",
                        adapter.label(),
                        adapter.addresses()
                    ),
                ));
            }
        }
    }
    // Without an adapter of the player's, the local part reports the missing
    // address, and every VPN adapter would count as another.
    if !own_adapters.is_empty() && findings.iter().any(|f| f.severity >= Severity::Warning) {
        for adapter in others {
            findings.push(Finding::new(
                Severity::Warning,
                format!(
                    "VPN adapter {} with {} is up and is the likely cause: disconnect that VPN and check again",
                    adapter.label(),
                    adapter.addresses()
                ),
            ));
        }
    }
    findings
}

#[cfg(test)]
mod tests {
    use super::*;

    const PEER: Ipv4Addr = Ipv4Addr::new(26, 0, 0, 2);
    const OWN: Ipv4Addr = Ipv4Addr::new(26, 0, 0, 1);

    fn radmin() -> Adapter {
        Adapter {
            name: "Radmin VPN".into(),
            description: "Famatech Radmin VPN Ethernet Adapter".into(),
            if_index: "7".into(),
            ipv4: vec![OWN],
            category: Some("Public".into()),
        }
    }

    fn other_vpn() -> Adapter {
        Adapter {
            name: "ProtonVPN".into(),
            description: "WireGuard Tunnel".into(),
            if_index: "9".into(),
            ipv4: vec![Ipv4Addr::new(10, 2, 0, 2)],
            category: Some("Public".into()),
        }
    }

    fn wifi() -> Adapter {
        Adapter {
            name: "Wi-Fi".into(),
            description: "Intel Wi-Fi 6 AX201".into(),
            if_index: "12".into(),
            ipv4: vec![Ipv4Addr::new(192, 168, 1, 5)],
            category: Some("Private".into()),
        }
    }

    fn route_through(adapter: &Adapter, next_hop: &str) -> Route {
        Route {
            alias: adapter.name.clone(),
            description: adapter.description.clone(),
            local_ip: adapter.ipv4.first().copied(),
            prefix_length: "8".into(),
            next_hop: next_hop.into(),
            destination: "26.0.0.0/8".into(),
            if_index: adapter.if_index.clone(),
        }
    }

    fn records(route: Option<Route>, adapters: Vec<Adapter>) -> Records {
        Records {
            route,
            adapters,
            ..Default::default()
        }
    }

    fn worst(findings: &[Finding]) -> Severity {
        findings
            .iter()
            .map(|f| f.severity)
            .max()
            .unwrap_or(Severity::Ok)
    }

    fn mentions(findings: &[Finding], severity: Severity, text: &str) -> bool {
        findings
            .iter()
            .any(|f| f.severity == severity && f.text.contains(text))
    }

    #[test]
    fn parse_reads_every_record_kind() {
        let text = "\u{FEFF}ROUTE|Radmin VPN|Famatech Radmin VPN Ethernet Adapter|26.0.0.1|8|0.0.0.0|26.0.0.0/8|7\r\n\
            ADAPTER|Radmin VPN|Famatech  VPN  Adapter|7|26.0.0.1,26.0.0.9|Public\r\n\
            ADAPTER|Ethernet|Realtek|3||\r\n\
            FIREWALL|Public|True|NotConfigured\r\n\
            FIREWALL|Private|False|Allow\r\n\
            RULE|C:\\Games\\deadspace3.exe|Dead Space 3|Inbound|Allow|True|Private, Public|UDP|Any\r\n\
            THIRDPARTY|Some Firewall\r\n\
            noise from a module\r\n\
            \r\n";
        let records = Records::parse(text);
        let route = records.route.expect("a route");
        assert_eq!(route.local_ip, Some(OWN));
        assert_eq!(route.next_hop, ON_LINK);
        assert_eq!(route.if_index, "7");
        assert_eq!(records.adapters.len(), 2);
        assert_eq!(
            records.adapters[0].ipv4,
            vec![OWN, Ipv4Addr::new(26, 0, 0, 9)]
        );
        assert_eq!(records.adapters[0].category.as_deref(), Some("Public"));
        assert!(records.adapters[1].ipv4.is_empty());
        assert_eq!(records.adapters[1].category, None);
        assert!(records.firewall[0].enabled && records.firewall[0].blocks_inbound);
        assert!(!records.firewall[1].enabled && !records.firewall[1].blocks_inbound);
        let rule = &records.rules[0];
        assert!(rule.inbound && rule.allow && rule.enabled);
        assert_eq!(rule.profiles, "Private, Public");
        assert_eq!(records.third_party, ["Some Firewall"]);
    }

    #[test]
    fn route_through_the_own_adapter_on_its_network_passes() {
        let radmin = radmin();
        let records = records(Some(route_through(&radmin, ON_LINK)), vec![radmin, wifi()]);
        for own in [Own::Address(OWN), Own::AnyVpn] {
            let findings = route(&records, PEER, &own, Some(OWN));
            assert_eq!(worst(&findings), Severity::Ok, "{own:?}: {findings:?}");
        }
    }

    #[test]
    fn address_no_adapter_holds_blames_no_vpn() {
        let radmin = radmin();
        let records = records(Some(route_through(&radmin, ON_LINK)), vec![radmin]);
        let findings = route(
            &records,
            PEER,
            &Own::Address(Ipv4Addr::new(10, 9, 9, 9)),
            Some(OWN),
        );
        assert!(mentions(
            &findings,
            Severity::Problem,
            "not through the adapter holding 10.9.9.9"
        ));
        assert!(
            !findings.iter().any(|f| f.text.contains("likely cause")),
            "{findings:?}"
        );
    }

    #[test]
    fn no_route_is_a_problem() {
        let findings = route(
            &records(None, vec![radmin()]),
            PEER,
            &Own::Address(OWN),
            None,
        );
        assert!(mentions(&findings, Severity::Problem, "no route"));
    }

    #[test]
    fn route_through_another_adapter_is_a_problem() {
        let wifi = wifi();
        let records = records(
            Some(route_through(&wifi, "192.168.1.1")),
            vec![radmin(), wifi],
        );
        for own in [Own::Address(OWN), Own::AnyVpn] {
            let findings = route(&records, PEER, &own, None);
            assert!(
                mentions(&findings, Severity::Problem, "\"Wi-Fi\""),
                "{own:?}: {findings:?}"
            );
        }
        // Without a chosen address the VPN adapter is the player's own, not
        // the cause.
        let findings = route(&records, PEER, &Own::AnyVpn, None);
        assert!(
            !findings.iter().any(|f| f.text.contains("likely cause")),
            "{findings:?}"
        );
        assert!(mentions(&findings, Severity::Note, "\"Radmin VPN\""));
    }

    #[test]
    fn route_through_another_vpn_adapter_fails_against_the_chosen_address() {
        let other = other_vpn();
        let records = records(Some(route_through(&other, ON_LINK)), vec![radmin(), other]);
        let findings = route(
            &records,
            PEER,
            &Own::Address(OWN),
            Some(Ipv4Addr::new(10, 2, 0, 2)),
        );
        assert!(mentions(
            &findings,
            Severity::Problem,
            "not through the adapter holding 26.0.0.1"
        ));
        assert!(mentions(
            &findings,
            Severity::Warning,
            "\"ProtonVPN\" (WireGuard Tunnel) with 10.2.0.2 is up and is the likely cause"
        ));
        // Any adapter that looks like a VPN passes without a chosen address.
        assert!(!mentions(
            &route(&records, PEER, &Own::AnyVpn, None),
            Severity::Problem,
            ""
        ));
    }

    #[test]
    fn gateway_in_between_is_a_warning() {
        let radmin = radmin();
        let records = records(Some(route_through(&radmin, "26.0.0.254")), vec![radmin]);
        let findings = route(&records, PEER, &Own::Address(OWN), Some(OWN));
        assert!(mentions(&findings, Severity::Warning, "gateway 26.0.0.254"));
        assert_eq!(worst(&findings), Severity::Warning);
    }

    #[test]
    fn source_other_than_the_chosen_address_is_a_warning() {
        let radmin = radmin();
        let records = records(Some(route_through(&radmin, ON_LINK)), vec![radmin]);
        let findings = route(
            &records,
            PEER,
            &Own::Address(OWN),
            Some(Ipv4Addr::new(10, 2, 0, 2)),
        );
        assert!(mentions(
            &findings,
            Severity::Warning,
            "source 10.2.0.2, not 26.0.0.1"
        ));
    }

    #[test]
    fn other_vpn_adapter_is_named_by_route_only_when_it_fails() {
        let radmin = radmin();
        let passing = records(
            Some(route_through(&radmin, ON_LINK)),
            vec![radmin.clone(), other_vpn()],
        );
        let findings = route(&passing, PEER, &Own::Address(OWN), Some(OWN));
        assert_eq!(worst(&findings), Severity::Ok, "{findings:?}");
        assert!(!findings.iter().any(|f| f.text.contains("ProtonVPN")));

        let failing = records(
            Some(route_through(&radmin, "26.0.0.254")),
            vec![radmin, other_vpn()],
        );
        let findings = route(&failing, PEER, &Own::Address(OWN), Some(OWN));
        assert!(mentions(&findings, Severity::Warning, "\"ProtonVPN\""));
    }

    fn setup(own: Own) -> Setup {
        Setup {
            own,
            server: Some(PathBuf::from(r"C:\Games\ds3-blaze\ds3-blaze.exe")),
        }
    }

    fn public_firewall(blocks_inbound: bool) -> Vec<FirewallProfile> {
        vec![FirewallProfile {
            name: "Public".into(),
            enabled: true,
            blocks_inbound,
        }]
    }

    fn rule(program: &str, protocol: &str, allow: bool) -> Rule {
        Rule {
            program: program.into(),
            name: format!("{program} {protocol}"),
            inbound: true,
            allow,
            enabled: true,
            profiles: "Public".into(),
            protocol: protocol.into(),
            local_ports: "Any".into(),
        }
    }

    #[test]
    fn missing_own_adapter_is_a_problem() {
        let records = records(None, vec![wifi()]);
        let findings = local(&records, &setup(Own::Address(OWN)));
        assert!(mentions(
            &findings,
            Severity::Problem,
            "no adapter that is up holds 26.0.0.1"
        ));
        let findings = local(&records, &setup(Own::AnyVpn));
        assert!(mentions(
            &findings,
            Severity::Problem,
            "no VPN adapter is up"
        ));
    }

    #[test]
    fn other_vpn_adapter_is_a_note_of_the_local_part() {
        let records = records(None, vec![radmin(), other_vpn()]);
        let findings = local(&records, &setup(Own::Address(OWN)));
        assert!(mentions(
            &findings,
            Severity::Note,
            "another VPN adapter is up: \"ProtonVPN\""
        ));
    }

    #[test]
    fn public_network_offers_the_private_profile() {
        let records = records(None, vec![radmin()]);
        let findings = local(&records, &setup(Own::Address(OWN)));
        let public = findings
            .iter()
            .find(|f| f.text.contains("as Public"))
            .expect("a Public finding");
        assert_eq!(public.severity, Severity::Note);
        assert_eq!(public.fix, Some(Fix::PrivateProfile(OWN)));
    }

    #[test]
    fn block_rule_is_a_problem() {
        let mut records = records(None, vec![radmin()]);
        records.firewall = public_firewall(true);
        records.rules = vec![
            rule(r"C:\Games\deadspace3.exe", "UDP", true),
            rule(r"D:\Old\DeadSpace3.exe", "Any", false),
        ];
        let findings = local(&records, &setup(Own::Address(OWN)));
        assert!(mentions(
            &findings,
            Severity::Problem,
            "blocks inbound UDP to deadspace3.exe"
        ));
    }

    #[test]
    fn missing_rule_needs_a_fix_only_where_inbound_is_blocked() {
        let server = setup(Own::Address(OWN)).server.expect("a server");
        let mut records = records(None, vec![radmin()]);
        records.firewall = public_firewall(true);
        records.rules = vec![rule(
            &server.display().to_string().to_uppercase(),
            "TCP",
            true,
        )];
        let findings = local(&records, &setup(Own::Address(OWN)));
        let fixes: Vec<(&str, &Option<Fix>)> = findings
            .iter()
            .filter(|f| f.severity == Severity::Warning)
            .map(|f| (f.text.as_str(), &f.fix))
            .collect();
        assert_eq!(fixes.len(), 2, "{fixes:?}");
        assert!(fixes[0].0.contains("inbound UDP to deadspace3.exe"));
        assert_eq!(fixes[0].1, &Some(Fix::GameRule));
        assert!(
            fixes[1]
                .0
                .contains("inbound UDP to C:\\Games\\ds3-blaze\\ds3-blaze.exe")
        );
        assert_eq!(fixes[1].1, &Some(Fix::ServerRule(server)));
        assert!(mentions(&findings, Severity::Ok, "allows inbound TCP"));

        records.firewall = public_firewall(false);
        let findings = local(&records, &setup(Own::Address(OWN)));
        assert!(
            findings.iter().all(|f| f.severity < Severity::Warning),
            "{findings:?}"
        );
    }

    #[test]
    fn rule_for_another_copy_of_the_server_does_not_count() {
        let mut records = records(None, vec![radmin()]);
        records.firewall = public_firewall(true);
        records.rules = vec![rule(r"D:\Old\ds3-blaze.exe", "Any", true)];
        let findings = local(&records, &setup(Own::Address(OWN)));
        assert!(mentions(
            &findings,
            Severity::Warning,
            "inbound TCP to C:\\Games\\ds3-blaze\\ds3-blaze.exe"
        ));
    }

    #[test]
    fn guest_gets_no_server_findings() {
        let mut records = records(None, vec![radmin()]);
        records.firewall = public_firewall(true);
        let guest = Setup {
            own: Own::Address(OWN),
            server: None,
        };
        let findings = local(&records, &guest);
        assert!(!findings.iter().any(|f| f.text.contains("Blaze and QoS")));
        assert!(mentions(&findings, Severity::Warning, "deadspace3.exe"));
    }

    #[test]
    fn port_rule_without_a_program_counts_as_the_games() {
        let guest = Setup {
            own: Own::Address(OWN),
            server: None,
        };
        let mut records = records(None, vec![radmin()]);
        records.firewall = public_firewall(true);
        let mut port_rule = rule("Any", "UDP", true);
        port_rule.local_ports = format!("{} {GAME_PORT}", GAME_PORT - 1);
        records.rules = vec![port_rule.clone()];
        let findings = local(&records, &guest);
        assert!(
            mentions(
                &findings,
                Severity::Ok,
                "allows inbound UDP to deadspace3.exe"
            ),
            "{findings:?}"
        );

        // A port that only starts with the game's does not count.
        port_rule.local_ports = format!("{GAME_PORT}0");
        records.rules = vec![port_rule.clone()];
        assert!(mentions(
            &local(&records, &guest),
            Severity::Warning,
            "deadspace3.exe"
        ));

        // Nor does a rule for another program on the same port.
        port_rule.program = r"C:\Tools\other.exe".into();
        port_rule.local_ports = GAME_PORT.to_string();
        records.rules = vec![port_rule];
        assert!(mentions(
            &local(&records, &guest),
            Severity::Warning,
            "deadspace3.exe"
        ));
    }
}
