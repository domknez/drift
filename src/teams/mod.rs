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

use crate::backend::{Backend, Error, Result};
use crate::model::{Conversation, Directory, Kind, Message};
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

impl Backend for Teams {
    fn me(&self) -> String {
        self.session().identity().map(|i| i.name.clone()).unwrap_or_default()
    }

    fn directory(&self) -> Result<Directory> {
        let roster = self.roster()?;
        let listed = parse::conversations(
            &self.chat_service(&format!("/v1/users/ME/conversations?view=msnp24Equivalent&pageSize=500&startTime=0&{TARGETS}"))?,
        );
        let teams = parse::teams(&roster);
        let me = self.my_id();

        // Chats from the aggregator, which knows their members; what the chat service
        // knows (unread, newest message) on top.
        let by_id: HashMap<&str, &Conversation> = listed.iter().map(|c| (c.id.as_str(), c)).collect();
        let mut chats = parse::chats(&roster);
        if chats.is_empty() {
            chats = listed.iter().filter(|c| matches!(c.kind, Kind::Chat | Kind::Meeting)).map(|c| (c.clone(), Vec::new())).collect();
        }
        let others: Vec<String> = chats.iter().flat_map(|(_, m)| m.iter()).filter(|m| **m != me).cloned().collect();
        let names = self.names_of(&others);
        let mut conversations: Vec<Conversation> = chats
            .into_iter()
            .map(|(mut c, members)| {
                if let Some(known) = by_id.get(c.id.as_str()) {
                    c.unread |= known.unread;
                    if known.last.as_ref().map(|l| l.at) > c.last.as_ref().map(|l| l.at) {
                        c.last = known.last.clone();
                    }
                }
                if c.title.is_empty() {
                    c.title = chat_title(&members, &me, &names, &c);
                }
                c
            })
            .collect();

        // Channels: their names from the roster, their state from the chat service.
        for team in &teams {
            for channel in &team.channels {
                let known = by_id.get(channel.id.as_str());
                conversations.push(Conversation {
                    id: channel.id.clone(),
                    kind: Kind::Channel,
                    title: channel.name.clone(),
                    last: known.and_then(|k| k.last.clone()),
                    unread: known.is_some_and(|k| k.unread),
                });
            }
        }
        conversations.sort_by_key(|c| std::cmp::Reverse(c.last.as_ref().map_or(0, |l| l.at)));
        Ok(Directory { teams, conversations })
    }

    fn messages(&self, conversation: &str) -> Result<Vec<Message>> {
        let path = format!(
            "/v1/users/ME/conversations/{}/messages?view=msnp24Equivalent|supportsMessageProperties&pageSize={PAGE}&startTime=1",
            http::encode(conversation)
        );
        Ok(parse::messages(&self.chat_service(&path)?, &self.my_id()))
    }
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
        };
        let ids = |l: &[&str]| l.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(chat_title(&ids(&["8:orgid:me", "8:orgid:a"]), "8:orgid:me", &names, &chat), "Maya");
        assert_eq!(chat_title(&ids(&["8:orgid:a", "8:orgid:b", "8:orgid:c"]), "8:orgid:me", &names, &chat), "Maya, Jonas and 1 more");
        assert_eq!(chat_title(&[], "8:orgid:me", &names, &chat), "Lea");
    }
}
