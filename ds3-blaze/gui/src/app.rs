//! The window's state and its drawing: the start screen, then the chosen
//! mode's steps, with the background work they start.

use crate::elevate;
use crate::row;
use crate::settings::{self, Mode, Settings};
use crate::start::{self, Form, StartClick};
use crate::step::Step;
use ds3_blaze_core::hosts;
use eframe::egui::{self, Color32, RichText};
use std::future::Future;
use std::io;
use std::path::PathBuf;
use std::sync::mpsc;

/// Titles of each mode's steps, in the order they run.
fn titles(mode: Mode) -> &'static [&'static str] {
    match mode {
        Mode::Host => &[
            "Your VPN address",
            "Point game at this PC",
            "Server",
            "Players",
            "Direct link to the guest",
            "Room",
            "Log",
        ],
        Mode::Guest => &[
            "Host's VPN address",
            "Point game at host",
            "Reach host's server",
            "Direct link to host",
            "Start Dead Space 3",
        ],
    }
}

/// What the window shows.
enum Screen {
    Start(Form),
    Mode { mode: Mode, steps: Vec<Step> },
}

/// Result of work the window started in the background.
enum Done {
    /// `hosts disable` ended, with the command line's exit code.
    Restored(io::Result<i32>),
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
    /// Whether an elevated command is running.
    busy: bool,
    /// What the last elevated command left to say.
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

    /// Runs `work` on the runtime and hands its result to the next frame,
    /// which it asks egui to draw.
    fn spawn(&self, ctx: &egui::Context, work: impl Future<Output = Done> + Send + 'static) {
        let sender = self.done_sender.clone();
        let ctx = ctx.clone();
        self.runtime.spawn(async move {
            // The receiver lives as long as the window.
            let _ = sender.send(work.await);
            ctx.request_repaint();
        });
    }

    /// Starts `hosts disable` through the command line, elevated.
    fn restore_hosts(&mut self, ctx: &egui::Context) {
        self.busy = true;
        self.message = None;
        self.spawn(ctx, async {
            let args = vec!["hosts".to_string(), "disable".to_string()];
            Done::Restored(elevate::run_cli(args).await)
        });
    }

    /// Takes the results of the background work that ended.
    fn take_done(&mut self, ctx: &egui::Context) {
        while let Ok(done) = self.done.try_recv() {
            match done {
                Done::Restored(result) => self.restored(ctx, result),
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

    /// Saves the name and `mode` and shows that mode's steps.
    fn choose(&mut self, mode: Mode, name: String) {
        self.settings.name = name;
        self.settings.mode = Some(mode);
        self.save();
        let steps: Vec<Step> = titles(mode).iter().map(|title| Step::new(title)).collect();
        self.open = vec![false; steps.len()];
        self.screen = Screen::Mode { mode, steps };
    }
}

impl eframe::App for App {
    /// Takes finished background work, draws the current screen and carries
    /// out what its clicks ask for.
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.take_done(&ctx);
        self.hold_close(&ctx);
        let mut start_click = None;
        let mut back = false;
        let busy = self.busy;
        egui::CentralPanel::default_margins().show(ui, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                match &mut self.screen {
                    Screen::Start(form) => {
                        start_click = form.show(ui, busy).map(|click| (click, form.name.clone()));
                    }
                    Screen::Mode { mode, steps } => {
                        ui.horizontal(|ui| {
                            back = ui.button("Back").clicked();
                            ui.heading(match mode {
                                Mode::Host => "Host a game",
                                Mode::Guest => "Join a friend",
                            });
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
            Some((StartClick::Mode(mode), name)) => self.choose(mode, name),
            Some((StartClick::RestoreHosts, _)) => self.restore_hosts(&ctx),
            None => {}
        }
        if back {
            self.screen = Screen::Start(Form::read(self.settings.name.clone()));
        }
    }
}
