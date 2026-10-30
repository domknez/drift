//! The open conversation: a header, then the messages. Channels show posts as
//! cards with their replies folded in under them; chats show messages in a row,
//! grouped by author.

use crate::model::{Directory, Kind, Message, time};
use crate::ui::style::{colors, metrics, typography as ty};
use crate::ui::widgets;
use crate::ui::workspace::Workspace;
use eframe::egui::{self, Align, CornerRadius, Frame, Layout, Margin, Rect, RichText, Sense, Stroke, Ui, vec2};
use std::collections::HashMap;

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
    let is_channel = directory.channel(&id).is_some() || directory.conversation(&id).is_some_and(|c| c.kind == Kind::Channel);

    // Header.
    let mut head = ui.new_child(
        egui::UiBuilder::new().max_rect(header.shrink2(vec2(metrics::PAGE_PAD, 0.0))).layout(Layout::left_to_right(Align::Center)),
    );
    title(&mut head, &directory, &id);
    if let Some(error) = &ws.error {
        head.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.add(egui::Label::new(RichText::new(error).font(ty::small()).color(p.danger)).truncate());
        });
    }
    ui.painter().hline(full.x_range(), header.bottom(), Stroke::new(1.0, p.line));

    let body = Rect::from_min_max(header.left_bottom() + vec2(0.0, 1.0), full.max);
    let mut page = ui.new_child(egui::UiBuilder::new().max_rect(body).layout(Layout::top_down(Align::Min)));
    let Some(messages) = ws.messages.clone() else {
        page.add_space(40.0);
        page.vertical_centered(|ui| ui.spinner());
        return;
    };
    egui::ScrollArea::vertical().id_salt(("messages", &id)).stick_to_bottom(true).auto_shrink([false, false]).show(&mut page, |ui| {
        let width = ui.available_width().min(metrics::READING_WIDTH + 2.0 * metrics::PAGE_PAD);
        let side = ((ui.available_width() - width) / 2.0).max(0.0);
        ui.horizontal(|ui| {
            ui.add_space(side);
            ui.vertical(|ui| {
                ui.set_width(width);
                Frame::new()
                    .inner_margin(Margin { left: metrics::PAGE_PAD as i8, right: metrics::PAGE_PAD as i8, top: 8, bottom: 24 })
                    .show(ui, |ui| {
                        if messages.is_empty() {
                            ui.add_space(30.0);
                            ui.label(RichText::new("No messages yet.").font(ty::body()).color(p.faint));
                        } else if is_channel {
                            posts(ui, ws, &messages);
                        } else {
                            chat(ui, &messages);
                        }
                    });
            });
        });
    });
}

fn title(ui: &mut Ui, directory: &Directory, id: &str) {
    let p = colors();
    if let Some((team, channel)) = directory.channel(id) {
        ui.label(RichText::new(format!("{} /", team.name)).font(ty::title()).color(p.faint));
        ui.label(RichText::new(format!("# {}", channel.name)).font(ty::title()).color(p.text));
    } else {
        let name = directory.conversation(id).map_or("", |c| c.title.as_str());
        ui.add(egui::Label::new(RichText::new(name).font(ty::title()).color(p.text)).truncate());
    }
}

fn empty(ui: &mut Ui, loaded: bool) {
    let p = colors();
    ui.vertical_centered(|ui| {
        ui.add_space(ui.available_height() / 2.0 - 30.0);
        let text = if loaded { "Pick a chat or a channel on the left." } else { "Loading…" };
        ui.label(RichText::new(text).font(ty::body()).color(p.faint));
    });
}

/// A channel: posts as cards, each with its replies under it.
fn posts(ui: &mut Ui, ws: &mut Workspace, messages: &[Message]) {
    let p = colors();
    let ids: std::collections::HashSet<&str> = messages.iter().map(|m| m.id.as_str()).collect();
    let mut replies: HashMap<&str, Vec<&Message>> = HashMap::new();
    let mut roots = Vec::new();
    for m in messages {
        match m.root.as_deref().filter(|r| ids.contains(r)) {
            Some(root) => replies.entry(root).or_default().push(m),
            None => roots.push(m),
        }
    }
    let now = time::now_ms();
    let mut day = None;
    for post in roots {
        let d = time::day_number(post.at);
        if day != Some(d) {
            day = Some(d);
            ui.add_space(14.0);
            ui.label(RichText::new(time::day(post.at, now).to_uppercase()).font(ty::meta()).color(p.faint));
            ui.add_space(4.0);
        }
        Frame::new()
            .fill(p.surface)
            .stroke(Stroke::new(1.0, p.line))
            .corner_radius(CornerRadius::same(metrics::POST_RADIUS))
            .inner_margin(Margin::symmetric(16, 14))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                message(ui, post, metrics::AVATAR, true);
                let thread = replies.get(post.id.as_str()).map(Vec::as_slice).unwrap_or(&[]);
                if !thread.is_empty() {
                    ui.add_space(10.0);
                    let (line, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
                    ui.painter().hline(line.x_range(), line.center().y, Stroke::new(1.0, p.line));
                    ui.add_space(8.0);
                    let unfolded = ws.unfolded_threads.contains(&post.id);
                    let hidden = if unfolded { 0 } else { thread.len().saturating_sub(metrics::REPLIES_SHOWN) };
                    if hidden > 0 {
                        let text = if hidden == 1 { "Show 1 earlier reply".to_string() } else { format!("Show {hidden} earlier replies") };
                        let link =
                            ui.add(egui::Label::new(RichText::new(text).font(ty::small_strong()).color(p.accent)).sense(Sense::click()));
                        if link.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                            ws.unfolded_threads.insert(post.id.clone());
                        }
                        ui.add_space(4.0);
                    }
                    for reply in &thread[hidden..] {
                        ui.add_space(6.0);
                        message(ui, reply, metrics::REPLY_AVATAR, true);
                    }
                }
            });
        ui.add_space(8.0);
    }
}

/// A chat: messages in a row, a header only where the author or the hour changes.
fn chat(ui: &mut Ui, messages: &[Message]) {
    let p = colors();
    let now = time::now_ms();
    let mut previous: Option<&Message> = None;
    for m in messages {
        let new_day = previous.is_none_or(|prev| time::day_number(prev.at) != time::day_number(m.at));
        if new_day {
            ui.add_space(14.0);
            ui.label(RichText::new(time::day(m.at, now).to_uppercase()).font(ty::meta()).color(p.faint));
        }
        let grouped = !new_day && previous.is_some_and(|prev| prev.author.id == m.author.id && m.at - prev.at < metrics::GROUP_GAP);
        ui.add_space(if grouped { 2.0 } else { 10.0 });
        message(ui, m, metrics::AVATAR, !grouped);
        previous = Some(m);
    }
}

/// One message: avatar, author and time (unless it continues the one above), text,
/// card and reactions.
fn message(ui: &mut Ui, m: &Message, avatar: f32, header: bool) {
    let p = colors();
    ui.horizontal_top(|ui| {
        if header {
            widgets::avatar(ui, &m.author.id, &m.author.name, avatar);
        } else {
            ui.add_space(avatar);
        }
        ui.add_space(4.0);
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 4.0;
            if header {
                ui.horizontal(|ui| {
                    let font = if avatar < metrics::AVATAR { ty::small_strong() } else { ty::author() };
                    ui.label(RichText::new(&m.author.name).font(font).color(p.text));
                    let mut stamp = time::clock(m.at);
                    if m.edited {
                        stamp.push_str(" · edited");
                    }
                    ui.label(RichText::new(stamp).font(ty::meta()).color(p.faint));
                });
            }
            if !m.body.is_empty() {
                widgets::rich_text(ui, &m.body, p.text);
            }
            if let Some(card) = &m.card {
                widgets::card(ui, card);
            }
            widgets::reactions(ui, &m.reactions);
        });
    });
}
