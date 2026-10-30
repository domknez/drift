//! The sidebar: search, a nav row for Activity, teams with their channels, direct
//! messages, meeting chats, and the account row at the bottom.

use crate::model::{Conversation, Directory, Kind, Picture};
use crate::settings::Appearance;
use crate::ui::app::Request;
use crate::ui::style::icons::Icon;
use crate::ui::style::{colors, metrics, typography as ty};
use crate::ui::widgets;
use crate::ui::workspace::Workspace;
use eframe::egui::{self, Align, CornerRadius, Layout, Margin, Rect, RichText, Sense, Stroke, TextureId, Ui, vec2};

/// The account row at the bottom.
const FOOTER: f32 = 58.0;
/// Meeting chats listed when unfolded (the newest).
const MEETINGS_SHOWN: usize = 12;

pub fn show(ui: &mut Ui, ws: &mut Workspace, theme: Appearance) -> Option<Request> {
    let p = colors();
    let mut request = None;
    let top = Rect::from_min_size(ui.max_rect().min, vec2(ui.max_rect().width(), metrics::TITLE_BAR));
    super::drag_area(ui, top, "sidebar-drag");
    ui.add_space(metrics::TITLE_BAR);

    egui::Frame::new().inner_margin(Margin::symmetric(12, 0)).show(ui, |ui| {
        ui.horizontal(|ui| {
            let field = Rect::from_min_size(ui.cursor().min, vec2(ui.available_width(), 34.0));
            ui.painter().rect(field, CornerRadius::same(9), p.surface, Stroke::new(1.0, p.line), egui::StrokeKind::Inside);
            ui.add_space(10.0);
            widgets::icon(ui, Icon::Search, 15.0, p.faint);
            ui.add_space(6.0);
            let search = egui::TextEdit::singleline(&mut ws.filter)
                .hint_text(RichText::new("Search chats and channels").color(p.faint))
                .font(ty::item())
                .frame(egui::Frame::NONE)
                .margin(Margin::symmetric(0, 9))
                .desired_width(ui.available_width() - 12.0);
            ui.add(search);
        });
    });
    ui.add_space(6.0);

    let list_height = (ui.available_height() - FOOTER).max(0.0);
    egui::ScrollArea::vertical().max_height(list_height).auto_shrink([false, false]).show(ui, |ui| {
        ui.add_space(2.0);
        match ws.directory.clone() {
            Some(directory) => list(ui, ws, &directory),
            None => {
                ui.add_space(20.0);
                ui.vertical_centered(|ui| {
                    ui.spinner();
                    ui.label(RichText::new("Loading your teams and chats…").font(ty::small()).color(p.faint));
                });
            }
        }
        ui.add_space(12.0);
    });

    request = request.or(footer(ui, ws, theme));
    request
}

fn footer(ui: &mut Ui, ws: &mut Workspace, theme: Appearance) -> Option<Request> {
    let p = colors();
    let mut request = None;
    let full = ui.max_rect();
    let rect = Rect::from_min_max(egui::pos2(full.left(), full.bottom() - FOOTER), full.max);
    ui.painter().hline(rect.x_range(), rect.top(), Stroke::new(1.0, p.line));
    let mut row = ui.new_child(egui::UiBuilder::new().max_rect(rect.shrink2(vec2(16.0, 0.0))).layout(Layout::left_to_right(Align::Center)));
    let me = Picture::Person { id: ws.me_id.clone(), name: ws.me.clone() };
    let picture = if ws.me_id.is_empty() { None } else { ws.pictures.get(&me) };
    widgets::avatar(&mut row, &ws.me_id, &ws.me, 30.0, picture);
    row.add_space(8.0);
    row.with_layout(Layout::right_to_left(Align::Center), |ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        if widgets::icon_button(ui, Icon::SignOut, "Sign out").clicked() {
            request = Some(Request::SignOut);
        }
        let (icon, next, hint) = match theme {
            Appearance::Dark => (Icon::Moon, Appearance::Light, "Dark theme (click for light)"),
            Appearance::Light => (Icon::Sun, Appearance::System, "Light theme (click to follow the system)"),
            Appearance::System => (if ui.visuals().dark_mode { Icon::Moon } else { Icon::Sun }, Appearance::Dark, "Following the system (click for dark)"),
        };
        if widgets::icon_button(ui, icon, hint).clicked() {
            request = Some(Request::Theme(next));
        }
        ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
            ui.add(egui::Label::new(RichText::new(&ws.me).font(ty::item_strong()).color(p.text)).truncate());
        });
    });
    request
}

fn list(ui: &mut Ui, ws: &mut Workspace, directory: &Directory) {
    let p = colors();
    let filter = ws.filter.trim().to_lowercase();
    let matches = |title: &str| filter.is_empty() || title.to_lowercase().contains(&filter);

    // Activity: the mentions feed, as a nav row at the top (hidden while searching).
    if filter.is_empty()
        && let Some(activity) = directory.activity()
    {
        let open = ws.open.as_deref() == Some(activity.id.as_str());
        let r = nav_row(ui, Icon::At, "Activity", open, activity.unread);
        if r.clicked() {
            ws.open(&activity.id);
        }
        ui.add_space(4.0);
    }

    // Favourites first, as in Teams: pinned chats and favourite channels.
    let pinned: Vec<&Conversation> = directory.conversations.iter().filter(|c| c.pinned && c.kind != Kind::Feed && matches(&c.title)).collect();
    if !pinned.is_empty() && section(ui, ws, "pinned", "Pinned", None, false, false) {
        for c in pinned {
            row(ui, ws, c);
        }
    }

    for team in &directory.teams {
        let channels: Vec<&Conversation> =
            team.channels.iter().filter(|c| matches(&c.name) || matches(&team.name)).filter_map(|c| directory.conversation(&c.id)).collect();
        if channels.is_empty() {
            continue;
        }
        let key = format!("team:{}", team.id);
        let unread = channels.iter().any(|c| c.unread);
        let picture = team.picture.as_ref().and_then(|p| ws.pictures.get(p));
        if section(ui, ws, &key, &team.name, Some((&team.id, picture)), unread, false) {
            for channel in channels {
                row(ui, ws, channel);
            }
        }
    }

    let chats: Vec<&Conversation> = directory.chats().filter(|c| !c.pinned && matches(&c.title)).collect();
    if !chats.is_empty() && section(ui, ws, "chats", "Direct messages", None, false, true) {
        for chat in chats {
            row(ui, ws, chat);
        }
    }

    let meetings: Vec<&Conversation> =
        directory.conversations.iter().filter(|c| c.kind == Kind::Meeting && !c.pinned && matches(&c.title)).take(MEETINGS_SHOWN).collect();
    if !meetings.is_empty() && section(ui, ws, "meetings", "Meeting chats", None, false, false) {
        for meeting in meetings {
            row(ui, ws, meeting);
        }
    }
    let _ = p;
}

/// A top-level nav row (Activity): an icon, a label, and an unread dot.
fn nav_row(ui: &mut Ui, icon: Icon, label: &str, selected: bool, unread: bool) -> egui::Response {
    let p = colors();
    let (outer, response) = ui.allocate_exact_size(vec2(ui.available_width(), metrics::ROW), Sense::click());
    let rect = Rect::from_min_size(outer.min + vec2(metrics::SIDE_PAD, 0.0), vec2(outer.width() - 2.0 * metrics::SIDE_PAD, metrics::ROW));
    if selected || response.hovered() {
        ui.painter().rect_filled(rect, CornerRadius::same(metrics::RADIUS), if selected { p.selected } else { p.hover });
    }
    let color = if selected || unread { p.text } else { p.dim };
    let icon_rect = Rect::from_min_size(rect.min + vec2(10.0, (metrics::ROW - 17.0) / 2.0), vec2(17.0, 17.0));
    crate::ui::style::icons::paint(ui.painter(), icon_rect, icon, color);
    let font = if unread { ty::item_strong() } else { ty::item() };
    ui.painter().text(rect.min + vec2(36.0, metrics::ROW / 2.0), egui::Align2::LEFT_CENTER, label, font, color);
    if unread {
        ui.painter().circle_filled(egui::pos2(rect.right() - 10.0, rect.center().y), 3.5, p.accent);
    }
    response
}

/// What leads a row: a channel's mark, a meeting's calendar, a chat's avatar.
fn lead(ui: &mut Ui, c: &Conversation, picture: Option<TextureId>) {
    let color = colors().faint;
    match c.kind {
        Kind::Channel => {
            widgets::icon(ui, Icon::Hash, 15.0, color);
        }
        Kind::Meeting => {
            widgets::icon(ui, Icon::Calendar, 15.0, color);
        }
        Kind::Chat | Kind::Feed => {
            widgets::avatar(ui, &c.id, &c.title, 22.0, picture);
        }
    }
}

/// A section header that folds its rows away; true while it's open. An optional team
/// tile sits before the label; `unread` brightens it.
fn section(ui: &mut Ui, ws: &mut Workspace, key: &str, label: &str, tile: Option<(&str, Option<TextureId>)>, unread: bool, plus: bool) -> bool {
    let p = colors();
    let open = !ws.folded.contains(key);
    ui.add_space(10.0);
    let response = ui
        .horizontal(|ui| {
            ui.add_space(12.0);
            widgets::icon(ui, if open { Icon::ChevronDown } else { Icon::ChevronRight }, 12.0, p.faint);
            ui.add_space(4.0);
            if let Some((id, picture)) = tile {
                widgets::tile(ui, id, label, 18.0, picture);
                ui.add_space(2.0);
                ui.label(RichText::new(label).font(ty::item_strong()).color(if unread { p.text } else { p.dim }));
            } else {
                ui.label(RichText::new(label.to_uppercase()).font(ty::label()).color(p.faint));
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if plus {
                    widgets::icon(ui, Icon::Plus, 14.0, p.faint);
                }
                ui.allocate_space(vec2(ui.available_width(), 18.0));
            });
        })
        .response
        .interact(Sense::click());
    if response.clicked() {
        if open {
            ws.folded.insert(key.to_string());
        } else {
            ws.folded.remove(key);
        }
    }
    ui.add_space(2.0);
    open
}

/// A conversation's row: its leading mark, title, a muted icon or an unread dot.
fn row(ui: &mut Ui, ws: &mut Workspace, conversation: &Conversation) {
    let p = colors();
    let selected = ws.open.as_deref() == Some(conversation.id.as_str());
    let width = ui.available_width() - 2.0 * metrics::SIDE_PAD;
    let (outer, response) = ui.allocate_exact_size(vec2(ui.available_width(), metrics::ROW), Sense::click());
    let rect = Rect::from_min_size(outer.min + vec2(metrics::SIDE_PAD, 0.0), vec2(width, metrics::ROW));
    if selected || response.hovered() {
        ui.painter().rect_filled(rect, CornerRadius::same(metrics::RADIUS), if selected { p.selected } else { p.hover });
    }
    let mut inner = ui.new_child(egui::UiBuilder::new().max_rect(rect.shrink2(vec2(10.0, 0.0))).layout(Layout::left_to_right(Align::Center)));
    let picture = conversation.picture.as_ref().and_then(|p| ws.pictures.get(p));
    lead(&mut inner, conversation, picture);
    inner.add_space(8.0);
    let (font, color) = if conversation.muted {
        (ty::item(), p.faint)
    } else if conversation.unread {
        (ty::item_strong(), p.text)
    } else if selected {
        (ty::item(), p.text)
    } else {
        (ty::item(), p.dim)
    };
    let title = if conversation.title.is_empty() { "Untitled" } else { &conversation.title };
    inner.with_layout(Layout::right_to_left(Align::Center), |ui| {
        if conversation.muted {
            widgets::icon(ui, Icon::Mute, 14.0, p.faint);
        } else if conversation.unread {
            widgets::unread_dot(ui);
        }
        ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
            ui.add(egui::Label::new(RichText::new(title).font(font).color(color)).truncate().selectable(false));
        });
    });
    if response.clicked() {
        ws.open(&conversation.id);
    }
}
