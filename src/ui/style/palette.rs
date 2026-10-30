//! Colours: "Tide". A cool slate dark and a clean white light, with one teal accent.

use eframe::egui::Color32;
use std::sync::atomic::{AtomicBool, Ordering};

pub struct Palette {
    /// The conversation's page.
    pub bg: Color32,
    /// The icon rail on the far left.
    pub rail: Color32,
    /// The list beside the rail.
    pub side: Color32,
    /// Cards, fields and other raised areas.
    pub surface: Color32,
    pub hover: Color32,
    /// The open conversation, the active rail button, the meeting card.
    pub selected: Color32,
    /// Hairlines between areas.
    pub line: Color32,
    /// Outlines of fields, buttons and the composer.
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
    bg: rgb(0x16, 0x18, 0x1b),
    rail: rgb(0x0f, 0x11, 0x13),
    side: rgb(0x13, 0x15, 0x18),
    surface: rgb(0x1c, 0x1f, 0x23),
    hover: rgb(0x20, 0x24, 0x2a),
    selected: rgb(0x1f, 0x2b, 0x2d),
    line: rgb(0x26, 0x2a, 0x2f),
    border: rgb(0x32, 0x38, 0x40),
    text: rgb(0xe7, 0xe9, 0xec),
    dim: rgb(0x9a, 0xa1, 0xaa),
    faint: rgb(0x6d, 0x74, 0x7d),
    accent: rgb(0x3d, 0xd6, 0xc0),
    accent_fill: rgb(0x2b, 0xc4, 0xae),
    on_accent: rgb(0x06, 0x24, 0x20),
    code: rgb(0x22, 0x27, 0x2d),
    danger: rgb(0xf2, 0x70, 0x6c),
};

pub static LIGHT: Palette = Palette {
    bg: rgb(0xff, 0xff, 0xff),
    rail: rgb(0xee, 0xf0, 0xf2),
    side: rgb(0xf6, 0xf7, 0xf8),
    surface: rgb(0xff, 0xff, 0xff),
    hover: rgb(0xec, 0xef, 0xf1),
    selected: rgb(0xdf, 0xf4, 0xf0),
    line: rgb(0xe5, 0xe8, 0xeb),
    border: rgb(0xd4, 0xd9, 0xde),
    text: rgb(0x15, 0x18, 0x1b),
    dim: rgb(0x59, 0x61, 0x6b),
    faint: rgb(0x8a, 0x92, 0x9b),
    accent: rgb(0x0f, 0x8f, 0x7e),
    accent_fill: rgb(0x14, 0xa8, 0x92),
    on_accent: rgb(0xff, 0xff, 0xff),
    code: rgb(0xf0, 0xf2, 0xf4),
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
