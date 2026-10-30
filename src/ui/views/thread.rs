//! The thread beside the conversation: the post, its replies, and a box to reply.

use super::conversation::{self, COMPOSER_HEIGHT};
use crate::model::Message;
use crate::ui::style::icons::Icon;
use crate::ui::style::{colors, metrics, typography as ty};
use crate::ui::widgets;
use crate::ui::workspace::Workspace;
use eframe::egui::{self, Align, Layout, Rect, RichText, Sense, Stroke, Ui, pos2, vec2};

pub fn show(ui: &mut Ui, ws: &mut Workspace) {
    let p = colors();
    let full = ui.max_rect();
    ui.painter().vline(full.left() + 0.5, full.y_range(), Stroke::new(1.0, p.line));
    let header = Rect::from_min_size(full.min, vec2(full.width(), metrics::HEADER));
    super::drag_area(ui, header, "thread-drag");

    let Some(root) = ws.thread.clone() else { return };
    // Until the messages are in, there's nothing to show yet (but the thread stays open).
    let Some(messages) = ws.messages.clone() else { return };
    let Some(post) = messages.iter().find(|m| m.id == root).cloned() else {
        ws.thread = None;
        return;
    };
    let replies: Vec<&Message> = messages.iter().filter(|m| m.root.as_deref() == Some(root.as_str())).collect();
    let channel = ws.open.as_deref().and_then(|id| ws.directory.as_ref()?.conversation(id)).map(|c| c.title.clone()).unwrap_or_default();

    let mut h = ui.new_child(egui::UiBuilder::new().max_rect(header.shrink2(vec2(16.0, 0.0))).layout(Layout::left_to_right(Align::Center)));
    h.label(RichText::new("Thread").font(ty::title()).color(p.text));
    h.add_space(4.0);
    h.label(RichText::new(format!("#{channel}")).font(ty::small()).color(p.faint));
    h.with_layout(Layout::right_to_left(Align::Center), |ui| {
        if widgets::icon_button(ui, Icon::Close, "Close the thread").clicked() {
            ws.thread = None;
        }
    });
    ui.painter().hline(full.x_range(), header.bottom() - 0.5, Stroke::new(1.0, p.line));
    if ws.thread.is_none() {
        return;
    }

    let bottom = full.bottom() - COMPOSER_HEIGHT;
    let body = Rect::from_min_max(pos2(full.left(), header.bottom()), pos2(full.right(), bottom));
    let mut page = ui.new_child(egui::UiBuilder::new().max_rect(body).layout(Layout::top_down(Align::Min)));
    egui::ScrollArea::vertical().id_salt(("thread", &root)).auto_shrink([false, false]).show(&mut page, |ui| {
        ui.add_space(6.0);
        conversation::message(ui, &mut ws.pictures, &post, true, None, false);
        let n = replies.len();
        if n > 0 {
            let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 26.0), Sense::hover());
            let label = if n == 1 { "1 reply".to_string() } else { format!("{n} replies") };
            let galley = ui.painter().layout_no_wrap(label, ty::meta(), p.faint);
            let x = rect.left() + metrics::PAGE_PAD;
            ui.painter().galley(pos2(x, rect.center().y - galley.size().y / 2.0), galley.clone(), p.faint);
            ui.painter().hline(x + galley.size().x + 10.0..=rect.right() - metrics::PAGE_PAD, rect.center().y, Stroke::new(1.0, p.line));
        }
        let mut previous: Option<&Message> = None;
        for m in &replies {
            let grouped = previous.is_some_and(|prev| prev.author.id == m.author.id && m.at - prev.at < metrics::GROUP_GAP);
            conversation::message(ui, &mut ws.pictures, m, !grouped, None, false);
            previous = Some(m);
        }
        ui.add_space(8.0);
    });
    conversation::composer(ui, ws, Rect::from_min_max(pos2(full.left(), bottom), full.max), "Reply…", true);
}
