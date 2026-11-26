//! Window of ds3-blaze: hosting a Dead Space 3 co-op game or joining one,
//! step by step.

// A release build opens no console; a debug build keeps one for panics.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
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

use eframe::egui;

/// Size of the window when it opens, in points.
const WINDOW_SIZE: [f32; 2] = [820.0, 680.0];

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
        Box::new(|creation| {
            style(&creation.egui_ctx);
            Ok(Box::new(app::App::new()?))
        }),
    )
}

/// Sets the padding and the height the window's widgets are drawn with.
fn style(ctx: &egui::Context) {
    ctx.all_styles_mut(|style| {
        style.spacing.button_padding = egui::vec2(12.0, 5.0);
        style.spacing.interact_size.y = 28.0;
    });
}
