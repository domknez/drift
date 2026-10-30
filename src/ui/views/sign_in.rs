//! The sign-in page: the wordmark, one button, and why the last attempt failed.

use crate::ui::app::SignIn;
use crate::ui::style::{colors, typography as ty};
use eframe::egui::{self, Align, Frame, Layout, RichText, vec2};

pub fn show(ui: &mut egui::Ui, sign_in: &mut SignIn) {
    let p = colors();
    egui::CentralPanel::default().frame(Frame::new().fill(p.sidebar)).show(ui, |ui| {
        super::drag_area(ui, egui::Rect::from_min_size(ui.max_rect().min, vec2(ui.max_rect().width(), 40.0)), "sign-in-drag");
        ui.with_layout(Layout::top_down(Align::Center), |ui| {
            ui.add_space((ui.available_height() / 2.0 - 120.0).max(40.0));
            ui.label(RichText::new("drift").font(egui::FontId::new(40.0, ty::semibold())).color(p.text));
            ui.add_space(6.0);
            ui.label(RichText::new("Your Teams chats and channels, light as air.").font(ty::body()).color(p.dim));
            ui.add_space(28.0);
            let waiting = sign_in.pending.is_running();
            let label = if waiting { "Waiting for the sign-in window…" } else { "Sign in with Microsoft" };
            let button = egui::Button::new(RichText::new(label).font(ty::item_strong()).color(p.on_accent))
                .fill(p.accent_fill)
                .min_size(vec2(240.0, 40.0))
                .corner_radius(10);
            if ui.add_enabled(!waiting, button).clicked() {
                sign_in.start(ui.ctx());
            }
            ui.add_space(14.0);
            if let Some(error) = &sign_in.error {
                ui.label(RichText::new(error).font(ty::small()).color(p.danger));
            } else {
                ui.label(
                    RichText::new("Your work or school account. drift keeps the sign-in in the system keychain.")
                        .font(ty::small())
                        .color(p.faint),
                );
            }
        });
    });
}
