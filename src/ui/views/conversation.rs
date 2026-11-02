//! The open conversation: a header, the messages as bubbles (others' on the left
//! with their picture, the user's own on the right), and the composer. In a channel
//! a post links to its thread. Right-clicking a message reacts, replies or copies.

use crate::model::{Directory, Kind, Message, Picture, markdown, text, time};
use crate::ui::pictures::Pictures;
use crate::ui::style::icons::{self, Icon};
use crate::ui::style::{colors, metrics, typography as ty};
use crate::ui::views::emoji_picker;
use crate::ui::widgets;
use crate::ui::workspace::{Side, Workspace};
use eframe::egui::{self, Align, CornerRadius, Frame, Layout, Margin, Rect, RichText, Sense, Stroke, Ui, pos2, vec2};
use std::collections::HashMap;

/// The box's height with one line of text, and its tallest (more text scrolls in it).
const BOX_LOW: f32 = 56.0;
const BOX_HIGH: f32 = 260.0;
/// Room above and below the text inside the box.
const TEXT_PAD: f32 = 14.0;
/// Room above the box, and below it (for the formatting hint).
const ABOVE: f32 = 8.0;
const BELOW: f32 = 22.0;
/// The "replying to" / "editing" line above the composer.
const BANNER: f32 = 44.0;
/// The reactions offered on right-click.
const QUICK_REACTIONS: [(&str, &str); 6] =
    [("like", "👍"), ("heart", "❤"), ("laugh", "😆"), ("surprised", "😮"), ("sad", "😢"), ("praying", "🙏")];

/// What a click on a message asks for.
pub enum Action {
    OpenThread(String),
    React {
        message: String,
        key: String,
    },
    Quote(Message),
    Edit(Message),
    Delete(String),
    /// Bring a message into view and mark it (a reply's quote was clicked).
    Show(String),
    /// Show older messages too.
    Earlier,
    /// Mark the conversation unread from this message on.
    UnreadFrom(String),
    /// See a picture big.
    View(String),
}

/// Do what a click on a message asked for; `reply` when it came from the thread.
pub fn apply(ui: &Ui, ws: &mut Workspace, action: Action, reply: bool) {
    match action {
        Action::OpenThread(root) => {
            ws.thread = Some(root);
            ws.reply_draft.clear();
        }
        Action::React { message, key } => {
            ws.used_emoji(&key);
            ws.react(ui.ctx(), &message, &key);
        }
        Action::Quote(message) => {
            ws.quote(message, reply);
            focus_composer(ui, reply);
        }
        Action::Edit(message) => {
            ws.edit(&message, reply);
            focus_composer(ui, reply);
        }
        Action::Delete(id) => ws.delete(ui.ctx(), &id),
        Action::Show(id) => {
            ws.focus = Some(id);
            ws.focus_scrolled = false;
        }
        Action::View(url) => ws.viewing = Some(url),
        Action::UnreadFrom(message) => ws.unread_from(ui.ctx(), &message),
        Action::Earlier => {
            ws.newest += crate::ui::workspace::NEWEST;
            ws.earlier_from = ws.list_height.as_ref().map(|(_, h)| *h);
        }
    }
}

fn composer_id(reply: bool) -> egui::Id {
    egui::Id::unique(if reply { "reply-composer" } else { "composer" })
}

fn focus_composer(ui: &Ui, reply: bool) {
    ui.memory_mut(|m| m.request_focus(composer_id(reply)));
}

/// The composer's height: its box grows with the text typed, and it's taller with a
/// "replying to" or "editing" line, or pictures, on top.
pub fn composer_height(ws: &Workspace, reply: bool) -> f32 {
    let banner = ws.quoting.as_ref().is_some_and(|(r, _)| *r == reply) || ws.editing.as_ref().is_some_and(|(r, _)| *r == reply);
    let pictures = ws.attachments.iter().any(|(r, _)| *r == reply);
    ABOVE + box_height(ws, reply) + BELOW + if banner { BANNER } else { 0.0 } + if pictures { BANNER } else { 0.0 }
}

/// The box's height for the text typed (as tall as it was last drawn).
fn box_height(ws: &Workspace, reply: bool) -> f32 {
    (ws.composer_text[usize::from(reply)] + 2.0 * TEXT_PAD).clamp(BOX_LOW, BOX_HIGH)
}

pub fn show(ui: &mut Ui, ws: &mut Workspace) {
    let p = colors();
    let full = ui.max_rect();
    let header = Rect::from_min_size(full.min, vec2(full.width(), metrics::HEADER));

    let Some(id) = ws.open.clone() else {
        empty(ui, ws.directory.is_some());
        return;
    };
    let directory = ws.directory.clone().unwrap_or_default();
    let conversation = directory.conversation(&id).cloned();
    let kind = conversation.as_ref().map(|c| c.kind);
    let is_channel = kind == Some(Kind::Channel) || directory.channel(&id).is_some();
    let can_write = kind != Some(Kind::Feed);

    head(ui, ws, header, &directory, &id);
    ui.painter().hline(full.x_range(), header.bottom() - 0.5, Stroke::new(1.0, p.line));

    let bottom = if can_write { full.bottom() - composer_height(ws, false) } else { full.bottom() };
    let body = Rect::from_min_max(pos2(full.left(), header.bottom()), pos2(full.right(), bottom));
    let mut page = ui.new_child(egui::UiBuilder::new().max_rect(body).layout(Layout::top_down(Align::Min)));
    let mut action = None;
    match ws.messages.clone() {
        None => {
            page.add_space(40.0);
            page.vertical_centered(|ui| ui.spinner());
        }
        Some(messages) => {
            // Drawn the first time: laid out once unseen, so the list opens at its newest
            // messages rather than showing its oldest for a frame.
            if ws.drawn.as_deref() != Some(id.as_str()) {
                ws.drawn = Some(id.clone());
                ui.ctx().request_discard("a conversation's first layout");
            }
            // Finding: the matches are marked in the text (the list panel lists them).
            let query = ws.find.as_deref().map(str::trim).filter(|q| !q.is_empty()).map(str::to_string);
            widgets::set_highlight(query.clone());
            let me = ws.me_id.clone();
            let open_thread = ws.thread.clone();
            let focus = ws.focus.clone();
            let read_by = ws.read_by.clone();
            let members = conversation.as_ref().map(|c| c.members.clone()).unwrap_or_default();
            let custom_emoji = ws.custom_emoji.clone();
            let recent_emoji = ws.recent_emoji.clone().unwrap_or_default();
            let mut view = View {
                me: &me,
                read_by: &read_by,
                channel: is_channel,
                open: open_thread.as_deref(),
                focus: focus.as_deref(),
                scrolled: ws.focus_scrolled,
                newest: ws.newest,
                conversation: &id,
                top_at: None,
                custom_emoji: &custom_emoji,
                recent_emoji: &recent_emoji,
                new_after: ws.new_after,
                new_scrolled: ws.new_scrolled || ws.focus.is_some(),
                members: &members,
            };
            let mut area = egui::ScrollArea::vertical().id_salt(("messages", &id)).stick_to_bottom(true).auto_shrink([false, false]);
            if let Some(y) = ws.scroll_to.take() {
                // Not pulled back to the bottom while it was there.
                area = area.vertical_scroll_offset(y).stick_to_bottom(false);
            }
            let scrolled = area.show(&mut page, |ui| {
                Frame::new().inner_margin(Margin::symmetric(metrics::PAGE_PAD as i8, 10)).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    if messages.is_empty() {
                        ui.add_space(30.0);
                        ui.vertical_centered(|ui| ui.label(RichText::new("No messages yet. Say hello.").font(ty::body()).color(p.faint)));
                    } else {
                        action = list(ui, &mut ws.pictures, &messages, &mut view);
                    }
                });
            });
            // The list grew or shrank (a picture came, "Seen" moved): this frame was drawn
            // at the old scroll position, so it's laid out again at the new one rather
            // than shown a frame off.
            let height = scrolled.content_size.y;
            // Earlier messages came in above: stay on what was in view.
            if let Some(before) = ws.earlier_from.filter(|before| *before != height) {
                ws.earlier_from = None;
                ws.scroll_to = Some(scrolled.state.clamped_offset().y + height - before);
            }
            if ws.list_height.as_ref().is_some_and(|(of, h)| *of == id && *h != height) {
                ui.ctx().request_discard("the message list changed height");
            }
            ws.list_height = Some((id.clone(), height));
            // Scrolled up: the day of the message on top, floating over the list.
            let offset = scrolled.state.clamped_offset().y;
            let from_bottom = height - scrolled.inner_rect.height() - offset;
            if (offset - ws.scrolled_at.0).abs() > 0.5 && from_bottom > 40.0 {
                ws.scrolled_at.1 = Some(std::time::Instant::now());
            }
            ws.scrolled_at.0 = offset;
            if let (Some(at), Some(since)) = (view.top_at, ws.scrolled_at.1) {
                floating_day(ui, scrolled.inner_rect, at, since.elapsed().as_secs_f32());
            }
            // The focused message is in the main list unless it's a reply (then the thread scrolls).
            if ws.thread.is_none() || !is_channel {
                ws.focus_scrolled |= view.scrolled;
            }
            ws.new_scrolled |= view.new_scrolled;
        }
    }
    if let Some(action) = action {
        apply(ui, ws, action, false);
    }
    if can_write {
        let rect = Rect::from_min_max(pos2(full.left(), bottom), full.max);
        let name = conversation.as_ref().map_or(String::new(), |c| c.title.clone());
        let hint = if is_channel { format!("Message #{name}") } else { format!("Message {name}") };
        composer(ui, ws, rect, &hint, false);
    }
}

/// The header: the conversation's picture or mark, its name, where it belongs, and
/// actions in small squares.
fn head(ui: &mut Ui, ws: &mut Workspace, rect: Rect, directory: &Directory, id: &str) {
    let p = colors();
    let conversation = directory.conversation(id);
    let inner = rect.shrink2(vec2(metrics::PAGE_PAD, 0.0));
    let pic = Rect::from_min_size(pos2(inner.left(), rect.center().y - 18.0), vec2(36.0, 36.0));
    match conversation.map(|c| c.kind) {
        Some(Kind::Chat) => {
            let c = conversation.expect("a chat");
            let picture = c.picture.as_ref().and_then(|pic| ws.pictures.get(pic));
            let mut child = ui.new_child(egui::UiBuilder::new().max_rect(pic));
            widgets::avatar(&mut child, &c.id, &c.title, 36.0, picture);
            if let Some(presence) = ws.presence_of(c) {
                widgets::presence_dot(ui.painter(), pic.right_bottom() - vec2(4.0, 4.0), 5.0, presence, p.panel);
            }
        }
        kind => {
            ui.painter().rect_filled(pic, CornerRadius::same(11), p.surface);
            let icon = match kind {
                Some(Kind::Meeting) => Icon::Calendar,
                Some(Kind::Feed) => Icon::Bell,
                _ => Icon::Hash,
            };
            icons::paint(ui.painter(), Rect::from_center_size(pic.center(), vec2(17.0, 17.0)), icon, p.dim);
        }
    }
    let sub = if let Some((team, _)) = directory.channel(id) {
        team.name.clone()
    } else {
        match conversation.map(|c| c.kind) {
            Some(Kind::Meeting) => "Meeting chat".into(),
            Some(Kind::Feed) => "Mentions and replies".into(),
            _ => String::new(),
        }
    };
    let text = Rect::from_min_max(pos2(pic.right() + 12.0, rect.top()), pos2(inner.right() - 130.0, rect.bottom()));
    let mut t = ui.new_child(egui::UiBuilder::new().max_rect(text).layout(Layout::top_down(Align::Min)));
    t.add_space(if sub.is_empty() { 22.0 } else { 13.0 });
    t.spacing_mut().item_spacing.y = 1.0;
    let name = conversation.map_or("", |c| c.title.as_str());
    t.add(egui::Label::new(RichText::new(name).font(ty::title()).color(p.text)).truncate().selectable(false));
    if !sub.is_empty() {
        t.label(RichText::new(sub).font(ty::small()).color(p.faint));
    }

    let mut x = inner.right();
    let join = conversation.and_then(|c| c.join_url.clone());
    let mut square = |ui: &mut Ui, icon: Icon, hint: &str| {
        x -= 34.0;
        let r = Rect::from_min_size(pos2(x, rect.center().y - 17.0), vec2(34.0, 34.0));
        x -= 8.0;
        let response = ui.interact(r, ui.scope_id().with(("head", hint)), Sense::click()).on_hover_text(hint);
        ui.painter().rect_filled(r, CornerRadius::same(10), if response.hovered() { p.hover } else { p.surface });
        icons::paint(ui.painter(), Rect::from_center_size(r.center(), vec2(16.0, 16.0)), icon, p.dim);
        response
    };
    if let Some(url) = &join
        && square(ui, Icon::Video, "Join the meeting (Teams on the web)").clicked()
    {
        ui.ctx().open_url(egui::OpenUrl::new_tab(crate::teams::web_join(url)));
    }
    if square(ui, Icon::Members, "People here").clicked() {
        ws.side = if ws.side == Side::People { Side::List } else { Side::People };
    }
    if square(ui, Icon::Search, "Find in this conversation (⌘F)").clicked()
        || ui.input_mut(|i| i.consume_shortcut(&egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::F)))
    {
        if ws.find.is_some() {
            ws.stop_find();
        } else {
            ws.start_find();
            ui.memory_mut(|m| m.request_focus(find_id()));
        }
    }
    // The find field, left of the buttons, while finding: what's typed, which match
    // is shown of how many, and ↑ ↓ to go between them (also Shift+Enter / Enter).
    if ws.find.is_some() {
        let field = Rect::from_min_size(pos2(x - 300.0, rect.center().y - 17.0), vec2(300.0, 34.0));
        x = field.left() - 8.0;
        ui.painter().rect(field, CornerRadius::same(10), p.surface, Stroke::new(1.5, p.accent_fill), egui::StrokeKind::Inside);
        icons::paint(
            ui.painter(),
            Rect::from_min_size(pos2(field.left() + 10.0, field.center().y - 7.5), vec2(15.0, 15.0)),
            Icon::Search,
            p.accent,
        );
        let count = ws.found().len();
        let typed = ws.find.as_deref().is_some_and(|q| !q.trim().is_empty());
        // ↑ ↓ on the right, the count left of them.
        let mut step = 0i32;
        for (k, icon, tip) in [(0, Icon::ChevronDown, "Older match (Enter)"), (1, Icon::ChevronUp, "Newer match (Shift+Enter)")] {
            let r = Rect::from_center_size(pos2(field.right() - 16.0 - 26.0 * k as f32, field.center().y), vec2(24.0, 24.0));
            let b = ui.interact(r, ui.scope_id().with(("find-step", k)), Sense::click()).on_hover_text(tip);
            if b.hovered() && count > 0 {
                ui.painter().rect_filled(r, CornerRadius::same(6), p.hover);
            }
            icons::paint(ui.painter(), Rect::from_center_size(r.center(), vec2(13.0, 13.0)), icon, if count > 0 { p.dim } else { p.faint });
            if b.clicked() && count > 0 {
                step = if k == 0 { 1 } else { -1 };
            }
        }
        if typed {
            let label = if count == 0 { "No matches".to_string() } else { format!("{} of {count}", ws.find_index + 1) };
            let color = if count == 0 { p.danger } else { p.faint };
            ui.painter().text(pos2(field.right() - 62.0, field.center().y), egui::Align2::RIGHT_CENTER, label, ty::meta(), color);
        }
        let line = ui.painter().layout_no_wrap("Ag".into(), ty::small(), p.text).size().y;
        let text = Rect::from_min_max(
            pos2(field.left() + 32.0, field.center().y - line / 2.0),
            pos2(field.right() - 130.0, field.center().y + line / 2.0),
        );
        let mut c = ui.new_child(egui::UiBuilder::new().max_rect(text));
        let query = ws.find.get_or_insert_default();
        let response = c.add(
            egui::TextEdit::singleline(query)
                .id(find_id())
                .hint_text(RichText::new("Find in this conversation").font(ty::small()).color(p.faint))
                .font(ty::small())
                .frame(Frame::NONE)
                .margin(Margin::ZERO)
                .desired_width(f32::INFINITY),
        );
        if response.changed() {
            ws.show_found(0);
        }
        if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            step = if ui.input(|i| i.modifiers.shift) { -1 } else { 1 };
            response.request_focus();
        }
        if step != 0 && count > 0 {
            ws.show_found((ws.find_index as i64 + i64::from(step)).rem_euclid(count as i64) as usize);
        }
        if response.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            ws.stop_find();
        }
    }
    if let Some(error) = &ws.error {
        let r = Rect::from_min_max(pos2(x - 260.0, rect.top()), pos2(x, rect.bottom()));
        let mut e = ui.new_child(egui::UiBuilder::new().max_rect(r).layout(Layout::right_to_left(Align::Center)));
        e.add(egui::Label::new(RichText::new(error).font(ty::small()).color(p.danger)).truncate());
    }
}

fn find_id() -> egui::Id {
    egui::Id::unique("find")
}

fn empty(ui: &mut Ui, loaded: bool) {
    let p = colors();
    ui.vertical_centered(|ui| {
        ui.add_space(ui.available_height() / 2.0 - 30.0);
        let text = if loaded { "Pick a chat or a channel." } else { "Loading…" };
        ui.label(RichText::new(text).font(ty::body()).color(p.faint));
    });
}

/// A day, as a pill in the middle.
pub fn day_pill(ui: &mut Ui, at: i64) {
    let p = colors();
    ui.add_space(10.0);
    let label = time::day(at, time::now_ms());
    let galley = ui.painter().layout_no_wrap(label, ty::pill(), p.faint);
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 26.0), Sense::hover());
    let pill = Rect::from_center_size(rect.center(), vec2(galley.size().x + 24.0, 24.0));
    ui.painter().rect_filled(pill, CornerRadius::same(12), p.surface);
    ui.painter().galley(pill.center() - galley.size() / 2.0, galley, p.faint);
    ui.add_space(4.0);
}

/// How long the floating day stays after scrolling stops, and its fade.
const FLOATING_DAY_STAYS: f32 = 1.2;
const FLOATING_DAY_FADE: f32 = 0.3;

/// The day of the message on top of the list, in a pill floating at the top while
/// the user scrolls through older messages.
fn floating_day(ui: &Ui, list: Rect, at: i64, since: f32) {
    let alpha = 1.0 - ((since - FLOATING_DAY_STAYS) / FLOATING_DAY_FADE).clamp(0.0, 1.0);
    if alpha <= 0.0 {
        return;
    }
    ui.ctx().request_repaint();
    let p = colors();
    let galley = ui.painter().layout_no_wrap(time::day(at, time::now_ms()), ty::pill(), p.dim.gamma_multiply(alpha));
    let pill = Rect::from_center_size(pos2(list.center().x, list.top() + 22.0), vec2(galley.size().x + 24.0, 24.0));
    let painter = ui.painter().with_clip_rect(list);
    painter.rect(
        pill,
        CornerRadius::same(12),
        p.panel.gamma_multiply(alpha),
        Stroke::new(1.0, p.border.gamma_multiply(alpha)),
        egui::StrokeKind::Inside,
    );
    painter.galley(pill.center() - galley.size() / 2.0, galley, p.dim.gamma_multiply(alpha));
}

/// The messages: in a channel its posts (replies are in their threads), in a chat
/// all of them; grouped by author into runs of bubbles.
/// How a list of messages is shown: whose they are, whether it's a channel, the post
/// whose thread is open, and a message to bring into view and mark.
pub struct View<'a> {
    pub me: &'a str,
    /// In a chat, up to which message each other member has read (for "Seen").
    pub read_by: &'a [(String, i64)],
    pub channel: bool,
    pub open: Option<&'a str>,
    pub focus: Option<&'a str>,
    /// Set once the focused message has been scrolled to.
    pub scrolled: bool,
    /// How many of the newest messages are shown (the rest behind "Show earlier").
    pub newest: usize,
    /// The conversation's id (for links to its messages).
    pub conversation: &'a str,
    /// When the message at the top of the visible part was sent (set by [`list`]).
    pub top_at: Option<i64>,
    /// For reacting with any emoji: the organisation's, and those picked lately.
    pub custom_emoji: &'a [crate::model::CustomEmoji],
    pub recent_emoji: &'a [String],
    /// The last message read before the conversation was opened (by id): a "New
    /// messages" line goes above the first one after it from someone else.
    pub new_after: Option<i64>,
    /// Set once the list has been scrolled to that line (or there's none to scroll to).
    pub new_scrolled: bool,
    /// Who's in the chat, for the names of those who've read a message.
    pub members: &'a [crate::model::Mention],
}

/// Who has read a message of the user's (by name) and who hasn't yet, in a chat
/// where the service says how far each member has read.
fn readers(m: &Message, view: &View) -> Option<(Vec<String>, Vec<String>)> {
    let id = m.id.parse::<i64>().ok()?;
    if view.read_by.is_empty() || view.me.is_empty() || m.author.id != view.me {
        return None;
    }
    let name = |who: &str| view.members.iter().find(|p| p.id == who).map_or_else(|| "Someone".to_string(), |p| p.name.clone());
    let (read, unread): (Vec<_>, Vec<_>) = view.read_by.iter().partition(|(_, up_to)| *up_to >= id);
    let names = |list: Vec<&(String, i64)>| {
        let mut n: Vec<String> = list.into_iter().map(|(who, _)| name(who)).collect();
        n.sort();
        n
    };
    Some((names(read), names(unread)))
}

/// The names of who has read a message, and who hasn't, as lines for a tooltip.
fn readers_text(read: &[String], unread: &[String]) -> String {
    let mut lines = Vec::new();
    if !read.is_empty() {
        lines.push(format!("Seen by {}", read.join(", ")));
    }
    if !unread.is_empty() {
        lines.push(format!("Not yet {}", unread.join(", ")));
    }
    lines.join("\n")
}

/// Whether `focus` is one of the replies to `m`.
fn thread_has(replies: &HashMap<&str, Vec<&Message>>, m: &Message, focus: &str) -> bool {
    replies.get(m.id.as_str()).is_some_and(|r| r.iter().any(|reply| reply.id == focus))
}

pub fn list(ui: &mut Ui, pictures: &mut Pictures, messages: &[Message], view: &mut View) -> Option<Action> {
    let channel = view.channel;
    let ids: std::collections::HashSet<&str> = messages.iter().map(|m| m.id.as_str()).collect();
    let mut replies: HashMap<&str, Vec<&Message>> = HashMap::new();
    let mut shown = Vec::new();
    for m in messages {
        match m.root.as_deref().filter(|r| channel && ids.contains(r)) {
            Some(root) => replies.entry(root).or_default().push(m),
            None => shown.push(m),
        }
    }
    // "Seen" goes under the user's newest message.
    let newest_mine = shown.iter().rev().find(|m| !view.me.is_empty() && m.author.id == view.me).map(|m| m.id.clone());
    let mut action = None;
    // Only the newest are laid out (fewer pictures loading above them, and a short list
    // to scroll), back far enough to include a message to bring into view.
    let mut start = shown.len().saturating_sub(view.newest);
    if let Some(at) = view.focus.and_then(|f| shown.iter().position(|m| m.id == f || thread_has(&replies, m, f))) {
        start = start.min(at);
    }
    let first_new = view.new_after.and_then(|after| {
        shown.iter().position(|m| m.id.parse::<i64>().is_ok_and(|id| id > after) && (view.me.is_empty() || m.author.id != view.me))
    });
    if let Some(at) = first_new {
        start = start.min(at);
    }
    if start > 0 {
        ui.vertical_centered(|ui| {
            let label = RichText::new("Show earlier messages").font(ty::small_strong()).color(colors().accent);
            if ui.add(egui::Button::new(label).frame(false)).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                action = Some(Action::Earlier);
            }
        });
        ui.add_space(6.0);
    }
    let shown = &shown[start..];
    for (i, m) in shown.iter().enumerate() {
        let prev = i.checked_sub(1).map(|j| shown[j]);
        let next = shown.get(i + 1).copied();
        let thread = replies.get(m.id.as_str()).map(Vec::as_slice).unwrap_or(&[]);
        let new_day = prev.is_none_or(|prev| time::day_number(prev.at) != time::day_number(m.at));
        if ui.cursor().top() <= ui.clip_rect().top() + 40.0 {
            view.top_at = Some(m.at);
        }
        if new_day {
            day_pill(ui, m.at);
        }
        if first_new == Some(start + i) {
            let line = new_messages(ui);
            if !view.new_scrolled {
                ui.scroll_to_rect(line, Some(Align::Min));
                view.new_scrolled = true;
            }
        }
        let prev_thread = prev.is_some_and(|prev| replies.contains_key(prev.id.as_str()));
        let first = new_day || prev_thread || !prev.is_some_and(|prev| continues(prev, m));
        let last = !thread.is_empty() || !next.is_some_and(|n| continues(m, n) && time::day_number(n.at) == time::day_number(m.at));
        let threaded = channel.then_some(thread);
        if let Some(a) = bubble(ui, pictures, m, view, first, last, threaded) {
            action = Some(a);
        }
        if newest_mine.as_deref() == Some(m.id.as_str()) && !view.read_by.is_empty() {
            seen(ui, m, view);
        }
    }
    action
}

/// A line with "New messages" above the first unread one.
fn new_messages(ui: &mut Ui) -> Rect {
    let p = colors();
    ui.add_space(8.0);
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 18.0), Sense::hover());
    let galley = ui.painter().layout_no_wrap("New messages".into(), ty::meta(), p.accent);
    let label = Rect::from_center_size(rect.center(), galley.size() + vec2(16.0, 0.0));
    let stroke = Stroke::new(1.0, p.accent.gamma_multiply(0.6));
    ui.painter().hline(rect.left()..=label.left(), rect.center().y, stroke);
    ui.painter().hline(label.right()..=rect.right(), rect.center().y, stroke);
    ui.painter().galley(label.center() - galley.size() / 2.0, galley, p.accent);
    ui.add_space(2.0);
    // Some room above it when it's scrolled to the top.
    rect.expand2(vec2(0.0, 40.0))
}

fn continues(prev: &Message, m: &Message) -> bool {
    prev.author.id == m.author.id && m.at - prev.at < metrics::GROUP_GAP
}

/// The bar over a message while the pointer's on it: quick reactions, quote, and a
/// menu of the rest. It stays while its menu is open.
fn hover_bar(ui: &mut Ui, pictures: &mut Pictures, m: &Message, mine: bool, thread: bool, view: &View, bubble: Rect) -> Option<Action> {
    let conversation = view.conversation;
    const CELL: f32 = 32.0;
    let p = colors();
    let more_id = egui::Id::unique(("message-more", conversation, &m.id));
    let emoji_id = egui::Id::unique(("message-emoji", conversation, &m.id));
    let menu_open = egui::Popup::is_id_open(ui.ctx(), more_id) || egui::Popup::is_id_open(ui.ctx(), emoji_id);
    let cells = QUICK_REACTIONS.len() + 3;
    let size = vec2(CELL * cells as f32 + 8.0, CELL + 6.0);
    let x = if mine { bubble.left() + 8.0 } else { (bubble.right() - size.x - 8.0).max(bubble.left() + 8.0) };
    // Inside the list, whatever the bubble's width (a short one of the user's sits at
    // the right edge, and the bar would run past it).
    let room = ui.clip_rect().shrink(6.0);
    let x = x.min(room.right() - size.x).max(room.left());
    let bar = Rect::from_min_size(pos2(x, bubble.top() - size.y + 10.0), size);
    // One bar at a time: a bar reaches over the bubble above, and the pointer on it
    // is on both. The bar shown last frame keeps it.
    let shown_id = egui::Id::unique("hover-bar");
    let frame = ui.ctx().cumulative_frame_nr();
    let shown: Option<(String, Rect)> = ui
        .ctx()
        .data(|d| d.get_temp::<(String, Rect, u64)>(shown_id))
        .filter(|(_, _, at)| frame.saturating_sub(*at) <= 1)
        .map(|(id, r, _)| (id, r));
    let pointer = ui.ctx().pointer_hover_pos();
    let on_other_bar = shown.as_ref().is_some_and(|(id, r)| *id != m.id && pointer.is_some_and(|at| r.contains(at)));
    let on_own_bar = shown.as_ref().is_some_and(|(id, _)| *id == m.id) && pointer.is_some_and(|at| bar.contains(at));
    // DRIFT_DEMO_HOVER=<message>: the bar shown over that message, as if pointed at.
    let over = !on_other_bar && (pointer.is_some_and(|at| bubble.contains(at)) || on_own_bar)
        || std::env::var("DRIFT_DEMO_HOVER").is_ok_and(|id| id == m.id);
    if !(over || menu_open) || ui.is_sizing_pass() {
        return None;
    }
    ui.ctx().data_mut(|d| d.insert_temp(shown_id, (m.id.clone(), bar, frame)));
    ui.painter().add(
        egui::Shadow { offset: [0, 3], blur: 10, spread: 0, color: egui::Color32::from_black_alpha(30) }
            .as_shape(bar, CornerRadius::same(10)),
    );
    ui.painter().rect(bar, CornerRadius::same(10), p.panel, Stroke::new(1.0, p.border), egui::StrokeKind::Inside);
    let mut action = None;
    let cell = |i: usize| Rect::from_min_size(pos2(bar.left() + 4.0 + CELL * i as f32, bar.top() + 3.0), vec2(CELL, CELL));
    for (i, (key, emoji)) in QUICK_REACTIONS.iter().enumerate() {
        let rect = cell(i);
        let response = ui.interact(rect, ui.scope_id().with(("react", &m.id, i)), Sense::click()).on_hover_text(*key);
        if response.hovered() {
            ui.painter().rect_filled(rect.shrink(1.0), CornerRadius::same(8), p.hover);
        }
        match pictures.get(&Picture::Emoji(key.to_string())) {
            Some(texture) => {
                egui::Image::new((texture, vec2(20.0, 20.0))).paint_at(ui, Rect::from_center_size(rect.center(), vec2(20.0, 20.0)));
            }
            None => {
                ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, *emoji, egui::FontId::proportional(16.0), p.text);
            }
        }
        if response.clicked() {
            action = Some(Action::React { message: m.id.clone(), key: key.to_string() });
        }
    }
    // Any other emoji, from the whole set.
    let rect = cell(QUICK_REACTIONS.len());
    let others = ui.interact(rect, ui.scope_id().with(("emoji", &m.id)), Sense::click()).on_hover_text("More reactions");
    if others.hovered() || egui::Popup::is_id_open(ui.ctx(), emoji_id) {
        ui.painter().rect_filled(rect.shrink(1.0), CornerRadius::same(8), p.hover);
    }
    icons::paint(ui.painter(), Rect::from_center_size(rect.center(), vec2(18.0, 18.0)), Icon::Smile, p.dim);
    egui::Popup::menu(&others).id(emoji_id).close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside).show(|ui| {
        let state = egui::Id::unique(("message-emoji-query", &m.id));
        let mut picker = ui.ctx().data_mut(|d| d.get_temp::<emoji_picker::Picker>(state)).unwrap_or_default();
        let picked = emoji_picker::show(ui, pictures, &mut picker, &m.id, view.custom_emoji, view.recent_emoji);
        ui.ctx().data_mut(|d| d.insert_temp(state, picker));
        if let Some(e) = picked {
            action = Some(Action::React { message: m.id.clone(), key: e.key.to_string() });
            ui.ctx().data_mut(|d| d.remove::<emoji_picker::Picker>(state));
            ui.close();
        }
    });
    // Quote: reply with this message quoted.
    let rect = cell(QUICK_REACTIONS.len() + 1);
    let quote = ui.interact(rect, ui.scope_id().with(("quote", &m.id)), Sense::click()).on_hover_text("Reply with quote");
    if quote.hovered() {
        ui.painter().rect_filled(rect.shrink(1.0), CornerRadius::same(8), p.hover);
    }
    ui.painter().text(rect.center() + vec2(0.0, 5.0), egui::Align2::CENTER_CENTER, "\u{201C}", egui::FontId::new(26.0, ty::bold()), p.dim);
    if quote.clicked() {
        action = Some(Action::Quote(m.clone()));
    }
    // More: the rest of the message's menu.
    let rect = cell(QUICK_REACTIONS.len() + 2);
    let more = ui.interact(rect, ui.scope_id().with(("more", &m.id)), Sense::click()).on_hover_text("More actions");
    if more.hovered() || menu_open {
        ui.painter().rect_filled(rect.shrink(1.0), CornerRadius::same(8), p.hover);
    }
    for dx in [-6.0, 0.0, 6.0] {
        ui.painter().circle_filled(rect.center() + vec2(dx, 0.0), 1.8, p.dim);
    }
    egui::Popup::menu(&more).id(more_id).show(|ui| {
        ui.set_min_width(200.0);
        message_menu(ui, m, mine, thread, view, &mut action);
    });
    action
}

/// A message's menu, after its quick reactions: replying, copying, reading, and for
/// the user's own, editing and deleting.
fn message_menu(ui: &mut Ui, m: &Message, mine: bool, thread: bool, view: &View, action: &mut Option<Action>) {
    let p = colors();
    let conversation = view.conversation;
    if ui.button("Reply with quote").clicked() {
        *action = Some(Action::Quote(m.clone()));
        ui.close();
    }
    if thread && ui.button("Reply in thread").clicked() {
        *action = Some(Action::OpenThread(m.id.clone()));
        ui.close();
    }
    ui.separator();
    if ui.button("Copy text").clicked() {
        ui.ctx().copy_text(text::flatten(&m.body));
        ui.close();
    }
    if ui.button("Copy link").clicked() {
        let link = format!("https://teams.microsoft.com/l/message/{}/{}", crate::teams::encode(conversation), crate::teams::encode(&m.id));
        ui.ctx().copy_text(link);
        ui.close();
    }
    if let Some(url) = m.card.as_ref().and_then(|c| c.link.clone())
        && ui.button("Open link").clicked()
    {
        ui.ctx().open_url(egui::OpenUrl::new_tab(url));
        ui.close();
    }
    if !mine && ui.button("Mark as unread from here").clicked() {
        *action = Some(Action::UnreadFrom(m.id.clone()));
        ui.close();
    }
    // Who has read it (the user's own, in a chat).
    if let Some((read, unread)) = readers(m, view) {
        ui.separator();
        let title = format!("READ BY {} OF {}", read.len(), read.len() + unread.len());
        ui.add_space(4.0);
        ui.label(RichText::new(title).font(ty::section()).color(p.faint));
        ui.add_space(2.0);
        for (names, seen) in [(&read, true), (&unread, false)] {
            for name in names {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    let (icon, color) = if seen { (Icon::Seen, p.accent) } else { (Icon::Check, p.faint) };
                    widgets::icon(ui, icon, 14.0, color);
                    ui.label(RichText::new(name).font(ty::small()).color(if seen { p.text } else { p.faint }));
                });
            }
        }
        ui.add_space(2.0);
    }
    if mine {
        ui.separator();
        if ui.button("Edit").clicked() {
            *action = Some(Action::Edit(m.clone()));
            ui.close();
        }
        if ui.button(RichText::new("Delete").color(p.danger)).clicked() {
            *action = Some(Action::Delete(m.id.clone()));
            ui.close();
        }
    }
}

/// One message as a bubble. `first` shows who and when inside it; `last` puts the
/// picture beside it and leaves a gap after. Right-click opens its menu.
#[allow(clippy::too_many_arguments)]
pub fn bubble(
    ui: &mut Ui,
    pictures: &mut Pictures,
    m: &Message,
    view: &mut View,
    first: bool,
    last: bool,
    thread: Option<&[&Message]>,
) -> Option<Action> {
    let me = view.me;
    let open = view.open == Some(m.id.as_str());
    let focused = view.focus == Some(m.id.as_str());
    let p = colors();
    let mine = !me.is_empty() && m.author.id == me;
    let avatar = metrics::AVATAR;
    let indent = if mine { 0.0 } else { avatar + 10.0 };
    let max = (ui.available_width() - indent - 40.0).clamp(160.0, metrics::BUBBLE_MAX);
    let (fill, radius) = if mine {
        (p.mine, CornerRadius { nw: 16, ne: 16, sw: 16, se: if last { 4 } else { 16 } })
    } else {
        (if open { p.selected } else { p.surface }, CornerRadius { nw: 16, ne: 16, sw: if last { 4 } else { 16 }, se: 16 })
    };
    // Only a few emoji: big, without the bubble, as Teams shows them.
    let jumbo = m.card.is_none() && m.files.is_empty() && text::only_emoji(&m.body);
    let fill = if jumbo { egui::Color32::TRANSPARENT } else { fill };
    let mut action = None;
    ui.add_space(if first { 8.0 } else { 3.0 });
    let mut clicked = None;
    // What goes inside the bubble: who and when (on the first), the text, a card.
    let mut content = |ui: &mut Ui| {
        ui.spacing_mut().item_spacing.y = 4.0;
        if first {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                if !mine {
                    ui.label(RichText::new(&m.author.name).font(ty::author()).color(p.text));
                }
                let mut stamp = time::clock(m.at);
                if m.edited {
                    stamp.push_str(" · edited");
                }
                ui.label(RichText::new(stamp).font(ty::meta()).color(p.faint));
            });
        }
        widgets::set_jumbo(jumbo);
        if !m.body.is_empty()
            && let Some(click) = widgets::blocks(ui, pictures, &m.body)
        {
            clicked = Some(click);
        }
        widgets::set_jumbo(false);
        if let Some(card) = &m.card {
            widgets::card(ui, card);
        }
        for file in &m.files {
            widgets::file(ui, file);
        }
    };
    // Measure it first (an invisible pass), so the bubble is as wide as its text.
    const PAD_X: f32 = 14.0;
    let probe_rect = Rect::from_min_size(ui.cursor().min, vec2(max - 2.0 * PAD_X, f32::INFINITY));
    let mut probe =
        ui.new_child(egui::UiBuilder::new().max_rect(probe_rect).layout(Layout::top_down(Align::Min)).sizing_pass().invisible());
    content(&mut probe);
    let width = (probe.min_rect().width() + 2.0 * PAD_X).min(max);
    let height = probe.min_rect().height() + 20.0;
    // The bubble's own click area (right-click menu) goes in first, under its text:
    // links and cards inside are added after it and so take their clicks.
    let left = if mine { ui.max_rect().right() - width } else { ui.max_rect().left() + indent };
    let area = Rect::from_min_size(pos2(left, ui.cursor().top()), vec2(width, height));
    let response = ui.interact(area, ui.next_auto_id().with(("bubble", &m.id)), Sense::click());

    let layout = if mine { Layout::right_to_left(Align::Min) } else { Layout::left_to_right(Align::Min) };
    let row = ui.with_layout(layout, |ui| {
        if !mine {
            ui.add_space(indent - ui.spacing().item_spacing.x);
        }
        ui.allocate_ui_with_layout(vec2(width, 0.0), Layout::top_down(Align::Min), |ui| {
            Frame::new()
                .fill(if focused { p.selected } else { fill })
                .stroke(if focused { Stroke::new(1.5, p.accent) } else { Stroke::NONE })
                .corner_radius(radius)
                .inner_margin(Margin::symmetric(PAD_X as i8, 10))
                .show(ui, |ui| {
                    ui.set_width(width - 2.0 * PAD_X);
                    content(ui);
                })
                .response
        })
        .inner
    });
    let frame = row.inner;
    match clicked.take() {
        Some(widgets::Clicked::Quote(id)) => action = Some(Action::Show(id)),
        Some(widgets::Clicked::Image(url)) => action = Some(Action::View(url)),
        None => {}
    }
    if focused && !view.scrolled {
        response.scroll_to_me(Some(Align::Center));
        view.scrolled = true;
    }
    // The author's picture beside the last bubble of a run, level with its bottom.
    if !mine && last {
        let at = Rect::from_min_size(pos2(frame.rect.left() - indent, frame.rect.bottom() - avatar), vec2(avatar, avatar));
        let picture = pictures.get(&Picture::Person { id: m.author.id.clone(), name: m.author.name.clone() });
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(at));
        widgets::avatar(&mut child, &m.author.id, &m.author.name, avatar, picture);
    }
    response.context_menu(|ui| {
        ui.set_min_width(220.0);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            for (key, emoji) in QUICK_REACTIONS {
                let button = match pictures.get(&Picture::Emoji(key.to_string())) {
                    Some(texture) => {
                        egui::Button::image(egui::Image::new((texture, vec2(24.0, 24.0)))).frame(false).min_size(vec2(34.0, 34.0))
                    }
                    None => egui::Button::new(RichText::new(emoji).size(18.0)).frame(false).min_size(vec2(34.0, 34.0)),
                };
                if ui.add(button).on_hover_text(key).clicked() {
                    action = Some(Action::React { message: m.id.clone(), key: key.to_string() });
                    ui.close();
                }
            }
        });
        ui.separator();
        message_menu(ui, m, mine, thread.is_some(), view, &mut action);
    });
    // Over a message: a bar on its top edge with quick reactions, quote and more.
    if let Some(a) = hover_bar(ui, pictures, m, mine, thread.is_some(), view, frame.rect) {
        action = Some(a);
    }

    // Reactions hang off the bubble's bottom edge; the thread link sits right under it.
    if !m.reactions.is_empty() {
        ui.add_space(-ui.spacing().item_spacing.y - 7.0);
        let inset = 12.0;
        if mine {
            ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
                ui.add_space(inset);
                if let Some(key) = widgets::reactions(ui, pictures, &m.reactions, true) {
                    action = Some(Action::React { message: m.id.clone(), key });
                }
            });
        } else {
            ui.horizontal(|ui| {
                ui.add_space(indent + inset - ui.spacing().item_spacing.x);
                if let Some(key) = widgets::reactions(ui, pictures, &m.reactions, false) {
                    action = Some(Action::React { message: m.id.clone(), key });
                }
            });
        }
    }
    if let Some(thread) = thread.filter(|t| !t.is_empty()) {
        ui.horizontal(|ui| {
            ui.add_space(indent - ui.spacing().item_spacing.x);
            if thread_link(ui, pictures, thread) {
                action = Some(Action::OpenThread(m.id.clone()));
            }
        });
    }
    if last {
        ui.add_space(6.0);
    }
    action
}

/// Under the user's newest message in a chat: Sent, or Seen (by how many, in a group).
fn seen(ui: &mut Ui, m: &Message, view: &View) {
    let p = colors();
    let Some((read, unread)) = readers(m, view) else { return };
    let (readers, total) = (read.len(), read.len() + unread.len());
    let (icon, label, color) = match (readers, total) {
        (0, _) => (Icon::Check, "Sent".to_string(), p.faint),
        (_, 1) => (Icon::Seen, "Seen".to_string(), p.accent),
        (r, t) if r == t => (Icon::Seen, "Seen by everyone".to_string(), p.accent),
        (r, t) => (Icon::Seen, format!("Seen by {r} of {t}"), p.accent),
    };
    ui.add_space(-2.0);
    let row = ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
        ui.add_space(4.0);
        ui.spacing_mut().item_spacing.x = 4.0;
        let label = ui.label(RichText::new(label).font(ty::meta()).color(color));
        let icon = widgets::icon(ui, icon, 14.0, color);
        label.rect.union(icon.rect)
    });
    // Who, on hover.
    if total > 1 {
        ui.interact(row.inner, ui.scope_id().with(("seen", &m.id)), Sense::hover()).on_hover_text(readers_text(&read, &unread));
    }
}

/// "3 replies · Last reply 12 min ago" with the repliers' pictures; true when clicked.
fn thread_link(ui: &mut Ui, pictures: &mut Pictures, thread: &[&Message]) -> bool {
    let p = colors();
    ui.add_space(4.0);
    let response = ui
        .horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            let mut seen: Vec<&str> = Vec::new();
            for m in thread {
                if seen.len() < 3 && !seen.contains(&m.author.id.as_str()) {
                    seen.push(&m.author.id);
                    let picture = pictures.get(&Picture::Person { id: m.author.id.clone(), name: m.author.name.clone() });
                    let r = widgets::avatar(ui, &m.author.id, &m.author.name, 20.0, picture);
                    ui.painter().circle_stroke(r.rect.center(), 10.5, Stroke::new(2.0, p.panel));
                    ui.add_space(-5.0);
                }
            }
            ui.add_space(13.0);
            let n = thread.len();
            let label = if n == 1 { "1 reply".to_string() } else { format!("{n} replies") };
            ui.label(RichText::new(label).font(ty::small_strong()).color(p.accent));
            ui.add_space(8.0);
            if let Some(last) = thread.last() {
                ui.label(RichText::new(format!("Last reply {}", ago(last.at))).font(ty::meta()).color(p.faint));
            }
        })
        .response
        .interact(Sense::click())
        .on_hover_cursor(egui::CursorIcon::PointingHand);
    response.clicked()
}

/// "12 min ago", "3 h ago", or the day.
fn ago(at: i64) -> String {
    let now = time::now_ms();
    let minutes = (now - at) / 60_000;
    match minutes {
        ..1 => "just now".into(),
        1..60 => format!("{minutes} min ago"),
        60..1440 => format!("{} h ago", minutes / 60),
        _ => time::day(at, now),
    }
}

/// Typed text laid out with its Markdown shown: bold, italic, struck, code and quotes
/// as they'll be sent, the marks faint.
fn typed(text: &str) -> egui::text::LayoutJob {
    let p = colors();
    let looks = crate::model::markdown::looks(text);
    let mut job = egui::text::LayoutJob::default();
    let mut start = 0;
    while start < text.len() {
        let look = looks[start];
        let mut end = start + 1;
        while end < text.len() && (looks[end] == look || !text.is_char_boundary(end)) {
            end += 1;
        }
        let font = if look.code {
            ty::code()
        } else if look.bold {
            ty::body_strong()
        } else {
            ty::body()
        };
        let color = if look.mark {
            p.faint
        } else if look.quote {
            p.dim
        } else {
            p.text
        };
        let mut format = egui::TextFormat::simple(font, color);
        format.italics = look.italic || look.quote;
        if look.strike {
            format.strikethrough = Stroke::new(1.0, color);
        }
        if look.code && !look.mark {
            format.background = p.code;
        }
        job.append(&text[start..end], 0.0, format);
        start = end;
    }
    job
}

/// The composer: the text, a few tools and the send button in one rounded box, with
/// a "replying to" or "editing" line above when there's one. Enter sends, Shift+Enter
/// breaks the line, ↑ in an empty box edits the last message, Esc cancels, `@` offers
/// people to mention. `reply` is the thread's composer.
/// What's pasted into the composer: text keeps the links it had (from the clipboard's
/// HTML), and a picture (a screenshot, say) is attached.
fn paste(ui: &mut Ui, ws: &mut Workspace, reply: bool) {
    let mut pictures = Vec::new();
    ui.input_mut(|i| {
        i.events.retain_mut(|event| match event {
            egui::Event::Paste(text) => {
                if let Some(html) = arboard::Clipboard::new().ok().and_then(|mut c| c.get().html().ok()) {
                    *text = markdown::with_pasted_links(text, &html);
                }
                true
            }
            egui::Event::PasteImage(image) => {
                pictures.push(image.clone());
                false
            }
            _ => true,
        })
    });
    for image in pictures {
        match encode_png(&image) {
            Some(bytes) => ws.attach(reply, "Pasted picture.png", bytes),
            None => ws.error = Some("Couldn't paste the picture".into()),
        }
    }
}

fn encode_png(image: &egui::ColorImage) -> Option<Vec<u8>> {
    let mut bytes = Vec::new();
    let mut encoder = png::Encoder::new(&mut bytes, image.size[0] as u32, image.size[1] as u32);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let rgba: Vec<u8> = image.pixels.iter().flat_map(|c| c.to_srgba_unmultiplied()).collect();
    encoder.write_header().ok()?.write_image_data(&rgba).ok()?;
    Some(bytes)
}

pub fn composer(ui: &mut Ui, ws: &mut Workspace, rect: Rect, hint: &str, reply: bool) {
    let p = colors();
    let mut rect = rect;
    // The line above: what's quoted, or that a message is being edited.
    let quoted = ws.quoting.as_ref().filter(|(r, _)| *r == reply).map(|(_, m)| m.clone());
    let editing = ws.editing.as_ref().is_some_and(|(r, _)| *r == reply);
    if quoted.is_some() || editing {
        let line = Rect::from_min_max(
            pos2(rect.left() + metrics::PAGE_PAD, rect.top() + 6.0),
            pos2(rect.right() - metrics::PAGE_PAD, rect.top() + BANNER),
        );
        ui.painter().rect_filled(line, CornerRadius::same(12), p.accent_fill.gamma_multiply(0.13));
        let bar = Rect::from_min_size(line.min, vec2(3.0, line.height()));
        ui.painter().rect_filled(bar, CornerRadius { nw: 12, sw: 12, ne: 0, se: 0 }, p.accent);
        let mut b =
            ui.new_child(egui::UiBuilder::new().max_rect(line.shrink2(vec2(14.0, 0.0))).layout(Layout::left_to_right(Align::Center)));
        let mut job = egui::text::LayoutJob::default();
        match &quoted {
            Some(m) => {
                job.append("Replying to ", 0.0, egui::TextFormat::simple(ty::small(), p.dim));
                job.append(&m.author.name, 0.0, egui::TextFormat::simple(ty::small_strong(), p.accent));
                job.append(&format!("  {}", text::flatten(&m.body)), 0.0, egui::TextFormat::simple(ty::small(), p.faint));
            }
            None => {
                job.append("Editing your message", 0.0, egui::TextFormat::simple(ty::small_strong(), p.accent));
                job.append("  Esc to cancel", 0.0, egui::TextFormat::simple(ty::small(), p.faint));
            }
        }
        b.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if widgets::icon_button(ui, Icon::Close, "Cancel (Esc)").clicked() {
                ws.cancel_compose(reply);
            }
            ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                ui.add(egui::Label::new(job).truncate().selectable(false));
            });
        });
        rect.min.y += BANNER - 4.0;
    }

    // Pictures waiting to go, as chips with their name, size and ×.
    if ws.attachments.iter().any(|(r, _)| *r == reply) {
        let line = Rect::from_min_max(
            pos2(rect.left() + metrics::PAGE_PAD, rect.top() + 6.0),
            pos2(rect.right() - metrics::PAGE_PAD, rect.top() + BANNER),
        );
        let mut row = ui.new_child(egui::UiBuilder::new().max_rect(line).layout(Layout::left_to_right(Align::Center)));
        row.spacing_mut().item_spacing.x = 6.0;
        let mut remove = None;
        for (i, (r, a)) in ws.attachments.iter().enumerate() {
            if *r != reply {
                continue;
            }
            Frame::new()
                .fill(p.surface)
                .corner_radius(CornerRadius::same(10))
                .inner_margin(Margin { left: 10, right: 4, top: 4, bottom: 4 })
                .show(&mut row, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 6.0;
                        widgets::icon(ui, Icon::Image, 15.0, p.accent);
                        ui.label(RichText::new(&a.name).font(ty::small_strong()).color(p.text));
                        ui.label(RichText::new(format!("{}×{}", a.size.0, a.size.1)).font(ty::meta()).color(p.faint));
                        if widgets::icon_button(ui, Icon::Close, "Remove").clicked() {
                            remove = Some(i);
                        }
                    });
                });
        }
        if let Some(i) = remove {
            ws.attachments.remove(i);
        }
        rect.min.y += BANNER - 4.0;
    }

    let inner = Rect::from_min_max(
        pos2(rect.left() + metrics::PAGE_PAD, rect.bottom() - BELOW - box_height(ws, reply)),
        pos2(rect.right() - metrics::PAGE_PAD, rect.bottom() - BELOW),
    );
    ui.painter().rect_filled(inner, CornerRadius::same(16), p.surface);
    let sending = ws.sending();
    // The send button and the tools sit at the bottom of the box as it grows.
    let send_rect = Rect::from_min_size(pos2(inner.right() - 8.0 - 40.0, inner.bottom() - 8.0 - 40.0), vec2(40.0, 40.0));
    let tools_w = 3.0 * 32.0;
    // The text, centred while it's one line, then filling the box less its padding.
    let text_h = ws.composer_text[usize::from(reply)];
    let text_top = (inner.center().y - text_h / 2.0).max(inner.top() + TEXT_PAD);
    let text_rect =
        Rect::from_min_max(pos2(inner.left() + 16.0, text_top), pos2(send_rect.left() - tools_w - 10.0, inner.bottom() - TEXT_PAD));

    // Keys the box shouldn't see: ↑ to edit, Esc to cancel, Tab/Enter to pick a person.
    let id = composer_id(reply);
    let focused = ui.memory(|m| m.has_focus(id));
    let people = mention_matches(ws, reply);
    let emoji_found = emoji_matches(ws, reply);
    let mut pick = None;
    let mut pick_emoji = None;
    if focused {
        let empty = ws.draft(reply).trim().is_empty();
        if empty
            && !people.as_ref().is_some_and(|p| !p.is_empty())
            && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp))
        {
            ws.edit_last(reply);
        }
        if ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
            ws.cancel_compose(reply);
        }
        paste(ui, ws, reply);
        if let Some(first) = people.as_ref().and_then(|p| p.first())
            && ui.input_mut(|i| {
                i.consume_key(egui::Modifiers::NONE, egui::Key::Tab) || i.consume_key(egui::Modifiers::NONE, egui::Key::Enter)
            })
        {
            pick = Some(first.clone());
        }
        if let Some(first) = emoji_found.as_ref().and_then(|e| e.first())
            && ui.input_mut(|i| {
                i.consume_key(egui::Modifiers::NONE, egui::Key::Tab) || i.consume_key(egui::Modifiers::NONE, egui::Key::Enter)
            })
        {
            pick_emoji = Some(first.clone());
        }
    }

    let mut t = ui.new_child(egui::UiBuilder::new().max_rect(text_rect).layout(Layout::top_down(Align::Min)));
    // The Markdown shows as it will be sent while it's typed.
    let mut layouter = |ui: &Ui, text: &dyn egui::TextBuffer, wrap: f32| {
        let mut job = typed(text.as_str());
        job.wrap.max_width = wrap;
        ui.fonts_mut(|f| f.layout_job(job))
    };
    let edit = egui::TextEdit::multiline(ws.draft_mut(reply))
        .id(id)
        .hint_text(RichText::new(hint).color(p.faint))
        .font(ty::body())
        .frame(Frame::NONE)
        .margin(Margin::ZERO)
        .desired_rows(1)
        .desired_width(f32::INFINITY)
        .return_key(Some(egui::KeyboardShortcut::new(egui::Modifiers::SHIFT, egui::Key::Enter)))
        .layouter(&mut layouter);
    // Taller than the box can grow: it scrolls, keeping the end (where one types) in view.
    // Its bar shows only then (not for the frame a new line takes to grow the box).
    let tallest = BOX_HIGH - 2.0 * TEXT_PAD;
    let bar = if text_h >= tallest {
        egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded
    } else {
        egui::scroll_area::ScrollBarVisibility::AlwaysHidden
    };
    // No fade at its edges: it darkened the line being typed for the frame a new line
    // takes to grow the box.
    t.spacing_mut().scroll.fade.strength = 0.0;
    let response = egui::ScrollArea::vertical()
        .id_salt(("composer-scroll", reply))
        .scroll_bar_visibility(bar)
        .max_height(tallest)
        .stick_to_bottom(true)
        .show(&mut t, |ui| ui.add(edit))
        .inner;
    let text_height = response.rect.height().round();
    if text_height != ws.composer_text[usize::from(reply)] {
        ws.composer_text[usize::from(reply)] = text_height;
        ui.ctx().request_discard("the composer changed height");
    }
    // The box stays ready to type into: it takes the keys whenever nothing else has them.
    if !reply && ws.find.is_none() && ws.switcher.is_none() && ui.memory(|m| m.focused().is_none()) {
        response.request_focus();
    }
    let enter = response.has_focus() && t.input(|i| i.key_pressed(egui::Key::Enter) && !i.modifiers.shift);
    let has_text = !ws.draft(reply).trim().is_empty() || ws.attachments.iter().any(|(r, _)| *r == reply);

    // People to mention, above the box, while an @word is being typed.
    if let Some(people) = people.filter(|p| !p.is_empty())
        && response.has_focus()
    {
        let row_h = 34.0;
        let h = row_h * people.len() as f32 + 12.0;
        let popup = Rect::from_min_size(pos2(inner.left(), inner.top() - h - 6.0), vec2(280.0, h));
        egui::Area::new(egui::Id::unique(if reply { "mentions-reply" } else { "mentions" }))
            .order(egui::Order::Foreground)
            .fixed_pos(popup.min)
            .show(ui.ctx(), |ui| {
                Frame::new()
                    .fill(p.panel)
                    .stroke(Stroke::new(1.0, p.line))
                    .corner_radius(CornerRadius::same(12))
                    .inner_margin(Margin::same(6))
                    .show(ui, |ui| {
                        ui.set_width(popup.width() - 12.0);
                        ui.spacing_mut().item_spacing.y = 0.0;
                        for (i, person) in people.iter().enumerate() {
                            let (r, click) = ui.allocate_exact_size(vec2(ui.available_width(), row_h), Sense::click());
                            if i == 0 || click.hovered() {
                                ui.painter().rect_filled(r, CornerRadius::same(8), p.hover);
                            }
                            let picture =
                                ws.pictures.get(&crate::model::Picture::Person { id: person.id.clone(), name: person.name.clone() });
                            let mut c = ui.new_child(
                                egui::UiBuilder::new().max_rect(r.shrink2(vec2(8.0, 0.0))).layout(Layout::left_to_right(Align::Center)),
                            );
                            widgets::avatar(&mut c, &person.id, &person.name, 24.0, picture);
                            c.add_space(4.0);
                            c.label(RichText::new(&person.name).font(ty::small_strong()).color(p.text));
                            if click.clicked() {
                                pick = Some(person.clone());
                            }
                        }
                    });
            });
    }
    // Emoji to put in, above the box, while a :name is being typed.
    if let Some(found) = emoji_found.filter(|e| !e.is_empty())
        && response.has_focus()
    {
        let row_h = 34.0;
        let h = row_h * found.len() as f32 + 12.0;
        let popup = Rect::from_min_size(pos2(inner.left(), inner.top() - h - 6.0), vec2(280.0, h));
        egui::Area::new(egui::Id::unique(if reply { "emoji-reply" } else { "emoji" }))
            .order(egui::Order::Foreground)
            .fixed_pos(popup.min)
            .show(ui.ctx(), |ui| {
                Frame::new()
                    .fill(p.panel)
                    .stroke(Stroke::new(1.0, p.line))
                    .corner_radius(CornerRadius::same(12))
                    .inner_margin(Margin::same(6))
                    .show(ui, |ui| {
                        ui.set_width(popup.width() - 12.0);
                        ui.spacing_mut().item_spacing.y = 0.0;
                        for (i, e) in found.iter().enumerate() {
                            let (r, click) = ui.allocate_exact_size(vec2(ui.available_width(), row_h), Sense::click());
                            if i == 0 || click.hovered() {
                                ui.painter().rect_filled(r, CornerRadius::same(8), p.hover);
                            }
                            let face = Rect::from_center_size(pos2(r.left() + 20.0, r.center().y), vec2(22.0, 22.0));
                            match ws.pictures.get(&Picture::Emoji(e.key.clone())) {
                                Some(texture) => egui::Image::new((texture, face.size())).paint_at(ui, face),
                                None => {
                                    let shown = e.ch.unwrap_or("·");
                                    ui.painter().text(
                                        face.center(),
                                        egui::Align2::CENTER_CENTER,
                                        shown,
                                        egui::FontId::proportional(18.0),
                                        p.text,
                                    );
                                }
                            }
                            let name = egui::pos2(face.right() + 10.0, r.center().y);
                            ui.painter().text(name, egui::Align2::LEFT_CENTER, &e.label, ty::small_strong(), p.text);
                            if click.clicked() {
                                pick_emoji = Some(e.clone());
                            }
                        }
                    });
            });
    }
    let picked = pick.is_some() || pick_emoji.is_some();
    if let Some(e) = pick_emoji {
        ws.used_emoji(&e.key);
        let draft = ws.draft_mut(reply);
        if let Some(at) = draft.rfind(':') {
            draft.truncate(at);
        }
        draft.push_str(&e.text);
        draft.push(' ');
        set_cursor_to_end(ui, id, ws.draft(reply));
        response.request_focus();
    }
    if let Some(person) = pick {
        insert_mention(ws.draft_mut(reply), &person.name);
        set_cursor_to_end(ui, id, ws.draft(reply));
        response.request_focus();
    }

    let tools =
        Rect::from_min_max(pos2(send_rect.left() - tools_w - 6.0, send_rect.top()), pos2(send_rect.left() - 6.0, send_rect.bottom()));
    let mut row = ui.new_child(egui::UiBuilder::new().max_rect(tools).layout(Layout::left_to_right(Align::Center)));
    row.spacing_mut().item_spacing.x = 2.0;
    if widgets::icon_button(&mut row, Icon::Clip, "Attach pictures (or drop them on the window)").clicked()
        && let Some(files) =
            rfd::FileDialog::new().set_title("Attach pictures").add_filter("Pictures", &["png", "jpg", "jpeg"]).pick_files()
    {
        for file in files {
            let name = file.file_name().map_or_else(|| "picture".into(), |n| n.to_string_lossy().into_owned());
            match std::fs::read(&file) {
                Ok(bytes) => ws.attach(reply, &name, bytes),
                Err(e) => ws.error = Some(format!("{name}: {e}")),
            }
        }
        response.request_focus();
    }
    let smile = widgets::icon_button(&mut row, Icon::Smile, "Emoji");
    let mut emoji = None;
    let mut picker =
        egui::Popup::menu(&smile).align(egui::RectAlign::TOP_END).gap(10.0).close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside);
    // DRIFT_DEMO_EMOJI: the picker open. (Only then: setting it replaces the click that opens it.)
    if std::env::var_os("DRIFT_DEMO_EMOJI").is_some() && !reply {
        picker = picker.open_memory(egui::SetOpenCommand::Bool(true));
    }
    picker.show(|ui| {
        let recent = ws.recent_emoji.clone().unwrap_or_default();
        let salt = if reply { "reply" } else { "composer" };
        if let Some(e) = emoji_picker::show(ui, &mut ws.pictures, &mut ws.emoji_picker, salt, &ws.custom_emoji, &recent) {
            emoji = Some(e);
            ws.emoji_picker.query.clear();
            ui.close();
        }
    });
    if let Some(picked) = emoji {
        ws.used_emoji(&picked.key);
        // An organisation's emoji goes as :name:, with room around it.
        let draft = ws.draft_mut(reply);
        if picked.text.starts_with(':') && !draft.is_empty() && !draft.ends_with(' ') {
            draft.push(' ');
        }
        draft.push_str(&picked.text);
        set_cursor_to_end(ui, id, ws.draft(reply));
        response.request_focus();
    }
    if widgets::icon_button(&mut row, Icon::At, "Mention someone").clicked() {
        let draft = ws.draft_mut(reply);
        if !draft.is_empty() && !draft.ends_with(' ') {
            draft.push(' ');
        }
        draft.push('@');
        set_cursor_to_end(ui, id, ws.draft(reply));
        response.request_focus();
    }
    if response.has_focus() || has_text {
        let tip = "**bold**  *italic*  ~~strike~~  `code`  > quote  - list";
        ui.painter().text(
            pos2(inner.left() + 16.0, inner.bottom() + BELOW / 2.0 + 1.0),
            egui::Align2::LEFT_CENTER,
            tip,
            ty::meta(),
            p.faint,
        );
    }
    let click = ui.interact(send_rect, ui.scope_id().with(("send", reply)), Sense::click());
    let fill = if has_text && !sending { p.accent_fill } else { p.border };
    ui.painter().rect_filled(send_rect, CornerRadius::same(12), fill);
    let icon = if editing { Icon::Check } else { Icon::Send };
    icons::paint(ui.painter(), Rect::from_center_size(send_rect.center(), vec2(17.0, 17.0)), icon, p.on_accent);
    if (click.clicked() || (enter && !picked)) && has_text && !sending {
        ws.send(ui.ctx(), reply);
        response.request_focus();
    }
}

/// The people whose names start like the `@word` at the end of the draft; None
/// when the draft doesn't end in one.
fn mention_matches(ws: &Workspace, reply: bool) -> Option<Vec<crate::model::markdown::Mention>> {
    let draft = ws.draft(reply);
    let at = draft.rfind('@')?;
    if draft[..at].chars().last().is_some_and(|c| !c.is_whitespace()) {
        return None;
    }
    let typed = &draft[at + 1..];
    if typed.contains('\n') || typed.chars().count() > 30 || typed.contains("  ") {
        return None;
    }
    let typed = typed.to_lowercase();
    let people: Vec<_> = ws
        .people()
        .into_iter()
        .filter(|p| p.name.to_lowercase().starts_with(&typed) || p.name.to_lowercase().split_whitespace().any(|w| w.starts_with(&typed)))
        .take(6)
        .collect();
    Some(people)
}

/// An emoji offered while `:name` is typed: its key, the text it puts in (the
/// character, or `:name:` of the organisation's own), and how it's listed.
#[derive(Clone)]
struct Suggestion {
    key: String,
    text: String,
    label: String,
    ch: Option<&'static str>,
}

/// Emoji for the `:word` at the end of the draft (two letters at least, after a
/// space or at the start, so times like 10:30 aren't taken for one).
fn emoji_matches(ws: &Workspace, reply: bool) -> Option<Vec<Suggestion>> {
    let draft = ws.draft(reply);
    let at = draft.rfind(':')?;
    if draft[..at].chars().last().is_some_and(|c| !c.is_whitespace()) {
        return None;
    }
    let typed = &draft[at + 1..];
    let length = typed.chars().count();
    if !(2..=30).contains(&length) || !typed.chars().all(|c| c.is_alphanumeric() || matches!(c, '_' | '-' | '+')) {
        return None;
    }
    let typed = typed.to_lowercase();
    // The organisation's first: they're what can't be typed any other way.
    let mut found: Vec<Suggestion> = ws
        .custom_emoji
        .iter()
        .filter(|e| e.name.to_lowercase().contains(&typed) || e.keywords.iter().any(|k| k.starts_with(&typed)))
        .map(|e| Suggestion { key: e.key.clone(), text: format!(":{}:", e.name), label: format!(":{}:", e.name), ch: None })
        .collect();
    found.extend(crate::model::emoji::search(&typed).into_iter().map(|e| Suggestion {
        key: e.key.to_string(),
        text: e.ch.to_string(),
        label: e.name.to_string(),
        ch: Some(e.ch),
    }));
    found.truncate(6);
    Some(found)
}

/// Swap the `@word` at the end of the draft for `@Full Name `.
fn insert_mention(draft: &mut String, name: &str) {
    if let Some(at) = draft.rfind('@') {
        draft.truncate(at);
    }
    draft.push('@');
    draft.push_str(name);
    draft.push(' ');
}

fn set_cursor_to_end(ui: &Ui, id: egui::Id, text: &str) {
    if let Some(mut state) = egui::TextEdit::load_state(ui.ctx(), id) {
        let end = egui::text::CCursor::new(text.chars().count());
        state.cursor.set_char_range(Some(egui::text::CCursorRange::one(end)));
        state.store(ui.ctx(), id);
    }
}
