//! Reusable painted pieces: avatars, message text, cards, reactions, icon buttons.

use super::style::icons::{self, Icon};
use super::style::{colors, metrics, palette, typography as ty};
use crate::model::{Card, Inline, Reaction};
use eframe::egui::{self, Align2, Color32, CornerRadius, Frame, Margin, Rect, Response, RichText, Sense, Stroke, Ui, vec2};

/// A round avatar with the person's initials.
pub fn avatar(ui: &mut Ui, id: &str, name: &str, size: f32) -> Response {
    let (rect, response) = ui.allocate_exact_size(vec2(size, size), Sense::hover());
    let painter = ui.painter();
    painter.circle_filled(rect.center(), size / 2.0, palette::avatar(id));
    painter.text(rect.center(), Align2::CENTER_CENTER, initials(name), ty::initials(size), Color32::WHITE);
    response
}

/// A team's tile: its initial on a rounded square.
pub fn tile(ui: &mut Ui, id: &str, name: &str, size: f32) {
    let (rect, _) = ui.allocate_exact_size(vec2(size, size), Sense::hover());
    ui.painter().rect_filled(rect, CornerRadius::same(5), palette::avatar(id));
    let initial: String = name.chars().find(|c| c.is_alphanumeric()).map(|c| c.to_uppercase().collect()).unwrap_or_default();
    ui.painter().text(rect.center(), Align2::CENTER_CENTER, initial, ty::initials(size * 1.3), Color32::WHITE);
}

/// "Maya Lindqvist" → "ML"; one word → its first letter.
pub fn initials(name: &str) -> String {
    let words: Vec<&str> =
        name.split(|c: char| c.is_whitespace() || c == ',').filter(|w| w.chars().next().is_some_and(char::is_alphanumeric)).collect();
    let first = |w: &str| w.chars().next().map(|c| c.to_uppercase().collect::<String>()).unwrap_or_default();
    match words.as_slice() {
        [] => "?".into(),
        [one] => first(one),
        [a, .., b] => first(a) + &first(b),
    }
}

/// An icon in a square of `size`.
pub fn icon(ui: &mut Ui, icon: Icon, size: f32, color: Color32) -> Response {
    let (rect, response) = ui.allocate_exact_size(vec2(size, size), Sense::hover());
    icons::paint(ui.painter(), rect, icon, color);
    response
}

/// A borderless square button with an icon, highlighted under the pointer.
pub fn icon_button(ui: &mut Ui, icon: Icon, hint: &str) -> Response {
    let p = colors();
    let (rect, response) = ui.allocate_exact_size(vec2(30.0, 30.0), Sense::click());
    if response.hovered() {
        ui.painter().rect_filled(rect, CornerRadius::same(metrics::RADIUS), p.hover);
    }
    let color = if response.hovered() { p.text } else { p.dim };
    icons::paint(ui.painter(), Rect::from_center_size(rect.center(), vec2(17.0, 17.0)), icon, color);
    response.on_hover_text(hint)
}

/// A message's text: styled runs, wrapped to the width there is; links open.
pub fn rich_text(ui: &mut Ui, body: &[Inline], color: Color32) {
    let p = colors();
    let mut lines: Vec<Vec<(&str, &Inline)>> = vec![Vec::new()];
    for run in body {
        for (i, part) in run.text.split('\n').enumerate() {
            if i > 0 {
                lines.push(Vec::new());
            }
            if !part.is_empty() {
                lines.last_mut().expect("a line").push((part, run));
            }
        }
    }
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 3.0;
        for line in lines {
            if line.is_empty() {
                ui.add_space(4.0);
                continue;
            }
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                for (text, run) in line {
                    let s = &run.span;
                    let font = if s.code {
                        ty::code()
                    } else if s.bold || s.mention {
                        ty::body_strong()
                    } else {
                        ty::body()
                    };
                    let mut rich = RichText::new(text).font(font).color(if s.mention { p.accent } else { color });
                    if s.italic {
                        rich = rich.italics();
                    }
                    if s.code {
                        rich = rich.background_color(p.code);
                    }
                    match &s.link {
                        Some(url) => {
                            ui.add(egui::Hyperlink::from_label_and_url(rich.color(p.accent), url));
                        }
                        None => {
                            ui.label(rich);
                        }
                    }
                }
            });
        }
    });
}

/// A card a bot posted, as one compact row; clicking it opens its link.
pub fn card(ui: &mut Ui, card: &Card) {
    let p = colors();
    let response = Frame::new()
        .fill(p.bg)
        .stroke(Stroke::new(1.0, p.line))
        .corner_radius(CornerRadius::same(10))
        .inner_margin(Margin::symmetric(12, 10))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                let (tile, _) = ui.allocate_exact_size(vec2(32.0, 32.0), Sense::hover());
                ui.painter().rect_filled(tile, CornerRadius::same(8), p.selected);
                icons::paint(ui.painter(), Rect::from_center_size(tile.center(), vec2(18.0, 18.0)), Icon::Card, p.accent);
                ui.add_space(4.0);
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 2.0;
                    ui.add(egui::Label::new(RichText::new(&card.title).font(ty::small_strong()).color(p.text)).truncate());
                    if !card.detail.is_empty() {
                        ui.add(egui::Label::new(RichText::new(&card.detail).font(ty::meta()).color(p.faint)).truncate());
                    }
                });
            });
        })
        .response;
    if let Some(url) = &card.link {
        let response = response.interact(Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
        if response.clicked() {
            ui.ctx().open_url(egui::OpenUrl::new_tab(url));
        }
    }
}

/// Reactions under a message, as small pills; the user's own ones in the accent.
pub fn reactions(ui: &mut Ui, reactions: &[Reaction]) {
    if reactions.is_empty() {
        return;
    }
    let p = colors();
    ui.add_space(2.0);
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        for r in reactions {
            let (fill, stroke, text) = if r.mine { (p.selected, p.accent, p.accent) } else { (Color32::TRANSPARENT, p.line, p.dim) };
            Frame::new()
                .fill(fill)
                .stroke(Stroke::new(1.0, stroke))
                .corner_radius(CornerRadius::same(255))
                .inner_margin(Margin::symmetric(8, 2))
                .show(ui, |ui| {
                    ui.label(RichText::new(format!("{} {}", r.emoji(), r.count)).font(ty::small()).color(text));
                });
        }
    });
}

/// A count in the accent (unread messages); a dot for "something new".
pub fn unread_dot(ui: &mut Ui) {
    let (rect, _) = ui.allocate_exact_size(vec2(8.0, 8.0), Sense::hover());
    ui.painter().circle_filled(rect.center(), 3.5, colors().accent);
}

#[cfg(test)]
mod tests {
    use super::initials;

    #[test]
    fn makes_initials() {
        assert_eq!(initials("Maya Lindqvist"), "ML");
        assert_eq!(initials("Tomás de la Rivera"), "TR");
        assert_eq!(initials("repos"), "R");
        assert_eq!(initials("Maya, Jonas and 1 more"), "MM");
        assert_eq!(initials(""), "?");
    }
}
