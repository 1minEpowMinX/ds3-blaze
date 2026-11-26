//! The window's state and its drawing: the start screen, then the chosen
//! mode's steps.

use crate::row;
use crate::settings::{self, Mode, Settings};
use crate::start::{Form, StartClick};
use crate::step::Step;
use eframe::egui::{self, Color32, RichText};
use std::path::PathBuf;

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

/// The window.
pub struct App {
    settings: Settings,
    settings_path: PathBuf,
    screen: Screen,
    /// Whether each row of the mode's steps is open, by position.
    open: Vec<bool>,
    /// The error of saving the settings last, shown until a save succeeds.
    save_error: Option<String>,
}

impl App {
    /// Returns the window with the settings from [`settings::path`] and the
    /// start screen.
    pub fn new() -> App {
        let settings_path = settings::path();
        let settings = Settings::load(&settings_path);
        let form = Form::read(settings.name.clone());
        App {
            settings,
            settings_path,
            screen: Screen::Start(form),
            open: Vec::new(),
            save_error: None,
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
    /// Draws the current screen and carries out what its clicks ask for.
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let mut chosen = None;
        let mut back = false;
        egui::CentralPanel::default_margins().show(ui, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                match &mut self.screen {
                    Screen::Start(form) => {
                        if let Some(StartClick::Mode(mode)) = form.show(ui, false) {
                            chosen = Some((mode, form.name.clone()));
                        }
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
                if let Some(error) = &self.save_error {
                    ui.label(
                        RichText::new(format!("the settings were not saved: {error}"))
                            .color(Color32::from_rgb(220, 80, 70)),
                    );
                }
            });
        });
        if let Some((mode, name)) = chosen {
            self.choose(mode, name);
        }
        if back {
            self.screen = Screen::Start(Form::read(self.settings.name.clone()));
        }
    }
}
