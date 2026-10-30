//! The icon rail on the far left: Activity, Chats, Teams and Meetings, with counts
//! of what's unread, and the user's avatar (theme, sign out) at the bottom.

use crate::model::{Directory, Kind, Picture};
use crate::settings::Appearance;
use crate::ui::app::Request;
use crate::ui::style::icons::{self, Icon};
use crate::ui::style::{colors, metrics, typography as ty};
use crate::ui::widgets;
use crate::ui::workspace::{Section, Workspace};
use eframe::egui::{self, CornerRadius, Rect, RichText, Sense, Stroke, Ui, pos2, vec2};

pub fn show(ui: &mut Ui, ws: &mut Workspace, theme: Appearance) -> Option<Request> {
    let p = colors();
    let full = ui.max_rect();
    super::drag_area(ui, Rect::from_min_size(full.min, vec2(full.width(), metrics::TITLE_BAR)), "rail-drag");
    ui.painter().vline(full.right() - 0.5, full.y_range(), Stroke::new(1.0, p.line));

    let counts = ws.directory.as_ref().map(unread_counts).unwrap_or_default();
    let mut y = full.top() + metrics::TITLE_BAR;
    for (section, icon, hint, count) in [
        (Section::Activity, Icon::At, "Activity", counts.0),
        (Section::Chats, Icon::Chat, "Chats", counts.1),
        (Section::Teams, Icon::Members, "Teams", 0),
        (Section::Meetings, Icon::Calendar, "Meetings", 0),
    ] {
        let rect =
            Rect::from_min_size(pos2(full.center().x - metrics::RAIL_BUTTON / 2.0, y), vec2(metrics::RAIL_BUTTON, metrics::RAIL_BUTTON));
        if button(ui, rect, icon, hint, ws.section == section, count).clicked() {
            ws.section = section;
            if section == Section::Activity
                && let Some(id) = ws.directory.as_ref().and_then(|d| d.activity()).map(|c| c.id.clone())
            {
                ws.open(&id);
            }
        }
        y += metrics::RAIL_BUTTON + 4.0;
    }

    // The user, at the bottom: a menu with the theme and signing out.
    let size = 32.0;
    let rect = Rect::from_min_size(pos2(full.center().x - size / 2.0, full.bottom() - size - 14.0), vec2(size, size));
    let me = Picture::Person { id: ws.me_id.clone(), name: ws.me.clone() };
    let picture = if ws.me_id.is_empty() { None } else { ws.pictures.get(&me) };
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect));
    let response = widgets::avatar(&mut child, &ws.me_id, &ws.me, size, picture)
        .interact(Sense::click())
        .on_hover_cursor(egui::CursorIcon::PointingHand);
    let mut request = None;
    egui::Popup::menu(&response).align(egui::RectAlign::RIGHT_END).gap(10.0).width(200.0).show(|ui| {
        ui.label(RichText::new(&ws.me).font(ty::item_strong()).color(p.text));
        ui.separator();
        for option in [Appearance::System, Appearance::Light, Appearance::Dark] {
            let label = match option {
                Appearance::System => "Follow the system",
                Appearance::Light => "Light",
                Appearance::Dark => "Dark",
            };
            if ui.selectable_label(theme == option, label).clicked() {
                request = Some(Request::Theme(option));
            }
        }
        ui.separator();
        if ui.button("Sign out").clicked() {
            request = Some(Request::SignOut);
        }
    });
    request
}

/// Unread activity, and unread chats (direct messages and meeting chats).
fn unread_counts(d: &Directory) -> (usize, usize) {
    let activity = d.conversations.iter().filter(|c| c.kind == Kind::Feed && c.unread).count();
    let chats = d.conversations.iter().filter(|c| matches!(c.kind, Kind::Chat | Kind::Meeting) && c.unread && !c.muted).count();
    (activity, chats)
}

/// A rail button: the icon on a rounded square, accent while it's the section shown,
/// a count in the corner.
fn button(ui: &mut Ui, rect: Rect, icon: Icon, hint: &str, on: bool, count: usize) -> egui::Response {
    let p = colors();
    let response = ui.interact(rect, ui.scope_id().with(hint), Sense::click()).on_hover_text(hint);
    if on || response.hovered() {
        ui.painter().rect_filled(rect, CornerRadius::same(12), if on { p.selected } else { p.hover });
    }
    let color = if on { p.accent } else { p.dim };
    icons::paint(ui.painter(), Rect::from_center_size(rect.center(), vec2(20.0, 20.0)), icon, color);
    if count > 0 {
        let text = if count > 99 { "99+".to_string() } else { count.to_string() };
        let galley = ui.painter().layout_no_wrap(text, ty::badge(), p.on_accent);
        let w = (galley.size().x + 8.0).max(17.0);
        let badge = Rect::from_min_size(pos2(rect.right() - w + 4.0, rect.top() + 5.0), vec2(w, 17.0));
        ui.painter().rect_filled(badge, CornerRadius::same(9), p.accent_fill);
        ui.painter().galley(badge.center() - galley.size() / 2.0, galley, p.on_accent);
    }
    response
}
