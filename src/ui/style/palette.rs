//! Colours: "Calm". A deep ink dark and a warm paper light (the sidebar a shade
//! darker than the page), both with one violet accent.

use eframe::egui::Color32;
use std::sync::atomic::{AtomicBool, Ordering};

pub struct Palette {
    /// The conversation's page.
    pub bg: Color32,
    pub sidebar: Color32,
    /// Posts, fields and other raised areas.
    pub surface: Color32,
    pub hover: Color32,
    /// The open conversation in the sidebar, tabs, own reactions.
    pub selected: Color32,
    /// Hairlines between areas.
    pub line: Color32,
    /// Outlines of fields and the reply box.
    pub border: Color32,
    pub text: Color32,
    pub dim: Color32,
    pub faint: Color32,
    /// Accent for text, links and icons.
    pub accent: Color32,
    /// Accent as a fill (buttons, counts), with `on_accent` on top.
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
    bg: rgb(0x13, 0x13, 0x16),
    sidebar: rgb(0x18, 0x18, 0x1c),
    surface: rgb(0x1f, 0x1f, 0x24),
    hover: rgb(0x23, 0x23, 0x29),
    selected: rgb(0x2a, 0x28, 0x36),
    line: rgb(0x26, 0x26, 0x2c),
    border: rgb(0x34, 0x34, 0x3c),
    text: rgb(0xec, 0xec, 0xf1),
    dim: rgb(0xa1, 0xa1, 0xad),
    faint: rgb(0x70, 0x70, 0x7c),
    accent: rgb(0xa9, 0x9b, 0xff),
    accent_fill: rgb(0x8b, 0x7c, 0xf6),
    on_accent: rgb(0xff, 0xff, 0xff),
    code: rgb(0x25, 0x25, 0x2b),
    danger: rgb(0xf2, 0x70, 0x6c),
};

pub static LIGHT: Palette = Palette {
    bg: rgb(0xfc, 0xfb, 0xf9),
    sidebar: rgb(0xeb, 0xe7, 0xe2),
    surface: rgb(0xff, 0xff, 0xff),
    hover: rgb(0xe2, 0xdd, 0xd6),
    selected: rgb(0xe2, 0xdb, 0xf6),
    line: rgb(0xe4, 0xdf, 0xd8),
    border: rgb(0xd6, 0xd0, 0xc8),
    text: rgb(0x1d, 0x1b, 0x19),
    dim: rgb(0x5f, 0x5a, 0x54),
    faint: rgb(0x93, 0x8c, 0x85),
    accent: rgb(0x6a, 0x55, 0xe8),
    accent_fill: rgb(0x74, 0x61, 0xf0),
    on_accent: rgb(0xff, 0xff, 0xff),
    code: rgb(0xf1, 0xed, 0xe8),
    danger: rgb(0xc8, 0x3c, 0x34),
};

static IS_DARK: AtomicBool = AtomicBool::new(true);

pub fn colors() -> &'static Palette {
    if IS_DARK.load(Ordering::Relaxed) { &DARK } else { &LIGHT }
}

pub(super) fn set_dark(dark: bool) {
    IS_DARK.store(dark, Ordering::Relaxed);
}

/// A person's avatar colour, the same for them every time.
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
