//! The list panel: what the tab picked — recent chats and channels behind filter
//! chips, teams with their channels, meeting chats, or the activity feed. Each row
//! has a picture, the name, the newest message and when it came.

use crate::model::{Conversation, Directory, Kind, Message, Picture, time};
use crate::ui::style::icons::{self, Icon};
use crate::ui::style::{colors, metrics, typography as ty};
use crate::ui::widgets;
use crate::ui::workspace::{Chip, Section, Workspace};
use eframe::egui::{self, Align, Align2, CornerRadius, Layout, Rect, RichText, Sense, TextureId, Ui, pos2, vec2};

/// Meeting chats listed under Recent (the newest); the Meetings tab has them all.
const MEETINGS_IN_RECENT: usize = 5;

pub fn show(ui: &mut Ui, ws: &mut Workspace) {
    let p = colors();
    let full = ui.max_rect();
    let Some(directory) = ws.directory.clone() else {
        let mut c = ui.new_child(egui::UiBuilder::new().max_rect(full.shrink(20.0)));
        c.add_space(20.0);
        c.vertical_centered(|ui| {
            ui.spinner();
            ui.label(RichText::new("Loading…").font(ty::small()).color(p.faint));
        });
        return;
    };
    let mut top = full.top() + 14.0;
    if ws.section == Section::Chats {
        chips(ui, ws, pos2(full.left() + 14.0, top));
        top += 30.0 + 6.0;
    }
    // Collapse or expand every section at once.
    let keys: Vec<String> = match ws.section {
        Section::Chats => vec!["pinned".into(), "recent".into()],
        Section::Teams => directory.teams.iter().map(|t| format!("team:{}", t.id)).collect(),
        _ => Vec::new(),
    };
    if !keys.is_empty() {
        fold_all(ui, ws, Rect::from_min_max(pos2(full.left() + 14.0, top), pos2(full.right() - 18.0, top + 20.0)), &keys);
        top += 20.0;
    }
    let area = Rect::from_min_max(pos2(full.left(), top), pos2(full.right(), full.bottom() - 6.0));
    let mut body = ui.new_child(egui::UiBuilder::new().max_rect(area).layout(Layout::top_down(Align::Min)));
    egui::ScrollArea::vertical().id_salt(("list", ws.section as u8)).auto_shrink([false, false]).show(&mut body, |ui| {
        match ws.section {
            Section::Chats => chats(ui, ws, &directory),
            Section::Teams => teams(ui, ws, &directory),
            Section::Meetings => meetings(ui, ws, &directory),
            Section::Activity => activity(ui, ws, &directory),
        }
        ui.add_space(10.0);
    });
}

/// The filter chips above the chats.
fn chips(ui: &mut Ui, ws: &mut Workspace, at: egui::Pos2) {
    let p = colors();
    let mut x = at.x;
    for (chip, label) in [(Chip::All, "All"), (Chip::Unread, "Unread"), (Chip::Channels, "Channels"), (Chip::People, "People")] {
        let galley = ui.painter().layout_no_wrap(label.to_string(), ty::small_strong(), p.text);
        let rect = Rect::from_min_size(pos2(x, at.y), vec2(galley.size().x + 22.0, 28.0));
        let response =
            ui.interact(rect, ui.scope_id().with(("chip", label)), Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
        let on = ws.chip == chip;
        let (fill, fg) = if on {
            (p.text, p.panel)
        } else if response.hovered() {
            (p.hover, p.text)
        } else {
            (p.surface, p.dim)
        };
        ui.painter().rect_filled(rect, CornerRadius::same(14), fill);
        ui.painter().text(rect.center(), Align2::CENTER_CENTER, label, ty::small_strong(), fg);
        if response.clicked() {
            ws.chip = chip;
        }
        x = rect.right() + 6.0;
    }
}

fn matches(ws: &Workspace, title: &str) -> bool {
    let filter = ws.filter.trim().to_lowercase();
    filter.is_empty() || title.to_lowercase().contains(&filter)
}

fn chats(ui: &mut Ui, ws: &mut Workspace, d: &Directory) {
    let chip = ws.chip;
    let fits = |c: &Conversation| match chip {
        Chip::All => true,
        Chip::Unread => c.unread,
        Chip::Channels => c.kind == Kind::Channel,
        Chip::People => c.kind == Kind::Chat,
    };
    let pinned: Vec<Conversation> =
        d.conversations.iter().filter(|c| c.pinned && c.kind != Kind::Feed && fits(c) && matches(ws, &c.title)).cloned().collect();
    if !pinned.is_empty() && section(ui, ws, "pinned", "PINNED") {
        for c in &pinned {
            row(ui, ws, d, c);
        }
    }
    // Recent: chats and channels together, newest first; only a few meeting chats.
    let mut meetings = 0;
    let recent: Vec<Conversation> = d
        .conversations
        .iter()
        .filter(|c| !c.pinned && c.kind != Kind::Feed && fits(c) && matches(ws, &c.title))
        .filter(|c| {
            if c.kind != Kind::Meeting {
                return true;
            }
            meetings += 1;
            meetings <= MEETINGS_IN_RECENT || c.unread
        })
        .cloned()
        .collect();
    if !recent.is_empty() && section(ui, ws, "recent", "RECENT") {
        for c in &recent {
            row(ui, ws, d, c);
        }
    }
    if pinned.is_empty() && recent.is_empty() {
        empty(ui, "Nothing here.");
    }
}

fn teams(ui: &mut Ui, ws: &mut Workspace, d: &Directory) {
    let p = colors();
    for team in &d.teams {
        let channels: Vec<Conversation> = team
            .channels
            .iter()
            .filter(|c| matches(ws, &c.name) || matches(ws, &team.name))
            .filter_map(|c| d.conversation(&c.id))
            .cloned()
            .collect();
        if channels.is_empty() {
            continue;
        }
        let key = format!("team:{}", team.id);
        let open = !ws.folded.contains(&key);
        ui.add_space(6.0);
        let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), 34.0), Sense::click());
        let picture = team.picture.as_ref().and_then(|pic| ws.pictures.get(pic));
        let mut c =
            ui.new_child(egui::UiBuilder::new().max_rect(rect.shrink2(vec2(18.0, 0.0))).layout(Layout::left_to_right(Align::Center)));
        widgets::tile(&mut c, &team.id, &team.name, 22.0, picture);
        c.add_space(4.0);
        c.label(RichText::new(&team.name).font(ty::team()).color(p.text));
        c.with_layout(Layout::right_to_left(Align::Center), |ui| {
            widgets::icon(ui, if open { Icon::ChevronDown } else { Icon::ChevronRight }, 13.0, p.faint);
        });
        if response.clicked() {
            if open {
                ws.folded.insert(key);
            } else {
                ws.folded.remove(&key);
            }
        }
        if open {
            for channel in &channels {
                row(ui, ws, d, channel);
            }
        }
    }
}

fn meetings(ui: &mut Ui, ws: &mut Workspace, d: &Directory) {
    let list: Vec<Conversation> = d.conversations.iter().filter(|c| c.kind == Kind::Meeting && matches(ws, &c.title)).cloned().collect();
    if list.is_empty() {
        empty(ui, "No meeting chats yet.");
    }
    for c in &list {
        row(ui, ws, d, c);
    }
}

/// The activity feed: who mentioned or answered you, and where.
fn activity(ui: &mut Ui, ws: &mut Workspace, d: &Directory) {
    let Some(items) = ws.activity.clone() else {
        ui.add_space(20.0);
        ui.vertical_centered(|ui| ui.spinner());
        return;
    };
    let shown: Vec<&Message> =
        items.iter().rev().filter(|m| matches(ws, &m.author.name) || m.source.as_ref().is_some_and(|s| matches(ws, &s.title))).collect();
    if shown.is_empty() {
        empty(ui, "No mentions or replies yet.");
    }
    ui.add_space(4.0);
    for m in shown {
        activity_row(ui, ws, d, m);
    }
}

/// A section label that folds its rows away (click it); true while it's open.
fn section(ui: &mut Ui, ws: &mut Workspace, key: &str, text: &str) -> bool {
    let p = colors();
    let open = !ws.folded.contains(key);
    ui.add_space(6.0);
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), 24.0), Sense::click());
    let color = if response.hovered() { p.dim } else { p.faint };
    let chevron = Rect::from_min_size(pos2(rect.left() + 18.0, rect.center().y - 6.0), vec2(12.0, 12.0));
    icons::paint(ui.painter(), chevron, if open { Icon::ChevronDown } else { Icon::ChevronRight }, color);
    ui.painter().text(pos2(chevron.right() + 6.0, rect.center().y), Align2::LEFT_CENTER, text, ty::section(), color);
    if response.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
        if open {
            ws.folded.insert(key.to_string());
        } else {
            ws.folded.remove(key);
        }
    }
    open
}

/// "Collapse all" / "Expand all" for the sections `keys`, right-aligned in `rect`.
fn fold_all(ui: &mut Ui, ws: &mut Workspace, rect: Rect, keys: &[String]) {
    if keys.is_empty() {
        return;
    }
    let p = colors();
    let any_open = keys.iter().any(|k| !ws.folded.contains(k));
    let text = if any_open { "Collapse all" } else { "Expand all" };
    let galley = ui.painter().layout_no_wrap(text.to_string(), ty::meta(), p.faint);
    let r = Rect::from_min_size(pos2(rect.right() - galley.size().x - 4.0, rect.center().y - 9.0), vec2(galley.size().x + 4.0, 18.0));
    let response = ui.interact(r, ui.scope_id().with("fold-all"), Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
    let color = if response.hovered() { p.accent } else { p.faint };
    ui.painter().text(pos2(r.right(), r.center().y), Align2::RIGHT_CENTER, text, ty::meta(), color);
    if response.clicked() {
        for k in keys {
            if any_open {
                ws.folded.insert(k.clone());
            } else {
                ws.folded.remove(k);
            }
        }
    }
}

fn empty(ui: &mut Ui, text: &str) {
    ui.add_space(14.0);
    ui.horizontal(|ui| {
        ui.add_space(20.0);
        ui.label(RichText::new(text).font(ty::small()).color(colors().faint));
    });
}

/// What leads a row: a person's photo, a group's, or a tile with a channel's or a
/// meeting's mark.
fn lead(ui: &mut Ui, ws: &mut Workspace, c: &Conversation, rect: Rect) {
    let p = colors();
    let tile = |ui: &mut Ui, icon: Icon| {
        ui.painter().rect_filled(rect, CornerRadius::same(11), p.surface);
        icons::paint(ui.painter(), Rect::from_center_size(rect.center(), vec2(17.0, 17.0)), icon, p.dim);
    };
    match c.kind {
        Kind::Channel => tile(ui, Icon::Hash),
        Kind::Meeting => tile(ui, Icon::Calendar),
        Kind::Feed => tile(ui, Icon::Bell),
        Kind::Chat => {
            let picture: Option<TextureId> = c.picture.as_ref().and_then(|pic| ws.pictures.get(pic));
            let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect));
            widgets::avatar(&mut child, &c.id, &c.title, rect.width(), picture);
        }
    }
}

/// The newest message, as "Name: text" (or "You: text").
fn preview(ws: &Workspace, c: &Conversation) -> String {
    let Some(last) = &c.last else { return String::new() };
    let first = last.author.split_whitespace().next().unwrap_or("");
    if last.author == ws.me {
        format!("You: {}", last.text)
    } else if last.author.is_empty() || (c.kind == Kind::Chat && last.author == c.title) {
        last.text.clone()
    } else {
        format!("{first}: {}", last.text)
    }
}

/// A conversation's row: picture, name and newest message; time and an unread dot
/// (or the mute mark) on the right.
fn row(ui: &mut Ui, ws: &mut Workspace, d: &Directory, c: &Conversation) {
    let p = colors();
    let selected = ws.open.as_deref() == Some(c.id.as_str());
    let (outer, response) = ui.allocate_exact_size(vec2(ui.available_width(), metrics::ROW + 2.0), Sense::click());
    let rect = Rect::from_min_size(outer.min + vec2(8.0, 1.0), vec2(outer.width() - 16.0, metrics::ROW));
    if selected || response.hovered() {
        ui.painter().rect_filled(rect, CornerRadius::same(12), if selected { p.selected } else { p.hover });
    }
    let size = metrics::LIST_AVATAR;
    let pic = Rect::from_min_size(pos2(rect.left() + 10.0, rect.center().y - size / 2.0), vec2(size, size));
    lead(ui, ws, c, pic);

    // Right column: time, and an unread dot or the mute mark under it.
    let right = rect.right() - 12.0;
    let when = c.last.as_ref().map(|l| short_time(l.at)).unwrap_or_default();
    let galley = ui.painter().layout_no_wrap(when, ty::meta(), p.faint);
    let time_w = galley.size().x;
    ui.painter().galley(pos2(right - time_w, rect.top() + 10.0), galley, p.faint);
    let mark = pos2(right - 5.0, rect.bottom() - 15.0);
    if c.muted {
        icons::paint(ui.painter(), Rect::from_center_size(mark, vec2(13.0, 13.0)), Icon::Mute, p.faint);
    } else if c.unread {
        ui.painter().circle_filled(mark, 4.5, p.accent_fill);
    }

    // Name and preview.
    let text_left = pic.right() + 11.0;
    let text_right = right - time_w.max(14.0) - 8.0;
    let strong = c.unread || selected;
    let name_color = if c.muted {
        p.faint
    } else if strong {
        p.text
    } else {
        p.dim
    };
    let mut name = c.title.clone();
    if c.kind == Kind::Channel
        && ws.section != Section::Teams
        && let Some((team, _)) = d.channel(&c.id)
    {
        name = format!("{name} · {}", team.name);
    }
    let top = Rect::from_min_max(pos2(text_left, rect.top() + 8.0), pos2(text_right, rect.top() + 26.0));
    let mut t = ui.new_child(egui::UiBuilder::new().max_rect(top).layout(Layout::left_to_right(Align::Center)));
    t.add(egui::Label::new(RichText::new(name).font(ty::item()).color(name_color)).truncate().selectable(false));
    let bottom = Rect::from_min_max(pos2(text_left, rect.top() + 28.0), pos2(rect.right() - 26.0, rect.bottom() - 6.0));
    let mut b = ui.new_child(egui::UiBuilder::new().max_rect(bottom).layout(Layout::left_to_right(Align::Center)));
    let preview_color = if c.unread && !c.muted { p.dim } else { p.faint };
    b.add(egui::Label::new(RichText::new(preview(ws, c)).font(ty::small()).color(preview_color)).truncate().selectable(false));

    if response.clicked() {
        ws.open(&c.id);
    }
    response.context_menu(|ui| {
        let label = if c.pinned { "Unpin" } else { "Pin to the top" };
        if ui.button(label).clicked() {
            ws.set_pinned(ui.ctx(), &c.id, !c.pinned);
            ui.close();
        }
    });
}

/// An activity item: who, what they did, where, what they wrote, and when.
fn activity_row(ui: &mut Ui, ws: &mut Workspace, d: &Directory, m: &Message) {
    let p = colors();
    let Some(source) = &m.source else { return };
    let selected = ws.open.as_deref() == Some(source.conversation.as_str()) && ws.focus.is_some() && ws.focus == source.message;
    let (outer, response) = ui.allocate_exact_size(vec2(ui.available_width(), 74.0), Sense::click());
    let rect = Rect::from_min_size(outer.min + vec2(8.0, 1.0), vec2(outer.width() - 16.0, 72.0));
    if selected || response.hovered() {
        ui.painter().rect_filled(rect, CornerRadius::same(12), if selected { p.selected } else { p.hover });
    }
    let size = metrics::LIST_AVATAR;
    let pic = Rect::from_min_size(pos2(rect.left() + 10.0, rect.top() + 10.0), vec2(size, size));
    let picture = ws.pictures.get(&Picture::Person { id: m.author.id.clone(), name: m.author.name.clone() });
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(pic));
    widgets::avatar(&mut child, &m.author.id, &m.author.name, size, picture);

    let galley = ui.painter().layout_no_wrap(short_time(m.at), ty::meta(), p.faint);
    let time_w = galley.size().x;
    ui.painter().galley(pos2(rect.right() - 12.0 - time_w, rect.top() + 9.0), galley, p.faint);

    let what = match source.what.as_str() {
        "mention" | "mentionInChat" => "mentioned you",
        "reply" | "replyToReply" => "replied",
        "reaction" | "reactionInChat" => "reacted",
        "teamMembershipChange" => "added you",
        _ => "",
    };
    let left = pic.right() + 11.0;
    let line1 = Rect::from_min_max(pos2(left, rect.top() + 7.0), pos2(rect.right() - 18.0 - time_w, rect.top() + 25.0));
    let mut a = ui.new_child(egui::UiBuilder::new().max_rect(line1).layout(Layout::left_to_right(Align::Center)));
    let mut job = egui::text::LayoutJob::default();
    job.append(&m.author.name, 0.0, egui::TextFormat::simple(ty::item(), p.text));
    if !what.is_empty() {
        job.append(what, 5.0, egui::TextFormat::simple(ty::small(), p.dim));
    }
    a.add(egui::Label::new(job).truncate().selectable(false));
    let line2 = Rect::from_min_max(pos2(left, rect.top() + 27.0), pos2(rect.right() - 12.0, rect.top() + 44.0));
    let mut b = ui.new_child(egui::UiBuilder::new().max_rect(line2).layout(Layout::left_to_right(Align::Center)));
    b.add(
        egui::Label::new(RichText::new(crate::model::text::flatten(&m.body)).font(ty::small()).color(p.dim)).truncate().selectable(false),
    );
    let place = if source.title.is_empty() {
        d.conversation(&source.conversation).map_or(String::new(), |c| c.title.clone())
    } else {
        source.title.clone()
    };
    let line3 = Rect::from_min_max(pos2(left, rect.top() + 46.0), pos2(rect.right() - 12.0, rect.top() + 62.0));
    let mut c = ui.new_child(egui::UiBuilder::new().max_rect(line3).layout(Layout::left_to_right(Align::Center)));
    if !place.is_empty() {
        c.add(egui::Label::new(RichText::new(format!("in {place}")).font(ty::meta()).color(p.faint)).truncate().selectable(false));
    }
    if response.clicked() {
        ws.open_message(&source.conversation, source.message.clone(), source.root.clone());
    }
}

/// Today's time, "Yesterday", the weekday this week, or the date.
fn short_time(at: i64) -> String {
    let now = time::now_ms();
    let day = time::day(at, now);
    match time::day_number(now) - time::day_number(at) {
        0 => time::clock(at),
        1 => "Yesterday".into(),
        2..=6 => day.chars().take(3).collect(),
        _ => {
            let parts: Vec<&str> = day.split_whitespace().collect();
            if parts.len() >= 3 { format!("{} {}", parts[1], parts[2]) } else { day }
        }
    }
}
