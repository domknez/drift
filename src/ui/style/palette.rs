//! Colours: panels floating on a canvas, with one accent. Themes give the greys
//! (three dark, two light), the accent is picked on its own.

use crate::settings::{Accent, Theme};
use eframe::egui::Color32;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicUsize, Ordering};

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

/// A theme's greys: everything but the accent.
struct Base {
    dark: bool,
    canvas: Color32,
    panel: Color32,
    surface: Color32,
    hover: Color32,
    line: Color32,
    border: Color32,
    text: Color32,
    dim: Color32,
    faint: Color32,
    code: Color32,
    danger: Color32,
}

/// The deep ink dark drift started with.
const MIDNIGHT: Base = Base {
    dark: true,
    canvas: rgb(0x0e, 0x0e, 0x11),
    panel: rgb(0x18, 0x18, 0x1c),
    surface: rgb(0x22, 0x22, 0x28),
    hover: rgb(0x26, 0x26, 0x2d),
    line: rgb(0x26, 0x26, 0x2c),
    border: rgb(0x34, 0x34, 0x3c),
    text: rgb(0xec, 0xec, 0xf1),
    dim: rgb(0xa1, 0xa1, 0xad),
    faint: rgb(0x88, 0x88, 0x92),
    code: rgb(0x2b, 0x2b, 0x32),
    danger: rgb(0xf2, 0x70, 0x6c),
};

/// A softer dark grey, with less contrast.
const DIM: Base = Base {
    dark: true,
    canvas: rgb(0x1b, 0x1c, 0x20),
    panel: rgb(0x24, 0x25, 0x2a),
    surface: rgb(0x2e, 0x2f, 0x35),
    hover: rgb(0x32, 0x33, 0x3a),
    line: rgb(0x31, 0x32, 0x38),
    border: rgb(0x40, 0x41, 0x49),
    text: rgb(0xe4, 0xe4, 0xe9),
    dim: rgb(0xa4, 0xa5, 0xaf),
    faint: rgb(0x95, 0x96, 0x9f),
    code: rgb(0x35, 0x36, 0x3d),
    danger: rgb(0xf0, 0x74, 0x70),
};

/// A bluish slate between dark and light.
const SLATE: Base = Base {
    dark: true,
    canvas: rgb(0x25, 0x2a, 0x33),
    panel: rgb(0x2f, 0x35, 0x3f),
    surface: rgb(0x3a, 0x41, 0x4d),
    hover: rgb(0x3e, 0x45, 0x52),
    line: rgb(0x3d, 0x44, 0x50),
    border: rgb(0x4e, 0x56, 0x65),
    text: rgb(0xec, 0xef, 0xf4),
    dim: rgb(0xb0, 0xb7, 0xc3),
    faint: rgb(0xa6, 0xac, 0xb7),
    code: rgb(0x41, 0x48, 0x55),
    danger: rgb(0xf4, 0x7c, 0x78),
};

/// Warm paper, its canvas a shade below the white panels.
const PAPER: Base = Base {
    dark: false,
    canvas: rgb(0xeb, 0xe7, 0xe2),
    panel: rgb(0xff, 0xff, 0xff),
    surface: rgb(0xf5, 0xf2, 0xee),
    hover: rgb(0xef, 0xeb, 0xe6),
    line: rgb(0xec, 0xe8, 0xe3),
    border: rgb(0xdd, 0xd7, 0xd0),
    text: rgb(0x1d, 0x1b, 0x19),
    dim: rgb(0x61, 0x5c, 0x56),
    faint: rgb(0x74, 0x6e, 0x69),
    code: rgb(0xef, 0xeb, 0xe6),
    danger: rgb(0xc8, 0x3c, 0x34),
};

/// A neutral, cool light grey.
const COOL: Base = Base {
    dark: false,
    canvas: rgb(0xe8, 0xea, 0xee),
    panel: rgb(0xff, 0xff, 0xff),
    surface: rgb(0xf2, 0xf3, 0xf6),
    hover: rgb(0xeb, 0xed, 0xf1),
    line: rgb(0xe5, 0xe7, 0xeb),
    border: rgb(0xd3, 0xd7, 0xde),
    text: rgb(0x1a, 0x1c, 0x21),
    dim: rgb(0x58, 0x5d, 0x68),
    faint: rgb(0x6a, 0x6e, 0x77),
    code: rgb(0xed, 0xef, 0xf2),
    danger: rgb(0xc4, 0x37, 0x35),
};

/// An accent: for text on dark and its fill, then for text on light and its fill.
type AccentColors = [Color32; 4];

fn accent_colors(accent: Accent) -> AccentColors {
    match accent {
        Accent::Violet => [rgb(0xa9, 0x9b, 0xff), rgb(0x73, 0x67, 0xcc), rgb(0x6a, 0x55, 0xe8), rgb(0x72, 0x5f, 0xeb)],
        Accent::Indigo => [rgb(0xa4, 0xa8, 0xff), rgb(0x67, 0x6c, 0xc6), rgb(0x4f, 0x52, 0xb2), rgb(0x5b, 0x5f, 0xc7)],
        Accent::Blue => [rgb(0x82, 0xb8, 0xff), rgb(0x34, 0x72, 0xd8), rgb(0x1d, 0x5f, 0xd1), rgb(0x2b, 0x6c, 0xe0)],
        Accent::Teal => [rgb(0x6a, 0xd8, 0xc8), rgb(0x11, 0x83, 0x77), rgb(0x0b, 0x76, 0x6c), rgb(0x0f, 0x83, 0x78)],
        Accent::Green => [rgb(0x86, 0xda, 0x96), rgb(0x27, 0x84, 0x48), rgb(0x1c, 0x78, 0x39), rgb(0x25, 0x84, 0x44)],
        Accent::Orange => [rgb(0xff, 0xb5, 0x70), rgb(0xac, 0x62, 0x19), rgb(0xa8, 0x55, 0x0a), rgb(0xb4, 0x5e, 0x0c)],
        Accent::Pink => [rgb(0xff, 0x94, 0xc8), rgb(0xc2, 0x48, 0x85), rgb(0xb3, 0x2b, 0x68), rgb(0xc9, 0x3a, 0x7b)],
    }
}

/// `a` with `t` of `b` mixed in.
fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let c = |x: u8, y: u8| (f32::from(x) + (f32::from(y) - f32::from(x)) * t).round() as u8;
    rgb(c(a.r(), b.r()), c(a.g(), b.g()), c(a.b(), b.b()))
}

fn palette(base: &Base, accent: Accent) -> Palette {
    let [dark_text, dark_fill, light_text, light_fill] = accent_colors(accent);
    let (accent, fill) = if base.dark { (dark_text, dark_fill) } else { (light_text, light_fill) };
    // The open conversation and the user's bubbles: the panel tinted with the accent.
    let (selected, mine) = if base.dark { (0.14, 0.22) } else { (0.12, 0.16) };
    Palette {
        canvas: base.canvas,
        panel: base.panel,
        surface: base.surface,
        hover: base.hover,
        selected: mix(base.panel, fill, selected),
        mine: mix(base.panel, fill, mine),
        line: base.line,
        border: base.border,
        text: base.text,
        dim: base.dim,
        faint: base.faint,
        accent,
        accent_fill: fill,
        on_accent: rgb(0xff, 0xff, 0xff),
        code: base.code,
        danger: base.danger,
    }
}

fn base(theme: Theme) -> &'static Base {
    match theme {
        Theme::Midnight => &MIDNIGHT,
        Theme::Dim => &DIM,
        Theme::Slate => &SLATE,
        Theme::Paper => &PAPER,
        Theme::Cool => &COOL,
    }
}

/// Every theme with every accent, made once.
fn all() -> &'static Vec<Palette> {
    static ALL: OnceLock<Vec<Palette>> = OnceLock::new();
    ALL.get_or_init(|| Theme::ALL.iter().flat_map(|t| Accent::ALL.iter().map(|a| palette(base(*t), *a))).collect())
}

static ACTIVE: AtomicUsize = AtomicUsize::new(0);

pub fn colors() -> &'static Palette {
    &all()[ACTIVE.load(Ordering::Relaxed)]
}

/// Use `theme` with `accent` from now on; whether it's a dark one.
pub(super) fn set(theme: Theme, accent: Accent) -> bool {
    let t = Theme::ALL.iter().position(|x| *x == theme).unwrap_or(0);
    let a = Accent::ALL.iter().position(|x| *x == accent).unwrap_or(0);
    ACTIVE.store(t * Accent::ALL.len() + a, Ordering::Relaxed);
    base(theme).dark
}

/// A theme's canvas, panel and text, and an accent's fill, for the menu's swatches.
pub fn swatch(theme: Theme) -> (Color32, Color32, Color32) {
    let b = base(theme);
    (b.canvas, b.panel, b.text)
}

pub fn accent_swatch(accent: Accent, dark: bool) -> Color32 {
    accent_colors(accent)[if dark { 1 } else { 3 }]
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
