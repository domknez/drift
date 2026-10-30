//! The open conversation: a header, then the messages. Channels show posts as
//! cards with their replies folded in under them; chats show messages in a row,
//! grouped by author.

use crate::model::{Directory, Kind, Message, Picture, time};
use crate::ui::style::icons::Icon;
use crate::ui::pictures::Pictures;
use crate::ui::style::{colors, metrics, typography as ty};
use crate::ui::widgets;
use crate::ui::workspace::Workspace;
use eframe::egui::{self, Align, CornerRadius, Frame, Layout, Margin, Rect, RichText, Sense, Stroke, Ui, vec2};

/// Height reserved for the composer at the bottom.
const COMPOSER_HEIGHT: f32 = 60.0;
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
    let join_url = directory.conversation(&id).and_then(|c| c.join_url.clone());
    head.with_layout(Layout::right_to_left(Align::Center), |ui| {
        let _ = widgets::icon_button(ui, Icon::More, "More");
        if let Some(url) = &join_url
            && widgets::icon_button(ui, Icon::Video, "Join the meeting").clicked()
        {
            ui.ctx().open_url(egui::OpenUrl::new_tab(url));
        }
        let _ = widgets::icon_button(ui, Icon::Members, "Members");
        let _ = widgets::icon_button(ui, Icon::Search, "Search this conversation");
        if let Some(error) = &ws.error {
            ui.add_space(6.0);
            ui.add(egui::Label::new(RichText::new(error).font(ty::small()).color(p.danger)).truncate());
        }
    });
    ui.painter().hline(full.x_range(), header.bottom(), Stroke::new(1.0, p.line));

    let kind = directory.conversation(&id).map(|c| c.kind);
    let can_write = !matches!(kind, Some(Kind::Feed));
    let composer_h = if can_write { COMPOSER_HEIGHT } else { 0.0 };
    let body = Rect::from_min_max(header.left_bottom() + vec2(0.0, 1.0), egui::pos2(full.right(), full.bottom() - composer_h));
    let mut page = ui.new_child(egui::UiBuilder::new().max_rect(body).layout(Layout::top_down(Align::Min)));
    let Some(messages) = ws.messages.clone() else {
        page.add_space(40.0);
        page.vertical_centered(|ui| ui.spinner());
        if can_write {
            composer(ui, ws, full, &directory, &id, is_channel);
        }
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
                            chat(ui, &mut ws.pictures, &messages, &ws.me);
                        }
                    });
            });
        });
    });
    if can_write {
        composer(ui, ws, full, &directory, &id, is_channel);
    }
}

/// The box to type a message in, pinned to the bottom of the conversation.
fn composer(ui: &mut Ui, ws: &mut Workspace, full: Rect, directory: &Directory, id: &str, is_channel: bool) {
    let p = colors();
    let rect = Rect::from_min_max(egui::pos2(full.left(), full.bottom() - COMPOSER_HEIGHT), full.max);
    let width = rect.width().min(metrics::READING_WIDTH + 2.0 * metrics::PAGE_PAD);
    let side = ((rect.width() - width) / 2.0).max(0.0);
    let inner = Rect::from_min_max(egui::pos2(rect.left() + side + metrics::PAGE_PAD, rect.top() + 8.0), egui::pos2(rect.right() - side - metrics::PAGE_PAD, rect.bottom() - 16.0));
    let mut c = ui.new_child(egui::UiBuilder::new().max_rect(inner).layout(Layout::left_to_right(Align::Center)));
    let name = directory.conversation(id).map(|c| c.title.as_str()).unwrap_or("");
    let hint = if is_channel { format!("Start a new post in #{name}") } else { format!("Message {name}") };
    Frame::new()
        .fill(p.surface)
        .stroke(Stroke::new(1.0, p.border))
        .corner_radius(CornerRadius::same(14))
        .inner_margin(Margin { left: 16, right: 8, top: 6, bottom: 6 })
        .show(&mut c, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_centered(|ui| {
                let sending = ws.sending();
                let send_w = 38.0;
                let edit = egui::TextEdit::multiline(&mut ws.draft)
                    .hint_text(RichText::new(hint).color(p.faint))
                    .font(ty::body())
                    .frame(egui::Frame::NONE)
                    .desired_rows(1)
                    .desired_width(ui.available_width() - send_w)
                    .return_key(Some(egui::KeyboardShortcut::new(egui::Modifiers::SHIFT, egui::Key::Enter)));
                let response = ui.add_enabled(!sending, edit);
                let submit = response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter) && !i.modifiers.shift);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let has_text = !ws.draft.trim().is_empty();
                    let (rect, click) = ui.allocate_exact_size(vec2(34.0, 34.0), egui::Sense::click());
                    let color = if has_text && !sending { p.accent_fill } else { p.border };
                    ui.painter().rect_filled(rect, CornerRadius::same(10), color);
                    super::super::style::icons::paint(ui.painter(), Rect::from_center_size(rect.center(), vec2(17.0, 17.0)), Icon::Send, p.on_accent);
                    if (click.clicked() || submit) && has_text && !sending {
                        ws.send(ui.ctx());
                        response.request_focus();
                    }
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
                message(ui, &mut ws.pictures, post, metrics::AVATAR, true);
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
                        message(ui, &mut ws.pictures, reply, metrics::REPLY_AVATAR, true);
                    }
                }
            });
        ui.add_space(8.0);
    }
}

/// A chat: messages in a row, a header only where the author or the hour changes.
fn chat(ui: &mut Ui, pictures: &mut Pictures, messages: &[Message], me: &str) {
    let p = colors();
    let now = time::now_ms();
    // Runs of messages by one author, close together on one day, share a card.
    let mut runs: Vec<&[Message]> = Vec::new();
    let mut start = 0;
    for i in 1..=messages.len() {
        let ends = i == messages.len() || {
            let (prev, m) = (&messages[i - 1], &messages[i]);
            prev.author.id != m.author.id || m.at - prev.at >= metrics::GROUP_GAP || time::day_number(prev.at) != time::day_number(m.at)
        };
        if ends {
            runs.push(&messages[start..i]);
            start = i;
        }
    }
    let mut day = None;
    for run in runs {
        let first = &run[0];
        if day != Some(time::day_number(first.at)) {
            day = Some(time::day_number(first.at));
            ui.add_space(14.0);
            ui.label(RichText::new(time::day(first.at, now).to_uppercase()).font(ty::meta()).color(p.faint));
            ui.add_space(4.0);
        }
        let mine = !me.is_empty() && first.author.name == me;
        Frame::new()
            .fill(if mine { p.selected.gamma_multiply(0.55) } else { p.surface })
            .stroke(Stroke::new(1.0, p.line))
            .corner_radius(CornerRadius::same(metrics::POST_RADIUS))
            .inner_margin(Margin::symmetric(16, 12))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                for (i, m) in run.iter().enumerate() {
                    if i > 0 {
                        ui.add_space(6.0);
                    }
                    message(ui, pictures, m, metrics::AVATAR, i == 0);
                }
            });
        ui.add_space(8.0);
    }
}

/// One message: avatar, author and time (unless it continues the one above), text,
/// card and reactions.
fn message(ui: &mut Ui, pictures: &mut Pictures, m: &Message, avatar: f32, header: bool) {
    let p = colors();
    ui.horizontal_top(|ui| {
        if header {
            let picture = pictures.get(&Picture::Person { id: m.author.id.clone(), name: m.author.name.clone() });
            widgets::avatar(ui, &m.author.id, &m.author.name, avatar, picture);
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
                widgets::blocks(ui, &m.body);
            }
            if let Some(card) = &m.card {
                widgets::card(ui, card);
            }
            widgets::reactions(ui, &m.reactions);
        });
    });
}
