//! The list panel: what the tab picked — recent chats and channels behind filter
//! chips, teams with their channels, meeting chats, or the activity feed. Each row
//! has a picture, the name, the newest message and when it came.

use crate::model::{Conversation, Directory, Kind, Message, Picture, time};
use crate::ui::style::icons::{self, Icon};
use crate::ui::style::{colors, metrics, typography as ty};
use crate::ui::widgets;
use crate::ui::workspace::{Chip, Section, Side, Workspace};
use eframe::egui::{self, Align, Align2, CornerRadius, Layout, Rect, RichText, Sense, Stroke, TextureId, Ui, pos2, vec2};

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
    match ws.side {
        Side::Found => return found_panel(ui, ws, &directory, full),
        Side::People => return people_panel(ui, ws, &directory, full),
        Side::List => {}
    }
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
        if !ws.filter.trim().is_empty() {
            hits(ui, ws);
        }
        match ws.section {
            Section::Chats => chats(ui, ws, &directory),
            Section::Teams => teams(ui, ws, &directory),
            Section::Meetings => meetings(ui, ws, &directory),
            Section::Activity => activity(ui, ws, &directory),
        }
        ui.add_space(10.0);
    });
}

/// A panel's title with a × that goes back to the list.
fn panel_head(ui: &mut Ui, ws: &mut Workspace, full: Rect, title: &str, subtitle: &str) -> f32 {
    let p = colors();
    let head = Rect::from_min_size(pos2(full.left() + 18.0, full.top() + 12.0), vec2(full.width() - 30.0, 40.0));
    let mut h = ui.new_child(egui::UiBuilder::new().max_rect(head).layout(Layout::top_down(Align::Min)));
    h.spacing_mut().item_spacing.y = 1.0;
    h.label(RichText::new(title).font(ty::item()).color(p.text));
    h.add(egui::Label::new(RichText::new(subtitle).font(ty::meta()).color(p.faint)).truncate());
    let close = Rect::from_center_size(pos2(full.right() - 26.0, head.top() + 14.0), vec2(26.0, 26.0));
    let response = ui.interact(close, ui.scope_id().with("panel-close"), Sense::click()).on_hover_text("Back to the list");
    if response.hovered() {
        ui.painter().rect_filled(close, CornerRadius::same(8), p.hover);
    }
    icons::paint(ui.painter(), Rect::from_center_size(close.center(), vec2(13.0, 13.0)), Icon::Close, p.dim);
    if response.clicked() {
        if ws.side == Side::Found {
            ws.stop_find();
        }
        ws.side = Side::List;
    }
    head.bottom() + 10.0
}

/// What's found in the open conversation: each match with who, the text with the
/// match marked, and when. A click shows it; the one shown is highlighted.
fn found_panel(ui: &mut Ui, ws: &mut Workspace, d: &Directory, full: Rect) {
    let p = colors();
    let name = ws.open.as_deref().and_then(|id| d.conversation(id)).map_or(String::new(), |c| c.title.clone());
    let found = ws.found();
    let query = ws.find.clone().unwrap_or_default().trim().to_string();
    let subtitle = match (query.is_empty(), found.len()) {
        (true, _) => format!("in {name}"),
        (false, 0) => format!("nothing for “{query}” in {name}"),
        (false, 1) => format!("1 message in {name}"),
        (false, n) => format!("{n} messages in {name}"),
    };
    let top = panel_head(ui, ws, full, "Found", &subtitle);
    let area = Rect::from_min_max(pos2(full.left(), top), pos2(full.right(), full.bottom() - 6.0));
    let mut body = ui.new_child(egui::UiBuilder::new().max_rect(area).layout(Layout::top_down(Align::Min)));
    egui::ScrollArea::vertical().id_salt("found").auto_shrink([false, false]).show(&mut body, |ui| {
        if query.is_empty() {
            empty(ui, "Type above to find in this conversation.");
        }
        let mut show = None;
        for (i, m) in found.iter().enumerate() {
            let hit = crate::model::SearchHit {
                conversation: String::new(),
                message: m.id.clone(),
                author: m.author.name.clone(),
                at: m.at,
                text: crate::model::text::flatten(&m.body),
                place: String::new(),
            };
            if result_row(ui, &hit, &query, i == ws.find_index && ws.focus.as_deref() == Some(m.id.as_str())) {
                show = Some(i);
            }
        }
        if let Some(i) = show {
            ws.show_found(i);
        }
        ui.add_space(10.0);
    });
    let _ = p;
}

/// The people in the open conversation (a chat's members; in a channel, who wrote in
/// it). A click opens your chat with them.
fn people_panel(ui: &mut Ui, ws: &mut Workspace, d: &Directory, full: Rect) {
    let p = colors();
    let id = ws.open.clone().unwrap_or_default();
    let name = d.conversation(&id).map_or(String::new(), |c| c.title.clone());
    let mut people: Vec<crate::model::Mention> = d.conversation(&id).map(|c| c.members.clone()).unwrap_or_default();
    let channel = d.channel(&id).is_some();
    if people.is_empty() {
        for m in ws.messages.iter().flatten().rev() {
            if m.author.id.starts_with("8:") && m.author.id != ws.me_id && !people.iter().any(|x| x.id == m.author.id) {
                people.push(crate::model::Mention { id: m.author.id.clone(), name: m.author.name.clone() });
            }
        }
    }
    let subtitle = if channel { format!("who's written in {name}") } else { format!("in {name}") };
    let top = panel_head(ui, ws, full, &format!("People · {}", people.len()), &subtitle);
    let area = Rect::from_min_max(pos2(full.left(), top), pos2(full.right(), full.bottom() - 6.0));
    let mut body = ui.new_child(egui::UiBuilder::new().max_rect(area).layout(Layout::top_down(Align::Min)));
    egui::ScrollArea::vertical().id_salt("people").auto_shrink([false, false]).show(&mut body, |ui| {
        if people.is_empty() {
            empty(ui, "Nobody yet.");
        }
        for person in &people {
            let chat = d
                .conversations
                .iter()
                .find(|c| matches!(&c.picture, Some(Picture::Person { id, .. }) if *id == person.id))
                .map(|c| c.id.clone());
            let (outer, response) = ui.allocate_exact_size(vec2(ui.available_width(), 48.0), Sense::click());
            let rect = Rect::from_min_size(outer.min + vec2(8.0, 1.0), vec2(outer.width() - 16.0, 46.0));
            if response.hovered() && chat.is_some() {
                ui.painter().rect_filled(rect, CornerRadius::same(12), p.hover);
            }
            let pic = Rect::from_min_size(pos2(rect.left() + 10.0, rect.center().y - 16.0), vec2(32.0, 32.0));
            let picture = ws.pictures.get(&Picture::Person { id: person.id.clone(), name: person.name.clone() });
            let mut c = ui.new_child(egui::UiBuilder::new().max_rect(pic));
            widgets::avatar(&mut c, &person.id, &person.name, 32.0, picture);
            ui.painter().text(pos2(pic.right() + 11.0, rect.center().y - 8.0), Align2::LEFT_CENTER, &person.name, ty::item(), p.text);
            let hint = if chat.is_some() { "Open your chat" } else { "No chat with them yet" };
            ui.painter().text(pos2(pic.right() + 11.0, rect.center().y + 9.0), Align2::LEFT_CENTER, hint, ty::meta(), p.faint);
            if let Some(chat) = chat
                && response.on_hover_cursor(egui::CursorIcon::PointingHand).clicked()
            {
                ws.open(&chat);
            }
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
        empty(ui, if ws.filter.trim().is_empty() { "Nothing here." } else { "No chats or channels by that name." });
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

/// The calendar: meetings from today on, by day; the one on now stands out. A click
/// opens the meeting's chat; Join opens it in Teams on the web.
fn meetings(ui: &mut Ui, ws: &mut Workspace, d: &Directory) {
    let p = colors();
    let Some(all) = ws.meetings.clone() else {
        ui.add_space(20.0);
        ui.vertical_centered(|ui| ui.spinner());
        return;
    };
    let now = time::now_ms();
    let today = time::day_number(now);
    let shown: Vec<_> = all
        .into_iter()
        .filter(|m| time::day_number(m.start) >= today && (m.end > now || time::day_number(m.start) == today))
        .filter(|m| matches(ws, &m.subject) || matches(ws, &m.organizer))
        .collect();
    let mut shown = shown;
    shown.sort_by_key(|m| (m.start, m.end));
    if shown.is_empty() {
        empty(ui, "No meetings in the next two weeks.");
    }
    let mut day = None;
    for m in &shown {
        let n = time::day_number(m.start);
        if day != Some(n) {
            day = Some(n);
            let label = match n - today {
                0 => "TODAY".to_string(),
                1 => "TOMORROW".to_string(),
                _ => time::day(m.start, now).to_uppercase(),
            };
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.add_space(20.0);
                ui.label(RichText::new(label).font(ty::section()).color(p.faint));
            });
            ui.add_space(2.0);
        }
        meeting_row(ui, ws, d, m, now);
    }
    ui.add_space(6.0);
}

/// One meeting: its time, subject, who set it up and where, and Join.
fn meeting_row(ui: &mut Ui, ws: &mut Workspace, d: &Directory, m: &crate::model::Meeting, now: i64) {
    let p = colors();
    let live = m.start - 5 * 60_000 <= now && now < m.end && !m.cancelled;
    let past = m.end <= now;
    let chat = m.chat.as_ref().filter(|c| d.conversation(c).is_some()).cloned();
    let selected = chat.is_some() && ws.open == chat;
    let (outer, response) = ui.allocate_exact_size(vec2(ui.available_width(), 60.0), Sense::click());
    let rect = Rect::from_min_size(outer.min + vec2(8.0, 1.0), vec2(outer.width() - 16.0, 58.0));
    let fill = if selected {
        Some(p.selected)
    } else if live {
        Some(p.accent_fill.gamma_multiply(0.12))
    } else if response.hovered() {
        Some(p.hover)
    } else {
        None
    };
    if let Some(fill) = fill {
        ui.painter().rect_filled(rect, CornerRadius::same(12), fill);
    }
    // The bar on the left: accent while it's on.
    let bar = Rect::from_min_size(pos2(rect.left() + 10.0, rect.top() + 10.0), vec2(3.0, rect.height() - 20.0));
    ui.painter().rect_filled(bar, CornerRadius::same(2), if live { p.accent_fill } else { p.border });

    let fade = if past || m.cancelled { 0.55 } else { 1.0 };
    let times = format!("{} – {}", time::clock(m.start), time::clock(m.end));
    let left = bar.right() + 10.0;
    let join_w = if m.join_url.is_some() && !past && !m.cancelled { 64.0 } else { 0.0 };
    let text_right = rect.right() - 12.0 - join_w;
    let mut t = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(Rect::from_min_max(pos2(left, rect.top() + 7.0), pos2(text_right, rect.top() + 25.0)))
            .layout(Layout::left_to_right(Align::Center)),
    );
    let mut subject = RichText::new(&m.subject).font(ty::item()).color(p.text.gamma_multiply(fade));
    if m.cancelled {
        subject = subject.strikethrough();
    }
    t.add(egui::Label::new(subject).truncate().selectable(false));
    let detail = [times, if live { "now".into() } else { String::new() }, m.organizer.clone()]
        .into_iter()
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" · ");
    let mut b = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(Rect::from_min_max(pos2(left, rect.top() + 27.0), pos2(text_right, rect.top() + 44.0)))
            .layout(Layout::left_to_right(Align::Center)),
    );
    b.add(
        egui::Label::new(RichText::new(detail).font(ty::small()).color(if live { p.accent } else { p.dim.gamma_multiply(fade) }))
            .truncate()
            .selectable(false),
    );
    let place = if m.cancelled { "Cancelled".to_string() } else { m.location.clone() };
    if !place.is_empty() {
        let mut c = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(Rect::from_min_max(pos2(left, rect.top() + 43.0), pos2(text_right, rect.top() + 57.0)))
                .layout(Layout::left_to_right(Align::Center)),
        );
        c.add(egui::Label::new(RichText::new(place).font(ty::meta()).color(p.faint)).truncate().selectable(false));
    }

    let mut joined = false;
    if join_w > 0.0
        && let Some(url) = &m.join_url
    {
        let button = Rect::from_min_size(pos2(rect.right() - 12.0 - 56.0, rect.center().y - 14.0), vec2(56.0, 28.0));
        let b = ui.interact(button, ui.scope_id().with(("join", &m.id, m.start)), Sense::click()).on_hover_text("Join in Teams on the web");
        let (fill, fg, stroke) = if live {
            (if b.hovered() { p.accent } else { p.accent_fill }, p.on_accent, Stroke::NONE)
        } else {
            (if b.hovered() { p.hover } else { egui::Color32::TRANSPARENT }, p.accent, Stroke::new(1.0, p.accent.gamma_multiply(0.6)))
        };
        ui.painter().rect(button, CornerRadius::same(8), fill, stroke, egui::StrokeKind::Inside);
        ui.painter().text(button.center(), Align2::CENTER_CENTER, "Join", ty::small_strong(), fg);
        if b.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
            joined = true;
            ui.ctx().open_url(egui::OpenUrl::new_tab(crate::teams::web_join(url)));
        }
    }
    if !joined && response.clicked() {
        match chat {
            Some(chat) => ws.open(&chat),
            None => {
                if let Some(url) = &m.join_url {
                    ui.ctx().open_url(egui::OpenUrl::new_tab(crate::teams::web_join(url)));
                }
            }
        }
    }
}

/// Messages found for what's typed in the search field.
fn hits(ui: &mut Ui, ws: &mut Workspace) {
    let p = colors();
    let query = ws.filter.trim().to_string();
    let found = ws.hits.as_ref().filter(|(q, _)| *q == query).map(|(_, h)| h.clone());
    if !section(ui, ws, "messages", "MESSAGES") {
        return;
    }
    match found {
        None => {
            ui.horizontal(|ui| {
                ui.add_space(20.0);
                ui.spinner();
                ui.label(RichText::new("Searching messages…").font(ty::small()).color(p.faint));
            });
        }
        Some(hits) if hits.is_empty() => empty(ui, "No messages found."),
        Some(hits) => {
            for hit in &hits {
                hit_row(ui, ws, hit, &query);
            }
        }
    }
    ui.add_space(6.0);
}

/// A message found across conversations: a click opens it.
fn hit_row(ui: &mut Ui, ws: &mut Workspace, hit: &crate::model::SearchHit, query: &str) {
    let selected = ws.focus.as_deref() == Some(hit.message.as_str());
    if result_row(ui, hit, query, selected) {
        ws.open_message(&hit.conversation, Some(hit.message.clone()), None);
    }
}

/// A found message: who wrote it, the text with the query marked, where and when.
/// True when clicked.
fn result_row(ui: &mut Ui, hit: &crate::model::SearchHit, query: &str, selected: bool) -> bool {
    let p = colors();
    let (outer, response) = ui.allocate_exact_size(vec2(ui.available_width(), 66.0), Sense::click());
    let rect = Rect::from_min_size(outer.min + vec2(8.0, 1.0), vec2(outer.width() - 16.0, 64.0));
    if selected || response.hovered() {
        ui.painter().rect_filled(rect, CornerRadius::same(12), if selected { p.selected } else { p.hover });
    }
    let left = rect.left() + 12.0;
    let right = rect.right() - 12.0;
    let galley = ui.painter().layout_no_wrap(short_time(hit.at), ty::meta(), p.faint);
    let time_w = galley.size().x;
    ui.painter().galley(pos2(right - time_w, rect.top() + 8.0), galley, p.faint);
    let line = |y: f32, r: f32| Rect::from_min_max(pos2(left, rect.top() + y), pos2(r, rect.top() + y + 18.0));
    let mut a = ui.new_child(egui::UiBuilder::new().max_rect(line(6.0, right - time_w - 6.0)).layout(Layout::left_to_right(Align::Center)));
    a.add(egui::Label::new(RichText::new(&hit.author).font(ty::item()).color(p.text)).truncate().selectable(false));
    // The text, with the query in the accent.
    let mut job = egui::text::LayoutJob::default();
    let lower = hit.text.to_lowercase();
    let q = query.to_lowercase();
    let mut at = 0;
    while let Some(found) = lower[at..].find(&q).map(|f| f + at) {
        job.append(&hit.text[at..found], 0.0, egui::TextFormat::simple(ty::small(), p.dim));
        job.append(&hit.text[found..found + q.len()], 0.0, egui::TextFormat::simple(ty::small_strong(), p.accent));
        at = found + q.len();
        if q.is_empty() {
            break;
        }
    }
    job.append(&hit.text[at.min(hit.text.len())..], 0.0, egui::TextFormat::simple(ty::small(), p.dim));
    let mut b = ui.new_child(egui::UiBuilder::new().max_rect(line(25.0, right)).layout(Layout::left_to_right(Align::Center)));
    b.add(egui::Label::new(job).truncate().selectable(false));
    let mut c = ui.new_child(egui::UiBuilder::new().max_rect(line(43.0, right)).layout(Layout::left_to_right(Align::Center)));
    if !hit.place.is_empty() {
        c.add(egui::Label::new(RichText::new(format!("in {}", hit.place)).font(ty::meta()).color(p.faint)).truncate().selectable(false));
    }
    response.on_hover_cursor(egui::CursorIcon::PointingHand).clicked()
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
    // New since Activity was last looked at.
    if ws.activity_is_new(m) {
        ui.painter().circle_filled(pos2(rect.right() - 17.0, rect.bottom() - 15.0), 4.5, p.accent_fill);
    }

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
    let line3 = Rect::from_min_max(pos2(left, rect.top() + 46.0), pos2(rect.right() - 30.0, rect.top() + 62.0));
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
