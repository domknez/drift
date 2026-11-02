//! The keyboard shortcuts, in a sheet over the window (⌘/, or from the menu), each
//! written as this platform writes it (⌘ on a Mac, Ctrl elsewhere).

use crate::ui::style::{colors, typography as ty};
use eframe::egui::{self, Context, CornerRadius, Frame, Key, KeyboardShortcut, Margin, Modifiers, RichText, vec2};

/// Show the sheet while `open`; Esc, a click outside or Close shut it.
pub fn show(ctx: &Context, open: &mut bool) {
    if !*open {
        return;
    }
    let p = colors();
    let frame = Frame::new().fill(p.panel).corner_radius(CornerRadius::same(16)).inner_margin(Margin::same(22));
    let response = egui::Modal::new(egui::Id::unique("shortcuts")).frame(frame).show(ctx, |ui| {
        ui.set_width(420.0);
        ui.label(RichText::new("Keyboard shortcuts").font(ty::title()).color(p.text));
        ui.add_space(14.0);
        egui::Grid::new("shortcut-list").num_columns(2).spacing(vec2(24.0, 7.0)).show(ui, |ui| {
            for (keys, what) in list(ctx) {
                ui.label(RichText::new(keys).font(ty::small_strong()).color(p.text));
                ui.label(RichText::new(what).font(ty::small()).color(p.dim));
                ui.end_row();
            }
        });
        ui.add_space(16.0);
        ui.vertical_centered(|ui| ui.button("Close").clicked()).inner
    });
    if response.inner || response.should_close() {
        *open = false;
    }
}

/// The shortcut that opens the sheet.
pub fn opener() -> KeyboardShortcut {
    KeyboardShortcut::new(Modifiers::COMMAND, Key::Slash)
}

/// A shortcut as this platform writes it, with arrows the fonts have (egui's own
/// arrow symbols aren't in them).
pub fn keys(ctx: &Context, shortcut: &KeyboardShortcut) -> String {
    ctx.format_shortcut(shortcut).replace('⏶', "↑").replace('⏷', "↓").replace('⏴', "←").replace('⏵', "→")
}

fn list(ctx: &Context) -> Vec<(String, &'static str)> {
    let key = |modifiers, key| keys(ctx, &KeyboardShortcut::new(modifiers, key));
    let cmd = Modifiers::COMMAND;
    let alt_shift = Modifiers::ALT | Modifiers::SHIFT;
    vec![
        (key(cmd, Key::K), "Search"),
        (key(cmd, Key::T), "Go to a conversation"),
        (format!("{} – {}", key(cmd, Key::Num1), key(cmd, Key::Num4)), "Activity, Chats, Teams, Calendar"),
        (format!("{}  {}", key(Modifiers::ALT, Key::ArrowUp), key(Modifiers::ALT, Key::ArrowDown)), "Previous or next conversation"),
        (format!("{}  {}", key(alt_shift, Key::ArrowUp), key(alt_shift, Key::ArrowDown)), "Previous or next unread one"),
        (key(cmd | Modifiers::SHIFT, Key::U), "Mark the conversation unread"),
        (key(cmd, Key::F), "Find in the conversation"),
        (key(Modifiers::NONE, Key::Enter), "Send"),
        (key(Modifiers::SHIFT, Key::Enter), "New line"),
        (key(Modifiers::NONE, Key::ArrowUp), "Edit your last message (in an empty box)"),
        (key(Modifiers::NONE, Key::Escape), "Cancel a quote or an edit"),
        (key(Modifiers::NONE, Key::Tab), "Move between controls and messages"),
        ("R  E  Delete".into(), "On a message: quote, edit, delete"),
        (key(Modifiers::SHIFT, Key::F10), "On a message: its menu"),
        (format!("{}  {}  {}", key(cmd, Key::Plus), key(cmd, Key::Minus), key(cmd, Key::Num0)), "Bigger, smaller, usual size"),
        (key(cmd, Key::R), "Refresh"),
        (key(cmd, Key::Slash), "These shortcuts"),
    ]
}
