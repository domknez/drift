//! The thread beside the conversation: the post, its replies, and a box to reply.

use super::conversation;
use crate::ui::style::icons::{self, Icon};
use crate::ui::style::{colors, metrics, typography as ty};
use crate::ui::workspace::Workspace;
use eframe::egui::{self, Align, CornerRadius, Frame, Layout, Margin, Rect, RichText, Sense, Stroke, Ui, pos2, vec2};

pub fn show(ui: &mut Ui, ws: &mut Workspace) {
    let p = colors();
    let full = ui.max_rect();
    let header = Rect::from_min_size(full.min, vec2(full.width(), metrics::HEADER));
    let Some(root) = ws.thread.clone() else { return };
    // Until the messages are in, there's nothing to show yet (but the thread stays open).
    let Some(messages) = ws.messages.clone() else { return };
    let Some(post) = messages.iter().find(|m| m.id == root).cloned() else {
        ws.thread = None;
        return;
    };
    let conversation = ws.open.clone().unwrap_or_default();
    let channel = ws.open.as_deref().and_then(|id| ws.directory.as_ref()?.conversation(id)).map(|c| c.title.clone()).unwrap_or_default();

    let mut h =
        ui.new_child(egui::UiBuilder::new().max_rect(header.shrink2(vec2(metrics::PAGE_PAD, 0.0))).layout(Layout::top_down(Align::Min)));
    h.add_space(13.0);
    h.spacing_mut().item_spacing.y = 1.0;
    h.label(RichText::new("Thread").font(ty::title()).color(p.text));
    h.label(RichText::new(format!("#{channel}")).font(ty::small()).color(p.faint));
    let close = Rect::from_min_size(pos2(header.right() - metrics::PAGE_PAD - 34.0, header.center().y - 17.0), vec2(34.0, 34.0));
    let response = ui.interact(close, ui.scope_id().with("close-thread"), Sense::click()).on_hover_text("Close the thread");
    ui.painter().rect_filled(close, CornerRadius::same(10), if response.hovered() { p.hover } else { p.surface });
    icons::paint(ui.painter(), Rect::from_center_size(close.center(), vec2(15.0, 15.0)), Icon::Close, p.dim);
    if response.clicked() {
        ws.thread = None;
        return;
    }
    ui.painter().hline(full.x_range(), header.bottom() - 0.5, Stroke::new(1.0, p.line));

    // The post, then its replies, as one conversation.
    let mut thread = vec![post];
    thread.extend(messages.iter().filter(|m| m.root.as_deref() == Some(root.as_str())).cloned());
    let bottom = full.bottom() - conversation::composer_height(ws, true);
    let body = Rect::from_min_max(pos2(full.left(), header.bottom()), pos2(full.right(), bottom));
    let mut page = ui.new_child(egui::UiBuilder::new().max_rect(body).layout(Layout::top_down(Align::Min)));
    let me = ws.me_id.clone();
    let mut action = None;
    egui::ScrollArea::vertical().id_salt(("thread", &root)).stick_to_bottom(true).auto_shrink([false, false]).show(&mut page, |ui| {
        Frame::new().inner_margin(Margin::symmetric(metrics::PAGE_PAD as i8, 10)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            let focus = ws.focus.clone();
            let custom_emoji = ws.custom_emoji.clone();
            let recent_emoji = ws.recent_emoji.clone().unwrap_or_default();
            let mut view = conversation::View {
                me: &me,
                read_by: &[],
                channel: false,
                open: None,
                focus: focus.as_deref(),
                scrolled: ws.focus_scrolled,
                newest: usize::MAX,
                conversation: &conversation,
                top_at: None,
                custom_emoji: &custom_emoji,
                recent_emoji: &recent_emoji,
            };
            action = conversation::list(ui, &mut ws.pictures, &thread, &mut view);
            ws.focus_scrolled |= view.scrolled;
        });
    });
    if let Some(action) = action {
        conversation::apply(ui, ws, action, true);
    }
    conversation::composer(ui, ws, Rect::from_min_max(pos2(full.left(), bottom), full.max), "Reply…", true);
}
