//! Reusable painted pieces: avatars, message text, cards, reactions, icon buttons.

use super::style::icons::{self, Icon};
use super::style::{colors, metrics, palette, typography as ty};
use crate::model::{Block, Card, Inline, Picture, Reaction};
use crate::ui::pictures::Pictures;
use eframe::egui::{self, Align2, Color32, CornerRadius, Frame, Margin, Rect, Response, RichText, Sense, Stroke, TextureId, Ui, vec2};

/// A round avatar: the picture once it's loaded, the initials until then.
pub fn avatar(ui: &mut Ui, id: &str, name: &str, size: f32, picture: Option<TextureId>) -> Response {
    let (rect, response) = ui.allocate_exact_size(vec2(size, size), Sense::hover());
    if let Some(texture) = picture {
        egui::Image::new((texture, vec2(size, size))).corner_radius(size / 2.0).paint_at(ui, rect);
        return response;
    }
    let painter = ui.painter();
    // Bots and connectors (ids starting 28:) get a rounded square, people a circle.
    if id.starts_with("28:") {
        painter.rect_filled(rect, CornerRadius::same((size * 0.22) as u8), palette::avatar(id));
        icons::paint(painter, Rect::from_center_size(rect.center(), vec2(size * 0.5, size * 0.5)), Icon::Card, Color32::WHITE);
        return response;
    }
    painter.circle_filled(rect.center(), size / 2.0, palette::avatar(id));
    painter.text(rect.center(), Align2::CENTER_CENTER, initials(name), ty::initials(size), Color32::WHITE);
    response
}

/// A team's tile: its picture, or its initial on a rounded square.
pub fn tile(ui: &mut Ui, id: &str, name: &str, size: f32, picture: Option<TextureId>) {
    let (rect, _) = ui.allocate_exact_size(vec2(size, size), Sense::hover());
    if let Some(texture) = picture {
        egui::Image::new((texture, vec2(size, size))).corner_radius(5).paint_at(ui, rect);
        return;
    }
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
pub fn rich_text(ui: &mut Ui, pictures: &mut Pictures, body: &[Inline], color: Color32) {
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
            // A line of nothing but emoji shows them big, as chat apps do.
            let line_is_emoji_only = line.iter().all(|(text, run)| run.span.emoji.is_some() || text.trim().is_empty());
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                for (text, run) in line {
                    let s = &run.span;
                    // Emoji the service has a picture of are drawn in colour.
                    if let Some(texture) = s.emoji.as_ref().and_then(|key| pictures.get(&Picture::Emoji(key.clone()))) {
                        let size = if line_is_emoji_only { 30.0 } else { 19.0 };
                        ui.add(egui::Image::new((texture, vec2(size, size))));
                        continue;
                    }
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
                    if s.strike {
                        rich = rich.strikethrough();
                    }
                    if s.underline {
                        rich = rich.underline();
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

/// A message's blocks: paragraphs, tables, quotes and code.
pub fn blocks(ui: &mut Ui, pictures: &mut Pictures, blocks: &[Block]) {
    let p = colors();
    for block in blocks {
        match block {
            Block::Text(runs) => rich_text(ui, pictures, runs, p.text),
            Block::Quote(runs) => quote(ui, pictures, runs),
            Block::Code(code) => {
                Frame::new().fill(p.code).corner_radius(CornerRadius::same(metrics::RADIUS)).inner_margin(Margin::symmetric(12, 10)).show(
                    ui,
                    |ui| {
                        ui.set_width(ui.available_width());
                        egui::ScrollArea::horizontal().id_salt(code.as_ptr()).show(ui, |ui| {
                            ui.add(egui::Label::new(RichText::new(code).font(ty::code()).color(p.text)).extend());
                        });
                    },
                );
            }
            Block::Table { rows, header } => table(ui, pictures, rows, *header),
        }
    }
}

/// A quote (or the message a reply quotes): a tinted box with an accent edge, the
/// quoted author in the accent.
fn quote(ui: &mut Ui, pictures: &mut Pictures, runs: &[Inline]) {
    let p = colors();
    let response = Frame::new()
        .fill(p.accent_fill.gamma_multiply(0.13))
        .corner_radius(CornerRadius::same(8))
        .inner_margin(Margin { left: 13, right: 10, top: 7, bottom: 7 })
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            // The author's name (bold) in the accent, the quoted text dimmer.
            let (name, rest): (Vec<Inline>, Vec<Inline>) = runs.iter().cloned().partition(|r| r.span.bold && !r.text.contains('\n'));
            if !name.is_empty() {
                let who: String = name.iter().map(|r| r.text.as_str()).collect();
                ui.label(RichText::new(who.trim()).font(ty::small_strong()).color(p.accent));
            }
            rich_text(ui, pictures, &rest, p.dim);
        })
        .response;
    let bar = Rect::from_min_size(response.rect.min, vec2(3.0, response.rect.height()));
    ui.painter().rect_filled(bar, CornerRadius { nw: 8, sw: 8, ne: 0, se: 0 }, p.accent);
}

/// A table with hairlines between its cells; wide ones scroll sideways.
fn table(ui: &mut Ui, pictures: &mut Pictures, rows: &[Vec<Vec<Inline>>], header: bool) {
    let p = colors();
    let columns = rows.iter().map(Vec::len).max().unwrap_or(0);
    Frame::new().stroke(Stroke::new(1.0, p.line)).corner_radius(CornerRadius::same(metrics::RADIUS)).show(ui, |ui| {
        egui::ScrollArea::horizontal().id_salt(rows.as_ptr()).show(ui, |ui| {
            let id = ui.next_auto_id();
            egui::Grid::new(id).spacing(vec2(0.0, 0.0)).min_col_width(40.0).max_col_width(320.0).show(ui, |ui| {
                for (r, row) in rows.iter().enumerate() {
                    let is_header = header && r == 0;
                    for c in 0..columns {
                        let response = Frame::new()
                            .fill(if is_header { p.hover } else { egui::Color32::TRANSPARENT })
                            .inner_margin(Margin::symmetric(10, 6))
                            .show(ui, |ui| match row.get(c) {
                                Some(cell) if !cell.is_empty() => rich_text(ui, pictures, cell, p.text),
                                _ => {
                                    ui.label("");
                                }
                            })
                            .response;
                        let r = response.rect;
                        if c > 0 {
                            ui.painter().vline(r.left(), r.y_range(), Stroke::new(1.0, p.line));
                        }
                        if !std::ptr::eq(row, rows.last().expect("a row")) {
                            ui.painter().hline(r.x_range(), r.bottom(), Stroke::new(1.0, p.line));
                        }
                    }
                    ui.end_row();
                }
            });
        });
    });
}

/// A card a bot posted, as one compact row; clicking it opens its link.
pub fn card(ui: &mut Ui, card: &Card) {
    let p = colors();
    ui.add_space(2.0);
    let link = card.link.is_some();
    let response = Frame::new()
        .fill(p.panel)
        .corner_radius(CornerRadius::same(12))
        .inner_margin(Margin::symmetric(12, 10))
        .show(ui, |ui| {
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = 12.0;
                let (tile, _) = ui.allocate_exact_size(vec2(34.0, 34.0), Sense::hover());
                ui.painter().rect_filled(tile, CornerRadius::same(10), p.selected);
                icons::paint(ui.painter(), Rect::from_center_size(tile.center(), vec2(18.0, 18.0)), Icon::Card, p.accent);
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 3.0;
                    if !card.title.is_empty() {
                        let color = if link { p.accent } else { p.text };
                        ui.add(egui::Label::new(RichText::new(&card.title).font(ty::small_strong()).color(color)).wrap());
                    }
                    if !card.detail.is_empty() {
                        ui.add(egui::Label::new(RichText::new(&card.detail).font(ty::mono_small()).color(p.faint)).truncate());
                    }
                    if !card.facts.is_empty() {
                        ui.add_space(3.0);
                        egui::Grid::new(ui.next_auto_id()).num_columns(2).spacing(vec2(12.0, 3.0)).show(ui, |ui| {
                            for (label, value) in &card.facts {
                                ui.label(RichText::new(label).font(ty::meta()).color(p.faint));
                                ui.add(egui::Label::new(RichText::new(value).font(ty::small()).color(p.dim)).truncate());
                                ui.end_row();
                            }
                        });
                    }
                });
            });
        })
        .response;
    if let Some(url) = &card.link {
        let response = response.interact(Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(url);
        if response.clicked() {
            ui.ctx().open_url(egui::OpenUrl::new_tab(url));
        }
    }
}

/// Reactions under a message, as small pills; the user's own ones in the accent.
/// Clicking one returns its key (to add it, or take it back).
pub fn reactions(ui: &mut Ui, pictures: &mut Pictures, reactions: &[Reaction], right: bool) -> Option<String> {
    if reactions.is_empty() {
        return None;
    }
    let p = colors();
    let mut clicked = None;
    ui.add_space(4.0);
    let layout = if right { egui::Layout::right_to_left(egui::Align::Min) } else { egui::Layout::left_to_right(egui::Align::Min) };
    ui.with_layout(layout.with_main_wrap(!right), |ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        for r in reactions {
            let (fill, text) = if r.mine { (p.selected, p.accent) } else { (p.surface, p.dim) };
            // Painted at its own size: an emoji (picture, or the character) and the count.
            let count = ui.painter().layout_no_wrap(r.count.to_string(), ty::small_strong(), text);
            let picture = pictures.get(&Picture::Emoji(r.key.clone()));
            let glyph = picture.is_none().then(|| ui.painter().layout_no_wrap(r.emoji(), ty::small_strong(), text));
            let emoji_w = glyph.as_ref().map_or(17.0, |g| g.size().x);
            let size = vec2(9.0 + emoji_w + 4.0 + count.size().x + 9.0, 24.0);
            let (rect, response) = ui.allocate_exact_size(size, Sense::click());
            let stroke = if r.mine { Stroke::new(1.0, p.accent.gamma_multiply(0.6)) } else { Stroke::NONE };
            ui.painter().rect(rect, CornerRadius::same(12), fill, stroke, egui::StrokeKind::Inside);
            let emoji = Rect::from_min_size(egui::pos2(rect.left() + 9.0, rect.center().y - 8.5), vec2(emoji_w, 17.0));
            match (picture, glyph) {
                (Some(texture), _) => {
                    egui::Image::new((texture, emoji.size())).paint_at(ui, emoji);
                }
                (None, Some(g)) => ui.painter().galley(egui::pos2(emoji.left(), rect.center().y - g.size().y / 2.0), g, text),
                (None, None) => {}
            }
            ui.painter().galley(egui::pos2(emoji.right() + 4.0, rect.center().y - count.size().y / 2.0), count, text);
            if response.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                clicked = Some(r.key.clone());
            }
        }
    });
    clicked
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
