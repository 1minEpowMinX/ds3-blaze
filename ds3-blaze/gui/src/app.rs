//! The window's state and its drawing: the start screen, then the chosen
//! mode's steps, with the background work they start.

use crate::banner;
use crate::elevate;
use crate::guest::{GuestAction, GuestFlow, GuestInput};
use crate::host::{HostAction, HostFlow, HostInput};
use crate::row;
use crate::run;
use crate::session;
use crate::settings::{self, Mode, Settings};
use crate::start::{self, Form, StartClick};
use crate::step::{BUSY, Button, Click};
use ds3_blaze_core::event::Event;
use ds3_blaze_core::hosts;
use ds3_blaze_core::log;
use ds3_blaze_core::server::{Config, Ports, Server, ServerHandle};
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
    },
}

/// What the player did on a mode's screen during a frame.
#[derive(Default)]
struct ModeInput {
    click: Option<Click>,
    back: bool,
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
    /// Whether the player opened the folded "Setup done" line; it folds
    /// again once a step needs attention and then ends.
    setup_open: bool,
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
            setup_open: false,
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
    /// at `own`.
    fn choose(&mut self, mode: Mode, name: String, own: Ipv4Addr) {
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
            },
        };
        let rows = match &self.screen {
            Screen::Guest { flow, .. } => flow.steps().len(),
            Screen::Host { flow, .. } => flow.steps().len(),
            Screen::Start(_) => 0,
        };
        self.open = vec![false; rows];
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
        if host.trim().is_empty() {
            *refused = Some("type the host's VPN address".into());
            return;
        }
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

    /// Draws the current mode's screen and returns what the player did.
    fn mode_screen(&mut self, ui: &mut egui::Ui) -> ModeInput {
        let mut input = ModeInput::default();
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
                let title = if flow.host().is_unspecified() {
                    "Joining a friend".to_string()
                } else {
                    format!("Joining {}", flow.host())
                };
                input.back = header(ui, &title, busy.then_some(BUSY));
                let exit = footer(ui, &flow.exit(), errors);
                if busy {
                    ui.ctx().request_repaint_after(TICK);
                }
                egui::CentralPanel::default_margins().show(ui, |ui| {
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        let mut enter = false;
                        let banner = banner::show(ui, &flow.banner(), |ui| {
                            let field = egui::TextEdit::singleline(host)
                                .hint_text("26.0.0.1")
                                .margin(vec2(8.0, 6.0))
                                .desired_width(260.0);
                            let field = ui.add_enabled(!busy, field);
                            enter = field.lost_focus()
                                && ui.input(|input| input.key_pressed(egui::Key::Enter));
                            if let Some(refused) = refused.as_deref() {
                                ui.label(RichText::new(refused).color(ERROR));
                            }
                        });
                        ui.add_space(8.0);
                        let done = flow.setup_done();
                        self.setup_open &= done;
                        let rows = row::setup(
                            ui,
                            flow.steps(),
                            &mut self.open,
                            done,
                            &mut self.setup_open,
                        );
                        input.click = exit.or(banner).or(rows);
                        if enter && !busy {
                            input.click = Some(Click::Connect);
                        }
                    });
                });
            }
            Screen::Host { flow } => {
                let stay = if flow.busy() {
                    Some(BUSY)
                } else if flow.serving() {
                    Some("Stop hosting first")
                } else {
                    None
                };
                input.back = header(ui, "Hosting", stay);
                let exit = footer(ui, &flow.exit(), errors);
                if flow.busy() {
                    ui.ctx().request_repaint_after(TICK);
                }
                egui::CentralPanel::default_margins().show(ui, |ui| {
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        let banner = banner::show(ui, &flow.banner(), |_| {});
                        ui.add_space(8.0);
                        let done = flow.setup_done();
                        self.setup_open &= done;
                        let rows = row::setup(
                            ui,
                            flow.steps(),
                            &mut self.open,
                            done,
                            &mut self.setup_open,
                        );
                        let session = flow.session().and_then(|view| session::show(ui, &view));
                        input.click = exit.or(banner).or(rows).or(session);
                    });
                });
            }
        }
        input
    }
}

/// Draws the header of a mode's screen: Back as an arrow, then `title`;
/// returns whether Back was pressed. `stay` disables Back and says why.
fn header(ui: &mut egui::Ui, title: &str, stay: Option<&'static str>) -> bool {
    let mut back = false;
    egui::Panel::top("header")
        .frame(
            egui::Frame::new()
                .fill(ui.visuals().extreme_bg_color)
                .inner_margin(Margin::symmetric(16, 10)),
        )
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                let arrow = egui::Button::new(RichText::new("←").size(18.0));
                let response = ui.add_enabled(stay.is_none(), arrow);
                let response = match stay {
                    Some(reason) => response.on_disabled_hover_text(reason),
                    None => response.on_hover_text("Back"),
                };
                back = response.clicked();
                ui.add_space(6.0);
                ui.label(RichText::new(title).size(16.0).strong());
            });
        });
    back
}

/// Draws the footer of a mode's screen with the `errors` the window has to
/// show above it and the `exit` button at the right, and returns the click
/// the button gave.
fn footer(ui: &mut egui::Ui, exit: &Button, errors: [&Option<String>; 2]) -> Option<Click> {
    let mut click = None;
    egui::Panel::bottom("footer")
        .frame(egui::Frame::new().inner_margin(Margin::symmetric(16, 12)))
        .show(ui, |ui| {
            for error in errors.into_iter().flatten() {
                ui.label(RichText::new(error).color(ERROR));
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if row::action(ui, exit, 32.0) {
                    click = Some(exit.click.clone());
                }
            });
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
                    start_click = form
                        .show(ui, busy)
                        .map(|click| (click, form.name.clone(), form.address));
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
            Some((StartClick::Mode(mode), name, Some(own))) => self.choose(mode, name, own),
            Some((StartClick::RestoreHosts, ..)) => self.restore_hosts(&ctx),
            _ => {}
        }
        match mode.click {
            // Connect reads the typed address before the steps start.
            Some(Click::Connect) => self.connect(&ctx),
            Some(Click::CopyAddress) => {
                if let Screen::Host { flow } = &self.screen {
                    ctx.copy_text(flow.own().to_string());
                }
            }
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
        if mode.back {
            self.screen = Screen::Start(Form::read(self.settings.name.clone()));
        }
    }
}
