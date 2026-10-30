//! The signed-in state: the rail's section, what the list shows, the open
//! conversation and thread, and keeping them fresh.

use super::load::Load;
use super::pictures::Pictures;
use crate::backend::{Backend, Error};
use crate::model::{ConversationId, Directory, Message};
use eframe::egui;
use std::collections::HashSet;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// How often the sidebar is fetched again (until the service pushes changes).
const DIRECTORY_EVERY: Duration = Duration::from_secs(60);
/// How often the open conversation is fetched again.
const MESSAGES_EVERY: Duration = Duration::from_secs(20);

pub struct Workspace {
    pub backend: Arc<dyn Backend>,
    pub section: Section,
    /// The post whose thread is open beside the conversation.
    pub thread: Option<String>,
    /// The reply being typed in the open thread.
    pub reply_draft: String,
    pub me: String,
    /// The signed-in user's id (for their picture).
    pub me_id: String,
    pub pictures: Pictures,
    /// What the sidebar lists; None until the first load.
    pub directory: Option<Directory>,
    directory_load: Load<Directory, Error>,
    directory_at: Option<Instant>,
    pub open: Option<ConversationId>,
    /// The open conversation's messages, oldest first; None while they load.
    pub messages: Option<Vec<Message>>,
    messages_load: Load<(ConversationId, Vec<Message>), Error>,
    messages_at: Option<Instant>,
    /// The last failure, shown until the next success.
    pub error: Option<String>,
    /// What the sidebar's search field holds.
    pub filter: String,
    /// Sidebar sections folded away, by key.
    pub folded: HashSet<String>,
    /// The message being typed in the open conversation.
    pub draft: String,
    sending: Load<(), Error>,
}

/// What the rail picks for the list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Section {
    Activity,
    Chats,
    Teams,
    Meetings,
}

/// What the workspace needs the app to do.
pub enum Outcome {
    /// The service no longer takes the sign-in.
    SignedOut,
}

impl Workspace {
    pub fn new(ctx: &egui::Context, backend: Arc<dyn Backend>, open: Option<ConversationId>) -> Workspace {
        Workspace {
            me: backend.me(),
            me_id: backend.me_id(),
            pictures: Pictures::new(ctx, backend.clone()),
            backend,
            section: Section::Teams,
            thread: None,
            reply_draft: String::new(),
            directory: None,
            directory_load: Load::Idle,
            directory_at: None,
            open,
            messages: None,
            messages_load: Load::Idle,
            messages_at: None,
            error: None,
            filter: String::new(),
            // Meeting chats pile up; they start folded.
            folded: HashSet::from(["meetings".to_string()]),
            draft: String::new(),
            sending: Load::Idle,
        }
    }

    /// Whether a send is in flight (the composer shows it).
    pub fn sending(&self) -> bool {
        self.sending.is_running()
    }

    /// Send the draft to the open conversation, or the reply to the open thread; it's
    /// cleared and the view refreshes.
    pub fn send(&mut self, ctx: &egui::Context, reply: bool) {
        let Some(open) = self.open.clone() else { return };
        let (draft, to) = if reply {
            let Some(root) = self.thread.clone() else { return };
            (&mut self.reply_draft, format!("{open};messageid={root}"))
        } else {
            (&mut self.draft, open)
        };
        let text = draft.trim().to_string();
        if text.is_empty() || self.sending.is_running() {
            return;
        }
        draft.clear();
        let backend = self.backend.clone();
        self.sending = Load::start(ctx, move || backend.send(&to, &text));
    }

    /// Collect finished loads and start the ones that are due.
    pub fn tick(&mut self, ctx: &egui::Context) -> Option<Outcome> {
        self.pictures.begin_frame();
        if let Some(result) = self.sending.take() {
            match result {
                Ok(()) => {
                    self.error = None;
                    self.messages_at = None; // show the sent message right away
                }
                Err(Error::SignedOut) => return Some(Outcome::SignedOut),
                Err(e) => self.failed(e),
            }
        }
        if let Some(result) = self.directory_load.take() {
            match result {
                Ok(directory) => {
                    if self.me.is_empty() {
                        self.me = self.backend.me();
                        self.me_id = self.backend.me_id();
                    }
                    self.directory = Some(directory);
                    self.error = None;
                }
                Err(Error::SignedOut) => return Some(Outcome::SignedOut),
                Err(e) => self.failed(e),
            }
        }
        if let Some(result) = self.messages_load.take() {
            match result {
                Ok((id, messages)) if Some(&id) == self.open.as_ref() => {
                    self.messages = Some(messages);
                    self.error = None;
                }
                Ok(_) => {}
                Err(Error::SignedOut) => return Some(Outcome::SignedOut),
                Err(e) => self.failed(e),
            }
        }

        let due = |at: Option<Instant>, every| at.is_none_or(|at: Instant| at.elapsed() >= every);
        if !self.directory_load.is_running() && due(self.directory_at, DIRECTORY_EVERY) {
            self.directory_at = Some(Instant::now());
            let backend = self.backend.clone();
            self.directory_load = Load::start(ctx, move || backend.directory());
        }
        if let Some(id) = self.open.clone()
            && !self.messages_load.is_running()
            && due(self.messages_at, MESSAGES_EVERY)
        {
            self.messages_at = Some(Instant::now());
            let backend = self.backend.clone();
            self.messages_load = Load::start(ctx, move || backend.messages(&id).map(|m| (id, m)));
        }
        // Wake up for the next refresh even when nothing moves.
        ctx.request_repaint_after(MESSAGES_EVERY);
        None
    }

    fn failed(&mut self, e: Error) {
        log_warn!("{e}");
        self.error = Some(e.to_string());
    }

    /// Open a conversation; its messages load right away.
    pub fn open(&mut self, id: &str) {
        if self.open.as_deref() == Some(id) {
            return;
        }
        self.open = Some(id.to_string());
        self.messages = None;
        self.messages_load = Load::Idle;
        self.messages_at = None;
        self.thread = None;
        self.draft.clear();
        self.reply_draft.clear();
        // Read here; Teams itself still counts it until it's read there.
        if let Some(c) = self.directory.as_mut().and_then(|d| d.conversations.iter_mut().find(|c| c.id == id)) {
            c.unread = false;
        }
    }

    /// Fetch everything again now.
    pub fn refresh(&mut self) {
        self.directory_at = None;
        self.messages_at = None;
    }
}
