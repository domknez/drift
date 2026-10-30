//! Colours: panels floating on a canvas, with one violet accent. A deep ink dark,
//! and a warm paper light whose canvas sits a shade below the white panels.

use eframe::egui::Color32;
use std::sync::atomic::{AtomicBool, Ordering};

pub struct Palette {
    /// The window behind the panels.
    pub canvas: Color32,
    /// The panels: the list, the conversation, the thread, the top bar's pills.
    pub panel: Color32,
    /// Raised inside a panel: message bubbles, chips, fields, icon buttons.
    pub surface: Color32,
    pub hover: Color32,
    /// The open conversation, own reactions.
    pub selected: Color32,
    /// The user's own message bubbles.
    pub mine: Color32,
    /// Hairlines.
    pub line: Color32,
    /// Outlines of fields.
    pub border: Color32,
    pub text: Color32,
    pub dim: Color32,
    pub faint: Color32,
    /// Accent for text, links and icons.
    pub accent: Color32,
    /// Accent as a fill (buttons, the active tab, counts), with `on_accent` on top.
    pub accent_fill: Color32,
    pub on_accent: Color32,
    /// Behind inline code.
    pub code: Color32,
    pub danger: Color32,
}

const fn rgb(r: u8, g: u8, b: u8) -> Color32 {
    Color32::from_rgb(r, g, b)
}

pub static DARK: Palette = Palette {
    canvas: rgb(0x0e, 0x0e, 0x11),
    panel: rgb(0x18, 0x18, 0x1c),
    surface: rgb(0x22, 0x22, 0x28),
    hover: rgb(0x26, 0x26, 0x2d),
    selected: rgb(0x2a, 0x28, 0x36),
    mine: rgb(0x2e, 0x2a, 0x45),
    line: rgb(0x26, 0x26, 0x2c),
    border: rgb(0x34, 0x34, 0x3c),
    text: rgb(0xec, 0xec, 0xf1),
    dim: rgb(0xa1, 0xa1, 0xad),
    faint: rgb(0x70, 0x70, 0x7c),
    accent: rgb(0xa9, 0x9b, 0xff),
    accent_fill: rgb(0x8b, 0x7c, 0xf6),
    on_accent: rgb(0xff, 0xff, 0xff),
    code: rgb(0x2b, 0x2b, 0x32),
    danger: rgb(0xf2, 0x70, 0x6c),
};

pub static LIGHT: Palette = Palette {
    canvas: rgb(0xeb, 0xe7, 0xe2),
    panel: rgb(0xff, 0xff, 0xff),
    surface: rgb(0xf5, 0xf2, 0xee),
    hover: rgb(0xef, 0xeb, 0xe6),
    selected: rgb(0xec, 0xe8, 0xfd),
    mine: rgb(0xe9, 0xe4, 0xfd),
    line: rgb(0xec, 0xe8, 0xe3),
    border: rgb(0xdd, 0xd7, 0xd0),
    text: rgb(0x1d, 0x1b, 0x19),
    dim: rgb(0x61, 0x5c, 0x56),
    faint: rgb(0x94, 0x8d, 0x86),
    accent: rgb(0x6a, 0x55, 0xe8),
    accent_fill: rgb(0x74, 0x61, 0xf0),
    on_accent: rgb(0xff, 0xff, 0xff),
    code: rgb(0xef, 0xeb, 0xe6),
    danger: rgb(0xc8, 0x3c, 0x34),
};

static IS_DARK: AtomicBool = AtomicBool::new(true);

pub fn colors() -> &'static Palette {
    if IS_DARK.load(Ordering::Relaxed) { &DARK } else { &LIGHT }
}

pub(super) fn set_dark(dark: bool) {
    IS_DARK.store(dark, Ordering::Relaxed);
}

/// A person's or team's colour behind their initials, the same for them every time.
pub fn avatar(id: &str) -> Color32 {
    const COLORS: [Color32; 8] = [
        rgb(0xe0, 0x61, 0x7a),
        rgb(0xd9, 0x8a, 0x2b),
        rgb(0x2f, 0xa3, 0x7a),
        rgb(0x7a, 0x5c, 0xd6),
        rgb(0x2b, 0x8f, 0xd9),
        rgb(0xc0, 0x54, 0x8f),
        rgb(0x4f, 0x9a, 0x94),
        rgb(0x5b, 0x7c, 0xfa),
    ];
    let hash = id.bytes().fold(0u32, |h, b| h.wrapping_mul(31).wrapping_add(u32::from(b)));
    COLORS[hash as usize % COLORS.len()]
}
