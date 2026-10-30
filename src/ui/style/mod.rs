//! The look: palette, type, dimensions and icons, and the egui style they make.
//! Views use these names rather than raw values.

pub mod icons;
pub mod metrics;
pub mod palette;
pub mod typography;

pub use palette::colors;

use eframe::egui::{self, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Stroke, TextStyle, Visuals};
use std::sync::Arc;

pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    for (name, bytes) in [
        ("Geist", &include_bytes!("../../../assets/fonts/Geist-Regular.ttf")[..]),
        ("Geist-Medium", include_bytes!("../../../assets/fonts/Geist-Medium.ttf")),
        ("Geist-SemiBold", include_bytes!("../../../assets/fonts/Geist-SemiBold.ttf")),
        ("GeistMono", include_bytes!("../../../assets/fonts/GeistMono-Regular.ttf")),
        // Wider coverage (accents, symbols) where Geist has no glyph.
        ("Inter", include_bytes!("../../../assets/fonts/Inter-Regular.ttf")),
        // Monochrome emoji (egui can't paint colour fonts): reactions, message emoji.
        ("Emoji", include_bytes!("../../../assets/fonts/NotoEmoji-Regular.ttf")),
    ] {
        fonts.font_data.insert(name.into(), Arc::new(FontData::from_static(bytes)));
    }
    // egui's own fonts stay last in every chain as a final fallback.
    let fallbacks = fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    let chain = |first: &str| [vec![first.to_string(), "Inter".to_string(), "Emoji".to_string()], fallbacks.clone()].concat();
    fonts.families.insert(FontFamily::Proportional, chain("Geist"));
    fonts.families.insert(typography::medium(), chain("Geist-Medium"));
    fonts.families.insert(typography::semibold(), chain("Geist-SemiBold"));
    fonts.families.insert(FontFamily::Monospace, chain("GeistMono"));
    ctx.set_fonts(fonts);
}

pub fn apply(ctx: &egui::Context, dark: bool) {
    palette::set_dark(dark);
    let theme = if dark { egui::Theme::Dark } else { egui::Theme::Light };
    ctx.set_theme(theme);
    ctx.set_visuals_of(theme, visuals(dark));
    ctx.all_styles_mut(|s| {
        s.text_styles = [
            (TextStyle::Heading, typography::heading()),
            (TextStyle::Body, typography::body()),
            (TextStyle::Button, FontId::new(13.5, typography::medium())),
            (TextStyle::Small, typography::small()),
            (TextStyle::Monospace, typography::code()),
        ]
        .into();
        s.spacing.item_spacing = egui::vec2(8.0, 6.0);
        s.spacing.button_padding = egui::vec2(12.0, 6.0);
        s.spacing.scroll = egui::style::ScrollStyle::floating();
        s.spacing.scroll.bar_width = 6.0;
        s.interaction.selectable_labels = false;
    });
}

fn visuals(dark: bool) -> Visuals {
    let p = colors();
    let mut v = if dark { Visuals::dark() } else { Visuals::light() };
    v.panel_fill = p.bg;
    v.window_fill = p.surface;
    v.window_stroke = Stroke::new(1.0, p.line);
    v.window_corner_radius = CornerRadius::same(metrics::POST_RADIUS);
    v.menu_corner_radius = CornerRadius::same(10);
    v.extreme_bg_color = p.surface;
    v.faint_bg_color = p.hover;
    v.code_bg_color = p.code;
    v.hyperlink_color = p.accent;
    v.selection.bg_fill = p.accent_fill.gamma_multiply(0.35);
    v.selection.stroke = Stroke::new(1.0, p.accent);
    v.text_cursor.stroke = Stroke::new(2.0, p.accent);
    v.popup_shadow = egui::Shadow { offset: [0, 8], blur: 24, spread: 0, color: Color32::from_black_alpha(if dark { 110 } else { 40 }) };
    v.window_shadow = v.popup_shadow;
    for (w, bg) in [
        (&mut v.widgets.noninteractive, Color32::TRANSPARENT),
        (&mut v.widgets.inactive, p.surface),
        (&mut v.widgets.hovered, p.hover),
        (&mut v.widgets.active, p.selected),
        (&mut v.widgets.open, p.hover),
    ] {
        w.bg_fill = bg;
        w.weak_bg_fill = bg;
        w.bg_stroke = Stroke::NONE;
        w.corner_radius = CornerRadius::same(metrics::RADIUS);
        w.fg_stroke.color = p.text;
    }
    v.widgets.noninteractive.fg_stroke.color = p.dim;
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, p.line);
    v.widgets.inactive.bg_stroke = Stroke::new(1.0, p.border);
    v
}
