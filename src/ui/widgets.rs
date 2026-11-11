//! Reusable painted pieces: avatars, message text, cards, reactions, icon buttons.

use super::style::icons::{self, Icon};
use super::style::{colors, metrics, palette, typography as ty};
use crate::model::{Availability, Block, Card, Inline, Picture, Presence, Reaction};
use crate::ui::pictures::Pictures;
use eframe::egui::{
    self, Align2, Color32, CornerRadius, Frame, Margin, Rect, Response, RichText, Sense, Stroke, TextureId, Ui, pos2, vec2,
};

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
    // A mark in each colour, as Teams has, so it isn't told by colour alone: a tick,
    // a bar, the hands of a clock (too small to draw on the tiniest dots).
    let r = radius;
    let mark = Stroke::new((r * 0.32).max(1.2), Color32::WHITE);
    match status {
        Availability::DoNotDisturb => {
            painter.line_segment([center - vec2(r * 0.5, 0.0), center + vec2(r * 0.5, 0.0)], Stroke::new(r * 0.4, Color32::WHITE));
        }
        _ if r < 5.0 => {}
        Availability::Available => {
            painter.add(egui::Shape::line(
                vec![center + vec2(-r * 0.45, 0.0), center + vec2(-r * 0.1, r * 0.35), center + vec2(r * 0.45, -r * 0.35)],
                mark,
            ));
        }
        Availability::BeRightBack | Availability::Away => {
            painter.add(egui::Shape::line(vec![center + vec2(0.0, -r * 0.5), center, center + vec2(r * 0.4, r * 0.2)], mark));
        }
        _ => {}
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

/// What a control drift draws itself is, for screen readers (its kind and name),
/// and a ring around it while the keyboard is on it.
pub fn control(ui: &Ui, response: &Response, kind: egui::Role, name: &str) {
    response.widget_info(|| egui::WidgetInfo::labeled(kind, ui.is_enabled(), name));
    if response.has_focus() {
        let ring = Stroke::new(2.0, colors().accent);
        ui.painter().rect_stroke(response.rect.expand(1.0), CornerRadius::same(8), ring, egui::StrokeKind::Outside);
    }
}

/// A borderless square button with an icon, highlighted under the pointer.
pub fn icon_button(ui: &mut Ui, icon: Icon, hint: &str) -> Response {
    let p = colors();
    let (rect, response) = ui.allocate_exact_size(vec2(30.0, 30.0), Sense::click());
    control(ui, &response, egui::Role::Button, hint);
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
    /// Emoji are drawn bigger still: the message is nothing but emoji.
    static JUMBO: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    /// A table's cells are being drawn.
    static IN_TABLE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Draw emoji in message text from now on at their biggest (a message of only emoji), or not.
pub fn set_jumbo(on: bool) {
    JUMBO.with(|j| j.set(on));
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
            // Not in a table's cell: there they keep the size of the text beside them.
            let line_is_emoji_only =
                !IN_TABLE.with(std::cell::Cell::get) && line.iter().all(|(text, run)| run.span.emoji.is_some() || text.trim().is_empty());
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                for (text, run) in line {
                    let s = &run.span;
                    // Emoji the service has a picture of are drawn in colour, the character
                    // where there's no picture. Either way they take the same square, so the
                    // text doesn't move when a picture comes (or turns out not to).
                    if let Some(emoji) = s.emoji.as_ref().map(|key| Picture::Emoji(key.clone())) {
                        let side = if JUMBO.with(std::cell::Cell::get) {
                            44.0
                        } else if line_is_emoji_only {
                            30.0
                        } else {
                            19.0
                        };
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
                        // A link is text like the rest (selected and copied with it, wrapped
                        // when long) that opens its address when clicked.
                        (Some(url), _) => {
                            let link = selectable(ui, egui::Label::new(rich.color(p.accent)).sense(Sense::click()));
                            let link = link.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(url);
                            if link.clicked() {
                                ui.ctx().open_url(egui::OpenUrl::new_tab(url));
                            }
                        }
                        (None, Some(query)) if text.to_lowercase().contains(&query) => {
                            // The found text, marked; the rest as it is.
                            for (part, hit) in split_matches(text, &query) {
                                let mut piece = RichText::new(part).font(font.clone()).color(if hit { MARK_TEXT } else { color });
                                if hit {
                                    piece = piece.background_color(MARK);
                                }
                                selectable(ui, egui::Label::new(piece));
                            }
                        }
                        // Message text can be selected and copied (across runs and lines too).
                        (None, _) => {
                            selectable(ui, egui::Label::new(rich));
                        }
                    }
                }
            });
        }
    });
}

/// Message text that can be selected and copied. egui follows a selection only
/// through the text on screen, and drops it once either end has scrolled out of
/// view; so while a selection is being made (or is there), the text out of view
/// takes part too, and a selection can be dragged on while the list scrolls.
fn selectable(ui: &mut Ui, label: egui::Label) -> Response {
    use egui::text_selection::LabelSelectionState;
    let (pos, galley, response) = label.selectable(true).layout_in_ui(ui);
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, ui.is_enabled(), galley.text()));
    let selecting = || ui.input(|i| i.pointer.primary_down()) || ui.ctx().plugin::<LabelSelectionState>().lock().has_selection();
    if ui.is_visible() && (ui.is_rect_visible(response.rect) || selecting()) {
        LabelSelectionState::label_text_selection(ui, &response, pos, galley, ui.style().visuals.text_color(), Stroke::NONE);
    }
    response
}

/// What was clicked in a message's blocks.
pub enum Clicked {
    /// A quote: the id of the message it's of.
    Quote(String),
    /// A picture: its URL.
    Image(String),
}

/// A message's blocks: paragraphs, tables, quotes, code and pictures. `names` by id
/// name who wrote a quoted message.
pub fn blocks(
    ui: &mut Ui,
    pictures: &mut Pictures,
    blocks: &[Block],
    names: &std::collections::HashMap<String, String>,
) -> Option<Clicked> {
    let p = colors();
    let mut clicked = None;
    for block in blocks {
        match block {
            Block::Text(runs) => rich_text(ui, pictures, runs, p.text),
            Block::Quote(runs, of) => {
                if quote(ui, pictures, runs, of.is_some(), names) {
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
fn quote(
    ui: &mut Ui,
    pictures: &mut Pictures,
    runs: &[Inline],
    clickable: bool,
    names: &std::collections::HashMap<String, String>,
) -> bool {
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
                // By the author's id when known: the name in the markup can be a placeholder.
                let who: String = match name.iter().find_map(|r| names.get(r.span.author.as_deref()?)) {
                    Some(known) => known.clone(),
                    None => name.iter().map(|r| r.text.as_str()).collect(),
                };
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
    Frame::new().stroke(Stroke::new(1.0, p.border)).corner_radius(CornerRadius::same(metrics::RADIUS)).show(ui, |ui| {
        // Too wide a table is cut at the bubble's edge (no sideways scroll area: inside
        // the scrolling list it flips its scroll bar every frame, and the list flickers).
        // Sideways only: a bubble's height isn't known yet, its max rect has none.
        let mut clip = ui.clip_rect();
        clip.set_right(clip.right().min(ui.max_rect().right()));
        let room = ui.available_width();
        let inner = egui::UiBuilder::new().max_rect(ui.available_rect_before_wrap()).layout(egui::Layout::top_down(egui::Align::Min));
        let mut cut = ui.new_child(inner);
        {
            let ui = &mut cut;
            ui.set_clip_rect(clip);
            let id = ui.next_auto_id();
            let header_fill = ui.painter().add(egui::Shape::Noop);
            // Where the lines go: a row's cells are as tall as they need, the lines as the row.
            let mut bottoms = Vec::with_capacity(rows.len());
            let mut lefts = vec![f32::INFINITY; columns];
            IN_TABLE.with(|t| t.set(true));
            let grid = egui::Grid::new(id).spacing(vec2(0.0, 0.0)).min_col_width(40.0).max_col_width(320.0).show(ui, |ui| {
                for row in rows {
                    let mut bottom = f32::NEG_INFINITY;
                    for (c, left) in lefts.iter_mut().enumerate() {
                        let cell = Frame::new()
                            .inner_margin(Margin::symmetric(10, 6))
                            .show(ui, |ui| match row.get(c) {
                                Some(cell) if !cell.is_empty() => rich_text(ui, pictures, cell, p.text),
                                _ => {
                                    ui.label("");
                                }
                            })
                            .response
                            .rect;
                        *left = left.min(cell.left());
                        bottom = bottom.max(cell.bottom());
                    }
                    bottoms.push(bottom);
                    ui.end_row();
                }
            });
            IN_TABLE.with(|t| t.set(false));
            let area = grid.response.rect;
            if let Some(first) = bottoms.first().filter(|_| header) {
                let radius = metrics::RADIUS;
                let top = Rect::from_min_max(area.min, pos2(area.right(), *first));
                ui.painter()
                    .set(header_fill, egui::Shape::rect_filled(top, CornerRadius { nw: radius, ne: radius, sw: 0, se: 0 }, p.hover));
            }
            let line = Stroke::new(1.0, p.border);
            for bottom in bottoms.iter().take(rows.len() - 1) {
                ui.painter().hline(area.x_range(), *bottom, line);
            }
            for left in lefts.iter().skip(1).filter(|l| l.is_finite()) {
                ui.painter().vline(*left, area.y_range(), line);
            }
        }
        // It takes no more room than there is, whatever was cut.
        let size = cut.min_rect().size();
        ui.allocate_space(vec2(size.x.min(room), size.y));
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

/// Who reacted, for reading: "You, Maya Lindqvist and 2 others".
fn reacted(r: &Reaction, names: &std::collections::HashMap<String, String>) -> String {
    let mut who: Vec<String> = r.who.iter().map(|id| names.get(id).cloned().unwrap_or_else(|| "Someone".into())).collect();
    // The user first, then the rest by name.
    who.sort_by_key(|n| (n != "You", n.clone()));
    let unknown = r.count.saturating_sub(who.len());
    let shown = 4;
    let more = who.len().saturating_sub(shown) + unknown;
    who.truncate(shown);
    match (who.as_slice(), more) {
        ([], n) => format!("{n} {}", if n == 1 { "person" } else { "people" }),
        (names, 0) => match names {
            [one] => one.clone(),
            [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
            [] => unreachable!(),
        },
        (names, n) => format!("{} and {n} {}", names.join(", "), if n == 1 { "other" } else { "others" }),
    }
}

/// Reactions under a message, as small pills; the user's own ones in the accent.
/// Clicking one returns its key (to add it, or take it back).
/// Who reacted shows on hover (`names` by id; someone not among them as "Someone").
pub fn reactions(
    ui: &mut Ui,
    pictures: &mut Pictures,
    reactions: &[Reaction],
    right: bool,
    names: &std::collections::HashMap<String, String>,
) -> Option<String> {
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
            let who = reacted(r, names);
            let tip = format!("{who} reacted with {}", r.emoji());
            if response.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(tip).clicked() {
                clicked = Some(r.key.clone());
            }
        }
    });
    clicked
}

#[cfg(test)]
mod tests {

    #[test]
    fn says_who_reacted() {
        let names: std::collections::HashMap<String, String> =
            [("me", "You"), ("m", "Maya Lindqvist"), ("t", "Tomás Rivera"), ("l", "Lea Novak"), ("p", "Priya Nair"), ("j", "Jonas Weber")]
                .iter()
                .map(|(i, n)| (i.to_string(), n.to_string()))
                .collect();
        let r = |who: &[&str], count| Reaction { key: "like".into(), count, mine: false, who: who.iter().map(|w| w.to_string()).collect() };
        assert_eq!(reacted(&r(&["m"], 1), &names), "Maya Lindqvist");
        assert_eq!(reacted(&r(&["t", "me"], 2), &names), "You and Tomás Rivera");
        assert_eq!(reacted(&r(&["m", "t", "l", "p", "j", "me"], 6), &names), "You, Jonas Weber, Lea Novak, Maya Lindqvist and 2 others");
        assert_eq!(reacted(&r(&["x"], 1), &names), "Someone");
        assert_eq!(reacted(&r(&[], 3), &names), "3 people");
    }

    use super::{Reaction, initials, reacted, split_matches};

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
