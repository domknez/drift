//! The sidebar: search, teams with their channels, chats, meeting chats, and the
//! account row at the bottom.

use crate::model::{Conversation, Directory, Kind};
use crate::settings::Appearance;
use crate::ui::app::Request;
use crate::ui::style::icons::Icon;
use crate::ui::style::{colors, metrics, typography as ty};
use crate::ui::widgets;
use crate::ui::workspace::Workspace;
use eframe::egui::{self, Align, CornerRadius, Layout, Margin, Rect, RichText, Sense, Ui, vec2};

/// Meeting chats listed when unfolded (the newest).
const MEETINGS_SHOWN: usize = 12;

pub fn show(ui: &mut Ui, ws: &mut Workspace, theme: Appearance) -> Option<Request> {
    let p = colors();
    let mut request = None;
    let top = Rect::from_min_size(ui.max_rect().min, vec2(ui.max_rect().width(), metrics::TITLE_BAR));
    super::drag_area(ui, top, "sidebar-drag");
    ui.add_space(metrics::TITLE_BAR);

    egui::Frame::new().inner_margin(Margin::symmetric(12, 0)).show(ui, |ui| {
        let search = egui::TextEdit::singleline(&mut ws.filter)
            .hint_text(RichText::new("Search chats and channels").color(p.faint))
            .font(ty::item())
            .margin(Margin::symmetric(10, 8))
            .desired_width(f32::INFINITY);
        ui.add(search);
    });
    ui.add_space(6.0);

    let bottom = 52.0;
    let list_height = (ui.available_height() - bottom).max(0.0);
    egui::ScrollArea::vertical().max_height(list_height).auto_shrink([false, false]).show(ui, |ui| {
        ui.add_space(4.0);
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

    ui.with_layout(Layout::bottom_up(Align::Min), |ui| {
        ui.add_space(10.0);
        egui::Frame::new().inner_margin(Margin::symmetric(12, 0)).show(ui, |ui| {
            ui.horizontal(|ui| {
                widgets::avatar(ui, &ws.me, &ws.me, 28.0);
                ui.add(egui::Label::new(RichText::new(&ws.me).font(ty::item_strong()).color(p.text)).truncate());
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if widgets::icon_button(ui, Icon::SignOut, "Sign out").clicked() {
                        request = Some(Request::SignOut);
                    }
                    let (icon, next, hint) = match theme {
                        Appearance::Dark => (Icon::Moon, Appearance::Light, "Dark theme (click for light)"),
                        Appearance::Light => (Icon::Sun, Appearance::System, "Light theme (click to follow the system)"),
                        Appearance::System => (
                            if ui.visuals().dark_mode { Icon::Moon } else { Icon::Sun },
                            Appearance::Dark,
                            "Following the system (click for dark)",
                        ),
                    };
                    if widgets::icon_button(ui, icon, hint).clicked() {
                        request = Some(Request::Theme(next));
                    }
                });
            });
        });
        ui.separator();
    });
    request
}

fn list(ui: &mut Ui, ws: &mut Workspace, directory: &Directory) {
    let filter = ws.filter.trim().to_lowercase();
    let matches = |title: &str| filter.is_empty() || title.to_lowercase().contains(&filter);

    for team in &directory.teams {
        let channels: Vec<&Conversation> = team
            .channels
            .iter()
            .filter(|c| matches(&c.name) || matches(&team.name))
            .filter_map(|c| directory.conversation(&c.id))
            .collect();
        if channels.is_empty() {
            continue;
        }
        let key = format!("team:{}", team.id);
        let unread = channels.iter().any(|c| c.unread);
        if section(ui, ws, &key, |ui| {
            widgets::tile(ui, &team.id, &team.name, 18.0);
            ui.label(RichText::new(&team.name).font(ty::label()).color(if unread { colors().text } else { colors().dim }));
        }) {
            for channel in channels {
                row(ui, ws, channel, |ui| {
                    widgets::icon(ui, Icon::Hash, 15.0, colors().faint);
                });
            }
        }
    }

    let chats: Vec<&Conversation> = directory.chats().filter(|c| matches(&c.title)).collect();
    if !chats.is_empty()
        && section(ui, ws, "chats", |ui| {
            ui.label(RichText::new("Chats").font(ty::label()).color(colors().dim));
        })
    {
        for chat in chats {
            row(ui, ws, chat, |ui| {
                widgets::avatar(ui, &chat.id, &chat.title, 22.0);
            });
        }
    }

    let meetings: Vec<&Conversation> =
        directory.conversations.iter().filter(|c| c.kind == Kind::Meeting && matches(&c.title)).take(MEETINGS_SHOWN).collect();
    if !meetings.is_empty()
        && section(ui, ws, "meetings", |ui| {
            ui.label(RichText::new("Meeting chats").font(ty::label()).color(colors().dim));
        })
    {
        for meeting in meetings {
            row(ui, ws, meeting, |ui| {
                widgets::icon(ui, Icon::Calendar, 15.0, colors().faint);
            });
        }
    }
}

/// A section header that folds its rows away; true while it's open.
fn section(ui: &mut Ui, ws: &mut Workspace, key: &str, label: impl FnOnce(&mut Ui)) -> bool {
    let open = !ws.folded.contains(key);
    ui.add_space(8.0);
    let response = ui
        .horizontal(|ui| {
            ui.add_space(14.0);
            widgets::icon(ui, if open { Icon::ChevronDown } else { Icon::ChevronRight }, 12.0, colors().faint);
            label(ui);
            ui.allocate_space(vec2(ui.available_width(), 18.0));
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

/// A conversation's row: its leading mark, title and unread dot.
fn row(ui: &mut Ui, ws: &mut Workspace, conversation: &Conversation, lead: impl FnOnce(&mut Ui)) {
    let p = colors();
    let selected = ws.open.as_deref() == Some(conversation.id.as_str());
    let width = ui.available_width() - 2.0 * metrics::SIDE_PAD;
    let (outer, response) = ui.allocate_exact_size(vec2(ui.available_width(), metrics::ROW), Sense::click());
    let rect = Rect::from_min_size(outer.min + vec2(metrics::SIDE_PAD, 0.0), vec2(width, metrics::ROW));
    if selected || response.hovered() {
        ui.painter().rect_filled(rect, CornerRadius::same(metrics::RADIUS), if selected { p.selected } else { p.hover });
    }
    let mut inner =
        ui.new_child(egui::UiBuilder::new().max_rect(rect.shrink2(vec2(10.0, 0.0))).layout(Layout::left_to_right(Align::Center)));
    lead(&mut inner);
    inner.add_space(2.0);
    let (font, color) = if conversation.unread {
        (ty::item_strong(), p.text)
    } else if selected {
        (ty::item(), p.text)
    } else {
        (ty::item(), p.dim)
    };
    let title = if conversation.title.is_empty() { "Untitled" } else { &conversation.title };
    inner.with_layout(Layout::right_to_left(Align::Center), |ui| {
        if conversation.unread {
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
