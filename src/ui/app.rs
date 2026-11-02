//! The app: the sign-in page or the workspace, the theme, and moving between them.

use super::load::Load;
use super::style;
use super::updates::Updates;
use super::views;
use super::workspace::{Outcome, Section, Workspace};
use crate::backend::Backend;
use crate::demo;
use crate::settings::{Accent, Appearance, Prefs, Theme};
use crate::teams::Teams;
use crate::teams::auth::Session;
use eframe::egui;
use std::sync::Arc;

pub struct App {
    prefs: Prefs,
    /// The theme, accent and size last applied, and the size the smallest window was set for.
    applied: Option<(Theme, Accent, u16)>,
    min_size_for: Option<u16>,
    /// The list's width changed by a drag not yet saved.
    list_width_changed: bool,
    screen: Screen,
    updates: Updates,
    /// `DRIFT_SCREENSHOT=<file.png>`: save the window once it has loaded, then quit.
    capture: Option<Capture>,
    /// Drafts are kept across restarts (not in the demo): as last saved, and when.
    keep_drafts: bool,
    saved_drafts: Option<std::collections::HashMap<String, String>>,
    drafts_at: Option<std::time::Instant>,
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
    /// The theme, accent or size changed.
    Look(crate::settings::Look),
    Notifications(bool),
    /// Turn updating automatically on or off.
    AutoUpdate(bool),
    CheckForUpdates,
    /// The update offer was clicked: install and restart, or open the release page.
    InstallUpdate,
    SignOut,
}

impl App {
    pub fn new(cc: &eframe::CreationContext) -> App {
        style::install_fonts(&cc.egui_ctx);
        // Room for the passes a frame can ask for (a new table or card measures itself,
        // a conversation's first drawing and a change in its height lay out again):
        // past the limit egui shows the half-measured frame, and the list jumps.
        cc.egui_ctx.options_mut(|o| o.max_passes = std::num::NonZeroUsize::new(4).expect("not zero"));
        crate::notify::init();
        let capture = std::env::var_os("DRIFT_SCREENSHOT").map(|path| Capture { path: path.into(), frames: 0, requested: false });
        // `DRIFT_DEMO=1`: the made-up workspace, with settings that are never saved.
        if std::env::var_os("DRIFT_DEMO").is_some() {
            let theme = std::env::var("DRIFT_THEME").unwrap_or_default();
            // DRIFT_PREFS: more settings, `key=value` separated by `;`.
            let more = std::env::var("DRIFT_PREFS").unwrap_or_default().replace(';', "\n");
            let prefs = Prefs::parse(&format!("theme={theme}\n{more}"));
            // DRIFT_DEMO_OPEN picks the conversation, DRIFT_DEMO_TAB the tab.
            let open = std::env::var("DRIFT_DEMO_OPEN").unwrap_or_else(|_| demo::OPEN.to_string());
            let mut workspace = Workspace::new(&cc.egui_ctx, Arc::new(demo::Demo), Some(open.clone()));
            if open == demo::OPEN {
                workspace.thread = Some(demo::THREAD.to_string());
            }
            // DRIFT_DEMO_FOCUS=<message>[,<post>]: open on a message, as a click in Activity does.
            if let Ok(focus) = std::env::var("DRIFT_DEMO_FOCUS") {
                let mut parts = focus.split(',').map(str::to_string);
                let (message, root) = (parts.next(), parts.next());
                workspace.open_message(&open, message, root);
            }
            // DRIFT_DEMO_SWITCHER=<query>: the quick switcher open, with that typed.
            if let Ok(query) = std::env::var("DRIFT_DEMO_SWITCHER") {
                workspace.switcher = Some(super::views::switcher::Switcher::with(&query));
            }
            if let Ok(path) = std::env::var("DRIFT_DEMO_ATTACH")
                && let Ok(bytes) = std::fs::read(&path)
            {
                workspace.attach(false, path.rsplit('/').next().unwrap_or("picture.png"), bytes);
            }
            if std::env::var_os("DRIFT_DEMO_TOAST").is_some() {
                workspace.toasts.push(
                    "Priya Nair",
                    Some("Release crew"),
                    "The cut moved to 16:00, a test is still red on main.",
                    "19:release",
                );
            }
            if let Ok(find) = std::env::var("DRIFT_DEMO_FIND") {
                workspace.start_find();
                workspace.find = Some(find);
            }
            if std::env::var_os("DRIFT_DEMO_PEOPLE").is_some() {
                workspace.side = crate::ui::workspace::Side::People;
            }
            // DRIFT_DEMO_EMOJI=<search>: the picker open, with that searched.
            if let Ok(query) = std::env::var("DRIFT_DEMO_EMOJI") {
                workspace.emoji_picker.query = query;
            }
            if let Ok(filter) = std::env::var("DRIFT_DEMO_FILTER") {
                workspace.filter = filter;
            }
            // DRIFT_DEMO_DRAFT: text in the composer; DRIFT_DEMO_QUOTE: a message quoted in it.
            if let Ok(draft) = std::env::var("DRIFT_DEMO_DRAFT") {
                workspace.draft = draft;
                cc.egui_ctx.memory_mut(|m| m.request_focus(egui::Id::unique("composer")));
            }
            if let Ok(id) = std::env::var("DRIFT_DEMO_QUOTE")
                && let Some(m) = demo::Demo.messages(&open).ok().and_then(|ms| ms.into_iter().find(|m| m.id == id))
            {
                workspace.quote(m, false);
            }
            workspace.section = match std::env::var("DRIFT_DEMO_TAB").as_deref() {
                Ok("activity") => Section::Activity,
                Ok("teams") => Section::Teams,
                Ok("meetings") => Section::Meetings,
                _ => Section::Chats,
            };
            return App {
                prefs,
                applied: None,
                min_size_for: None,
                list_width_changed: false,
                screen: Screen::Signed(Box::new(workspace)),
                updates: Updates::demo(),
                capture,
                keep_drafts: false,
                saved_drafts: None,
                drafts_at: None,
            };
        }
        let prefs = Prefs::load();
        let screen = match Session::stored() {
            Some(session) => {
                Screen::Signed(Box::new(Workspace::new(&cc.egui_ctx, Arc::new(Teams::new(session)), prefs.conversation.clone())))
            }
            None => Screen::SignIn(SignIn::new(None)),
        };
        let updates = Updates::new(prefs.auto_update);
        App {
            prefs,
            applied: None,
            min_size_for: None,
            list_width_changed: false,
            screen,
            updates,
            capture,
            keep_drafts: true,
            saved_drafts: None,
            drafts_at: None,
        }
    }

    /// Take the screenshot once the workspace has loaded, save it and quit.
    fn capture(&mut self, ctx: &egui::Context) {
        let Some(capture) = &mut self.capture else { return };
        // Past the start-up screen (unless it's what to capture).
        let past_start_up = |ws: &Workspace| {
            std::env::var_os("DRIFT_DEMO_SPLASH").is_some() || ws.ready_at.is_some_and(|at| at.elapsed().as_secs_f32() > 0.5)
        };
        let ready = matches!(&self.screen, Screen::Signed(ws) if ws.directory.is_some() && ws.messages.is_some() && past_start_up(ws))
            || matches!(&self.screen, Screen::SignIn(_));
        if !ready {
            return;
        }
        capture.frames += 1;
        ctx.request_repaint();
        if std::env::var_os("DRIFT_DEMO_FILTER").is_some() {
            ctx.memory_mut(|m| m.request_focus(egui::Id::unique("search")));
        }
        // A demo draft is shown as being typed (so @-suggestions open).
        if std::env::var_os("DRIFT_DEMO_DRAFT").is_some() {
            ctx.memory_mut(|m| m.request_focus(egui::Id::unique("composer")));
        }
        // DRIFT_DEMO_SCROLL=<points>: the list scrolled up there just before the shot.
        if capture.frames == 20
            && let Screen::Signed(ws) = &mut self.screen
            && let Some(y) = std::env::var("DRIFT_DEMO_SCROLL").ok().and_then(|y| y.parse().ok())
        {
            ws.scroll_to = Some(y);
        }
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
        let theme = if dark { self.prefs.dark_theme } else { self.prefs.light_theme };
        let now = (theme, self.prefs.accent, self.prefs.text_size);
        if self.applied != Some(now) {
            self.applied = Some(now);
            style::apply(ctx, theme, self.prefs.accent, f32::from(self.prefs.text_size) / 100.0);
        }
        // The smallest window grows with the text, so the panels still fit: its size is
        // in points, which the size scales, once the size is in effect.
        let zoom = f32::from(self.prefs.text_size) / 100.0;
        if self.min_size_for != Some(self.prefs.text_size) && (ctx.zoom_factor() - zoom).abs() < 0.001 {
            self.min_size_for = Some(self.prefs.text_size);
            let [w, h] = super::metrics::MIN_WINDOW_SIZE;
            ctx.send_viewport_cmd(egui::ViewportCommand::MinInnerSize(egui::vec2(w, h)));
        }
    }

    fn handle(&mut self, ctx: &egui::Context, request: Request) {
        match request {
            Request::Look(look) => self.prefs.set_look(look),
            Request::Notifications(on) => self.prefs.update(|p| p.notifications = on),
            Request::AutoUpdate(on) => {
                self.updates.auto = on;
                self.prefs.update(|p| p.auto_update = on);
            }
            Request::CheckForUpdates => self.updates.check_now(ctx),
            Request::InstallUpdate => {
                if let Some(restart) = self.updates.act(ctx) {
                    crate::update::relaunch(&restart);
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
            Request::SignOut => {
                Session::sign_out();
                // What was being written is the account's: it goes with it.
                if self.keep_drafts {
                    crate::settings::save_drafts(&Default::default());
                }
                self.saved_drafts = None;
                self.prefs.update(|p| p.conversation = None);
                self.screen = Screen::SignIn(SignIn::new(None));
            }
        }
    }
}

impl eframe::App for App {
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        if let (true, Screen::Signed(workspace)) = (self.keep_drafts, &self.screen) {
            crate::settings::save_drafts(&workspace.all_drafts());
        }
        self.updates.apply_on_exit();
    }

    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        // The traffic lights level with the top bar's middle (the size changes it).
        let zoom = f64::from(ctx.zoom_factor());
        super::lights::place(frame, f64::from(super::metrics::TOP_BAR / 2.0) * zoom, 20.0);
        self.apply_theme(&ctx);
        self.updates.poll(&ctx);
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
                workspace.notifications = self.prefs.notifications;
                if workspace.muted_here.is_none() {
                    workspace.muted_here = Some(self.prefs.muted.iter().cloned().collect());
                }
                if workspace.following.is_none() {
                    workspace.following = Some(self.prefs.following.iter().cloned().collect());
                }
                if !workspace.drafts_loaded {
                    workspace.drafts_loaded = true;
                    // The list's width as it was left (read with the drafts, once).
                    if let Some(width) = self.prefs.list_width {
                        workspace.list_width = f32::from(width);
                    }
                    if self.keep_drafts {
                        let mut saved = crate::settings::load_drafts();
                        if let Some(open) = &workspace.open
                            && workspace.draft.is_empty()
                        {
                            workspace.draft = saved.remove(open).unwrap_or_default();
                        }
                        workspace.drafts.extend(saved);
                        self.saved_drafts = Some(workspace.all_drafts());
                    }
                }
                if workspace.recent_emoji.is_none() {
                    workspace.recent_emoji = Some(self.prefs.recent_emoji.clone());
                }
                if workspace.hidden.is_none() {
                    workspace.hidden = Some(self.prefs.hidden.iter().cloned().collect());
                }
                if !workspace.folded_loaded {
                    workspace.folded_loaded = true;
                    if let Some(folded) = &self.prefs.folded {
                        workspace.folded = folded.iter().cloned().collect();
                    }
                }
                let folded_before = workspace.folded.clone();
                let muted_before = workspace.muted_here.clone();
                let following_before = workspace.following.clone();
                let width_before = workspace.list_width;
                let recent_before = workspace.recent_emoji.clone();
                let hidden_before = workspace.hidden.clone();
                request = views::workspace(ui, workspace, self.prefs.look(), &self.updates);
                if workspace.open != before {
                    let open = workspace.open.clone();
                    self.prefs.update(|p| p.conversation = open);
                }
                if workspace.folded != folded_before {
                    let mut folded: Vec<String> = workspace.folded.iter().cloned().collect();
                    folded.sort();
                    self.prefs.update(|p| p.folded = Some(folded));
                }
                if workspace.hidden != hidden_before {
                    let mut hidden: Vec<(String, i64)> = workspace.hidden.iter().flatten().map(|(id, at)| (id.clone(), *at)).collect();
                    hidden.sort();
                    self.prefs.update(|p| p.hidden = hidden);
                }
                if workspace.recent_emoji != recent_before {
                    let recent = workspace.recent_emoji.clone().unwrap_or_default();
                    self.prefs.update(|p| p.recent_emoji = recent);
                }
                // Drafts saved a second after they change at most, and at quitting.
                if self.keep_drafts {
                    let drafts = workspace.all_drafts();
                    if self.saved_drafts.as_ref() != Some(&drafts) {
                        if self.drafts_at.is_none_or(|at| at.elapsed() >= std::time::Duration::from_secs(1)) {
                            crate::settings::save_drafts(&drafts);
                            self.saved_drafts = Some(drafts);
                            self.drafts_at = Some(std::time::Instant::now());
                        } else {
                            ctx.request_repaint_after(std::time::Duration::from_secs(1));
                        }
                    }
                }
                if workspace.muted_here != muted_before {
                    let mut muted: Vec<String> = workspace.muted_here.iter().flatten().cloned().collect();
                    muted.sort();
                    self.prefs.update(|p| p.muted = muted);
                }
                // Saved once the drag ends (not on every frame of it).
                if workspace.list_width != width_before {
                    self.list_width_changed = true;
                }
                if self.list_width_changed && !ui.input(|i| i.pointer.any_down()) {
                    self.list_width_changed = false;
                    let width = workspace.list_width.round() as u16;
                    self.prefs.update(|p| p.list_width = (f32::from(width) != super::metrics::LIST_WIDTH).then_some(width));
                }
                if workspace.following != following_before {
                    let mut following: Vec<String> = workspace.following.iter().flatten().cloned().collect();
                    following.sort();
                    self.prefs.update(|p| p.following = following);
                }
            }
        }
        if let Some(request) = request {
            self.handle(&ctx, request);
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
