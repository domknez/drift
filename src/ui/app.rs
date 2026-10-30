//! The app: the sign-in page or the workspace, the theme, and moving between them.

use super::load::Load;
use super::style;
use super::views;
use super::workspace::{Outcome, Workspace};
use crate::backend::Backend;
use crate::settings::{Appearance, Prefs};
use crate::teams::Teams;
use crate::teams::auth::Session;
use eframe::egui;
use std::sync::Arc;

pub struct App {
    prefs: Prefs,
    /// The theme last applied.
    dark: Option<bool>,
    screen: Screen,
}

enum Screen {
    SignIn(SignIn),
    Signed(Box<Workspace>),
}

/// The sign-in page: idle, or waiting on the sign-in window.
pub struct SignIn {
    pub pending: Load<Arc<dyn Backend>, String>,
    /// Why the last attempt (or the stored sign-in) failed.
    pub error: Option<String>,
}

impl SignIn {
    fn new(error: Option<String>) -> SignIn {
        SignIn { pending: Load::Idle, error }
    }

    /// Open the sign-in window and redeem what it returns.
    pub fn start(&mut self, ctx: &egui::Context) {
        self.error = None;
        self.pending = Load::start(ctx, || {
            let code = crate::login::obtain_code()?;
            let session = Session::sign_in(&code).map_err(|e| e.to_string())?;
            Ok(Arc::new(Teams::new(session)) as Arc<dyn Backend>)
        });
    }
}

/// What the views ask the app for.
pub enum Request {
    Theme(Appearance),
    SignOut,
}

impl App {
    pub fn new(cc: &eframe::CreationContext) -> App {
        style::install_fonts(&cc.egui_ctx);
        let prefs = Prefs::load();
        let screen = match Session::stored() {
            Some(session) => {
                Screen::Signed(Box::new(Workspace::new(&cc.egui_ctx, Arc::new(Teams::new(session)), prefs.conversation.clone())))
            }
            None => Screen::SignIn(SignIn::new(None)),
        };
        App { prefs, dark: None, screen }
    }

    fn apply_theme(&mut self, ctx: &egui::Context) {
        let dark = match self.prefs.theme {
            Appearance::Dark => true,
            Appearance::Light => false,
            Appearance::System => ctx.input(|i| i.raw.system_theme) != Some(egui::Theme::Light),
        };
        if self.dark != Some(dark) {
            self.dark = Some(dark);
            style::apply(ctx, dark);
        }
    }

    fn handle(&mut self, request: Request) {
        match request {
            Request::Theme(theme) => self.prefs.update(|p| p.theme = theme),
            Request::SignOut => {
                Session::sign_out();
                self.prefs.update(|p| p.conversation = None);
                self.screen = Screen::SignIn(SignIn::new(None));
            }
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.apply_theme(&ctx);
        let mut request = None;
        match &mut self.screen {
            Screen::SignIn(sign_in) => {
                if let Some(result) = sign_in.pending.take() {
                    match result {
                        Ok(backend) => {
                            self.screen = Screen::Signed(Box::new(Workspace::new(&ctx, backend, self.prefs.conversation.clone())));
                            ctx.request_repaint();
                            return;
                        }
                        Err(e) => sign_in.error = Some(e),
                    }
                }
                views::sign_in::show(ui, sign_in);
            }
            Screen::Signed(workspace) => {
                if let Some(Outcome::SignedOut) = workspace.tick(&ctx) {
                    Session::sign_out();
                    self.screen = Screen::SignIn(SignIn::new(Some("Your sign-in has expired. Please sign in again.".into())));
                    return;
                }
                let before = workspace.open.clone();
                request = views::workspace(ui, workspace, self.prefs.theme);
                if workspace.open != before {
                    let open = workspace.open.clone();
                    self.prefs.update(|p| p.conversation = open);
                }
            }
        }
        if let Some(request) = request {
            self.handle(request);
        }
    }
}
