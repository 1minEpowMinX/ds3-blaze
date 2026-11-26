//! The start screen: the player's name and VPN address, chosen once for
//! both modes, and the choice of mode.

use crate::row;
use crate::settings::Mode;
use crate::step::{BUSY, Button, Click};
use crate::theme::{self, ACCENT, ORANGE, RED, STRONG, TEXT, WEAK};
use ds3_blaze_core::hosts::{self, State};
use ds3_blaze_core::players::check_nickname;
use ds3_blaze_core::vpn::{self, Adapter};
use eframe::egui::{
    self, Align, CursorIcon, Label, Layout, Margin, RichText, Sense, Stroke, pos2, vec2,
};
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
    fn holder(&self) -> Option<&Adapter> {
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

    /// Draws the form and returns what a click asked for; `busy` disables
    /// "Restore EA servers" while a restore runs.
    pub fn show(&mut self, ui: &mut egui::Ui, busy: bool) -> Option<StartClick> {
        let mut click = None;
        ui.add_space(24.0);
        ui.label(RichText::new("ds3-blaze").size(30.0).color(STRONG));
        ui.label(RichText::new("Dead Space 3 co-op over a VPN").color(WEAK));

        theme::section(ui, "You");
        let mut name_problem = None;
        let mut address_problem = None;
        theme::card().inner_margin(Margin::same(18)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            caption(ui, "Name in the game");
            ui.add(
                egui::TextEdit::singleline(&mut self.name)
                    .desired_width(f32::INFINITY)
                    .margin(vec2(10.0, 7.0)),
            );
            name_problem = self.name_problem();
            if let Some(problem) = &name_problem {
                ui.label(RichText::new(problem).color(RED));
            }
            ui.add_space(12.0);
            caption(ui, "VPN address");
            address_problem = self.address(ui);
        });
        ui.add_space(6.0);

        theme::section(ui, "Play");
        let ready = name_problem.is_none() && address_problem.is_none();
        let spacing = 12.0;
        let width = (ui.available_width() - spacing) / 2.0;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = spacing;
            if card(ui, width, "Host a game", "Friends join your PC", ready) {
                click = Some(StartClick::Mode(Mode::Host));
            }
            if card(
                ui,
                width,
                "Join a friend",
                "Type the host's address next",
                ready,
            ) {
                click = Some(StartClick::Mode(Mode::Guest));
            }
        });

        if self.offers_restore() {
            ui.add_space(16.0);
            let restore = Button::elevated("Restore EA servers", Click::RestoreHosts)
                .disabled_by(busy.then_some(BUSY));
            theme::card()
                .fill(theme::SURFACE.lerp_to_gamma(ORANGE, 0.07))
                .stroke(Stroke::new(1.0, ORANGE.gamma_multiply(0.35)))
                .inner_margin(Margin::same(16))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    let (_, pressed) = egui::Sides::new().shrink_left().show(
                        ui,
                        |ui| {
                            ui.add(
                                Label::new(
                                    RichText::new(
                                        "The game still goes to ds3-blaze, not to the EA servers",
                                    )
                                    .color(TEXT),
                                )
                                .wrap(),
                            );
                        },
                        |ui| row::action(ui, &restore, 32.0),
                    );
                    if pressed {
                        click = Some(StartClick::RestoreHosts);
                    }
                });
        }
        click
    }

    /// Draws the chosen address with Change, or why none is chosen, and the
    /// list while it is open; returns why no address is chosen.
    fn address(&mut self, ui: &mut egui::Ui) -> Option<String> {
        let address_problem = self.address_problem();
        match (&address_problem, self.holder()) {
            (None, Some(holder)) => {
                let name = holder.name.clone();
                let address = self.address.expect("a holder holds the address");
                ui.horizontal(|ui| {
                    ui.label(RichText::new(address.to_string()).size(16.0).color(STRONG));
                    ui.label(RichText::new(name).color(WEAK));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let label = if self.choosing { "Done" } else { "Change" };
                        if ui.small_button(label).clicked() {
                            self.choosing = !self.choosing;
                        }
                    });
                });
            }
            (problem, _) => {
                ui.horizontal(|ui| {
                    if let Some(problem) = problem {
                        ui.label(RichText::new(problem).color(ORANGE));
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.small_button("Read adapters again").clicked() {
                            self.refresh();
                        }
                    });
                });
            }
        }
        if self.choosing && address_problem.is_none() {
            ui.add_space(6.0);
            self.address_list(ui);
        }
        if let Some(other) = self.other_vpn() {
            ui.add_space(6.0);
            ui.add(
                Label::new(
                    RichText::new(format!(
                        "{} is up too and can take over the other player's traffic",
                        other.name
                    ))
                    .color(ORANGE),
                )
                .wrap(),
            );
        }
        address_problem
    }

    /// Draws the open list of addresses with the buttons that widen and read
    /// it again.
    fn address_list(&mut self, ui: &mut egui::Ui) {
        let mut chosen = self.address;
        theme::inset(10).show(ui, |ui| {
            ui.set_width(ui.available_width());
            for (ip, adapter) in self.shown() {
                ui.horizontal(|ui| {
                    ui.radio_value(&mut chosen, Some(ip), ip.to_string());
                    ui.label(RichText::new(&adapter.name).color(WEAK));
                });
            }
            ui.horizontal(|ui| {
                if !self.all && ui.small_button("Show all adapters").clicked() {
                    self.all = true;
                }
                if ui.small_button("Read adapters again").clicked() {
                    self.refresh();
                }
            });
        });
        self.address = chosen;
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

/// Draws the caption over a field.
fn caption(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text).small().color(WEAK));
    ui.add_space(2.0);
}

/// Draws a mode's card `width` points wide and returns whether it was
/// clicked; a card that is not `enabled` is drawn faded and takes no click.
fn card(ui: &mut egui::Ui, width: f32, title: &str, subtitle: &str, enabled: bool) -> bool {
    ui.add_enabled_ui(enabled, |ui| {
        let id = egui::Id::unique(("mode card", title));
        let hovered = enabled && ui.ctx().read_response(id).is_some_and(|r| r.hovered());
        let frame = theme::card()
            .fill(if hovered {
                theme::SURFACE.lerp_to_gamma(ACCENT, 0.06)
            } else {
                theme::SURFACE
            })
            .stroke(Stroke::new(1.0, if hovered { ACCENT } else { theme::LINE }))
            .inner_margin(Margin::same(20))
            .show(ui, |ui| {
                // The margins and the border take the rest of `width`.
                ui.set_width(width - 42.0);
                ui.set_min_height(64.0);
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        let title = RichText::new(title).size(18.0).color(STRONG);
                        ui.add(Label::new(title).selectable(false));
                        ui.add_space(2.0);
                        ui.add(
                            Label::new(RichText::new(subtitle).color(WEAK))
                                .selectable(false)
                                .wrap(),
                        );
                    });
                });
            });
        let rect = frame.response.rect;
        let arrow = pos2(rect.right() - 26.0, rect.center().y);
        let colour = if hovered { ACCENT } else { WEAK };
        ui.painter().line(
            vec![
                arrow + vec2(-4.0, -5.0),
                arrow + vec2(1.0, 0.0),
                arrow + vec2(-4.0, 5.0),
            ],
            Stroke::new(2.0, colour),
        );
        let response = ui.interact(rect, id, Sense::click());
        // Screen readers and UI Automation see the card as a button.
        response
            .widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, ui.is_enabled(), title));
        response.on_hover_cursor(CursorIcon::PointingHand).clicked()
    })
    .inner
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
