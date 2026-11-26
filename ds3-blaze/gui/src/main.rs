//! Window of ds3-blaze: hosting a Dead Space 3 co-op game or joining one,
//! step by step.

// A release build opens no console; a debug build keeps one for panics.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod elevate;
mod row;
mod settings;
mod start;
#[expect(
    dead_code,
    reason = "the guest's and the host's steps construct the rest of it"
)]
mod step;

use eframe::egui;

/// Size of the window when it opens, in points.
const WINDOW_SIZE: [f32; 2] = [760.0, 600.0];

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("ds3-blaze")
            .with_inner_size(WINDOW_SIZE),
        ..Default::default()
    };
    eframe::run_native(
        "ds3-blaze",
        options,
        Box::new(|_| Ok(Box::new(app::App::new()?))),
    )
}
