//! Text roles: Plus Jakarta Sans throughout, JetBrains Mono for code.

use eframe::egui::{FontFamily, FontId};

pub fn medium() -> FontFamily {
    FontFamily::Name("medium".into())
}

pub fn semibold() -> FontFamily {
    FontFamily::Name("semibold".into())
}

pub fn bold() -> FontFamily {
    FontFamily::Name("bold".into())
}

pub fn extrabold() -> FontFamily {
    FontFamily::Name("extrabold".into())
}

/// "drift." in the top bar.
pub fn wordmark() -> FontId {
    FontId::new(17.0, extrabold())
}

/// Tabs in the top bar.
pub fn tab() -> FontId {
    FontId::new(13.0, semibold())
}

/// The conversation's name in its header.
pub fn title() -> FontId {
    FontId::new(16.0, bold())
}

/// Sign-in and empty-page headings.
pub fn heading() -> FontId {
    FontId::new(22.0, extrabold())
}

/// Names in the list.
pub fn item() -> FontId {
    FontId::new(13.5, semibold())
}

/// Teams above their channels in the list.
pub fn team() -> FontId {
    FontId::new(13.0, bold())
}

/// Section labels ("PINNED", "RECENT").
pub fn section() -> FontId {
    FontId::new(11.0, bold())
}

/// Message text.
pub fn body() -> FontId {
    FontId::new(14.0, FontFamily::Proportional)
}

pub fn body_strong() -> FontId {
    FontId::new(14.0, bold())
}

/// Authors inside bubbles.
pub fn author() -> FontId {
    FontId::new(13.0, bold())
}

/// Previews, subtitles, chips.
pub fn small() -> FontId {
    FontId::new(12.5, FontFamily::Proportional)
}

pub fn small_strong() -> FontId {
    FontId::new(12.5, bold())
}

/// Times and other small print.
pub fn meta() -> FontId {
    FontId::new(11.5, medium())
}

/// Day pills.
pub fn pill() -> FontId {
    FontId::new(11.5, bold())
}

/// Counts on badges.
pub fn badge() -> FontId {
    FontId::new(10.5, bold())
}

pub fn code() -> FontId {
    FontId::new(12.5, FontFamily::Monospace)
}

/// Card details (branches, ids).
pub fn mono_small() -> FontId {
    FontId::new(11.5, FontFamily::Monospace)
}

/// Initials in avatars of `size`.
pub fn initials(size: f32) -> FontId {
    FontId::new((size * 0.36).round(), bold())
}
