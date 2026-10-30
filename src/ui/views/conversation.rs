//! The open conversation: a header, the messages as bubbles (others' on the left
//! with their picture, the user's own on the right), and the composer. In a channel
//! a post links to its thread. Right-clicking a message reacts, replies or copies.

use crate::model::{Directory, Kind, Message, Picture, text, time};
use crate::ui::pictures::Pictures;
use crate::ui::style::icons::{self, Icon};
use crate::ui::style::{colors, metrics, typography as ty};
use crate::ui::widgets;
use crate::ui::workspace::Workspace;
use eframe::egui::{self, Align, CornerRadius, Frame, Layout, Margin, Rect, RichText, Sense, Stroke, Ui, pos2, vec2};
use std::collections::HashMap;

/// The composer's height, reserved at the bottom.
pub const COMPOSER_HEIGHT: f32 = 80.0;
/// The reactions offered on right-click.
const QUICK_REACTIONS: [(&str, &str); 6] =
    [("like", "👍"), ("heart", "❤"), ("laugh", "😆"), ("surprised", "😮"), ("sad", "😢"), ("pray", "🙏")];

/// What a click on a message asks for.
pub enum Action {
    OpenThread(String),
    React { message: String, key: String },
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

    let bottom = if can_write { full.bottom() - COMPOSER_HEIGHT } else { full.bottom() };
    let body = Rect::from_min_max(pos2(full.left(), header.bottom()), pos2(full.right(), bottom));
    let mut page = ui.new_child(egui::UiBuilder::new().max_rect(body).layout(Layout::top_down(Align::Min)));
    let mut action = None;
    match ws.messages.clone() {
        None => {
            page.add_space(40.0);
            page.vertical_centered(|ui| ui.spinner());
        }
        Some(messages) => {
            let me = ws.me_id.clone();
            let open_thread = ws.thread.clone();
            egui::ScrollArea::vertical().id_salt(("messages", &id)).stick_to_bottom(true).auto_shrink([false, false]).show(
                &mut page,
                |ui| {
                    Frame::new().inner_margin(Margin::symmetric(metrics::PAGE_PAD as i8, 10)).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        if messages.is_empty() {
                            ui.add_space(30.0);
                            ui.vertical_centered(|ui| {
                                ui.label(RichText::new("No messages yet. Say hello.").font(ty::body()).color(p.faint))
                            });
                        } else {
                            action = list(ui, &mut ws.pictures, &messages, &me, is_channel, open_thread.as_deref());
                        }
                    });
                },
            );
        }
    }
    match action {
        Some(Action::OpenThread(root)) => {
            ws.thread = Some(root);
            ws.reply_draft.clear();
        }
        Some(Action::React { message, key }) => ws.react(ui.ctx(), &message, &key),
        None => {}
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
        && square(ui, Icon::Video, "Join the meeting").clicked()
    {
        ui.ctx().open_url(egui::OpenUrl::new_tab(url));
    }
    let _ = square(ui, Icon::Members, "Members");
    let _ = square(ui, Icon::Search, "Search");
    if let Some(error) = &ws.error {
        let r = Rect::from_min_max(pos2(x - 260.0, rect.top()), pos2(x, rect.bottom()));
        let mut e = ui.new_child(egui::UiBuilder::new().max_rect(r).layout(Layout::right_to_left(Align::Center)));
        e.add(egui::Label::new(RichText::new(error).font(ty::small()).color(p.danger)).truncate());
    }
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

/// The messages: in a channel its posts (replies are in their threads), in a chat
/// all of them; grouped by author into runs of bubbles.
pub fn list(ui: &mut Ui, pictures: &mut Pictures, messages: &[Message], me: &str, channel: bool, open: Option<&str>) -> Option<Action> {
    let ids: std::collections::HashSet<&str> = messages.iter().map(|m| m.id.as_str()).collect();
    let mut replies: HashMap<&str, Vec<&Message>> = HashMap::new();
    let mut shown = Vec::new();
    for m in messages {
        match m.root.as_deref().filter(|r| channel && ids.contains(r)) {
            Some(root) => replies.entry(root).or_default().push(m),
            None => shown.push(m),
        }
    }
    let mut action = None;
    for (i, m) in shown.iter().enumerate() {
        let prev = i.checked_sub(1).map(|j| shown[j]);
        let next = shown.get(i + 1).copied();
        let thread = replies.get(m.id.as_str()).map(Vec::as_slice).unwrap_or(&[]);
        let new_day = prev.is_none_or(|prev| time::day_number(prev.at) != time::day_number(m.at));
        if new_day {
            day_pill(ui, m.at);
        }
        let prev_thread = prev.is_some_and(|prev| replies.contains_key(prev.id.as_str()));
        let first = new_day || prev_thread || !prev.is_some_and(|prev| continues(prev, m));
        let last = !thread.is_empty() || !next.is_some_and(|n| continues(m, n) && time::day_number(n.at) == time::day_number(m.at));
        let threaded = channel.then_some(thread);
        if let Some(a) = bubble(ui, pictures, m, me, first, last, threaded, open == Some(m.id.as_str())) {
            action = Some(a);
        }
    }
    action
}

fn continues(prev: &Message, m: &Message) -> bool {
    prev.author.id == m.author.id && m.at - prev.at < metrics::GROUP_GAP
}

/// One message as a bubble. `first` shows who and when inside it; `last` puts the
/// picture beside it and leaves a gap after. Right-click opens its menu.
#[allow(clippy::too_many_arguments)]
pub fn bubble(
    ui: &mut Ui,
    pictures: &mut Pictures,
    m: &Message,
    me: &str,
    first: bool,
    last: bool,
    thread: Option<&[&Message]>,
    open: bool,
) -> Option<Action> {
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
    let mut action = None;
    ui.add_space(if first { 8.0 } else { 3.0 });
    // What goes inside the bubble: who and when (on the first), the text, a card.
    let content = |ui: &mut Ui| {
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
        if !m.body.is_empty() {
            widgets::blocks(ui, &m.body);
        }
        if let Some(card) = &m.card {
            widgets::card(ui, card);
        }
    };
    // Measure it first (an invisible pass), so the bubble is as wide as its text.
    const PAD_X: f32 = 14.0;
    let probe_rect = Rect::from_min_size(ui.cursor().min, vec2(max - 2.0 * PAD_X, f32::INFINITY));
    let mut probe =
        ui.new_child(egui::UiBuilder::new().max_rect(probe_rect).layout(Layout::top_down(Align::Min)).sizing_pass().invisible());
    content(&mut probe);
    let width = (probe.min_rect().width() + 2.0 * PAD_X).min(max);

    let layout = if mine { Layout::right_to_left(Align::Min) } else { Layout::left_to_right(Align::Min) };
    let row = ui.with_layout(layout, |ui| {
        if !mine {
            ui.add_space(indent - ui.spacing().item_spacing.x);
        }
        ui.allocate_ui_with_layout(vec2(width, 0.0), Layout::top_down(Align::Min), |ui| {
            Frame::new()
                .fill(fill)
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
    let response = row.inner.interact(Sense::click());
    // The author's picture beside the last bubble of a run, level with its bottom.
    if !mine && last {
        let at = Rect::from_min_size(pos2(response.rect.left() - indent, response.rect.bottom() - avatar), vec2(avatar, avatar));
        let picture = pictures.get(&Picture::Person { id: m.author.id.clone(), name: m.author.name.clone() });
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(at));
        widgets::avatar(&mut child, &m.author.id, &m.author.name, avatar, picture);
    }
    response.context_menu(|ui| {
        ui.set_min_width(220.0);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            for (key, emoji) in QUICK_REACTIONS {
                let button = egui::Button::new(RichText::new(emoji).size(18.0)).frame(false).min_size(vec2(32.0, 32.0));
                if ui.add(button).on_hover_text(key).clicked() {
                    action = Some(Action::React { message: m.id.clone(), key: key.to_string() });
                    ui.close();
                }
            }
        });
        ui.separator();
        if thread.is_some() && ui.button("Reply in thread").clicked() {
            action = Some(Action::OpenThread(m.id.clone()));
            ui.close();
        }
        if ui.button("Copy text").clicked() {
            ui.ctx().copy_text(text::flatten(&m.body));
            ui.close();
        }
        if let Some(url) = m.card.as_ref().and_then(|c| c.link.clone())
            && ui.button("Open link").clicked()
        {
            ui.ctx().open_url(egui::OpenUrl::new_tab(url));
            ui.close();
        }
    });

    // Reactions and the thread link, under the bubble.
    let has_thread = thread.is_some_and(|t| !t.is_empty());
    if !m.reactions.is_empty() || has_thread {
        if mine {
            ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
                if let Some(key) = widgets::reactions(ui, &m.reactions, true) {
                    action = Some(Action::React { message: m.id.clone(), key });
                }
            });
        } else {
            ui.horizontal(|ui| {
                ui.add_space(indent - ui.spacing().item_spacing.x);
                ui.vertical(|ui| {
                    if let Some(key) = widgets::reactions(ui, &m.reactions, false) {
                        action = Some(Action::React { message: m.id.clone(), key });
                    }
                    if let Some(thread) = thread.filter(|t| !t.is_empty())
                        && thread_link(ui, pictures, thread)
                    {
                        action = Some(Action::OpenThread(m.id.clone()));
                    }
                });
            });
        }
    }
    if last {
        ui.add_space(6.0);
    }
    action
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

/// The composer: the text, a few tools and the send button in one rounded box.
/// `reply` sends to the open thread.
pub fn composer(ui: &mut Ui, ws: &mut Workspace, rect: Rect, hint: &str, reply: bool) {
    let p = colors();
    let inner = Rect::from_min_max(
        pos2(rect.left() + metrics::PAGE_PAD, rect.top() + 8.0),
        pos2(rect.right() - metrics::PAGE_PAD, rect.bottom() - 16.0),
    );
    ui.painter().rect_filled(inner, CornerRadius::same(16), p.surface);
    let sending = ws.sending();
    let send_rect = Rect::from_min_size(pos2(inner.right() - 8.0 - 40.0, inner.center().y - 20.0), vec2(40.0, 40.0));
    let tools_w = 3.0 * 32.0;
    let text_rect =
        Rect::from_min_max(pos2(inner.left() + 16.0, inner.top() + 6.0), pos2(send_rect.left() - tools_w - 10.0, inner.bottom() - 6.0));
    let mut t = ui.new_child(egui::UiBuilder::new().max_rect(text_rect).layout(Layout::left_to_right(Align::Center)));
    let draft = if reply { &mut ws.reply_draft } else { &mut ws.draft };
    let edit = egui::TextEdit::multiline(draft)
        .id_salt(if reply { "reply" } else { "compose" })
        .hint_text(RichText::new(hint).color(p.faint))
        .font(ty::body())
        .frame(Frame::NONE)
        .margin(Margin::ZERO)
        .desired_rows(1)
        .desired_width(f32::INFINITY)
        .return_key(Some(egui::KeyboardShortcut::new(egui::Modifiers::SHIFT, egui::Key::Enter)));
    let response = t.add_enabled(!sending, edit);
    let enter = response.has_focus() && t.input(|i| i.key_pressed(egui::Key::Enter) && !i.modifiers.shift);
    let has_text = !draft.trim().is_empty();

    let tools = Rect::from_min_max(pos2(send_rect.left() - tools_w - 6.0, inner.top()), pos2(send_rect.left() - 6.0, inner.bottom()));
    let mut row = ui.new_child(egui::UiBuilder::new().max_rect(tools).layout(Layout::left_to_right(Align::Center)));
    row.spacing_mut().item_spacing.x = 2.0;
    for (icon, tip) in [(Icon::Smile, "Emoji (coming soon)"), (Icon::Clip, "Attach (coming soon)"), (Icon::At, "Mention (coming soon)")] {
        let _ = widgets::icon_button(&mut row, icon, tip);
    }
    let click = ui.interact(send_rect, ui.scope_id().with(("send", reply)), Sense::click());
    let fill = if has_text && !sending { p.accent_fill } else { p.border };
    ui.painter().rect_filled(send_rect, CornerRadius::same(12), fill);
    icons::paint(ui.painter(), Rect::from_center_size(send_rect.center(), vec2(17.0, 17.0)), Icon::Send, p.on_accent);
    if (click.clicked() || enter) && has_text && !sending {
        ws.send(ui.ctx(), reply);
        response.request_focus();
    }
}
