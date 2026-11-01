//! Teams' whole set of emoji with a search field on top: for the composer, and for
//! reacting to a message with any emoji (not only the quick ones).

use crate::model::Picture;
use crate::model::emoji::{self, Emoji};
use crate::ui::pictures::Pictures;
use crate::ui::style::{colors, typography as ty};
use eframe::egui::{self, CornerRadius, Rect, RichText, Sense, Ui, vec2};

const CELL: f32 = 36.0;
const COLUMNS: usize = 9;
const HEIGHT: f32 = 320.0;
/// Search results shown at most (the best come first).
const MOST_FOUND: usize = 180;

/// The picker's state while it's open: what's typed in its search field.
#[derive(Clone, Default)]
pub struct Picker {
    pub query: String,
}

/// The picker; returns the emoji clicked (or the first one found when Enter is pressed
/// in the search field).
pub fn show(ui: &mut Ui, pictures: &mut Pictures, picker: &mut Picker, salt: &str) -> Option<&'static Emoji> {
    let p = colors();
    let width = CELL * COLUMNS as f32;
    ui.set_width(width);
    let search_id = egui::Id::unique(("emoji-search", salt));
    let field = egui::TextEdit::singleline(&mut picker.query)
        .id(search_id)
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
        if picker.query.trim().is_empty() {
            let catalog = emoji::catalog();
            for (i, title) in catalog.categories.iter().enumerate() {
                ui.add_space(if i == 0 { 2.0 } else { 8.0 });
                ui.label(RichText::new(title.to_uppercase()).font(ty::section()).color(p.faint));
                ui.add_space(4.0);
                let all: Vec<&Emoji> = catalog.emoji.iter().filter(|e| e.category == i).collect();
                if let Some(e) = grid(ui, pictures, &all) {
                    picked = Some(e);
                }
            }
        } else {
            let found = emoji::search(&picker.query);
            if enter {
                picked = found.first().copied();
            }
            if found.is_empty() {
                ui.add_space(16.0);
                ui.vertical_centered(|ui| ui.label(RichText::new("No emoji found").font(ty::small()).color(p.faint)));
                ui.add_space(16.0);
            } else if let Some(e) = grid(ui, pictures, &found[..found.len().min(MOST_FOUND)]) {
                picked = Some(e);
            }
        }
    });
    picked
}

/// Emoji in rows; only the ones on screen are fetched.
fn grid(ui: &mut Ui, pictures: &mut Pictures, all: &[&'static Emoji]) -> Option<&'static Emoji> {
    let p = colors();
    let mut picked = None;
    for row in all.chunks(COLUMNS) {
        let (line, _) = ui.allocate_exact_size(vec2(CELL * COLUMNS as f32, CELL), Sense::hover());
        if !ui.is_rect_visible(line) {
            continue;
        }
        for (i, e) in row.iter().enumerate() {
            let cell = Rect::from_min_size(line.min + vec2(CELL * i as f32, 0.0), vec2(CELL, CELL));
            let response = ui
                .interact(cell, ui.scope_id().with(("emoji", e.key)), Sense::click())
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .on_hover_text(e.name);
            if response.hovered() {
                ui.painter().rect_filled(cell.shrink(1.0), CornerRadius::same(8), p.hover);
            }
            match pictures.get(&Picture::Emoji(e.key.to_string())) {
                Some(texture) => {
                    let image = Rect::from_center_size(cell.center(), vec2(26.0, 26.0));
                    ui.painter().image(
                        texture,
                        image,
                        Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                        egui::Color32::WHITE,
                    );
                }
                None => {
                    ui.painter().text(cell.center(), egui::Align2::CENTER_CENTER, e.ch, egui::FontId::proportional(20.0), p.text);
                }
            }
            if response.clicked() {
                picked = Some(*e);
            }
        }
    }
    picked
}
