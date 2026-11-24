//! "Remind me" at a time of the user's choosing: a day (stepped through with ‹ ›)
//! and a time of day as typed, over the window.

use super::super::style::{colors, typography as ty};
use super::super::workspace::Workspace;
use crate::model::{Message, text, time};
use eframe::egui::{self, CornerRadius, Frame, Margin, RichText, Stroke, vec2};

const WIDTH: f32 = 380.0;
/// How far ahead a reminder may be set, in days.
const FURTHEST: i64 = 365;
/// Times offered with a click.
const TIMES: [(&str, i64); 4] = [("09:00", 9 * 60), ("12:00", 12 * 60), ("15:00", 15 * 60), ("17:00", 17 * 60)];

/// A reminder being set: for which message, on which day (days from today), at
/// what time (as typed).
pub struct Custom {
    pub message: Message,
    pub day: i64,
    pub time: String,
    focused: bool,
}

impl Custom {
    /// Starting at the next full hour (tomorrow's first, past 23:00).
    pub fn new(message: Message) -> Custom {
        let next = time::minutes_of_day(time::now_ms()) / 60 + 1;
        let (day, hour) = if next >= 24 { (1, 9) } else { (0, next) };
        Custom { message, day, time: format!("{hour:02}:00"), focused: false }
    }

    /// When the reminder would be, or why it can't be.
    fn when(&self, now: i64) -> Result<i64, &'static str> {
        let minutes = time::parse_clock(&self.time).ok_or("Type a time like 14:30")?;
        let at = time::local_at(now, self.day, minutes);
        if at <= now { Err("That time has passed") } else { Ok(at) }
    }
}

pub fn show(ctx: &egui::Context, ws: &mut Workspace) {
    let Some(custom) = &mut ws.custom_reminder else { return };
    let p = colors();
    let now = time::now_ms();
    let mut set = None;
    let mut close = false;
    let frame = Frame::new().fill(p.panel).corner_radius(CornerRadius::same(16)).inner_margin(Margin::same(20));
    let modal = egui::Modal::new(egui::Id::unique("custom-reminder")).frame(frame).show(ctx, |ui| {
        ui.set_width(WIDTH);
        ui.spacing_mut().item_spacing.y = 6.0;
        ui.label(RichText::new("Remind me").font(ty::title()).color(p.text));
        let said: String = text::said(&custom.message.body).chars().take(90).collect();
        let about = if said.trim().is_empty() { custom.message.author.name.clone() } else { format!("{}: {said}", custom.message.author.name) };
        ui.add(egui::Label::new(RichText::new(about).font(ty::small()).color(p.faint)).truncate());
        ui.add_space(10.0);

        ui.label(RichText::new("DAY").font(ty::section()).color(p.faint));
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            let step = |ui: &mut egui::Ui, label: &str, enabled: bool| {
                ui.add_enabled(enabled, egui::Button::new(RichText::new(label).font(ty::body_strong())).min_size(vec2(32.0, 30.0))).clicked()
            };
            if step(ui, "‹", custom.day > 0) {
                custom.day -= 1;
            }
            let (field, _) = ui.allocate_exact_size(vec2(WIDTH - 2.0 * (32.0 + 6.0), 30.0), egui::Sense::hover());
            ui.painter().rect(field, CornerRadius::same(8), p.surface, Stroke::new(1.0, p.border), egui::StrokeKind::Inside);
            let day = time::day_ahead(now, custom.day);
            ui.painter().text(field.center(), egui::Align2::CENTER_CENTER, day, ty::body(), p.text);
            if step(ui, "›", custom.day < FURTHEST) {
                custom.day += 1;
            }
        });
        ui.add_space(8.0);

        ui.label(RichText::new("TIME").font(ty::section()).color(p.faint));
        let mut enter = false;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            let field = ui.add(
                egui::TextEdit::singleline(&mut custom.time)
                    .font(ty::body())
                    .hint_text("14:30")
                    .desired_width(84.0)
                    .margin(Margin::symmetric(8, 6)),
            );
            if !custom.focused {
                custom.focused = true;
                field.request_focus();
            }
            enter = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            ui.add_space(6.0);
            for (label, minutes) in TIMES {
                let on = time::parse_clock(&custom.time) == Some(minutes);
                if ui.add(egui::Button::new(RichText::new(label).font(ty::small())).selected(on).min_size(vec2(0.0, 30.0))).clicked() {
                    custom.time = label.to_string();
                }
            }
        });
        ui.add_space(10.0);

        let when = custom.when(now);
        let (line, color) = match when {
            Ok(at) => (format!("You'll be reminded {}", spoken(now, custom.day, at)), p.dim),
            Err(why) => (why.to_string(), p.danger),
        };
        ui.label(RichText::new(line).font(ty::small()).color(color));
        ui.add_space(12.0);

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            let button = egui::Button::new(RichText::new("Set reminder").font(ty::small_strong()).color(p.on_accent))
                .fill(p.accent_fill)
                .corner_radius(CornerRadius::same(8))
                .min_size(vec2(0.0, 32.0));
            if ui.add_enabled(when.is_ok(), button).clicked() || (enter && when.is_ok()) {
                set = when.ok();
            }
            if ui.add(egui::Button::new(RichText::new("Cancel").font(ty::small())).min_size(vec2(0.0, 32.0))).clicked() {
                close = true;
            }
        });
    });
    if let Some(at) = set {
        let message = custom.message.clone();
        ws.remind(&message, Some(at));
        close = true;
    }
    if close || modal.should_close() {
        ws.custom_reminder = None;
    }
}

/// When, as said after "You'll be reminded": `today at 14:05`, `tomorrow at
/// 09:00`, `on Wednesday 26 November at 09:00`.
fn spoken(now: i64, day: i64, at: i64) -> String {
    let clock = time::clock(at);
    match day {
        0 => format!("today at {clock}"),
        1 => format!("tomorrow at {clock}"),
        _ => format!("on {} at {clock}", time::day_ahead(now, day)),
    }
}
