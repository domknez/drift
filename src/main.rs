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
    if std::env::args().nth(1).as_deref() == Some("shape") {
        shape();
        return Ok(());
    }
    ui::run()
}

/// `drift shape`: the structure of what Teams answers, for troubleshooting (no
/// names or message text).
fn shape() {
    let Some(session) = teams::auth::Session::stored() else {
        eprintln!("drift: not signed in");
        std::process::exit(1);
    };
    match teams::Teams::new(session).shape() {
        Ok(text) => print!("{text}"),
        Err(e) => {
            eprintln!("drift: {e}");
            std::process::exit(1);
        }
    }
}
