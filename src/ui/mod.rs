//! egui front-end: a top bar with tabs, and panels floating on a canvas — the list,
//! the open conversation as bubbles, and its thread on the right.
//!
//! - [`app`]: state, what happens each frame, signing in and out, the demo and screenshots
//! - [`workspace`]: the signed-in state: the section, the open conversation and thread, refreshes
//! - [`views`]: the top bar, the list, the conversation, the thread, the sign-in page
//! - [`widgets`]: avatars, rich text, cards, reactions
//! - [`pictures`]: people's and teams' pictures
//! - [`style`]: palette, type, metrics and icons
//! - [`load`]: background work

mod app;
mod load;
mod pictures;
mod style;
mod views;
mod widgets;
mod workspace;

use eframe::egui;
use style::metrics;

/// Open the main window and run the app until it closes.
pub fn run() -> eframe::Result {
    crate::log::to_file();
    log_info!("drift {} starting ({} {})", env!("CARGO_PKG_VERSION"), std::env::consts::OS, std::env::consts::ARCH);
    let viewport = egui::ViewportBuilder::default()
        .with_title("drift")
        .with_inner_size(metrics::WINDOW_SIZE)
        .with_min_inner_size(metrics::MIN_WINDOW_SIZE);
    // The sidebar runs up under a transparent title bar, the traffic lights over it.
    #[cfg(target_os = "macos")]
    let viewport = viewport.with_fullsize_content_view(true).with_title_shown(false).with_titlebar_shown(false);
    let options = eframe::NativeOptions { viewport, ..Default::default() };
    eframe::run_native("drift", options, Box::new(|cc| Ok(Box::new(app::App::new(cc)))))
}
