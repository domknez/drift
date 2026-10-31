//! Updates in the background: check, download and stage a new release quietly, then
//! offer "Restart to update" in the top bar. An update staged but not yet applied
//! installs when drift quits.

use crate::update::{self, Release, Restart, Staged};
use eframe::egui;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::{Receiver, TryRecvError};
use std::time::{Duration, Instant};

const INTERVAL: Duration = Duration::from_secs(6 * 60 * 60);
/// Let start-up traffic (chats, messages, pictures) go first.
const FIRST_CHECK: Duration = Duration::from_secs(15);

pub enum State {
    UpToDate,
    /// Downloading and unpacking in the background; nothing in the top bar yet.
    Downloading {
        release: Release,
        progress: Arc<AtomicU32>,
        done: Receiver<Result<Staged, String>>,
    },
    /// Staged next to this copy: one click swaps it in and restarts.
    Ready(Release, Staged),
    /// drift can't replace itself here (a build of your own, a read-only folder):
    /// offer the release page.
    Manual(Release),
    /// Swapping in failed; the button falls back to the release page.
    Failed(Release, String),
}

/// What the top bar offers.
pub struct Offer {
    pub label: String,
    /// Installs and restarts (rather than opening the release page).
    pub installs: bool,
    pub hint: String,
}

pub struct Updates {
    /// "Update automatically" (setting): check now and then, and download what's new.
    pub auto: bool,
    pub state: State,
    checking: Option<Receiver<Result<Option<Release>, String>>>,
    /// A check has finished since start-up.
    checked: bool,
    next_check: Instant,
    /// Why the last check or download failed, for the menu.
    last_error: Option<String>,
}

impl Updates {
    pub fn new(auto: bool) -> Updates {
        Updates { auto, state: State::UpToDate, checking: None, checked: false, next_check: Instant::now() + FIRST_CHECK, last_error: None }
    }

    /// Never checks (the demo); `DRIFT_DEMO_UPDATE=<version>` shows that version as out.
    pub fn demo() -> Updates {
        let mut updates = Updates::new(false);
        if let Ok(version) = std::env::var("DRIFT_DEMO_UPDATE") {
            updates.state = State::Manual(Release::demo(&version));
        }
        updates
    }

    pub fn checking(&self) -> bool {
        self.checking.is_some()
    }

    /// One line for the menu.
    pub fn status(&self) -> String {
        match &self.state {
            _ if self.checking.is_some() => "Checking…".into(),
            State::Downloading { release, progress, .. } => {
                format!("Downloading {}… {}%", release.version, progress.load(Ordering::Relaxed) / 10)
            }
            State::Ready(release, _) => format!("Version {} is ready to install", release.version),
            State::Manual(release) | State::Failed(release, _) => format!("Version {} is available", release.version),
            State::UpToDate => match &self.last_error {
                Some(_) => "Couldn't check for updates".into(),
                None if self.checked => "You have the latest version".into(),
                None => "Free and open source".into(),
            },
        }
    }

    /// The status is news (an update), or trouble.
    pub fn notable(&self) -> Option<bool> {
        match &self.state {
            State::Ready(..) | State::Manual(_) => Some(true),
            State::Failed(..) => Some(false),
            State::UpToDate if self.checking.is_none() && self.last_error.is_some() => Some(false),
            _ => None,
        }
    }

    /// What the top bar offers, if anything.
    pub fn offer(&self) -> Option<Offer> {
        match &self.state {
            State::Ready(release, _) => Some(Offer {
                label: "Restart to update".into(),
                installs: true,
                hint: format!("Installs drift {} and opens it again", release.version),
            }),
            State::Manual(release) => {
                Some(Offer { label: format!("drift {} is out", release.version), installs: false, hint: "Open the download page".into() })
            }
            State::Failed(_, e) => {
                Some(Offer { label: "Update failed".into(), installs: false, hint: format!("{e}\nClick to download it yourself") })
            }
            State::UpToDate | State::Downloading { .. } => None,
        }
    }

    /// Start a check now (the menu's Check, or when one is due).
    pub fn check_now(&mut self, ctx: &egui::Context) {
        let busy = matches!(self.state, State::Downloading { .. } | State::Ready(..));
        if self.checking.is_some() || busy {
            return;
        }
        let (tx, rx) = std::sync::mpsc::channel();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(update::check());
            ctx.request_repaint();
        });
        self.checking = Some(rx);
        self.next_check = Instant::now() + INTERVAL;
    }

    /// Call every frame: runs checks when due and collects what the background brought.
    pub fn poll(&mut self, ctx: &egui::Context) {
        if self.auto {
            if Instant::now() >= self.next_check {
                self.check_now(ctx);
            }
            ctx.request_repaint_after(self.next_check.saturating_duration_since(Instant::now()));
        }
        if let Some(rx) = &self.checking {
            match rx.try_recv() {
                Ok(result) => {
                    self.checking = None;
                    self.checked = true;
                    match result {
                        Ok(Some(release)) => self.found(ctx, release),
                        Ok(None) => self.last_error = None,
                        Err(e) => {
                            log_warn!("checking for updates: {e}");
                            self.last_error = Some(e);
                        }
                    }
                }
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => self.checking = None,
            }
        }
        if let State::Downloading { done, release, .. } = &self.state {
            ctx.request_repaint_after(Duration::from_millis(500));
            match done.try_recv() {
                Ok(Ok(staged)) => {
                    log_info!("update: {} staged, waiting for a restart", staged.version);
                    self.state = State::Ready(release.clone(), staged);
                }
                // Tried again at the next check.
                Ok(Err(e)) => {
                    log_warn!("update download failed: {e}");
                    self.last_error = Some(e);
                    self.state = State::UpToDate;
                }
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => self.state = State::UpToDate,
            }
        }
    }

    /// A newer release is out: download it in the background, or offer its page.
    fn found(&mut self, ctx: &egui::Context, release: Release) {
        self.last_error = None;
        if !update::can_install(&release) {
            log_info!("update: {} is out (to install by hand)", release.version);
            self.state = State::Manual(release);
            return;
        }
        log_info!("update: {} is out, downloading", release.version);
        let progress = Arc::new(AtomicU32::new(0));
        let (tx, done) = std::sync::mpsc::channel();
        let (p, ctx, pending) = (progress.clone(), ctx.clone(), release.clone());
        std::thread::spawn(move || {
            let _ = tx.send(update::stage(&pending, &p));
            ctx.request_repaint();
        });
        self.state = State::Downloading { release, progress, done };
    }

    /// The top bar's offer was clicked: swap the update in and restart (the caller
    /// relaunches and quits), or open the release page.
    pub fn act(&mut self, ctx: &egui::Context) -> Option<Restart> {
        match std::mem::replace(&mut self.state, State::UpToDate) {
            State::Ready(release, staged) => match update::apply(&staged) {
                Ok(restart) => return Some(restart),
                Err(e) => {
                    log_warn!("update failed: {e}");
                    self.state = State::Failed(release, e);
                }
            },
            State::Manual(release) | State::Failed(release, _) => {
                ctx.open_url(egui::OpenUrl::new_tab(&release.page));
                self.state = State::Manual(release);
            }
            other => self.state = other,
        }
        None
    }

    /// drift is quitting: install a staged update so the next start is the new version.
    pub fn apply_on_exit(&mut self) {
        if let State::Ready(_, staged) = &self.state {
            match update::apply(staged) {
                Ok(_) => log_info!("update: installed on quit"),
                Err(e) => log_warn!("update on quit failed: {e}"),
            }
        }
    }
}
