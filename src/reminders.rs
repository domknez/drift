//! Reminders the user set on messages ("Remind me"), kept in `reminders.json` next
//! to the settings. drift reminds of them itself: one that came due while it was
//! closed is reminded of when it next runs.

use serde_json::{Value, json};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reminder {
    /// When (Unix ms).
    pub at: i64,
    pub conversation: String,
    pub message: String,
    /// In a channel, the post whose thread the message is in.
    pub root: Option<String>,
    /// Who wrote it, where (None for a one-on-one chat), and what it says, for the
    /// notification.
    pub author: String,
    pub place: Option<String>,
    pub text: String,
}

/// The reminders set, soonest first.
#[derive(Default)]
pub struct Reminders {
    list: Vec<Reminder>,
    /// The file changes are saved to; None keeps them in memory.
    file: Option<PathBuf>,
}

impl Reminders {
    /// The saved reminders.
    pub fn load() -> Reminders {
        let file = crate::settings::dir().map(|d| d.join("reminders.json"));
        Reminders { list: file.as_deref().map(read).unwrap_or_default(), file }
    }

    /// Remind of a message then (instead of when it was to be before).
    pub fn set(&mut self, reminder: Reminder) {
        self.list.retain(|r| r.message != reminder.message);
        self.list.push(reminder);
        self.list.sort_by_key(|r| r.at);
        self.save();
    }

    pub fn cancel(&mut self, message: &str) {
        let before = self.list.len();
        self.list.retain(|r| r.message != message);
        if self.list.len() != before {
            self.save();
        }
    }

    /// The reminders due by `now`, taken out.
    pub fn due(&mut self, now: i64) -> Vec<Reminder> {
        let (due, later): (Vec<_>, Vec<_>) = std::mem::take(&mut self.list).into_iter().partition(|r| r.at <= now);
        self.list = later;
        if !due.is_empty() {
            self.save();
        }
        due
    }

    /// When the next one is due.
    pub fn next(&self) -> Option<i64> {
        self.list.first().map(|r| r.at)
    }

    /// When each message of a conversation is to be reminded of, by its id.
    pub fn of(&self, conversation: &str) -> HashMap<String, i64> {
        self.list.iter().filter(|r| r.conversation == conversation).map(|r| (r.message.clone(), r.at)).collect()
    }

    fn save(&self) {
        let Some(path) = &self.file else { return };
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Err(e) = write(path, &self.list) {
            log_warn!("could not save reminders to {}: {e}", path.display());
        }
    }
}

fn read(path: &Path) -> Vec<Reminder> {
    let Some(Value::Array(items)) = std::fs::read(path).ok().and_then(|b| serde_json::from_slice(&b).ok()) else { return Vec::new() };
    let mut list: Vec<Reminder> = items
        .iter()
        .filter_map(|v| {
            let s = |k: &str| v.get(k).and_then(Value::as_str).map(str::to_string);
            Some(Reminder {
                at: v.get("at")?.as_i64()?,
                conversation: s("conversation")?,
                message: s("message")?,
                root: s("root"),
                author: s("author").unwrap_or_default(),
                place: s("place"),
                text: s("text").unwrap_or_default(),
            })
        })
        .collect();
    list.sort_by_key(|r| r.at);
    list
}

/// Write the reminders to `path`, or remove the file when there are none.
fn write(path: &Path, list: &[Reminder]) -> std::io::Result<()> {
    if list.is_empty() {
        return match std::fs::remove_file(path) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        };
    }
    let items: Vec<Value> = list
        .iter()
        .map(|r| {
            json!({
                "at": r.at, "conversation": r.conversation, "message": r.message, "root": r.root,
                "author": r.author, "place": r.place, "text": r.text,
            })
        })
        .collect();
    std::fs::write(path, serde_json::to_vec_pretty(&items).unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reminder(message: &str, at: i64) -> Reminder {
        Reminder {
            at,
            conversation: "19:c".into(),
            message: message.into(),
            root: None,
            author: "Maya".into(),
            place: None,
            text: "hi".into(),
        }
    }

    #[test]
    fn due_ones_are_taken_out_and_the_rest_kept_in_order() {
        let mut r = Reminders::default();
        r.set(reminder("b", 300));
        r.set(reminder("a", 100));
        r.set(reminder("c", 200));
        // Set again: moved, not added.
        r.set(reminder("c", 400));
        assert_eq!(r.next(), Some(100));
        assert_eq!(r.due(250).iter().map(|r| r.message.as_str()).collect::<Vec<_>>(), ["a"]);
        assert_eq!(r.of("19:c"), HashMap::from([("b".to_string(), 300), ("c".to_string(), 400)]));
        r.cancel("b");
        assert_eq!(r.next(), Some(400));
    }

    #[test]
    fn they_last_in_their_file() {
        let dir = std::env::temp_dir().join(format!("drift-reminders-{}", std::process::id()));
        let path = dir.join("reminders.json");
        let mut r = Reminders { list: Vec::new(), file: Some(path.clone()) };
        let mut one = reminder("m", 5);
        one.root = Some("p".into());
        one.place = Some("General".into());
        r.set(one.clone());
        assert_eq!(read(&path), vec![one]);
        r.cancel("m");
        assert!(!path.exists());
        let _ = std::fs::remove_dir_all(dir);
    }
}
