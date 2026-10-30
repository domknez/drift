//! Toasts: new messages from other conversations while drift is in front, in the
//! bottom right corner. Each says who wrote, where and what; a click opens it. They
//! fade after a few seconds, stay while the pointer is on them, and close with ×.

use super::style::icons::Icon;
use super::style::{colors, typography as ty};
use super::widgets;
use crate::model::ConversationId;
use eframe::egui::{self, Align, CornerRadius, Frame, Layout, Margin, Order, Rect, RichText, Sense, Stroke, pos2};
use std::time::{Duration, Instant};

/// How long a toast stays up.
const STAYS: Duration = Duration::from_secs(7);
/// Fading in, and again out.
const FADE: Duration = Duration::from_millis(250);
/// Most toasts up at once; a new one pushes out the oldest.
const MOST: usize = 3;
const WIDTH: f32 = 340.0;
const INSET: f32 = 18.0;

struct Toast {
    title: String,
    place: Option<String>,
    text: String,
    conversation: ConversationId,
    /// When it came (again), or was last under the pointer.
    since: Instant,
}

impl Toast {
    fn opacity(&self) -> f32 {
        let age = self.since.elapsed();
        let fade_in = age.as_secs_f32() / FADE.as_secs_f32();
        let fade_out = (STAYS + FADE).saturating_sub(age).as_secs_f32() / FADE.as_secs_f32();
        fade_in.min(fade_out).clamp(0.0, 1.0)
    }
}

#[derive(Default)]
pub struct Toasts {
    up: Vec<Toast>,
}

impl Toasts {
    /// A new message: its author, where it was written (None for a 1:1 chat), its text.
    pub fn push(&mut self, title: &str, place: Option<&str>, text: &str, conversation: &str) {
        // A newer message from the same conversation replaces its toast.
        self.up.retain(|t| t.conversation != conversation);
        self.up.push(Toast {
            title: title.to_string(),
            place: place.map(str::to_string),
            text: text.chars().take(200).collect(),
            conversation: conversation.to_string(),
            since: Instant::now(),
        });
        while self.up.len() > MOST {
            self.up.remove(0);
        }
    }

    /// Draw the toasts in the bottom right of `area`; the conversation of one clicked.
    pub fn show(&mut self, ctx: &egui::Context, area: Rect) -> Option<ConversationId> {
        self.up.retain(|t| t.since.elapsed() < STAYS + FADE);
        if self.up.is_empty() {
            return None;
        }
        let p = colors();
        let mut open = None;
        let mut close = None;
        let mut bottom = area.bottom() - INSET - 90.0;
        for (i, toast) in self.up.iter_mut().enumerate().rev() {
            let alpha = toast.opacity();
            let id = egui::Id::unique(("toast", i));
            let area_response = egui::Area::new(id)
                .order(Order::Foreground)
                .pivot(egui::Align2::RIGHT_BOTTOM)
                .fixed_pos(pos2(area.right() - INSET, bottom))
                .show(ctx, |ui| {
                    ui.set_opacity(alpha);
                    Frame::new()
                        .fill(p.panel)
                        .stroke(Stroke::new(1.0, p.border))
                        .corner_radius(CornerRadius::same(14))
                        .inner_margin(Margin::symmetric(14, 12))
                        .shadow(egui::Shadow { offset: [0, 8], blur: 24, spread: 0, color: egui::Color32::from_black_alpha(60) })
                        .show(ui, |ui| {
                            ui.set_width(WIDTH);
                            ui.horizontal(|ui| {
                                widgets::icon(ui, Icon::Chat, 16.0, p.accent);
                                ui.label(RichText::new(&toast.title).font(ty::item()).color(p.text));
                                if let Some(place) = &toast.place {
                                    ui.add(
                                        egui::Label::new(RichText::new(format!("in {place}")).font(ty::meta()).color(p.faint)).truncate(),
                                    );
                                }
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    if widgets::icon_button(ui, Icon::Close, "Close").clicked() {
                                        close = Some(i);
                                    }
                                });
                            });
                            ui.add(egui::Label::new(RichText::new(&toast.text).font(ty::small()).color(p.dim)).wrap());
                        })
                        .response
                })
                .inner;
            let response = area_response.interact(Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
            if response.hovered() {
                // Stays while being read.
                toast.since = Instant::now() - FADE;
            }
            if response.clicked() && close != Some(i) {
                open = Some(toast.conversation.clone());
            }
            bottom = response.rect.top() - 10.0;
        }
        if let Some(i) = close {
            self.up.remove(i);
        }
        if let Some(c) = &open {
            self.up.retain(|t| t.conversation != *c);
        }
        ctx.request_repaint_after(Duration::from_millis(100));
        open
    }
}
