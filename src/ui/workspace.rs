//! The signed-in state: the tab picked, what the list shows, the open
//! conversation and thread, and keeping them fresh.

use super::load::Load;
use super::pictures::Pictures;
use crate::backend::{Backend, Error, Push};
use crate::model::markdown::{self, Mention};
use crate::model::{
    Availability, Conversation, ConversationId, Directory, Kind, Meeting, Message, Picture, Presence, SearchHit, Status, text,
};
use eframe::egui;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender};
use std::time::{Duration, Instant};

/// How often the sidebar is fetched again.
const DIRECTORY_EVERY: Duration = Duration::from_secs(10);
/// How often the sidebar and the open conversation are fetched again while the
/// service pushes changes (in case a push goes missing).
const PUSHED_EVERY: Duration = Duration::from_secs(60);
/// How often the open conversation is fetched again.
const MESSAGES_EVERY: Duration = Duration::from_secs(20);
/// How often the open conversation is fetched again while drift is in front.
const MESSAGES_FRONT_EVERY: Duration = Duration::from_secs(5);
/// How many of a conversation's newest messages are shown at first, and how many
/// more each "Show earlier" adds.
pub const NEWEST: usize = 40;
/// How often the calendar is fetched again.
const CALENDAR_EVERY: Duration = Duration::from_secs(5 * 60);
/// How often the user's status is fetched again (it changes in other clients too).
const STATUS_EVERY: Duration = Duration::from_secs(60);
/// How often the presence of the people in one-to-one chats is fetched again.
const PRESENCE_EVERY: Duration = Duration::from_secs(60);
/// How often drift tells the presence service the user is (or isn't) at the computer.
const ACTIVITY_EVERY: Duration = Duration::from_secs(120);
/// Idle this long (s), the user shows as away.
const IDLE_AFTER: f64 = 5.0 * 60.0;
/// Searching messages starts once typing has paused this long.
const SEARCH_PAUSE: Duration = Duration::from_millis(500);

pub struct Workspace {
    pub backend: Arc<dyn Backend>,
    pub section: Section,
    pub chip: Chip,
    /// The calendar from yesterday to two weeks on; None until loaded.
    pub meetings: Option<Vec<Meeting>>,
    /// Meetings already reminded of (by id).
    reminded: HashSet<String>,
    meetings_load: Load<Vec<Meeting>, Error>,
    meetings_at: Option<Instant>,
    /// When the workspace opened, and when everything the start-up screen waits for
    /// was in (it fades out from then).
    pub started: Instant,
    pub ready_at: Option<Instant>,
    /// How the people in one-to-one chats show, by id.
    pub presence: HashMap<String, Presence>,
    presence_load: Load<Vec<(String, Presence)>, Error>,
    presence_at: Option<Instant>,
    /// The user's status (None until it's in).
    pub status: Option<Status>,
    status_load: Load<Status, Error>,
    /// The organisation's own emoji, once in (asked for once, after the list).
    pub custom_emoji: Vec<crate::model::CustomEmoji>,
    custom_emoji_load: Option<Load<Vec<crate::model::CustomEmoji>, Error>>,
    status_at: Option<Instant>,
    /// Setting the user's status, while it's under way.
    status_set: Load<(), Error>,
    /// Telling the presence service whether the user is active: the last report,
    /// what it said, and the report under way.
    reported_at: Option<Instant>,
    active: bool,
    activity_report: Load<(), Error>,
    /// The activity feed's items (mentions, replies), newest last; None until loaded.
    pub activity: Option<Vec<Message>>,
    activity_load: Load<Vec<Message>, Error>,
    activity_at: Option<Instant>,
    /// The post whose thread is open beside the conversation.
    pub thread: Option<String>,
    /// A message to show and mark (opened from Activity), until it has been scrolled to.
    pub focus: Option<String>,
    pub focus_scrolled: bool,
    /// The open conversation had unread messages: the last one read before (by id),
    /// for the "New messages" line, and whether the list has been scrolled to it.
    pub new_after: Option<i64>,
    pub new_scrolled: bool,
    /// Whether `new_after` has been taken from the list (it may come after the open).
    new_checked: bool,
    /// The reply being typed in the open thread.
    pub reply_draft: String,
    /// A message quoted in the reply being written (true: in the thread's composer).
    pub quoting: Option<(bool, Message)>,
    /// One of the user's messages being edited (true: in the thread's composer).
    pub editing: Option<(bool, String)>,
    /// What was in the composer before an edit took it, back when the edit ends.
    before_edit: Option<(bool, String)>,
    /// Pictures to send with the next message (true: in the thread's composer).
    pub attachments: Vec<(bool, Attachment)>,
    pub me: String,
    /// The signed-in user's id (for their picture).
    pub me_id: String,
    pub pictures: Pictures,
    /// What the sidebar lists; None until the first load.
    pub directory: Option<Directory>,
    /// The list, with the user's name and id (read off the UI thread: they wait on the sign-in).
    directory_load: Load<(Directory, String, String), Error>,
    directory_at: Option<Instant>,
    pub open: Option<ConversationId>,
    /// The open conversation's messages, oldest first; None while they load.
    pub messages: Option<Vec<Message>>,
    /// In a chat, up to which message each other member has read.
    pub read_by: Vec<(String, i64)>,
    read_load: Load<(ConversationId, Vec<(String, i64)>), Error>,
    read_at: Option<Instant>,
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
    /// Whether the saved folds are in.
    pub folded_loaded: bool,
    /// The message being typed in the open conversation.
    pub draft: String,
    /// Messages left half-written in other conversations, by conversation: back in
    /// the composer when the conversation is opened again.
    pub drafts: HashMap<String, String>,
    /// The saved drafts have been put in (once, at start).
    pub drafts_loaded: bool,
    sending: Load<(), Error>,
    /// Writes in the background (reactions, read marks); true asks to fetch the
    /// messages again after.
    chores: Vec<(Chore, Load<bool, Error>)>,
    /// The message being sent, and one that couldn't be (with why), in its conversation.
    outbox: Option<Outbox>,
    pub unsent: Option<(String, bool, String)>,
    pub pending_delete: Option<PendingDelete>,
    /// Teams couldn't be reached the last time it was asked.
    pub offline: bool,
    /// The conversation and message last marked read, so it's marked once.
    marked: Option<(String, String)>,
    /// Show a notification for each new message (the user's setting).
    pub notifications: bool,
    /// The newest message seen in each conversation (notifications are for newer ones).
    seen: HashMap<String, i64>,
    seen_once: bool,
    /// Conversations whose notification was clicked.
    clicked: (Sender<String>, Receiver<String>),
    /// How many of the open conversation's newest messages are shown.
    pub newest: usize,
    /// The list's height when earlier messages were asked for (to stay in place once they're in).
    pub earlier_from: Option<f32>,
    /// A scroll position for the message list's next drawing.
    pub scroll_to: Option<f32>,
    /// The message list's scroll position last frame, and when the user last scrolled
    /// it away from the newest (the day of what's on top shows meanwhile).
    pub scrolled_at: (f32, Option<std::time::Instant>),
    /// The composer's emoji picker (its search).
    pub emoji_picker: crate::ui::views::emoji_picker::Picker,
    /// While Activity is shown: up to which item it had been read when it was opened
    /// (newer ones stay marked new while it's looked at, though they're read by then).
    pub activity_new_after: Option<i64>,
    /// The quick switcher (⌘T), while it's open.
    pub switcher: Option<super::views::switcher::Switcher>,
    /// Conversations opened, the latest first.
    pub recent: Vec<ConversationId>,
    /// Conversations hidden in drift, with when (None until the saved ones are in).
    pub hidden: Option<HashMap<String, i64>>,
    /// Conversations muted in drift (None until the saved ones are in).
    pub muted_here: Option<HashSet<String>>,
    /// Channels followed in drift: every new post notifies (None until the saved ones are in).
    pub following: Option<HashSet<String>>,
    /// Emoji picked lately (keys, the latest first; None until the saved ones are in).
    pub recent_emoji: Option<Vec<String>>,
    /// The height of the text in the composer and in the thread's (as last drawn).
    pub composer_text: [f32; 2],
    /// The picture shown over the window, by its URL.
    pub viewing: Option<String>,
    /// The conversation last drawn (its first drawing is laid out once unseen, so it
    /// opens at its newest messages without a flash of its oldest).
    pub drawn: Option<ConversationId>,
    /// The message list's height last drawn, and whose (a change redraws the frame).
    pub list_height: Option<(ConversationId, f32)>,
    /// Changes the service pushes.
    pushes: Receiver<Push>,
    /// Whether the service is pushing changes (then polling slows down).
    live: bool,
    /// The unread count last put on the Dock icon.
    badge: Option<usize>,
    /// New messages shown in the corner while drift is in front.
    pub toasts: super::toasts::Toasts,
}

/// Work done behind the UI, and what its failing means for the user.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Chore {
    Delete,
    React,
    Pin,
    MarkRead,
}

/// A message on its way: all of it, to put back in the composer if it fails.
#[derive(Clone)]
struct Outbox {
    conversation: String,
    reply: bool,
    text: String,
    attachments: Vec<Attachment>,
    quoting: Option<Message>,
    editing: Option<String>,
}

/// A message deleted on screen, sent to the service once its Undo has run out.
pub struct PendingDelete {
    conversation: String,
    message: Message,
    at: Instant,
}

impl PendingDelete {
    pub fn is_in(&self, conversation: &str) -> bool {
        self.conversation == conversation
    }
}

/// How long a deleted message can be brought back.
const UNDO_FOR: Duration = Duration::from_secs(5);

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
    People,
    Meetings,
}

/// What the workspace needs the app to do.
pub enum Outcome {
    /// The service no longer takes the sign-in.
    SignedOut,
}

impl Workspace {
    pub fn new(ctx: &egui::Context, backend: Arc<dyn Backend>, open: Option<ConversationId>) -> Workspace {
        let (push, pushes) = std::sync::mpsc::channel();
        let repaint = ctx.clone();
        backend.clone().listen(Box::new(move |change| {
            if push.send(change).is_ok() {
                repaint.request_repaint();
            }
        }));
        Workspace {
            me: backend.me(),
            me_id: backend.me_id(),
            pictures: Pictures::new(ctx, backend.clone()),
            backend,
            section: Section::Chats,
            chip: Chip::All,
            meetings: None,
            reminded: HashSet::new(),
            meetings_load: Load::Idle,
            meetings_at: None,
            presence: HashMap::new(),
            presence_load: Load::Idle,
            presence_at: None,
            started: Instant::now(),
            ready_at: None,
            status: None,
            status_load: Load::Idle,
            custom_emoji: Vec::new(),
            custom_emoji_load: None,
            recent_emoji: None,
            status_at: None,
            status_set: Load::Idle,
            reported_at: None,
            active: false,
            activity_report: Load::Idle,
            activity: None,
            activity_load: Load::Idle,
            activity_at: None,
            thread: None,
            focus: None,
            focus_scrolled: false,
            new_after: None,
            new_scrolled: false,
            new_checked: false,
            reply_draft: String::new(),
            quoting: None,
            editing: None,
            before_edit: None,
            attachments: Vec::new(),
            directory: None,
            directory_load: Load::Idle,
            directory_at: None,
            open,
            messages: None,
            read_by: Vec::new(),
            read_load: Load::Idle,
            read_at: None,
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
            folded: HashSet::from(["meetings".to_string(), "hidden".to_string()]),
            folded_loaded: false,
            draft: String::new(),
            drafts: HashMap::new(),
            drafts_loaded: false,
            sending: Load::Idle,
            chores: Vec::new(),
            outbox: None,
            unsent: None,
            pending_delete: None,
            offline: false,
            marked: None,
            notifications: true,
            seen: HashMap::new(),
            seen_once: false,
            clicked: std::sync::mpsc::channel(),
            newest: NEWEST,
            earlier_from: None,
            scrolled_at: (0.0, None),
            emoji_picker: Default::default(),
            scroll_to: None,
            activity_new_after: None,
            switcher: None,
            recent: Vec::new(),
            muted_here: None,
            following: None,
            hidden: None,
            composer_text: [20.0; 2],
            viewing: None,
            drawn: None,
            list_height: None,
            pushes,
            live: false,
            badge: None,
            toasts: Default::default(),
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
        let mut message =
            if text.is_empty() { markdown::Outgoing::default() } else { markdown::to_html_with(&text, &self.people(), &self.custom_emoji) };
        let quoting = self.quoting.take_if(|(r, _)| *r == reply).map(|(_, m)| m);
        if let Some(quoted) = &quoting {
            message.html = quote_html(quoted) + &message.html;
        }
        let editing = self.editing.take_if(|(r, _)| *r == reply).map(|(_, id)| id);
        if editing.is_some() {
            self.end_edit(reply);
        } else {
            self.draft_mut(reply).clear();
        }
        // Kept until it's sent: a failure puts it all back.
        self.unsent = None;
        self.outbox = Some(Outbox {
            conversation: open.clone(),
            reply,
            text: text.clone(),
            attachments: pictures.clone(),
            quoting,
            editing: editing.clone(),
        });
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
        // What's being written waits for the edit to end (kept from before the first one).
        if !self.editing.as_ref().is_some_and(|(r, _)| *r == reply) {
            self.before_edit = Some((reply, std::mem::take(self.draft_mut(reply))));
        }
        self.editing = Some((reply, message.id.clone()));
        *self.draft_mut(reply) = text::to_markdown(&message.body);
    }

    /// The composer gets back what was in it before the edit.
    fn end_edit(&mut self, reply: bool) {
        let before = self.before_edit.take_if(|(r, _)| *r == reply).map(|(_, text)| text).unwrap_or_default();
        *self.draft_mut(reply) = before;
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
            self.end_edit(reply);
        }
        if self.quoting.as_ref().is_some_and(|(r, _)| *r == reply) {
            self.quoting = None;
        }
    }

    /// Delete one of the user's messages: gone from the screen at once, from Teams once
    /// its Undo has run out (a delete still waiting goes right away).
    pub fn delete(&mut self, ctx: &egui::Context, message: &str) {
        let Some(open) = self.open.clone() else { return };
        if let Some(pending) = self.pending_delete.take() {
            self.send_delete(ctx, pending);
        }
        let Some(messages) = self.messages.as_mut() else { return };
        let Some(at) = messages.iter().position(|m| m.id == message) else { return };
        let message = messages.remove(at);
        self.pending_delete = Some(PendingDelete { conversation: open, message, at: Instant::now() });
    }

    /// Bring back the message deleted last, while its Undo lasts.
    pub fn undo_delete(&mut self) {
        let Some(pending) = self.pending_delete.take() else { return };
        if self.open.as_deref() == Some(pending.conversation.as_str())
            && let Some(messages) = self.messages.as_mut()
        {
            let key = |m: &Message| (m.at, m.id.parse::<i64>().unwrap_or(0));
            let at = messages.partition_point(|m| key(m) < key(&pending.message));
            messages.insert(at, pending.message);
        }
    }

    /// Delete the message waiting for its Undo now.
    pub fn commit_delete(&mut self, ctx: &egui::Context) {
        if let Some(pending) = self.pending_delete.take() {
            self.send_delete(ctx, pending);
        }
    }

    fn send_delete(&mut self, ctx: &egui::Context, pending: PendingDelete) {
        let (backend, conversation, id) = (self.backend.clone(), pending.conversation, pending.message.id);
        self.chores.push((Chore::Delete, Load::start(ctx, move || backend.delete(&conversation, &id).map(|()| true))));
    }

    /// Put a message that couldn't be sent back where it was written (or, if another
    /// conversation is open, among the drafts), and say so.
    fn put_back(&mut self, outbox: Outbox, why: String) {
        let here = self.open.as_deref() == Some(outbox.conversation.as_str());
        let reply = outbox.reply;
        // An edit that failed takes the composer again; what's there waits as before.
        if here && outbox.editing.is_some() && self.editing.is_none() {
            self.before_edit = Some((reply, std::mem::take(self.draft_mut(reply))));
        }
        if here && self.draft(reply).trim().is_empty() {
            *self.draft_mut(reply) = outbox.text;
            self.attachments.extend(outbox.attachments.into_iter().map(|a| (reply, a)));
            if let Some(m) = outbox.quoting {
                self.quoting = Some((reply, m));
            }
            if let Some(id) = outbox.editing {
                self.editing = Some((reply, id));
            }
        } else if !outbox.text.trim().is_empty() {
            let draft = self.drafts.entry(outbox.conversation.clone()).or_default();
            if !draft.is_empty() {
                draft.push_str("\n\n");
            }
            draft.push_str(&outbox.text);
        }
        self.unsent = Some((outbox.conversation, reply, why));
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
        self.chores.push((Chore::React, Load::start(ctx, move || backend.react(&open, &message, &key, add).map(|()| true))));
    }

    /// Notifications for messages newer than the last ones seen: in chats and meeting
    /// chats (not the user's own, not muted ones, not the one on screen while drift is
    /// in front), in channels followed in drift every post, and in other channels only
    /// what Activity shows (mentions, replies). The first list only sets what's been seen.
    fn notify_new(&mut self, ctx: &egui::Context, directory: &Directory) {
        let focused = ctx.input(|i| i.viewport().focused.unwrap_or(true));
        let mut new: Vec<&Conversation> = Vec::new();
        for c in &directory.conversations {
            let Some(last) = &c.last else { continue };
            let before = self.seen.insert(c.id.clone(), last.at);
            let newer = before.is_none_or(|at| last.at > at);
            let on_screen = focused && self.open.as_deref() == Some(c.id.as_str());
            let fresh = self.seen_once && newer && !c.muted && !on_screen;
            let followed = c.kind == Kind::Channel && self.is_following(&c.id);
            match c.kind {
                Kind::Chat | Kind::Meeting if fresh && last.author != self.me && !last.author.is_empty() => new.push(c),
                Kind::Channel if followed && fresh && last.author != self.me && !last.author.is_empty() => new.push(c),
                Kind::Feed if fresh && self.notifications => {
                    let (tx, id, ctx) = (self.clicked.0.clone(), c.id.clone(), ctx.clone());
                    let body: String = last.text.chars().take(160).collect();
                    crate::notify::show("New in Activity", None, &body, move || {
                        let _ = tx.send(id);
                        ctx.request_repaint();
                    });
                }
                _ => {}
            }
        }
        self.seen_once = true;
        if !self.notifications || new.is_empty() {
            return;
        }
        // A handful each on its own; more than that as one.
        if new.len() > 3 && !focused {
            let tx = self.clicked.0.clone();
            let first = new[0].id.clone();
            let ctx = ctx.clone();
            crate::notify::show(
                &format!("{} new messages", new.len()),
                None,
                &new.iter().map(|c| c.title.as_str()).collect::<Vec<_>>().join(", "),
                move || {
                    let _ = tx.send(first);
                    ctx.request_repaint();
                },
            );
            return;
        }
        if focused {
            crate::notify::chime();
        }
        for c in new {
            let Some(last) = &c.last else { continue };
            let one_to_one = c.kind == Kind::Chat && c.members.len() <= 1;
            let place = (!one_to_one).then_some(c.title.as_str());
            let body: String = last.text.chars().take(160).collect();
            // In front: a toast in the corner; otherwise the system's notification.
            if focused {
                self.toasts.push(&last.author, place, &body, &c.id);
                continue;
            }
            let (tx, id, ctx) = (self.clicked.0.clone(), c.id.clone(), ctx.clone());
            crate::notify::show(&last.author, place, &body, move || {
                let _ = tx.send(id);
                ctx.request_repaint();
            });
        }
    }

    /// The conversation `by` places from the open one among chats, meeting chats and
    /// channels, newest first (with `unread`, only those with something unread);
    /// the first one when none is open.
    pub fn neighbour(&self, by: i32, unread: bool) -> Option<String> {
        let d = self.directory.as_ref()?;
        let mut all: Vec<&Conversation> = d
            .conversations
            .iter()
            .filter(|c| c.kind != Kind::Feed && c.last.is_some() && !self.is_hidden(c))
            .filter(|c| !unread || (c.unread && !c.muted) || self.open.as_deref() == Some(c.id.as_str()))
            .collect();
        all.sort_by_key(|c| std::cmp::Reverse(c.last.as_ref().map_or(0, |l| l.at)));
        let at = self.open.as_deref().and_then(|open| all.iter().position(|c| c.id == open));
        let to = match at {
            Some(at) => usize::try_from(at as i32 + by).ok()?,
            None => 0,
        };
        all.get(to).filter(|c| Some(c.id.as_str()) != self.open.as_deref()).map(|c| c.id.clone())
    }

    /// A notification a minute before each meeting the user hasn't declined, once;
    /// clicking it joins (or opens the meeting's chat). One that started a little
    /// while ago (drift was just opened) is reminded of too.
    fn remind_meetings(&mut self, ctx: &egui::Context) {
        const BEFORE: i64 = 60_000;
        const STILL_AFTER: i64 = 5 * 60_000;
        let Some(meetings) = &self.meetings else { return };
        let now = crate::model::time::now_ms();
        let mut next: Option<i64> = None;
        for m in meetings.iter().filter(|m| !m.cancelled && m.response != "Decline") {
            if now < m.start - BEFORE {
                next = Some(next.map_or(m.start - BEFORE, |n| n.min(m.start - BEFORE)));
                continue;
            }
            if now > m.start + STILL_AFTER || !self.reminded.insert(m.id.clone()) || !self.notifications {
                continue;
            }
            let minutes = (m.start - now) / 60_000;
            let when = match minutes {
                1.. => format!("Starts in {minutes} min"),
                0 if m.start >= now => "Starts in less than a minute".to_string(),
                _ if now - m.start < 60_000 => "Starting now".to_string(),
                _ => format!("Started {} min ago", (now - m.start) / 60_000),
            };
            let body = if m.join_url.is_some() { format!("{when}. Click to join.") } else { when };
            let (join, chat, tx, ctx2) = (m.join_url.clone(), m.chat.clone(), self.clicked.0.clone(), ctx.clone());
            crate::notify::show(&m.subject, Some("Meeting"), &body, move || {
                match (join, chat) {
                    (Some(url), _) => ctx2.open_url(egui::OpenUrl::new_tab(url)),
                    (None, Some(chat)) => {
                        let _ = tx.send(chat);
                    }
                    (None, None) => {}
                }
                ctx2.request_repaint();
            });
        }
        if let Some(at) = next {
            ctx.request_repaint_after(Duration::from_millis((at - now).clamp(0, 3_600_000) as u64));
        }
    }

    /// The online meeting going on now (or about to start), else the next one today.
    pub fn next_meeting(&self) -> Option<&Meeting> {
        let now = crate::model::time::now_ms();
        let today = crate::model::time::day_number(now);
        let online = || self.meetings.iter().flatten().filter(|m| !m.cancelled && m.join_url.is_some());
        let live = online().filter(|m| m.start - 5 * 60_000 <= now && now < m.end).min_by_key(|m| m.start);
        live.or_else(|| online().filter(|m| m.start > now && crate::model::time::day_number(m.start) == today).min_by_key(|m| m.start))
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
        self.chores.push((Chore::Pin, Load::start(ctx, move || backend.pin(&id, pinned).map(|()| false))));
    }

    /// Mark a conversation read (up to now), or unread (from its newest message on).
    pub fn set_read(&mut self, ctx: &egui::Context, id: &str, read: bool) {
        let Some(c) = self.directory.as_mut().and_then(|d| d.conversations.iter_mut().find(|c| c.id == id)) else { return };
        c.unread = !read;
        let up_to = if read { crate::model::time::now_ms() } else { c.last.as_ref().map_or(0, |l| l.at) - 1 };
        c.read_up_to = Some(up_to);
        // The open one isn't marked read again until something new comes.
        if !read && self.open.as_deref() == Some(id) {
            let last = self.messages.as_ref().and_then(|m| m.last()).map(|m| m.id.clone()).unwrap_or_default();
            self.marked = Some((id.to_string(), last));
        }
        let (backend, id) = (self.backend.clone(), id.to_string());
        self.chores.push((Chore::MarkRead, Load::start(ctx, move || backend.mark_read(&id, &up_to.to_string()).map(|()| false))));
    }

    /// Set the user's status (`None`: by activity again). It shows set right away.
    pub fn set_status(&mut self, ctx: &egui::Context, to: Option<Availability>) {
        if let Some(status) = self.status.as_mut() {
            *status = Status { shown: to.unwrap_or(status.shown), set: to };
        }
        let backend = self.backend.clone();
        self.status_load = Load::Idle;
        self.status_set = Load::start(ctx, move || backend.set_status(to));
    }

    /// How the person a one-to-one chat is with shows (the user, for their notes).
    pub fn presence_of(&self, c: &Conversation) -> Option<Presence> {
        if let Some(Picture::Person { id, .. }) = &c.picture
            && *id == self.me_id
            && c.members.is_empty()
        {
            return self.status.map(|s| Presence { availability: s.shown, out_of_office: false });
        }
        person(c).and_then(|id| self.presence.get(id)).copied()
    }

    /// An emoji was picked: it goes first among the recent ones.
    pub fn used_emoji(&mut self, key: &str) {
        let recent = self.recent_emoji.get_or_insert_with(Vec::new);
        recent.retain(|k| k != key);
        recent.insert(0, key.to_string());
        recent.truncate(crate::ui::views::emoji_picker::RECENT);
    }

    /// Mark the open conversation unread from `message` on.
    pub fn unread_from(&mut self, ctx: &egui::Context, message: &str) {
        let (Some(id), Ok(at)) = (self.open.clone(), message.parse::<i64>()) else { return };
        if let Some(c) = self.directory.as_mut().and_then(|d| d.conversations.iter_mut().find(|c| c.id == id)) {
            c.unread = true;
            c.read_up_to = Some(at - 1);
        }
        // Not marked read again until something new comes.
        let last = self.messages.as_ref().and_then(|m| m.last()).map(|m| m.id.clone()).unwrap_or_default();
        self.marked = Some((id.clone(), last));
        let backend = self.backend.clone();
        let up_to = (at - 1).to_string();
        self.chores.push((Chore::MarkRead, Load::start(ctx, move || backend.mark_read(&id, &up_to).map(|()| false))));
    }

    /// Whether a conversation is hidden: hidden in drift, and nothing newer since.
    pub fn is_hidden(&self, c: &Conversation) -> bool {
        self.hidden.as_ref().and_then(|h| h.get(&c.id)).is_some_and(|at| c.last.as_ref().is_none_or(|l| l.at <= *at))
    }

    /// Hide a conversation from the list (until something new comes), or show it again.
    pub fn set_hidden(&mut self, id: &str, hidden: bool) {
        let set = self.hidden.get_or_insert_default();
        if hidden {
            set.insert(id.to_string(), crate::model::time::now_ms());
            if self.open.as_deref() == Some(id) {
                self.keep_draft();
                self.open = None;
                self.messages = None;
            }
        } else {
            set.remove(id);
        }
    }

    /// Mute a conversation in drift (no notifications, not counted unread), or unmute it.
    pub fn set_muted(&mut self, id: &str, muted: bool) {
        let set = self.muted_here.get_or_insert_default();
        if muted {
            set.insert(id.to_string());
        } else {
            set.remove(id);
        }
        if let Some(c) = self.directory.as_mut().and_then(|d| d.conversations.iter_mut().find(|c| c.id == id)) {
            c.muted = muted;
        }
    }

    /// Follow a channel in drift (a notification for every new post), or stop.
    pub fn set_following(&mut self, id: &str, on: bool) {
        let set = self.following.get_or_insert_default();
        if on {
            set.insert(id.to_string());
        } else {
            set.remove(id);
        }
    }

    pub fn is_following(&self, id: &str) -> bool {
        self.following.as_ref().is_some_and(|f| f.contains(id))
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
        self.chores.push((Chore::MarkRead, Load::start(ctx, move || backend.mark_read(&open, &last).map(|()| false))));
    }

    /// Collect finished loads and start the ones that are due.
    pub fn tick(&mut self, ctx: &egui::Context) -> Option<Outcome> {
        self.pictures.begin_frame();
        if let Some(result) = self.sending.take() {
            let outbox = self.outbox.take();
            match result {
                Ok(()) => {
                    self.error = None;
                    self.messages_at = None; // show the sent message right away
                    self.read_at = None;
                }
                Err(Error::SignedOut) => return Some(Outcome::SignedOut),
                Err(e) => {
                    log_warn!("sending: {e}");
                    if let Some(outbox) = outbox {
                        self.put_back(outbox, e.to_string());
                    }
                }
            }
        }
        // A deleted message whose Undo ran out goes to the service.
        if let Some(pending) = self.pending_delete.take_if(|p| p.at.elapsed() >= UNDO_FOR) {
            self.send_delete(ctx, pending);
        } else if self.pending_delete.is_some() {
            ctx.request_repaint_after(Duration::from_millis(250));
        }
        // The service pushed changes: fetch what they touch now.
        while let Ok(push) = self.pushes.try_recv() {
            match push {
                Push::Live(live) => self.live = live,
                Push::Everything => {
                    self.directory_at = None;
                    self.messages_at = None;
                    self.activity_at = None;
                }
                Push::Conversation(id) => {
                    self.directory_at = None;
                    if self.open.as_deref() == Some(id.as_str()) {
                        self.messages_at = None;
                        self.read_at = None;
                    }
                    if self.directory.as_ref().and_then(|d| d.activity()).is_some_and(|c| c.id == id) {
                        self.activity_at = None;
                    }
                }
            }
        }
        // A notification was clicked: open its conversation (or Activity), in front.
        while let Ok(id) = self.clicked.1.try_recv() {
            let feed = self.directory.as_ref().and_then(|d| d.conversation(&id)).is_some_and(|c| c.kind == Kind::Feed);
            self.section = if feed { Section::Activity } else { Section::Chats };
            if !feed {
                self.open(&id);
            }
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        }
        let unread = self
            .directory
            .as_ref()
            .map_or(0, |d| d.conversations.iter().filter(|c| c.unread && !c.muted && matches!(c.kind, Kind::Chat | Kind::Meeting)).count());
        if self.badge != Some(unread) {
            self.badge = Some(unread);
            crate::notify::badge(unread);
        }
        let mut refetch = false;
        let mut failed = Vec::new();
        for (chore, load) in &mut self.chores {
            match load.take() {
                Some(Ok(again)) => refetch |= again,
                Some(Err(Error::SignedOut)) => return Some(Outcome::SignedOut),
                Some(Err(e)) => failed.push((*chore, e)),
                None => {}
            }
        }
        self.chores.retain(|(_, load)| load.is_running());
        for (chore, e) in failed {
            log_warn!("{chore:?}: {e}");
            // What's on screen goes back to what the service has.
            refetch = true;
            match chore {
                // Asked again with the next messages.
                Chore::MarkRead => self.marked = None,
                Chore::Delete => self.error = Some(format!("Couldn't delete the message. {e}")),
                Chore::React => self.error = Some(format!("Couldn't add the reaction. {e}")),
                Chore::Pin => {
                    self.error = Some(format!("Couldn't pin or unpin it. {e}"));
                    self.directory_at = None;
                }
            }
        }
        if refetch {
            self.messages_at = None;
        }
        if let Some(result) = self.directory_load.take() {
            match result {
                Ok((mut directory, me, me_id)) => {
                    // Muted in drift counts as muted.
                    for c in &mut directory.conversations {
                        c.muted |= self.muted_here.as_ref().is_some_and(|m| m.contains(&c.id));
                    }
                    if self.me.is_empty() {
                        (self.me, self.me_id) = (me, me_id);
                    }
                    self.notify_new(ctx, &directory);
                    // Something newer in the open conversation than what's on screen: fetch it now.
                    let newest = self.messages.as_ref().and_then(|m| m.last()).map(|m| m.at);
                    if let Some(open) = self.open.as_deref()
                        && let Some(last) = directory.conversation(open).and_then(|c| c.last.as_ref())
                        && newest.is_some_and(|at| last.at > at)
                    {
                        self.messages_at = None;
                    }
                    self.directory = Some(directory);
                    self.check_new();
                    self.error = None;
                    self.offline = false;
                }
                Err(Error::SignedOut) => return Some(Outcome::SignedOut),
                Err(e) => self.failed(e),
            }
        }
        // Search and the calendar have tokens of their own, which a tenant may refuse:
        // that's not the sign-in failing (the chat service would say so).
        if let Some(result) = self.hits_load.take() {
            match result {
                Ok(hits) => self.hits = Some(hits),
                Err(Error::SignedOut) => self.error = Some("Search isn't available for this account".into()),
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
        if let Some(result) = self.meetings_load.take() {
            match result {
                Ok(meetings) => self.meetings = Some(meetings),
                Err(e) => log_warn!("calendar: {e}"),
            }
        }
        self.remind_meetings(ctx);
        if let Some(result) = self.activity_load.take() {
            match result {
                Ok(items) => self.activity = Some(items),
                Err(Error::SignedOut) => return Some(Outcome::SignedOut),
                Err(e) => self.failed(e),
            }
        }
        self.read_activity(ctx);
        if let Some(result) = self.status_set.take() {
            match result {
                Ok(()) => {}
                Err(Error::SignedOut) => return Some(Outcome::SignedOut),
                Err(e) => {
                    log_warn!("setting the status: {e}");
                    self.error = Some(format!("Couldn't set your status. {e}"));
                }
            }
            // What the service makes of it (by activity: what it works out).
            self.status_at = None;
        }
        match self.activity_report.take() {
            // Shown as offline until the service counts this endpoint (at start-up,
            // the status is often fetched before the first report is in).
            Some(Ok(())) if self.status.is_some_and(|s| s.shown == Availability::Offline) => self.status_at = None,
            Some(Err(e)) => log_debug!("reporting activity: {e}"),
            _ => {}
        }
        if let Some(result) = self.presence_load.take() {
            match result {
                Ok(people) => self.presence.extend(people),
                Err(e) => log_debug!("presence: {e}"),
            }
        }
        if let Some(result) = self.custom_emoji_load.as_mut().and_then(Load::take) {
            match result {
                Ok(found) => self.custom_emoji = found,
                Err(e) => log_debug!("custom emoji: {e}"),
            }
        }
        if let Some(result) = self.status_load.take() {
            match result {
                Ok(status) => self.status = Some(status),
                Err(e) => log_debug!("status: {e}"),
            }
        }
        if let Some(result) = self.read_load.take() {
            match result {
                Ok((id, positions)) if Some(&id) == self.open.as_ref() => {
                    let me = self.me_id.clone();
                    self.read_by = positions.into_iter().filter(|(who, _)| *who != me).collect();
                }
                Ok(_) => {}
                Err(e) => log_debug!("read positions: {e}"),
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
                    let mut messages = messages;
                    // One deleted on screen stays deleted while its Undo lasts.
                    if let Some(p) = self.pending_delete.as_ref().filter(|p| p.conversation == id) {
                        messages.retain(|m| m.id != p.message.id);
                    }
                    self.messages = Some(messages);
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
        let focused = ctx.input(|i| i.viewport().focused.unwrap_or(true));
        let (directory_every, messages_every) = match (self.live, focused) {
            (true, _) => (PUSHED_EVERY, PUSHED_EVERY),
            (false, true) => (DIRECTORY_EVERY, MESSAGES_FRONT_EVERY),
            (false, false) => (DIRECTORY_EVERY, MESSAGES_EVERY),
        };
        if !self.directory_load.is_running() && due(self.directory_at, directory_every) {
            self.directory_at = Some(Instant::now());
            let backend = self.backend.clone();
            self.directory_load = Load::start(ctx, move || backend.directory().map(|d| (d, backend.me(), backend.me_id())));
        }
        if let Some(id) = self.open.clone()
            && !self.messages_load.is_running()
            && due(self.messages_at, messages_every)
        {
            self.messages_at = Some(Instant::now());
            let backend = self.backend.clone();
            self.messages_load = Load::start(ctx, move || backend.messages(&id).map(|m| (id, m)));
        }
        if !self.meetings_load.is_running() && due(self.meetings_at, CALENDAR_EVERY) {
            self.meetings_at = Some(Instant::now());
            let backend = self.backend.clone();
            let now = crate::model::time::now_ms();
            self.meetings_load = Load::start(ctx, move || backend.calendar(now - 86_400_000, now + 15 * 86_400_000));
        }
        // Reported on a schedule, and right away when the user comes back or goes idle.
        let active = crate::idle::seconds().is_none_or(|idle| idle < IDLE_AFTER);
        if !self.activity_report.is_running() && (active != self.active || due(self.reported_at, ACTIVITY_EVERY)) {
            let came_back = active && !self.active;
            self.reported_at = Some(Instant::now());
            self.active = active;
            let backend = self.backend.clone();
            self.activity_report = Load::start(ctx, move || backend.report_activity(active));
            if came_back {
                self.status_at = None;
            }
        }
        if !self.presence_load.is_running()
            && due(self.presence_at, PRESENCE_EVERY)
            && let Some(directory) = self.directory.as_ref()
        {
            self.presence_at = Some(Instant::now());
            let people: Vec<String> = directory.conversations.iter().filter_map(|c| person(c).map(str::to_string)).collect();
            let backend = self.backend.clone();
            self.presence_load = Load::start(ctx, move || backend.presence(&people));
        }
        if self.custom_emoji_load.is_none() && self.directory.is_some() {
            let backend = self.backend.clone();
            self.custom_emoji_load = Some(Load::start(ctx, move || backend.custom_emoji()));
        }
        if !self.status_load.is_running() && !self.status_set.is_running() && due(self.status_at, STATUS_EVERY) {
            self.status_at = Some(Instant::now());
            let backend = self.backend.clone();
            self.status_load = Load::start(ctx, move || backend.status());
        }
        // Who has read how far, in a chat (for "Seen").
        if let Some(id) = self.open.clone()
            && self.directory.as_ref().and_then(|d| d.conversation(&id)).is_some_and(|c| matches!(c.kind, Kind::Chat | Kind::Meeting))
            && !self.read_load.is_running()
            && due(self.read_at, MESSAGES_EVERY)
        {
            self.read_at = Some(Instant::now());
            let backend = self.backend.clone();
            self.read_load = Load::start(ctx, move || backend.read_by(&id).map(|r| (id, r)));
        }
        // The feed is fetched whatever's shown: its unread count is on the Activity tab.
        if !self.activity_load.is_running()
            && due(self.activity_at, if self.live { PUSHED_EVERY } else { MESSAGES_EVERY })
            && let Some(feed) = self.directory.as_ref().and_then(|d| d.activity()).map(|c| c.id.clone())
        {
            self.activity_at = Some(Instant::now());
            let backend = self.backend.clone();
            self.activity_load = Load::start(ctx, move || backend.messages(&feed));
        }
        // Wake up for the next refresh even when nothing moves.
        ctx.request_repaint_after(MESSAGES_FRONT_EVERY);
        None
    }

    /// Something couldn't be fetched: Teams unreachable shows as being offline (and
    /// clears when it answers again), anything else as an error.
    fn failed(&mut self, e: Error) {
        log_warn!("{e}");
        match e {
            Error::Network(_) => self.offline = true,
            e => self.error = Some(e.to_string()),
        }
    }

    /// Every half-written message, the open conversation's included (not an edit).
    pub fn all_drafts(&self) -> HashMap<String, String> {
        let mut all = self.drafts.clone();
        // While a message is edited, what was being written before it.
        let draft = match (&self.editing, &self.before_edit) {
            (Some((false, _)), Some((false, before))) => before,
            (Some((false, _)), _) => "",
            _ => &self.draft,
        };
        if let Some(open) = &self.open
            && !draft.trim().is_empty()
        {
            all.insert(open.clone(), draft.to_string());
        }
        all
    }

    /// Keep what's half-written in the open conversation for when it's opened again
    /// (not an edit of a message: that's dropped).
    fn keep_draft(&mut self) {
        let Some(open) = self.open.clone() else { return };
        // An edit is dropped; what was being written before it is kept.
        if self.editing.as_ref().is_some_and(|(reply, _)| !reply) {
            self.editing = None;
            self.end_edit(false);
        }
        let draft = std::mem::take(&mut self.draft);
        if draft.trim().is_empty() {
            self.drafts.remove(&open);
        } else {
            self.drafts.insert(open, draft);
        }
    }

    /// Open a conversation; its messages load right away.
    pub fn open(&mut self, id: &str) {
        if self.open.as_deref() == Some(id) {
            return;
        }
        self.keep_draft();
        self.open = Some(id.to_string());
        self.recent.retain(|r| r != id);
        self.recent.insert(0, id.to_string());
        self.recent.truncate(30);
        self.newest = NEWEST;
        self.messages = None;
        self.read_by.clear();
        self.read_at = None;
        self.messages_load = Load::Idle;
        self.messages_at = None;
        self.thread = None;
        self.focus = None;
        self.find = None;
        self.side = Side::List;
        self.draft = self.drafts.remove(id).unwrap_or_default();
        self.reply_draft.clear();
        self.quoting = None;
        self.editing = None;
        self.attachments.clear();
        self.new_after = None;
        self.new_scrolled = false;
        self.new_checked = false;
        self.check_new();
    }

    /// Where the open conversation's unread messages start, from the list (once per
    /// opening); it's read from then on.
    fn check_new(&mut self) {
        let Some(id) = self.open.clone() else { return };
        let Some(c) = self.directory.as_mut().and_then(|d| d.conversations.iter_mut().find(|c| c.id == id)) else { return };
        if !self.new_checked {
            self.new_checked = true;
            self.new_after = c.read_up_to.filter(|_| c.unread);
            // Read here; Teams itself still counts it until it's read there.
            c.unread = false;
        }
    }

    /// How many activity items came after what was last read (before the feed is
    /// loaded, 1 if anything did).
    pub fn activity_unread(&self) -> usize {
        let Some(feed) = self.directory.as_ref().and_then(|d| d.activity()) else { return 0 };
        match (&self.activity, feed.read_up_to) {
            (Some(items), Some(read)) => items.iter().filter(|m| m.id.parse::<i64>().is_ok_and(|id| id > read)).count(),
            _ => usize::from(feed.unread),
        }
    }

    /// Whether an activity item is new: after what was read when Activity was opened.
    pub fn activity_is_new(&self, item: &Message) -> bool {
        let read = self.activity_new_after.or_else(|| self.directory.as_ref().and_then(|d| d.activity()).and_then(|f| f.read_up_to));
        read.is_some_and(|read| item.id.parse::<i64>().is_ok_and(|id| id > read))
    }

    /// With Activity shown, mark the feed read up to its newest item (as Teams does
    /// when its bell is opened); the items that were new stay marked while it's shown.
    fn read_activity(&mut self, ctx: &egui::Context) {
        if self.section != Section::Activity {
            self.activity_new_after = None;
            return;
        }
        if self.activity_unread() == 0 {
            return;
        }
        let newest = self.activity.as_ref().and_then(|items| items.iter().filter_map(|m| m.id.parse::<i64>().ok()).max());
        let Some(newest) = newest else { return };
        let Some(directory) = self.directory.as_mut() else { return };
        let Some(id) = directory.activity().map(|f| f.id.clone()) else { return };
        let Some(feed) = directory.conversations.iter_mut().find(|c| c.id == id) else { return };
        if self.activity_new_after.is_none() {
            self.activity_new_after = Some(feed.read_up_to.unwrap_or(newest));
        }
        // Read here at once; the service hears of it in the background.
        feed.read_up_to = Some(newest);
        feed.unread = false;
        let backend = self.backend.clone();
        let message = newest.to_string();
        self.chores.push((Chore::MarkRead, Load::start(ctx, move || backend.mark_read(&id, &message).map(|()| false))));
    }

    /// Fetch everything again now.
    pub fn refresh(&mut self) {
        self.directory_at = None;
        self.meetings_at = None;
        self.messages_at = None;
        self.activity_at = None;
        self.presence_at = None;
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

/// Who a one-to-one chat is with.
fn person(c: &Conversation) -> Option<&str> {
    match c.members.as_slice() {
        [one] if c.kind == Kind::Chat => Some(one.id.as_str()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn workspace(open: &str) -> Workspace {
        let ctx = egui::Context::default();
        let mut ws = Workspace::new(&ctx, Arc::new(crate::demo::Demo), Some(open.to_string()));
        ws.messages = Some(crate::demo::Demo.messages(open).unwrap());
        ws
    }

    fn outbox(conversation: &str, text: &str) -> Outbox {
        Outbox { conversation: conversation.into(), reply: false, text: text.into(), attachments: Vec::new(), quoting: None, editing: None }
    }

    #[test]
    fn a_message_that_failed_goes_back_where_it_was_written() {
        let mut ws = workspace("19:maya");
        ws.put_back(outbox("19:maya", "see you at 1"), "HTTP 503".into());
        assert_eq!(ws.draft, "see you at 1");
        assert!(ws.unsent.as_ref().is_some_and(|(c, reply, why)| c == "19:maya" && !reply && why == "HTTP 503"));
    }

    #[test]
    fn a_failed_message_from_elsewhere_becomes_that_conversations_draft() {
        let mut ws = workspace("19:maya");
        ws.draft = "typing here".into();
        ws.put_back(outbox("19:tomas", "rebased"), "offline".into());
        assert_eq!(ws.draft, "typing here");
        assert_eq!(ws.drafts.get("19:tomas").map(String::as_str), Some("rebased"));
    }

    #[test]
    fn editing_a_message_keeps_what_was_being_written() {
        let mut ws = workspace("19:maya");
        ws.draft = "half a thought".into();
        let mine = ws.messages.iter().flatten().find(|m| m.author.id == crate::demo::ME).unwrap().clone();
        ws.edit(&mine, false);
        assert_ne!(ws.draft, "half a thought");
        assert_eq!(ws.all_drafts().get("19:maya").map(String::as_str), Some("half a thought"));
        ws.cancel_compose(false);
        assert_eq!(ws.draft, "half a thought");
        assert!(ws.editing.is_none());
    }

    #[test]
    fn a_deleted_message_comes_back_on_undo() {
        let mut ws = workspace("19:maya");
        let ctx = egui::Context::default();
        let before: Vec<String> = ws.messages.iter().flatten().map(|m| m.id.clone()).collect();
        ws.delete(&ctx, "3004");
        assert!(!ws.messages.iter().flatten().any(|m| m.id == "3004"));
        ws.undo_delete();
        assert_eq!(ws.messages.iter().flatten().map(|m| m.id.clone()).collect::<Vec<_>>(), before);
        assert!(ws.pending_delete.is_none());
    }

    #[test]
    fn moving_between_conversations() {
        let mut ws = workspace("19:maya");
        ws.directory = Some(crate::demo::Demo.directory().unwrap());
        let first = ws.neighbour(1, false);
        assert!(first.is_some_and(|id| id != "19:maya"));
        // From the newest, up goes nowhere.
        ws.open = Some(crate::demo::OPEN.to_string());
        let newest = {
            let mut all: Vec<_> =
                ws.directory.as_ref().unwrap().conversations.iter().filter(|c| c.last.is_some() && c.kind != Kind::Feed).collect();
            all.sort_by_key(|c| std::cmp::Reverse(c.last.as_ref().map_or(0, |l| l.at)));
            all[0].id.clone()
        };
        ws.open = Some(newest);
        assert_eq!(ws.neighbour(-1, false), None);
    }
}
