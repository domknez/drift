//! The services' JSON as `model` types. Fields are read leniently: the web client's
//! API isn't documented, and what's missing gets a default rather than an error.

use crate::model::{
    Author, Availability, Card, Channel, Conversation, Kind, Meeting, Message, Picture, Presence, Preview, Reaction, SearchHit, Source,
    Status, Team, text, time,
};
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
                        // isFavorite on a channel is whether it's shown in its team's list, not a pin.
                        pinned: c["isPinned"].as_bool().unwrap_or(false),
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
/// Per-chat extras the chat service doesn't give: who's in it (for naming 1:1s and
/// drawing their pictures), a group's title, whether it's a favourite or muted.
pub struct RosterChat {
    pub members: Vec<String>,
    pub title: String,
    pub favorite: bool,
    pub muted: bool,
}

/// The roster's chats by id (the chat service lists the chats; this names them).
pub fn roster_chats(roster: &Value) -> std::collections::HashMap<String, RosterChat> {
    roster["chats"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|c| {
            let id = str_of(c, &["id"])?.to_string();
            let members = c["members"].as_array().into_iter().flatten().filter_map(|m| m["mri"].as_str().map(str::to_string)).collect();
            let extra = RosterChat {
                members,
                title: str_of(c, &["title"]).unwrap_or("").to_string(),
                favorite: c["isFavorite"].as_bool().unwrap_or(false) || c["isSticky"].as_bool().unwrap_or(false),
                muted: c["isMuted"].as_bool().unwrap_or(false),
            };
            Some((id, extra))
        })
        .collect()
}

/// The chat with oneself, for notes.
pub const NOTES: &str = "48:notes";

/// The chat service's conversation list: what's unread and the newest message, by id.
pub fn conversations(v: &Value) -> Vec<Conversation> {
    v["conversations"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|c| {
            let id = str_of(c, &["id"])?.to_string();
            let props = &c["threadProperties"];
            let cprops = &c["properties"];
            // productThreadType is the clearest signal; fall back to threadType.
            let kind = match props["productThreadType"].as_str() {
                // The chat with oneself (notes), which Teams keeps among its streams.
                _ if id == NOTES => Kind::Chat,
                Some("OneToOneChat" | "Chat") => Kind::Chat,
                Some("Meeting") => Kind::Meeting,
                Some(t) if t.starts_with("Teams") => Kind::Channel,
                Some(t) if t.starts_with("StreamOf") => Kind::Feed,
                _ => match props["threadType"].as_str() {
                    Some("topic" | "space") => Kind::Channel,
                    Some("meeting") => Kind::Meeting,
                    Some(t) if t.starts_with("streamof") => Kind::Feed,
                    _ if id.starts_with("48:") => Kind::Feed,
                    _ => Kind::Chat,
                },
            };
            let last = preview(&c["lastMessage"]);
            let read_up_to = cprops["consumptionhorizon"].as_str().and_then(|h| h.split(';').next()?.parse::<i64>().ok());
            let last_id = c["lastMessage"]["id"].as_str().and_then(|i| i.parse::<i64>().ok());
            let unread = matches!((last_id, read_up_to), (Some(l), Some(r)) if l > r);
            // properties come as strings in this view ("true"/"false").
            let flag = |v: &Value, k: &str| v[k].as_str() == Some("true") || v[k].as_bool() == Some(true);
            let join_url = embedded(&props["meetingInfo"])["meetingJoinUrl"]
                .as_str()
                .or(embedded(&cprops["meetingInfo"])["meetingJoinUrl"].as_str())
                .map(str::to_string);
            Some(Conversation {
                id,
                kind,
                title: str_of(props, &["topic"]).unwrap_or("").to_string(),
                last,
                unread,
                read_up_to,
                pinned: flag(cprops, "favorite"),
                muted: cprops["alerts"].as_str().is_some_and(|a| a.eq_ignore_ascii_case("mute")),
                join_url,
                picture: None,
                members: Vec::new(),
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
    let activity = &m["properties"]["activity"];
    let content = str_of(m, &["content"]).unwrap_or("");
    let swift = (kind == "RichText/Media_Card").then(|| swift(content)).flatten();
    let mut body = match &swift {
        // A connector's card: its summary as the text (the rest of the content repeats it).
        Some(card) => text::plain(card["summary"].as_str().unwrap_or("")),
        None if kind == "Text" => text::plain(content),
        None => text::from_html(content),
    };
    if body.is_empty()
        && let Some(preview) = str_of(activity, &["messagePreview"])
    {
        body = text::plain(preview);
    }
    let card = swift
        .as_ref()
        .and_then(|s| card(&s["attachments"]))
        .or_else(|| card(&m["properties"]["cards"]))
        .or_else(|| card(&m["properties"]["attachments"]));
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
    // Activity-feed items are written by the service; who acted is in `activity`.
    let author = match str_of(activity, &["sourceUserId"]) {
        Some(id) => Author { id: id.to_string(), name: str_of(activity, &["sourceUserImDisplayName"]).unwrap_or("Someone").to_string() },
        None => Author {
            id: from.rsplit('/').next().unwrap_or(from).to_string(),
            name: str_of(m, &["imdisplayname", "fromDisplayNameInToken"]).unwrap_or("Someone").to_string(),
        },
    };
    let number = |k: &str| activity[k].as_i64().map(|n| n.to_string()).or_else(|| str_of(activity, &[k]).map(str::to_string));
    let source_message = number("sourceMessageId");
    let source = str_of(activity, &["sourceThreadId"]).map(|conversation| Source {
        conversation: conversation.to_string(),
        root: number("sourceReplyChainId").filter(|r| Some(r) != source_message.as_ref() && r != "0"),
        message: source_message.clone(),
        title: str_of(activity, &["sourceThreadTopic"]).unwrap_or("").to_string(),
        what: str_of(activity, &["activityType"]).unwrap_or("").to_string(),
    });
    Some(Message {
        id,
        root,
        author,
        at,
        body,
        card,
        edited: !m["properties"]["edittime"].is_null() || m["skypeeditedid"].is_string(),
        reactions: reactions(&m["properties"]["emotions"], me),
        source,
    })
}

/// The card JSON inside a `RichText/Media_Card` message:
/// `<URIObject type="SWIFT.1">…<Swift b64="…"/>…</URIObject>`.
pub fn swift(content: &str) -> Option<Value> {
    let at = content.find("<Swift")?;
    let tag = &content[at..content[at..].find('>').map(|e| at + e)?];
    let start = tag.find("b64=\"")? + 5;
    let b64 = &tag[start..start + tag[start..].find('"')?];
    serde_json::from_slice(&super::auth::base64url(b64)?).ok()
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

/// An adaptive card (or a hero card) reduced to what the UI shows: the emphasised
/// line as the title (a Markdown link in it is the card's link), the other lines,
/// and its facts.
fn card(v: &Value) -> Option<Card> {
    let v = embedded(v);
    if v.is_null() {
        return None;
    }
    let mut found = Found::default();
    walk(&v, &mut found);
    let mut lines: Vec<(String, bool)> = found.lines.into_iter().map(|(l, strong)| (strip_link(&l, &mut found.link), strong)).collect();
    lines.retain(|(l, _)| !l.is_empty());
    let at = lines.iter().position(|(_, strong)| *strong).unwrap_or(0);
    if lines.is_empty() && found.facts.is_empty() {
        return None;
    }
    let title = if lines.is_empty() { String::new() } else { lines.remove(at).0 };
    let detail = lines.into_iter().map(|(l, _)| l).take(2).collect::<Vec<_>>().join(" · ");
    let facts = found.facts.into_iter().map(|(t, v)| (t.trim_end_matches(':').to_string(), strip_link(&v, &mut None))).take(6).collect();
    Some(Card { title, detail, link: found.link, facts })
}

#[derive(Default)]
struct Found {
    /// Text lines, and whether each is emphasised (bold, or a larger size).
    lines: Vec<(String, bool)>,
    facts: Vec<(String, String)>,
    link: Option<String>,
}

fn walk(v: &Value, found: &mut Found) {
    match v {
        Value::Object(o) => {
            let kind = o.get("type").and_then(Value::as_str).unwrap_or("");
            let text = |k: &str| o.get(k).and_then(Value::as_str);
            if found.link.is_none() && kind == "Action.OpenUrl" {
                found.link = text("url").map(str::to_string);
            }
            if matches!(kind, "TextBlock" | "TextRun") {
                if let Some(t) = text("text") {
                    let strong = text("weight") == Some("bolder") || matches!(text("size"), Some("medium" | "large" | "extraLarge"));
                    found.lines.push((text::flatten(&text::from_html(t)), strong));
                }
            } else if let (Some(t), Some(value)) = (text("title"), text("value")) {
                found.facts.push((t.to_string(), value.to_string()));
            } else if let Some(t) = text("title").filter(|_| kind.is_empty()) {
                found.lines.push((t.to_string(), true));
            }
            for (k, child) in o {
                if k != "text" && k != "title" && k != "value" {
                    walk(child, found);
                }
            }
        }
        Value::Array(a) => a.iter().for_each(|c| walk(c, found)),
        Value::String(s) if s.trim_start().starts_with('{') || s.trim_start().starts_with('[') => walk(&embedded(v), found),
        _ => {}
    }
}

/// `[text](url)` as its text; the first url is kept as the link.
fn strip_link(s: &str, link: &mut Option<String>) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(open) = rest.find('[') {
        let Some(close) = rest[open..].find("](").map(|c| open + c) else { break };
        let Some(end) = rest[close + 2..].find(')').map(|e| close + 2 + e) else { break };
        out.push_str(&rest[..open]);
        out.push_str(&rest[open + 1..close]);
        if link.is_none() {
            *link = Some(rest[close + 2..end].to_string());
        }
        rest = &rest[end + 1..];
    }
    out.push_str(rest);
    out.trim().to_string()
}

/// Each member's read position in a thread: up to which message (its id, a time in
/// ms) they have read.
pub fn read_positions(v: &Value) -> Vec<(String, i64)> {
    v["consumptionhorizons"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|h| {
            let id = str_of(h, &["id"])?.to_string();
            let up_to = str_of(h, &["consumptionhorizon"])?.split(';').next()?.parse::<i64>().ok()?;
            Some((id, up_to))
        })
        .collect()
}

/// The user's status from their presence entry. A status set with an end that has
/// passed no longer counts.
pub fn status(entry: &Value) -> Status {
    let presence = &entry["presence"];
    let forced = &presence["forcedAvailability"];
    let current = forced["expiry"].as_str().and_then(time::parse_iso).is_none_or(|end| end > time::now_ms());
    Status {
        shown: Availability::from_key(str_of(presence, &["availability"]).unwrap_or("")),
        set: str_of(forced, &["availability"]).filter(|_| current).map(Availability::from_key).filter(|a| *a != Availability::Unknown),
    }
}

/// Each person's presence from a presence answer, by id.
pub fn presences(v: &Value) -> Vec<(String, Presence)> {
    v.as_array()
        .into_iter()
        .flatten()
        .filter_map(|entry| {
            let presence = &entry["presence"];
            let availability = Availability::from_key(str_of(presence, &["availability"]).unwrap_or(""));
            let out_of_office = presence["calendarData"]["isOutOfOffice"].as_bool().unwrap_or(false);
            Some((str_of(entry, &["mri"])?.to_string(), Presence { availability, out_of_office }))
        })
        .collect()
}

/// Meetings from the middle tier's calendar view, earliest first.
pub fn meetings(v: &Value) -> Vec<Meeting> {
    let mut list: Vec<Meeting> = v["value"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|e| {
            let start = str_of(e, &["startTime"]).and_then(time::parse_iso)?;
            let end = str_of(e, &["endTime"]).and_then(time::parse_iso).unwrap_or(start);
            Some(Meeting {
                id: str_of(e, &["objectId", "iCalUID"]).unwrap_or("").to_string(),
                subject: str_of(e, &["subject"]).unwrap_or("(no subject)").to_string(),
                start,
                end,
                organizer: str_of(e, &["organizerName"]).unwrap_or("").to_string(),
                location: str_of(e, &["location"]).unwrap_or("").to_string(),
                join_url: str_of(e, &["skypeTeamsMeetingUrl", "joinOnlineMeetingUrl"]).map(str::to_string),
                chat: str_of(&e["skypeTeamsDataObject"], &["cid"]).map(str::to_string),
                cancelled: e["isCancelled"].as_bool().unwrap_or(false),
                response: str_of(e, &["myResponseType"]).unwrap_or("").to_string(),
            })
        })
        .collect();
    list.sort_by_key(|m| (m.start, m.end));
    list
}

/// Messages found by Microsoft 365 search.
pub fn search_hits(v: &Value) -> Vec<SearchHit> {
    v["EntitySets"][0]["ResultSets"][0]["Results"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|r| {
            let src = &r["Source"];
            let conversation = str_of(src, &["ClientConversationId", "ClientThreadId"])?.to_string();
            let message = str_of(src, &["InternetMessageId"])?.to_string();
            let at = str_of(src, &["DateTimeSent", "DateTimeReceived", "DateTimeCreated"]).and_then(time::parse_iso).unwrap_or(0);
            let author = str_of(&src["From"]["EmailAddress"], &["Name"]).unwrap_or("").to_string();
            let summary = str_of(r, &["HitHighlightedSummary"]).or(str_of(src, &["Preview"])).unwrap_or("");
            // The summary marks the matches with <c0>…</c0>; the text alone is kept.
            let text = text::flatten(&text::from_html(&summary.replace("<c0>", "").replace("</c0>", "")));
            let place = match (str_of(src, &["TeamName"]), str_of(src, &["ChannelName"])) {
                (Some(team), Some(channel)) => format!("{channel} · {team}"),
                (_, Some(channel)) => channel.to_string(),
                _ => str_of(src, &["Subject", "DisplayTo"]).unwrap_or("").to_string(),
            };
            Some(SearchHit { conversation, message, author, at, text, place })
        })
        .collect()
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

    #[test]
    fn reads_status() {
        let set = json!({"presence": {"availability": "Offline", "activity": "OffWork",
            "forcedAvailability": {"availability": "Offline", "activity": "OffWork", "expiry": "9999-12-31T23:59:59.9999999Z"}}});
        assert_eq!(status(&set), Status { shown: Availability::Offline, set: Some(Availability::Offline) });
        let ended =
            json!({"presence": {"availability": "Away", "forcedAvailability": {"availability": "Busy", "expiry": "2020-01-01T00:00:00Z"}}});
        assert_eq!(status(&ended), Status { shown: Availability::Away, set: None });
        assert_eq!(status(&json!({"presence": {"availability": "AvailableIdle"}})).shown, Availability::Available);
        let people = json!([
            {"mri": "8:orgid:a", "presence": {"availability": "Busy", "calendarData": {"isOutOfOffice": true}}},
            {"mri": "8:orgid:b", "presence": {"availability": "PresenceUnknown"}},
        ]);
        assert_eq!(
            presences(&people),
            vec![
                ("8:orgid:a".into(), Presence { availability: Availability::Busy, out_of_office: true }),
                ("8:orgid:b".into(), Presence { availability: Availability::Unknown, out_of_office: false }),
            ]
        );
    }

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
    fn roster_names_and_members() {
        let roster = json!({"chats": [
            {"id": "19:a", "title": "Release crew", "isFavorite": true, "members": [{"mri": ME}, {"mri": "8:orgid:maya"}]},
            {"id": "19:b", "isMuted": true, "members": [{"mri": ME}, {"mri": "8:orgid:tom"}]}]});
        let map = roster_chats(&roster);
        assert_eq!(map["19:a"].title, "Release crew");
        assert!(map["19:a"].favorite && map["19:b"].muted);
        assert_eq!(map["19:b"].members, [ME, "8:orgid:tom"]);
    }

    #[test]
    fn classifies_and_tells_unread() {
        let v = json!({"conversations": [
            {"id": "19:x@thread.tacv2", "threadProperties": {"productThreadType": "TeamsStandardChannel", "topic": "PR Reviews"}, "properties": {"consumptionhorizon": "100;100;100", "favorite": "true"}, "lastMessage": {"id": "200", "composetime": "2026-10-30T09:00:00Z"}},
            {"id": "19:y", "threadProperties": {"productThreadType": "OneToOneChat"}, "properties": {"consumptionhorizon": "300;300;300"}, "lastMessage": {"id": "200"}},
            {"id": "19:m", "threadProperties": {"productThreadType": "Meeting", "meetingInfo": "{\"meetingJoinUrl\":\"https://teams/x\"}"}},
            {"id": "48:mentions", "threadProperties": {"productThreadType": "StreamOfMentions"}}]});
        let c = conversations(&v);
        assert_eq!((c[0].kind, c[0].unread, c[0].title.as_str(), c[0].pinned), (Kind::Channel, true, "PR Reviews", true));
        assert_eq!((c[1].kind, c[1].unread), (Kind::Chat, false));
        assert_eq!((c[2].kind, c[2].join_url.as_deref()), (Kind::Meeting, Some("https://teams/x")));
        assert_eq!(c[3].kind, Kind::Feed);
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
    fn activity_items_name_who_acted() {
        let v = json!({"messages": [{"id": "9", "messagetype": "RichText/Html", "content": "<p>please take over</p>", "imdisplayname": "",
            "from": "8:orgid:me", "originalarrivaltime": "2026-10-30T10:00:00Z",
            "properties": {"activity": {"sourceUserId": "8:orgid:boris", "sourceUserImDisplayName": "Boris", "sourceThreadId": "19:x",
                "sourceThreadTopic": "[DEV] General", "activityType": "mention"}}}]});
        let m = messages(&v, ME).remove(0);
        assert_eq!(m.author, Author { id: "8:orgid:boris".into(), name: "Boris".into() });
        assert_eq!(
            m.source,
            Some(Source { conversation: "19:x".into(), message: None, root: None, title: "[DEV] General".into(), what: "mention".into() })
        );
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
        assert_eq!(card.detail, "10500: manual trigger sends the picked rule again");
        assert_eq!(card.facts, [("Target branch".to_string(), "main".to_string())]);
        assert_eq!(card.link.as_deref(), Some("https://example.com/pr/10500"));
    }

    #[test]
    fn reads_connector_cards() {
        use crate::model::text::Block;
        let card = json!({"summary": "Lea created a pull request", "attachments": [{"contentType": "application/vnd.microsoft.card.adaptive", "content": {
            "type": "AdaptiveCard", "body": [
                {"type": "ColumnSet", "columns": [{"type": "Column", "items": [{"type": "Image", "url": "https://x/icon.png"}]},
                    {"type": "Column", "items": [
                        {"type": "TextBlock", "size": "medium", "weight": "bolder", "text": "[10499: feat(timelines): pan by dragging](https://dev.example.com/pr/10499)"},
                        {"type": "TextBlock", "size": "small", "isSubtle": true, "text": "Active | pm-ui"}]}]},
                {"type": "Container", "items": [{"type": "FactSet", "facts": [{"title": "Target branch:", "value": "develop"}, {"title": "Source branch:", "value": "feature/pan"}]}]}]}}]});
        let b64 = {
            const A: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
            let bytes = card.to_string().into_bytes();
            bytes
                .chunks(3)
                .map(|c| {
                    let n = (u32::from(c[0]) << 16) | (u32::from(*c.get(1).unwrap_or(&0)) << 8) | u32::from(*c.get(2).unwrap_or(&0));
                    (0..=c.len()).map(|i| A[(n >> (18 - 6 * i) & 63) as usize] as char).collect::<String>()
                })
                .collect::<String>()
        };
        let content = format!(
            "<URIObject type=\"SWIFT.1\"><Title>Lea created a pull request</Title><Swift b64=\"{b64}\"/><Description>Lea created a pull request</Description></URIObject>"
        );
        let v = json!({"messages": [{"id": "1", "messagetype": "RichText/Media_Card", "content": content, "originalarrivaltime": "2026-10-30T10:42:00Z", "imdisplayname": "Azure Repos"}]});
        let m = messages(&v, ME).remove(0);
        assert_eq!(
            m.body,
            vec![Block::Text(
                text::plain("Lea created a pull request")
                    .into_iter()
                    .flat_map(|b| match b {
                        Block::Text(r) => r,
                        _ => vec![],
                    })
                    .collect()
            )]
        );
        let card = m.card.unwrap();
        assert_eq!(card.title, "10499: feat(timelines): pan by dragging");
        assert_eq!(card.link.as_deref(), Some("https://dev.example.com/pr/10499"));
        assert_eq!(card.detail, "Active | pm-ui");
        assert_eq!(card.facts.len(), 2);
    }

    #[test]
    fn reads_read_positions() {
        let v = json!({"consumptionhorizons": [{"id": "8:orgid:maya", "consumptionhorizon": "1793304000000;1793304001000;123"}, {"id": "8:orgid:x"}]});
        assert_eq!(read_positions(&v), [("8:orgid:maya".to_string(), 1_793_304_000_000)]);
    }

    #[test]
    fn reads_the_calendar() {
        let v = json!({"value": [
            {"objectId": "b", "subject": "Release review", "startTime": "2026-10-30T14:00:00Z", "endTime": "2026-10-30T14:30:00Z", "organizerName": "Priya Nair"},
            {"objectId": "a", "subject": "Daily stand-up", "startTime": "2026-10-30T09:00:00Z", "endTime": "2026-10-30T09:15:00Z",
             "skypeTeamsMeetingUrl": "https://teams.microsoft.com/l/meetup-join/x", "skypeTeamsDataObject": {"cid": "19:meeting_x@thread.v2"}, "myResponseType": "Accepted"}]});
        let m = meetings(&v);
        assert_eq!(m.iter().map(|m| m.subject.as_str()).collect::<Vec<_>>(), ["Daily stand-up", "Release review"]);
        assert_eq!(m[0].join_url.as_deref(), Some("https://teams.microsoft.com/l/meetup-join/x"));
        assert_eq!(m[0].chat.as_deref(), Some("19:meeting_x@thread.v2"));
        assert_eq!(m[1].end - m[1].start, 30 * 60 * 1000);
    }

    #[test]
    fn reads_search_hits() {
        let v = json!({"EntitySets": [{"ResultSets": [{"Results": [{"HitHighlightedSummary": "can you check the <c0>release</c0> notes",
            "Source": {"ClientConversationId": "19:x@thread.v2", "InternetMessageId": "1793304000000", "DateTimeSent": "2026-10-29T20:00:00Z",
                "From": {"EmailAddress": {"Name": "Priya Nair"}}, "ChannelName": "General", "TeamName": "Halcyon"}}]}]}]});
        assert_eq!(
            search_hits(&v),
            [SearchHit {
                conversation: "19:x@thread.v2".into(),
                message: "1793304000000".into(),
                author: "Priya Nair".into(),
                at: 1_793_304_000_000,
                text: "can you check the release notes".into(),
                place: "General · Halcyon".into(),
            }]
        );
    }

    #[test]
    fn reads_profiles() {
        let v = json!({"value": [{"mri": "8:orgid:maya", "displayName": "Maya Lindqvist"}, {"mri": "8:orgid:x"}]});
        assert_eq!(profiles(&v), [("8:orgid:maya".to_string(), "Maya Lindqvist".to_string())]);
    }
}
