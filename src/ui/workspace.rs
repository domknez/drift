//! The signed-in state: the tab picked, what the list shows, the open
//! conversation and thread, and keeping them fresh.

use super::load::Load;
use super::pictures::Pictures;
use crate::backend::{Backend, Error};
use crate::model::markdown::{self, Mention};
use crate::model::{ConversationId, Directory, Message, Picture, SearchHit, text};
use eframe::egui;
use std::collections::HashSet;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// How often the sidebar is fetched again (until the service pushes changes).
const DIRECTORY_EVERY: Duration = Duration::from_secs(60);
/// How often the open conversation is fetched again.
const MESSAGES_EVERY: Duration = Duration::from_secs(20);
/// Searching messages starts once typing has paused this long.
const SEARCH_PAUSE: Duration = Duration::from_millis(500);

pub struct Workspace {
    pub backend: Arc<dyn Backend>,
    pub section: Section,
    pub chip: Chip,
    /// The activity feed's items (mentions, replies), newest last; None until loaded.
    pub activity: Option<Vec<Message>>,
    activity_load: Load<Vec<Message>, Error>,
    activity_at: Option<Instant>,
    /// The post whose thread is open beside the conversation.
    pub thread: Option<String>,
    /// A message to show and mark (opened from Activity), until it has been scrolled to.
    pub focus: Option<String>,
    pub focus_scrolled: bool,
    /// The reply being typed in the open thread.
    pub reply_draft: String,
    /// A message quoted in the reply being written (true: in the thread's composer).
    pub quoting: Option<(bool, Message)>,
    /// One of the user's messages being edited (true: in the thread's composer).
    pub editing: Option<(bool, String)>,
    /// Pictures to send with the next message (true: in the thread's composer).
    pub attachments: Vec<(bool, Attachment)>,
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
    /// Finding in the open conversation: what's typed (None while not finding).
    pub find: Option<String>,
    /// Which of the found messages is shown (0 = the newest).
    pub find_index: usize,
    pub side: Side,
    /// What the search field holds: it narrows the list, and finds messages.
    pub filter: String,
    /// Messages found for a query, and the query.
    pub hits: Option<(String, Vec<SearchHit>)>,
    hits_load: Load<(String, Vec<SearchHit>), Error>,
    /// The query last seen, and when it last changed (searching waits for a pause).
    typed: (String, Instant),
    /// Sidebar sections folded away, by key.
    pub folded: HashSet<String>,
    /// The message being typed in the open conversation.
    pub draft: String,
    sending: Load<(), Error>,
    /// Writes in the background (reactions, read marks); true asks to fetch the
    /// messages again after.
    chores: Vec<Load<bool, Error>>,
    /// The conversation and message last marked read, so it's marked once.
    marked: Option<(String, String)>,
}

/// A picture waiting in a composer to go with the next message.
#[derive(Clone)]
pub struct Attachment {
    pub name: String,
    pub bytes: Arc<Vec<u8>>,
    pub mime: &'static str,
    pub size: (u32, u32),
}

/// The tab picked in the top bar: what the list shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Section {
    Activity,
    Chats,
    Teams,
    Meetings,
}

/// What the list panel shows: the list, what's found in the open conversation, or
/// the people in it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    List,
    Found,
    People,
}

/// The chip picked above the list of chats.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Chip {
    All,
    Unread,
    Channels,
    People,
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
            section: Section::Chats,
            chip: Chip::All,
            activity: None,
            activity_load: Load::Idle,
            activity_at: None,
            thread: None,
            focus: None,
            focus_scrolled: false,
            reply_draft: String::new(),
            quoting: None,
            editing: None,
            attachments: Vec::new(),
            directory: None,
            directory_load: Load::Idle,
            directory_at: None,
            open,
            messages: None,
            messages_load: Load::Idle,
            messages_at: None,
            error: None,
            find: None,
            find_index: 0,
            side: Side::List,
            filter: String::new(),
            hits: None,
            hits_load: Load::Idle,
            typed: (String::new(), Instant::now()),
            // Meeting chats pile up; they start folded.
            folded: HashSet::from(["meetings".to_string()]),
            draft: String::new(),
            sending: Load::Idle,
            chores: Vec::new(),
            marked: None,
        }
    }

    /// Whether a send is in flight (the composer shows it).
    pub fn sending(&self) -> bool {
        self.sending.is_running()
    }

    /// Send the draft to the open conversation, or the reply to the open thread (or
    /// save the edit being made); it's cleared and the view refreshes. The draft is
    /// Markdown; `@Name` of someone picked becomes a mention; a quote goes on top.
    pub fn send(&mut self, ctx: &egui::Context, reply: bool) {
        let Some(open) = self.open.clone() else { return };
        let to = if reply {
            let Some(root) = self.thread.clone() else { return };
            format!("{open};messageid={root}")
        } else {
            open.clone()
        };
        let text = self.draft(reply).trim().to_string();
        let pictures: Vec<Attachment> = self.attachments.iter().filter(|(r, _)| *r == reply).map(|(_, a)| a.clone()).collect();
        if (text.is_empty() && pictures.is_empty()) || self.sending.is_running() {
            return;
        }
        self.attachments.retain(|(r, _)| *r != reply);
        let mut message = if text.is_empty() { markdown::Outgoing::default() } else { markdown::to_html(&text, &self.people()) };
        if let Some(quoted) = self.quoting.take_if(|(r, _)| *r == reply).map(|(_, m)| m) {
            message.html = quote_html(&quoted) + &message.html;
        }
        let editing = self.editing.take_if(|(r, _)| *r == reply).map(|(_, id)| id);
        self.draft_mut(reply).clear();
        let backend = self.backend.clone();
        self.sending = Load::start(ctx, move || {
            // Pictures go up first; the message then shows them under its text.
            for picture in &pictures {
                let url = backend.upload_image(&to, &picture.name, &picture.bytes, picture.mime)?;
                let (w, h) = picture.size;
                let scale = (300.0 / w.max(1) as f32).min(1.0);
                message.html.push_str(&format!(
                    "<p><img itemscope=\"\" itemtype=\"http://schema.skype.com/AMSImage\" src=\"{url}\" width=\"{}\" height=\"{}\"></p>",
                    (w as f32 * scale).round(),
                    (h as f32 * scale).round()
                ));
            }
            match editing {
                // An edit of a reply in a thread still lives in the channel's conversation.
                Some(id) => backend.edit(&open, &id, &message),
                None => backend.send(&to, &message),
            }
        });
    }

    /// Add a picture to the composer (the thread's when `reply`); only pictures can go
    /// for now, so anything else is refused, with the reason.
    pub fn attach(&mut self, reply: bool, name: &str, bytes: Vec<u8>) {
        match super::pictures::image_info(&bytes) {
            Some((w, h, mime)) => {
                self.attachments.push((reply, Attachment { name: name.to_string(), bytes: Arc::new(bytes), mime, size: (w, h) }))
            }
            None => self.error = Some(format!("{name}: only pictures (PNG or JPEG) can be sent for now")),
        }
    }

    pub fn draft(&self, reply: bool) -> &String {
        if reply { &self.reply_draft } else { &self.draft }
    }

    pub fn draft_mut(&mut self, reply: bool) -> &mut String {
        if reply { &mut self.reply_draft } else { &mut self.draft }
    }

    /// Quote a message in the reply being written (in the thread when `reply`).
    pub fn quote(&mut self, message: Message, reply: bool) {
        self.editing = None;
        self.quoting = Some((reply, message));
    }

    /// Edit one of the user's messages in the composer (the thread's when `reply`).
    pub fn edit(&mut self, message: &Message, reply: bool) {
        self.quoting = None;
        self.editing = Some((reply, message.id.clone()));
        *self.draft_mut(reply) = text::to_markdown(&message.body);
    }

    /// ↑ in an empty composer: edit the user's last message there. False if there's none.
    pub fn edit_last(&mut self, reply: bool) -> bool {
        let me = self.me_id.clone();
        let thread = self.thread.clone();
        let last = self.messages.as_ref().and_then(|messages| {
            messages
                .iter()
                .rev()
                .find(|m| {
                    let here =
                        if reply { m.root == thread || Some(&m.id) == thread.as_ref() } else { m.root.is_none() || self.thread.is_none() };
                    here && m.author.id == me
                })
                .cloned()
        });
        match last {
            Some(m) => {
                self.edit(&m, reply);
                true
            }
            None => false,
        }
    }

    /// Stop editing or quoting (Esc).
    pub fn cancel_compose(&mut self, reply: bool) {
        if self.editing.as_ref().is_some_and(|(r, _)| *r == reply) {
            self.editing = None;
            self.draft_mut(reply).clear();
        }
        if self.quoting.as_ref().is_some_and(|(r, _)| *r == reply) {
            self.quoting = None;
        }
    }

    /// Delete one of the user's messages: gone at once, deleted behind.
    pub fn delete(&mut self, ctx: &egui::Context, message: &str) {
        let Some(open) = self.open.clone() else { return };
        if let Some(messages) = self.messages.as_mut() {
            messages.retain(|m| m.id != message);
        }
        let (backend, message) = (self.backend.clone(), message.to_string());
        self.chores.push(Load::start(ctx, move || backend.delete(&open, &message).map(|()| true)));
    }

    /// The people who can be mentioned: those the user chats with, and everyone who
    /// wrote in the open conversation.
    pub fn people(&self) -> Vec<Mention> {
        let mut people: Vec<Mention> = Vec::new();
        let mut add = |id: &str, name: &str| {
            if !id.is_empty() && !name.is_empty() && id != self.me_id && id.starts_with("8:") && !people.iter().any(|p| p.id == id) {
                people.push(Mention { id: id.to_string(), name: name.to_string() });
            }
        };
        for m in self.messages.iter().flatten().rev() {
            add(&m.author.id, &m.author.name);
        }
        for c in self.directory.iter().flat_map(|d| &d.conversations) {
            if let Some(Picture::Person { id, name }) = &c.picture {
                add(id, name);
            }
        }
        people
    }

    /// React to a message (or take the reaction back): shown at once, sent behind.
    pub fn react(&mut self, ctx: &egui::Context, message: &str, key: &str) {
        let Some(open) = self.open.clone() else { return };
        let Some(m) = self.messages.as_mut().and_then(|ms| ms.iter_mut().find(|m| m.id == message)) else { return };
        let add = !m.reactions.iter().any(|r| r.key == key && r.mine);
        match m.reactions.iter_mut().find(|r| r.key == key) {
            Some(r) => {
                r.mine = add;
                r.count = if add { r.count + 1 } else { r.count.saturating_sub(1) };
            }
            None if add => m.reactions.push(crate::model::Reaction { key: key.to_string(), count: 1, mine: true }),
            None => {}
        }
        m.reactions.retain(|r| r.count > 0);
        let (backend, message, key) = (self.backend.clone(), message.to_string(), key.to_string());
        self.chores.push(Load::start(ctx, move || backend.react(&open, &message, &key, add).map(|()| true)));
    }

    /// Open the message an activity item is about: its conversation, its thread if it's
    /// a reply, and the message itself marked.
    pub fn open_message(&mut self, conversation: &str, message: Option<String>, root: Option<String>) {
        self.open(conversation);
        self.thread = root;
        self.focus = message;
        self.focus_scrolled = false;
    }

    /// The open conversation's messages that match what's being found, newest first.
    pub fn found(&self) -> Vec<Message> {
        let Some(query) = self.find.as_deref().map(str::trim).filter(|q| !q.is_empty()).map(str::to_lowercase) else { return Vec::new() };
        self.messages
            .iter()
            .flatten()
            .rev()
            .filter(|m| text::flatten(&m.body).to_lowercase().contains(&query) || m.author.name.to_lowercase().contains(&query))
            .cloned()
            .collect()
    }

    /// Start finding in the open conversation; the list panel shows what's found.
    pub fn start_find(&mut self) {
        self.find = Some(String::new());
        self.find_index = 0;
        self.side = Side::Found;
    }

    pub fn stop_find(&mut self) {
        self.find = None;
        self.focus = None;
        if self.side == Side::Found {
            self.side = Side::List;
        }
    }

    /// Show the found message `index` (wrapping around): marked and scrolled to.
    pub fn show_found(&mut self, index: usize) {
        let found = self.found();
        if found.is_empty() {
            self.focus = None;
            return;
        }
        self.find_index = index % found.len();
        let m = &found[self.find_index];
        // A reply in a channel shows in its thread.
        if m.root.is_some() && self.thread.as_ref() != m.root.as_ref() {
            self.thread = m.root.clone();
        }
        self.focus = Some(m.id.clone());
        self.focus_scrolled = false;
    }

    /// Search messages for what's in the search field now.
    pub fn search(&mut self, ctx: &egui::Context) {
        let query = self.filter.trim().to_string();
        if query.is_empty() {
            self.hits = None;
            return;
        }
        let backend = self.backend.clone();
        self.hits_load = Load::start(ctx, move || backend.search(&query).map(|hits| (query, hits)));
    }

    /// Pin or unpin a conversation: shown at once, sent behind.
    pub fn set_pinned(&mut self, ctx: &egui::Context, id: &str, pinned: bool) {
        if let Some(c) = self.directory.as_mut().and_then(|d| d.conversations.iter_mut().find(|c| c.id == id)) {
            c.pinned = pinned;
        }
        let (backend, id) = (self.backend.clone(), id.to_string());
        self.chores.push(Load::start(ctx, move || backend.pin(&id, pinned).map(|()| false)));
    }

    /// Mark the open conversation read up to its newest message, once per message.
    fn mark_read(&mut self, ctx: &egui::Context) {
        let (Some(open), Some(last)) = (self.open.clone(), self.messages.as_ref().and_then(|m| m.last()).map(|m| m.id.clone())) else {
            return;
        };
        if self.marked.as_ref() == Some(&(open.clone(), last.clone())) {
            return;
        }
        self.marked = Some((open.clone(), last.clone()));
        let backend = self.backend.clone();
        self.chores.push(Load::start(ctx, move || backend.mark_read(&open, &last).map(|()| false)));
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
        let mut refetch = false;
        for chore in &mut self.chores {
            match chore.take() {
                Some(Ok(again)) => refetch |= again,
                Some(Err(Error::SignedOut)) => return Some(Outcome::SignedOut),
                Some(Err(e)) => log_warn!("{e}"),
                None => {}
            }
        }
        self.chores.retain(Load::is_running);
        if refetch {
            self.messages_at = None;
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
        if let Some(result) = self.hits_load.take() {
            match result {
                Ok(hits) => self.hits = Some(hits),
                Err(Error::SignedOut) => return Some(Outcome::SignedOut),
                Err(e) => self.failed(e),
            }
        }
        // Search messages once typing pauses.
        let query = self.filter.trim().to_string();
        if query != self.typed.0 {
            self.typed = (query.clone(), Instant::now());
        }
        if query.chars().count() >= 2
            && self.typed.1.elapsed() >= SEARCH_PAUSE
            && self.hits.as_ref().is_none_or(|(q, _)| *q != query)
            && !self.hits_load.is_running()
        {
            self.search(ctx);
        } else if query.chars().count() >= 2 && self.typed.1.elapsed() < SEARCH_PAUSE {
            ctx.request_repaint_after(SEARCH_PAUSE);
        }
        if let Some(result) = self.activity_load.take() {
            match result {
                Ok(items) => self.activity = Some(items),
                Err(Error::SignedOut) => return Some(Outcome::SignedOut),
                Err(e) => self.failed(e),
            }
        }
        if let Some(result) = self.messages_load.take() {
            match result {
                Ok((id, messages)) if Some(&id) == self.open.as_ref() => {
                    // A message to show that's a reply: its thread opens too.
                    if self.thread.is_none()
                        && let Some(root) =
                            self.focus.as_ref().and_then(|f| messages.iter().find(|m| &m.id == f)).and_then(|m| m.root.clone())
                    {
                        self.thread = Some(root);
                    }
                    self.messages = Some(messages);
                    self.error = None;
                    self.mark_read(ctx);
                    // Finding while the messages came in: show the newest match.
                    if self.side == Side::Found && self.focus.is_none() && self.find.as_deref().is_some_and(|q| !q.trim().is_empty()) {
                        self.show_found(0);
                    }
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
        if self.section == Section::Activity
            && !self.activity_load.is_running()
            && due(self.activity_at, MESSAGES_EVERY)
            && let Some(feed) = self.directory.as_ref().and_then(|d| d.activity()).map(|c| c.id.clone())
        {
            self.activity_at = Some(Instant::now());
            let backend = self.backend.clone();
            self.activity_load = Load::start(ctx, move || backend.messages(&feed));
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
        self.focus = None;
        self.find = None;
        self.side = Side::List;
        self.draft.clear();
        self.reply_draft.clear();
        self.quoting = None;
        self.editing = None;
        self.attachments.clear();
        // Read here; Teams itself still counts it until it's read there.
        if let Some(c) = self.directory.as_mut().and_then(|d| d.conversations.iter_mut().find(|c| c.id == id)) {
            c.unread = false;
        }
    }

    /// Fetch everything again now.
    pub fn refresh(&mut self) {
        self.directory_at = None;
        self.messages_at = None;
        self.activity_at = None;
    }
}

/// The Teams markup of a quoted message, put above the reply.
fn quote_html(m: &Message) -> String {
    let esc = |s: &str| s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");
    let preview: String = text::flatten(&m.body).chars().take(240).collect();
    format!(
        "<blockquote itemscope=\"\" itemtype=\"http://schema.skype.com/Reply\" itemid=\"{id}\"><strong itemprop=\"mri\" itemid=\"{mri}\">{name}</strong><span itemprop=\"time\" itemid=\"{id}\"></span><p itemprop=\"preview\">{preview}</p></blockquote>",
        id = m.id,
        mri = esc(&m.author.id),
        name = esc(&m.author.name),
        preview = esc(&preview),
    )
}
