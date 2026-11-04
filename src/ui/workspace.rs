//! The signed-in state: the tab picked, what the list shows, the open
//! conversation and thread, and keeping them fresh.

use super::load::Load;
use super::pictures::Pictures;
use crate::backend::{Backend, Error, Push};
use crate::model::markdown::{self, Mention};
use crate::model::{
    Availability, Conversation, ConversationId, Directory, Kind, Meeting, Message, Picture, Presence, Preview, SearchHit, Status, text,
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
    /// Conversations' messages as last fetched, shown at once when one is opened
    /// (and fetched again behind); the newest few are fetched ahead.
    cache: HashMap<String, Vec<Message>>,
    prefetch: Vec<String>,
    prefetch_load: Load<(ConversationId, Vec<Message>), Error>,
    /// Messages sent from here, shown until Teams lists them (by conversation).
    sent_here: Vec<(String, Message)>,
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
    /// The person or emoji picked with the arrows in the composer's suggestions.
    pub suggestion: usize,
    /// How wide the list is (dragged by the user, kept in the settings).
    pub list_width: f32,
    /// Someone calling (answered on another device, or called back from here).
    pub ringing: Option<Ringing>,
    /// The caller's sign-in address, for calling them back: the call's id with it.
    address_load: Load<(String, Option<String>), Error>,
    /// When the ring last sounded.
    rang_at: Option<Instant>,
    /// The keyboard shortcuts' sheet is open.
    pub shortcuts: bool,
    /// When the others were last told the user is typing (and where).
    typing_sent: Option<(String, Instant)>,
    /// Who's typing where (by conversation: their id and name, and when they last said so).
    typing: HashMap<String, Vec<(String, String, Instant)>>,
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
    /// The newest message seen at the bottom of the open conversation (with its id),
    /// to count what came in below while scrolled up.
    pub seen_down_to: Option<(String, String)>,
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
    Forward,
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

/// A call ringing: its id, who's calling (id, name, and sign-in address once looked
/// up), whether it's a group call, and since when.
#[derive(Clone)]
pub struct Ringing {
    pub id: String,
    pub from: String,
    pub name: String,
    pub address: Option<String>,
    pub group: bool,
    pub since: Instant,
}

/// How long a deleted message can be brought back.
const UNDO_FOR: Duration = Duration::from_secs(5);

/// A file waiting in a composer to go with the next message: a picture (shown in
/// the message) or any other file (shared as a file).
#[derive(Clone)]
pub struct Attachment {
    pub name: String,
    pub bytes: Arc<Vec<u8>>,
    /// A picture's MIME type and size; None for any other file.
    pub picture: Option<(&'static str, (u32, u32))>,
}

/// The biggest file sent (Teams' own limit is far above; this keeps it in memory).
const BIGGEST_FILE: usize = 250 << 20;

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
            cache: HashMap::new(),
            prefetch: Vec::new(),
            prefetch_load: Load::Idle,
            sent_here: Vec::new(),
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
            shortcuts: false,
            ringing: None,
            address_load: Load::Idle,
            rang_at: None,
            list_width: crate::ui::style::metrics::LIST_WIDTH,
            suggestion: 0,
            typing: HashMap::new(),
            typing_sent: None,
            marked: None,
            notifications: true,
            seen: HashMap::new(),
            seen_once: false,
            clicked: std::sync::mpsc::channel(),
            newest: NEWEST,
            earlier_from: None,
            seen_down_to: None,
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
        let text = crate::model::emoji::emoticons(self.draft(reply).trim());
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
        // Shown at once, as Teams does: a new message at the bottom, an edit in place.
        let body = text::from_html(&message.html);
        match &editing {
            Some(id) => {
                if let Some(m) = self.messages.iter_mut().flatten().find(|m| m.id == *id) {
                    m.body = body;
                    m.edited = true;
                }
            }
            None => {
                let now = crate::model::time::now_ms();
                let shown = Message {
                    id: format!("sending-{now}"),
                    root: reply.then(|| self.thread.clone()).flatten(),
                    author: crate::model::Author { id: self.me_id.clone(), name: self.me.clone() },
                    at: now,
                    body,
                    card: None,
                    files: Vec::new(),
                    edited: false,
                    reactions: Vec::new(),
                    source: None,
                    sending: true,
                };
                if let Some(messages) = self.messages.as_mut() {
                    messages.push(shown.clone());
                }
                self.sent_here.push((open.clone(), shown));
            }
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
        let place = self.file_place(&open);
        self.sending = Load::start(ctx, move || {
            // Files go up first; the message then shows pictures under its text and the
            // other files as files.
            for file in &pictures {
                let Some((mime, (w, h))) = file.picture else {
                    let place = place.as_ref().ok_or_else(|| Error::Service("files can't be sent here".into()))?;
                    message.files.push(backend.upload_file(place, &file.name, &file.bytes)?);
                    continue;
                };
                let url = backend.upload_image(&to, &file.name, &file.bytes, mime)?;
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

    /// Add a file to the composer (the thread's when `reply`): a picture shows in the
    /// message, anything else goes as a file.
    pub fn attach(&mut self, reply: bool, name: &str, bytes: Vec<u8>) {
        if bytes.len() > BIGGEST_FILE {
            self.error = Some(format!("{name} is too big to send from drift (the most is {} MB)", BIGGEST_FILE >> 20));
            return;
        }
        let picture = super::pictures::image_info(&bytes).map(|(w, h, mime)| (mime, (w, h)));
        self.attachments.push((reply, Attachment { name: name.to_string(), bytes: Arc::new(bytes), picture }));
    }

    /// Where a file sent in `conversation` goes: a channel's folder, or the user's
    /// OneDrive shared with the chat's members.
    fn file_place(&self, conversation: &str) -> Option<crate::backend::FilePlace> {
        let d = self.directory.as_ref()?;
        if let Some((team, channel)) = d.channel(conversation) {
            let Some(Picture::Team { group, .. }) = &team.picture else { return None };
            return Some(crate::backend::FilePlace::Channel { group: group.clone(), channel: channel.id.clone() });
        }
        let c = d.conversation(conversation)?;
        Some(crate::backend::FilePlace::Chat { members: c.members.iter().map(|m| m.id.clone()).collect() })
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

    /// Names by id of everyone the user is seen with (for who reacted): the people of
    /// the open conversation and of the chats, the user as "You".
    pub fn names(&self) -> HashMap<String, String> {
        let mut names: HashMap<String, String> = self.people().into_iter().map(|p| (p.id, p.name)).collect();
        if let Some(c) = self.open.as_deref().and_then(|o| self.directory.as_ref()?.conversation(o)) {
            names.extend(c.members.iter().map(|m| (m.id.clone(), m.name.clone())));
        }
        if !self.me_id.is_empty() {
            names.insert(self.me_id.clone(), "You".into());
        }
        names
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
        let me = self.me_id.clone();
        match m.reactions.iter_mut().find(|r| r.key == key) {
            Some(r) => {
                r.mine = add;
                r.count = if add { r.count + 1 } else { r.count.saturating_sub(1) };
                r.who.retain(|w| *w != me);
                if add {
                    r.who.push(me);
                }
            }
            None if add => m.reactions.push(crate::model::Reaction { key: key.to_string(), count: 1, mine: true, who: vec![me] }),
            None => {}
        }
        m.reactions.retain(|r| r.count > 0);
        let (backend, message, key) = (self.backend.clone(), message.to_string(), key.to_string());
        self.chores.push((Chore::React, Load::start(ctx, move || backend.react(&open, &message, &key, add).map(|()| true))));
    }

    /// Whether the user wrote a conversation's newest message (by id when the service
    /// says who, else by name); unknown authors count as the user's (no notification).
    fn is_mine(&self, last: &Preview) -> bool {
        if !last.author_id.is_empty() && !self.me_id.is_empty() {
            last.author_id == self.me_id
        } else {
            last.author.is_empty() || last.author == self.me
        }
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
                Kind::Chat | Kind::Meeting if fresh && !self.is_mine(last) => new.push(c),
                Kind::Channel if followed && fresh && !self.is_mine(last) => new.push(c),
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
        let Some(meetings) = &self.meetings else { return };
        let now = crate::model::time::now_ms();
        let (due, next) = reminders(meetings, now, &self.reminded);
        let due: Vec<Meeting> = due.into_iter().cloned().collect();
        if let Some(at) = next {
            ctx.request_repaint_after(Duration::from_millis((at - now).clamp(0, 3_600_000) as u64));
        }
        for m in due {
            self.reminded.insert(reminder_key(&m));
            if !self.notifications {
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
                        // The one shown as on its way goes; the text is back in the composer.
                        if let Some(at) = self.sent_here.iter().rposition(|(c, _)| *c == outbox.conversation) {
                            let (_, gone) = self.sent_here.remove(at);
                            if let Some(messages) = self.messages.as_mut() {
                                messages.retain(|m| m.id != gone.id);
                            }
                        }
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
                Push::Message { conversation, message } => self.pushed(&conversation, *message),
                Push::Call { id, from, name, group } => self.called(ctx, id, &from, name, group),
                Push::Typing { conversation, id, name, typing } => {
                    let who = self.typing.entry(conversation).or_default();
                    who.retain(|(i, ..)| *i != id);
                    if typing && id != self.me_id && !name.is_empty() {
                        who.push((id, name, Instant::now()));
                    }
                }
                Push::Everything => {
                    self.directory_at = None;
                    self.messages_at = None;
                    self.activity_at = None;
                }
                Push::Conversation(id) => {
                    // A message in: whoever was typing it is done.
                    self.typing.remove(&id);
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
                Chore::Forward => self.error = Some(format!("Couldn't forward the message. {e}")),
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
                    self.queue_prefetch();
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
        self.ring(ctx);
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
        if let Some(result) = self.address_load.take() {
            match result {
                Ok((call, address)) => {
                    if let Some(ringing) = self.ringing.as_mut().filter(|r| r.id == call) {
                        ringing.address = address;
                    }
                }
                Err(e) => log_debug!("the caller's address: {e}"),
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
                    self.cache_messages(&id, &messages);
                    self.with_sent_here(&id, &mut messages);
                    self.messages = Some(messages);
                    self.mark_read(ctx);
                    // Finding while the messages came in: show the newest match.
                    if self.side == Side::Found && self.focus.is_none() && self.find.as_deref().is_some_and(|q| !q.trim().is_empty()) {
                        self.show_found(0);
                    }
                }
                Ok((id, messages)) => self.cache_messages(&id, &messages),
                Err(Error::SignedOut) => return Some(Outcome::SignedOut),
                Err(e) => self.failed(e),
            }
        }
        if let Some(result) = self.prefetch_load.take() {
            match result {
                Ok((id, messages)) => self.cache_messages(&id, &messages),
                Err(e) => log_debug!("fetching ahead: {e}"),
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
        // The newest conversations fetched ahead, one at a time, behind everything else.
        if !self.prefetch_load.is_running()
            && !self.messages_load.is_running()
            && let Some(id) = self.prefetch.pop()
        {
            let backend = self.backend.clone();
            self.prefetch_load = Load::start(ctx, move || backend.messages(&id).map(|m| (id, m)));
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

    /// Someone's calling: a notification (a click brings drift up), and in the window
    /// a bar with Call back and Decline that rings until one is clicked or it would
    /// have stopped ringing. drift can't take the call itself: it's answered on
    /// another device, or the caller is called back from Teams on the web.
    fn called(&mut self, ctx: &egui::Context, id: String, from: &str, name: String, group: bool) {
        if from == self.me_id || self.ringing.as_ref().is_some_and(|r| r.id == id) {
            return;
        }
        let title = if group { format!("{name} is calling (group call)") } else { format!("{name} is calling") };
        let ctx2 = ctx.clone();
        crate::notify::show(&title, Some("Call"), "Answer on your phone, or call back from drift", move || ctx2.request_repaint());
        let backend = self.backend.clone();
        let (call, person) = (id.clone(), from.to_string());
        self.address_load = Load::start(ctx, move || backend.address(&person).map(|a| (call, a)));
        self.ringing = Some(Ringing { id, from: from.to_string(), name, address: None, group, since: Instant::now() });
        self.rang_at = None;
    }

    /// Call the caller back (the ringing stops): Teams on the web opens calling them,
    /// or opens as it is when their address isn't known.
    pub fn call_back(&mut self, ctx: &egui::Context) {
        if let Some(call) = self.ringing.take() {
            ctx.open_url(egui::OpenUrl::new_tab(call_back_link(call.address.as_deref())));
        }
    }

    /// Stop the ringing here (the call still rings on the user's other devices: Teams
    /// takes a decline only from a client that has joined the call).
    pub fn decline_call(&mut self) {
        self.ringing = None;
    }

    /// While a call rings: the sound every few seconds (the notice shows it), until
    /// it's been ringing as long as Teams rings.
    fn ring(&mut self, ctx: &egui::Context) {
        const RINGS_FOR: Duration = Duration::from_secs(45);
        const EVERY: Duration = Duration::from_secs(4);
        if self.ringing.as_ref().is_some_and(|r| r.since.elapsed() >= RINGS_FOR) {
            self.ringing = None;
        }
        if self.ringing.is_none() {
            return;
        }
        if self.rang_at.is_none_or(|at| at.elapsed() >= EVERY) {
            self.rang_at = Some(Instant::now());
            crate::notify::chime();
        }
        ctx.request_repaint_after(Duration::from_millis(500));
    }

    /// A message Teams pushed: into the open conversation (and the kept ones) right
    /// away, new or in place of the one it changes. The fetch that follows confirms it.
    fn pushed(&mut self, conversation: &str, message: Message) {
        let place = |messages: &mut Vec<Message>| {
            // Those shown as on their way are put back after (see `with_sent_here`).
            messages.retain(|m| !m.sending);
            match messages.iter_mut().find(|m| m.id == message.id) {
                Some(old) => {
                    // Reactions aren't in a push the same way; keep what's known.
                    let reactions = std::mem::take(&mut old.reactions);
                    *old = Message { reactions, ..message.clone() };
                }
                None => {
                    let key = |m: &Message| (m.at, m.id.parse::<i64>().unwrap_or(0));
                    let at = messages.partition_point(|m| key(m) <= key(&message));
                    messages.insert(at, message.clone());
                }
            }
        };
        if let Some(kept) = self.cache.get_mut(conversation) {
            place(kept);
        }
        if self.open.as_deref() == Some(conversation)
            && let Some(mut messages) = self.messages.take()
        {
            place(&mut messages);
            self.with_sent_here(conversation, &mut messages);
            self.messages = Some(messages);
        }
    }

    /// Keep a conversation's messages for opening it again quickly (the most recent
    /// conversations' only).
    fn cache_messages(&mut self, id: &str, messages: &[Message]) {
        const KEPT: usize = 40;
        self.cache.insert(id.to_string(), messages.to_vec());
        if self.cache.len() > KEPT {
            let newest = |m: &Vec<Message>| m.last().map_or(0, |m| m.at);
            if let Some(oldest) = self
                .cache
                .iter()
                .filter(|(k, _)| Some(k.as_str()) != self.open.as_deref())
                .min_by_key(|(_, m)| newest(m))
                .map(|(k, _)| k.clone())
            {
                self.cache.remove(&oldest);
            }
        }
    }

    /// Messages sent from here that Teams doesn't list yet stay at the bottom; once
    /// it does (the same text from the user, since a minute before), they go.
    fn with_sent_here(&mut self, id: &str, messages: &mut Vec<Message>) {
        let now = crate::model::time::now_ms();
        let me = self.me_id.clone();
        self.sent_here.retain(|(c, shown)| {
            if c != id {
                return now - shown.at < 120_000;
            }
            let listed = messages
                .iter()
                .any(|m| m.author.id == me && m.at >= shown.at - 60_000 && text::flatten(&m.body) == text::flatten(&shown.body));
            !listed && now - shown.at < 120_000
        });
        for (_, shown) in self.sent_here.iter().filter(|(c, _)| c == id) {
            messages.push(shown.clone());
        }
    }

    /// The newest conversations whose messages aren't kept (or are older than their
    /// newest message): fetched ahead.
    fn queue_prefetch(&mut self) {
        const AHEAD: usize = 8;
        let Some(d) = self.directory.as_ref() else { return };
        let mut newest: Vec<&Conversation> = d.conversations.iter().filter(|c| c.kind != Kind::Feed && c.last.is_some()).collect();
        newest.sort_by_key(|c| std::cmp::Reverse(c.last.as_ref().map_or(0, |l| l.at)));
        let stale = |c: &Conversation| {
            let last = c.last.as_ref().map_or(0, |l| l.at);
            self.cache.get(&c.id).is_none_or(|m| m.last().is_none_or(|m| m.at < last))
        };
        // Popped from the end: the newest last.
        self.prefetch = newest
            .into_iter()
            .take(AHEAD)
            .filter(|c| stale(c) && Some(c.id.as_str()) != self.open.as_deref())
            .map(|c| c.id.clone())
            .rev()
            .collect();
    }

    /// Forward a message to another conversation (as a quote, saying who wrote it where).
    pub fn forward(&mut self, ctx: &egui::Context, message: &Message, to: &str) {
        let place =
            self.open.as_deref().and_then(|o| self.directory.as_ref()?.conversation(o)).map_or("a chat".to_string(), |c| c.title.clone());
        let outgoing = markdown::Outgoing { html: forwarded_html(message, &place), ..Default::default() };
        let (backend, to) = (self.backend.clone(), to.to_string());
        self.chores.push((Chore::Forward, Load::start(ctx, move || backend.send(&to, &outgoing).map(|()| false))));
    }

    /// The user typed in the composer: the others hear of it, once every few seconds.
    pub fn typed(&mut self) {
        let Some(open) = self.open.clone() else { return };
        if self.draft.trim().is_empty()
            || self.typing_sent.as_ref().is_some_and(|(c, at)| *c == open && at.elapsed() < Duration::from_secs(5))
        {
            return;
        }
        self.typing_sent = Some((open.clone(), Instant::now()));
        let backend = self.backend.clone();
        // Not a chore: nobody needs to hear if it didn't go.
        std::thread::spawn(move || {
            if let Err(e) = backend.typing(&open) {
                log_debug!("telling the others of typing: {e}");
            }
        });
    }

    /// "Maya is typing…" for the open conversation, if anyone is (Teams says so again
    /// every few seconds while they type; a message from them ends it too).
    pub fn typing(&mut self, ctx: &egui::Context) -> Option<String> {
        const LASTS: Duration = Duration::from_secs(6);
        let open = self.open.as_ref()?;
        let who = self.typing.get_mut(open)?;
        who.retain(|(.., at)| at.elapsed() < LASTS);
        let names: Vec<&str> = who.iter().map(|(_, name, _)| name.split_whitespace().next().unwrap_or(name)).collect();
        if names.is_empty() {
            return None;
        }
        ctx.request_repaint_after(Duration::from_secs(1));
        Some(match names.as_slice() {
            [one] => format!("{one} is typing…"),
            [a, b] => format!("{a} and {b} are typing…"),
            [a, rest @ ..] => format!("{a} and {} others are typing…", rest.len()),
            [] => unreachable!(),
        })
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
        // What was fetched before shows at once; the fetch that follows brings it up to date.
        self.messages = self.cache.get(id).cloned().map(|mut m| {
            self.with_sent_here(id, &mut m);
            m
        });
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

/// A message forwarded: who wrote it and where, then all of it, as a quote.
fn forwarded_html(m: &Message, place: &str) -> String {
    let esc = |s: &str| s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");
    let body = markdown::to_html_with(&text::to_markdown(&m.body), &[], &[]).html;
    format!("<p><i>Forwarded from {name} in {place}</i></p><blockquote>{body}</blockquote>", name = esc(&m.author.name), place = esc(place),)
}

/// Who a one-to-one chat is with.
fn person(c: &Conversation) -> Option<&str> {
    match c.members.as_slice() {
        [one] if c.kind == Kind::Chat => Some(one.id.as_str()),
        _ => None,
    }
}

/// Teams on the web, where a caller is called back (drift doesn't take calls itself).
const TEAMS_WEB: &str = "https://teams.microsoft.com/";

/// The link that has Teams on the web call `address` (a sign-in address, as Teams'
/// call links take); Teams on the web itself without one.
fn call_back_link(address: Option<&str>) -> String {
    match address.filter(|a| a.contains('@')) {
        Some(address) => url::Url::parse_with_params("https://teams.microsoft.com/l/call/0/0", [("users", address)])
            .map_or_else(|_| TEAMS_WEB.to_string(), String::from),
        None => TEAMS_WEB.to_string(),
    }
}

/// How long before a meeting it's reminded of, and how long after its start still.
const REMIND_BEFORE: i64 = 60_000;
const REMIND_AFTER: i64 = 5 * 60_000;

/// A meeting as reminded of: by id and start, so a meeting moved is reminded of again.
fn reminder_key(m: &Meeting) -> String {
    format!("{}@{}", m.id, m.start)
}

/// The meetings to remind of now (not cancelled, not declined, not yet reminded),
/// and when the next one is due.
fn reminders<'a>(meetings: &'a [Meeting], now: i64, reminded: &HashSet<String>) -> (Vec<&'a Meeting>, Option<i64>) {
    let mut due = Vec::new();
    let mut next: Option<i64> = None;
    for m in meetings.iter().filter(|m| !m.cancelled && m.response != "Decline") {
        let from = m.start - REMIND_BEFORE;
        if now < from {
            next = Some(next.map_or(from, |n| n.min(from)));
        } else if now <= m.start + REMIND_AFTER && !reminded.contains(&reminder_key(m)) {
            due.push(m);
        }
    }
    (due, next)
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
    fn meetings_are_reminded_of_once_a_minute_before() {
        let meeting = |id: &str, start: i64| Meeting {
            id: id.into(),
            subject: "Stand-up".into(),
            start,
            end: start + 900_000,
            organizer: String::new(),
            location: String::new(),
            join_url: None,
            chat: None,
            cancelled: false,
            response: "Accept".into(),
        };
        let start = 10_000_000;
        let none = HashSet::new();
        let ids = |(due, _): (Vec<&Meeting>, Option<i64>)| due.iter().map(|m| m.id.clone()).collect::<Vec<_>>();
        let m = [meeting("a", start)];
        assert!(ids(reminders(&m, start - 61_000, &none)).is_empty());
        assert_eq!(reminders(&m, start - 61_000, &none).1, Some(start - 60_000));
        assert_eq!(ids(reminders(&m, start - 59_000, &none)), ["a"]);
        assert_eq!(ids(reminders(&m, start + 299_000, &none)), ["a"]);
        assert!(ids(reminders(&m, start + 301_000, &none)).is_empty());
        let once = HashSet::from([reminder_key(&m[0])]);
        assert!(ids(reminders(&m, start - 30_000, &once)).is_empty());
        // Moved: reminded again.
        let moved = [meeting("a", start + 3_600_000)];
        assert_eq!(ids(reminders(&moved, start + 3_600_000 - 30_000, &once)), ["a"]);
        let declined = [Meeting { response: "Decline".into(), ..meeting("b", start) }];
        let cancelled = [Meeting { cancelled: true, ..meeting("c", start) }];
        assert!(ids(reminders(&declined, start, &none)).is_empty() && ids(reminders(&cancelled, start, &none)).is_empty());
    }

    #[test]
    fn new_messages_notify_only_when_they_should() {
        let mut ws = workspace("19:maya");
        ws.notifications = true;
        ws.me = "Alex Morgan".into();
        ws.me_id = crate::demo::ME.into();
        let ctx = egui::Context::default();
        let mut directory = crate::demo::Demo.directory().unwrap();
        // The first list only says what's been seen.
        ws.notify_new(&ctx, &directory);
        assert!(ws.toasts.conversations().is_empty());
        let newer = |d: &mut Directory, id: &str, author: &str, author_id: &str| {
            let c = d.conversations.iter_mut().find(|c| c.id == id).unwrap();
            let last = c.last.as_mut().unwrap();
            (last.at, last.author, last.author_id) = (last.at + 60_000, author.into(), author_id.into());
        };
        newer(&mut directory, "19:release", "Priya Nair", "8:orgid:priya"); // someone else: yes
        newer(&mut directory, "19:tomas", "Alex Morgan", crate::demo::ME); // the user's own: no
        newer(&mut directory, "19:maya", "Maya Lindqvist", "8:orgid:maya"); // on screen, in front: no
        let muted = directory.conversations.iter().find(|c| c.muted).map(|c| c.id.clone()).unwrap();
        newer(&mut directory, &muted, "Jonas Weber", "8:orgid:jonas"); // muted: no
        // A namesake of the user is someone else.
        newer(&mut directory, "19:priya", "Alex Morgan", "8:orgid:other-alex");
        ws.notify_new(&ctx, &directory);
        assert_eq!(ws.toasts.conversations(), ["19:release", "19:priya"]);
    }

    #[test]
    fn forwards_say_who_wrote_it_where() {
        let ws = workspace("19:maya");
        let m = ws.messages.iter().flatten().find(|m| m.id == "3006").unwrap();
        let html = forwarded_html(m, "Maya <& co>");
        assert!(html.starts_with("<p><i>Forwarded from Alex Morgan in Maya &lt;&amp; co&gt;</i></p><blockquote>"));
        assert!(html.contains("<b>release cut at 15:00</b>"));
    }

    #[test]
    fn a_sent_message_shows_at_once_until_teams_lists_it() {
        let mut ws = workspace("19:maya");
        ws.me_id = crate::demo::ME.into();
        ws.me = "Alex Morgan".into();
        ws.draft = "on my way".into();
        ws.send(&egui::Context::default(), false);
        let last = ws.messages.as_ref().unwrap().last().unwrap().clone();
        assert!(last.sending && text::flatten(&last.body) == "on my way");
        // A fetch from before it went: it stays at the bottom.
        let mut before = crate::demo::Demo.messages("19:maya").unwrap();
        ws.with_sent_here("19:maya", &mut before);
        assert!(before.last().unwrap().sending);
        // Teams lists it: the one shown goes.
        let mut after = crate::demo::Demo.messages("19:maya").unwrap();
        after.push(Message { id: "9999".into(), sending: false, at: last.at + 500, ..last.clone() });
        ws.with_sent_here("19:maya", &mut after);
        assert!(!after.iter().any(|m| m.sending));
        assert!(ws.sent_here.is_empty());
    }

    #[test]
    fn pushed_messages_show_without_a_fetch() {
        let mut ws = workspace("19:maya");
        ws.me_id = crate::demo::ME.into();
        let count = ws.messages.as_ref().unwrap().len();
        let mut m = ws.messages.as_ref().unwrap()[0].clone();
        // After the newest (the demo's are at fixed times of today, some maybe still to come).
        let newest = ws.messages.as_ref().unwrap().iter().map(|m| m.at).max().unwrap();
        (m.id, m.at, m.author.id) = ("9001".into(), newest + 1000, "8:orgid:maya".into());
        ws.pushed("19:maya", m.clone());
        assert_eq!(ws.messages.as_ref().unwrap().len(), count + 1);
        assert_eq!(ws.messages.as_ref().unwrap().last().unwrap().id, "9001");
        // The same message changed (an edit): in place.
        m.edited = true;
        ws.pushed("19:maya", m);
        assert_eq!(ws.messages.as_ref().unwrap().len(), count + 1);
        assert!(ws.messages.as_ref().unwrap().last().unwrap().edited);
        // Another conversation's: not here.
        let other = ws.messages.as_ref().unwrap()[1].clone();
        ws.pushed("19:tomas", Message { id: "9002".into(), ..other });
        assert_eq!(ws.messages.as_ref().unwrap().len(), count + 1);
    }

    #[test]
    fn a_conversation_fetched_before_opens_at_once() {
        let mut ws = workspace("19:maya");
        let tomas = crate::demo::Demo.messages("19:tomas").unwrap();
        ws.cache_messages("19:tomas", &tomas);
        ws.open("19:tomas");
        assert_eq!(ws.messages.as_deref(), Some(tomas.as_slice()));
        ws.open("19:release");
        assert!(ws.messages.is_none());
    }

    #[test]
    fn says_who_is_typing() {
        let mut ws = workspace("19:release");
        let ctx = egui::Context::default();
        assert_eq!(ws.typing(&ctx), None);
        let now = Instant::now();
        ws.typing.insert("19:release".into(), vec![("8:orgid:priya".into(), "Priya Nair".into(), now)]);
        assert_eq!(ws.typing(&ctx).as_deref(), Some("Priya is typing…"));
        ws.typing.get_mut("19:release").unwrap().push(("8:orgid:lea".into(), "Lea Novak".into(), now));
        assert_eq!(ws.typing(&ctx).as_deref(), Some("Priya and Lea are typing…"));
        // Said long ago: over.
        ws.typing.insert("19:release".into(), vec![("8:orgid:priya".into(), "Priya Nair".into(), now - Duration::from_secs(10))]);
        assert_eq!(ws.typing(&ctx), None);
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

    #[test]
    fn calls_back_by_address() {
        assert_eq!(
            call_back_link(Some("lea.novak@halcyon.example")),
            "https://teams.microsoft.com/l/call/0/0?users=lea.novak%40halcyon.example"
        );
        assert_eq!(
            call_back_link(Some("o'neil+x@halcyon.example")),
            "https://teams.microsoft.com/l/call/0/0?users=o%27neil%2Bx%40halcyon.example"
        );
        // Not an address (an id, say): Teams on the web as it is.
        assert_eq!(call_back_link(Some("8:orgid:lea")), TEAMS_WEB);
        assert_eq!(call_back_link(None), TEAMS_WEB);
    }
}
