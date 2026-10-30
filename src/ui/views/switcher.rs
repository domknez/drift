//! The quick switcher (⌘T): a box over the window to jump to a conversation, a
//! channel or a place by typing part of its name. Empty, it offers the places and the
//! conversations opened last. ↑↓ pick, Enter goes, Esc closes.

use super::super::style::icons::{self, Icon};
use super::super::style::{colors, typography as ty};
use super::super::workspace::{Chip, Section, Side, Workspace};
use crate::model::{Conversation, ConversationId, Directory, Kind};
use eframe::egui::{self, Align2, Color32, CornerRadius, Frame, Key, Margin, Rect, RichText, Sense, Stroke, pos2, vec2};

const WIDTH: f32 = 620.0;
const ROW: f32 = 40.0;
/// Most rows listed.
const MOST: usize = 9;

#[derive(Default)]
pub struct Switcher {
    query: String,
    /// The row picked with the arrows.
    index: usize,
}

impl Switcher {
    /// Open with `query` typed (for the demo).
    pub fn with(query: &str) -> Switcher {
        Switcher { query: query.to_string(), index: 0 }
    }
}

/// Where a row goes.
#[derive(Clone)]
enum Target {
    Conversation(ConversationId),
    Section(Section),
    Unread,
    Join(String),
}

struct Row {
    target: Target,
    title: String,
    /// Where it belongs, or what it is.
    detail: String,
    icon: Option<Icon>,
    /// A conversation's own picture or mark leads its row.
    conversation: Option<Conversation>,
}

pub fn show(ctx: &egui::Context, ws: &mut Workspace) {
    // Taken out while it's drawn (rows draw pictures from the workspace), put back after.
    let Some(mut switcher) = ws.switcher.take() else { return };
    // The keys it answers to, taken before the text field sees them.
    let (up, down, enter, escape) = ctx.input_mut(|i| {
        (
            i.consume_key(egui::Modifiers::NONE, Key::ArrowUp),
            i.consume_key(egui::Modifiers::NONE, Key::ArrowDown),
            i.consume_key(egui::Modifiers::NONE, Key::Enter),
            i.consume_key(egui::Modifiers::NONE, Key::Escape),
        )
    });
    if escape {
        return;
    }
    let query = switcher.query.trim().to_lowercase();
    let rows = rows(ws, &query);
    if down {
        switcher.index = (switcher.index + 1).min(rows.len().saturating_sub(1));
    }
    if up {
        switcher.index = switcher.index.saturating_sub(1);
    }
    switcher.index = switcher.index.min(rows.len().saturating_sub(1));
    let mut chosen = enter.then(|| rows.get(switcher.index).map(|r| r.target.clone())).flatten();

    let p = colors();
    let screen = ctx.content_rect();
    // A dimmed window behind; a click on it closes the switcher.
    let mut close = false;
    egui::Area::new(egui::Id::unique("switcher-backdrop")).order(egui::Order::Middle).fixed_pos(screen.min).show(ctx, |ui| {
        let response = ui.allocate_rect(screen, Sense::click());
        ui.painter().rect_filled(screen, CornerRadius::ZERO, Color32::from_black_alpha(70));
        close = response.clicked();
    });
    egui::Area::new(egui::Id::unique("switcher"))
        .order(egui::Order::Foreground)
        .pivot(Align2::CENTER_TOP)
        .fixed_pos(pos2(screen.center().x, screen.top() + 90.0))
        .show(ctx, |ui| {
            Frame::new()
                .fill(p.panel)
                .stroke(Stroke::new(1.0, p.border))
                .corner_radius(CornerRadius::same(14))
                .shadow(egui::Shadow { offset: [0, 12], blur: 36, spread: 0, color: Color32::from_black_alpha(70) })
                .show(ui, |ui| {
                    ui.set_width(WIDTH);
                    ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
                    // The search line.
                    Frame::new().inner_margin(Margin::symmetric(16, 14)).show(ui, |ui| {
                        ui.horizontal(|ui| {
                            let (icon, _) = ui.allocate_exact_size(vec2(18.0, 18.0), Sense::hover());
                            icons::paint(ui.painter(), icon, Icon::Search, p.dim);
                            ui.add_space(10.0);
                            let edit = egui::TextEdit::singleline(&mut switcher.query)
                                .hint_text(RichText::new("Jump to a conversation, channel or place").color(p.faint))
                                .font(ty::body())
                                .frame(Frame::NONE)
                                .desired_width(f32::INFINITY);
                            let response = ui.add(edit);
                            response.request_focus();
                            if response.changed() {
                                switcher.index = 0;
                            }
                        });
                    });
                    ui.painter().hline(ui.min_rect().x_range(), ui.min_rect().bottom(), Stroke::new(1.0, p.line));
                    ui.add_space(6.0);
                    let label = if query.is_empty() {
                        "Recent"
                    } else if rows.is_empty() {
                        "Nothing matches"
                    } else {
                        "Results"
                    };
                    Frame::new().inner_margin(Margin { left: 18, right: 18, top: 6, bottom: 6 }).show(ui, |ui| {
                        ui.label(RichText::new(label).font(ty::small()).color(p.faint));
                    });
                    for (i, row) in rows.iter().enumerate() {
                        let (rect, response) = ui.allocate_exact_size(vec2(WIDTH, ROW), Sense::click());
                        let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
                        if response.hovered() && ui.input(|i| i.pointer.delta() != egui::Vec2::ZERO) {
                            switcher.index = i;
                        }
                        let on = i == switcher.index;
                        let inner = rect.shrink2(vec2(8.0, 1.0));
                        if on {
                            ui.painter().rect_filled(inner, CornerRadius::same(9), p.hover);
                        }
                        draw_row(ui, ws, row, inner, on);
                        if response.clicked() {
                            chosen = Some(row.target.clone());
                        }
                    }
                    ui.add_space(6.0);
                    ui.painter().hline(ui.min_rect().x_range(), ui.cursor().top(), Stroke::new(1.0, p.line));
                    Frame::new().inner_margin(Margin::symmetric(18, 10)).show(ui, |ui| {
                        ui.label(RichText::new("↑ ↓  select      ↵  open      esc  close").font(ty::meta()).color(p.faint));
                    });
                });
        });
    if let Some(target) = chosen {
        go(ctx, ws, target);
    } else if !close {
        ws.switcher = Some(switcher);
    }
}

/// One row: its mark, its name, where it belongs, and "Enter" on the picked one.
fn draw_row(ui: &mut egui::Ui, ws: &mut Workspace, row: &Row, rect: Rect, on: bool) {
    let p = colors();
    let mark = Rect::from_min_size(pos2(rect.left() + 10.0, rect.center().y - 13.0), vec2(26.0, 26.0));
    match (&row.conversation, row.icon) {
        (Some(c), _) => super::list::lead(ui, ws, c, mark),
        (None, Some(icon)) => {
            ui.painter().rect_filled(mark, CornerRadius::same(8), p.surface);
            icons::paint(ui.painter(), Rect::from_center_size(mark.center(), vec2(15.0, 15.0)), icon, p.dim);
        }
        (None, None) => {}
    }
    let x = mark.right() + 12.0;
    let title = ui.painter().layout_no_wrap(row.title.clone(), ty::item(), p.text);
    let title_w = title.size().x.min(rect.width() - 200.0);
    ui.painter().with_clip_rect(Rect::from_min_max(pos2(x, rect.top()), pos2(x + title_w, rect.bottom()))).galley(
        pos2(x, rect.center().y - title.size().y / 2.0),
        title,
        p.text,
    );
    if !row.detail.is_empty() {
        let detail = ui.painter().layout_no_wrap(row.detail.clone(), ty::small(), p.faint);
        let dx = x + title_w + 10.0;
        let room = rect.right() - 70.0 - dx;
        if room > 20.0 {
            ui.painter().with_clip_rect(Rect::from_min_max(pos2(dx, rect.top()), pos2(dx + room, rect.bottom()))).galley(
                pos2(dx, rect.center().y - detail.size().y / 2.0),
                detail,
                p.faint,
            );
        }
    }
    if on {
        let enter = ui.painter().layout_no_wrap("Enter".into(), ty::meta(), p.dim);
        let pill = Rect::from_min_size(
            pos2(rect.right() - 14.0 - enter.size().x - 14.0, rect.center().y - 11.0),
            vec2(enter.size().x + 14.0, 22.0),
        );
        ui.painter().rect_stroke(pill, CornerRadius::same(6), Stroke::new(1.0, p.border), egui::StrokeKind::Inside);
        ui.painter().galley(pill.center() - enter.size() / 2.0, enter, p.dim);
    }
}

/// What's offered for `query` (lowercase, trimmed): the best matches, or with no
/// query the places and the conversations opened last.
fn rows(ws: &Workspace, query: &str) -> Vec<Row> {
    let Some(d) = ws.directory.as_ref() else { return Vec::new() };
    let mut places = vec![
        Row {
            target: Target::Section(Section::Activity),
            title: "Activity".into(),
            detail: "Mentions and replies".into(),
            icon: Some(Icon::Bell),
            conversation: None,
        },
        Row {
            target: Target::Unread,
            title: "Unreads".into(),
            detail: "Chats with something new".into(),
            icon: Some(Icon::Chat),
            conversation: None,
        },
        Row {
            target: Target::Section(Section::Chats),
            title: "Chats".into(),
            detail: String::new(),
            icon: Some(Icon::Chat),
            conversation: None,
        },
        Row {
            target: Target::Section(Section::Teams),
            title: "Teams".into(),
            detail: "Teams and channels".into(),
            icon: Some(Icon::Members),
            conversation: None,
        },
        Row {
            target: Target::Section(Section::Meetings),
            title: "Calendar".into(),
            detail: "Today's meetings".into(),
            icon: Some(Icon::Calendar),
            conversation: None,
        },
    ];
    if let Some((subject, when, url)) = super::topbar::meeting_to_join(ws) {
        places.insert(
            0,
            Row { target: Target::Join(url), title: format!("Join {subject}"), detail: when, icon: Some(Icon::Video), conversation: None },
        );
    }
    if query.is_empty() {
        // The conversations opened last; at first, the latest active.
        let mut recent: Vec<&Conversation> = ws.recent.iter().filter_map(|id| d.conversation(id)).collect();
        if recent.is_empty() {
            recent = d.conversations.iter().filter(|c| c.kind != Kind::Feed).collect();
            recent.sort_by_key(|c| std::cmp::Reverse(c.last.as_ref().map_or(0, |l| l.at)));
        }
        let mut rows: Vec<Row> = places.into_iter().take(3).collect();
        rows.extend(recent.into_iter().filter(|c| ws.open.as_deref() != Some(c.id.as_str())).take(MOST - rows.len()).map(|c| row_of(d, c)));
        return rows;
    }
    let mut scored: Vec<(i64, Row)> = Vec::new();
    for c in d.conversations.iter().filter(|c| c.kind != Kind::Feed) {
        let row = row_of(d, c);
        let name = searchable(d, c);
        if let Some(score) = score(&name, query) {
            let recency = ws.recent.iter().position(|r| *r == c.id).map_or(0, |i| 30 - i as i64);
            let bonus = recency + i64::from(c.unread) * 5 + i64::from(c.pinned) * 5;
            let latest = c.last.as_ref().map_or(0, |l| l.at / 60_000_000);
            scored.push((score * 1000 + bonus * 10 + latest.min(9), row));
        }
    }
    for place in places {
        if let Some(score) = score(&place.title.to_lowercase(), query) {
            scored.push((score * 1000 + 400, place));
        }
    }
    scored.sort_by_key(|(score, _)| std::cmp::Reverse(*score));
    scored.into_iter().take(MOST).map(|(_, row)| row).collect()
}

/// A conversation's row: a channel with its team, a chat with "Chat" or its members.
fn row_of(d: &Directory, c: &Conversation) -> Row {
    let (detail, icon) = match c.kind {
        Kind::Channel => (d.channel(&c.id).map_or(String::new(), |(team, _)| team.name.clone()), Icon::Hash),
        Kind::Meeting => ("Meeting chat".into(), Icon::Calendar),
        Kind::Feed => (String::new(), Icon::Bell),
        Kind::Chat if c.members.len() > 1 => (format!("Group chat · {} people", c.members.len() + 1), Icon::Members),
        Kind::Chat => ("Chat".into(), Icon::Chat),
    };
    let title = match d.channel(&c.id) {
        Some((_, channel)) => channel.name.clone(),
        None => c.title.clone(),
    };
    let detail = if c.unread && !c.muted { format!("{detail} · new") } else { detail };
    Row { target: Target::Conversation(c.id.clone()), title, detail, icon: Some(icon), conversation: Some(c.clone()) }
}

/// What a conversation is found by: its name, and for a channel its team's too.
fn searchable(d: &Directory, c: &Conversation) -> String {
    match d.channel(&c.id) {
        Some((team, channel)) => format!("{} {}", channel.name, team.name).to_lowercase(),
        None => c.title.to_lowercase(),
    }
}

/// How well `name` matches `query` (both lowercase): the start of the name beats the
/// start of a word, which beats anywhere in it, which beats its letters in order.
fn score(name: &str, query: &str) -> Option<i64> {
    if name.starts_with(query) {
        return Some(4);
    }
    if name.split(|c: char| !c.is_alphanumeric()).any(|w| w.starts_with(query)) {
        return Some(3);
    }
    if name.contains(query) {
        return Some(2);
    }
    let mut letters = name.chars();
    query.chars().filter(|c| !c.is_whitespace()).all(|q| letters.any(|n| n == q)).then_some(1)
}

/// Go where a row points.
fn go(ctx: &egui::Context, ws: &mut Workspace, target: Target) {
    match target {
        Target::Conversation(id) => {
            let kind = ws.directory.as_ref().and_then(|d| d.conversation(&id)).map(|c| c.kind);
            ws.section = match kind {
                Some(Kind::Channel) => Section::Teams,
                Some(Kind::Feed) => Section::Activity,
                _ => Section::Chats,
            };
            ws.side = Side::List;
            ws.open(&id);
        }
        Target::Section(section) => ws.section = section,
        Target::Unread => {
            ws.section = Section::Chats;
            ws.chip = Chip::Unread;
        }
        Target::Join(url) => ctx.open_url(egui::OpenUrl::new_tab(crate::teams::web_join(&url))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranks_matches() {
        assert_eq!(score("maya lindqvist", "maya"), Some(4));
        assert_eq!(score("maya lindqvist", "lind"), Some(3));
        assert_eq!(score("pr reviews", "view"), Some(2));
        assert_eq!(score("pr reviews", "prv"), Some(1));
        assert_eq!(score("pr reviews", "xyz"), None);
    }
}
