//! The window's state and its drawing: the start screen, then the chosen
//! mode's steps, with the background work they start.

use crate::elevate;
use crate::guest::{GuestAction, GuestFlow, GuestInput};
use crate::host::{HostAction, HostFlow, HostInput};
use crate::row;
use crate::run;
use crate::settings::{self, Mode, Settings};
use crate::start::{self, Form, StartClick};
use crate::step::{Click, Footer, Step};
use ds3_blaze_core::event::Event;
use ds3_blaze_core::hosts;
use ds3_blaze_core::log;
use ds3_blaze_core::server::{Config, Ports, Server, ServerHandle};
use ds3_blaze_core::vpn::Adapter;
use eframe::egui::{self, Margin, RichText, vec2};
use std::future::Future;
use std::io;
use std::net::Ipv4Addr;
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::{Duration, Instant};
use tokio::sync::broadcast::error::RecvError;

/// How often a running countdown is drawn again.
const TICK: Duration = Duration::from_secs(1);

/// Colour of errors the window shows.
const ERROR: egui::Color32 = row::RED;

/// What the window shows.
enum Screen {
    Start(Form),
    Guest {
        flow: Box<GuestFlow>,
        own: Ipv4Addr,
        /// The host's address as typed.
        host: String,
        /// Why the typed address was refused.
        refused: Option<String>,
    },
    Host {
        flow: Box<HostFlow>,
        /// The adapters the host's address is chosen from.
        adapters: Vec<Adapter>,
    },
}

/// What the player did on a mode's screen during a frame.
#[derive(Default)]
struct ModeInput {
    click: Option<Click>,
    back: bool,
    /// The VPN address the host chose in place of the current one.
    own: Option<Ipv4Addr>,
}

/// Result of work the window started in the background.
enum Done {
    /// `hosts disable` from the start screen or the closing dialog ended,
    /// with the command line's exit code.
    Restored(io::Result<i32>),
    Guest(GuestInput),
    Host(HostInput),
    ServerStarted(io::Result<ServerHandle>),
    /// The running server's state changed.
    Changed,
}

/// Where closing the window stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Closing {
    /// No close was asked for.
    No,
    /// The dialog asks whether to restore the EA servers first.
    Asking,
    /// The EA servers are being restored before the window closes.
    Restoring,
    /// The next close request goes through.
    Allowed,
}

/// The window.
pub struct App {
    settings: Settings,
    settings_path: PathBuf,
    screen: Screen,
    /// Whether each row of the mode's steps is open, by position.
    open: Vec<bool>,
    /// The error of saving the settings last, shown until a save succeeds.
    save_error: Option<String>,
    runtime: tokio::runtime::Runtime,
    done_sender: mpsc::Sender<Done>,
    done: mpsc::Receiver<Done>,
    /// The host's running server.
    server: Option<ServerHandle>,
    /// The log file, or the error of opening it.
    log: Result<PathBuf, String>,
    /// Whether a restore from the start screen or the closing dialog runs.
    busy: bool,
    /// What that restore or the last window-side action left to say.
    message: Option<String>,
    closing: Closing,
}

impl App {
    /// Returns the window with the settings from [`settings::path`] and the
    /// start screen, logging to a new file.
    ///
    /// # Errors
    ///
    /// Returns the error of starting the runtime the background work uses.
    pub fn new() -> io::Result<App> {
        let settings_path = settings::path();
        let settings = Settings::load(&settings_path);
        let form = Form::read(settings.name.clone());
        let (done_sender, done) = mpsc::channel();
        let app = App {
            settings,
            settings_path,
            screen: Screen::Start(form),
            open: Vec::new(),
            save_error: None,
            runtime: tokio::runtime::Runtime::new()?,
            done_sender,
            done,
            server: None,
            log: log::init().map_err(|err| format!("the log file cannot be opened: {err}")),
            busy: false,
            message: None,
            closing: Closing::No,
        };
        #[cfg(debug_assertions)]
        let app = app.preview();
        Ok(app)
    }

    /// Returns the window showing the preview [`preview::VARIABLE`] names,
    /// with the settings kept in a file of its own, or the window unchanged
    /// when it names no mode.
    #[cfg(debug_assertions)]
    fn preview(mut self) -> App {
        use crate::preview;
        let (screen, name, open) = match std::env::var(preview::VARIABLE).as_deref() {
            Ok("host") => (
                Screen::Host {
                    flow: Box::new(preview::host()),
                    adapters: preview::adapters(preview::HOST),
                },
                "Isaac",
                3,
            ),
            Ok("guest") => (
                Screen::Guest {
                    flow: Box::new(preview::guest()),
                    own: preview::GUEST,
                    host: preview::HOST.to_string(),
                    refused: None,
                },
                "Carver",
                3,
            ),
            _ => return self,
        };
        self.settings_path = std::env::temp_dir().join("ds3-blaze-preview.toml");
        self.settings.name = name.into();
        self.screen = screen;
        self.open = vec![false; 8];
        self.open[open] = true;
        self
    }

    /// Returns a function that hands a result to the next frame, which it
    /// asks egui to draw.
    fn sink<T: 'static>(
        &self,
        ctx: &egui::Context,
        wrap: fn(T) -> Done,
    ) -> impl Fn(T) + Send + Sync + Clone + 'static {
        let sender = self.done_sender.clone();
        let ctx = ctx.clone();
        move |value| {
            // The receiver lives as long as the window.
            let _ = sender.send(wrap(value));
            ctx.request_repaint();
        }
    }

    /// Runs `work` on the runtime.
    fn spawn(&self, work: impl Future<Output = ()> + Send + 'static) {
        self.runtime.spawn(work);
    }

    /// Starts `hosts disable` through the command line, elevated.
    fn restore_hosts(&mut self, ctx: &egui::Context) {
        self.busy = true;
        self.message = None;
        let send = self.sink(ctx, Done::Restored);
        self.spawn(async move { send(elevate::run_cli(elevate::hosts_args(None)).await) });
    }

    /// Carries out the guest's `actions`.
    fn run_guest(&self, ctx: &egui::Context, actions: Vec<GuestAction>) {
        for action in actions {
            let send = self.sink(ctx, Done::Guest);
            self.spawn(run::guest(action, send));
        }
    }

    /// Carries out the host's `actions`: those on the server and the
    /// settings here, the others on the runtime.
    fn run_host(&mut self, ctx: &egui::Context, actions: Vec<HostAction>) {
        for action in actions {
            match action {
                HostAction::StartServer { advertise, roster } => {
                    let send = self.sink(ctx, Done::ServerStarted);
                    self.spawn(async move {
                        send(
                            Server::start(Config {
                                // The host's game connects through 127.0.0.1,
                                // the guest's through the VPN address.
                                bind: Ipv4Addr::UNSPECIFIED,
                                advertise,
                                ports: Ports::default(),
                                players: roster,
                            })
                            .await,
                        )
                    });
                }
                HostAction::Rename(ip, name) => {
                    if let Some(server) = &self.server {
                        server.players().set_name(ip, name);
                    }
                }
                HostAction::Allow(ip) => {
                    if let Some(server) = &self.server {
                        server.players().set_allowed(ip, true);
                    }
                }
                HostAction::OnlyAllowed(on) => {
                    if let Some(server) = &self.server {
                        server.players().set_only_allowed(on);
                    }
                }
                HostAction::SaveNames(names) => {
                    self.settings.players = names;
                    self.save();
                }
                HostAction::OpenLogs => self.open_logs(),
                HostAction::StopAndRestore => {
                    if let Some(server) = self.server.take() {
                        server.stop();
                    }
                    let send = self.sink(ctx, Done::Host);
                    self.spawn(async move {
                        send(HostInput::Elevated(
                            elevate::run_cli(elevate::hosts_args(None)).await,
                        ))
                    });
                }
                action => {
                    let send = self.sink(ctx, Done::Host);
                    self.spawn(run::host(action, send));
                }
            }
        }
    }

    /// Opens the folder of the log file in Explorer.
    fn open_logs(&mut self) {
        let folder = self
            .log
            .as_ref()
            .ok()
            .and_then(|path| path.parent().map(PathBuf::from));
        self.message = match folder {
            Some(folder) => std::process::Command::new("explorer")
                .arg(&folder)
                .spawn()
                .err()
                .map(|err| format!("{} cannot be opened: {err}", folder.display())),
            None => self.log.as_ref().err().cloned(),
        };
    }

    /// Keeps the started server and forwards its events to the next frames.
    fn server_started(&mut self, ctx: &egui::Context, result: io::Result<ServerHandle>) {
        let Screen::Host { flow, .. } = &mut self.screen else {
            return;
        };
        let now = Instant::now();
        let handle = match result {
            Ok(handle) => handle,
            Err(err) => {
                let actions = flow.apply(HostInput::Started(Err(err)), now);
                self.run_host(ctx, actions);
                return;
            }
        };
        let mut events = handle.events();
        let mut actions = flow.apply(HostInput::Started(Ok(handle.ports())), now);
        actions.extend(flow.apply(HostInput::Snapshot(handle.snapshot()), now));
        self.server = Some(handle);
        let send_host = self.sink(ctx, Done::Host);
        let send_changed = self.sink(ctx, |()| Done::Changed);
        self.spawn(async move {
            loop {
                match events.recv().await {
                    Ok(Event::Log(line)) => send_host(HostInput::Log(line.to_string())),
                    Ok(Event::Connection(ip)) => send_host(HostInput::Connection(ip)),
                    Ok(Event::Changed) => send_changed(()),
                    // A later snapshot shows whatever the missed events changed.
                    Err(RecvError::Lagged(_)) => send_changed(()),
                    Err(RecvError::Closed) => break,
                }
            }
        });
        self.run_host(ctx, actions);
    }

    /// Takes the results of the background work that ended.
    fn take_done(&mut self, ctx: &egui::Context) {
        while let Ok(done) = self.done.try_recv() {
            let now = Instant::now();
            match done {
                Done::Restored(result) => self.restored(ctx, result),
                // A result for a screen that was left is dropped.
                Done::Guest(input) => {
                    if let Screen::Guest { flow, .. } = &mut self.screen {
                        let actions = flow.apply(input, now);
                        self.run_guest(ctx, actions);
                    }
                }
                Done::Host(input) => {
                    if let Screen::Host { flow, .. } = &mut self.screen {
                        let actions = flow.apply(input, now);
                        self.run_host(ctx, actions);
                    }
                }
                Done::ServerStarted(result) => self.server_started(ctx, result),
                Done::Changed => {
                    if let (Some(server), Screen::Host { flow, .. }) =
                        (&self.server, &mut self.screen)
                    {
                        flow.apply(HostInput::Snapshot(server.snapshot()), now);
                    }
                }
            }
        }
    }

    /// Reads the hosts state after `hosts disable` and says what it found;
    /// the exit code alone is not trusted.
    fn restored(&mut self, ctx: &egui::Context, result: io::Result<i32>) {
        self.busy = false;
        if let Screen::Start(form) = &mut self.screen {
            form.refresh();
        }
        let state = hosts::status();
        let still = state.as_ref().is_ok_and(start::points_at_ds3_blaze);
        self.message = match (&result, &state) {
            (Err(err), _) => Some(err.to_string()),
            (Ok(_), Ok(state)) if still => {
                Some(format!("the EA servers were not restored: {state}"))
            }
            (Ok(_), Err(err)) => Some(format!("the hosts file cannot be read: {err}")),
            (Ok(_), Ok(_)) => None,
        };
        if self.closing == Closing::Restoring {
            if result.is_ok() && !still {
                self.closing = Closing::Allowed;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            } else {
                self.closing = Closing::No;
            }
        }
    }

    /// Holds a close request back while the hosts file points at ds3-blaze,
    /// so the dialog can ask about restoring the EA servers.
    fn hold_close(&mut self, ctx: &egui::Context) {
        if !ctx.input(|input| input.viewport().close_requested()) {
            return;
        }
        if self.closing == Closing::Allowed {
            return;
        }
        let points = hosts::status()
            .as_ref()
            .is_ok_and(start::points_at_ds3_blaze);
        if points || self.closing == Closing::Restoring {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            if self.closing == Closing::No {
                self.closing = Closing::Asking;
            }
        }
    }

    /// Draws the closing dialog while it asks, with restoring as the default
    /// that Enter picks.
    fn closing_dialog(&mut self, ctx: &egui::Context) {
        if self.closing != Closing::Asking {
            return;
        }
        let mut restore = false;
        let mut close = false;
        let mut stay = false;
        egui::Modal::new(egui::Id::unique("closing")).show(ctx, |ui| {
            ui.label("The hosts file still sends the game to ds3-blaze.");
            ui.label("Restore the EA servers before closing?");
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                let default = ui.button(RichText::new("Restore and close").strong());
                default.request_focus();
                restore =
                    default.clicked() || ui.input(|input| input.key_pressed(egui::Key::Enter));
                close = ui.button("Close without restoring").clicked();
                stay = ui.button("Cancel").clicked()
                    || ui.input(|input| input.key_pressed(egui::Key::Escape));
            });
        });
        if restore {
            // The server stops before the hosts file stops pointing at it.
            self.server = None;
            self.closing = Closing::Restoring;
            self.restore_hosts(ctx);
        } else if close {
            self.closing = Closing::Allowed;
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        } else if stay {
            self.closing = Closing::No;
        }
    }

    /// Writes the settings, keeping the error to show it.
    fn save(&mut self) {
        self.save_error = self
            .settings
            .save(&self.settings_path)
            .err()
            .map(|err| format!("{}: {err}", self.settings_path.display()));
    }

    /// Saves the name and `mode` and shows that mode's steps for the player
    /// at `own`, one of the addresses of `adapters`.
    fn choose(&mut self, mode: Mode, name: String, own: Ipv4Addr, adapters: Vec<Adapter>) {
        self.settings.name = name.clone();
        self.settings.mode = Some(mode);
        self.save();
        self.screen = match mode {
            Mode::Guest => Screen::Guest {
                flow: Box::new(GuestFlow::new(
                    own,
                    name,
                    self.settings.host.unwrap_or(Ipv4Addr::UNSPECIFIED),
                )),
                own,
                host: self
                    .settings
                    .host
                    .map(|host| host.to_string())
                    .unwrap_or_default(),
                refused: None,
            },
            Mode::Host => Screen::Host {
                flow: Box::new(HostFlow::new(own, name, self.settings.players.clone())),
                adapters,
            },
        };
        let rows = match &self.screen {
            Screen::Guest { flow, .. } => flow.steps().len(),
            Screen::Host { flow, .. } => flow.steps().len(),
            Screen::Start(_) => 0,
        };
        self.open = vec![false; rows];
    }

    /// Starts the host's steps over for the address `own`.
    fn host_at(&mut self, own: Ipv4Addr) {
        if let Screen::Host { flow, .. } = &mut self.screen {
            **flow = HostFlow::new(
                own,
                self.settings.name.clone(),
                self.settings.players.clone(),
            );
        }
    }

    /// Starts the guest's steps for the typed host address, or says why the
    /// address is refused.
    fn connect(&mut self, ctx: &egui::Context) {
        let Screen::Guest {
            flow,
            own,
            host,
            refused,
        } = &mut self.screen
        else {
            return;
        };
        let address = match host.trim().parse::<Ipv4Addr>() {
            Ok(address) if address != *own && !address.is_unspecified() => address,
            Ok(_) => {
                *refused = Some("type the host's VPN address, not your own".into());
                return;
            }
            Err(_) => {
                *refused = Some(format!("{host:?} is not an IPv4 address"));
                return;
            }
        };
        *refused = None;
        **flow = GuestFlow::new(*own, self.settings.name.clone(), address);
        let actions = flow.apply(GuestInput::Click(Click::Connect), Instant::now());
        self.settings.host = Some(address);
        self.save();
        self.run_guest(ctx, actions);
    }

    /// Draws `steps` after the first as numbered rows and returns the click
    /// one of them gave.
    fn rows(ui: &mut egui::Ui, steps: &[Step], open: &mut [bool]) -> Option<Click> {
        let mut click = None;
        for (index, step) in steps.iter().enumerate().skip(1) {
            if let Some(row_click) = row::show(ui, index, step, &mut open[index]) {
                click = Some(row_click);
            }
        }
        click
    }

    /// Draws the current mode's screen and returns what the player did.
    fn mode_screen(&mut self, ui: &mut egui::Ui) -> ModeInput {
        let mut input = ModeInput::default();
        let name = &self.settings.name;
        let errors = [&self.message, &self.save_error];
        match &mut self.screen {
            Screen::Start(_) => {}
            Screen::Guest {
                flow,
                host,
                refused,
                ..
            } => {
                let busy = flow.busy();
                let title = format!("ds3-blaze: joining as {name}");
                input.back = header(ui, title, "guest", !busy);
                let footer_click = footer(ui, &flow.footer(), errors);
                if busy {
                    ui.ctx().request_repaint_after(TICK);
                }
                egui::CentralPanel::default_margins().show(ui, |ui| {
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        let steps = flow.steps();
                        let mut enter = false;
                        let first = row::show_input(ui, &steps[0], refused.as_deref(), |ui| {
                            let field = egui::TextEdit::singleline(host)
                                .hint_text("the address the host's window shows")
                                .margin(vec2(8.0, 6.0))
                                .desired_width(ui.available_width() - 8.0);
                            let field = ui.add_enabled(!busy, field);
                            enter = field.lost_focus()
                                && ui.input(|input| input.key_pressed(egui::Key::Enter));
                        });
                        let rest = Self::rows(ui, steps, &mut self.open);
                        input.click = footer_click.or(first).or(rest);
                        if enter && !busy {
                            input.click = Some(Click::Connect);
                        }
                    });
                });
            }
            Screen::Host { flow, adapters } => {
                let leave = !flow.busy() && !flow.serving();
                let title = format!("ds3-blaze: hosting as {name}");
                input.back = header(ui, title, "host", leave);
                let footer_click = footer(ui, &flow.footer(), errors);
                if flow.busy() {
                    ui.ctx().request_repaint_after(TICK);
                }
                egui::CentralPanel::default_margins().show(ui, |ui| {
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        let steps = flow.steps();
                        let own = flow.own();
                        let mut chosen = own;
                        let first = row::show_input(ui, &steps[0], None, |ui| {
                            ui.add_enabled_ui(leave, |ui| {
                                egui::ComboBox::from_id_salt("own address")
                                    .width(ui.available_width() - 8.0)
                                    .selected_text(address_label(own, adapters))
                                    .show_ui(ui, |ui| {
                                        for (ip, adapter) in addresses(adapters) {
                                            ui.selectable_value(
                                                &mut chosen,
                                                ip,
                                                format!("{ip} ({})", adapter.name),
                                            );
                                        }
                                    });
                            });
                        });
                        let rest = Self::rows(ui, steps, &mut self.open);
                        input.click = footer_click.or(first).or(rest);
                        input.own = (chosen != own).then_some(chosen);
                    });
                });
            }
        }
        input
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

/// Returns `own` with the name of the adapter holding it.
fn address_label(own: Ipv4Addr, adapters: &[Adapter]) -> String {
    match addresses(adapters).find(|(ip, _)| *ip == own) {
        Some((ip, adapter)) => format!("{ip} ({})", adapter.name),
        None => own.to_string(),
    }
}

/// Draws the header of a mode's screen, `title` with Back before it and the
/// name of the `mode` at the right, and returns whether Back was pressed;
/// `leave` enables Back.
fn header(ui: &mut egui::Ui, title: String, mode: &str, leave: bool) -> bool {
    let mut back = false;
    egui::Panel::top("header")
        .frame(
            egui::Frame::new()
                .fill(ui.visuals().extreme_bg_color)
                .inner_margin(Margin::symmetric(16, 10)),
        )
        .show(ui, |ui| {
            egui::Sides::new().show(
                ui,
                |ui| {
                    back = ui
                        .add_enabled(leave, egui::Button::new("Back"))
                        .on_disabled_hover_text("stop the server or wait for the step to end")
                        .clicked();
                    ui.add_space(6.0);
                    ui.label(RichText::new(title).strong());
                },
                |ui| {
                    ui.label(RichText::new(mode).weak());
                },
            );
        });
    back
}

/// Draws the footer of a mode's screen with the `errors` the window has to
/// show above it, and returns the click its button gave.
fn footer(ui: &mut egui::Ui, footer: &Footer, errors: [&Option<String>; 2]) -> Option<Click> {
    let mut click = None;
    egui::Panel::bottom("footer")
        .frame(egui::Frame::new().inner_margin(Margin::symmetric(16, 12)))
        .show(ui, |ui| {
            for error in errors.into_iter().flatten() {
                ui.label(RichText::new(error).color(ERROR));
            }
            egui::Sides::new().show(
                ui,
                |ui| {
                    ui.label(RichText::new(&footer.text).weak());
                },
                |ui| {
                    let button = egui::Button::new(footer.button.label).min_size(vec2(0.0, 32.0));
                    if ui.add_enabled(footer.button.enabled, button).clicked() {
                        click = Some(footer.button.click.clone());
                    }
                },
            );
        });
    click
}

impl eframe::App for App {
    /// Takes finished background work, draws the current screen and carries
    /// out what its clicks ask for.
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.take_done(&ctx);
        match &mut self.screen {
            Screen::Guest { flow, .. } => {
                flow.apply(GuestInput::Tick, Instant::now());
            }
            Screen::Host { flow, .. } => {
                flow.apply(HostInput::Tick, Instant::now());
            }
            Screen::Start(_) => {}
        }
        self.hold_close(&ctx);
        let mut start_click = None;
        let mut mode = ModeInput::default();
        let busy = self.busy;
        if let Screen::Start(form) = &mut self.screen {
            egui::CentralPanel::default_margins().show(ui, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    start_click = form.show(ui, busy).map(|click| {
                        (
                            click,
                            form.name.clone(),
                            form.address,
                            form.adapters.clone(),
                        )
                    });
                    if busy {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label("waiting for the administrator prompt and the command line");
                        });
                    }
                    for error in [&self.message, &self.save_error].into_iter().flatten() {
                        ui.label(RichText::new(error).color(ERROR));
                    }
                });
            });
        } else {
            mode = self.mode_screen(ui);
        }
        self.closing_dialog(&ctx);
        match start_click {
            Some((StartClick::Mode(mode), name, Some(own), adapters)) => {
                self.choose(mode, name, own, adapters)
            }
            Some((StartClick::RestoreHosts, ..)) => self.restore_hosts(&ctx),
            _ => {}
        }
        match mode.click {
            // Connect reads the typed address before the steps start.
            Some(Click::Connect) => self.connect(&ctx),
            Some(click) => {
                let now = Instant::now();
                match &mut self.screen {
                    Screen::Guest { flow, .. } => {
                        let actions = flow.apply(GuestInput::Click(click), now);
                        self.run_guest(&ctx, actions);
                    }
                    Screen::Host { flow, .. } => {
                        let actions = flow.apply(HostInput::Click(click), now);
                        self.run_host(&ctx, actions);
                    }
                    Screen::Start(_) => {}
                }
            }
            None => {}
        }
        if let Some(own) = mode.own {
            self.host_at(own);
        }
        if mode.back {
            self.screen = Screen::Start(Form::read(self.settings.name.clone()));
        }
    }
}
