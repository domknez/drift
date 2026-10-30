//! The services' JSON as `model` types. Fields are read leniently: the web client's
//! API isn't documented, and what's missing gets a default rather than an error.

use crate::model::{Author, Card, Channel, Conversation, Kind, Message, Picture, Preview, Reaction, Team, text, time};
use serde_json::Value;

fn str_of<'a>(v: &'a Value, keys: &[&str]) -> Option<&'a str> {
    keys.iter().find_map(|k| v[*k].as_str()).filter(|s| !s.is_empty())
}

/// Teams and their channels from the aggregator's roster (archived ones left out).
pub fn teams(roster: &Value) -> Vec<Team> {
    let live = |v: &&Value| !v["isArchived"].as_bool().unwrap_or(false);
    let mut teams: Vec<Team> = roster["teams"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(live)
        .filter_map(|t| {
            let id = str_of(t, &["id"])?.to_string();
            let mut channels: Vec<Channel> = t["channels"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(live)
                .filter_map(|c| {
                    let cid = str_of(c, &["id"])?.to_string();
                    let general = c["isGeneral"].as_bool().or(c["isGeneralChannel"].as_bool()).unwrap_or(cid == id);
                    Some(Channel {
                        name: str_of(c, &["displayName"]).unwrap_or(if general { "General" } else { "Channel" }).to_string(),
                        id: cid,
                        general,
                        unread: !c["isMessageRead"].as_bool().unwrap_or(true),
                        pinned: c["isFavorite"].as_bool().unwrap_or(false) || c["isPinned"].as_bool().unwrap_or(false),
                        muted: c["isMuted"].as_bool().unwrap_or(false),
                        last: preview(&c["lastMessage"]),
                    })
                })
                .collect();
            // General first, then favourites, the rest by name.
            channels.sort_by_key(|c| (!c.general, !c.pinned, c.name.to_lowercase()));
            let name = str_of(t, &["displayName"]).unwrap_or("Team").to_string();
            let picture = str_of(&t["teamSiteInformation"], &["groupId"]).map(|group| Picture::Team {
                group: group.to_string(),
                etag: str_of(t, &["pictureETag"]).unwrap_or("").to_string(),
                name: name.clone(),
            });
            Some(Team { name, id, channels, picture })
        })
        .collect();
    teams.sort_by_key(|t| t.name.to_lowercase());
    teams
}

/// The chats from the aggregator's roster: hidden ones left out, newest first.
/// Chats without a title are named by `name_of(member ids)` later.
pub fn chats(roster: &Value) -> Vec<(Conversation, Vec<String>)> {
    let mut chats: Vec<(Conversation, Vec<String>)> = roster["chats"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|c| !c["hidden"].as_bool().unwrap_or(false))
        .filter_map(|c| {
            let id = str_of(c, &["id"])?.to_string();
            // Recurring chats carry meeting details too; only the type tells a meeting's chat.
            let meeting = [c["chatType"].as_str(), c["threadType"].as_str()].contains(&Some("meeting"));
            let members = c["members"].as_array().into_iter().flatten().filter_map(|m| m["mri"].as_str().map(str::to_string)).collect();
            let conversation = Conversation {
                id,
                kind: if meeting { Kind::Meeting } else { Kind::Chat },
                title: str_of(c, &["title"]).unwrap_or("").to_string(),
                last: preview(&c["lastMessage"]),
                unread: !c["isRead"].as_bool().unwrap_or(true),
                pinned: c["isSticky"].as_bool().unwrap_or(false),
                muted: c["isMuted"].as_bool().unwrap_or(false),
                join_url: str_of(&c["meetingInformation"], &["meetingJoinUrl"]).map(str::to_string),
                picture: None,
            };
            Some((conversation, members))
        })
        .collect();
    chats.sort_by_key(|(c, _)| std::cmp::Reverse(c.last.as_ref().map_or(0, |p| p.at)));
    chats
}

/// The chat service's conversation list: what's unread and the newest message, by id.
pub fn conversations(v: &Value) -> Vec<Conversation> {
    v["conversations"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|c| {
            let id = str_of(c, &["id"])?.to_string();
            let props = &c["threadProperties"];
            let kind = match props["threadType"].as_str() {
                Some("topic" | "space") => Kind::Channel,
                Some("meeting") => Kind::Meeting,
                Some(t) if t.starts_with("streamof") => Kind::Feed,
                _ if id.starts_with("48:") => Kind::Feed,
                _ => Kind::Chat,
            };
            let last = preview(&c["lastMessage"]);
            let read_up_to = c["properties"]["consumptionhorizon"].as_str().and_then(|h| h.split(';').next()?.parse::<i64>().ok());
            let last_id = c["lastMessage"]["id"].as_str().and_then(|i| i.parse::<i64>().ok());
            let unread = matches!((last_id, read_up_to), (Some(l), Some(r)) if l > r);
            Some(Conversation {
                id,
                kind,
                title: str_of(props, &["topic"]).unwrap_or("").to_string(),
                last,
                unread,
                pinned: false,
                muted: false,
                join_url: None,
                picture: None,
            })
        })
        .collect()
}

fn preview(m: &Value) -> Option<Preview> {
    let at = str_of(m, &["composetime", "composeTime", "originalarrivaltime", "originalArrivalTime"]).and_then(time::parse_iso)?;
    let content = str_of(m, &["content"]).unwrap_or("");
    let html = str_of(m, &["messagetype", "messageType"]).is_some_and(|t| t.starts_with("RichText"));
    let body = if html { text::from_html(content) } else { text::plain(content) };
    let author = str_of(m, &["imdisplayname", "imDisplayName"]).unwrap_or("").to_string();
    Some(Preview { author, text: text::flatten(&body), at })
}

/// A message list from the chat service, oldest first. Control messages, call and
/// membership events and deleted messages are left out.
pub fn messages(v: &Value, me: &str) -> Vec<Message> {
    let mut out: Vec<Message> = v["messages"].as_array().into_iter().flatten().filter_map(|m| message(m, me)).collect();
    out.sort_by_key(|m| (m.at, m.id.parse::<i64>().unwrap_or(0)));
    out
}

fn message(m: &Value, me: &str) -> Option<Message> {
    let kind = str_of(m, &["messagetype"])?;
    if !matches!(kind, "Text" | "RichText" | "RichText/Html" | "RichText/Media_Card") || m["properties"]["deletetime"].is_string() {
        return None;
    }
    let id = str_of(m, &["id"])?.to_string();
    let content = str_of(m, &["content"]).unwrap_or("");
    let body = if kind == "Text" { text::plain(content) } else { text::from_html(content) };
    let card = card(&m["properties"]["cards"]).or_else(|| card(&m["properties"]["attachments"]));
    if body.is_empty() && card.is_none() {
        return None;
    }
    let at = str_of(m, &["originalarrivaltime", "composetime"]).and_then(time::parse_iso).unwrap_or(0);
    // Channel replies name their post (`rootMessageId`, or `…;messageid=<post id>` in the link).
    let root = str_of(m, &["rootMessageId"])
        .map(str::to_string)
        .or_else(|| {
            str_of(m, &["conversationLink", "conversationid"]).and_then(|l| l.rsplit_once(";messageid=").map(|(_, r)| r.to_string()))
        })
        .filter(|r| *r != id);
    let from = str_of(m, &["from"]).unwrap_or("");
    let author = Author {
        id: from.rsplit('/').next().unwrap_or(from).to_string(),
        name: str_of(m, &["imdisplayname"]).unwrap_or("Unknown").to_string(),
    };
    Some(Message {
        id,
        root,
        author,
        at,
        body,
        card,
        edited: m["properties"]["edittime"].is_string() || m["skypeeditedid"].is_string(),
        reactions: reactions(&m["properties"]["emotions"], me),
    })
}

/// A JSON value the service may send as JSON text instead.
fn embedded(v: &Value) -> Value {
    match v {
        Value::String(s) => serde_json::from_str(s).unwrap_or(Value::Null),
        v => v.clone(),
    }
}

fn reactions(v: &Value, me: &str) -> Vec<Reaction> {
    embedded(v)
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|e| {
            let users = e["users"].as_array()?;
            let mine = users.iter().any(|u| u["mri"].as_str() == Some(me));
            let key = str_of(e, &["key"])?.to_string();
            (!users.is_empty()).then_some(Reaction { key, count: users.len(), mine })
        })
        .collect()
}

/// An adaptive card (or a hero card) reduced to its first lines and its link.
fn card(v: &Value) -> Option<Card> {
    let v = embedded(v);
    if v.is_null() {
        return None;
    }
    let mut lines = Vec::new();
    let mut link = None;
    walk(&v, &mut lines, &mut link);
    let mut lines = lines.into_iter().map(|l| text::flatten(&text::from_html(&l))).filter(|l| !l.is_empty());
    let title = lines.next()?;
    let detail = lines.take(3).collect::<Vec<_>>().join(" · ");
    Some(Card { title, detail, link })
}

fn walk(v: &Value, lines: &mut Vec<String>, link: &mut Option<String>) {
    match v {
        Value::Object(o) => {
            let kind = o.get("type").and_then(Value::as_str).unwrap_or("");
            if link.is_none() && kind == "Action.OpenUrl" {
                *link = o.get("url").and_then(Value::as_str).map(str::to_string);
            }
            if matches!(kind, "TextBlock" | "TextRun") {
                if let Some(t) = o.get("text").and_then(Value::as_str) {
                    lines.push(t.to_string());
                }
            } else if let Some(t) = o.get("title").and_then(Value::as_str).filter(|_| kind.is_empty() && !o.contains_key("value")) {
                lines.push(t.to_string());
            }
            // Fact sets: "Target branch: develop".
            if let (Some(t), Some(v)) = (o.get("title").and_then(Value::as_str), o.get("value").and_then(Value::as_str)) {
                lines.push(format!("{t} {v}"));
            }
            for (k, child) in o {
                if k != "text" && k != "title" {
                    walk(child, lines, link);
                }
            }
        }
        Value::Array(a) => a.iter().for_each(|c| walk(c, lines, link)),
        Value::String(s) if s.trim_start().starts_with('{') || s.trim_start().starts_with('[') => walk(&embedded(v), lines, link),
        _ => {}
    }
}

/// Display names by id from the middle tier's profile answer.
pub fn profiles(v: &Value) -> Vec<(String, String)> {
    let list = v["value"].as_array().filter(|a| !a.is_empty()).or(v["resolvedUsers"].as_array());
    list.into_iter()
        .flatten()
        .filter_map(|p| Some((str_of(p, &["mri"])?.to_string(), str_of(p, &["displayName", "givenName"])?.to_string())))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const ME: &str = "8:orgid:me";

    #[test]
    fn reads_teams_and_channels() {
        let roster = json!({"teams": [
            {"id": "19:b@thread.tacv2", "displayName": "Side project", "channels": [{"id": "19:b@thread.tacv2", "displayName": "General", "isGeneral": true}]},
            {"id": "19:a@thread.tacv2", "displayName": "Halcyon", "channels": [
                {"id": "19:x@thread.tacv2", "displayName": "PR Reviews"},
                {"id": "19:a@thread.tacv2", "displayName": "General"},
                {"id": "19:old@thread.tacv2", "displayName": "Old", "isArchived": true}]},
            {"id": "19:gone@thread.tacv2", "displayName": "Gone", "isArchived": true}]});
        let teams = teams(&roster);
        assert_eq!(teams.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(), ["Halcyon", "Side project"]);
        assert_eq!(teams[0].channels.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), ["General", "PR Reviews"]);
        assert!(teams[0].channels[0].general);
    }

    #[test]
    fn reads_chats_newest_first() {
        let roster = json!({"chats": [
            {"id": "19:old", "title": "Release crew", "isRead": true, "lastMessage": {"composeTime": "2026-10-28T09:00:00Z", "content": "cut at 15:00", "imDisplayName": "Priya"}},
            {"id": "19:new", "isRead": false, "members": [{"mri": ME}, {"mri": "8:orgid:maya"}], "lastMessage": {"composeTime": "2026-10-30T09:00:00Z", "messageType": "RichText/Html", "content": "<p>lunch?</p>"}},
            {"id": "19:meet", "chatType": "meeting", "title": "Stand-up"},
            {"id": "19:hidden", "hidden": true}]});
        let chats = chats(&roster);
        assert_eq!(chats.iter().map(|(c, _)| c.id.as_str()).collect::<Vec<_>>(), ["19:new", "19:old", "19:meet"]);
        assert!(chats[0].0.unread && !chats[1].0.unread);
        assert_eq!(chats[0].0.last.as_ref().unwrap().text, "lunch?");
        assert_eq!(chats[0].1, [ME, "8:orgid:maya"]);
        assert_eq!(chats[2].0.kind, Kind::Meeting);
    }

    #[test]
    fn tells_unread_from_the_horizon() {
        let v = json!({"conversations": [
            {"id": "19:x@thread.tacv2", "threadProperties": {"threadType": "topic", "topic": "PR Reviews"}, "properties": {"consumptionhorizon": "100;100;100"}, "lastMessage": {"id": "200", "composetime": "2026-10-30T09:00:00Z"}},
            {"id": "19:y@thread.v2", "properties": {"consumptionhorizon": "300;300;300"}, "lastMessage": {"id": "200"}},
            {"id": "48:mentions", "threadProperties": {"threadType": "streamofmentions"}}]});
        let c = conversations(&v);
        assert_eq!((c[0].kind, c[0].unread, c[0].title.as_str()), (Kind::Channel, true, "PR Reviews"));
        assert_eq!((c[1].kind, c[1].unread), (Kind::Chat, false));
        assert_eq!(c[2].kind, Kind::Feed);
    }

    #[test]
    fn reads_messages_oldest_first() {
        let v = json!({"messages": [
            {"id": "3", "messagetype": "RichText/Html", "content": "<p>Approved</p>", "imdisplayname": "Alex", "from": "https://x/v1/users/ME/contacts/8:orgid:me",
             "originalarrivaltime": "2026-10-30T10:30:00.000Z", "conversationLink": "https://x/conversations/19:c@thread.tacv2;messageid=1",
             "properties": {"emotions": "[{\"key\":\"like\",\"users\":[{\"mri\":\"8:orgid:me\"},{\"mri\":\"8:orgid:b\"}]}]", "edittime": "1"}},
            {"id": "2", "messagetype": "ThreadActivity/AddMember", "content": "<addmember/>"},
            {"id": "4", "messagetype": "Text", "content": "gone", "properties": {"deletetime": "5"}},
            {"id": "1", "messagetype": "RichText/Html", "content": "<p>Take a look?</p>", "imdisplayname": "Tomás", "from": "8:orgid:t",
             "originalarrivaltime": "2026-10-30T10:05:00.000Z", "conversationLink": "https://x/conversations/19:c@thread.tacv2;messageid=1"}]});
        let m = messages(&v, ME);
        assert_eq!(m.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(), ["1", "3"]);
        assert_eq!((m[0].root.as_deref(), m[1].root.as_deref()), (None, Some("1")));
        assert_eq!(m[1].author, Author { id: ME.into(), name: "Alex".into() });
        assert!(m[1].edited);
        assert_eq!(m[1].reactions, [Reaction { key: "like".into(), count: 2, mine: true }]);
    }

    #[test]
    fn reduces_cards() {
        let cards = json!([{"content": {"type": "AdaptiveCard", "body": [
            {"type": "TextBlock", "text": "Lea created a pull request"},
            {"type": "TextBlock", "text": "10500: manual trigger sends the picked rule again"},
            {"type": "FactSet", "facts": [{"title": "Target branch:", "value": "main"}]}],
            "actions": [{"type": "Action.OpenUrl", "title": "Open", "url": "https://example.com/pr/10500"}]}}]);
        let v = json!({"messages": [{"id": "1", "messagetype": "RichText/Html", "content": "", "originalarrivaltime": "2026-10-30T10:42:00Z",
            "imdisplayname": "Repos", "properties": {"cards": cards.to_string()}}]});
        let card = messages(&v, ME).remove(0).card.unwrap();
        assert_eq!(card.title, "Lea created a pull request");
        assert_eq!(card.detail, "10500: manual trigger sends the picked rule again · Target branch: main");
        assert_eq!(card.link.as_deref(), Some("https://example.com/pr/10500"));
    }

    #[test]
    fn reads_profiles() {
        let v = json!({"value": [{"mri": "8:orgid:maya", "displayName": "Maya Lindqvist"}, {"mri": "8:orgid:x"}]});
        assert_eq!(profiles(&v), [("8:orgid:maya".to_string(), "Maya Lindqvist".to_string())]);
    }
}
