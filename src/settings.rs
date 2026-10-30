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

/// The user's preferences, as kept in the settings file. Change them with
/// [`Prefs::update`], which saves what changed.
#[derive(Clone, Debug, PartialEq)]
pub struct Prefs {
    pub theme: Appearance,
    /// The conversation open last (its id).
    pub conversation: Option<String>,
    /// A notification for each new message.
    pub notifications: bool,
    /// Conversations muted in drift (their ids): no notifications, not counted unread.
    pub muted: Vec<String>,
    /// The sidebar sections folded away, by key; None for drift's defaults.
    pub folded: Option<Vec<String>>,
    /// Conversations hidden in drift, each with when (Unix ms): they're listed again
    /// once something newer comes.
    pub hidden: Vec<(String, i64)>,
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
            conversation: get("conversation").map(str::to_string),
            notifications: get("notifications") != Some("false"),
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
            file: None,
        }
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

    fn values(&self) -> [(&'static str, String); 6] {
        [
            ("conversation", self.conversation.clone().unwrap_or_default()),
            (
                "folded",
                match &self.folded {
                    Some(f) if f.is_empty() => "-".into(),
                    Some(f) => f.join(","),
                    None => String::new(),
                },
            ),
            ("hidden", self.hidden.iter().map(|(id, at)| format!("{id}|{at}")).collect::<Vec<_>>().join(",")),
            ("muted", self.muted.join(",")),
            ("notifications", if self.notifications { "true" } else { "false" }.into()),
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

    const SAVED: &str = "conversation=19:abc@thread.v2\nfolded=meetings,team:19:t@thread.tacv2\nhidden=19:old@thread.v2|1793000000000\nmuted=19:x@thread.v2,19:y\nnotifications=false\ntheme=light\n";

    #[test]
    fn reads_a_saved_file() {
        let p = Prefs::parse(SAVED);
        assert_eq!(p.theme, Appearance::Light);
        assert_eq!(p.conversation.as_deref(), Some("19:abc@thread.v2"));
    }

    #[test]
    fn missing_or_odd_values_get_defaults() {
        let p = Prefs::parse("theme=blue\nconversation=\n");
        assert_eq!((p.theme, p.conversation), (Appearance::System, None));
    }

    #[test]
    fn round_trips_through_the_file() {
        assert_eq!(merge("", &Prefs::parse(SAVED).values()), SAVED);
    }

    #[test]
    fn keeps_keys_it_does_not_know() {
        assert_eq!(merge("future=1\ntheme=dark\n", &[("theme", "light".into())]), "future=1\ntheme=light\n");
    }
}
