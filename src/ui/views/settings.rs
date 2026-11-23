//! Settings, in a sheet over the window: its pages listed on the left (Appearance,
//! Layout, Notifications, Composer, Updates, Help), the page's settings on the
//! right. Opens from the user's menu, or with ⌘, (Ctrl+, elsewhere).

use super::topbar::{divider, menu_label, segments, switch_row, theme_swatch};
use crate::settings::{Accent, Appearance, Look, Options, TEXT_SIZES, Theme};
use crate::ui::app::Request;
use crate::ui::style::{colors, palette, typography as ty};
use crate::ui::updates::Updates;
use crate::ui::widgets;
use crate::ui::workspace::Workspace;
use eframe::egui::{self, Align2, CornerRadius, Frame, Margin, Rect, RichText, Sense, Stroke, Ui, pos2, vec2};

const WIDTH: f32 = 680.0;
/// The list of pages.
const NAV_WIDTH: f32 = 170.0;
const HEIGHT: f32 = 470.0;

/// A page of Settings, in the list on the left.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Page {
    #[default]
    Appearance,
    Layout,
    Notifications,
    Composer,
    Updates,
    Help,
}

impl Page {
    const ALL: [Page; 6] = [Page::Appearance, Page::Layout, Page::Notifications, Page::Composer, Page::Updates, Page::Help];

    /// A page by its name, lower case (the first one for any other).
    pub fn from_key(key: &str) -> Page {
        Page::ALL.into_iter().find(|p| p.label().eq_ignore_ascii_case(key)).unwrap_or_default()
    }

    fn label(self) -> &'static str {
        match self {
            Page::Appearance => "Appearance",
            Page::Layout => "Layout",
            Page::Notifications => "Notifications",
            Page::Composer => "Composer",
            Page::Updates => "Updates",
            Page::Help => "Help",
        }
    }
}

/// The shortcut that opens (and closes) Settings.
pub fn opener() -> egui::KeyboardShortcut {
    egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::Comma)
}

/// Settings while open (`ws.settings`); Esc, a click outside or Close shut it.
pub fn show(ctx: &egui::Context, ws: &mut Workspace, look: Look, updates: &Updates) -> Option<Request> {
    if ctx.input_mut(|i| i.consume_shortcut(&opener())) {
        ws.settings = if ws.settings.is_some() { None } else { Some(Page::default()) };
    }
    let page = ws.settings?;
    let p = colors();
    let screen = ctx.content_rect();
    let width = WIDTH.min(screen.width() - 60.0);
    let height = HEIGHT.min(screen.height() - 140.0).max(240.0);
    let mut request = None;
    let mut close = false;
    let frame = Frame::new().fill(p.panel).corner_radius(CornerRadius::same(16)).inner_margin(Margin::same(20));
    let modal = egui::Modal::new(egui::Id::unique("settings")).frame(frame).show(ctx, |ui| {
        ui.set_width(width);
        ui.horizontal(|ui| {
            ui.label(RichText::new("Settings").font(ty::title()).color(p.text));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let keys = ctx.format_shortcut(&opener());
                if widgets::icon_button(ui, crate::ui::style::icons::Icon::Close, &format!("Close ({keys} or Esc)")).clicked() {
                    close = true;
                }
            });
        });
        ui.add_space(12.0);
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                ui.set_width(NAV_WIDTH);
                ui.spacing_mut().item_spacing.y = 2.0;
                for each in Page::ALL {
                    if nav_item(ui, each.label(), each == page) {
                        ws.settings = Some(each);
                    }
                }
                ui.add_space(14.0);
                ui.horizontal(|ui| {
                    ui.add_space(10.0);
                    ui.label(RichText::new(format!("drift {}", crate::update::CURRENT)).font(ty::meta()).color(p.faint));
                });
            });
            let (line, _) = ui.allocate_exact_size(vec2(1.0, height), Sense::hover());
            ui.painter().vline(line.center().x, line.y_range(), Stroke::new(1.0, p.line));
            ui.add_space(10.0);
            ui.vertical(|ui| {
                egui::ScrollArea::vertical().id_salt(("settings", page as u8)).max_height(height).auto_shrink([false, false]).show(
                    ui,
                    |ui| {
                        ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
                        request = match page {
                            Page::Appearance => appearance(ui, look),
                            Page::Layout => layout(ui, ws.options),
                            Page::Notifications => notifications(ui, ws),
                            Page::Composer => composer(ui, ctx, ws.options),
                            Page::Updates => updates_page(ui, updates),
                            Page::Help => {
                                help(ui, ctx, ws);
                                None
                            }
                        };
                    },
                );
            });
        });
    });
    if close || modal.should_close() {
        ws.settings = None;
    }
    request
}

/// A page's name in the list on the left; true when clicked.
fn nav_item(ui: &mut Ui, label: &str, on: bool) -> bool {
    let p = colors();
    let (row, response) = ui.allocate_exact_size(vec2(ui.available_width(), 32.0), Sense::click());
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    widgets::control(ui, &response, egui::Role::Tab, label);
    if on {
        ui.painter().rect_filled(row, CornerRadius::same(8), p.selected);
    } else if response.hovered() {
        ui.painter().rect_filled(row, CornerRadius::same(8), p.hover);
    }
    let font = if on { ty::small_strong() } else { ty::small() };
    ui.painter().text(pos2(row.left() + 10.0, row.center().y), Align2::LEFT_CENTER, label, font, if on { p.text } else { p.dim });
    response.clicked()
}

/// A line of explanation under a heading.
fn note(ui: &mut Ui, text: &str) {
    ui.horizontal(|ui| {
        ui.add_space(10.0);
        ui.add(egui::Label::new(RichText::new(text).font(ty::meta()).color(colors().faint)).wrap());
    });
    ui.add_space(6.0);
}

/// Light, dark or as the system is; the theme for each; the accent; the size.
fn appearance(ui: &mut Ui, look: Look) -> Option<Request> {
    let p = colors();
    let mut request = None;
    menu_label(ui, "Mode");
    let modes = [(Appearance::System, "System"), (Appearance::Light, "Light"), (Appearance::Dark, "Dark")];
    if let Some(mode) = segments(ui, "appearance", &modes, look.mode) {
        request = Some(Request::Look(Look { mode, ..look }));
    }
    ui.add_space(14.0);
    menu_label(ui, "Theme");
    note(ui, "One for light and one for dark; the ringed one is in use.");
    let dark_now = ui.visuals().dark_mode;
    ui.horizontal(|ui| {
        ui.add_space(6.0);
        ui.spacing_mut().item_spacing.x = 8.0;
        for theme in Theme::ALL.iter().copied() {
            // The one in use, and the other kind's pick (used when drift is that kind).
            let picked = theme == if theme.dark() { look.dark } else { look.light };
            let in_use = picked && theme.dark() == dark_now;
            if theme_swatch(ui, theme, picked, in_use).clicked() {
                // Picking the other kind's theme switches to it too.
                let mut to = if theme.dark() { Look { dark: theme, ..look } } else { Look { light: theme, ..look } };
                if theme.dark() != dark_now {
                    to.mode = if theme.dark() { Appearance::Dark } else { Appearance::Light };
                }
                request = Some(Request::Look(to));
            }
        }
    });
    ui.add_space(16.0);
    menu_label(ui, "Accent");
    ui.horizontal(|ui| {
        ui.add_space(8.0);
        ui.spacing_mut().item_spacing.x = 10.0;
        for accent in Accent::ALL.iter().copied() {
            let (dot, response) = ui.allocate_exact_size(vec2(24.0, 24.0), Sense::click());
            let response = response.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(accent.label());
            widgets::control(ui, &response, egui::Role::RadioButton, &format!("{} accent", accent.label()));
            ui.painter().circle_filled(dot.center(), 10.0, palette::accent_swatch(accent, dark_now));
            if accent == look.accent {
                ui.painter().circle_stroke(dot.center(), 12.0, Stroke::new(1.5, p.text));
            } else if response.hovered() {
                ui.painter().circle_stroke(dot.center(), 12.0, Stroke::new(1.0, p.border));
            }
            if response.clicked() {
                request = Some(Request::Look(Look { accent, ..look }));
            }
        }
    });
    ui.add_space(16.0);
    menu_label(ui, "Text size");
    let sizes: Vec<(u16, String)> = TEXT_SIZES.iter().map(|s| (*s, format!("{s}%"))).collect();
    let sizes: Vec<(u16, &str)> = sizes.iter().map(|(s, l)| (*s, l.as_str())).collect();
    if let Some(text_size) = segments(ui, "text-size", &sizes, look.text_size).filter(|s| *s != look.text_size) {
        request = Some(Request::Look(Look { text_size, ..look }));
    }
    ui.add_space(6.0);
    note(ui, "⌘+ and ⌘− change it too, ⌘0 sets it back.");
    request
}

/// The tabs in the top bar, and how the list looks.
fn layout(ui: &mut Ui, o: Options) -> Option<Request> {
    let mut to = o;
    menu_label(ui, "Tabs in the top bar");
    note(ui, "Chats always shows. A tab turned off is hidden, and its channels don't count on the Dock icon.");
    if switch_row(ui, "Activity", "Mentions, replies and reactions", o.activity_tab) {
        to.activity_tab = !o.activity_tab;
    }
    if switch_row(ui, "Teams", "Teams and their channels", o.teams_tab) {
        to.teams_tab = !o.teams_tab;
    }
    if switch_row(ui, "Calendar", "Today's and the coming meetings", o.calendar_tab) {
        to.calendar_tab = !o.calendar_tab;
    }
    ui.add_space(8.0);
    divider(ui);
    menu_label(ui, "List and messages");
    if switch_row(ui, "Message previews", "The newest message under each conversation's name", o.list_previews) {
        to.list_previews = !o.list_previews;
    }
    if switch_row(ui, "Compact", "Tighter rows in the list, smaller message bubbles", o.compact) {
        to.compact = !o.compact;
    }
    (to != o).then_some(Request::Options(to))
}

/// Whether, how and about what drift notifies.
fn notifications(ui: &mut Ui, ws: &Workspace) -> Option<Request> {
    let p = colors();
    let o = ws.options;
    let mut to = o;
    let mut request = None;
    menu_label(ui, "Notifications");
    if switch_row(ui, "Show notifications", "For new messages, when drift is behind", ws.notifications) {
        request = Some(Request::Notifications(!ws.notifications));
    }
    if let Some(until) = ws.paused_until.filter(|u| *u > crate::model::time::now_ms()) {
        let (row, response) = ui.allocate_exact_size(vec2(ui.available_width(), 34.0), Sense::click());
        let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
        if response.hovered() {
            ui.painter().rect_filled(row.shrink2(vec2(2.0, 2.0)), CornerRadius::same(8), p.hover);
        }
        let text = format!("Paused until {}", crate::model::time::clock(until));
        ui.painter().text(pos2(row.left() + 10.0, row.center().y), Align2::LEFT_CENTER, text, ty::small(), p.accent);
        ui.painter().text(pos2(row.right() - 10.0, row.center().y), Align2::RIGHT_CENTER, "Resume", ty::small_strong(), p.text);
        if response.clicked() {
            request = Some(Request::Pause(None));
        }
    }
    if switch_row(ui, "Sound", "drift's chime with each notification", o.sound) {
        to.sound = !o.sound;
    }
    if switch_row(ui, "Message text", "What was written, in the notification (off: only who and where)", o.notification_text) {
        to.notification_text = !o.notification_text;
    }
    ui.add_space(8.0);
    divider(ui);
    menu_label(ui, "Channels");
    let sub = "Each new post (off: only mentions and replies)";
    if switch_row(ui, "Posts in followed channels", sub, o.channel_posts) {
        to.channel_posts = !o.channel_posts;
    }
    #[cfg(target_os = "macos")]
    {
        ui.add_space(8.0);
        divider(ui);
        if link_row(ui, "Notification settings in macOS", "Banners or alerts, the badge, Focus") {
            let url = "x-apple.systempreferences:com.apple.Notifications-Settings.extension";
            if let Err(e) = std::process::Command::new("open").arg(url).spawn() {
                log_warn!("opening the notification settings: {e}");
            }
        }
    }
    request.or((to != o).then_some(Request::Options(to)))
}

/// How the composer sends, and what it does as one types.
fn composer(ui: &mut Ui, ctx: &egui::Context, o: Options) -> Option<Request> {
    let mut to = o;
    menu_label(ui, "Send with");
    let enter = ctx.format_shortcut(&egui::KeyboardShortcut::new(egui::Modifiers::NONE, egui::Key::Enter));
    let cmd_enter = ctx.format_shortcut(&egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::Enter));
    if let Some(enter_sends) = segments(ui, "send-with", &[(true, enter.as_str()), (false, cmd_enter.as_str())], o.enter_sends) {
        to.enter_sends = enter_sends;
    }
    ui.add_space(6.0);
    let shift_enter = ctx.format_shortcut(&egui::KeyboardShortcut::new(egui::Modifiers::SHIFT, egui::Key::Enter));
    note(ui, &if o.enter_sends { format!("{shift_enter} starts a new line.") } else { format!("{enter} starts a new line.") });
    ui.add_space(4.0);
    divider(ui);
    menu_label(ui, "As you type");
    if switch_row(ui, "Formatting hints", "The Markdown it takes, under the box", o.markdown_hints) {
        to.markdown_hints = !o.markdown_hints;
    }
    if switch_row(ui, "Emoticons to emoji", ":) and the like become emoji", o.emoticons) {
        to.emoticons = !o.emoticons;
    }
    (to != o).then_some(Request::Options(to))
}

/// This version, whether there's a newer one, and updating by itself.
fn updates_page(ui: &mut Ui, updates: &Updates) -> Option<Request> {
    let p = colors();
    let mut request = None;
    menu_label(ui, "Version");
    let (row, _) = ui.allocate_exact_size(vec2(ui.available_width(), 46.0), Sense::hover());
    ui.painter().text(
        pos2(row.left() + 10.0, row.top() + 15.0),
        Align2::LEFT_CENTER,
        format!("drift {}", crate::update::CURRENT),
        ty::small_strong(),
        p.text,
    );
    let color = match updates.notable() {
        Some(true) => p.accent,
        Some(false) => p.danger,
        None => p.faint,
    };
    ui.painter().text(pos2(row.left() + 10.0, row.top() + 32.0), Align2::LEFT_CENTER, updates.status(), ty::meta(), color);
    let (label, on_click, strong) = match updates.offer() {
        Some(offer) if offer.installs => ("Restart", Request::InstallUpdate, true),
        Some(_) => ("Download", Request::InstallUpdate, true),
        None => ("Check now", Request::CheckForUpdates, false),
    };
    let text_color = if strong { p.on_accent } else { p.text };
    let galley = ui.painter().layout_no_wrap(label.into(), ty::small_strong(), text_color);
    let button =
        Rect::from_min_size(pos2(row.right() - 10.0 - galley.size().x - 24.0, row.center().y - 14.0), vec2(galley.size().x + 24.0, 28.0));
    let click = ui.interact(button, ui.scope_id().with("update"), Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
    let fill = if strong { p.accent_fill } else { p.surface };
    ui.painter().rect_filled(button, CornerRadius::same(8), if click.hovered() { p.hover.lerp_to_gamma(fill, 0.5) } else { fill });
    ui.painter().galley(button.center() - galley.size() / 2.0, galley, text_color);
    if click.clicked() && !updates.checking() {
        request = Some(on_click);
    }
    ui.add_space(6.0);
    divider(ui);
    if switch_row(ui, "Update automatically", "Installs new versions when drift restarts", updates.auto) {
        request = Some(Request::AutoUpdate(!updates.auto));
    }
    request
}

/// The keyboard shortcuts, and the log for when something goes wrong.
fn help(ui: &mut Ui, ctx: &egui::Context, ws: &mut Workspace) {
    menu_label(ui, "Help");
    let keys = ctx.format_shortcut(&super::shortcuts::opener());
    if link_row(ui, "Keyboard shortcuts", &keys) {
        ws.settings = None;
        ws.shortcuts = true;
    }
    if link_row(ui, "Show log file", "What drift did, for troubleshooting") {
        reveal_log();
    }
    if link_row(ui, "drift on GitHub", "Source, releases and issues") {
        ctx.open_url(egui::OpenUrl::new_tab("https://github.com/domknez/drift"));
    }
}

/// A row that does something when clicked: its name, and a detail on the right.
fn link_row(ui: &mut Ui, name: &str, detail: &str) -> bool {
    let p = colors();
    let (row, response) = ui.allocate_exact_size(vec2(ui.available_width(), 38.0), Sense::click());
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    widgets::control(ui, &response, egui::Role::Button, name);
    if response.hovered() {
        ui.painter().rect_filled(row.shrink2(vec2(2.0, 2.0)), CornerRadius::same(8), p.hover);
    }
    ui.painter().text(pos2(row.left() + 10.0, row.center().y), Align2::LEFT_CENTER, name, ty::small_strong(), p.text);
    ui.painter().text(pos2(row.right() - 10.0, row.center().y), Align2::RIGHT_CENTER, detail, ty::meta(), p.faint);
    response.clicked()
}

/// Show the log file in the file manager.
fn reveal_log() {
    let Some(path) = crate::log::file_path() else { return };
    let shown = if cfg!(target_os = "macos") {
        std::process::Command::new("open").arg("-R").arg(&path).spawn()
    } else if cfg!(windows) {
        std::process::Command::new("explorer").arg(format!("/select,{}", path.display())).spawn()
    } else {
        std::process::Command::new("xdg-open").arg(path.parent().unwrap_or(&path)).spawn()
    };
    if let Err(e) = shown {
        log_warn!("showing the log: {e}");
    }
}
