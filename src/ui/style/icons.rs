//! Small line icons painted with egui shapes, drawn on a 24-unit grid scaled into
//! the icon's rect.

use eframe::egui::{Color32, Painter, Pos2, Rect, Shape, Stroke, pos2};
use std::f32::consts::{PI, TAU};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Icon {
    /// A channel.
    Hash,
    ChevronDown,
    ChevronRight,
    /// A calendar page: meetings.
    Calendar,
    /// A pull request's branches: cards from code hosts.
    Card,
    /// A paper plane: send.
    Send,
    /// An at sign: activity and mentions.
    At,
    /// A camera: a meeting / join.
    Video,
    /// A bell with a slash: muted.
    Mute,
    /// Two people: members.
    Members,
    /// A magnifier: search.
    Search,
    /// A speech bubble: chats.
    Chat,
    /// A bell: activity.
    Bell,
    /// A cross: close.
    Close,
    /// A smiling face: emoji.
    Smile,
    /// A paperclip: attach.
    Clip,
}

/// Paint `icon` into the square `rect`.
pub fn paint(painter: &Painter, rect: Rect, icon: Icon, color: Color32) {
    let s = rect.width() / 24.0;
    let at = |x: f32, y: f32| pos2(rect.left() + x * s, rect.top() + y * s);
    let stroke = Stroke::new((1.7 * s).max(1.2), color);
    let line = |points: &[(f32, f32)]| painter.add(Shape::line(points.iter().map(|&(x, y)| at(x, y)).collect(), stroke));
    let circle = |x: f32, y: f32, r: f32| painter.circle_stroke(at(x, y), r * s, stroke);
    let arc = |x: f32, y: f32, r: f32, from: f32, to: f32| painter.add(Shape::line(arc_points(at(x, y), r * s, from, to), stroke));
    match icon {
        Icon::Hash => {
            line(&[(4.5, 9.0), (19.5, 9.0)]);
            line(&[(4.5, 15.0), (19.5, 15.0)]);
            line(&[(10.0, 3.5), (8.0, 20.5)]);
            line(&[(16.0, 3.5), (14.0, 20.5)]);
        }
        Icon::ChevronDown => {
            line(&[(7.0, 10.0), (12.0, 15.0), (17.0, 10.0)]);
        }
        Icon::ChevronRight => {
            line(&[(10.0, 7.0), (15.0, 12.0), (10.0, 17.0)]);
        }
        Icon::Calendar => {
            painter.rect_stroke(Rect::from_min_max(at(3.5, 5.0), at(20.5, 21.0)), 2.0 * s, stroke, eframe::egui::StrokeKind::Middle);
            line(&[(3.5, 10.0), (20.5, 10.0)]);
            line(&[(8.0, 3.0), (8.0, 7.0)]);
            line(&[(16.0, 3.0), (16.0, 7.0)]);
        }
        Icon::Card => {
            circle(6.0, 6.0, 2.6);
            circle(18.0, 18.0, 2.6);
            line(&[(6.0, 8.6), (6.0, 21.0)]);
            line(&[(13.0, 6.0), (16.0, 6.0), (18.0, 8.0), (18.0, 15.4)]);
        }
        Icon::Send => {
            line(&[(21.0, 3.0), (10.5, 13.5)]);
            line(&[(21.0, 3.0), (14.0, 21.0), (10.5, 13.5), (3.0, 10.0), (21.0, 3.0)]);
        }
        Icon::At => {
            circle(12.0, 12.0, 3.8);
            line(&[(15.8, 8.2), (15.8, 13.0)]);
            arc(18.8, 13.0, 3.0, PI, TAU);
            arc(12.0, 12.0, 9.0, 0.15 * PI, 1.8 * PI);
        }
        Icon::Video => {
            painter.rect_stroke(Rect::from_min_max(at(3.0, 7.0), at(15.0, 17.0)), 2.0 * s, stroke, eframe::egui::StrokeKind::Middle);
            line(&[(15.0, 10.5), (21.0, 7.5), (21.0, 16.5), (15.0, 13.5)]);
        }
        Icon::Mute => {
            arc(12.0, 11.0, 6.0, PI, TAU);
            line(&[(6.0, 11.0), (6.0, 16.0), (18.0, 16.0), (18.0, 11.0)]);
            line(&[(10.0, 19.0), (14.0, 19.0)]);
            line(&[(3.0, 4.0), (21.0, 21.0)]);
        }
        Icon::Members => {
            circle(9.0, 8.0, 3.2);
            arc(9.0, 20.0, 6.0, PI, TAU);
            circle(16.5, 8.5, 2.6);
            arc(17.5, 19.0, 4.5, 1.15 * PI, TAU);
        }
        Icon::Search => {
            circle(11.0, 11.0, 6.5);
            line(&[(16.0, 16.0), (20.5, 20.5)]);
        }
        Icon::Bell => {
            arc(12.0, 10.0, 6.0, PI, TAU);
            line(&[(6.0, 10.0), (6.0, 15.0), (4.0, 17.5), (20.0, 17.5), (18.0, 15.0), (18.0, 10.0)]);
            arc(12.0, 19.5, 2.0, 0.0, PI);
        }
        Icon::Chat => {
            line(&[(4.0, 5.0), (20.0, 5.0), (20.0, 16.0), (8.0, 16.0), (4.0, 20.0), (4.0, 5.0)]);
        }
        Icon::Close => {
            line(&[(6.0, 6.0), (18.0, 18.0)]);
            line(&[(18.0, 6.0), (6.0, 18.0)]);
        }
        Icon::Smile => {
            circle(12.0, 12.0, 9.0);
            arc(12.0, 12.0, 5.0, 0.2 * PI, 0.8 * PI);
            painter.circle_filled(at(9.0, 9.5), 1.1 * s, color);
            painter.circle_filled(at(15.0, 9.5), 1.1 * s, color);
        }
        Icon::Clip => {
            line(&[(16.5, 7.0), (8.5, 15.0)]);
            arc(10.0, 16.5, 2.1, 0.25 * PI, 1.25 * PI);
            line(&[(8.5, 18.0), (18.5, 8.0)]);
            arc(16.0, 5.5, 3.5, -0.75 * PI, 0.25 * PI);
            line(&[(13.5, 3.0), (5.0, 11.5)]);
            arc(9.0, 15.5, 5.6, 0.75 * PI, 1.25 * PI);
        }
    }
}

fn arc_points(center: Pos2, r: f32, from: f32, to: f32) -> Vec<Pos2> {
    let steps = 24;
    (0..=steps)
        .map(|i| {
            let a = from + (to - from) * i as f32 / steps as f32;
            pos2(center.x + r * a.cos(), center.y + r * a.sin())
        })
        .collect()
}
