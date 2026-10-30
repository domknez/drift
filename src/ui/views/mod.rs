//! One file per part of the window.

pub mod conversation;
pub mod sidebar;
pub mod sign_in;

use super::app::Request;
use super::style::{colors, metrics};
use super::workspace::Workspace;
use crate::settings::Appearance;
use eframe::egui::{self, Frame, Margin, ViewportCommand};

/// The signed-in window: the sidebar and the open conversation.
pub fn workspace(ui: &mut egui::Ui, ws: &mut Workspace, theme: Appearance) -> Option<Request> {
    let p = colors();
    let mut request = None;
    if ui.input_mut(|i| i.consume_shortcut(&egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::R))) {
        ws.refresh();
    }
    egui::Panel::left("sidebar")
        .exact_size(metrics::SIDEBAR_WIDTH)
        .resizable(false)
        .show_separator_line(true)
        .frame(Frame::new().fill(p.sidebar).inner_margin(Margin::ZERO))
        .show(ui, |ui| request = sidebar::show(ui, ws, theme));
    egui::CentralPanel::default().frame(Frame::new().fill(p.bg)).show(ui, |ui| conversation::show(ui, ws));
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
