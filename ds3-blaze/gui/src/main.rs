//! Window of ds3-blaze: hosting a Dead Space 3 co-op game or joining one,
//! step by step.

// A release build opens no console; a debug build keeps one for panics.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod banner;
mod checks;
mod elevate;
mod guest;
mod host;
mod link;
mod network;
#[cfg(debug_assertions)]
mod preview;
mod row;
mod run;
mod session;
mod settings;
mod start;
mod step;
mod theme;

use eframe::egui;

/// Size of the window when it opens, in points.
const WINDOW_SIZE: [f32; 2] = [820.0, 720.0];

/// Smallest size the window can be dragged to, in points.
const MIN_SIZE: [f32; 2] = [560.0, 480.0];

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("ds3-blaze")
            .with_inner_size(WINDOW_SIZE)
            .with_min_inner_size(MIN_SIZE),
        ..Default::default()
    };
    eframe::run_native(
        "ds3-blaze",
        options,
        Box::new(|creation| {
            theme::apply(&creation.egui_ctx);
            Ok(Box::new(app::App::new()?))
        }),
    )
}
