//! Tiny `key=value` settings file in the platform config directory, read and
//! written as typed [`Prefs`].

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// drift's folder in the platform config directory.
pub fn dir() -> Option<PathBuf> {
    let env = |k| std::env::var_os(k).map(PathBuf::from);
    let base = if cfg!(target_os = "macos") {
        env("HOME").map(|h| h.join("Library/Application Support"))
    } else if cfg!(windows) {
        env("APPDATA")
    } else {
        env("XDG_CONFIG_HOME").or_else(|| env("HOME").map(|h| h.join(".config")))
    }?;
    Some(base.join("drift"))
}

/// Messages left half-written, by conversation, kept in `drafts.json` next to the
/// settings so they're back after a restart.
pub fn load_drafts() -> std::collections::HashMap<String, String> {
    dir().map(|d| read_drafts(&d.join("drafts.json"))).unwrap_or_default()
}

pub fn save_drafts(drafts: &std::collections::HashMap<String, String>) {
    if let Some(dir) = dir() {
        write_drafts(&dir.join("drafts.json"), drafts);
    }
}

fn read_drafts(path: &Path) -> std::collections::HashMap<String, String> {
    std::fs::read(path).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

/// Write `drafts` to `path`, or remove the file when there are none.
fn write_drafts(path: &Path, drafts: &std::collections::HashMap<String, String>) {
    let result = if drafts.is_empty() {
        match std::fs::remove_file(path) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        }
    } else {
        // Written aside and renamed over: a crash mid-write leaves the old file whole.
        let dir = path.parent().unwrap_or(Path::new("."));
        let aside = path.with_extension("json.new");
        std::fs::create_dir_all(dir)
            .and_then(|()| std::fs::write(&aside, serde_json::to_vec(drafts).unwrap_or_default()))
            .and_then(|()| std::fs::rename(&aside, path))
    };
    if let Err(e) = result {
        log_warn!("could not save drafts to {}: {e}", path.display());
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Appearance {
    System,
    Dark,
    Light,
}

impl Appearance {
    pub const ALL: [Appearance; 3] = [Appearance::System, Appearance::Dark, Appearance::Light];

    pub fn key(self) -> &'static str {
        match self {
            Appearance::System => "system",
            Appearance::Dark => "dark",
            Appearance::Light => "light",
        }
    }

    fn from_key(k: &str) -> Option<Appearance> {
        Appearance::ALL.into_iter().find(|a| a.key() == k)
    }
}

/// A choice kept by its key in the settings file.
macro_rules! choice {
    ($(#[$doc:meta])* $name:ident { $($(#[$vdoc:meta])* $variant:ident => $key:literal),+ $(,)? }) => {
        $(#[$doc])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub enum $name {
            $($(#[$vdoc])* $variant),+
        }

        impl $name {
            pub const ALL: &'static [$name] = &[$($name::$variant),+];

            pub fn key(self) -> &'static str {
                match self {
                    $($name::$variant => $key),+
                }
            }

            fn from_key(k: &str) -> Option<$name> {
                $name::ALL.iter().copied().find(|v| v.key() == k)
            }
        }
    };
}

choice!(
    /// The greys drift is drawn in: three dark themes and two light ones.
    Theme {
        /// Deep ink, nearly black.
        Midnight => "midnight",
        /// A softer dark grey.
        Dim => "dim",
        /// A bluish slate, between dark and light.
        Slate => "slate",
        /// Warm paper.
        Paper => "paper",
        /// A neutral, cool light grey.
        Cool => "cool",
    }
);

impl Theme {
    pub fn label(self) -> &'static str {
        match self {
            Theme::Midnight => "Midnight",
            Theme::Dim => "Dim",
            Theme::Slate => "Slate",
            Theme::Paper => "Paper",
            Theme::Cool => "Cool",
        }
    }

    pub fn dark(self) -> bool {
        matches!(self, Theme::Midnight | Theme::Dim | Theme::Slate)
    }
}

choice!(
    /// The colour of buttons, links and what's picked.
    Accent {
        Violet => "violet",
        Indigo => "indigo",
        Blue => "blue",
        Teal => "teal",
        Green => "green",
        Orange => "orange",
        Pink => "pink",
    }
);

impl Accent {
    pub fn label(self) -> &'static str {
        match self {
            Accent::Violet => "Violet",
            Accent::Indigo => "Indigo",
            Accent::Blue => "Blue",
            Accent::Teal => "Teal",
            Accent::Green => "Green",
            Accent::Orange => "Orange",
            Accent::Pink => "Pink",
        }
    }
}

/// How big everything is drawn, in percent.
pub const TEXT_SIZES: [u16; 4] = [90, 100, 110, 125];

/// How drift looks: light, dark or as the system is, the theme for each, the
/// accent and the size.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Look {
    pub mode: Appearance,
    pub light: Theme,
    pub dark: Theme,
    pub accent: Accent,
    pub text_size: u16,
}

impl Look {
    /// The next size up (`step` 1) or down (-1), staying within [`TEXT_SIZES`].
    pub fn resized(self, step: i32) -> Look {
        let at = TEXT_SIZES.iter().position(|s| *s == self.text_size).unwrap_or(1) as i32;
        let to = (at + step).clamp(0, TEXT_SIZES.len() as i32 - 1) as usize;
        Look { text_size: TEXT_SIZES[to], ..self }
    }
}

/// The user's preferences, as kept in the settings file. Change them with
/// [`Prefs::update`], which saves what changed.
#[derive(Clone, Debug, PartialEq)]
pub struct Prefs {
    pub theme: Appearance,
    /// The theme used when drift is light, and the one when it's dark.
    pub light_theme: Theme,
    pub dark_theme: Theme,
    pub accent: Accent,
    /// How big everything is drawn, in percent (one of [`TEXT_SIZES`]).
    pub text_size: u16,
    /// The conversation open last (its id).
    pub conversation: Option<String>,
    /// A notification for each new message.
    pub notifications: bool,
    /// Conversations muted in drift (their ids): no notifications, not counted unread.
    pub muted: Vec<String>,
    /// Channels followed in drift (their ids): a notification for every new post.
    pub following: Vec<String>,
    /// The sidebar sections folded away, by key; None for drift's defaults.
    pub folded: Option<Vec<String>>,
    /// Conversations hidden in drift, each with when (Unix ms): they're listed again
    /// once something newer comes.
    pub hidden: Vec<(String, i64)>,
    /// Emoji picked lately (their keys), the latest first.
    pub recent_emoji: Vec<String>,
    /// Check for new versions now and then, and download them to install on restart.
    pub auto_update: bool,
    /// The settings file changes are saved to; None keeps them in memory.
    file: Option<PathBuf>,
}

impl Prefs {
    /// The saved preferences (defaults for anything missing or unreadable).
    pub fn load() -> Prefs {
        let file = dir().map(|d| d.join("settings.conf"));
        Prefs { file: file.clone(), ..Prefs::parse(&file.as_deref().map(read).unwrap_or_default()) }
    }

    /// Preferences from a settings file's text, not tied to the file.
    pub fn parse(text: &str) -> Prefs {
        let saved = entries(text);
        let get = |k: &str| saved.get(k).map(String::as_str).filter(|v| !v.is_empty());
        Prefs {
            theme: get("theme").and_then(Appearance::from_key).unwrap_or(Appearance::System),
            light_theme: get("light_theme").and_then(Theme::from_key).filter(|t| !t.dark()).unwrap_or(Theme::Paper),
            dark_theme: get("dark_theme").and_then(Theme::from_key).filter(|t| t.dark()).unwrap_or(Theme::Midnight),
            accent: get("accent").and_then(Accent::from_key).unwrap_or(Accent::Violet),
            text_size: get("text_size").and_then(|v| v.parse().ok()).filter(|s| TEXT_SIZES.contains(s)).unwrap_or(100),
            conversation: get("conversation").map(str::to_string),
            notifications: get("notifications") != Some("false"),
            auto_update: get("auto_update") != Some("false"),
            recent_emoji: get("recent_emoji")
                .map(|v| v.split(',').filter(|s| !s.is_empty()).map(str::to_string).collect())
                .unwrap_or_default(),
            hidden: get("hidden")
                .map(|v| {
                    v.split(',')
                        .filter_map(|e| e.rsplit_once('|'))
                        .filter_map(|(id, at)| Some((id.to_string(), at.parse().ok()?)))
                        .collect()
                })
                .unwrap_or_default(),
            // "-" for none folded (an empty value reads as not set).
            folded: saved.get("folded").map(|v| v.split(',').filter(|s| !s.is_empty() && *s != "-").map(str::to_string).collect()),
            muted: get("muted").map(|v| v.split(',').filter(|s| !s.is_empty()).map(str::to_string).collect()).unwrap_or_default(),
            following: get("following").map(|v| v.split(',').filter(|s| !s.is_empty()).map(str::to_string).collect()).unwrap_or_default(),
            file: None,
        }
    }

    pub fn look(&self) -> Look {
        Look { mode: self.theme, light: self.light_theme, dark: self.dark_theme, accent: self.accent, text_size: self.text_size }
    }

    pub fn set_look(&mut self, look: Look) {
        self.update(|p| {
            p.theme = look.mode;
            p.light_theme = look.light;
            p.dark_theme = look.dark;
            p.accent = look.accent;
            p.text_size = look.text_size;
        });
    }

    /// Change preferences and save the ones that changed. Keys this version
    /// doesn't know stay in the file as they were.
    pub fn update(&mut self, change: impl FnOnce(&mut Prefs)) {
        let before = self.values();
        change(self);
        let changed: Vec<_> = self.values().into_iter().filter(|v| !before.contains(v)).collect();
        if changed.is_empty() {
            return;
        }
        let Some(p) = &self.file else { return };
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Err(e) = std::fs::write(p, merge(&read(p), &changed)) {
            log_warn!("could not save settings to {}: {e}", p.display());
        }
    }

    fn values(&self) -> [(&'static str, String); 13] {
        [
            ("accent", self.accent.key().into()),
            ("auto_update", if self.auto_update { "true" } else { "false" }.into()),
            ("conversation", self.conversation.clone().unwrap_or_default()),
            ("dark_theme", self.dark_theme.key().into()),
            (
                "folded",
                match &self.folded {
                    Some(f) if f.is_empty() => "-".into(),
                    Some(f) => f.join(","),
                    None => String::new(),
                },
            ),
            ("following", self.following.join(",")),
            ("hidden", self.hidden.iter().map(|(id, at)| format!("{id}|{at}")).collect::<Vec<_>>().join(",")),
            ("light_theme", self.light_theme.key().into()),
            ("muted", self.muted.join(",")),
            ("notifications", if self.notifications { "true" } else { "false" }.into()),
            ("recent_emoji", self.recent_emoji.join(",")),
            ("text_size", self.text_size.to_string()),
            ("theme", self.theme.key().into()),
        ]
    }
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_default()
}

fn entries(text: &str) -> BTreeMap<String, String> {
    text.lines().filter_map(|l| l.split_once('=')).map(|(k, v)| (k.trim().to_string(), v.trim().to_string())).collect()
}

fn merge(text: &str, changes: &[(&str, String)]) -> String {
    let mut all = entries(text);
    for (k, v) in changes {
        all.insert(k.to_string(), v.clone());
    }
    all.iter().map(|(k, v)| format!("{k}={v}\n")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAVED: &str = "accent=teal\nauto_update=false\nconversation=19:abc@thread.v2\ndark_theme=slate\nfolded=meetings,team:19:t@thread.tacv2\nfollowing=19:prs@thread.tacv2\nhidden=19:old@thread.v2|1793000000000\nlight_theme=cool\nmuted=19:x@thread.v2,19:y\nnotifications=false\nrecent_emoji=yes,wave;0-weu-d1-abc\ntext_size=110\ntheme=light\n";

    #[test]
    fn reads_a_saved_file() {
        let p = Prefs::parse(SAVED);
        assert_eq!(p.theme, Appearance::Light);
        assert_eq!(p.conversation.as_deref(), Some("19:abc@thread.v2"));
        assert!(!p.auto_update);
        assert_eq!(p.following, ["19:prs@thread.tacv2"]);
        assert_eq!((p.light_theme, p.dark_theme, p.accent, p.text_size), (Theme::Cool, Theme::Slate, Accent::Teal, 110));
    }

    #[test]
    fn missing_or_odd_values_get_defaults() {
        let p = Prefs::parse("theme=blue\nconversation=\n");
        assert_eq!((p.theme, p.conversation, p.auto_update), (Appearance::System, None, true));
    }

    #[test]
    fn round_trips_through_the_file() {
        assert_eq!(merge("", &Prefs::parse(SAVED).values()), SAVED);
    }

    #[test]
    fn drafts_survive_a_restart() {
        let dir = std::env::temp_dir().join(format!("drift-drafts-test-{}", std::process::id()));
        let path = dir.join("drafts.json");
        let drafts = std::collections::HashMap::from([("19:a@thread.v2".to_string(), "half\nwritten, with \"quotes\"".to_string())]);
        write_drafts(&path, &drafts);
        assert_eq!(read_drafts(&path), drafts);
        write_drafts(&path, &Default::default());
        assert!(!path.exists());
        // A broken file reads as no drafts, and is replaced whole by the next save.
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(&path, b"{\"19:a@thread.v2\": \"cut of").unwrap();
        assert!(read_drafts(&path).is_empty());
        write_drafts(&path, &drafts);
        assert_eq!(read_drafts(&path), drafts);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn keeps_keys_it_does_not_know() {
        assert_eq!(merge("future=1\ntheme=dark\n", &[("theme", "light".into())]), "future=1\ntheme=light\n");
    }
}
