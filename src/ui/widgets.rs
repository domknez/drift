//! Reusable painted pieces: avatars, message text, cards, reactions, icon buttons.

use super::style::icons::{self, Icon};
use super::style::{colors, metrics, palette, typography as ty};
use crate::model::{Availability, Block, Card, Inline, Picture, Presence, Reaction};
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

/// A status dot centred at `center`, cut out of `behind` (the colour around it).
pub fn status_dot(painter: &egui::Painter, center: egui::Pos2, radius: f32, status: Availability, behind: Color32) {
    let p = colors();
    let (green, red, amber) =
        (Color32::from_rgb(0x5f, 0xb3, 0x36), Color32::from_rgb(0xd7, 0x46, 0x54), Color32::from_rgb(0xf0, 0xa3, 0x1e));
    painter.circle_filled(center, radius + 2.0, behind);
    let fill = match status {
        Availability::Available => green,
        Availability::Busy | Availability::DoNotDisturb => red,
        Availability::BeRightBack | Availability::Away => amber,
        Availability::Offline | Availability::Unknown => {
            let ring = if status == Availability::Offline { p.dim } else { p.faint };
            painter.circle_stroke(center, radius - 0.75, Stroke::new(1.5, ring));
            return;
        }
    };
    painter.circle_filled(center, radius, fill);
    if status == Availability::DoNotDisturb {
        let half = radius * 0.5;
        painter.line_segment([center - vec2(half, 0.0), center + vec2(half, 0.0)], Stroke::new(radius * 0.4, Color32::WHITE));
    }
}

/// Someone's presence as a dot on their avatar: out of office as a magenta dot with
/// an arrow (as in Teams), otherwise their availability; nothing when it's unknown.
pub fn presence_dot(painter: &egui::Painter, center: egui::Pos2, radius: f32, presence: Presence, behind: Color32) {
    if presence.out_of_office {
        painter.circle_filled(center, radius + 2.0, behind);
        painter.circle_filled(center, radius, Color32::from_rgb(0xb4, 0x2b, 0xa8));
        let (half, stroke) = (radius * 0.5, Stroke::new(radius * 0.32, Color32::WHITE));
        painter.line_segment([center - vec2(half, 0.0), center + vec2(half, 0.0)], stroke);
        painter.line_segment([center - vec2(half, 0.0), center + vec2(-half * 0.1, -half * 0.75)], stroke);
        painter.line_segment([center - vec2(half, 0.0), center + vec2(-half * 0.1, half * 0.75)], stroke);
    } else if presence.availability != Availability::Unknown {
        status_dot(painter, center, radius, presence.availability, behind);
    }
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

/// Found text: an amber mark with dark text, legible in both themes.
const MARK: Color32 = Color32::from_rgb(0xff, 0xd2, 0x5e);
const MARK_TEXT: Color32 = Color32::from_rgb(0x1d, 0x1b, 0x19);

thread_local! {
    /// What's being found in the open conversation: marked wherever it shows.
    static HIGHLIGHT: std::cell::RefCell<Option<String>> = const { std::cell::RefCell::new(None) };
}

/// Mark `query` in message text drawn from now on (None: mark nothing).
pub fn set_highlight(query: Option<String>) {
    HIGHLIGHT.with(|h| *h.borrow_mut() = query.map(|q| q.to_lowercase()));
}

/// `text` cut where `query` occurs: (part, is a match).
fn split_matches<'a>(text: &'a str, query: &str) -> Vec<(&'a str, bool)> {
    if query.is_empty() {
        return vec![(text, false)];
    }
    let lower = text.to_lowercase();
    // Lowercasing can change byte lengths; mark only when it doesn't.
    if lower.len() != text.len() {
        return vec![(text, false)];
    }
    let mut parts = Vec::new();
    let mut at = 0;
    while let Some(found) = lower[at..].find(query).map(|f| f + at) {
        if found > at {
            parts.push((&text[at..found], false));
        }
        parts.push((&text[found..found + query.len()], true));
        at = found + query.len();
    }
    if at < text.len() {
        parts.push((&text[at..], false));
    }
    parts
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
                    // Emoji the service has a picture of are drawn in colour, the character
                    // where there's no picture. Either way they take the same square, so the
                    // text doesn't move when a picture comes (or turns out not to).
                    if let Some(emoji) = s.emoji.as_ref().map(|key| Picture::Emoji(key.clone())) {
                        let side = if line_is_emoji_only { 30.0 } else { 19.0 };
                        let (rect, _) = ui.allocate_exact_size(vec2(side, side), Sense::hover());
                        if let Some(texture) = pictures.get(&emoji) {
                            egui::Image::new((texture, rect.size())).paint_at(ui, rect);
                        } else if !pictures.loading(&emoji) {
                            let font = egui::FontId::new(side * 0.8, egui::FontFamily::Proportional);
                            ui.painter().text(rect.center(), Align2::CENTER_CENTER, text, font, color);
                        }
                        continue;
                    }
                    let font = if s.code {
                        ty::code()
                    } else if s.bold || s.mention {
                        ty::body_strong()
                    } else {
                        ty::body()
                    };
                    let mut rich = RichText::new(text).font(font.clone()).color(if s.mention { p.accent } else { color });
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
                    let highlight = HIGHLIGHT.with(|h| h.borrow().clone());
                    match (&s.link, highlight) {
                        (Some(url), _) => {
                            ui.add(egui::Hyperlink::from_label_and_url(rich.color(p.accent), url));
                        }
                        (None, Some(query)) if text.to_lowercase().contains(&query) => {
                            // The found text, marked; the rest as it is.
                            for (part, hit) in split_matches(text, &query) {
                                let mut piece = RichText::new(part).font(font.clone()).color(if hit { MARK_TEXT } else { color });
                                if hit {
                                    piece = piece.background_color(MARK);
                                }
                                ui.label(piece);
                            }
                        }
                        (None, _) => {
                            ui.label(rich);
                        }
                    }
                }
            });
        }
    });
}

/// What was clicked in a message's blocks.
pub enum Clicked {
    /// A quote: the id of the message it's of.
    Quote(String),
    /// A picture: its URL.
    Image(String),
}

/// A message's blocks: paragraphs, tables, quotes, code and pictures.
pub fn blocks(ui: &mut Ui, pictures: &mut Pictures, blocks: &[Block]) -> Option<Clicked> {
    let p = colors();
    let mut clicked = None;
    for block in blocks {
        match block {
            Block::Text(runs) => rich_text(ui, pictures, runs, p.text),
            Block::Quote(runs, of) => {
                if quote(ui, pictures, runs, of.is_some()) {
                    clicked = of.clone().map(Clicked::Quote);
                }
            }
            Block::Code(code) => {
                Frame::new().fill(p.code).corner_radius(CornerRadius::same(metrics::RADIUS)).inner_margin(Margin::symmetric(12, 10)).show(
                    ui,
                    |ui| {
                        ui.set_width(ui.available_width());
                        // Long lines wrap: a sideways scroll area inside the scrolling list
                        // flips its scroll bar every frame, and the whole list flickers.
                        ui.add(egui::Label::new(RichText::new(code).font(ty::code()).color(p.text)).wrap());
                    },
                );
            }
            Block::Table { rows, header } => table(ui, pictures, rows, *header),
            Block::Image { url, size } => {
                if image(ui, pictures, url, *size) {
                    clicked = Some(Clicked::Image(url.clone()));
                }
            }
        }
    }
    clicked
}

/// A picture in a message, at most this wide, its corners rounded; a soft placeholder
/// of the right shape while it loads. True when it was clicked (to see it big).
fn image(ui: &mut Ui, pictures: &mut Pictures, url: &str, size: Option<(u32, u32)>) -> bool {
    const MAX: f32 = 320.0;
    let p = colors();
    let texture = pictures.get(&Picture::Image(url.to_string()));
    // The size the message gives comes first: the picture then takes the place its
    // placeholder had, and the list doesn't move when it loads.
    let natural = size
        .map(|(w, h)| (w as f32, h as f32))
        .or_else(|| texture.and_then(|t| ui.ctx().tex_manager().read().meta(t).map(|m| (m.size[0] as f32, m.size[1] as f32))))
        .unwrap_or((MAX, MAX * 0.6));
    let scale = (MAX.min(ui.available_width()) / natural.0).min(1.0);
    let shown = vec2(natural.0 * scale, natural.1 * scale);
    let (rect, response) = ui.allocate_exact_size(shown, Sense::click());
    match texture {
        Some(t) => {
            egui::Image::new((t, shown)).corner_radius(10).paint_at(ui, rect);
            response.on_hover_cursor(egui::CursorIcon::ZoomIn).clicked()
        }
        None => {
            ui.painter().rect_filled(rect, CornerRadius::same(10), p.hover);
            false
        }
    }
}

/// A picture over the whole window, as big as fits; a click anywhere or Esc closes
/// it. False once it's closed.
pub fn lightbox(ctx: &egui::Context, pictures: &mut Pictures, url: &str) -> bool {
    let Some(texture) = pictures.get(&Picture::Image(url.to_string())) else { return false };
    let natural = ctx.tex_manager().read().meta(texture).map_or(vec2(1.0, 1.0), |m| vec2(m.size[0] as f32, m.size[1] as f32));
    let mut open = !ctx.input(|i| i.key_pressed(egui::Key::Escape));
    egui::Area::new(egui::Id::unique("lightbox")).order(egui::Order::Foreground).fixed_pos(egui::Pos2::ZERO).show(ctx, |ui| {
        let screen = ctx.content_rect();
        let response = ui.allocate_rect(screen, Sense::click());
        ui.painter().rect_filled(screen, CornerRadius::ZERO, Color32::from_black_alpha(215));
        let room = screen.shrink(40.0).size();
        let scale = (room.x / natural.x).min(room.y / natural.y).min(1.0);
        let rect = Rect::from_center_size(screen.center(), natural * scale);
        egui::Image::new((texture, rect.size())).corner_radius(8).paint_at(ui, rect);
        if response.clicked() {
            open = false;
        }
    });
    open
}

/// A quote (or the message a reply quotes): a tinted box with an accent edge, the
/// quoted author in the accent. A reply's quote can be clicked (true when it was).
fn quote(ui: &mut Ui, pictures: &mut Pictures, runs: &[Inline], clickable: bool) -> bool {
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
    clickable
        && response.interact(Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text("Go to this message").clicked()
}

/// A table with hairlines between its cells.
fn table(ui: &mut Ui, pictures: &mut Pictures, rows: &[Vec<Vec<Inline>>], header: bool) {
    let p = colors();
    let columns = rows.iter().map(Vec::len).max().unwrap_or(0);
    Frame::new().stroke(Stroke::new(1.0, p.line)).corner_radius(CornerRadius::same(metrics::RADIUS)).show(ui, |ui| {
        // Too wide a table is cut at the bubble's edge (no sideways scroll area: inside
        // the scrolling list it flips its scroll bar every frame, and the list flickers).
        let clip = ui.clip_rect().intersect(ui.max_rect());
        ui.scope(|ui| {
            ui.set_clip_rect(clip);
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

/// A file shared in a message, as a row with its kind and name; clicking it opens
/// it in the browser (where it can be downloaded too).
pub fn file(ui: &mut Ui, file: &crate::model::File) {
    let p = colors();
    ui.add_space(2.0);
    let response = Frame::new()
        .fill(p.panel)
        .corner_radius(CornerRadius::same(12))
        .inner_margin(Margin::symmetric(12, 8))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 10.0;
                let (tile, _) = ui.allocate_exact_size(vec2(30.0, 30.0), Sense::hover());
                ui.painter().rect_filled(tile, CornerRadius::same(8), p.selected);
                icons::paint(ui.painter(), Rect::from_center_size(tile.center(), vec2(16.0, 16.0)), Icon::File, p.accent);
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 1.0;
                    ui.add(egui::Label::new(RichText::new(&file.name).font(ty::small_strong()).color(p.text)).truncate());
                    ui.label(RichText::new(file.kind()).font(ty::meta()).color(p.faint));
                });
            });
        })
        .response;
    let response = response.interact(Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text("Open in the browser");
    if response.clicked() {
        ui.ctx().open_url(egui::OpenUrl::new_tab(&file.url));
    }
    response.context_menu(|ui| {
        if ui.button("Open in the browser").clicked() {
            ui.ctx().open_url(egui::OpenUrl::new_tab(&file.url));
            ui.close();
        }
        if ui.button("Copy link").clicked() {
            ui.ctx().copy_text(file.url.clone());
            ui.close();
        }
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
    let layout = if right { egui::Layout::right_to_left(egui::Align::Min) } else { egui::Layout::left_to_right(egui::Align::Min) };
    ui.with_layout(layout.with_main_wrap(!right), |ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        for r in reactions {
            let (fill, text) = if r.mine { (p.selected, p.accent) } else { (p.surface, p.dim) };
            // Painted at its own size: an emoji (picture, or the character) and the count.
            let count = ui.painter().layout_no_wrap(r.count.to_string(), ty::small_strong(), text);
            let emoji = Picture::Emoji(r.key.clone());
            let picture = pictures.get(&emoji);
            // The emoji takes the same place whether it's a picture, still loading, or the character.
            let glyph =
                (picture.is_none() && !pictures.loading(&emoji)).then(|| ui.painter().layout_no_wrap(r.emoji(), ty::small_strong(), text));
            let emoji_w = 17.0;
            let size = vec2(9.0 + emoji_w + 4.0 + count.size().x + 9.0, 24.0);
            let (rect, response) = ui.allocate_exact_size(size, Sense::click());
            // A ring in the panel's colour cuts the pill out of the bubble it overlaps.
            ui.painter().rect_filled(rect.expand(2.0), CornerRadius::same(14), p.panel);
            let stroke = if r.mine { Stroke::new(1.0, p.accent.gamma_multiply(0.6)) } else { Stroke::NONE };
            ui.painter().rect(rect, CornerRadius::same(12), fill, stroke, egui::StrokeKind::Inside);
            let emoji = Rect::from_min_size(egui::pos2(rect.left() + 9.0, rect.center().y - 8.5), vec2(emoji_w, 17.0));
            match (picture, glyph) {
                (Some(texture), _) => {
                    egui::Image::new((texture, emoji.size())).paint_at(ui, emoji);
                }
                (None, Some(g)) => ui.painter().galley(emoji.center() - g.size() / 2.0, g, text),
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
    use super::{initials, split_matches};

    #[test]
    fn finds_matches_to_mark() {
        assert_eq!(
            split_matches("Release cut at 15:00, release!", "release"),
            [("Release", true), (" cut at 15:00, ", false), ("release", true), ("!", false)]
        );
        assert_eq!(split_matches("nothing here", "zzz"), [("nothing here", false)]);
    }

    #[test]
    fn makes_initials() {
        assert_eq!(initials("Maya Lindqvist"), "ML");
        assert_eq!(initials("Tomás de la Rivera"), "TR");
        assert_eq!(initials("repos"), "R");
        assert_eq!(initials("Maya, Jonas and 1 more"), "MM");
        assert_eq!(initials(""), "?");
    }
}
