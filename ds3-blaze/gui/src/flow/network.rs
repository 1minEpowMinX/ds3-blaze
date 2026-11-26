//! The network check both modes run: this PC's settings and the route to the
//! other player.

use ds3_blaze_diag::netcheck::{self, Finding, Own, Setup};
use std::io;
use std::net::Ipv4Addr;
use std::path::PathBuf;

/// Result of a network check.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Network {
    /// Findings about this PC.
    pub local: Vec<Finding>,
    /// Findings about the route to the other player, empty without one.
    pub route: Vec<Finding>,
    /// Name of the adapter the route takes, or without another player the
    /// adapter holding this PC's address.
    pub adapter: Option<String>,
    /// Names of the VPN adapters that are up besides this PC's own.
    pub other_vpn: Vec<String>,
}

/// Reads this PC's settings and checks them for the adapter holding `own`,
/// with the route to `peer` when one is given; `server` is the executable
/// that runs the server on this PC, `None` on a guest's.
///
/// # Errors
///
/// Returns the error of reading the settings through PowerShell.
pub fn check(
    own: Ipv4Addr,
    peer: Option<Ipv4Addr>,
    server: Option<PathBuf>,
) -> io::Result<Network> {
    let records = netcheck::read(peer)?;
    let holder = records
        .adapters
        .iter()
        .find(|adapter| adapter.ipv4.contains(&own))
        .map(|adapter| adapter.name.clone());
    let own = Own::Address(own);
    let setup = Setup { own, server };
    Ok(Network {
        local: netcheck::local(&records, &setup),
        route: peer
            .map(|peer| netcheck::route(&records, peer, &own, netcheck::source_ip(peer)))
            .unwrap_or_default(),
        adapter: match peer {
            Some(_) => records.route.as_ref().map(|route| route.alias.clone()),
            None => holder,
        },
        other_vpn: netcheck::other_vpn_adapters(&records, &own)
            .into_iter()
            .map(|adapter| adapter.name.clone())
            .collect(),
    })
}
