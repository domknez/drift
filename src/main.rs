#![cfg_attr(windows, windows_subsystem = "windows")]

//! drift — a free and open source, lightweight desktop client for Microsoft Teams
//! chats and channels.

#[macro_use]
mod log;
mod backend;
mod credentials;
mod login;
mod model;
mod settings;
mod teams;
mod ui;

fn main() -> eframe::Result {
    log::install_panic_hook();
    if std::env::args().any(|a| a == login::ARG) {
        login::run_window();
        return Ok(());
    }
    ui::run()
}
