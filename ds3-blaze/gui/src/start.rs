//! The start screen: the player's name and VPN address, the hosts state and
//! the choice of mode.

use crate::settings::Mode;
use ds3_blaze_core::hosts::{self, State};
use ds3_blaze_core::players::check_nickname;
use ds3_blaze_core::vpn::{self, Adapter};
use eframe::egui::{self, Color32, RichText};
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
        };
        form.refresh();
        form.address = form
            .adapters
            .iter()
            .flat_map(|adapter| &adapter.ipv4)
            .find(|ip| !ip.is_loopback())
            .copied();
        form
    }

    /// Reads the adapters and the hosts state again, keeping the chosen
    /// address only while an adapter still holds it.
    pub fn refresh(&mut self) {
        self.adapters = vpn::adapters().map(vpn::choices).unwrap_or_default();
        self.hosts = hosts::status().map_err(|err| err.to_string());
        if let Some(address) = self.address
            && !self
                .adapters
                .iter()
                .any(|adapter| adapter.ipv4.contains(&address))
        {
            self.address = None;
        }
    }

    /// Returns what keeps the player from choosing a mode, `None` when nothing
    /// does.
    pub fn problem(&self) -> Option<String> {
        if let Err(reason) = check_nickname(&self.name) {
            return Some(format!("the name is refused: {reason}"));
        }
        if self.address.is_none() {
            return Some("choose your VPN address".into());
        }
        None
    }

    /// Returns whether the hosts file holds ds3-blaze entries, which on the
    /// start screen are left behind by a server or a connection that ended
    /// without restoring them.
    pub fn offers_restore(&self) -> bool {
        self.hosts.as_ref().is_ok_and(points_at_ds3_blaze)
    }

    /// Draws the form and returns what a click asked for; `busy` disables
    /// "Restore EA servers" while a restore runs.
    pub fn show(&mut self, ui: &mut egui::Ui, busy: bool) -> Option<StartClick> {
        let mut click = None;
        ui.heading("ds3-blaze");
        ui.add_space(8.0);

        ui.horizontal(|ui| {
            ui.label("Your name");
            ui.text_edit_singleline(&mut self.name);
        });
        ui.add_space(8.0);

        ui.label("Your VPN address");
        if self.adapters.is_empty() {
            ui.label(
                RichText::new("no adapter with an IPv4 address is up; connect your VPN")
                    .color(Color32::GRAY),
            );
        }
        let chosen_adapter = self
            .address
            .and_then(|address| self.adapters.iter().position(|a| a.ipv4.contains(&address)));
        for (index, adapter) in self.adapters.iter().enumerate() {
            for ip in adapter.ipv4.iter().filter(|ip| !ip.is_loopback()) {
                ui.horizontal(|ui| {
                    ui.radio_value(&mut self.address, Some(*ip), format!("{ip}"));
                    ui.label(RichText::new(&adapter.name).color(Color32::GRAY));
                    if adapter.is_vpn() && chosen_adapter.is_some_and(|chosen| chosen != index) {
                        ui.label(
                            RichText::new("another VPN can take over the other player's traffic")
                                .color(Color32::from_rgb(220, 170, 50)),
                        );
                    }
                });
            }
        }
        if ui.small_button("Read adapters again").clicked() {
            self.refresh();
        }
        ui.add_space(8.0);

        ui.horizontal_wrapped(|ui| {
            ui.label("Hosts file:");
            match &self.hosts {
                Ok(state) => ui.label(state.to_string()),
                Err(err) => ui.label(RichText::new(err).color(Color32::from_rgb(220, 80, 70))),
            };
        });
        if self.offers_restore()
            && ui
                .add_enabled(!busy, egui::Button::new("Restore EA servers"))
                .clicked()
        {
            click = Some(StartClick::RestoreHosts);
        }
        ui.add_space(12.0);

        let problem = self.problem();
        ui.horizontal(|ui| {
            let ready = problem.is_none();
            if ui
                .add_enabled(ready, egui::Button::new("Host a game"))
                .clicked()
            {
                click = Some(StartClick::Mode(Mode::Host));
            }
            if ui
                .add_enabled(ready, egui::Button::new("Join a friend"))
                .clicked()
            {
                click = Some(StartClick::Mode(Mode::Guest));
            }
        });
        if let Some(problem) = problem {
            ui.label(RichText::new(problem).color(Color32::GRAY));
        }
        click
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ds3_blaze_core::hosts::Entry;

    fn form(name: &str, address: Option<Ipv4Addr>, hosts: State) -> Form {
        Form {
            name: name.into(),
            address,
            adapters: Vec::new(),
            hosts: Ok(hosts),
        }
    }

    const OWN: Ipv4Addr = Ipv4Addr::new(26, 0, 0, 1);

    #[test]
    fn the_name_and_the_address_have_to_be_given() {
        let refused = form("Isaac Clarke", Some(OWN), State::EaServers);
        assert!(
            refused
                .problem()
                .expect("a problem")
                .starts_with("the name is refused")
        );
        let without = form("Carver", None, State::EaServers);
        assert_eq!(
            without.problem().as_deref(),
            Some("choose your VPN address")
        );
        assert_eq!(form("Carver", Some(OWN), State::EaServers).problem(), None);
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
