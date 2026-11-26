//! The window's state and its drawing: the start screen, then the chosen
//! mode's steps, with the background work they start.

use crate::elevate;
use crate::guest::{GuestAction, GuestFlow, GuestInput};
use crate::row;
use crate::run;
use crate::settings::{self, Mode, Settings};
use crate::start::{self, Form, StartClick};
use crate::step::{Click, Step};
use ds3_blaze_core::hosts;
use eframe::egui::{self, Color32, RichText};
use std::future::Future;
use std::io;
use std::net::Ipv4Addr;
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::{Duration, Instant};

/// Titles of the host's steps, in the order they run.
const HOST_TITLES: [&str; 7] = [
    "Your VPN address",
    "Point game at this PC",
    "Server",
    "Players",
    "Direct link to the guest",
    "Room",
    "Log",
];

/// How often a running countdown is drawn again.
const TICK: Duration = Duration::from_secs(1);

/// What the window shows.
enum Screen {
    Start(Form),
    Guest {
        flow: GuestFlow,
        own: Ipv4Addr,
        /// The host's address as typed.
        host: String,
        /// Why the typed address was refused.
        refused: Option<String>,
    },
    Host {
        steps: Vec<Step>,
    },
}

/// Result of work the window started in the background.
enum Done {
    /// `hosts disable` from the start screen or the closing dialog ended,
    /// with the command line's exit code.
    Restored(io::Result<i32>),
    Guest(GuestInput),
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
    /// Whether a restore from the start screen or the closing dialog runs.
    busy: bool,
    /// What that restore left to say.
    message: Option<String>,
    closing: Closing,
}

impl App {
    /// Returns the window with the settings from [`settings::path`] and the
    /// start screen.
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

    /// Takes the results of the background work that ended.
    fn take_done(&mut self, ctx: &egui::Context) {
        while let Ok(done) = self.done.try_recv() {
            match done {
                Done::Restored(result) => self.restored(ctx, result),
                Done::Guest(input) => {
                    // A result for a guest screen that was left is dropped.
                    if let Screen::Guest { flow, .. } = &mut self.screen {
                        let actions = flow.apply(input, Instant::now());
                        self.run_guest(ctx, actions);
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
            Mode::Guest => {
                let host = self.settings.host.unwrap_or(Ipv4Addr::UNSPECIFIED);
                Screen::Guest {
                    flow: GuestFlow::new(own, name, host),
                    own,
                    host: self
                        .settings
                        .host
                        .map(|host| host.to_string())
                        .unwrap_or_default(),
                    refused: None,
                }
            }
            Mode::Host => Screen::Host {
                steps: HOST_TITLES.iter().map(|title| Step::new(title)).collect(),
            },
        };
        self.open = vec![false; HOST_TITLES.len()];
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
        *flow = GuestFlow::new(*own, self.settings.name.clone(), address);
        let actions = flow.apply(GuestInput::Click(Click::Connect), Instant::now());
        self.settings.host = Some(address);
        self.save();
        self.run_guest(ctx, actions);
    }

    /// Draws the guest's screen and returns the click a row gave, whether
    /// Connect was pressed and whether Back was.
    fn guest_screen(&mut self, ui: &mut egui::Ui) -> (Option<Click>, bool, bool) {
        let Screen::Guest {
            flow,
            host,
            refused,
            ..
        } = &mut self.screen
        else {
            return (None, false, false);
        };
        let busy = flow.busy();
        let mut connect = false;
        let mut back = false;
        ui.horizontal(|ui| {
            back = ui.add_enabled(!busy, egui::Button::new("Back")).clicked();
            ui.heading("Join a friend");
        });
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.label("Host's VPN address");
            let field =
                ui.add_enabled(!busy, egui::TextEdit::singleline(host).desired_width(140.0));
            let enter = field.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
            connect = ui
                .add_enabled(!busy, egui::Button::new("Connect"))
                .clicked()
                || enter;
        });
        if let Some(refused) = refused {
            ui.label(RichText::new(refused.as_str()).color(Color32::from_rgb(220, 80, 70)));
        }
        ui.add_space(8.0);
        let mut click = None;
        for (number, step) in flow.steps().iter().enumerate() {
            if let Some(row_click) = row::show(ui, number + 1, step, &mut self.open[number]) {
                click = Some(row_click);
            }
        }
        if busy {
            ui.ctx().request_repaint_after(TICK);
        }
        (click, connect, back)
    }
}

impl eframe::App for App {
    /// Takes finished background work, draws the current screen and carries
    /// out what its clicks ask for.
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.take_done(&ctx);
        if let Screen::Guest { flow, .. } = &mut self.screen {
            flow.apply(GuestInput::Tick, Instant::now());
        }
        self.hold_close(&ctx);
        let mut start_click = None;
        let mut guest = (None, false, false);
        let mut back = false;
        let busy = self.busy;
        egui::CentralPanel::default_margins().show(ui, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                match &mut self.screen {
                    Screen::Start(form) => {
                        start_click = form
                            .show(ui, busy)
                            .map(|click| (click, form.name.clone(), form.address));
                    }
                    Screen::Guest { .. } => guest = self.guest_screen(ui),
                    Screen::Host { steps } => {
                        ui.horizontal(|ui| {
                            back = ui.button("Back").clicked();
                            ui.heading("Host a game");
                        });
                        ui.add_space(8.0);
                        for (number, step) in steps.iter().enumerate() {
                            row::show(ui, number + 1, step, &mut self.open[number]);
                        }
                    }
                }
                if busy {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label("waiting for the administrator prompt and the command line");
                    });
                }
                for error in [&self.message, &self.save_error].into_iter().flatten() {
                    ui.label(RichText::new(error).color(Color32::from_rgb(220, 80, 70)));
                }
            });
        });
        self.closing_dialog(&ctx);
        match start_click {
            Some((StartClick::Mode(mode), name, Some(own))) => self.choose(mode, name, own),
            Some((StartClick::RestoreHosts, ..)) => self.restore_hosts(&ctx),
            _ => {}
        }
        let (click, connect, guest_back) = guest;
        if let (Some(click), Screen::Guest { flow, .. }) = (click, &mut self.screen) {
            let actions = flow.apply(GuestInput::Click(click), Instant::now());
            self.run_guest(&ctx, actions);
        }
        if connect {
            self.connect(&ctx);
        }
        if back || guest_back {
            self.screen = Screen::Start(Form::read(self.settings.name.clone()));
        }
    }
}
