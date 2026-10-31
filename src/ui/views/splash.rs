//! The start-up screen: drift's icon with its three waves drifting, and what's
//! being loaded, over the window until the list, the open conversation and the
//! user's picture are in. It fades out to the workspace drawn underneath.

use crate::ui::style::{colors, typography as ty};
use eframe::egui::{self, Align2, Color32, Pos2, Rect, Stroke, TextureHandle, pos2, vec2};
use std::f32::consts::TAU;

/// The icon tile's side (points).
const TILE: f32 = 120.0;
/// The tile's corner radius, as a share of its side (as in the icon).
const ROUND: f32 = 185.0 / 824.0;
/// The tile's gradient, top left to bottom right (as in the icon).
const GRADIENT: [(f32, [u8; 3]); 3] = [(0.0, [0x9d, 0x8f, 0xff]), (0.55, [0x74, 0x61, 0xf0]), (1.0, [0x4a, 0x35, 0xc8])];

/// One of the icon's waves, in the icon's units across its 824-unit tile (from its
/// top left corner): where it starts and ends, its height and its colour (opaque:
/// the icon's half-white top wave is white mixed with the violet under it, so its
/// round ends don't show where they overlap the line).
struct Wave {
    from: f32,
    to: f32,
    y: f32,
    color: Color32,
}

const WAVES: [Wave; 3] = [
    Wave { from: 230.0, to: 650.0, y: 292.0, color: Color32::from_rgb(0xc8, 0xc0, 0xfa) },
    Wave { from: 162.0, to: 642.0, y: 412.0, color: Color32::WHITE },
    Wave { from: 174.0, to: 594.0, y: 532.0, color: Color32::from_rgb(0xc9, 0xfb, 0xe9) },
];
/// A wave's length and height, in the icon's units.
const WAVELENGTH: f32 = 420.0;
const AMPLITUDE: f32 = 45.0;
const STROKE: f32 = 60.0;

/// Draw the start-up screen over everything, `opacity` 1 while loading and down to
/// 0 as it fades out. It takes the pointer (and drags the window) while it's shown.
pub fn show(ctx: &egui::Context, stage: &str, opacity: f32) {
    let p = colors();
    let screen = ctx.content_rect();
    let fade = |c: Color32| c.gamma_multiply(opacity);
    let time = ctx.input(|i| i.time) as f32;
    egui::Area::new(egui::Id::unique("splash")).order(egui::Order::Foreground).fixed_pos(screen.min).show(ctx, |ui| {
        let (rect, _) = ui.allocate_exact_size(screen.size(), egui::Sense::hover());
        super::drag_area(ui, rect, "splash-drag");
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, fade(p.canvas));

        // The tile floats a little, up and down.
        let center = rect.center() - vec2(0.0, 36.0) + vec2(0.0, (time * 1.3).sin() * 4.0);
        let tile = Rect::from_center_size(center, vec2(TILE, TILE));
        let radius = TILE * ROUND;
        let shadow = egui::Shadow { offset: [0, 10], blur: 28, spread: 0, color: fade(Color32::from_black_alpha(70)) };
        painter.add(shadow.as_shape(tile, radius));
        let texture = tile_texture(ctx);
        painter.image(texture.id(), tile, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), fade(Color32::WHITE));
        painter.rect_stroke(tile.shrink(0.5), radius, Stroke::new(1.0, fade(Color32::from_white_alpha(36))), egui::StrokeKind::Inside);

        // The waves roll along, each a beat behind the one above.
        let scale = TILE / 824.0;
        let at = |x: f32, y: f32| pos2(tile.left() + x * scale, tile.top() + y * scale);
        for (i, wave) in WAVES.iter().enumerate() {
            let phase = time * 2.2 - i as f32 * 0.9;
            let sway = (time * 0.8 + i as f32).sin() * 14.0;
            let steps = 48;
            let points: Vec<Pos2> = (0..=steps)
                .map(|s| {
                    let x = wave.from + (wave.to - wave.from) * s as f32 / steps as f32;
                    let y = wave.y - AMPLITUDE * (TAU * (x - wave.from) / WAVELENGTH + phase).sin();
                    at(x + sway, y)
                })
                .collect();
            let color = fade(wave.color);
            let width = STROKE * scale;
            // Round ends, as the icon's.
            for end in [points[0], points[steps]] {
                painter.circle_filled(end, width / 2.0, color);
            }
            painter.add(egui::epaint::PathShape::line(points, Stroke::new(width, color)));
        }

        // The wordmark, and what's on its way.
        let below = tile.bottom() + 34.0 - (time * 1.3).sin() * 4.0;
        let word = painter.layout_no_wrap("drift".into(), ty::wordmark(), fade(p.text));
        let dot = painter.layout_no_wrap(".".into(), ty::wordmark(), fade(p.accent));
        let left = rect.center().x - (word.size().x + dot.size().x) / 2.0;
        let word_w = word.size().x;
        painter.galley(pos2(left, below), word, fade(p.text));
        painter.galley(pos2(left + word_w, below), dot, fade(p.accent));
        painter.text(pos2(rect.center().x, below + 44.0), Align2::CENTER_TOP, stage, ty::body(), fade(p.faint));
    });
    ctx.request_repaint();
}

/// The icon's rounded, violet tile, drawn once.
fn tile_texture(ctx: &egui::Context) -> TextureHandle {
    let id = egui::Id::unique("splash-tile");
    if let Some(texture) = ctx.data(|d| d.get_temp::<TextureHandle>(id)) {
        return texture;
    }
    const SIDE: usize = 256;
    let side = SIDE as f32;
    let radius = side * ROUND;
    let pixels = (0..SIDE * SIDE)
        .map(|i| {
            let (x, y) = ((i % SIDE) as f32 + 0.5, (i / SIDE) as f32 + 0.5);
            // Coverage from the distance to the rounded square's edge (antialiased).
            let (dx, dy) = ((x - side / 2.0).abs() - (side / 2.0 - radius), (y - side / 2.0).abs() - (side / 2.0 - radius));
            let outside = vec2(dx.max(0.0), dy.max(0.0)).length() + dx.max(dy).min(0.0) - radius;
            let coverage = (0.5 - outside).clamp(0.0, 1.0);
            let [r, g, b] = gradient((x + y) / (2.0 * side));
            Color32::from_rgba_unmultiplied(r, g, b, (coverage * 255.0).round() as u8)
        })
        .collect();
    let texture = ctx.load_texture("splash-tile", egui::ColorImage::new([SIDE, SIDE], pixels), egui::TextureOptions::LINEAR);
    ctx.data_mut(|d| d.insert_temp(id, texture.clone()));
    texture
}

/// The tile's colour at `t` (0 top left, 1 bottom right).
fn gradient(t: f32) -> [u8; 3] {
    let i = GRADIENT.iter().rposition(|(at, _)| *at <= t).unwrap_or(0).min(GRADIENT.len() - 2);
    let ((a, from), (b, to)) = (GRADIENT[i], GRADIENT[i + 1]);
    let k = ((t - a) / (b - a)).clamp(0.0, 1.0);
    std::array::from_fn(|c| (f32::from(from[c]) + (f32::from(to[c]) - f32::from(from[c])) * k).round() as u8)
}
