//! One file per part of the window: the top bar, the list, the conversation, the
//! thread beside it, and the sign-in page. Panels float on the canvas.

pub mod conversation;
pub mod list;
pub mod sign_in;
pub mod thread;
pub mod topbar;

use super::app::Request;
use super::style::{colors, metrics};
use super::workspace::Workspace;
use crate::settings::Appearance;
use eframe::egui::{self, CornerRadius, Frame, Rect, ViewportCommand, pos2, vec2};

/// The signed-in window: the top bar, then the list, the conversation, and the
/// thread when one is open, as panels on the canvas.
pub fn workspace(ui: &mut egui::Ui, ws: &mut Workspace, theme: Appearance) -> Option<Request> {
    let p = colors();
    let mut request = None;
    if ui.input_mut(|i| i.consume_shortcut(&egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::R))) {
        ws.refresh();
    }
    // Pictures dropped on the window go with the next message (in the thread if one is open).
    let dropped = ui.input(|i| i.raw.dropped_files.clone());
    if ws.open.is_some() {
        for file in dropped {
            let name = file.path().file_name().map_or_else(|| "picture".into(), |n| n.to_string_lossy().into_owned());
            match file.bytes() {
                Ok(bytes) => ws.attach(ws.thread.is_some(), &name, bytes),
                Err(e) => ws.error = Some(format!("{name}: {e}")),
            }
        }
    }
    egui::CentralPanel::default().frame(Frame::new().fill(p.canvas)).show(ui, |ui| {
        let full = ui.max_rect();
        let bar = Rect::from_min_size(full.min, vec2(full.width(), metrics::TOP_BAR));
        request = topbar::show(&mut child(ui, bar), ws, theme);

        let gap = metrics::GAP;
        let body = Rect::from_min_max(pos2(full.left() + gap, bar.bottom()), pos2(full.right() - gap, full.bottom() - gap));
        let list = Rect::from_min_size(body.min, vec2(metrics::LIST_WIDTH, body.height()));
        let mut main = Rect::from_min_max(pos2(list.right() + gap, body.top()), body.max);
        if ws.thread.is_some() {
            let thread = Rect::from_min_max(pos2(body.right() - metrics::THREAD_WIDTH, body.top()), body.max);
            main.max.x = thread.left() - gap;
            thread::show(&mut panel(ui, thread), ws);
        }
        list::show(&mut panel(ui, list), ws);
        conversation::show(&mut panel(ui, main), ws);
    });
    request
}

/// A child Ui filling `rect`.
fn child(ui: &mut egui::Ui, rect: Rect) -> egui::Ui {
    ui.new_child(egui::UiBuilder::new().max_rect(rect))
}

/// A floating panel: a rounded plate in the panel colour, and a Ui clipped to it.
fn panel(ui: &mut egui::Ui, rect: Rect) -> egui::Ui {
    let p = colors();
    ui.painter().rect_filled(rect, CornerRadius::same(metrics::PANEL_RADIUS), p.panel);
    let mut inner = child(ui, rect);
    inner.set_clip_rect(rect);
    inner
}

/// Make `rect` drag the window (the title bar is hidden on macOS).
pub fn drag_area(ui: &egui::Ui, rect: egui::Rect, id: &str) {
    let response = ui.interact(rect, ui.scope_id().with(id), egui::Sense::click_and_drag());
    if response.drag_started() {
        ui.ctx().send_viewport_cmd(ViewportCommand::StartDrag);
    }
    if response.double_clicked() {
        let maximized = ui.ctx().input(|i| i.viewport().maximized.unwrap_or(false));
        ui.ctx().send_viewport_cmd(ViewportCommand::Maximized(!maximized));
    }
}
