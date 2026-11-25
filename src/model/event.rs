//! What happened to a conversation rather than what was said in it: people added
//! or leaving, a new name or picture. Teams sends these as markup naming people by
//! id; they're told in words as its own client does ("Markus added Joss to the chat
//! and shared chat history from the past 3 days.").

use std::collections::HashMap;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum What {
    Added,
    Removed,
    Left,
    Joined,
    Renamed(String),
    Picture,
    /// A meeting started in the conversation (by `by`).
    MeetingStarted,
    /// It ended, after so many seconds.
    MeetingEnded(Option<i64>),
}

/// How much of what was said before someone added may see.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum History {
    All,
    Days(i64),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Event {
    pub what: What,
    /// Who did it, by id, with the name the event gives (if any).
    pub by: Option<(String, Option<String>)>,
    /// Whom it was done to.
    pub whom: Vec<(String, Option<String>)>,
    pub history: Option<History>,
}

impl Event {
    /// An event from a message's type and content; None for messages, and for
    /// events nobody needs told of.
    pub fn parse(kind: &str, content: &str) -> Option<Event> {
        // A meeting started or ended: who started it is the message's sender.
        if kind.starts_with("Event/Call") {
            let ended = content.contains("<ended/>") || content.contains("type=\"ended\"");
            let what = if ended {
                What::MeetingEnded(tags(content, "duration").into_iter().find_map(|d| d.parse().ok()))
            } else {
                What::MeetingStarted
            };
            return Some(Event { what, by: None, whom: Vec::new(), history: None });
        }
        let what = kind.strip_prefix("ThreadActivity/")?;
        let first = |tag: &str| tags(content, tag).into_iter().next();
        let what = match what {
            "AddMember" => What::Added,
            "DeleteMember" => What::Removed,
            "MemberJoined" => What::Joined,
            "MemberLeft" => What::Left,
            "TopicUpdate" => What::Renamed(super::text::flatten(&super::text::from_html(&first("value").unwrap_or_default()))),
            "PictureUpdate" => What::Picture,
            _ => return None,
        };
        let by = first("initiator").map(|id| {
            let name =
                tags(content, "detailedinitiatorinfo").into_iter().next().and_then(|info| tags(&info, "friendlyName").into_iter().next());
            (id, name.filter(|n| !n.is_empty()))
        });
        // Each target, with its name where the details give one.
        let details: HashMap<String, String> = tags(content, "detailedtargetinfo")
            .iter()
            .filter_map(|info| {
                let id = tags(info, "id").into_iter().next()?;
                let name = tags(info, "friendlyName").into_iter().next().filter(|n| !n.is_empty())?;
                Some((id, name))
            })
            .collect();
        let whom: Vec<(String, Option<String>)> = tags(content, "target")
            .into_iter()
            .map(|id| {
                let name = details.get(&id).cloned();
                (id, name)
            })
            .collect();
        let history = (what == What::Added).then(|| {
            let at: i64 = first("eventtime")?.parse().ok()?;
            let shared: i64 = tags(content, "sharehistorytime").into_iter().next()?.parse().ok()?;
            match shared {
                ..0 => None,
                0 => Some(History::All),
                from => Some(History::Days(((at - from) as f64 / 86_400_000.0).round().max(1.0) as i64)),
            }
        });
        // Whoever adds others is listed among them at a meeting chat's start; Teams
        // names only the others.
        let mut whom = whom;
        if let Some((id, _)) = &by
            && whom.len() > 1
        {
            whom.retain(|(w, _)| w != id);
        }
        // Nobody named: nothing to tell.
        if whom.is_empty() && (by.is_none() || matches!(what, What::Added | What::Removed)) {
            return None;
        }
        // Someone removing themselves left.
        let what = match what {
            What::Removed if whom.len() == 1 && by.as_ref().is_some_and(|(id, _)| *id == whom[0].0) => What::Left,
            what => what,
        };
        Some(Event { what, by, whom, history: history.flatten() })
    }

    /// Everyone it names, by id.
    pub fn ids(&self) -> impl Iterator<Item = &String> {
        self.by.iter().chain(&self.whom).map(|(id, _)| id)
    }

    /// Give the people it doesn't name a name from `names`, by id.
    pub fn name(&mut self, names: &HashMap<String, String>) {
        for (id, name) in self.by.iter_mut().chain(&mut self.whom) {
            if name.is_none() {
                *name = names.get(id).filter(|n| !n.trim().is_empty()).cloned();
            }
        }
    }

    /// The event in words: names by id from `names` (the user as "You", under
    /// `me`), else as the event gives them; `place` is what it happened to
    /// ("chat", "team", "meeting": people join a meeting's chat, the meeting is named).
    pub fn told(&self, names: &HashMap<String, String>, me: &str, place: &str) -> String {
        let name = |(id, given): &(String, Option<String>)| {
            if !me.is_empty() && id == me {
                "You".to_string()
            } else {
                names.get(id).filter(|n| !n.trim().is_empty()).cloned().or_else(|| given.clone()).unwrap_or_else(|| "Someone".into())
            }
        };
        let by = self.by.as_ref().map_or_else(|| "Someone".to_string(), name);
        let whom = list(&self.whom.iter().map(name).collect::<Vec<_>>());
        let meeting = place == "meeting";
        let named = place;
        let place = if meeting { "chat" } else { place };
        match &self.what {
            What::Added => {
                let shared = match self.history {
                    Some(History::All) => " and shared all chat history".to_string(),
                    Some(History::Days(1)) => " and shared chat history from the past day".to_string(),
                    Some(History::Days(n)) => format!(" and shared chat history from the past {n} days"),
                    None => String::new(),
                };
                format!("{by} added {whom} to the {place}{shared}.")
            }
            What::Removed => format!("{by} removed {whom} from the {place}."),
            What::Left => format!("{} left the {place}.", if self.whom.is_empty() { by } else { whom }),
            What::Joined => format!("{} joined the {place}.", if self.whom.is_empty() { by } else { whom }),
            What::Renamed(to) if to.is_empty() => format!("{by} renamed the {named}."),
            What::Renamed(to) if meeting => format!("{by} named the meeting {to}."),
            What::Renamed(to) => format!("{by} renamed the {named} to {to}."),
            What::Picture => format!("{by} changed the {named}'s picture."),
            What::MeetingStarted => format!("{by} started a meeting."),
            What::MeetingEnded(Some(seconds)) => format!("Meeting ended: {}", lasted(*seconds)),
            What::MeetingEnded(None) => "Meeting ended.".into(),
        }
    }
}

/// How long, as Teams says it: "42s", "15m 3s", "1h 2m 3s".
fn lasted(seconds: i64) -> String {
    let (h, m, s) = (seconds / 3600, seconds / 60 % 60, seconds % 60);
    match (h, m) {
        (0, 0) => format!("{s}s"),
        (0, m) => format!("{m}m {s}s"),
        (h, m) => format!("{h}h {m}m {s}s"),
    }
}

/// "A", "A and B", "A, B and C"; "someone" for nobody.
fn list(names: &[String]) -> String {
    match names {
        [] => "someone".into(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

/// The text inside each `<tag>…</tag>` in `content`, trimmed.
fn tags(content: &str, tag: &str) -> Vec<String> {
    let (open, close) = (format!("<{tag}>"), format!("</{tag}>"));
    content.split(open.as_str()).skip(1).filter_map(|rest| rest.split_once(close.as_str()).map(|(v, _)| v.trim().to_string())).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const ADDED: &str = "<addmember><eventtime>1795511154006</eventtime><initiator>8:orgid:markus</initiator><rosterVersion>1</rosterVersion><target>8:orgid:joss</target><detailedtargetinfo><id>8:orgid:joss</id><sharehistorytime>1795251953213</sharehistorytime></detailedtargetinfo></addmember>";

    #[test]
    fn tells_who_added_whom_and_how_much_they_see() {
        let e = Event::parse("ThreadActivity/AddMember", ADDED).unwrap();
        assert_eq!(e.history, Some(History::Days(3)));
        let names = HashMap::from([
            ("8:orgid:markus".to_string(), "Markus Guerster".to_string()),
            ("8:orgid:joss".to_string(), "Joss de la Vega".to_string()),
        ]);
        assert_eq!(
            e.told(&names, "8:orgid:me", "chat"),
            "Markus Guerster added Joss de la Vega to the chat and shared chat history from the past 3 days."
        );
        // Not known: the name the event gives, else "Someone"; the user is "You".
        assert_eq!(
            e.told(&HashMap::new(), "8:orgid:markus", "chat"),
            "You added Someone to the chat and shared chat history from the past 3 days."
        );
    }

    #[test]
    fn names_given_in_the_event_and_lists_of_people() {
        let content = "<addmember><initiator>8:orgid:a</initiator><detailedinitiatorinfo><friendlyName>Ana</friendlyName></detailedinitiatorinfo><target>8:orgid:b</target><target>8:orgid:c</target><target>8:orgid:d</target><detailedtargetinfo><id>8:orgid:b</id><friendlyName>Bo</friendlyName></detailedtargetinfo><detailedtargetinfo><id>8:orgid:c</id><friendlyName>Cy</friendlyName><sharehistorytime>0</sharehistorytime></detailedtargetinfo></addmember>";
        let e = Event::parse("ThreadActivity/AddMember", content).unwrap();
        assert_eq!(e.told(&HashMap::new(), "", "team"), "Ana added Bo, Cy and Someone to the team.");
    }

    #[test]
    fn leaving_removing_renaming() {
        let left = Event::parse(
            "ThreadActivity/DeleteMember",
            "<deletemember><initiator>8:orgid:b</initiator><target>8:orgid:b</target></deletemember>",
        )
        .unwrap();
        let names = HashMap::from([("8:orgid:a".to_string(), "Ana".to_string()), ("8:orgid:b".to_string(), "Bo".to_string())]);
        assert_eq!(left.told(&names, "", "chat"), "Bo left the chat.");
        let removed = Event::parse(
            "ThreadActivity/DeleteMember",
            "<deletemember><initiator>8:orgid:a</initiator><target>8:orgid:b</target></deletemember>",
        )
        .unwrap();
        assert_eq!(removed.told(&names, "", "chat"), "Ana removed Bo from the chat.");
        let renamed = Event::parse(
            "ThreadActivity/TopicUpdate",
            "<topicupdate><initiator>8:orgid:a</initiator><value>Launch &amp; co</value></topicupdate>",
        )
        .unwrap();
        assert_eq!(renamed.told(&names, "", "chat"), "Ana renamed the chat to Launch & co.");
        assert_eq!(Event::parse("ThreadActivity/HistoryDisclosedUpdate", "<x/>"), None);
        assert_eq!(Event::parse("RichText/Html", "<p>hi</p>"), None);
        assert_eq!(Event::parse("ThreadActivity/AddMember", "<addmember/>"), None);
        assert_eq!(renamed.told(&names, "", "meeting"), "Ana named the meeting Launch & co.");
    }

    #[test]
    fn whoever_adds_isnt_among_those_added() {
        let content = "<addmember><initiator>8:orgid:a</initiator><target>8:orgid:a</target><target>8:orgid:me</target></addmember>";
        let e = Event::parse("ThreadActivity/AddMember", content).unwrap();
        let names = HashMap::from([("8:orgid:a".to_string(), "Markus Guerster".to_string())]);
        assert_eq!(e.told(&names, "8:orgid:me", "meeting"), "Markus Guerster added You to the chat.");
    }

    #[test]
    fn meetings_starting_and_ending() {
        let mut started =
            Event::parse("Event/Call", "<partlist alt=\"\"><part identity=\"8:orgid:a\"><name>8:orgid:a</name></part></partlist>").unwrap();
        assert_eq!(started.what, What::MeetingStarted);
        started.by = Some(("8:orgid:a".into(), Some("Mario Bošnjak".into())));
        assert_eq!(started.told(&HashMap::new(), "", "chat"), "Mario Bošnjak started a meeting.");
        let ended = Event::parse(
            "Event/Call",
            "<ended/><partlist count=\"2\"><part identity=\"8:orgid:a\"><duration>903</duration></part></partlist>",
        )
        .unwrap();
        assert_eq!(ended.told(&HashMap::new(), "", "chat"), "Meeting ended: 15m 3s");
        assert_eq!(lasted(42), "42s");
        assert_eq!(lasted(3723), "1h 2m 3s");
    }

    #[test]
    fn names_looked_up_fill_the_ones_it_lacks() {
        let mut e = Event::parse("ThreadActivity/AddMember", ADDED).unwrap();
        assert_eq!(e.ids().cloned().collect::<Vec<_>>(), ["8:orgid:markus", "8:orgid:joss"]);
        e.name(&HashMap::from([("8:orgid:markus".to_string(), "Renner Markus".to_string())]));
        assert_eq!(
            e.told(&HashMap::new(), "", "chat"),
            "Renner Markus added Someone to the chat and shared chat history from the past 3 days."
        );
    }
}
