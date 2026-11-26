//! The window's state and its drawing: the start screen, then the chosen
//! mode's steps, with the background work they start.

use crate::elevate;
use crate::guest::{GuestAction, GuestFlow, GuestInput};
use crate::host::{HostAction, HostFlow, HostInput};
use crate::row;
use crate::run;
use crate::settings::{self, Mode, Settings};
use crate::start::{self, Form, StartClick};
use crate::step::{Click, Step};
use ds3_blaze_core::event::Event;
use ds3_blaze_core::hosts;
use ds3_blaze_core::log;
use ds3_blaze_core::server::{Config, Ports, Server, ServerHandle};
use eframe::egui::{self, Color32, RichText};
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
const ERROR: Color32 = Color32::from_rgb(220, 80, 70);

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
        Ok(App {
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
        })
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
        let Screen::Host { flow } = &mut self.screen else {
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
                    if let Screen::Host { flow } = &mut self.screen {
                        let actions = flow.apply(input, now);
                        self.run_host(ctx, actions);
                    }
                }
                Done::ServerStarted(result) => self.server_started(ctx, result),
                Done::Changed => {
                    if let (Some(server), Screen::Host { flow }) = (&self.server, &mut self.screen)
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
            Screen::Host { flow } => flow.steps().len(),
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

    /// Draws `steps` as rows and returns the click one of them gave.
    fn rows(ui: &mut egui::Ui, steps: &[Step], open: &mut [bool]) -> Option<Click> {
        let mut click = None;
        for (number, step) in steps.iter().enumerate() {
            if let Some(row_click) = row::show(ui, number + 1, step, &mut open[number]) {
                click = Some(row_click);
            }
        }
        click
    }

    /// Draws the current mode's screen and returns the click a row gave,
    /// whether Connect was pressed and whether Back was.
    fn mode_screen(&mut self, ui: &mut egui::Ui) -> (Option<Click>, bool, bool) {
        let mut connect = false;
        let mut back = false;
        let click = match &mut self.screen {
            Screen::Start(_) => None,
            Screen::Guest {
                flow,
                host,
                refused,
                ..
            } => {
                let busy = flow.busy();
                ui.horizontal(|ui| {
                    back = ui.add_enabled(!busy, egui::Button::new("Back")).clicked();
                    ui.heading("Join a friend");
                });
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.label("Host's VPN address");
                    let field = ui
                        .add_enabled(!busy, egui::TextEdit::singleline(host).desired_width(140.0));
                    let enter =
                        field.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
                    connect = ui
                        .add_enabled(!busy, egui::Button::new("Connect"))
                        .clicked()
                        || enter;
                });
                if let Some(refused) = refused {
                    ui.label(RichText::new(refused.as_str()).color(ERROR));
                }
                ui.add_space(8.0);
                if busy {
                    ui.ctx().request_repaint_after(TICK);
                }
                Self::rows(ui, flow.steps(), &mut self.open)
            }
            Screen::Host { flow } => {
                let leave = !flow.busy() && !flow.serving();
                ui.horizontal(|ui| {
                    back = ui.add_enabled(leave, egui::Button::new("Back")).clicked();
                    ui.heading("Host a game");
                });
                ui.add_space(8.0);
                if flow.busy() {
                    ui.ctx().request_repaint_after(TICK);
                }
                Self::rows(ui, flow.steps(), &mut self.open)
            }
        };
        (click, connect, back)
    }
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
            Screen::Host { flow } => {
                flow.apply(HostInput::Tick, Instant::now());
            }
            Screen::Start(_) => {}
        }
        self.hold_close(&ctx);
        let mut start_click = None;
        let mut mode = (None, false, false);
        let busy = self.busy;
        egui::CentralPanel::default_margins().show(ui, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                if let Screen::Start(form) = &mut self.screen {
                    start_click = form
                        .show(ui, busy)
                        .map(|click| (click, form.name.clone(), form.address));
                } else {
                    mode = self.mode_screen(ui);
                }
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
        self.closing_dialog(&ctx);
        match start_click {
            Some((StartClick::Mode(mode), name, Some(own))) => self.choose(mode, name, own),
            Some((StartClick::RestoreHosts, ..)) => self.restore_hosts(&ctx),
            _ => {}
        }
        let (click, connect, back) = mode;
        if let Some(click) = click {
            let now = Instant::now();
            match &mut self.screen {
                Screen::Guest { flow, .. } => {
                    let actions = flow.apply(GuestInput::Click(click), now);
                    self.run_guest(&ctx, actions);
                }
                Screen::Host { flow } => {
                    let actions = flow.apply(HostInput::Click(click), now);
                    self.run_host(&ctx, actions);
                }
                Screen::Start(_) => {}
            }
        }
        if connect {
            self.connect(&ctx);
        }
        if back {
            self.screen = Screen::Start(Form::read(self.settings.name.clone()));
        }
    }
}
