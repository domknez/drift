#![cfg_attr(windows, windows_subsystem = "windows")]

//! drift — a free and open source, lightweight desktop client for Microsoft Teams
//! chats and channels.

#[macro_use]
mod log;
mod backend;
mod credentials;
mod demo;
mod icon;
mod idle;
mod login;
mod model;
mod notify;
mod settings;
mod teams;
mod ui;
mod update;

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
    if std::env::args().nth(1).as_deref() == Some("status") {
        status();
        return Ok(());
    }
    if std::env::args().nth(1).as_deref() == Some("listen") {
        listen();
        return Ok(());
    }
    ui::run()
}

/// `drift listen`: connect to the push service and print what kind of change each
/// push is, for troubleshooting (no ids, names or message text).
fn listen() {
    use backend::{Backend, Push};
    let Some(session) = teams::auth::Session::stored() else {
        eprintln!("drift: not signed in");
        std::process::exit(1);
    };
    let teams = std::sync::Arc::new(teams::Teams::new(session));
    teams.clone().listen(Box::new(|push| match push {
        Push::Live(live) => println!("{}", if live { "live" } else { "down" }),
        Push::Everything => println!("fetch everything"),
        Push::Conversation(id) => println!("change in a conversation ({})", id.split(':').next().unwrap_or("?")),
    }));
    loop {
        std::thread::park();
    }
}

/// `drift status`: the signed-in user's status, as drift reads it.
fn status() {
    use backend::Backend;
    let Some(session) = teams::auth::Session::stored() else {
        eprintln!("drift: not signed in");
        std::process::exit(1);
    };
    match teams::Teams::new(session).status() {
        Ok(s) => println!("shown: {}\nset: {}", s.shown.label(), s.set.map_or("nothing (by activity)", |a| a.label())),
        Err(e) => {
            eprintln!("drift: {e}");
            std::process::exit(1);
        }
    }
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
