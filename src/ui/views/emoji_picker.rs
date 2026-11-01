//! Teams' whole set of emoji, and the organisation's own, with a search field on
//! top: for the composer, and for reacting to a message with any emoji.

use crate::model::emoji::{self, Emoji};
use crate::model::{CustomEmoji, Picture};
use crate::ui::pictures::Pictures;
use crate::ui::style::{colors, typography as ty};
use eframe::egui::{self, CornerRadius, Rect, RichText, Sense, Ui, pos2, vec2};

const CELL: f32 = 36.0;
const COLUMNS: usize = 9;
const HEIGHT: f32 = 320.0;
/// Search results shown at most (the best come first).
const MOST_FOUND: usize = 180;
/// Emoji kept as recently used.
pub const RECENT: usize = 18;

/// The picker's state while it's open: what's typed in its search field.
#[derive(Clone, Default)]
pub struct Picker {
    pub query: String,
}

/// An emoji picked: its key (for reactions), and the text it puts in a message
/// (the character, or `:name:` of an organisation's own).
pub struct Pick {
    pub key: String,
    pub text: String,
}

/// One cell of the grid.
#[derive(Clone, Copy)]
struct Item<'a> {
    key: &'a str,
    /// The character, for Teams' own (drawn until the picture is in).
    ch: Option<&'a str>,
    name: &'a str,
}

impl<'a> Item<'a> {
    fn of(e: &'static Emoji) -> Item<'a> {
        Item { key: e.key, ch: Some(e.ch), name: e.name }
    }

    fn custom(e: &'a CustomEmoji) -> Item<'a> {
        Item { key: &e.key, ch: None, name: &e.name }
    }

    fn pick(self) -> Pick {
        Pick { key: self.key.to_string(), text: self.ch.map_or_else(|| format!(":{}:", self.name), str::to_string) }
    }
}

/// The picker; returns the emoji clicked (or the first one found when Enter is
/// pressed in the search field). `recent` are keys, the latest first.
pub fn show(
    ui: &mut Ui,
    pictures: &mut Pictures,
    picker: &mut Picker,
    salt: &str,
    custom: &[CustomEmoji],
    recent: &[String],
) -> Option<Pick> {
    let p = colors();
    let width = CELL * COLUMNS as f32;
    ui.set_width(width);
    let field = egui::TextEdit::singleline(&mut picker.query)
        .id(egui::Id::unique(("emoji-search", salt)))
        .hint_text("Search emoji")
        .font(ty::small())
        .desired_width(width)
        .margin(egui::Margin::symmetric(8, 6));
    let response = ui.add(field);
    if !response.has_focus() && !response.lost_focus() {
        response.request_focus();
    }
    let enter = response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
    ui.add_space(6.0);

    let mut picked = None;
    egui::ScrollArea::vertical().id_salt(("emoji-grid", salt)).max_height(HEIGHT).auto_shrink([false, true]).show(ui, |ui| {
        ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
        let query = picker.query.trim().to_lowercase();
        if query.is_empty() {
            let recent: Vec<Item> = recent
                .iter()
                .filter_map(|key| custom.iter().find(|e| e.key == *key).map(Item::custom).or_else(|| emoji::by_key(key).map(Item::of)))
                .collect();
            let mut sections: Vec<(&str, Vec<Item>)> = Vec::new();
            sections.push(("Recent", recent));
            sections.push(("Your organization", custom.iter().map(Item::custom).collect()));
            let catalog = emoji::catalog();
            for (i, title) in catalog.categories.iter().enumerate() {
                sections.push((title, catalog.emoji.iter().filter(|e| e.category == i).map(Item::of).collect()));
            }
            let mut first = true;
            for (title, items) in sections.into_iter().filter(|(_, items)| !items.is_empty()) {
                ui.add_space(if first { 2.0 } else { 8.0 });
                first = false;
                ui.label(RichText::new(title.to_uppercase()).font(ty::section()).color(p.faint));
                ui.add_space(4.0);
                if let Some(item) = grid(ui, pictures, &items) {
                    picked = Some(item.pick());
                }
            }
        } else {
            // The organisation's first: they're few, and what's searched for is often one.
            let mut found: Vec<Item> = custom
                .iter()
                .filter(|e| e.name.to_lowercase().contains(&query) || e.keywords.iter().any(|k| k.contains(&query)))
                .map(Item::custom)
                .collect();
            found.extend(emoji::search(&query).into_iter().map(Item::of));
            found.truncate(MOST_FOUND);
            if enter {
                picked = found.first().map(|i| i.pick());
            }
            if found.is_empty() {
                ui.add_space(16.0);
                ui.vertical_centered(|ui| ui.label(RichText::new("No emoji found").font(ty::small()).color(p.faint)));
                ui.add_space(16.0);
            } else if let Some(item) = grid(ui, pictures, &found) {
                picked = Some(item.pick());
            }
        }
    });
    picked
}

/// Emoji in rows; only the ones on screen are fetched.
fn grid<'a>(ui: &mut Ui, pictures: &mut Pictures, items: &[Item<'a>]) -> Option<Item<'a>> {
    let p = colors();
    let mut picked = None;
    for row in items.chunks(COLUMNS) {
        let (line, _) = ui.allocate_exact_size(vec2(CELL * COLUMNS as f32, CELL), Sense::hover());
        if !ui.is_rect_visible(line) {
            continue;
        }
        for (i, item) in row.iter().enumerate() {
            let cell = Rect::from_min_size(line.min + vec2(CELL * i as f32, 0.0), vec2(CELL, CELL));
            let response = ui
                .interact(cell, ui.scope_id().with(("emoji", item.key)), Sense::click())
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .on_hover_text(match item.ch {
                    Some(_) => item.name.to_string(),
                    None => format!(":{}:", item.name),
                });
            if response.hovered() {
                ui.painter().rect_filled(cell.shrink(1.0), CornerRadius::same(8), p.hover);
            }
            match pictures.get(&Picture::Emoji(item.key.to_string())) {
                Some(texture) => {
                    let image = Rect::from_center_size(cell.center(), vec2(26.0, 26.0));
                    ui.painter().image(texture, image, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), egui::Color32::WHITE);
                }
                None => {
                    let shown = item.ch.unwrap_or("·");
                    ui.painter().text(cell.center(), egui::Align2::CENTER_CENTER, shown, egui::FontId::proportional(20.0), p.text);
                }
            }
            if response.clicked() {
                picked = Some(*item);
            }
        }
    }
    picked
}
