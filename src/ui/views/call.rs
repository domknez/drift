//! A call ringing: a bar over the top of the window with who's calling, Call back
//! (from Teams on the web: drift can't take the call itself) and Decline.

use crate::model::Picture;
use crate::ui::style::icons::Icon;
use crate::ui::style::{colors, metrics, typography as ty};
use crate::ui::widgets;
use crate::ui::workspace::Workspace;
use eframe::egui::{self, Align, Color32, CornerRadius, Frame, Layout, Margin, RichText, Sense, Stroke, pos2, vec2};

const WIDTH: f32 = 460.0;
const GREEN: Color32 = Color32::from_rgb(0x1f, 0x8a, 0x4c);
const RED: Color32 = Color32::from_rgb(0xc4, 0x36, 0x3a);

pub fn show(ctx: &egui::Context, ws: &mut Workspace) {
    let Some(call) = ws.ringing.clone() else { return };
    let p = colors();
    let screen = ctx.content_rect();
    let at = pos2(screen.center().x - WIDTH / 2.0, screen.top() + metrics::TOP_BAR + 8.0);
    let mut answer = None;
    egui::Area::new(egui::Id::unique("call-bar")).order(egui::Order::Foreground).fixed_pos(at).show(ctx, |ui| {
        Frame::new()
            .fill(p.panel)
            .stroke(Stroke::new(1.0, p.border))
            .corner_radius(CornerRadius::same(18))
            .shadow(egui::Shadow { offset: [0, 10], blur: 30, spread: 0, color: Color32::from_black_alpha(60) })
            .inner_margin(Margin::symmetric(16, 14))
            .show(ui, |ui| {
                ui.set_width(WIDTH - 32.0);
                ui.horizontal(|ui| {
                    // The caller's picture, with a ring pulsing around it while it rings.
                    let size = 44.0;
                    let picture = ws.pictures.get(&Picture::Person { id: call.from.clone(), name: call.name.clone() });
                    let face = widgets::avatar(ui, &call.from, &call.name, size, picture).rect;
                    let t = call.since.elapsed().as_secs_f32();
                    let pulse = (t * 2.0).fract();
                    ui.painter().circle_stroke(
                        face.center(),
                        size / 2.0 + 3.0 + pulse * 8.0,
                        Stroke::new(2.0, GREEN.gamma_multiply(1.0 - pulse)),
                    );
                    ui.ctx().request_repaint();
                    ui.add_space(14.0);
                    ui.vertical(|ui| {
                        ui.add_space(3.0);
                        ui.add(egui::Label::new(RichText::new(&call.name).font(ty::item_unread()).color(p.text)).truncate());
                        let what = if call.group { "Incoming group call" } else { "Incoming call" };
                        ui.label(RichText::new(what).font(ty::small()).color(p.dim));
                    });
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.spacing_mut().item_spacing.x = 8.0;
                        let tip = match &call.address {
                            Some(_) => format!("Call {} from Teams on the web", call.name),
                            None => "Open Teams on the web to call back".to_string(),
                        };
                        if pill(ui, "Call back", Icon::Phone, GREEN, &tip).clicked() {
                            answer = Some(true);
                        }
                        let tip = "Decline the call (it stops ringing everywhere)";
                        if pill(ui, "Decline", Icon::Close, RED, tip).clicked() {
                            answer = Some(false);
                        }
                    });
                });
                // drift can't take the call: say where it's answered.
                ui.add_space(8.0);
                ui.label(RichText::new("Answer on your phone, or call back from Teams on the web").font(ty::small()).color(p.faint));
            });
    });
    match answer {
        Some(true) => ws.call_back(ctx),
        Some(false) => ws.decline_call(),
        None => {}
    }
}

/// A rounded button filled with `fill`: an icon and a word, white on it.
fn pill(ui: &mut egui::Ui, label: &str, icon: Icon, fill: Color32, tip: &str) -> egui::Response {
    let galley = ui.painter().layout_no_wrap(label.to_string(), ty::small_strong(), Color32::WHITE);
    let size = vec2(galley.size().x + 46.0, 38.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(tip);
    widgets::control(ui, &response, egui::Role::Button, label);
    let fill = if response.hovered() { fill.gamma_multiply(0.85) } else { fill };
    ui.painter().rect_filled(rect, CornerRadius::same(19), fill);
    let mark = egui::Rect::from_center_size(pos2(rect.left() + 22.0, rect.center().y), vec2(16.0, 16.0));
    crate::ui::style::icons::paint(ui.painter(), mark, icon, Color32::WHITE);
    ui.painter().galley(pos2(mark.right() + 7.0, rect.center().y - galley.size().y / 2.0), galley, Color32::WHITE);
    response
}
