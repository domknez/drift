//! A made-up workspace for screenshots and for working on the look without an
//! account (`DRIFT_DEMO=1`): the mockups' team, channels, people and messages.

use crate::backend::{Backend, Error, Result};
use crate::model::markdown::Outgoing;
use crate::model::text::{self, Block};
use crate::model::{
    Author, Card, Channel, Conversation, Directory, Kind, Meeting, Message, Picture, Preview, Reaction, SearchHit, Source, Team, time,
};

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
        members: match &picture {
            Some(Picture::Person { id, name }) => vec![crate::model::Mention { id: id.clone(), name: name.clone() }],
            _ => Vec::new(),
        },
        picture,
    }
}

fn channel_preview(id: &str) -> Preview {
    let (author, text, at) = match id {
        OPEN => ("Maya Lindqvist", "Small one, mostly gestures. Happy to pair on it after stand-up 🙂", today(10, 44)),
        "19:alerts@thread.tacv2" => ("Grafana", "p95 latency above 800 ms on api-gateway", today(9, 58)),
        "19:nightly@thread.tacv2" => ("Pipeline", "2 failed, 418 passed", today(5, 13)),
        "19:design@thread.tacv2" => ("Jonas Weber", "New icons are in Figma", today(11, 0) - 4 * 86_400_000),
        "19:side@thread.tacv2" => ("Priya Nair", "Build 0.4 is up for testing", today(11, 0) - 30 * 86_400_000),
        _ => ("Lea Novak", "Welcome to Halcyon!", today(11, 0) - 20 * 86_400_000),
    };
    Preview { author: author.into(), text: text.into(), at }
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
                    last: Some(channel_preview(&c.id)),
                    unread: c.unread,
                    pinned: c.pinned,
                    muted: false,
                    join_url: None,
                    picture: None,
                    members: Vec::new(),
                });
            }
        }
        Ok(Directory { teams, conversations })
    }

    fn messages(&self, conversation: &str) -> Result<Vec<Message>> {
        if conversation == "48:mentions" {
            return Ok(activity());
        }
        if conversation == "19:maya" {
            let like = |mine| vec![Reaction { key: "like".into(), count: 1, mine }];
            return Ok(vec![
                message(
                    "3001",
                    None,
                    "maya",
                    today(9, 2) - 86_400_000,
                    "<p>Did the timeline gestures land in the nightly?</p>",
                    None,
                    vec![],
                ),
                message(
                    "3002",
                    None,
                    ME,
                    today(9, 6) - 86_400_000,
                    "<p>Not yet, the review is still open. Should be in tomorrow's.</p>",
                    None,
                    like(false),
                ),
                message(
                    "3003",
                    None,
                    "maya",
                    today(10, 21),
                    "<p>Morning! Pan by dragging works great, zoom feels a bit fast with a trackpad though.</p>",
                    None,
                    vec![],
                ),
                message(
                    "3004",
                    None,
                    "maya",
                    today(10, 22),
                    "<p>Maybe scale it by <code>deltaY</code> instead of a fixed step?</p>",
                    None,
                    vec![],
                ),
                message(
                    "3005",
                    None,
                    ME,
                    today(10, 25),
                    "<blockquote itemtype=\"http://schema.skype.com/Reply\" itemid=\"3004\"><strong itemprop=\"mri\">Maya Lindqvist</strong><p itemprop=\"preview\">Maybe scale it by deltaY instead of a fixed step?</p></blockquote><p>Good catch, I'll try that after stand-up.</p>",
                    None,
                    vec![Reaction { key: "praying".into(), count: 1, mine: false }],
                ),
                message(
                    "3006",
                    None,
                    ME,
                    today(10, 25),
                    "<p>Also: <b>release cut at 15:00</b>, so anything after that goes in next week.</p>",
                    None,
                    vec![],
                ),
                message(
                    "3008",
                    None,
                    "maya",
                    today(10, 28),
                    "<p>This is how it looks now:</p><p><img itemtype=\"http://schema.skype.com/AMSImage\" src=\"demo:timeline\" width=\"480\" height=\"300\"></p>",
                    None,
                    vec![],
                ),
                message(
                    "3007",
                    None,
                    "maya",
                    today(10, 31),
                    "<p>lunch at 1? the new place on the corner <img itemtype=\"http://schema.skype.com/Emoji\" itemid=\"yum\" alt=\"😋\"></p>",
                    None,
                    like(true),
                ),
            ]);
        }
        if conversation != OPEN {
            let (who, line) = match conversation {
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
                vec![
                    Reaction { key: "1f440_eyes".into(), count: 2, mine: true },
                    Reaction { key: "praying".into(), count: 1, mine: false },
                ],
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
                "<p>Small one, mostly gestures. Happy to pair on it after stand-up <img itemtype=\"http://schema.skype.com/Emoji\" itemid=\"smile\" alt=\"🙂\"></p>",
                None,
                vec![],
            ),
        ])
    }

    fn picture(&self, picture: &Picture) -> Result<Vec<u8>> {
        match picture {
            // Emoji are public; people in the demo have no photos.
            Picture::Emoji(key) => crate::teams::emoji(key),
            Picture::Image(url) if url.starts_with("demo:") => Ok(gradient(480, 300)),
            _ => Err(Error::Service("no pictures in the demo".into())),
        }
    }

    fn send(&self, _conversation: &str, _message: &Outgoing) -> Result<()> {
        Ok(())
    }

    fn upload_image(&self, _conversation: &str, _name: &str, _bytes: &[u8], _mime: &str) -> Result<String> {
        Err(Error::Service("no uploads in the demo".into()))
    }

    fn edit(&self, _conversation: &str, _message: &str, _with: &Outgoing) -> Result<()> {
        Ok(())
    }

    fn delete(&self, _conversation: &str, _message: &str) -> Result<()> {
        Ok(())
    }

    fn react(&self, _conversation: &str, _message: &str, _key: &str, _add: bool) -> Result<()> {
        Ok(())
    }

    fn mark_read(&self, _conversation: &str, _message: &str) -> Result<()> {
        Ok(())
    }

    fn read_by(&self, conversation: &str) -> Result<Vec<(String, i64)>> {
        // Maya has read everything the user sent her.
        Ok(match conversation {
            "19:maya" => vec![(ME.to_string(), i64::MAX), ("8:orgid:maya".to_string(), 3007)],
            _ => Vec::new(),
        })
    }

    fn calendar(&self, _from: i64, _to: i64) -> Result<Vec<Meeting>> {
        let day = 86_400_000;
        let meeting = |id: &str, subject: &str, start: i64, minutes: i64, organizer: &str, online: bool| Meeting {
            id: id.into(),
            subject: subject.into(),
            start,
            end: start + minutes * 60_000,
            organizer: organizer.into(),
            location: if online { "Microsoft Teams Meeting".into() } else { "Room 4.12".into() },
            join_url: online.then(|| "https://teams.microsoft.com/l/meetup-join/demo".to_string()),
            chat: (id == "standup").then(|| "19:standup".to_string()),
            cancelled: false,
            response: "Accepted".into(),
        };
        Ok(vec![
            meeting("standup", "Daily stand-up", today(10, 0), 15, "Maya Lindqvist", true),
            meeting("review", "Release review", today(15, 0), 30, "Priya Nair", true),
            meeting("lunch", "Team lunch", today(13, 0), 60, "Jonas Weber", false),
            meeting("standup2", "Daily stand-up", today(10, 0) + day, 15, "Maya Lindqvist", true),
            meeting("design", "Timeline design sync", today(14, 0) + day, 45, "Lea Novak", true),
            meeting("retro", "Sprint retro", today(11, 0) + 3 * day, 60, "Tomás Rivera", true),
        ])
    }

    fn search(&self, query: &str) -> Result<Vec<SearchHit>> {
        let query = query.to_lowercase();
        let mut hits = Vec::new();
        for (conversation, place) in [(OPEN, "PR Reviews · Halcyon"), ("19:maya", "Maya Lindqvist")] {
            for m in self.messages(conversation)? {
                let text = crate::model::text::flatten(&m.body);
                if text.to_lowercase().contains(&query) {
                    hits.push(SearchHit {
                        conversation: conversation.into(),
                        message: m.id,
                        author: m.author.name,
                        at: m.at,
                        text,
                        place: place.into(),
                    });
                }
            }
        }
        Ok(hits)
    }

    fn pin(&self, _conversation: &str, _pinned: bool) -> Result<()> {
        Ok(())
    }
}

/// The activity feed: mentions and replies, oldest first.
fn activity() -> Vec<Message> {
    let item = |id: &str, who: &str, at: i64, text: &str, place: (&str, &str, &str, Option<&str>), what: &str| Message {
        source: Some(Source {
            conversation: place.0.into(),
            message: Some(place.2.into()),
            root: place.3.map(str::to_string),
            title: place.1.into(),
            what: what.into(),
        }),
        ..message(id, None, who, at, &format!("<p>{text}</p>"), None, vec![])
    };
    vec![
        item(
            "4001",
            "priya",
            today(16, 40) - 86_400_000,
            "@Alex can you double-check the release notes?",
            ("19:release", "Release crew", "2001", None),
            "mention",
        ),
        item(
            "4002",
            "lea",
            today(10, 14),
            "On it. Is the form data still sent twice when the rule has no schedule?",
            (OPEN, "PR Reviews", "1002", Some(THREAD)),
            "reply",
        ),
        item("4003", "maya", today(10, 44), "@Alex happy to pair on it after stand-up", (OPEN, "PR Reviews", "1006", None), "mention"),
    ]
}

fn pr(n: u32, title: &str, from: &str, to: &str) -> Card {
    Card {
        title: format!("{n}: {title}"),
        detail: "Active | pm-ui".into(),
        link: Some(format!("https://example.com/pr/{n}")),
        facts: vec![
            ("Target branch".into(), to.into()),
            ("Source branch".into(), from.into()),
            ("Reviewers".into(), "Lea Novak, Tomás Rivera and 3 more".into()),
        ],
    }
}

fn message(id: &str, root: Option<&str>, who: &str, at: i64, html: &str, card: Option<Card>, reactions: Vec<Reaction>) -> Message {
    let body: Vec<Block> = text::from_html(html);
    Message { id: id.into(), root: root.map(str::to_string), author: author(who), at, body, card, edited: false, reactions, source: None }
}

/// A soft violet-to-teal picture, made up for the demo's image message.
fn gradient(w: u32, h: u32) -> Vec<u8> {
    let mut pixels = Vec::with_capacity((w * h * 3) as usize);
    for y in 0..h {
        for x in 0..w {
            let (fx, fy) = (x as f32 / w as f32, y as f32 / h as f32);
            pixels.extend([(120.0 + 60.0 * fx) as u8, (100.0 + 100.0 * fy) as u8, (230.0 - 60.0 * fx * fy) as u8]);
        }
    }
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, w, h);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    if let Ok(mut writer) = encoder.write_header() {
        let _ = writer.write_image_data(&pixels);
    }
    out
}
