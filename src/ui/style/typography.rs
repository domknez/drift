//! Text roles: Inter throughout, JetBrains Mono for code.

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

/// The list's heading ("Teams", "Chats").
pub fn list_title() -> FontId {
    FontId::new(18.0, bold())
}

/// The conversation's name in its header.
pub fn title() -> FontId {
    FontId::new(16.0, bold())
}

/// Sign-in and empty-page headings.
pub fn heading() -> FontId {
    FontId::new(22.0, bold())
}

/// List rows.
pub fn item() -> FontId {
    FontId::new(13.5, FontFamily::Proportional)
}

pub fn item_strong() -> FontId {
    FontId::new(13.5, semibold())
}

/// Team names above their channels.
pub fn team() -> FontId {
    FontId::new(13.0, semibold())
}

/// Section labels ("Pinned", "Direct messages").
pub fn section() -> FontId {
    FontId::new(12.0, semibold())
}

/// Message text.
pub fn body() -> FontId {
    FontId::new(14.0, FontFamily::Proportional)
}

pub fn body_strong() -> FontId {
    FontId::new(14.0, semibold())
}

/// Authors above messages.
pub fn author() -> FontId {
    FontId::new(14.0, bold())
}

/// Small text: subtitles, times, previews.
pub fn small() -> FontId {
    FontId::new(12.5, FontFamily::Proportional)
}

pub fn small_strong() -> FontId {
    FontId::new(12.5, semibold())
}

/// Times beside authors, day dividers.
pub fn meta() -> FontId {
    FontId::new(12.0, FontFamily::Proportional)
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
    FontId::new((size * 0.36).round(), semibold())
}
