//! The top bar: the wordmark, the tabs (Activity, Chats, Teams, Meetings) with what's
//! unread, search, today's meeting with Join, and the user's menu.

use crate::model::{Conversation, Directory, Kind, Picture, time};
use crate::settings::Appearance;
use crate::ui::app::Request;
use crate::ui::style::icons::{self, Icon};
use crate::ui::style::{colors, metrics, typography as ty};
use crate::ui::widgets;
use crate::ui::workspace::{Section, Workspace};
use eframe::egui::{self, Align2, CornerRadius, Frame, Margin, Rect, RichText, Sense, Stroke, Ui, pos2, vec2};

const PILL_HEIGHT: f32 = 36.0;

pub fn show(ui: &mut Ui, ws: &mut Workspace, theme: Appearance) -> Option<Request> {
    let p = colors();
    let full = ui.max_rect();
    super::drag_area(ui, full, "topbar-drag");
    let cy = full.center().y;
    let mut x = full.left() + metrics::LIGHTS;

    // Wordmark: "drift" and an accent dot.
    let word = ui.painter().layout_no_wrap("drift".into(), ty::wordmark(), p.text);
    let dot = ui.painter().layout_no_wrap(".".into(), ty::wordmark(), p.accent);
    let word_w = word.size().x;
    ui.painter().galley(pos2(x, cy - word.size().y / 2.0), word, p.text);
    ui.painter().galley(pos2(x + word_w, cy - dot.size().y / 2.0), dot, p.accent);
    x += word_w + 34.0;

    // Tabs.
    let counts = (ws.activity_unread(), ws.directory.as_ref().map_or(0, unread_chats));
    let tabs = [
        (Section::Activity, Icon::Bell, "Activity", counts.0),
        (Section::Chats, Icon::Chat, "Chats", counts.1),
        (Section::Teams, Icon::Members, "Teams", 0),
        (Section::Meetings, Icon::Calendar, "Calendar", 0),
    ];
    let widths: Vec<f32> = tabs.iter().map(|t| tab_width(ui, t.2, t.3)).collect();
    let group = Rect::from_min_size(
        pos2(x, cy - PILL_HEIGHT / 2.0 - 1.0),
        vec2(widths.iter().sum::<f32>() + 8.0 + 4.0 * (tabs.len() - 1) as f32, PILL_HEIGHT + 2.0),
    );
    ui.painter().rect_filled(group, CornerRadius::same(12), p.panel);
    let mut tx = group.left() + 4.0;
    for ((section, icon, label, count), w) in tabs.into_iter().zip(widths) {
        let rect = Rect::from_min_size(pos2(tx, group.top() + 4.0), vec2(w, group.height() - 8.0));
        if tab(ui, rect, icon, label, count, ws.section == section).clicked() {
            ws.section = section;
            ws.filter.clear();
        }
        tx += w + 4.0;
    }
    x = group.right() + 16.0;

    // The user's menu at the far right, then today's meeting before it.
    let size = 32.0;
    let avatar = Rect::from_min_size(pos2(full.right() - 16.0 - size, cy - size / 2.0), vec2(size, size));
    let request = user_menu(ui, ws, avatar, theme);
    let mut right = avatar.left() - 12.0;
    // The meeting on now (or next today), from the calendar; a meeting chat active
    // today until the calendar is in.
    if let Some((title, label, url)) = meeting_to_join(ws) {
        right = meeting_pill(ui, &title, &label, &url, right, cy) - 12.0;
    }

    // Search, centred in what's left.
    let width = (right - x).min(420.0);
    if width > 120.0 {
        let left = x + ((right - x) - width) / 2.0;
        search(ui, ws, Rect::from_min_size(pos2(left, cy - PILL_HEIGHT / 2.0), vec2(width, PILL_HEIGHT)));
    }
    request
}

/// Unread activity, and unread chats (direct messages and meeting chats).
/// The meeting to join: its subject, when it is ("now", "in 20 min"), and its link.
pub fn meeting_to_join(ws: &Workspace) -> Option<(String, String, String)> {
    match ws.next_meeting() {
        Some(m) => m.join_url.clone().map(|url| (m.subject.clone(), when(m.start, m.end), url)),
        None if ws.meetings.is_none() => ws.directory.as_ref().and_then(live_meeting).and_then(|c| {
            let url = c.join_url.clone()?;
            Some((c.title.clone(), c.last.as_ref().map(|l| time::clock(l.at)).unwrap_or_default(), url))
        }),
        None => None,
    }
}

/// Chats and meeting chats with something unread (not counting muted ones).
fn unread_chats(d: &Directory) -> usize {
    d.conversations.iter().filter(|c| matches!(c.kind, Kind::Chat | Kind::Meeting) && c.unread && !c.muted).count()
}

fn count_text(count: usize) -> String {
    if count > 99 { "99+".into() } else { count.to_string() }
}

fn tab_width(ui: &Ui, label: &str, count: usize) -> f32 {
    let text = ui.painter().layout_no_wrap(label.to_string(), ty::tab(), colors().text).size().x;
    let badge = if count > 0 { ui.painter().layout_no_wrap(count_text(count), ty::badge(), colors().text).size().x + 18.0 } else { 0.0 };
    14.0 + 16.0 + 7.0 + text + badge + 14.0
}

/// One tab: icon, label, and a count; filled with the accent when it's the one shown.
fn tab(ui: &mut Ui, rect: Rect, icon: Icon, label: &str, count: usize, on: bool) -> egui::Response {
    let p = colors();
    let response = ui.interact(rect, ui.scope_id().with(("tab", label)), Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
    let (fill, fg) = if on {
        (p.accent_fill, p.on_accent)
    } else if response.hovered() {
        (p.hover, p.text)
    } else {
        (egui::Color32::TRANSPARENT, p.dim)
    };
    ui.painter().rect_filled(rect, CornerRadius::same(9), fill);
    let mut x = rect.left() + 14.0;
    icons::paint(ui.painter(), Rect::from_min_size(pos2(x, rect.center().y - 8.0), vec2(16.0, 16.0)), icon, fg);
    x += 23.0;
    let text = ui.painter().layout_no_wrap(label.to_string(), ty::tab(), fg);
    let tw = text.size().x;
    ui.painter().galley(pos2(x, rect.center().y - text.size().y / 2.0), text, fg);
    x += tw + 7.0;
    if count > 0 {
        let (bg, color) = if on { (egui::Color32::from_white_alpha(60), p.on_accent) } else { (p.selected, p.accent) };
        let galley = ui.painter().layout_no_wrap(count_text(count), ty::badge(), color);
        let badge = Rect::from_min_size(pos2(x, rect.center().y - 8.0), vec2(galley.size().x + 12.0, 16.0));
        ui.painter().rect_filled(badge, CornerRadius::same(8), bg);
        ui.painter().galley(badge.center() - galley.size() / 2.0, galley, color);
    }
    response
}

/// The search pill: it narrows the list as you type.
fn search(ui: &mut Ui, ws: &mut Workspace, rect: Rect) {
    let p = colors();
    let focused = ui.memory(|m| m.has_focus(search_id()));
    let font = ty::search();
    // A clear outline in the accent while typing; the pill alone otherwise.
    let stroke = if focused { Stroke::new(1.5, p.accent_fill) } else { Stroke::NONE };
    ui.painter().rect(rect, CornerRadius::same(12), p.panel, stroke, egui::StrokeKind::Inside);
    let icon = Rect::from_min_size(pos2(rect.left() + 13.0, rect.center().y - 8.0), vec2(16.0, 16.0));
    icons::paint(ui.painter(), icon, Icon::Search, if focused { p.accent } else { p.faint });

    // On the right: ⌘K, or × to clear what's typed.
    let has_text = !ws.filter.is_empty();
    let right = rect.right() - 10.0;
    if has_text {
        let clear = Rect::from_center_size(pos2(right - 10.0, rect.center().y), vec2(22.0, 22.0));
        let response = ui.interact(clear, ui.scope_id().with("clear-search"), Sense::click()).on_hover_text("Clear (Esc)");
        if response.hovered() {
            ui.painter().circle_filled(clear.center(), 11.0, p.hover);
        }
        icons::paint(ui.painter(), Rect::from_center_size(clear.center(), vec2(12.0, 12.0)), Icon::Close, p.dim);
        if response.clicked() {
            ws.filter.clear();
            ws.hits = None;
        }
    } else {
        ui.painter().text(pos2(right - 2.0, rect.center().y), Align2::RIGHT_CENTER, "⌘K", ty::mono_small(), p.faint);
    }

    // The text field, exactly one line tall and centred in the pill.
    let line = ui.painter().layout_no_wrap("Ag".into(), font.clone(), p.text).size().y;
    let field =
        Rect::from_min_max(pos2(icon.right() + 9.0, rect.center().y - line / 2.0), pos2(right - 30.0, rect.center().y + line / 2.0));
    let mut c = ui.new_child(egui::UiBuilder::new().max_rect(field));
    let hint = match ws.section {
        Section::Activity => "Search activity",
        Section::Chats => "Search messages, people, channels",
        Section::Teams => "Search teams and channels",
        Section::Meetings => "Search meetings",
    };
    let response = c.add(
        egui::TextEdit::singleline(&mut ws.filter)
            .id(search_id())
            .hint_text(RichText::new(hint).font(font.clone()).color(p.faint))
            .font(font)
            .text_color(p.text)
            .frame(Frame::NONE)
            .margin(Margin::ZERO)
            .desired_width(f32::INFINITY),
    );
    if ui.input_mut(|i| i.consume_shortcut(&egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::K))) {
        response.request_focus();
    }
    // Enter searches messages at once (otherwise a pause in typing does); Esc clears.
    if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
        ws.search(ui.ctx());
    }
    if response.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        ws.filter.clear();
        ws.hits = None;
        response.surrender_focus();
    }
    if response.changed() && ws.filter.is_empty() {
        ws.hits = None;
    }
}

/// The search field's id, for focusing it from elsewhere (the header's search button).
pub fn search_id() -> egui::Id {
    egui::Id::unique("search")
}

/// The meeting to join today: the newest meeting chat with a link and activity today.
pub fn live_meeting(d: &Directory) -> Option<&Conversation> {
    let today = time::day_number(time::now_ms());
    d.conversations
        .iter()
        .filter(|c| c.kind == Kind::Meeting && c.join_url.is_some())
        .filter(|c| c.last.as_ref().is_some_and(|l| time::day_number(l.at) == today))
        .max_by_key(|c| c.last.as_ref().map_or(0, |l| l.at))
}

/// Today's meeting, ending at `right`: its name, when it was active, Join. Returns its left edge.
fn meeting_pill(ui: &mut Ui, title: &str, label: &str, url: &str, right: f32, cy: f32) -> f32 {
    let p = colors();
    let name = ui.painter().layout_no_wrap(truncate(title, 26), ty::small_strong(), p.text);
    let now = label == "now";
    let when = ui.painter().layout_no_wrap(
        label.to_string(),
        if now { ty::small_strong() } else { ty::small() },
        if now { p.accent } else { p.dim },
    );
    let join = ui.painter().layout_no_wrap("Join".into(), ty::small_strong(), p.on_accent);
    let button_w = join.size().x + 16.0 + 26.0;
    let width = 12.0 + name.size().x + 8.0 + when.size().x + 10.0 + button_w + 4.0;
    let rect = Rect::from_min_size(pos2(right - width, cy - PILL_HEIGHT / 2.0), vec2(width, PILL_HEIGHT));
    ui.painter().rect_filled(rect, CornerRadius::same(12), p.panel);
    let mut x = rect.left() + 12.0;
    let nw = name.size().x;
    ui.painter().galley(pos2(x, cy - name.size().y / 2.0), name, p.text);
    x += nw + 8.0;
    let ww = when.size().x;
    let color = if now { p.accent } else { p.dim };
    ui.painter().galley(pos2(x, cy - when.size().y / 2.0), when, color);
    x += ww + 10.0;
    let button = Rect::from_min_size(pos2(x, rect.top() + 4.0), vec2(button_w, rect.height() - 8.0));
    let response = ui
        .interact(button, ui.scope_id().with("join"), Sense::click())
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text("Join in Teams on the web");
    ui.painter().rect_filled(button, CornerRadius::same(9), if response.hovered() { p.accent } else { p.accent_fill });
    icons::paint(ui.painter(), Rect::from_min_size(pos2(button.left() + 10.0, cy - 7.5), vec2(15.0, 15.0)), Icon::Video, p.on_accent);
    ui.painter().galley(pos2(button.left() + 31.0, cy - join.size().y / 2.0), join, p.on_accent);
    if response.clicked() {
        ui.ctx().open_url(egui::OpenUrl::new_tab(crate::teams::web_join(url)));
    }
    rect.left()
}

/// When a meeting is, said briefly: "now", "in 12 min", or its time.
pub fn when(start: i64, end: i64) -> String {
    let now = time::now_ms();
    if start <= now && now < end {
        "now".into()
    } else if start > now && start - now < 60 * 60_000 {
        format!("in {} min", ((start - now) / 60_000).max(1))
    } else {
        time::clock(start)
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max { s.to_string() } else { s.chars().take(max - 1).collect::<String>() + "…" }
}

/// The user's avatar, with a menu: who's signed in, the theme, signing out.
/// A hairline across the menu, with room around it.
fn divider(ui: &mut Ui) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 9.0), Sense::hover());
    ui.painter().hline(rect.x_range(), rect.center().y, Stroke::new(1.0, colors().line));
}

/// A small heading in the menu.
fn menu_label(ui: &mut Ui, text: &str) {
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.add_space(8.0);
        ui.label(RichText::new(text.to_uppercase()).font(ty::section()).color(colors().faint));
    });
    ui.add_space(8.0);
}

fn user_menu(ui: &mut Ui, ws: &mut Workspace, rect: Rect, theme: Appearance) -> Option<Request> {
    let p = colors();
    let me = Picture::Person { id: ws.me_id.clone(), name: ws.me.clone() };
    let picture = if ws.me_id.is_empty() { None } else { ws.pictures.get(&me) };
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect));
    let response = widgets::avatar(&mut child, &ws.me_id, &ws.me, rect.width(), picture)
        .interact(Sense::click())
        .on_hover_cursor(egui::CursorIcon::PointingHand);
    let mut request = None;
    egui::Popup::menu(&response)
        .align(egui::RectAlign::BOTTOM_END)
        .gap(8.0)
        .width(290.0)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        // DRIFT_DEMO_MENU: the menu open.
        .open_memory(std::env::var_os("DRIFT_DEMO_MENU").is_some().then_some(egui::SetOpenCommand::Bool(true)))
        .show(|ui| {
            ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
            // Who's signed in.
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.add_space(6.0);
                let picture = if ws.me_id.is_empty() { None } else { ws.pictures.get(&me) };
                widgets::avatar(ui, &ws.me_id, &ws.me, 40.0, picture);
                ui.add_space(12.0);
                ui.vertical(|ui| {
                    ui.add_space(3.0);
                    ui.label(RichText::new(&ws.me).font(ty::item()).color(p.text));
                    ui.add_space(2.0);
                    ui.label(RichText::new("Signed in to Microsoft Teams").font(ty::meta()).color(p.faint));
                });
            });
            ui.add_space(12.0);
            divider(ui);

            // Appearance, as three segments.
            menu_label(ui, "Appearance");
            let options = [(Appearance::System, "System"), (Appearance::Light, "Light"), (Appearance::Dark, "Dark")];
            let (track, _) = ui.allocate_exact_size(vec2(ui.available_width(), 34.0), Sense::hover());
            let track = track.shrink2(vec2(6.0, 0.0));
            ui.painter().rect_filled(track, CornerRadius::same(10), p.surface);
            let w = track.width() / options.len() as f32;
            for (i, (option, label)) in options.into_iter().enumerate() {
                let cell = Rect::from_min_size(pos2(track.left() + w * i as f32, track.top()), vec2(w, track.height())).shrink(3.0);
                let response = ui.interact(cell, ui.scope_id().with(("appearance", i)), Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
                let on = theme == option;
                if on {
                    ui.painter().rect(cell, CornerRadius::same(8), p.panel, Stroke::new(1.0, p.border), egui::StrokeKind::Inside);
                } else if response.hovered() {
                    ui.painter().rect_filled(cell, CornerRadius::same(8), p.hover);
                }
                let font = if on { ty::small_strong() } else { ty::small() };
                ui.painter().text(cell.center(), egui::Align2::CENTER_CENTER, label, font, if on { p.text } else { p.dim });
                if response.clicked() {
                    request = Some(Request::Theme(option));
                }
            }
            ui.add_space(12.0);
            divider(ui);

            // Notifications, as a switch.
            let (row, response) = ui.allocate_exact_size(vec2(ui.available_width(), 46.0), Sense::click());
            let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
            if response.hovered() {
                ui.painter().rect_filled(row.shrink2(vec2(2.0, 2.0)), CornerRadius::same(8), p.hover);
            }
            ui.painter().text(pos2(row.left() + 10.0, row.top() + 15.0), egui::Align2::LEFT_CENTER, "Notifications", ty::small_strong(), p.text);
            ui.painter().text(
                pos2(row.left() + 10.0, row.top() + 32.0),
                egui::Align2::LEFT_CENTER,
                "For new messages, when drift is behind",
                ty::meta(),
                p.faint,
            );
            let switch = Rect::from_min_size(pos2(row.right() - 10.0 - 34.0, row.center().y - 10.0), vec2(34.0, 20.0));
            let t = ui.ctx().animate_bool_responsive(ui.scope_id().with("notifications-switch"), ws.notifications);
            ui.painter().rect_filled(switch, CornerRadius::same(10), p.border.lerp_to_gamma(p.accent_fill, t));
            let knob = pos2(egui::lerp(switch.left() + 10.0..=switch.right() - 10.0, t), switch.center().y);
            ui.painter().circle_filled(knob, 7.5, egui::Color32::WHITE);
            if response.clicked() {
                request = Some(Request::Notifications(!ws.notifications));
            }
            ui.add_space(4.0);
            divider(ui);

            // This version, and whether there's a newer one.
            let (row, _) = ui.allocate_exact_size(vec2(ui.available_width(), 46.0), Sense::hover());
            ui.painter().text(
                pos2(row.left() + 10.0, row.top() + 15.0),
                egui::Align2::LEFT_CENTER,
                format!("drift {}", crate::update::current()),
                ty::small_strong(),
                p.text,
            );
            let (status, color) = match &ws.update_found {
                _ if ws.update.is_running() => ("Checking…".to_string(), p.faint),
                Some(Ok(crate::update::Found::Current)) => ("You have the latest version".into(), p.faint),
                Some(Ok(crate::update::Found::NoReleases)) => ("No releases published yet".into(), p.faint),
                Some(Ok(crate::update::Found::Newer { version, .. })) => (format!("Version {version} is available"), p.accent),
                Some(Err(_)) => ("Couldn't check for updates".into(), p.danger),
                None => ("Free and open source".into(), p.faint),
            };
            ui.painter().text(pos2(row.left() + 10.0, row.top() + 32.0), egui::Align2::LEFT_CENTER, status, ty::meta(), color);
            let (label, newer) = match &ws.update_found {
                Some(Ok(crate::update::Found::Newer { page, .. })) => ("Download", Some(page.clone())),
                _ => ("Check", None),
            };
            let text_color = if newer.is_some() { p.on_accent } else { p.text };
            let galley = ui.painter().layout_no_wrap(label.into(), ty::small_strong(), text_color);
            let button = Rect::from_min_size(pos2(row.right() - 10.0 - galley.size().x - 24.0, row.center().y - 14.0), vec2(galley.size().x + 24.0, 28.0));
            let click = ui.interact(button, ui.scope_id().with("update"), Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
            let fill = if newer.is_some() { p.accent_fill } else { p.surface };
            ui.painter().rect_filled(button, CornerRadius::same(8), if click.hovered() { p.hover.lerp_to_gamma(fill, 0.5) } else { fill });
            ui.painter().galley(button.center() - galley.size() / 2.0, galley, text_color);
            if click.clicked() {
                match newer {
                    Some(page) => ui.ctx().open_url(egui::OpenUrl::new_tab(page)),
                    None if !ws.update.is_running() => ws.check_for_update(ui.ctx()),
                    None => {}
                }
            }
            ui.add_space(4.0);
            divider(ui);

            // Signing out.
            let (row, response) = ui.allocate_exact_size(vec2(ui.available_width(), 36.0), Sense::click());
            let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
            if response.hovered() {
                ui.painter().rect_filled(row.shrink2(vec2(2.0, 2.0)), CornerRadius::same(8), p.hover);
            }
            ui.painter().text(pos2(row.left() + 10.0, row.center().y), egui::Align2::LEFT_CENTER, "Sign out", ty::small_strong(), p.danger);
            if response.clicked() {
                request = Some(Request::SignOut);
            }
            ui.add_space(2.0);
        });
    request
}
