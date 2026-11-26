//! What the start screen holds: the player's name and VPN address, chosen
//! once for both modes, and what the screen can ask for.

use crate::settings::Mode;
use ds3_blaze_core::hosts::{self, State};
use ds3_blaze_core::players::check_nickname;
use ds3_blaze_core::vpn::{self, Adapter};
use std::net::Ipv4Addr;

/// Returns whether the hosts file holds ds3-blaze entries.
pub fn points_at_ds3_blaze(state: &State) -> bool {
    matches!(
        state,
        State::Redirected(_) | State::Overridden { ours: Some(_), .. }
    )
}

/// What a click on the start screen asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StartClick {
    Mode(Mode),
    RestoreHosts,
    /// The adapters and the hosts state read again.
    ReadAdapters,
}

/// Contents of the start screen.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Form {
    pub name: String,
    /// The VPN address chosen, `None` until one is.
    pub address: Option<Ipv4Addr>,
    /// The adapters to choose from, as [`vpn::choices`] orders them.
    pub adapters: Vec<Adapter>,
    /// The hosts state, or the error of reading the hosts file.
    pub hosts: Result<State, String>,
    /// Whether the list of addresses is open.
    pub choosing: bool,
    /// Whether the list shows the adapters [`Adapter::is_vpn`] does not
    /// recognise.
    pub all: bool,
}

impl Form {
    /// Returns a form for `name` with this PC's adapters and hosts state, the
    /// first address of the first adapter chosen.
    pub fn read(name: String) -> Form {
        let mut form = Form {
            name,
            address: None,
            adapters: Vec::new(),
            hosts: Err(String::new()),
            choosing: false,
            all: false,
        };
        form.refresh();
        form
    }

    /// Reads the adapters and the hosts state again, keeping the chosen
    /// address while an adapter still holds it and taking the first address
    /// otherwise.
    pub fn refresh(&mut self) {
        self.adapters = vpn::adapters().map(vpn::choices).unwrap_or_default();
        self.hosts = hosts::status().map_err(|err| err.to_string());
        if self.holder().is_none() {
            self.address = addresses(&self.adapters).next().map(|(ip, _)| ip);
        }
    }

    /// Returns the adapter holding the chosen address.
    pub fn holder(&self) -> Option<&Adapter> {
        let address = self.address?;
        self.adapters
            .iter()
            .find(|adapter| adapter.ipv4.contains(&address))
    }

    /// Returns the addresses the list offers, each with its adapter: those of
    /// VPN adapters, and of the adapter holding the chosen address, unless
    /// `all` shows every adapter's.
    pub fn shown(&self) -> Vec<(Ipv4Addr, &Adapter)> {
        let holder = self.holder();
        addresses(&self.adapters)
            .filter(|(_, adapter)| {
                self.all || adapter.is_vpn() || holder.is_some_and(|holder| holder == *adapter)
            })
            .collect()
    }

    /// Returns the first VPN adapter that is up other than the one holding
    /// the chosen address.
    pub fn other_vpn(&self) -> Option<&Adapter> {
        let holder = self.holder();
        self.adapters.iter().find(|adapter| {
            adapter.up && adapter.is_vpn() && holder.is_none_or(|holder| holder != *adapter)
        })
    }

    /// Returns why the name can't be used, `None` when it can.
    pub fn name_problem(&self) -> Option<String> {
        check_nickname(&self.name)
            .err()
            .map(|reason| format!("This name can't be used: {reason}"))
    }

    /// Returns why no address is chosen, `None` once one is.
    pub fn address_problem(&self) -> Option<String> {
        if addresses(&self.adapters).next().is_none() {
            return Some("Connect your VPN first".into());
        }
        self.address
            .is_none()
            .then(|| "Choose your VPN address".into())
    }

    /// Returns whether the hosts file holds ds3-blaze entries, which on the
    /// start screen are left behind by a server or a connection that ended
    /// without restoring them.
    pub fn offers_restore(&self) -> bool {
        self.hosts.as_ref().is_ok_and(points_at_ds3_blaze)
    }
}

/// Returns the addresses of `adapters` other than loopback ones, each with
/// its adapter.
fn addresses(adapters: &[Adapter]) -> impl Iterator<Item = (Ipv4Addr, &Adapter)> {
    adapters.iter().flat_map(|adapter| {
        adapter
            .ipv4
            .iter()
            .filter(|ip| !ip.is_loopback())
            .map(move |ip| (*ip, adapter))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use ds3_blaze_core::hosts::Entry;

    const OWN: Ipv4Addr = Ipv4Addr::new(26, 0, 0, 1);
    const WIFI: Ipv4Addr = Ipv4Addr::new(192, 168, 1, 20);
    const ZEROTIER: Ipv4Addr = Ipv4Addr::new(10, 147, 0, 5);

    fn form(name: &str, address: Option<Ipv4Addr>, hosts: State) -> Form {
        Form {
            name: name.into(),
            address,
            adapters: vec![adapter("Radmin VPN", "Famatech Radmin VPN", OWN)],
            hosts: Ok(hosts),
            choosing: false,
            all: false,
        }
    }

    fn adapter(name: &str, description: &str, ip: Ipv4Addr) -> Adapter {
        Adapter {
            name: name.into(),
            description: description.into(),
            ipv4: vec![ip],
            up: true,
        }
    }

    #[test]
    fn a_refused_name_is_its_own_problem() {
        let refused = form("Isaac Clarke", Some(OWN), State::EaServers);
        assert!(
            refused
                .name_problem()
                .expect("a problem")
                .starts_with("This name can't be used")
        );
        assert_eq!(refused.address_problem(), None);
        let carver = form("Carver", Some(OWN), State::EaServers);
        assert_eq!(
            (carver.name_problem(), carver.address_problem()),
            (None, None)
        );
    }

    #[test]
    fn without_an_adapter_the_vpn_is_to_be_connected() {
        let mut without = form("Carver", None, State::EaServers);
        assert_eq!(
            without.address_problem().as_deref(),
            Some("Choose your VPN address")
        );
        without.adapters.clear();
        assert_eq!(
            without.address_problem().as_deref(),
            Some("Connect your VPN first")
        );
    }

    #[test]
    fn shown_lists_vpn_adapters_unless_all() {
        let mut form = form("Carver", Some(OWN), State::EaServers);
        form.adapters.push(adapter("Wi-Fi", "Intel Wireless", WIFI));
        let shown =
            |form: &Form| -> Vec<Ipv4Addr> { form.shown().into_iter().map(|(ip, _)| ip).collect() };
        assert_eq!(shown(&form), [OWN]);
        form.all = true;
        assert_eq!(shown(&form), [OWN, WIFI]);
    }

    #[test]
    fn another_vpn_is_named_once() {
        let mut form = form("Carver", Some(OWN), State::EaServers);
        assert_eq!(form.other_vpn(), None);
        form.adapters
            .push(adapter("ZeroTier One", "ZeroTier Virtual Port", ZEROTIER));
        assert_eq!(
            form.other_vpn().map(|adapter| adapter.name.as_str()),
            Some("ZeroTier One")
        );
    }

    #[test]
    fn restore_is_offered_while_ds3_blaze_entries_are_present() {
        let entry = Entry {
            line: 3,
            text: "127.0.0.1 gosredirector.ea.com".into(),
            address: Ipv4Addr::LOCALHOST,
        };
        assert!(form("Carver", None, State::Redirected(OWN)).offers_restore());
        assert!(
            form(
                "Carver",
                None,
                State::Overridden {
                    entry: entry.clone(),
                    ours: Some(OWN)
                }
            )
            .offers_restore()
        );
        assert!(!form("Carver", None, State::Overridden { entry, ours: None }).offers_restore());
        assert!(!form("Carver", None, State::EaServers).offers_restore());
    }
}
