//! Windows network diagnostics for a VPN peer: the route and adapter traffic to
//! the peer takes, other VPN adapters that are up, the network profile, and the
//! Windows Firewall state and rules for the game and this server.
//!
//! The data comes from the NetTCPIP, NetAdapter, NetConnection and NetSecurity
//! PowerShell modules, read by [`SCRIPT`] and printed as `|`-separated records.

use std::net::{Ipv4Addr, UdpSocket};
use std::process::Command;

/// PowerShell script printing one record per line; `-Peer` is the peer address.
///
/// Records: `ROUTE|alias|description|local ip|prefix length|next hop|destination|ifindex`,
/// `CATEGORY|network category`, `ADAPTER|name|description|ifindex|ipv4 list`,
/// `FIREWALL|profile|enabled|default inbound action`,
/// `RULE|program|name|direction|action|enabled|profiles|protocol|local ports`,
/// `THIRDPARTY|product`.
const SCRIPT: &str = r#"param([string]$Peer)
$ErrorActionPreference = 'SilentlyContinue'
[Console]::OutputEncoding = [Text.Encoding]::UTF8
function Clean($value) { "$value" -replace '[|\r\n]', ' ' }
$found = Find-NetRoute -RemoteIPAddress $Peer
$address = $found | Where-Object { $_.CimClass.CimClassName -eq 'MSFT_NetIPAddress' } | Select-Object -First 1
$route = $found | Where-Object { $_.CimClass.CimClassName -eq 'MSFT_NetRoute' } | Select-Object -First 1
if ($route) {
    $nic = Get-NetAdapter -InterfaceIndex $route.ifIndex -IncludeHidden
    "ROUTE|$(Clean $route.InterfaceAlias)|$(Clean $nic.InterfaceDescription)|$(Clean $address.IPAddress)|$(Clean $address.PrefixLength)|$(Clean $route.NextHop)|$(Clean $route.DestinationPrefix)|$($route.ifIndex)"
    $connection = Get-NetConnectionProfile -InterfaceIndex $route.ifIndex
    "CATEGORY|$(Clean $connection.NetworkCategory)"
}
Get-NetAdapter | Where-Object Status -eq 'Up' | ForEach-Object {
    $ips = (Get-NetIPAddress -InterfaceIndex $_.ifIndex -AddressFamily IPv4).IPAddress -join ','
    "ADAPTER|$(Clean $_.Name)|$(Clean $_.InterfaceDescription)|$($_.ifIndex)|$(Clean $ips)"
}
Get-NetFirewallProfile | ForEach-Object { "FIREWALL|$($_.Name)|$($_.Enabled)|$($_.DefaultInboundAction)" }
Get-NetFirewallApplicationFilter | Where-Object { $_.Program -match '(?i)\\(deadspace3|ds3-blaze)\.exe$' } | ForEach-Object {
    $program = $_.Program
    $_ | Get-NetFirewallRule | ForEach-Object {
        $ports = $_ | Get-NetFirewallPortFilter
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

/// Executables whose inbound firewall rules matter, with their role and the
/// protocols they receive: the game on both PCs, this server on the host.
const PROGRAMS: [(&str, &str, &[&str]); 2] = [
    (
        "deadspace3.exe",
        "the game's P2P traffic, both PCs",
        &["UDP"],
    ),
    ("ds3-blaze.exe", "Blaze and QoS, host only", &["TCP", "UDP"]),
];

/// Route to the peer as Windows chooses it.
struct Route {
    alias: String,
    description: String,
    local_ip: String,
    prefix_length: String,
    next_hop: String,
    destination: String,
    if_index: String,
}

/// A network adapter that is up.
struct Adapter {
    name: String,
    description: String,
    if_index: String,
    ipv4: String,
}

/// Windows Firewall state of one profile.
struct FirewallProfile {
    name: String,
    enabled: bool,
    /// `true` unless the default inbound action is `Allow`.
    blocks_inbound: bool,
}

/// A Windows Firewall rule bound to one of [`PROGRAMS`].
struct Rule {
    program: String,
    name: String,
    inbound: bool,
    allow: bool,
    enabled: bool,
    profiles: String,
    /// `Any`, `TCP`, `UDP` or a protocol number.
    protocol: String,
    local_ports: String,
}

impl Rule {
    /// Returns whether the rule matches protocol `protocol` (`TCP` or `UDP`).
    fn covers(&self, protocol: &str) -> bool {
        self.protocol.eq_ignore_ascii_case("any") || self.protocol.eq_ignore_ascii_case(protocol)
    }
}

/// Everything [`SCRIPT`] reports.
#[derive(Default)]
struct Report {
    route: Option<Route>,
    category: Option<String>,
    adapters: Vec<Adapter>,
    firewall: Vec<FirewallProfile>,
    rules: Vec<Rule>,
    third_party: Vec<String>,
}

/// Outcome counts of a diagnosis.
#[derive(Default)]
pub struct Verdict {
    pub problems: u32,
    pub warnings: u32,
}

impl Verdict {
    fn problem(&mut self, text: &str) {
        self.problems += 1;
        println!("  [problem] {text}");
    }

    fn warning(&mut self, text: &str) {
        self.warnings += 1;
        println!("  [warning] {text}");
    }
}

fn ok(text: &str) {
    println!("  [ok] {text}");
}

/// Prints the route, adapters and firewall findings for traffic to `peer` and
/// returns the counts; fails when PowerShell cannot run.
pub fn diagnose(peer: Ipv4Addr) -> std::io::Result<Verdict> {
    println!("network check for {peer} (reading Windows settings, a few seconds)");
    let report = collect(peer)?;
    let mut verdict = Verdict::default();
    check_route(peer, &report, &mut verdict);
    check_firewall(&report, &mut verdict);
    println!(
        "network check: {} problem(s), {} warning(s)",
        verdict.problems, verdict.warnings
    );
    Ok(verdict)
}

/// Runs [`SCRIPT`] for `peer` and parses its records.
fn collect(peer: Ipv4Addr) -> std::io::Result<Report> {
    let path = std::env::temp_dir().join(format!("ds3-blaze-netcheck-{}.ps1", std::process::id()));
    std::fs::write(&path, SCRIPT)?;
    let output = Command::new("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-File",
        ])
        .arg(&path)
        .args(["-Peer", &peer.to_string()])
        .output();
    let _ = std::fs::remove_file(&path);
    let output = output?;
    let text = String::from_utf8_lossy(&output.stdout);
    let mut report = Report::default();
    for line in text.lines() {
        let fields: Vec<String> = line
            .trim()
            .split('|')
            .map(|f| f.trim().to_string())
            .collect();
        let field = |i: usize| fields.get(i).cloned().unwrap_or_default();
        match fields[0].as_str() {
            "ROUTE" => {
                report.route = Some(Route {
                    alias: field(1),
                    description: field(2),
                    local_ip: field(3),
                    prefix_length: field(4),
                    next_hop: field(5),
                    destination: field(6),
                    if_index: field(7),
                })
            }
            "CATEGORY" => report.category = Some(field(1)),
            "ADAPTER" => report.adapters.push(Adapter {
                name: field(1),
                description: field(2),
                if_index: field(3),
                ipv4: field(4),
            }),
            "FIREWALL" => report.firewall.push(FirewallProfile {
                name: field(1),
                enabled: field(2).eq_ignore_ascii_case("true"),
                blocks_inbound: !field(3).eq_ignore_ascii_case("allow"),
            }),
            "RULE" => report.rules.push(Rule {
                program: field(1),
                name: field(2),
                inbound: field(3).eq_ignore_ascii_case("inbound"),
                allow: field(4).eq_ignore_ascii_case("allow"),
                enabled: field(5).eq_ignore_ascii_case("true"),
                profiles: field(6),
                protocol: field(7),
                local_ports: field(8),
            }),
            "THIRDPARTY" => report.third_party.push(field(1)),
            _ => {}
        }
    }
    Ok(report)
}

/// Returns whether an adapter name or description looks like a VPN adapter.
fn is_vpn(name: &str, description: &str) -> bool {
    let text = format!("{name} {description}").to_lowercase();
    VPN_MARKERS.iter().any(|marker| text.contains(marker))
}

/// Returns the source address Windows picks for UDP to `peer`, without sending.
fn source_ip(peer: Ipv4Addr) -> Option<Ipv4Addr> {
    let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).ok()?;
    socket.connect((peer, 9)).ok()?;
    match socket.local_addr().ok()?.ip() {
        std::net::IpAddr::V4(ip) => Some(ip),
        std::net::IpAddr::V6(_) => None,
    }
}

fn check_route(peer: Ipv4Addr, report: &Report, verdict: &mut Verdict) {
    println!("route:");
    let Some(route) = &report.route else {
        verdict.problem(&format!(
            "Windows has no route to {peer}: the VPN is not connected or the address is wrong"
        ));
        return;
    };
    println!(
        "  traffic to {peer} leaves through \"{}\" ({}) from {}/{}, route {} via {}",
        route.alias,
        route.description,
        route.local_ip,
        route.prefix_length,
        route.destination,
        route.next_hop
    );
    if let Some(source) = source_ip(peer)
        && source.to_string() != route.local_ip
    {
        verdict.warning(&format!(
            "a test socket picks source {source}, not {}: another tunnel may intercept traffic",
            route.local_ip
        ));
    }
    let others: Vec<&Adapter> = report
        .adapters
        .iter()
        .filter(|a| a.if_index != route.if_index && is_vpn(&a.name, &a.description))
        .collect();
    if !is_vpn(&route.alias, &route.description) {
        verdict.problem(
            "the route goes through an adapter that is not a known VPN: the peer address is not on the VPN, \
             or the VPN adapter is down",
        );
        for adapter in &others {
            println!(
                "  VPN adapter \"{}\" ({}) is up with {}: the peer's address should be on its network",
                adapter.name, adapter.description, adapter.ipv4
            );
        }
        if others.is_empty() {
            verdict.problem("no VPN adapter is up");
        }
        return;
    }
    ok("the route goes through a VPN adapter");
    let on_link = route.next_hop == "0.0.0.0";
    if on_link {
        ok("the peer is on the adapter's own network (no gateway in between)");
    } else {
        verdict.warning(&format!(
            "the route passes gateway {}: the peer is not on the same VPN network as this PC, \
             or another VPN routes this address",
            route.next_hop
        ));
    }
    for adapter in &others {
        println!(
            "  another VPN adapter is up: \"{}\" ({}) {}",
            adapter.name, adapter.description, adapter.ipv4
        );
    }
    if !others.is_empty() {
        if on_link {
            ok("traffic to the peer does not use the other VPN adapters");
        } else {
            verdict.warning("another VPN is active and may route the peer's address; disconnect it and check again");
        }
    }
}

/// Returns the firewall profile name Windows applies to network category `category`.
fn profile_of(category: &str) -> &'static str {
    match category {
        "Private" => "Private",
        "DomainAuthenticated" => "Domain",
        _ => "Public",
    }
}

fn check_firewall(report: &Report, verdict: &mut Verdict) {
    println!("firewall:");
    let profile = report.category.as_deref().map(profile_of);
    if let Some(category) = &report.category {
        println!(
            "  the network the route uses is in category {category}, firewall profile {}",
            profile_of(category)
        );
    }
    for product in &report.third_party {
        verdict.warning(&format!(
            "third-party firewall \"{product}\" is registered; its rules are not checked here"
        ));
    }
    let Some(profile) = profile else {
        verdict.warning("network category unknown, firewall rules are not matched to a profile");
        return;
    };
    let Some(state) = report
        .firewall
        .iter()
        .find(|p| p.name.eq_ignore_ascii_case(profile))
    else {
        verdict.warning("Windows Firewall state unreadable");
        return;
    };
    if !state.enabled {
        ok(&format!(
            "Windows Firewall is off for the {profile} profile"
        ));
        return;
    }
    println!(
        "  Windows Firewall is on for the {profile} profile, unsolicited inbound traffic is {}",
        if state.blocks_inbound {
            "blocked by default"
        } else {
            "allowed by default"
        }
    );
    for (program, role, protocols) in PROGRAMS {
        let applies = |rule: &&Rule| {
            rule.inbound
                && rule.enabled
                && rule.program.to_lowercase().ends_with(program)
                && (rule.profiles.eq_ignore_ascii_case("any") || rule.profiles.contains(profile))
        };
        let rules: Vec<&Rule> = report.rules.iter().filter(applies).collect();
        for protocol in protocols {
            let covering: Vec<&&Rule> = rules.iter().filter(|r| r.covers(protocol)).collect();
            if let Some(block) = covering.iter().find(|r| !r.allow) {
                verdict.problem(&format!(
                    "rule \"{}\" blocks inbound {protocol} to {program} ({role}) in the {profile} profile; \
                     block rules win over allow rules, disable or delete it",
                    block.name
                ));
            } else if let Some(allow) = covering.first() {
                let ports = if allow.local_ports.eq_ignore_ascii_case("any")
                    || allow.local_ports.is_empty()
                {
                    String::new()
                } else {
                    format!(", local ports {}", allow.local_ports)
                };
                ok(&format!(
                    "rule \"{}\" allows inbound {protocol} to {program} ({role}){ports}",
                    allow.name
                ));
            } else if state.blocks_inbound {
                verdict.warning(&format!(
                    "no rule allows inbound {protocol} to {program} ({role}) in the {profile} profile; \
                     allow it when Windows asks, or add a rule"
                ));
            }
        }
    }
}
