//! Someone's card, over the window where their name or picture was clicked: who
//! they are, how they show now (with their status message, and until when they're
//! out of office), how to reach them, and a chat or call with them.

use super::super::load::Load;
use super::super::style::{colors, typography as ty};
use super::super::widgets;
use super::super::workspace::Workspace;
use crate::backend::Error;
use crate::model::{Picture, Profile, time};
use eframe::egui::{self, CornerRadius, Frame, Margin, RichText, Stroke, Ui, vec2};
use std::time::{Duration, Instant};

const WIDTH: f32 = 320.0;
const PICTURE: f32 = 64.0;
/// An automatic reply is cut to this many characters.
const AWAY_NOTE: usize = 280;
/// A card loaded this recently is shown as it is, not loaded again.
const FRESH: Duration = Duration::from_secs(60);

/// A card open over the window.
pub struct Card {
    pub id: String,
    /// The name it was opened by (until the profile's is in).
    pub name: String,
    /// Where it was opened (it opens there, kept on screen).
    at: egui::Pos2,
    /// The frame it was opened on: the click that opened it doesn't close it.
    opened: u64,
    load: Load<Profile, Error>,
    failed: bool,
    copied: Option<(String, Instant)>,
}

impl Card {
    pub fn new(id: &str, name: &str, at: egui::Pos2, opened: u64, load: Load<Profile, Error>) -> Card {
        Card { id: id.to_string(), name: name.to_string(), at, opened, load, failed: false, copied: None }
    }
}

/// Open `id`'s card at `at`, loading their profile (shown at once when loaded a
/// moment ago). Bots and connectors have no profile: just their name.
pub fn open(ctx: &egui::Context, ws: &mut Workspace, id: &str, name: &str, at: egui::Pos2) {
    if id.is_empty() {
        return;
    }
    let fresh = ws.profiles.get(id).is_some_and(|(when, _)| when.elapsed() < FRESH);
    let load = if id.starts_with("8:") && !fresh {
        let (backend, person) = (ws.backend.clone(), id.to_string());
        Load::start(ctx, move || backend.profile(&person))
    } else {
        Load::Idle
    };
    ws.card = Some(Card::new(id, name, at + vec2(8.0, 8.0), ctx.cumulative_pass_nr(), load));
}

enum Do {
    Open(String),
    Link(String),
    Copy(String),
}

pub fn show(ctx: &egui::Context, ws: &mut Workspace) {
    let Some(id) = ws.card.as_ref().map(|c| c.id.clone()) else { return };
    let chat = ws.private_chat(&id);
    let Some(card) = &mut ws.card else { return };
    match card.load.take() {
        Some(Ok(profile)) => {
            ws.profiles.insert(id.clone(), (Instant::now(), profile));
        }
        Some(Err(e)) => {
            log_debug!("profile: {e}");
            card.failed = true;
        }
        None => {}
    }
    let p = colors();
    let now = time::now_ms();
    let profile = ws.profiles.get(&id).map(|(_, profile)| profile.clone());
    let known = profile.clone().unwrap_or_default();
    let name = Some(known.name.clone()).filter(|n| !n.trim().is_empty()).unwrap_or_else(|| card.name.clone());
    let presence = known.presence.or_else(|| ws.presence.get(&id).copied());
    let picture = ws.pictures.get(&Picture::Person { id: id.clone(), name: card.name.clone() });
    let loading = card.load.is_running() && profile.is_none();
    let mut action = None;

    let frame = Frame::new()
        .fill(p.panel)
        .stroke(Stroke::new(1.0, p.border))
        .corner_radius(CornerRadius::same(14))
        .inner_margin(Margin::same(18))
        .shadow(ctx.global_style().visuals.popup_shadow);
    let area = egui::Area::new(egui::Id::unique("profile-card")).order(egui::Order::Foreground).fixed_pos(card.at).constrain(true).show(
        ctx,
        |ui| {
            frame.show(ui, |ui| {
                ui.set_width(WIDTH);
                ui.spacing_mut().item_spacing.y = 4.0;
                // Picture, name, what they do, how they show.
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 14.0;
                    let r = widgets::avatar(ui, &id, &name, PICTURE, picture);
                    if let Some(presence) = presence {
                        widgets::presence_dot(ui.painter(), r.rect.right_bottom() - vec2(7.0, 7.0), 8.0, presence, p.panel);
                    }
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 3.0;
                        ui.add_space(4.0);
                        ui.add(egui::Label::new(RichText::new(&name).font(ty::title()).color(p.text)).wrap());
                        let work =
                            [known.title.as_str(), known.department.as_str()].iter().filter(|s| !s.is_empty()).copied().collect::<Vec<_>>();
                        if !work.is_empty() {
                            ui.add(egui::Label::new(RichText::new(work.join(" · ")).font(ty::small()).color(p.dim)).wrap());
                        }
                        if let Some(presence) = presence {
                            let mut shows = presence.label().to_string();
                            if !known.activity.is_empty() && !presence.out_of_office {
                                shows = format!("{shows} · {}", known.activity);
                            }
                            ui.label(RichText::new(shows).font(ty::small()).color(p.faint));
                        }
                    });
                });
                if !known.note.is_empty() {
                    ui.add_space(8.0);
                    boxed(ui, p.surface, |ui| {
                        ui.add(egui::Label::new(RichText::new(&known.note).font(ty::small()).color(p.text)).wrap());
                    });
                }
                if presence.is_some_and(|p| p.out_of_office) {
                    ui.add_space(8.0);
                    boxed(ui, p.surface, |ui| {
                        let head = known
                            .away_until
                            .map_or_else(|| "Out of office".to_string(), |until| format!("Out of office · {}", until_words(until, now)));
                        ui.label(RichText::new(head).font(ty::small_strong()).color(p.text));
                        if !known.away_note.is_empty() {
                            let mut note: String = known.away_note.chars().take(AWAY_NOTE).collect();
                            if note.len() < known.away_note.len() {
                                note.push('…');
                            }
                            ui.add(egui::Label::new(RichText::new(note).font(ty::small()).color(p.dim)).wrap());
                        }
                    });
                }

                // How to reach them.
                let rows = [
                    ("Email", known.email.as_str()),
                    ("Phone", known.phone.as_str()),
                    ("Office", known.office.as_str()),
                    ("Company", known.company.as_str()),
                    ("Reports to", known.manager.as_str()),
                ];
                if rows.iter().any(|(_, v)| !v.is_empty()) {
                    ui.add_space(10.0);
                    ui.separator();
                    ui.add_space(4.0);
                }
                for (label, value) in rows.into_iter().filter(|(_, v)| !v.is_empty()) {
                    ui.horizontal(|ui| {
                        ui.add_sized(vec2(78.0, 18.0), egui::Label::new(RichText::new(label).font(ty::small()).color(p.faint)));
                        let copyable = matches!(label, "Email" | "Phone");
                        let copied =
                            card.copied.as_ref().is_some_and(|(what, when)| what == value && when.elapsed() < Duration::from_secs(2));
                        let shown = if copied { "Copied".to_string() } else { value.to_string() };
                        let text = RichText::new(shown).font(ty::small()).color(if copyable { p.accent } else { p.text });
                        let r = ui.add(egui::Label::new(text).truncate().sense(if copyable {
                            egui::Sense::click()
                        } else {
                            egui::Sense::hover()
                        }));
                        if copyable && r.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text("Copy").clicked() {
                            action = Some(Do::Copy(value.to_string()));
                        }
                    });
                }
                if loading {
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        ui.add(egui::Spinner::new().size(14.0));
                        ui.label(RichText::new("Loading…").font(ty::small()).color(p.faint));
                    });
                } else if card.failed && profile.is_none() && id.starts_with("8:") {
                    ui.add_space(8.0);
                    ui.label(RichText::new("Couldn't load their profile.").font(ty::small()).color(p.faint));
                }

                // A chat or a call with them.
                if id.starts_with("8:") {
                    ui.add_space(12.0);
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 8.0;
                        let button = |label: &str, main: bool| {
                            let text = RichText::new(label).font(ty::small_strong()).color(if main { p.on_accent } else { p.text });
                            let b = egui::Button::new(text).corner_radius(CornerRadius::same(8)).min_size(vec2(0.0, 32.0));
                            if main { b.fill(p.accent_fill) } else { b }
                        };
                        let address = Some(known.email.clone()).filter(|a| a.contains('@'));
                        if (chat.is_some() || address.is_some()) && ui.add(button("Message", true)).clicked() {
                            action = match (&chat, &address) {
                                (Some(chat), _) => Some(Do::Open(chat.clone())),
                                (None, Some(address)) => Some(Do::Link(teams_link("chat", address))),
                                _ => None,
                            };
                        }
                        if let Some(address) = &address
                            && ui.add(button("Call", false)).on_hover_text("Teams on the web").clicked()
                        {
                            action = Some(Do::Link(teams_link("call", address)));
                        }
                    });
                }
            });
        },
    );

    // Esc, or a click anywhere else, closes it.
    let outside = ctx.input(|i| i.pointer.any_pressed() && i.pointer.interact_pos().is_some_and(|at| !area.response.rect.contains(at)));
    let mut close = ctx.input(|i| i.key_pressed(egui::Key::Escape)) || (outside && ctx.cumulative_pass_nr() > card.opened);
    match action {
        Some(Do::Copy(what)) => {
            ctx.copy_text(what.clone());
            card.copied = Some((what, Instant::now()));
            ctx.request_repaint_after(Duration::from_secs(2));
        }
        Some(Do::Link(url)) => {
            ctx.open_url(egui::OpenUrl::new_tab(url));
            close = true;
        }
        Some(Do::Open(chat)) => {
            ws.card = None;
            ws.open(&chat);
            return;
        }
        None => {}
    }
    if close {
        ws.card = None;
    }
}

/// Content in a softly filled box, full width.
fn boxed(ui: &mut Ui, fill: egui::Color32, add: impl FnOnce(&mut Ui)) {
    Frame::new().fill(fill).corner_radius(CornerRadius::same(10)).inner_margin(Margin::symmetric(12, 10)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.spacing_mut().item_spacing.y = 4.0;
        add(ui);
    });
}

/// Until when someone's away: back on a day (an end at midnight), or until a time.
fn until_words(until: i64, now: i64) -> String {
    let days = time::day_number(until) - time::day_number(now);
    let day = time::day_ahead(now, days);
    let day = if days <= 1 { day.to_lowercase() } else { day };
    if time::minutes_of_day(until) == 0 { format!("back {day}") } else { format!("until {day} {}", time::clock(until)) }
}

/// Teams on the web, starting a chat or a call with `address`.
fn teams_link(what: &str, address: &str) -> String {
    url::Url::parse_with_params(&format!("https://teams.microsoft.com/l/{what}/0/0"), [("users", address)])
        .map_or_else(|_| "https://teams.microsoft.com".to_string(), String::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn says_until_when_someone_is_away() {
        let now = time::local_at(time::now_ms(), 0, 10 * 60);
        assert_eq!(until_words(time::local_at(now, 1, 0), now), "back tomorrow");
        assert_eq!(until_words(time::local_at(now, 0, 17 * 60), now), "until today 17:00");
        assert!(until_words(time::local_at(now, 5, 0), now).starts_with("back "));
        assert_eq!(teams_link("chat", "a@b.example"), "https://teams.microsoft.com/l/chat/0/0?users=a%40b.example");
    }
}
