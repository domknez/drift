//! Text roles: Geist for everything, Geist Mono for times and counts.

use eframe::egui::{FontFamily, FontId};

pub fn medium() -> FontFamily {
    FontFamily::Name("medium".into())
}

pub fn semibold() -> FontFamily {
    FontFamily::Name("semibold".into())
}

/// Conversation titles in the header.
pub fn title() -> FontId {
    FontId::new(15.5, semibold())
}

/// Sign-in and empty-page headings.
pub fn heading() -> FontId {
    FontId::new(22.0, semibold())
}

/// Sidebar rows.
pub fn item() -> FontId {
    FontId::new(13.5, FontFamily::Proportional)
}

pub fn item_strong() -> FontId {
    FontId::new(13.5, semibold())
}

/// Section labels in the sidebar, author names under replies.
pub fn label() -> FontId {
    FontId::new(12.0, medium())
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
    FontId::new(14.0, semibold())
}

/// Small text: previews, card details.
pub fn small() -> FontId {
    FontId::new(12.5, FontFamily::Proportional)
}

pub fn small_strong() -> FontId {
    FontId::new(12.5, medium())
}

/// Times, counts and day dividers.
pub fn meta() -> FontId {
    FontId::new(11.5, FontFamily::Monospace)
}

pub fn code() -> FontId {
    FontId::new(12.5, FontFamily::Monospace)
}

/// Initials in avatars of `size`.
pub fn initials(size: f32) -> FontId {
    FontId::new((size * 0.38).round(), semibold())
}
