//! Microsoft Teams through the services its web client uses: the chat service for
//! conversations and messages, the chat aggregator for teams, channels and chats, and
//! the middle tier for people's names.
//!
//! - [`auth`]: signing in and the tokens each service takes
//! - [`http`]: requests and answers
//! - [`parse`]: their JSON as `model` types

pub mod auth;
mod http;
mod parse;
pub mod shape;

use crate::backend::{Backend, Error, Result};
use crate::model::{Conversation, Directory, Kind, Message, Picture};
use auth::{CHAT_AGGREGATOR, SPACES, Session};
use http::{Body, Method};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Mutex;

/// Messages fetched when a conversation opens.
const PAGE: usize = 200;
/// The kinds of conversation the chat service lists.
const TARGETS: &str = "targetType=Passport|Skype|Lync|Thread|PSTN|Agent";

pub struct Teams {
    session: Mutex<Session>,
    /// Display names by id, filled as people are looked up.
    names: Mutex<HashMap<String, String>>,
}

impl Teams {
    pub fn new(session: Session) -> Teams {
        Teams { session: Mutex::new(session), names: Mutex::new(HashMap::new()) }
    }

    fn session(&self) -> std::sync::MutexGuard<'_, Session> {
        self.session.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn my_id(&self) -> String {
        self.session().identity().map(|i| i.id.clone()).unwrap_or_default()
    }

    /// GET from the chat service; a refused skype token is renewed once.
    fn chat_service(&self, path: &str) -> Result<Value> {
        for attempt in 0..2 {
            let skype = self.session().skype()?;
            let url = format!("{}{path}", skype.chat_service);
            let auth = format!("skypetoken={}", skype.token);
            let headers = [
                ("Authentication", auth.as_str()),
                ("X-Skypetoken", skype.token.as_str()),
                ("BehaviorOverride", "redirectAs404"),
                ("Accept", "application/json; ver=1.0;"),
            ];
            match http::request(Method::Get, &url, &headers, Body::None) {
                Err(Error::SignedOut) if attempt == 0 => self.session().drop_skype(),
                answer => return answer,
            }
        }
        Err(Error::SignedOut)
    }

    /// Write to the chat service (send, react, mark read); a refused skype token is
    /// renewed once.
    fn chat_write(&self, method: Method, path: &str, body: &Value) -> Result<()> {
        for attempt in 0..2 {
            let skype = self.session().skype()?;
            let url = format!("{}{path}", skype.chat_service);
            let auth = format!("skypetoken={}", skype.token);
            let headers =
                [("Authentication", auth.as_str()), ("X-Skypetoken", skype.token.as_str()), ("BehaviorOverride", "redirectAs404")];
            match http::request(method, &url, &headers, Body::Json(body)) {
                Err(Error::SignedOut) if attempt == 0 => self.session().drop_skype(),
                answer => return answer.map(|_| ()),
            }
        }
        Err(Error::SignedOut)
    }

    /// The aggregator's roster: teams with their channels, and the chats.
    fn roster(&self) -> Result<Value> {
        let (skype, bearer) = {
            let mut session = self.session();
            (session.skype()?, format!("Bearer {}", session.access(CHAT_AGGREGATOR)?))
        };
        let url = format!("{}/api/v1/teams/users/me?isPrefetch=false&enableMembershipSummary=true", skype.aggregator);
        http::request(
            Method::Get,
            &url,
            &[("Authorization", &bearer), ("X-Skypetoken", &skype.token), ("Accept", "application/json")],
            Body::None,
        )
    }

    /// Names of the people with these ids, looking up the ones not known yet.
    fn names_of(&self, ids: &[String]) -> HashMap<String, String> {
        let unknown: Vec<&String> = {
            let names = self.names.lock().unwrap_or_else(|e| e.into_inner());
            ids.iter().filter(|id| id.starts_with("8:orgid:") && !names.contains_key(*id)).collect()
        };
        if !unknown.is_empty()
            && let Err(e) = self.look_up(&unknown)
        {
            log_warn!("could not look up {} names: {e}", unknown.len());
        }
        let names = self.names.lock().unwrap_or_else(|e| e.into_inner());
        ids.iter().filter_map(|id| Some((id.clone(), names.get(id)?.clone()))).collect()
    }

    fn look_up(&self, ids: &[&String]) -> Result<()> {
        let (skype, bearer) = {
            let mut session = self.session();
            (session.skype()?, format!("Bearer {}", session.access(SPACES)?))
        };
        let url = format!(
            "{}/beta/users/fetchShortProfile?isMailAddress=false&canBeSmtpAddress=false&enableGuest=true&includeIBBarredUsers=true&skypeTeamsInfo=true&includeBots=true",
            skype.middle_tier
        );
        let mut found = Vec::new();
        for batch in ids.chunks(200) {
            // The web client starts the list with an empty id.
            let body: Value = std::iter::once("").chain(batch.iter().map(|s| s.as_str())).collect::<Vec<_>>().into();
            let answer =
                http::request(Method::Post, &url, &[("Authorization", &bearer), ("X-Skypetoken", &skype.token)], Body::Json(&body))?;
            found.extend(parse::profiles(&answer));
        }
        self.names.lock().unwrap_or_else(|e| e.into_inner()).extend(found);
        Ok(())
    }
}

impl Teams {
    /// The structure of the roster, the conversation list and the messages of the
    /// newest chat and channel (see [`shape`]).
    pub fn shape(&self) -> Result<String> {
        let roster = self.roster()?;
        let list = self.chat_service(&format!("/v1/users/ME/conversations?view=msnp24Equivalent&pageSize=500&startTime=0&{TARGETS}"))?;
        let mut out = format!("== roster\n{}\n== conversations\n{}", shape::describe(&roster, 7), shape::describe(&list, 6));
        // Counts only, to see how chats and channels divide up (no names or text).
        {
            use std::collections::BTreeMap;
            let count = |v: &serde_json::Value, path: &[&str]| -> BTreeMap<String, usize> {
                let mut by: BTreeMap<String, usize> = BTreeMap::new();
                for item in v.as_array().into_iter().flatten() {
                    let key = path
                        .iter()
                        .find_map(|k| item.get(*k).or_else(|| item["threadProperties"].get(*k)).and_then(|x| x.as_str()))
                        .unwrap_or("(none)");
                    *by.entry(key.to_string()).or_default() += 1;
                }
                by
            };
            out.push_str(&format!(
                "== roster has chats array: {}, {} entries\n",
                roster.get("chats").is_some_and(|c| c.is_array()),
                roster["chats"].as_array().map_or(0, |a| a.len())
            ));
            out.push_str(&format!("== roster chats by threadType: {:?}\n", count(&roster["chats"], &["threadType"])));
            out.push_str(&format!("== listed by productThreadType: {:?}\n", count(&list["conversations"], &["productThreadType"])));
            let with_members =
                roster["chats"].as_array().map_or(0, |a| a.iter().filter(|c| c["members"].as_array().map_or(0, |m| m.len()) > 0).count());
            out.push_str(&format!("== roster chats with members: {with_members}\n"));
            let listed_ids: std::collections::HashSet<&str> =
                list["conversations"].as_array().into_iter().flatten().filter_map(|c| c["id"].as_str()).collect();
            let roster_ids: std::collections::HashSet<&str> =
                roster["chats"].as_array().into_iter().flatten().filter_map(|c| c["id"].as_str()).collect();
            out.push_str(&format!("== listed only (not in roster chats): {}\n", listed_ids.difference(&roster_ids).count()));
        }
        if let Ok(dir) = self.directory() {
            use std::collections::BTreeMap;
            let mut by: BTreeMap<String, usize> = BTreeMap::new();
            let mut untitled = 0;
            for c in &dir.conversations {
                *by.entry(format!("{:?}", c.kind)).or_default() += 1;
                if c.title.trim().is_empty() {
                    untitled += 1;
                }
            }
            out.push_str(&format!("== directory: {} teams, conversations by kind {by:?}, untitled {untitled}\n", dir.teams.len()));
        }
        let me = Picture::Person { id: self.my_id(), name: Backend::me(self) };
        match self.picture(&me) {
            Ok(bytes) => out.push_str(&format!("== own picture: {} bytes, starts {:02x?}\n", bytes.len(), &bytes[..bytes.len().min(4)])),
            Err(e) => out.push_str(&format!("== own picture: {e}\n")),
        }
        if let Some(team) = parse::teams(&roster).into_iter().find_map(|t| t.picture) {
            match self.picture(&team) {
                Ok(bytes) => {
                    out.push_str(&format!("== a team picture: {} bytes, starts {:02x?}\n", bytes.len(), &bytes[..bytes.len().min(4)]))
                }
                Err(e) => out.push_str(&format!("== a team picture: {e}\n")),
            }
        }
        let listed = parse::conversations(&list);
        for kind in [Kind::Chat, Kind::Channel, Kind::Meeting, Kind::Feed] {
            if let Some(c) = listed.iter().filter(|c| c.kind == kind).max_by_key(|c| c.last.as_ref().map_or(0, |l| l.at)) {
                let path = format!(
                    "/v1/users/ME/conversations/{}/messages?view=msnp24Equivalent|supportsMessageProperties&pageSize={PAGE}&startTime=1",
                    http::encode(&c.id)
                );
                out.push_str(&format!(
                    "== messages of the newest {kind:?}\n{}",
                    shape::describe(&self.chat_service(&path)?, if kind == Kind::Feed { 9 } else { 7 })
                ));
            }
        }
        Ok(out)
    }
}

impl Backend for Teams {
    fn me(&self) -> String {
        self.session().identity().map(|i| i.name.clone()).unwrap_or_default()
    }

    fn me_id(&self) -> String {
        self.my_id()
    }

    fn directory(&self) -> Result<Directory> {
        let roster = self.roster()?;
        let listed = parse::conversations(
            &self.chat_service(&format!("/v1/users/ME/conversations?view=msnp24Equivalent&pageSize=500&startTime=0&{TARGETS}"))?,
        );
        let teams = parse::teams(&roster);
        let me = self.my_id();
        let channel_ids: std::collections::HashSet<&str> = teams.iter().flat_map(|t| t.channels.iter().map(|c| c.id.as_str())).collect();

        // The chat service lists every conversation, newest first, as Teams itself
        // shows them. The roster names the chats and says who's in them.
        let roster_chats = parse::roster_chats(&roster);
        let member_ids: Vec<String> = roster_chats
            .values()
            .flat_map(|c| c.members.iter())
            .filter(|m| **m != me)
            .cloned()
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();
        let names = self.names_of(&member_ids);

        // Channels' fresh state (newest message, unread) from the chat service, by id.
        let channel_state: std::collections::HashMap<String, (Option<crate::model::Preview>, bool)> = listed
            .iter()
            .filter(|c| c.kind == Kind::Channel || channel_ids.contains(c.id.as_str()))
            .map(|c| (c.id.clone(), (c.last.clone(), c.unread)))
            .collect();

        let mut conversations: Vec<Conversation> = listed
            .into_iter()
            .filter(|c| matches!(c.kind, Kind::Chat | Kind::Meeting | Kind::Feed) && !channel_ids.contains(c.id.as_str()))
            .map(|mut c| {
                let roster = roster_chats.get(&c.id);
                let members: &[String] = roster.map(|r| r.members.as_slice()).unwrap_or(&[]);
                if let Some(r) = roster {
                    c.pinned |= r.favorite;
                    c.muted |= r.muted;
                    if c.title.is_empty() {
                        c.title = r.title.clone();
                    }
                }
                if c.kind == Kind::Feed {
                    if c.title.is_empty() {
                        c.title = "Activity".to_string();
                    }
                } else {
                    if c.title.is_empty() {
                        c.title = chat_title(members, &me, &names, &c);
                    }
                    c.picture = chat_picture(members, &me, &names);
                }
                c
            })
            .collect();

        // Channels come from the roster's teams; the chat service says what's new.
        for team in &teams {
            for channel in &team.channels {
                let (last, unread) = channel_state.get(&channel.id).cloned().unwrap_or((channel.last.clone(), channel.unread));
                conversations.push(Conversation {
                    id: channel.id.clone(),
                    kind: Kind::Channel,
                    title: channel.name.clone(),
                    last: last.or_else(|| channel.last.clone()),
                    unread: unread || channel.unread,
                    pinned: channel.pinned,
                    muted: channel.muted,
                    join_url: None,
                    picture: None,
                });
            }
        }
        conversations.sort_by_key(|c| std::cmp::Reverse(c.last.as_ref().map_or(0, |l| l.at)));
        Ok(Directory { teams, conversations })
    }
    fn send(&self, conversation: &str, text: &str) -> Result<()> {
        let text = text.trim();
        if text.is_empty() {
            return Err(Error::Service("empty message".into()));
        }
        let body = serde_json::json!({
            "clientmessageid": client_message_id(),
            "content": text,
            "messagetype": "Text",
            "contenttype": "text",
            "imdisplayname": Backend::me(self),
        });
        self.chat_write(Method::Post, &format!("/v1/users/ME/conversations/{}/messages", http::encode(conversation)), &body)
    }

    fn react(&self, conversation: &str, message: &str, key: &str, add: bool) -> Result<()> {
        let path = format!("/v1/users/ME/conversations/{}/messages/{message}/properties?name=emotions", http::encode(conversation));
        let (method, body) = if add {
            (Method::Put, serde_json::json!({"emotions": {"key": key, "value": crate::model::time::now_ms()}}))
        } else {
            (Method::Delete, serde_json::json!({"emotions": {"key": key}}))
        };
        self.chat_write(method, &path, &body)
    }

    fn mark_read(&self, conversation: &str, message: &str) -> Result<()> {
        let path = format!("/v1/users/ME/conversations/{}/properties?name=consumptionhorizon", http::encode(conversation));
        let now = crate::model::time::now_ms();
        self.chat_write(Method::Put, &path, &serde_json::json!({"consumptionhorizon": format!("{message};{now};{message}")}))
    }

    fn messages(&self, conversation: &str) -> Result<Vec<Message>> {
        let path = format!(
            "/v1/users/ME/conversations/{}/messages?view=msnp24Equivalent|supportsMessageProperties&pageSize={PAGE}&startTime=1",
            http::encode(conversation)
        );
        Ok(parse::messages(&self.chat_service(&path)?, &self.my_id()))
    }

    fn picture(&self, picture: &Picture) -> Result<Vec<u8>> {
        if let Picture::Emoji(key) = picture {
            return emoji(key);
        }
        let (skype, token) = {
            let mut session = self.session();
            (session.skype()?, session.access(SPACES)?)
        };
        let me = self.my_id();
        let object_id = me.strip_prefix("8:orgid:").unwrap_or(&me);
        let base = &skype.middle_tier;
        let url = match picture {
            Picture::Person { id, name } => {
                format!("{base}/beta/users/{}/profilepicturev2?displayname={}&size=HR64x64", http::encode(id), http::encode(name))
            }
            Picture::Team { group, etag, name } => format!(
                "{base}/beta/users/{object_id}/profilepicturev2/teams/{group}?etag={}&displayName={}",
                http::encode(etag),
                http::encode(name)
            ),
            Picture::Emoji(_) => unreachable!("emoji are fetched above"),
            Picture::Group(members) => {
                let users: Value = members.iter().map(|(id, name)| serde_json::json!({"userId": id, "displayName": name})).collect();
                format!("{base}/beta/users/{object_id}/mergedProfilePicturev2?usersInfo={}&size=HR64x64", http::encode(&users.to_string()))
            }
        };
        let bearer = format!("Bearer {token}");
        http::fetch(&url, &[("Authorization", &bearer)])
    }
}

/// A chat's picture: the other person's photo, or a few members' photos together.
fn chat_picture(members: &[String], me: &str, names: &HashMap<String, String>) -> Option<Picture> {
    let others: Vec<(String, String)> =
        members.iter().filter(|m| *m != me).map(|m| (m.clone(), names.get(m).cloned().unwrap_or_default())).take(4).collect();
    match others.len() {
        0 => None,
        1 => {
            let (id, name) = others.into_iter().next()?;
            Some(Picture::Person { id, name })
        }
        _ => Some(Picture::Group(others)),
    }
}

/// An emoji's image from Teams' own set (its key: `like`, `1f440_eyes`; a skin tone
/// suffix like `-tone2` falls back to the plain one). No sign-in needed.
pub fn emoji(key: &str) -> Result<Vec<u8>> {
    let base = key.split("-tone").next().unwrap_or(key);
    // Teams' own "like" picture is a face giving a thumbs-up; the plain thumb reads better.
    let base = if base == "like" { "yes" } else { base };
    if base.is_empty() || !base.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-') {
        return Err(Error::Service("not an emoji key".into()));
    }
    let url =
        format!("https://statics.teams.cdn.office.net/evergreen-assets/personal-expressions/v2/assets/emoticons/{base}/default/50_f.png");
    http::fetch(&url, &[])
}

/// A fresh 19-digit client message id (the service dedups and echoes it back).
fn client_message_id() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_nanos() as u64);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed) % 1000;
    format!("{:019}", nanos.wrapping_add(n))
}

/// A name for a chat without a topic: its other members, or who wrote last.
fn chat_title(members: &[String], me: &str, names: &HashMap<String, String>, chat: &Conversation) -> String {
    let others: Vec<&str> = members.iter().filter(|m| *m != me).filter_map(|m| names.get(m)).map(String::as_str).collect();
    match others.as_slice() {
        [] => chat.last.as_ref().map(|l| l.author.clone()).filter(|a| !a.is_empty()).unwrap_or_else(|| "Chat".into()),
        [one] => one.to_string(),
        [a, b] => format!("{a}, {b}"),
        [a, b, rest @ ..] => format!("{a}, {b} and {} more", rest.len()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Preview;

    #[test]
    fn names_chats_after_their_members() {
        let names: HashMap<String, String> = [("8:orgid:a", "Maya"), ("8:orgid:b", "Jonas"), ("8:orgid:c", "Priya")]
            .into_iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        let chat = Conversation {
            id: "19:x".into(),
            kind: Kind::Chat,
            title: String::new(),
            last: Some(Preview { author: "Lea".into(), text: "hi".into(), at: 1 }),
            unread: false,
            pinned: false,
            muted: false,
            join_url: None,
            picture: None,
        };
        let ids = |l: &[&str]| l.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(chat_title(&ids(&["8:orgid:me", "8:orgid:a"]), "8:orgid:me", &names, &chat), "Maya");
        assert_eq!(chat_title(&ids(&["8:orgid:a", "8:orgid:b", "8:orgid:c"]), "8:orgid:me", &names, &chat), "Maya, Jonas and 1 more");
        assert_eq!(chat_title(&[], "8:orgid:me", &names, &chat), "Lea");
    }
}
