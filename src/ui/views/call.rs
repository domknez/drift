//! A call: a bar over the top of the window. While it rings: who's calling, Answer
//! (or Call back from Teams on the web, in a build that can't take calls) and
//! Decline. Once answered: who with, how long, Mute and Hang up.

use crate::model::Picture;
use crate::ui::style::icons::Icon;
use crate::ui::style::{colors, metrics, typography as ty};
use crate::ui::widgets;
use crate::ui::workspace::Workspace;
use eframe::egui::{self, Align, Color32, CornerRadius, Frame, Layout, Margin, RichText, Sense, Stroke, pos2, vec2};

const WIDTH: f32 = 460.0;
const GREEN: Color32 = Color32::from_rgb(0x1f, 0x8a, 0x4c);
const RED: Color32 = Color32::from_rgb(0xc4, 0x36, 0x3a);

const GREY: Color32 = Color32::from_rgb(0x5b, 0x5f, 0x66);

pub fn show(ctx: &egui::Context, ws: &mut Workspace) {
    if ws.in_call.is_some() {
        in_call(ctx, ws);
    } else {
        ringing(ctx, ws);
    }
}

/// The bar's frame, centred under the top bar.
fn bar(ctx: &egui::Context, add: impl FnOnce(&mut egui::Ui)) {
    let p = colors();
    let screen = ctx.content_rect();
    let at = pos2(screen.center().x - WIDTH / 2.0, screen.top() + metrics::TOP_BAR + 8.0);
    egui::Area::new(egui::Id::unique("call-bar")).order(egui::Order::Foreground).fixed_pos(at).show(ctx, |ui| {
        Frame::new()
            .fill(p.panel)
            .stroke(Stroke::new(1.0, p.border))
            .corner_radius(CornerRadius::same(18))
            .shadow(egui::Shadow { offset: [0, 10], blur: 30, spread: 0, color: Color32::from_black_alpha(60) })
            .inner_margin(Margin::symmetric(16, 14))
            .show(ui, |ui| {
                ui.set_width(WIDTH - 32.0);
                add(ui);
            });
    });
}

/// Who the call is with, and a line under the name.
fn who(ui: &mut egui::Ui, ws: &mut Workspace, from: &str, name: &str, what: &str) -> egui::Rect {
    let p = colors();
    let picture = ws.pictures.get(&Picture::Person { id: from.to_string(), name: name.to_string() });
    let face = widgets::avatar(ui, from, name, 44.0, picture).rect;
    ui.add_space(14.0);
    ui.vertical(|ui| {
        ui.add_space(3.0);
        ui.add(egui::Label::new(RichText::new(name).font(ty::item_unread()).color(p.text)).truncate());
        ui.label(RichText::new(what).font(ty::small()).color(p.dim));
    });
    face
}

fn in_call(ctx: &egui::Context, ws: &mut Workspace) {
    let Some(call) = ws.in_call.as_ref() else { return };
    let (from, name, muted) = (call.from.clone(), call.name.clone(), call.muted);
    let what = match call.connected {
        Some(at) => {
            let s = at.elapsed().as_secs();
            format!("In call · {}:{:02}", s / 60, s % 60)
        }
        None => "Connecting…".to_string(),
    };
    let mut clicked = None;
    bar(ctx, |ui| {
        ui.horizontal(|ui| {
            who(ui, ws, &from, &name, &what);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                if pill(ui, "Hang up", Icon::Phone, RED, "Leave the call").clicked() {
                    clicked = Some(false);
                }
                let (label, icon, tip) = if muted {
                    ("Unmute", Icon::MicOff, "Let the call hear you")
                } else {
                    ("Mute", Icon::Mic, "Stop the call hearing you")
                };
                if pill(ui, label, icon, GREY, tip).clicked() {
                    clicked = Some(true);
                }
            });
        });
        ui.add_space(8.0);
        let p = colors();
        ui.label(RichText::new("Use headphones: drift doesn't cancel echo yet").font(ty::small()).color(p.faint));
    });
    match clicked {
        Some(true) => ws.toggle_mute(),
        Some(false) => ws.hang_up(),
        None => {}
    }
}

fn ringing(ctx: &egui::Context, ws: &mut Workspace) {
    let Some(call) = ws.ringing.clone() else { return };
    let p = colors();
    let can_answer = ws.can_answer();
    let mut answer = None;
    bar(ctx, |ui| {
        ui.horizontal(|ui| {
            // The caller's picture, with a ring pulsing around it while it rings.
            let what = if call.group { "Incoming group call" } else { "Incoming call" };
            let face = who(ui, ws, &call.from, &call.name, what);
            let pulse = (call.since.elapsed().as_secs_f32() * 2.0).fract();
            ui.painter().circle_stroke(
                face.center(),
                face.width() / 2.0 + 3.0 + pulse * 8.0,
                Stroke::new(2.0, GREEN.gamma_multiply(1.0 - pulse)),
            );
            ui.ctx().request_repaint();
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                if can_answer && !call.group {
                    if pill(ui, "Answer", Icon::Phone, GREEN, "Take the call here (audio)").clicked() {
                        answer = Some(Answer::Here);
                    }
                } else {
                    let tip = match &call.address {
                        Some(_) => format!("Call {} from Teams on the web", call.name),
                        None => "Open Teams on the web to call back".to_string(),
                    };
                    if pill(ui, "Call back", Icon::Phone, GREEN, &tip).clicked() {
                        answer = Some(Answer::CallBack);
                    }
                }
                if pill(ui, "Decline", Icon::Close, RED, "Decline the call (it stops ringing everywhere)").clicked() {
                    answer = Some(Answer::Decline);
                }
            });
        });
        if !can_answer || call.group {
            // drift can't take this call: say where it's answered.
            ui.add_space(8.0);
            ui.label(RichText::new("Answer on your phone, or call back from Teams on the web").font(ty::small()).color(p.faint));
        }
    });
    match answer {
        Some(Answer::Here) => ws.answer_call(ctx),
        Some(Answer::CallBack) => ws.call_back(ctx),
        Some(Answer::Decline) => ws.decline_call(),
        None => {}
    }
}

enum Answer {
    Here,
    CallBack,
    Decline,
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
