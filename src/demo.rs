//! A made-up workspace for screenshots and for working on the look without an
//! account (`DRIFT_DEMO=1`): the mockups' team, channels, people and messages.

use crate::backend::{Backend, Error, Result};
use crate::model::text::{self, Block};
use crate::model::{Author, Card, Channel, Conversation, Directory, Kind, Message, Picture, Preview, Reaction, Team, time};

pub struct Demo;

pub const ME: &str = "8:orgid:alex";
/// The conversation and thread the demo opens with.
pub const OPEN: &str = "19:pr-reviews@thread.tacv2";
pub const THREAD: &str = "1001";

fn person(id: &str) -> (&'static str, &'static str) {
    match id {
        ME => (ME, "Alex Morgan"),
        "maya" => ("8:orgid:maya", "Maya Lindqvist"),
        "tomas" => ("8:orgid:tomas", "Tomás Rivera"),
        "priya" => ("8:orgid:priya", "Priya Nair"),
        "jonas" => ("8:orgid:jonas", "Jonas Weber"),
        "lea" => ("8:orgid:lea", "Lea Novak"),
        _ => ("28:repos", "Repos"),
    }
}

fn author(id: &str) -> Author {
    let (id, name) = person(id);
    Author { id: id.into(), name: name.into() }
}

/// Today at `h:m`, local time.
fn today(h: i64, m: i64) -> i64 {
    let now = time::now_ms();
    let start = now - (now / 1000 + time::local_offset(now / 1000)).rem_euclid(86_400) * 1000;
    start + (h * 3600 + m * 60) * 1000
}

fn chat(id: &str, kind: Kind, title: &str, last: (&str, &str, i64), unread: bool, muted: bool, picture: Option<Picture>) -> Conversation {
    Conversation {
        id: id.into(),
        kind,
        title: title.into(),
        last: Some(Preview { author: last.0.into(), text: last.1.into(), at: last.2 }),
        unread,
        pinned: false,
        muted,
        join_url: None,
        picture,
    }
}

fn channel(id: &str, name: &str, general: bool, unread: bool, pinned: bool) -> Channel {
    Channel { id: id.into(), name: name.into(), general, unread, pinned, muted: false, last: None }
}

impl Backend for Demo {
    fn me(&self) -> String {
        "Alex Morgan".into()
    }

    fn me_id(&self) -> String {
        ME.into()
    }

    fn directory(&self) -> Result<Directory> {
        let teams = vec![
            Team {
                id: "19:halcyon@thread.tacv2".into(),
                name: "Halcyon".into(),
                picture: None,
                channels: vec![
                    channel("19:halcyon@thread.tacv2", "General", true, false, false),
                    channel("19:nightly@thread.tacv2", "Nightly UI tests", false, true, false),
                    channel(OPEN, "PR Reviews", false, false, true),
                    channel("19:alerts@thread.tacv2", "Prod Alerts", false, true, true),
                    channel("19:design@thread.tacv2", "Design", false, false, false),
                ],
            },
            Team {
                id: "19:side@thread.tacv2".into(),
                name: "Side project".into(),
                picture: None,
                channels: vec![channel("19:side@thread.tacv2", "Testing app", true, false, false)],
            },
        ];
        let person_picture = |id: &str| {
            let (id, name) = person(id);
            Some(Picture::Person { id: id.into(), name: name.into() })
        };
        let mut conversations = vec![
            chat(
                "19:maya",
                Kind::Chat,
                "Maya Lindqvist",
                ("Maya", "lunch at 1? the new place on the corner", today(10, 31)),
                true,
                false,
                person_picture("maya"),
            ),
            chat(
                "19:tomas",
                Kind::Chat,
                "Tomás Rivera",
                ("You", "rebased, should be green now", today(10, 12)),
                false,
                false,
                person_picture("tomas"),
            ),
            chat("19:release", Kind::Chat, "Release crew", ("Priya", "cut is at 15:00 today", today(9, 40)), true, false, None),
            chat(
                "19:priya",
                Kind::Chat,
                "Priya Nair",
                ("Priya", "thanks, looks good 👍", today(9, 0) - 86_400_000),
                false,
                false,
                person_picture("priya"),
            ),
            chat(
                "19:jonas",
                Kind::Chat,
                "Jonas Weber",
                ("Jonas", "brutal 😅", today(9, 0) - 3 * 86_400_000),
                false,
                true,
                person_picture("jonas"),
            ),
            Conversation {
                join_url: Some("https://teams.microsoft.com/l/meetup-join/demo".into()),
                ..chat("19:standup", Kind::Meeting, "Daily stand-up", ("Maya", "see you there", today(9, 58)), false, false, None)
            },
            chat("48:mentions", Kind::Feed, "Activity", ("Maya", "mentioned you in PR Reviews", today(10, 44)), true, false, None),
        ];
        for team in &teams {
            for c in &team.channels {
                conversations.push(Conversation {
                    id: c.id.clone(),
                    kind: Kind::Channel,
                    title: c.name.clone(),
                    last: Some(Preview { author: String::new(), text: String::new(), at: today(8, 0) }),
                    unread: c.unread,
                    pinned: c.pinned,
                    muted: false,
                    join_url: None,
                    picture: None,
                });
            }
        }
        Ok(Directory { teams, conversations })
    }

    fn messages(&self, conversation: &str) -> Result<Vec<Message>> {
        if conversation != OPEN {
            let (who, line) = match conversation {
                "19:maya" => ("maya", "lunch at 1? the new place on the corner"),
                "19:tomas" => (ME, "rebased, should be green now"),
                _ => ("priya", "cut is at 15:00 today"),
            };
            return Ok(vec![message("2001", None, who, today(10, 0), &format!("<p>{line}</p>"), None, vec![])]);
        }
        let yesterday = today(17, 34) - 86_400_000;
        Ok(vec![
            message(
                "1000",
                None,
                "repos",
                yesterday,
                "<p>Lea Novak created a pull request</p>",
                Some(pr(10500, "release(122.102): manual trigger sends the picked rule again", "releases/122.102", "main")),
                vec![],
            ),
            message(
                THREAD,
                None,
                "tomas",
                today(10, 5),
                "<p>Can someone take a look at <a href=\"https://example.com/pr/10500\">#10500</a> before the release cut? It only touches <code>trigger_rule()</code>, the rest is tests.</p>",
                None,
                vec![Reaction { key: "1f440_eyes".into(), count: 2, mine: true }, Reaction { key: "pray".into(), count: 1, mine: false }],
            ),
            message(
                "1002",
                Some(THREAD),
                "lea",
                today(10, 14),
                "<p>On it. Is the form data still sent twice when the rule has no schedule?</p>",
                None,
                vec![],
            ),
            message(
                "1003",
                Some(THREAD),
                "tomas",
                today(10, 19),
                "<p>No, that was the bug: the second request is gone now.</p>",
                None,
                vec![],
            ),
            message(
                "1004",
                Some(THREAD),
                ME,
                today(10, 30),
                "<p>Approved. Left one nit about the error message, not blocking.</p>",
                None,
                vec![],
            ),
            message(
                "1005",
                None,
                "repos",
                today(10, 42),
                "<p>Maya Lindqvist created a pull request</p>",
                Some(pr(10503, "feat(timelines): pan by dragging, zoom with ⌘ + wheel", "feature/timeline-pan-zoom", "develop")),
                vec![],
            ),
            message(
                "1006",
                None,
                "maya",
                today(10, 44),
                "<p>Small one, mostly gestures. Happy to pair on it after stand-up 🙂</p>",
                None,
                vec![],
            ),
        ])
    }

    fn picture(&self, _picture: &Picture) -> Result<Vec<u8>> {
        Err(Error::Service("no pictures in the demo".into()))
    }

    fn send(&self, _conversation: &str, _text: &str) -> Result<()> {
        Ok(())
    }
}

fn pr(n: u32, title: &str, from: &str, to: &str) -> Card {
    Card {
        title: format!("#{n} {title}"),
        detail: format!("{from} → {to} · 5 reviewers"),
        link: Some(format!("https://example.com/pr/{n}")),
    }
}

fn message(id: &str, root: Option<&str>, who: &str, at: i64, html: &str, card: Option<Card>, reactions: Vec<Reaction>) -> Message {
    let body: Vec<Block> = text::from_html(html);
    Message { id: id.into(), root: root.map(str::to_string), author: author(who), at, body, card, edited: false, reactions }
}
