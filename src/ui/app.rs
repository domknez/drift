//! The app: the sign-in page or the workspace, the theme, and moving between them.

use super::load::Load;
use super::style;
use super::views;
use super::workspace::{Outcome, Workspace};
use crate::backend::Backend;
use crate::demo;
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
    /// `DRIFT_SCREENSHOT=<file.png>`: save the window once it has loaded, then quit.
    capture: Option<Capture>,
}

struct Capture {
    path: std::path::PathBuf,
    /// Frames drawn since everything loaded (pictures and layout settle).
    frames: u32,
    requested: bool,
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
        let capture = std::env::var_os("DRIFT_SCREENSHOT").map(|path| Capture { path: path.into(), frames: 0, requested: false });
        // `DRIFT_DEMO=1`: the made-up workspace, with settings that are never saved.
        if std::env::var_os("DRIFT_DEMO").is_some() {
            let theme = std::env::var("DRIFT_THEME").unwrap_or_default();
            let prefs = Prefs::parse(&format!("theme={theme}"));
            let mut workspace = Workspace::new(&cc.egui_ctx, Arc::new(demo::Demo), Some(demo::OPEN.to_string()));
            workspace.thread = Some(demo::THREAD.to_string());
            return App { prefs, dark: None, screen: Screen::Signed(Box::new(workspace)), capture };
        }
        let prefs = Prefs::load();
        let screen = match Session::stored() {
            Some(session) => {
                Screen::Signed(Box::new(Workspace::new(&cc.egui_ctx, Arc::new(Teams::new(session)), prefs.conversation.clone())))
            }
            None => Screen::SignIn(SignIn::new(None)),
        };
        App { prefs, dark: None, screen, capture }
    }

    /// Take the screenshot once the workspace has loaded, save it and quit.
    fn capture(&mut self, ctx: &egui::Context) {
        let Some(capture) = &mut self.capture else { return };
        let ready = matches!(&self.screen, Screen::Signed(ws) if ws.directory.is_some() && ws.messages.is_some())
            || matches!(&self.screen, Screen::SignIn(_));
        if !ready {
            return;
        }
        capture.frames += 1;
        ctx.request_repaint();
        if capture.frames == 30 && !capture.requested {
            capture.requested = true;
            let path = capture.path.clone();
            let done = egui::ScreenshotCallback::new(move |image| {
                if let Err(e) = save_png(&path, &image) {
                    eprintln!("drift: screenshot: {e}");
                }
                std::process::exit(0);
            });
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(done));
        }
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
        self.capture(&ctx);
    }
}

fn save_png(path: &std::path::Path, image: &egui::ColorImage) -> Result<(), String> {
    let file = std::fs::File::create(path).map_err(|e| e.to_string())?;
    let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), image.size[0] as u32, image.size[1] as u32);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let rgba: Vec<u8> = image.pixels.iter().flat_map(|c| c.to_array()).collect();
    encoder.write_header().and_then(|mut w| w.write_image_data(&rgba)).map_err(|e| e.to_string())
}
