//! One file per part of the window: the rail, the list, the conversation, the
//! thread beside it, and the sign-in page.

pub mod conversation;
pub mod list;
pub mod rail;
pub mod sign_in;
pub mod thread;

use super::app::Request;
use super::style::{colors, metrics};
use super::workspace::Workspace;
use crate::settings::Appearance;
use eframe::egui::{self, Frame, ViewportCommand};

/// The signed-in window: rail, list, conversation, and the thread when one is open.
pub fn workspace(ui: &mut egui::Ui, ws: &mut Workspace, theme: Appearance) -> Option<Request> {
    let p = colors();
    let mut request = None;
    if ui.input_mut(|i| i.consume_shortcut(&egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::R))) {
        ws.refresh();
    }
    let panel = |fill| Frame::new().fill(fill);
    egui::Panel::left("rail")
        .exact_size(metrics::RAIL_WIDTH)
        .resizable(false)
        .show_separator_line(false)
        .frame(panel(p.rail))
        .show(ui, |ui| request = rail::show(ui, ws, theme));
    egui::Panel::left("list")
        .exact_size(metrics::LIST_WIDTH)
        .resizable(false)
        .show_separator_line(false)
        .frame(panel(p.side))
        .show(ui, |ui| list::show(ui, ws));
    if ws.thread.is_some() {
        egui::Panel::right("thread")
            .exact_size(metrics::THREAD_WIDTH)
            .resizable(false)
            .show_separator_line(false)
            .frame(panel(p.bg))
            .show(ui, |ui| thread::show(ui, ws));
    }
    egui::CentralPanel::default().frame(panel(p.bg)).show(ui, |ui| conversation::show(ui, ws));
    request
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
