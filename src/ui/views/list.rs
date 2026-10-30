//! The list beside the rail: the section's title, search, the meeting going on
//! today, and the section's conversations — teams with their channels, or chats.

use crate::model::{Conversation, Directory, Kind, time};
use crate::ui::style::icons::Icon;
use crate::ui::style::{colors, metrics, typography as ty};
use crate::ui::widgets;
use crate::ui::workspace::{Section, Workspace};
use eframe::egui::{self, Align, CornerRadius, Frame, Layout, Margin, Rect, RichText, Sense, Stroke, TextureId, Ui, pos2, vec2};

/// Meeting chats listed under Chats when unfolded (the newest).
const MEETINGS_SHOWN: usize = 12;

pub fn show(ui: &mut Ui, ws: &mut Workspace) {
    let p = colors();
    let full = ui.max_rect();
    super::drag_area(ui, Rect::from_min_size(full.min, vec2(full.width(), metrics::TITLE_BAR - 6.0)), "list-drag");
    ui.painter().vline(full.right() - 0.5, full.y_range(), Stroke::new(1.0, p.line));

    // Title and "new message".
    let title = match ws.section {
        Section::Activity => "Activity",
        Section::Chats => "Chats",
        Section::Teams => "Teams",
        Section::Meetings => "Meetings",
    };
    let head = Rect::from_min_size(pos2(full.left() + 16.0, full.top() + 10.0), vec2(full.width() - 28.0, 32.0));
    let mut h = ui.new_child(egui::UiBuilder::new().max_rect(head).layout(Layout::left_to_right(Align::Center)));
    h.label(RichText::new(title).font(ty::list_title()).color(p.text));
    h.with_layout(Layout::right_to_left(Align::Center), |ui| {
        let _ = widgets::icon_button(ui, Icon::Edit, "New message (coming soon)");
    });

    // Search.
    let field = Rect::from_min_size(pos2(full.left() + 12.0, head.bottom() + 8.0), vec2(full.width() - 24.0, 34.0));
    ui.painter().rect(field, CornerRadius::same(9), p.surface, Stroke::new(1.0, p.line), egui::StrokeKind::Inside);
    let icon = Rect::from_min_size(pos2(field.left() + 10.0, field.center().y - 7.5), vec2(15.0, 15.0));
    crate::ui::style::icons::paint(ui.painter(), icon, Icon::Search, p.faint);
    ui.painter().text(pos2(field.right() - 10.0, field.center().y), egui::Align2::RIGHT_CENTER, "⌘K", ty::meta(), p.faint);
    let area = Rect::from_min_max(pos2(icon.right() + 8.0, field.top()), pos2(field.right() - 36.0, field.bottom()));
    let mut s = ui.new_child(egui::UiBuilder::new().max_rect(area).layout(Layout::left_to_right(Align::Center)));
    let search = egui::TextEdit::singleline(&mut ws.filter)
        .id_salt("list-search")
        .hint_text(RichText::new("Search").color(p.faint).font(ty::small()))
        .font(ty::small())
        .frame(Frame::NONE)
        .margin(Margin::ZERO)
        .desired_width(f32::INFINITY);
    s.add(search);

    let top = field.bottom() + 12.0;
    let rest = Rect::from_min_max(pos2(full.left(), top), full.max);
    let mut body = ui.new_child(egui::UiBuilder::new().max_rect(rest).layout(Layout::top_down(Align::Min)));
    let Some(directory) = ws.directory.clone() else {
        body.add_space(20.0);
        body.vertical_centered(|ui| {
            ui.spinner();
            ui.label(RichText::new("Loading…").font(ty::small()).color(p.faint));
        });
        return;
    };
    if ws.filter.trim().is_empty()
        && matches!(ws.section, Section::Teams | Section::Chats)
        && let Some(meeting) = live_meeting(&directory)
    {
        meeting_card(&mut body, ws, meeting);
        body.add_space(8.0);
    }
    egui::ScrollArea::vertical().id_salt(("list", title)).auto_shrink([false, false]).show(&mut body, |ui| {
        match ws.section {
            Section::Teams => teams(ui, ws, &directory),
            Section::Chats => chats(ui, ws, &directory),
            Section::Meetings => meetings(ui, ws, &directory),
            Section::Activity => activity(ui, ws, &directory),
        }
        ui.add_space(14.0);
    });
}

/// The meeting to join today: the newest meeting chat with a link and activity today.
fn live_meeting(d: &Directory) -> Option<&Conversation> {
    let today = time::day_number(time::now_ms());
    d.conversations
        .iter()
        .filter(|c| c.kind == Kind::Meeting && c.join_url.is_some())
        .filter(|c| c.last.as_ref().is_some_and(|l| time::day_number(l.at) == today))
        .max_by_key(|c| c.last.as_ref().map_or(0, |l| l.at))
}

/// The meeting card: a camera, the meeting's name and when it was last active, Join.
fn meeting_card(ui: &mut Ui, ws: &mut Workspace, meeting: &Conversation) {
    let p = colors();
    Frame::new()
        .outer_margin(Margin::symmetric(12, 0))
        .fill(p.selected)
        .corner_radius(CornerRadius::same(10))
        .inner_margin(Margin::symmetric(12, 10))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                widgets::icon(ui, Icon::Video, 18.0, p.text);
                ui.add_space(4.0);
                let clicked = ui
                    .vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 1.0;
                        ui.add(egui::Label::new(RichText::new(&meeting.title).font(ty::small_strong()).color(p.text)).truncate());
                        let when = meeting.last.as_ref().map(|l| format!("Active {}", time::clock(l.at))).unwrap_or_default();
                        ui.label(RichText::new(when).font(ty::meta()).color(p.dim));
                    })
                    .response
                    .interact(Sense::click())
                    .clicked();
                if clicked {
                    ws.open(&meeting.id);
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if accent_button(ui, "Join").clicked()
                        && let Some(url) = &meeting.join_url
                    {
                        ui.ctx().open_url(egui::OpenUrl::new_tab(url));
                    }
                });
            });
        });
}

/// A small filled button in the accent.
pub fn accent_button(ui: &mut Ui, label: &str) -> egui::Response {
    let p = colors();
    let galley = ui.painter().layout_no_wrap(label.to_string(), ty::small_strong(), p.on_accent);
    let size = vec2(galley.size().x + 24.0, 28.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let fill = if response.hovered() { p.accent } else { p.accent_fill };
    ui.painter().rect_filled(rect, CornerRadius::same(8), fill);
    ui.painter().galley(rect.center() - galley.size() / 2.0, galley, p.on_accent);
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

fn matches(ws: &Workspace, title: &str) -> bool {
    let filter = ws.filter.trim().to_lowercase();
    filter.is_empty() || title.to_lowercase().contains(&filter)
}

fn teams(ui: &mut Ui, ws: &mut Workspace, d: &Directory) {
    let pinned: Vec<&Conversation> =
        d.conversations.iter().filter(|c| c.pinned && c.kind == Kind::Channel && matches(ws, &c.title)).collect();
    if !pinned.is_empty() && section(ui, ws, "pinned-channels", "Pinned") {
        for c in pinned {
            row(ui, ws, c);
        }
    }
    for team in &d.teams {
        let channels: Vec<&Conversation> = team
            .channels
            .iter()
            .filter(|c| matches(ws, &c.name) || matches(ws, &team.name))
            .filter_map(|c| d.conversation(&c.id))
            .collect();
        if channels.is_empty() {
            continue;
        }
        let picture = team.picture.as_ref().and_then(|p| ws.pictures.get(p));
        if team_header(ui, ws, &team.id, &team.name, picture) {
            for c in channels {
                row(ui, ws, c);
            }
        }
    }
}

fn chats(ui: &mut Ui, ws: &mut Workspace, d: &Directory) {
    let pinned: Vec<&Conversation> =
        d.conversations.iter().filter(|c| c.pinned && matches!(c.kind, Kind::Chat | Kind::Meeting) && matches(ws, &c.title)).collect();
    if !pinned.is_empty() && section(ui, ws, "pinned-chats", "Pinned") {
        for c in pinned {
            row(ui, ws, c);
        }
    }
    let direct: Vec<&Conversation> = d.chats().filter(|c| !c.pinned && matches(ws, &c.title)).collect();
    if !direct.is_empty() && section(ui, ws, "chats", "Direct messages") {
        for c in direct {
            row(ui, ws, c);
        }
    }
    let meetings: Vec<&Conversation> =
        d.conversations.iter().filter(|c| c.kind == Kind::Meeting && !c.pinned && matches(ws, &c.title)).take(MEETINGS_SHOWN).collect();
    if !meetings.is_empty() && section(ui, ws, "meetings", "Meeting chats") {
        for c in meetings {
            row(ui, ws, c);
        }
    }
}

fn meetings(ui: &mut Ui, ws: &mut Workspace, d: &Directory) {
    let list: Vec<&Conversation> = d.conversations.iter().filter(|c| c.kind == Kind::Meeting && matches(ws, &c.title)).collect();
    if list.is_empty() {
        empty(ui, "No meetings yet.");
    }
    for c in list {
        row(ui, ws, c);
    }
}

fn activity(ui: &mut Ui, ws: &mut Workspace, d: &Directory) {
    let feeds: Vec<&Conversation> = d.conversations.iter().filter(|c| c.kind == Kind::Feed).take(1).collect();
    if feeds.is_empty() {
        empty(ui, "Nothing here yet.");
    }
    for c in feeds {
        row(ui, ws, c);
    }
}

fn empty(ui: &mut Ui, text: &str) {
    ui.add_space(12.0);
    ui.horizontal(|ui| {
        ui.add_space(18.0);
        ui.label(RichText::new(text).font(ty::small()).color(colors().faint));
    });
}

/// A section label that folds its rows away; true while it's open.
fn section(ui: &mut Ui, ws: &mut Workspace, key: &str, label: &str) -> bool {
    let p = colors();
    let open = !ws.folded.contains(key);
    ui.add_space(6.0);
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), 24.0), Sense::click());
    let chevron = Rect::from_min_size(pos2(rect.left() + 16.0, rect.center().y - 6.5), vec2(13.0, 13.0));
    crate::ui::style::icons::paint(ui.painter(), chevron, if open { Icon::ChevronDown } else { Icon::ChevronRight }, p.faint);
    ui.painter().text(pos2(chevron.right() + 4.0, rect.center().y), egui::Align2::LEFT_CENTER, label, ty::section(), p.faint);
    if response.clicked() {
        toggle(ws, key, open);
    }
    open
}

/// A team's name with its tile; clicking folds its channels.
fn team_header(ui: &mut Ui, ws: &mut Workspace, id: &str, name: &str, picture: Option<TextureId>) -> bool {
    let p = colors();
    let key = format!("team:{id}");
    let open = !ws.folded.contains(&key);
    ui.add_space(8.0);
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), 28.0), Sense::click());
    let mut c = ui.new_child(egui::UiBuilder::new().max_rect(rect.shrink2(vec2(18.0, 0.0))).layout(Layout::left_to_right(Align::Center)));
    widgets::tile(&mut c, id, name, 20.0, picture);
    c.add_space(4.0);
    c.add(egui::Label::new(RichText::new(name).font(ty::team()).color(p.text)).truncate().selectable(false));
    if response.clicked() {
        toggle(ws, &key, open);
    }
    open
}

fn toggle(ws: &mut Workspace, key: &str, open: bool) {
    if open {
        ws.folded.insert(key.to_string());
    } else {
        ws.folded.remove(key);
    }
}

/// A conversation's row: a channel's mark or a chat's avatar, the name, a mute mark
/// or an unread dot.
fn row(ui: &mut Ui, ws: &mut Workspace, c: &Conversation) {
    let p = colors();
    let selected = ws.open.as_deref() == Some(c.id.as_str());
    let (outer, response) = ui.allocate_exact_size(vec2(ui.available_width(), metrics::ROW + 2.0), Sense::click());
    let rect = Rect::from_min_size(outer.min + vec2(metrics::SIDE_PAD, 1.0), vec2(outer.width() - 2.0 * metrics::SIDE_PAD, metrics::ROW));
    if selected || response.hovered() {
        ui.painter().rect_filled(rect, CornerRadius::same(metrics::RADIUS), if selected { p.selected } else { p.hover });
    }
    let mut inner =
        ui.new_child(egui::UiBuilder::new().max_rect(rect.shrink2(vec2(10.0, 0.0))).layout(Layout::left_to_right(Align::Center)));
    match c.kind {
        Kind::Channel => {
            let (r, _) = inner.allocate_exact_size(vec2(18.0, 18.0), Sense::hover());
            crate::ui::style::icons::paint(inner.painter(), Rect::from_center_size(r.center(), vec2(15.0, 15.0)), Icon::Hash, p.faint);
        }
        Kind::Meeting => {
            let (r, _) = inner.allocate_exact_size(vec2(18.0, 18.0), Sense::hover());
            crate::ui::style::icons::paint(inner.painter(), Rect::from_center_size(r.center(), vec2(15.0, 15.0)), Icon::Calendar, p.faint);
        }
        Kind::Feed => {
            let (r, _) = inner.allocate_exact_size(vec2(18.0, 18.0), Sense::hover());
            crate::ui::style::icons::paint(inner.painter(), Rect::from_center_size(r.center(), vec2(15.0, 15.0)), Icon::At, p.faint);
        }
        Kind::Chat => {
            let picture = c.picture.as_ref().and_then(|pic| ws.pictures.get(pic));
            widgets::avatar(&mut inner, &c.id, &c.title, 22.0, picture);
        }
    }
    inner.add_space(2.0);
    let (font, color) = if c.muted {
        (ty::item(), p.faint)
    } else if c.unread {
        (ty::item_strong(), p.text)
    } else if selected {
        (ty::item(), p.text)
    } else {
        (ty::item(), p.dim)
    };
    let title = if c.title.is_empty() { "Untitled" } else { &c.title };
    inner.with_layout(Layout::right_to_left(Align::Center), |ui| {
        if c.muted {
            widgets::icon(ui, Icon::Mute, 14.0, p.faint);
        } else if c.unread {
            widgets::unread_dot(ui);
        }
        ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
            ui.add(egui::Label::new(RichText::new(title).font(font).color(color)).truncate().selectable(false));
        });
    });
    if response.clicked() {
        ws.open(&c.id);
    }
}
