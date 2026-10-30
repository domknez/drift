//! The open conversation: a header, the messages one under another (in a channel,
//! posts with a link to their thread), and the composer at the bottom.

use crate::model::{Directory, Kind, Message, Picture, time};
use crate::ui::pictures::Pictures;
use crate::ui::style::icons::Icon;
use crate::ui::style::{colors, metrics, typography as ty};
use crate::ui::widgets;
use crate::ui::workspace::Workspace;
use eframe::egui::{self, Align, CornerRadius, Frame, Layout, Margin, Rect, RichText, Sense, Stroke, Ui, pos2, vec2};
use std::collections::HashMap;

/// The composer's height, reserved at the bottom.
pub const COMPOSER_HEIGHT: f32 = 104.0;

pub fn show(ui: &mut Ui, ws: &mut Workspace) {
    let p = colors();
    let full = ui.max_rect();
    let header = Rect::from_min_size(full.min, vec2(full.width(), metrics::HEADER));
    super::drag_area(ui, header, "header-drag");

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
    match ws.messages.clone() {
        None => {
            page.add_space(40.0);
            page.vertical_centered(|ui| ui.spinner());
        }
        Some(messages) => {
            egui::ScrollArea::vertical().id_salt(("messages", &id)).stick_to_bottom(true).auto_shrink([false, false]).show(
                &mut page,
                |ui| {
                    ui.add_space(8.0);
                    if messages.is_empty() {
                        ui.add_space(30.0);
                        ui.horizontal(|ui| {
                            ui.add_space(metrics::PAGE_PAD);
                            ui.label(RichText::new("No messages yet.").font(ty::body()).color(p.faint));
                        });
                    } else if is_channel {
                        posts(ui, ws, &messages);
                    } else {
                        flat(ui, &mut ws.pictures, &messages);
                    }
                    ui.add_space(8.0);
                },
            );
        }
    }
    if can_write {
        let rect = Rect::from_min_max(pos2(full.left(), bottom), full.max);
        let name = conversation.as_ref().map_or(String::new(), |c| c.title.clone());
        let hint = if is_channel { format!("Message #{name}") } else { format!("Message {name}") };
        composer(ui, ws, rect, &hint, false);
    }
}

/// The header: the channel's mark and name, where it belongs, and actions.
fn head(ui: &mut Ui, ws: &Workspace, rect: Rect, directory: &Directory, id: &str) {
    let p = colors();
    let mut h = ui.new_child(
        egui::UiBuilder::new().max_rect(rect.shrink2(vec2(metrics::PAGE_PAD, 0.0))).layout(Layout::left_to_right(Align::Center)),
    );
    let conversation = directory.conversation(id);
    let sub = if let Some((team, _)) = directory.channel(id) {
        widgets::icon(&mut h, Icon::Hash, 18.0, p.faint);
        Some(team.name.clone())
    } else {
        match conversation.map(|c| c.kind) {
            Some(Kind::Meeting) => Some("Meeting chat".to_string()),
            Some(Kind::Feed) => Some("Mentions and replies".to_string()),
            _ => None,
        }
    };
    h.add_space(2.0);
    let name = conversation.map_or("", |c| c.title.as_str());
    h.add(egui::Label::new(RichText::new(name).font(ty::title()).color(p.text)).truncate().selectable(false));
    if let Some(sub) = sub {
        h.add_space(4.0);
        h.label(RichText::new(sub).font(ty::small()).color(p.faint));
    }
    let join = conversation.and_then(|c| c.join_url.clone());
    h.with_layout(Layout::right_to_left(Align::Center), |ui| {
        if let Some(url) = &join
            && ghost_button(ui, Icon::Video, "Join").clicked()
        {
            ui.ctx().open_url(egui::OpenUrl::new_tab(url));
        }
        let _ = widgets::icon_button(ui, Icon::Members, "Members");
        let _ = widgets::icon_button(ui, Icon::Search, "Search");
        if let Some(error) = &ws.error {
            ui.add(egui::Label::new(RichText::new(error).font(ty::small()).color(p.danger)).truncate());
        }
    });
}

/// An outlined button with an icon and a label.
fn ghost_button(ui: &mut Ui, icon: Icon, label: &str) -> egui::Response {
    let p = colors();
    let galley = ui.painter().layout_no_wrap(label.to_string(), ty::small_strong(), p.text);
    let (rect, response) = ui.allocate_exact_size(vec2(galley.size().x + 44.0, 30.0), Sense::click());
    let fill = if response.hovered() { p.hover } else { egui::Color32::TRANSPARENT };
    ui.painter().rect(rect, CornerRadius::same(8), fill, Stroke::new(1.0, p.border), egui::StrokeKind::Inside);
    let icon_rect = Rect::from_min_size(pos2(rect.left() + 11.0, rect.center().y - 7.5), vec2(15.0, 15.0));
    crate::ui::style::icons::paint(ui.painter(), icon_rect, icon, p.text);
    ui.painter().galley(pos2(icon_rect.right() + 6.0, rect.center().y - galley.size().y / 2.0), galley, p.text);
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

fn empty(ui: &mut Ui, loaded: bool) {
    let p = colors();
    ui.vertical_centered(|ui| {
        ui.add_space(ui.available_height() / 2.0 - 30.0);
        let text = if loaded { "Pick a chat or a channel." } else { "Loading…" };
        ui.label(RichText::new(text).font(ty::body()).color(p.faint));
    });
}

/// A day divider: the day between two hairlines.
pub fn day_divider(ui: &mut Ui, at: i64) {
    let p = colors();
    ui.add_space(6.0);
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 28.0), Sense::hover());
    let label = time::day(at, time::now_ms());
    let galley = ui.painter().layout_no_wrap(label, ty::small_strong(), p.faint);
    let inner = rect.shrink2(vec2(metrics::PAGE_PAD, 0.0));
    let gap = galley.size().x / 2.0 + 12.0;
    let y = rect.center().y;
    ui.painter().hline(inner.left()..=rect.center().x - gap, y, Stroke::new(1.0, p.line));
    ui.painter().hline(rect.center().x + gap..=inner.right(), y, Stroke::new(1.0, p.line));
    ui.painter().galley(rect.center() - galley.size() / 2.0, galley, p.faint);
}

/// A channel: its posts, each with a link to its thread.
fn posts(ui: &mut Ui, ws: &mut Workspace, messages: &[Message]) {
    let ids: std::collections::HashSet<&str> = messages.iter().map(|m| m.id.as_str()).collect();
    let mut replies: HashMap<&str, Vec<&Message>> = HashMap::new();
    let mut roots = Vec::new();
    for m in messages {
        match m.root.as_deref().filter(|r| ids.contains(r)) {
            Some(root) => replies.entry(root).or_default().push(m),
            None => roots.push(m),
        }
    }
    let mut day = None;
    let mut previous: Option<&Message> = None;
    for post in roots {
        if day != Some(time::day_number(post.at)) {
            day = Some(time::day_number(post.at));
            day_divider(ui, post.at);
            previous = None;
        }
        let thread = replies.get(post.id.as_str()).map(Vec::as_slice).unwrap_or(&[]);
        let grouped = previous.is_some_and(|prev| continues(prev, post));
        let open = ws.thread.as_deref() == Some(post.id.as_str());
        if message(ui, &mut ws.pictures, post, !grouped, Some(thread), open) {
            ws.thread = Some(post.id.clone());
            ws.reply_draft.clear();
        }
        previous = if thread.is_empty() { Some(post) } else { None };
    }
}

/// A chat: messages one under another, a header only where the author or the
/// moment changes.
fn flat(ui: &mut Ui, pictures: &mut Pictures, messages: &[Message]) {
    let mut previous: Option<&Message> = None;
    for m in messages {
        if previous.is_none_or(|prev| time::day_number(prev.at) != time::day_number(m.at)) {
            day_divider(ui, m.at);
            previous = None;
        }
        let grouped = previous.is_some_and(|prev| continues(prev, m));
        message(ui, pictures, m, !grouped, None, false);
        previous = Some(m);
    }
}

fn continues(prev: &Message, m: &Message) -> bool {
    prev.author.id == m.author.id && m.at - prev.at < metrics::GROUP_GAP
}

/// One message, Slack-like: avatar, author and time (unless it continues the one
/// above), text, card, reactions, and in a channel the thread link. Hovering lights
/// the row. True when the thread link was clicked.
pub fn message(ui: &mut Ui, pictures: &mut Pictures, m: &Message, header: bool, thread: Option<&[&Message]>, open: bool) -> bool {
    let p = colors();
    let mut clicked = false;
    let backdrop = ui.painter().add(egui::Shape::Noop);
    let response = Frame::new()
        .inner_margin(Margin { left: metrics::PAGE_PAD as i8, right: metrics::PAGE_PAD as i8, top: if header { 8 } else { 2 }, bottom: 4 })
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_top(|ui| {
                if header {
                    let picture = pictures.get(&Picture::Person { id: m.author.id.clone(), name: m.author.name.clone() });
                    widgets::avatar(ui, &m.author.id, &m.author.name, metrics::AVATAR, picture);
                } else {
                    ui.add_space(metrics::AVATAR);
                }
                ui.add_space(4.0);
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 4.0;
                    if header {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 8.0;
                            ui.label(RichText::new(&m.author.name).font(ty::author()).color(p.text));
                            let mut stamp = time::clock(m.at);
                            if m.edited {
                                stamp.push_str(" (edited)");
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
                    widgets::reactions(ui, &m.reactions);
                    if let Some(thread) = thread.filter(|t| !t.is_empty())
                        && thread_link(ui, pictures, thread)
                    {
                        clicked = true;
                    }
                });
            });
        })
        .response;
    let row = response.rect;
    if open || ui.rect_contains_pointer(row) {
        let fill = if open { p.selected } else { p.hover };
        ui.painter().set(backdrop, egui::Shape::rect_filled(row, CornerRadius::ZERO, fill));
    }
    clicked
}

/// "3 replies · Last reply 12 min ago" with the repliers' avatars; true when clicked.
fn thread_link(ui: &mut Ui, pictures: &mut Pictures, thread: &[&Message]) -> bool {
    let p = colors();
    ui.add_space(2.0);
    let response = ui
        .horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            let mut seen: Vec<&str> = Vec::new();
            for m in thread {
                if seen.len() < 3 && !seen.contains(&m.author.id.as_str()) {
                    seen.push(&m.author.id);
                    let picture = pictures.get(&Picture::Person { id: m.author.id.clone(), name: m.author.name.clone() });
                    let r = widgets::avatar(ui, &m.author.id, &m.author.name, 20.0, picture);
                    ui.painter().circle_stroke(r.rect.center(), 10.5, Stroke::new(2.0, p.bg));
                    ui.add_space(-5.0);
                }
            }
            ui.add_space(13.0);
            let n = thread.len();
            let text = if n == 1 { "1 reply".to_string() } else { format!("{n} replies") };
            ui.label(RichText::new(text).font(ty::small_strong()).color(p.accent));
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

/// The composer: a box with the text, a row of tools and the send button. `reply`
/// sends to the open thread.
pub fn composer(ui: &mut Ui, ws: &mut Workspace, rect: Rect, hint: &str, reply: bool) {
    let p = colors();
    let inner = Rect::from_min_max(
        pos2(rect.left() + metrics::PAGE_PAD, rect.top() + 6.0),
        pos2(rect.right() - metrics::PAGE_PAD, rect.bottom() - 18.0),
    );
    ui.painter().rect(inner, CornerRadius::same(12), p.surface, Stroke::new(1.0, p.border), egui::StrokeKind::Inside);
    let sending = ws.sending();
    let text_rect = Rect::from_min_max(inner.min + vec2(14.0, 11.0), pos2(inner.right() - 14.0, inner.bottom() - 40.0));
    let mut t = ui.new_child(egui::UiBuilder::new().max_rect(text_rect));
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

    let tools = Rect::from_min_max(pos2(inner.left() + 8.0, inner.bottom() - 38.0), pos2(inner.right() - 8.0, inner.bottom() - 6.0));
    let mut row = ui.new_child(egui::UiBuilder::new().max_rect(tools).layout(Layout::left_to_right(Align::Center)));
    row.spacing_mut().item_spacing.x = 2.0;
    for (icon, hint) in [
        (Icon::Bold, "Formatting (coming soon)"),
        (Icon::Smile, "Emoji (coming soon)"),
        (Icon::Clip, "Attach (coming soon)"),
        (Icon::At, "Mention (coming soon)"),
    ] {
        let _ = widgets::icon_button(&mut row, icon, hint);
    }
    let mut send = false;
    row.with_layout(Layout::right_to_left(Align::Center), |ui| {
        let (r, click) = ui.allocate_exact_size(vec2(32.0, 32.0), Sense::click());
        let fill = if has_text && !sending { p.accent_fill } else { p.border };
        ui.painter().rect_filled(r, CornerRadius::same(8), fill);
        crate::ui::style::icons::paint(ui.painter(), Rect::from_center_size(r.center(), vec2(16.0, 16.0)), Icon::Send, p.on_accent);
        send = click.clicked();
    });
    if (send || enter) && has_text && !sending {
        ws.send(ui.ctx(), reply);
        response.request_focus();
    }
}
