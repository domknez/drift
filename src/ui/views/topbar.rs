//! The top bar: the wordmark, the tabs (Activity, Chats, Teams, Meetings) with what's
//! unread, search, today's meeting with Join, and the user's menu.

use crate::model::{Availability, Conversation, Directory, Kind, Picture, Status, time};
use crate::settings::{Accent, Appearance, Look, TEXT_SIZES, Theme};
use crate::ui::app::Request;
use crate::ui::style::icons::{self, Icon};
use crate::ui::style::palette;
use crate::ui::style::{colors, metrics, typography as ty};
use crate::ui::updates::Updates;
use crate::ui::widgets;
use crate::ui::workspace::{Section, Workspace};
use eframe::egui::{self, Align2, CornerRadius, Frame, Margin, Rect, RichText, Sense, Stroke, Ui, pos2, vec2};

const PILL_HEIGHT: f32 = 36.0;

pub fn show(ui: &mut Ui, ws: &mut Workspace, look: Look, updates: &Updates) -> Option<Request> {
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
    // A count where each item matters (activity, chats); a dot for channels, as
    // there are many and most are only looked in on.
    let counts = (ws.activity_unread(), ws.directory.as_ref().map_or(0, unread_chats));
    let channels = ws.directory.as_ref().is_some_and(unread_channels);
    let tabs = [
        (Section::Activity, Icon::Bell, "Activity", Badge::Count(counts.0)),
        (Section::Chats, Icon::Chat, "Chats", Badge::Count(counts.1)),
        (Section::Teams, Icon::Members, "Teams", Badge::Dot(channels)),
        (Section::Meetings, Icon::Calendar, "Calendar", Badge::Count(0)),
    ];
    let widths: Vec<f32> = tabs.iter().map(|t| tab_width(ui, t.2, t.3)).collect();
    let group = Rect::from_min_size(
        pos2(x, cy - PILL_HEIGHT / 2.0 - 1.0),
        vec2(widths.iter().sum::<f32>() + 8.0 + 4.0 * (tabs.len() - 1) as f32, PILL_HEIGHT + 2.0),
    );
    ui.painter().rect_filled(group, CornerRadius::same(12), p.panel);
    let mut tx = group.left() + 4.0;
    for (i, ((section, icon, label, badge), w)) in tabs.into_iter().zip(widths).enumerate() {
        let rect = Rect::from_min_size(pos2(tx, group.top() + 4.0), vec2(w, group.height() - 8.0));
        let number = [egui::Key::Num1, egui::Key::Num2, egui::Key::Num3, egui::Key::Num4][i.min(3)];
        let keys = ui.ctx().format_shortcut(&egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, number));
        if tab(ui, rect, icon, label, badge, ws.section == section).on_hover_text(keys).clicked() {
            ws.section = section;
            ws.filter.clear();
        }
        tx += w + 4.0;
    }
    x = group.right() + 16.0;

    // The user's menu at the far right, a new version before it, then today's meeting.
    let size = 32.0;
    let avatar = Rect::from_min_size(pos2(full.right() - 16.0 - size, cy - size / 2.0), vec2(size, size));
    let mut request = user_menu(ui, ws, avatar, look, updates);
    let mut right = avatar.left() - 12.0;
    if let Some(offer) = updates.offer() {
        let (left, clicked) = update_pill(ui, &offer, right, cy);
        if clicked {
            request = Some(Request::InstallUpdate);
        }
        right = left - 12.0;
    }
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

/// Whether a channel listed under Teams has something unread (not counting muted
/// ones). Read from the channels' conversations, as the list marks them: the
/// roster's own unread flag can disagree.
fn unread_channels(d: &Directory) -> bool {
    d.teams.iter().flat_map(|t| &t.channels).filter_map(|c| d.conversation(&c.id)).any(|c| c.unread && !c.muted)
}

/// What a tab shows of what's unread.
#[derive(Clone, Copy)]
enum Badge {
    Count(usize),
    Dot(bool),
}

/// Room for a dot after the label.
const DOT_ROOM: f32 = 12.0;

fn count_text(count: usize) -> String {
    if count > 99 { "99+".into() } else { count.to_string() }
}

fn tab_width(ui: &Ui, label: &str, badge: Badge) -> f32 {
    let text = ui.painter().layout_no_wrap(label.to_string(), ty::tab(), colors().text).size().x;
    let badge = match badge {
        Badge::Count(count) if count > 0 => ui.painter().layout_no_wrap(count_text(count), ty::badge(), colors().text).size().x + 18.0,
        Badge::Dot(true) => DOT_ROOM,
        _ => 0.0,
    };
    14.0 + 16.0 + 7.0 + text + badge + 14.0
}

/// One tab: icon, label, and a count or a dot; filled with the accent when it's the one shown.
fn tab(ui: &mut Ui, rect: Rect, icon: Icon, label: &str, badge: Badge, on: bool) -> egui::Response {
    let p = colors();
    let response = ui.interact(rect, ui.scope_id().with(("tab", label)), Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
    widgets::control(ui, &response, egui::Role::Tab, label);
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
    if let Badge::Dot(true) = badge {
        ui.painter().circle_filled(pos2(x + 3.5, rect.center().y), 3.5, if on { p.on_accent } else { p.accent });
    }
    if let Badge::Count(count) = badge
        && count > 0
    {
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
        let keys = ui.ctx().format_shortcut(&egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::K));
        ui.painter().text(pos2(right - 2.0, rect.center().y), Align2::RIGHT_CENTER, keys, ty::mono_small(), p.faint);
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

/// A new version, ending at `right`: "Restart to update" (in the accent), or the
/// version that's out. Returns its left edge, and whether it was clicked.
fn update_pill(ui: &mut Ui, offer: &crate::ui::updates::Offer, right: f32, cy: f32) -> (f32, bool) {
    let p = colors();
    let text = if offer.installs { p.on_accent } else { p.accent };
    let label = ui.painter().layout_no_wrap(offer.label.clone(), ty::small_strong(), text);
    let width = 12.0 + 15.0 + 8.0 + label.size().x + 14.0;
    let rect = Rect::from_min_size(pos2(right - width, cy - PILL_HEIGHT / 2.0), vec2(width, PILL_HEIGHT));
    let response = ui
        .interact(rect, ui.scope_id().with("update"), Sense::click())
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(&offer.hint);
    let fill = match (offer.installs, response.hovered()) {
        (true, true) => p.accent,
        (true, false) => p.accent_fill,
        (false, true) => p.hover,
        (false, false) => p.panel,
    };
    ui.painter().rect_filled(rect, CornerRadius::same(12), fill);
    icons::paint(ui.painter(), Rect::from_min_size(pos2(rect.left() + 12.0, cy - 7.5), vec2(15.0, 15.0)), Icon::Download, text);
    ui.painter().galley(pos2(rect.left() + 35.0, cy - label.size().y / 2.0), label, text);
    (rect.left(), response.clicked())
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

/// A row of the menu with a switch: its name, a line under it, and whether it's on.
/// Returns whether it was clicked.
fn switch_row(ui: &mut Ui, name: &str, detail: &str, on: bool) -> bool {
    let p = colors();
    let (row, response) = ui.allocate_exact_size(vec2(ui.available_width(), 46.0), Sense::click());
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    if response.hovered() {
        ui.painter().rect_filled(row.shrink2(vec2(2.0, 2.0)), CornerRadius::same(8), p.hover);
    }
    ui.painter().text(pos2(row.left() + 10.0, row.top() + 15.0), egui::Align2::LEFT_CENTER, name, ty::small_strong(), p.text);
    ui.painter().text(pos2(row.left() + 10.0, row.top() + 32.0), egui::Align2::LEFT_CENTER, detail, ty::meta(), p.faint);
    let switch = Rect::from_min_size(pos2(row.right() - 10.0 - 34.0, row.center().y - 10.0), vec2(34.0, 20.0));
    let t = ui.ctx().animate_bool_responsive(ui.scope_id().with((name, "switch")), on);
    ui.painter().rect_filled(switch, CornerRadius::same(10), p.border.lerp_to_gamma(p.accent_fill, t));
    let knob = pos2(egui::lerp(switch.left() + 10.0..=switch.right() - 10.0, t), switch.center().y);
    ui.painter().circle_filled(knob, 7.5, egui::Color32::WHITE);
    response.clicked()
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

fn user_menu(ui: &mut Ui, ws: &mut Workspace, rect: Rect, look: Look, updates: &Updates) -> Option<Request> {
    let p = colors();
    let me = Picture::Person { id: ws.me_id.clone(), name: ws.me.clone() };
    let picture = if ws.me_id.is_empty() { None } else { ws.pictures.get(&me) };
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect));
    let response = widgets::avatar(&mut child, &ws.me_id, &ws.me, rect.width(), picture)
        .interact(Sense::click())
        .on_hover_cursor(egui::CursorIcon::PointingHand);
    if let Some(status) = ws.status {
        widgets::status_dot(ui.painter(), rect.right_bottom() - vec2(4.0, 4.0), 5.0, status.shown, p.canvas);
    }
    let mut request = None;
    let mut popup = egui::Popup::menu(&response)
        .align(egui::RectAlign::BOTTOM_END)
        .gap(8.0)
        .width(290.0)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside);
    // DRIFT_DEMO_MENU: the menu open. (Only then: setting it replaces the click that opens it.)
    if std::env::var_os("DRIFT_DEMO_MENU").is_some() {
        popup = popup.open_memory(egui::SetOpenCommand::Bool(true));
    }
    popup.show(|ui| {
        ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
        // Who's signed in.
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.add_space(6.0);
            let picture = if ws.me_id.is_empty() { None } else { ws.pictures.get(&me) };
            let face = widgets::avatar(ui, &ws.me_id, &ws.me, 40.0, picture).rect;
            if let Some(status) = ws.status {
                widgets::status_dot(ui.painter(), face.right_bottom() - vec2(5.0, 5.0), 6.0, status.shown, p.panel);
            }
            ui.add_space(12.0);
            ui.vertical(|ui| {
                ui.add_space(3.0);
                ui.label(RichText::new(&ws.me).font(ty::item()).color(p.text));
                ui.add_space(2.0);
                let line = match ws.status {
                    Some(Status { shown, set: Some(set) }) if set == shown => format!("{} (set by you)", shown.label()),
                    Some(Status { shown, set: Some(set) }) => format!("{}, set to {}", shown.label(), set.label()),
                    Some(s) => s.shown.label().to_string(),
                    None => "Signed in to Microsoft Teams".into(),
                };
                ui.label(RichText::new(line).font(ty::meta()).color(p.faint));
            });
        });
        ui.add_space(12.0);
        divider(ui);

        // Status: what to show as, and going back to Teams working it out.
        if let Some(status) = ws.status {
            menu_label(ui, "Status");
            let picks = Availability::PICKS.into_iter().map(Some);
            for pick in picks.chain(status.set.is_some().then_some(None)) {
                let (row, response) = ui.allocate_exact_size(vec2(ui.available_width(), 30.0), Sense::click());
                let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
                if response.hovered() {
                    ui.painter().rect_filled(row.shrink2(vec2(2.0, 1.0)), CornerRadius::same(8), p.hover);
                }
                let text_x = row.left() + 34.0;
                match pick {
                    Some(a) => {
                        widgets::status_dot(ui.painter(), pos2(row.left() + 18.0, row.center().y), 5.0, a, p.panel);
                        ui.painter().text(pos2(text_x, row.center().y), Align2::LEFT_CENTER, a.pick_label(), ty::small(), p.text);
                    }
                    None => {
                        let label = "Reset status";
                        ui.painter().text(pos2(text_x, row.center().y), Align2::LEFT_CENTER, label, ty::small(), p.dim);
                    }
                }
                if pick.is_some() && pick == status.set {
                    let mark = Rect::from_center_size(pos2(row.right() - 18.0, row.center().y), vec2(14.0, 14.0));
                    icons::paint(ui.painter(), mark, Icon::Check, p.accent);
                }
                if response.clicked() && pick != status.set {
                    ws.set_status(ui.ctx(), pick);
                }
            }
            ui.add_space(8.0);
            divider(ui);
        }

        // Appearance: light, dark or as the system is; the themes; the accent; the size.
        menu_label(ui, "Appearance");
        let modes = [(Appearance::System, "System"), (Appearance::Light, "Light"), (Appearance::Dark, "Dark")];
        if let Some(mode) = segments(ui, "appearance", &modes, look.mode) {
            request = Some(Request::Look(Look { mode, ..look }));
        }
        ui.add_space(10.0);
        let dark_now = ui.visuals().dark_mode;
        ui.horizontal(|ui| {
            ui.add_space(6.0);
            ui.spacing_mut().item_spacing.x = 6.0;
            for theme in Theme::ALL.iter().copied() {
                // The one in use, and the other kind's pick (used when drift is that kind).
                let picked = theme == if theme.dark() { look.dark } else { look.light };
                let in_use = picked && theme.dark() == dark_now;
                if theme_swatch(ui, theme, picked, in_use).clicked() {
                    // Picking the other kind's theme switches to it too.
                    let mut to = if theme.dark() { Look { dark: theme, ..look } } else { Look { light: theme, ..look } };
                    if theme.dark() != dark_now {
                        to.mode = if theme.dark() { Appearance::Dark } else { Appearance::Light };
                    }
                    request = Some(Request::Look(to));
                }
            }
        });
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            ui.add_space(8.0);
            ui.spacing_mut().item_spacing.x = 8.0;
            for accent in Accent::ALL.iter().copied() {
                let (dot, response) = ui.allocate_exact_size(vec2(22.0, 22.0), Sense::click());
                let response = response.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(accent.label());
                widgets::control(ui, &response, egui::Role::RadioButton, &format!("{} accent", accent.label()));
                ui.painter().circle_filled(dot.center(), 9.0, palette::accent_swatch(accent, dark_now));
                if accent == look.accent {
                    ui.painter().circle_stroke(dot.center(), 11.0, Stroke::new(1.5, p.text));
                } else if response.hovered() {
                    ui.painter().circle_stroke(dot.center(), 11.0, Stroke::new(1.0, p.border));
                }
                if response.clicked() {
                    request = Some(Request::Look(Look { accent, ..look }));
                }
            }
        });
        ui.add_space(10.0);
        let sizes: Vec<(u16, String)> = TEXT_SIZES.iter().map(|s| (*s, format!("{s}%"))).collect();
        let sizes: Vec<(u16, &str)> = sizes.iter().map(|(s, l)| (*s, l.as_str())).collect();
        if let Some(text_size) = segments(ui, "text-size", &sizes, look.text_size).filter(|s| *s != look.text_size) {
            request = Some(Request::Look(Look { text_size, ..look }));
        }
        ui.add_space(12.0);
        divider(ui);

        if switch_row(ui, "Notifications", "For new messages, when drift is behind", ws.notifications) {
            request = Some(Request::Notifications(!ws.notifications));
        }
        ui.add_space(4.0);
        divider(ui);

        // This version, and whether there's a newer one.
        let (row, _) = ui.allocate_exact_size(vec2(ui.available_width(), 46.0), Sense::hover());
        ui.painter().text(
            pos2(row.left() + 10.0, row.top() + 15.0),
            egui::Align2::LEFT_CENTER,
            format!("drift {}", crate::update::CURRENT),
            ty::small_strong(),
            p.text,
        );
        let color = match updates.notable() {
            Some(true) => p.accent,
            Some(false) => p.danger,
            None => p.faint,
        };
        ui.painter().text(pos2(row.left() + 10.0, row.top() + 32.0), egui::Align2::LEFT_CENTER, updates.status(), ty::meta(), color);
        let (label, on_click, strong) = match updates.offer() {
            Some(offer) if offer.installs => ("Restart", Request::InstallUpdate, true),
            Some(_) => ("Download", Request::InstallUpdate, true),
            None => ("Check", Request::CheckForUpdates, false),
        };
        let text_color = if strong { p.on_accent } else { p.text };
        let galley = ui.painter().layout_no_wrap(label.into(), ty::small_strong(), text_color);
        let button = Rect::from_min_size(
            pos2(row.right() - 10.0 - galley.size().x - 24.0, row.center().y - 14.0),
            vec2(galley.size().x + 24.0, 28.0),
        );
        let click = ui.interact(button, ui.scope_id().with("update"), Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
        let fill = if strong { p.accent_fill } else { p.surface };
        ui.painter().rect_filled(button, CornerRadius::same(8), if click.hovered() { p.hover.lerp_to_gamma(fill, 0.5) } else { fill });
        ui.painter().galley(button.center() - galley.size() / 2.0, galley, text_color);
        if click.clicked() && !updates.checking() {
            request = Some(on_click);
        }
        if switch_row(ui, "Update automatically", "Installs new versions when drift restarts", updates.auto) {
            request = Some(Request::AutoUpdate(!updates.auto));
        }
        ui.add_space(4.0);
        divider(ui);

        // The keyboard shortcuts, in a sheet.
        let (row, response) = ui.allocate_exact_size(vec2(ui.available_width(), 32.0), Sense::click());
        let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
        widgets::control(ui, &response, egui::Role::Button, "Keyboard shortcuts");
        if response.hovered() {
            ui.painter().rect_filled(row.shrink2(vec2(2.0, 2.0)), CornerRadius::same(8), p.hover);
        }
        ui.painter().text(pos2(row.left() + 10.0, row.center().y), egui::Align2::LEFT_CENTER, "Keyboard shortcuts", ty::small(), p.dim);
        let keys = ui.ctx().format_shortcut(&egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::Slash));
        ui.painter().text(pos2(row.right() - 10.0, row.center().y), egui::Align2::RIGHT_CENTER, keys, ty::meta(), p.faint);
        if response.clicked() {
            ws.shortcuts = true;
            ui.close();
        }
        divider(ui);

        // Signing out.
        let (row, response) = ui.allocate_exact_size(vec2(ui.available_width(), 36.0), Sense::click());
        let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
        if response.hovered() {
            ui.painter().rect_filled(row.shrink2(vec2(2.0, 2.0)), CornerRadius::same(8), p.hover);
        }
        // A second click within a few seconds signs out (it means signing in again,
        // with the browser and MFA).
        let armed_id = egui::Id::unique("sign-out-armed");
        let now = ui.input(|i| i.time);
        let armed = ui.data(|d| d.get_temp::<f64>(armed_id)).is_some_and(|at| now - at < 4.0);
        let label = if armed { "Click again to sign out" } else { "Sign out" };
        ui.painter().text(pos2(row.left() + 10.0, row.center().y), egui::Align2::LEFT_CENTER, label, ty::small_strong(), p.danger);
        if armed {
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(500));
        }
        if response.clicked() {
            if armed {
                ui.data_mut(|d| d.remove::<f64>(armed_id));
                request = Some(Request::SignOut);
            } else {
                ui.data_mut(|d| d.insert_temp(armed_id, now));
            }
        }
        ui.add_space(2.0);
    });
    request
}

/// Options side by side in a track, the picked one raised; returns the one clicked.
fn segments<T: Copy + PartialEq>(ui: &mut Ui, salt: &str, options: &[(T, &str)], picked: T) -> Option<T> {
    let p = colors();
    let mut clicked = None;
    let (track, _) = ui.allocate_exact_size(vec2(ui.available_width(), 34.0), Sense::hover());
    let track = track.shrink2(vec2(6.0, 0.0));
    ui.painter().rect_filled(track, CornerRadius::same(10), p.surface);
    let w = track.width() / options.len() as f32;
    for (i, (option, label)) in options.iter().enumerate() {
        let cell = Rect::from_min_size(pos2(track.left() + w * i as f32, track.top()), vec2(w, track.height())).shrink(3.0);
        let response = ui.interact(cell, ui.scope_id().with((salt, i)), Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
        widgets::control(ui, &response, egui::Role::RadioButton, label);
        let on = *option == picked;
        if on {
            ui.painter().rect(cell, CornerRadius::same(8), p.panel, Stroke::new(1.0, p.border), egui::StrokeKind::Inside);
        } else if response.hovered() {
            ui.painter().rect_filled(cell, CornerRadius::same(8), p.hover);
        }
        let font = if on { ty::small_strong() } else { ty::small() };
        ui.painter().text(cell.center(), egui::Align2::CENTER_CENTER, *label, font, if on { p.text } else { p.dim });
        if response.clicked() {
            clicked = Some(*option);
        }
    }
    clicked
}

/// A theme in small: its canvas with a panel and two lines of text on it.
fn theme_swatch(ui: &mut Ui, theme: Theme, picked: bool, in_use: bool) -> egui::Response {
    let p = colors();
    let (rect, response) = ui.allocate_exact_size(vec2(48.0, 34.0), Sense::click());
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(theme.label());
    widgets::control(ui, &response, egui::Role::RadioButton, &format!("{} theme", theme.label()));
    let (canvas, panel, text) = palette::swatch(theme);
    let painter = ui.painter();
    painter.rect_filled(rect, CornerRadius::same(8), canvas);
    let card = Rect::from_min_max(rect.min + vec2(7.0, 7.0), rect.max - vec2(7.0, 7.0));
    painter.rect_filled(card, CornerRadius::same(4), panel);
    for (y, w) in [(card.top() + 6.0, 0.7), (card.top() + 12.0, 0.45)] {
        let line = Rect::from_min_size(pos2(card.left() + 5.0, y), vec2((card.width() - 10.0) * w, 2.5));
        painter.rect_filled(line, CornerRadius::same(1), text.gamma_multiply(0.6));
    }
    let ring = if in_use {
        Stroke::new(2.0, p.accent)
    } else if picked {
        Stroke::new(1.5, p.dim)
    } else if response.hovered() {
        Stroke::new(1.0, p.dim)
    } else {
        Stroke::new(1.0, p.border)
    };
    painter.rect_stroke(rect, CornerRadius::same(8), ring, egui::StrokeKind::Outside);
    response
}
