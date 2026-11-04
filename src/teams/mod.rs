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
mod trouter;

use crate::backend::{Backend, Error, FilePlace, Push, Result};
use crate::model::markdown::Outgoing;
use crate::model::{
    Availability, Conversation, CustomEmoji, Directory, Kind, Meeting, Message, Picture, Presence, SearchHit, Status, Upload,
};
use auth::{CHAT_AGGREGATOR, GRAPH, PRESENCE, SEARCH, SPACES, Session};
use http::{Body, Method};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// AMS only takes requests from a client calling itself this.
const AMS_AGENT: &str = "SkypeOfficialClient/0/0.0.0.0";
/// The client version the presence service is told.
const PRESENCE_CLIENT_VERSION: &str = "49/25113001312";
/// Messages fetched when a conversation opens.
const PAGE: usize = 200;
/// The kinds of conversation the chat service lists.
const TARGETS: &str = "targetType=Passport|Skype|Lync|Thread|PSTN|Agent";

pub struct Teams {
    session: Mutex<Session>,
    /// Display names by id, filled as people are looked up.
    names: Mutex<HashMap<String, String>>,
    /// The user's OneDrive (its site), once looked up.
    my_site: Mutex<Option<String>>,
}

impl Teams {
    pub fn new(session: Session) -> Teams {
        Teams { session: Mutex::new(session), names: Mutex::new(HashMap::new()), my_site: Mutex::new(None) }
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

    /// A file from AMS (an image in a message, a tenant's emoji), with the skype
    /// token where AMS takes it.
    fn ams_get(&self, url: &str) -> Result<Vec<u8>> {
        if !is_ams(url) {
            return http::fetch(url, &[]);
        }
        let token = format!("skype_token {}", self.session().skype()?.token);
        http::fetch(
            url,
            &[
                ("Authorization", &token),
                ("User-Agent", AMS_AGENT),
                ("X-Ms-Client-Type", "SkypeSpacesWeb"),
                ("X-Ms-Client-Version", "1.0.0.0"),
            ],
        )
    }

    /// A request to the presence service, with its bearer and headers.
    fn presence_request(&self, method: Method, path: &str, body: Body) -> Result<Value> {
        let (skype, bearer) = {
            let mut session = self.session();
            (session.skype()?, format!("Bearer {}", session.access(PRESENCE)?))
        };
        let endpoint = trouter::endpoint_id();
        let headers = [
            ("Authorization", bearer.as_str()),
            ("X-Skypetoken", skype.token.as_str()),
            ("Accept", "application/json"),
            ("x-ms-client-user-agent", "Teams-V2-Desktop"),
            ("x-ms-correlation-id", "1"),
            ("x-ms-client-version", PRESENCE_CLIENT_VERSION),
            ("x-ms-endpoint-id", endpoint.as_str()),
        ];
        http::request(method, &format!("{}{path}", skype.presence), &headers, body)
    }

    /// The user's own presence entry, as the presence service gives it.
    fn status_raw(&self) -> Result<Value> {
        // The id is known once a token has been fetched.
        self.session().access(PRESENCE)?;
        let body = serde_json::json!([{ "mri": self.my_id() }]);
        let answer = self.presence_request(Method::Post, "/v1/presence/getpresence/", Body::Json(&body))?;
        Ok(answer.as_array().and_then(|a| a.first()).cloned().unwrap_or(Value::Null))
    }

    /// The body of a send or an edit: the HTML, and who it mentions.
    fn message_body(&self, message: &Outgoing) -> Value {
        let mut body = serde_json::json!({
            "clientmessageid": client_message_id(),
            "content": message.html,
            "messagetype": "RichText/Html",
            "contenttype": "text",
            "imdisplayname": Backend::me(self),
        });
        if !message.files.is_empty() {
            body["properties"]["files"] = Value::String(file_cards(&message.files).to_string());
        }
        if !message.mentions.is_empty() {
            let mentions: Vec<Value> = message
                .mentions
                .iter()
                .enumerate()
                .map(|(i, m)| {
                    serde_json::json!({"@type": "http://schema.skype.com/Mention", "itemid": i, "mri": m.id, "mentionType": "person", "displayName": m.name})
                })
                .collect();
            body["properties"]["mentions"] = Value::String(Value::Array(mentions).to_string());
        }
        body
    }

    /// A Microsoft Graph request (`path` after `/v1.0`).
    fn graph(&self, method: Method, path: &str, body: Body) -> Result<Value> {
        let bearer = format!("Bearer {}", self.session().access(GRAPH)?);
        http::upload(
            method,
            &format!("https://graph.microsoft.com/v1.0{path}"),
            &[("Authorization", &bearer), ("Accept", "application/json")],
            body,
        )
    }

    /// The user's OneDrive site (`https://<tenant>-my.sharepoint.com/personal/<user>`).
    fn my_site(&self) -> Result<String> {
        if let Some(site) = self.my_site.lock().unwrap_or_else(|e| e.into_inner()).clone() {
            return Ok(site);
        }
        let me = self.graph(Method::Get, "/me?$select=mySite", Body::None)?;
        let site = me["mySite"].as_str().filter(|s| !s.is_empty()).ok_or_else(|| Error::Service("this account has no OneDrive".into()))?;
        let site = site.trim_end_matches('/').to_string();
        *self.my_site.lock().unwrap_or_else(|e| e.into_inner()) = Some(site.clone());
        Ok(site)
    }

    /// A chat's file: into the user's OneDrive (where Teams keeps them, "Microsoft
    /// Teams Chat Files"), then shared with the chat's other members to read.
    fn upload_to_onedrive(&self, members: &[String], name: &str, bytes: &[u8]) -> Result<Upload> {
        let site = self.my_site()?;
        let host = url::Url::parse(&site)
            .ok()
            .and_then(|u| u.host_str().map(str::to_string))
            .ok_or_else(|| Error::Service("no OneDrive address".into()))?;
        let bearer = format!("Bearer {}", self.session().access(&format!("https://{host}"))?);
        let headers = [("Authorization", bearer.as_str()), ("Accept", "application/json")];
        let path = format!("{}/{}", http::encode(CHAT_FILES), http::encode(&onedrive_name(name)));
        let put = format!("{site}/_api/v2.0/drive/root:/{path}:/content?@name.conflictBehavior=rename&$select=*,sharepointIds");
        let item = http::upload(Method::Put, &put, &headers, Body::Bytes(bytes, "application/octet-stream"))?;
        let mut upload = uploaded(&item, &site)?;
        let others: Vec<Value> = members
            .iter()
            .filter_map(|m| m.strip_prefix("8:orgid:"))
            .filter(|oid| format!("8:orgid:{oid}") != self.my_id())
            .map(|oid| serde_json::json!({"objectId": oid}))
            .collect();
        if !others.is_empty() {
            // Shared quietly (no e-mail), as Teams does: the message is the notice.
            let invite = serde_json::json!({"recipients": others, "roles": ["read"], "requireSignIn": true, "sendInvitation": false});
            let url = format!("{site}/_api/v2.0/sites/root/items/{}/driveItem/invite", upload.item_id);
            let granted = http::request(Method::Post, &url, &headers, Body::Json(&invite))?;
            let first = &granted["value"][0];
            upload.share_id = first["id"].as_str().unwrap_or("").to_string();
            upload.share_url = first["link"]["webUrl"].as_str().unwrap_or("").to_string();
        }
        Ok(upload)
    }

    /// A channel's file: into the channel's folder in the team's SharePoint, which the
    /// team can read already.
    fn upload_to_channel(&self, group: &str, channel: &str, name: &str, bytes: &[u8]) -> Result<Upload> {
        let folder =
            self.graph(Method::Get, &format!("/teams/{}/channels/{}/filesFolder", http::encode(group), http::encode(channel)), Body::None)?;
        let (drive, id) = (folder["parentReference"]["driveId"].as_str().unwrap_or(""), folder["id"].as_str().unwrap_or(""));
        if drive.is_empty() || id.is_empty() {
            return Err(Error::Service("the channel has no files folder".into()));
        }
        let put =
            format!("/drives/{drive}/items/{id}:/{}:/content?@microsoft.graph.conflictBehavior=rename", http::encode(&onedrive_name(name)));
        let item = self.graph(Method::Put, &put, Body::Bytes(bytes, "application/octet-stream"))?;
        let item_id = item["id"].as_str().unwrap_or("");
        let item = self.graph(Method::Get, &format!("/drives/{drive}/items/{item_id}?$select=id,name,webUrl,sharepointIds"), Body::None)?;
        let site = item["sharepointIds"]["siteUrl"].as_str().unwrap_or("").trim_end_matches('/').to_string();
        uploaded(&item, &site)
    }

    /// The calendar between two times (Unix ms), as the middle tier gives it.
    fn calendar_raw(&self, from: i64, to: i64) -> Result<Value> {
        let (skype, bearer) = {
            let mut session = self.session();
            (session.skype()?, format!("Bearer {}", session.access(SPACES)?))
        };
        let iso = |ms: i64| {
            let c = crate::model::time::civil(ms / 1000);
            format!("{:04}-{:02}-{:02}T{:02}:{:02}:00.000Z", c.year, c.month, c.day, c.hour, c.minute)
        };
        let url = format!(
            "{}/v2.0/me/calendars/default/calendarView?StartDate={}&EndDate={}&shouldDecryptData=true",
            skype.middle_tier,
            http::encode(&iso(from)),
            http::encode(&iso(to))
        );
        http::request(Method::Get, &url, &[("Authorization", &bearer), ("X-Skypetoken", &skype.token)], Body::None)
    }

    /// Search Teams messages (Microsoft 365 search, as Teams' own search box does).
    fn search_raw(&self, query: &str) -> Result<Value> {
        let bearer = format!("Bearer {}", self.session().access(SEARCH)?);
        let id = || {
            format!(
                "{:08x}-{:04x}-4{:03x}-a{:03x}-{:012x}",
                fastrand::u32(..),
                fastrand::u16(..),
                fastrand::u16(..0xfff),
                fastrand::u16(..0xfff),
                fastrand::u64(..0xffff_ffff_ffff)
            )
        };
        let body = serde_json::json!({
            "entityRequests": [{
                "entityType": "Message",
                "contentSources": ["Teams"],
                "propertySet": "Optimized",
                "query": {"queryString": query, "displayQueryString": query},
                "from": 0,
                "size": 25,
                "topResultsCount": 5
            }],
            "cvid": id(),
            "logicalId": id(),
            "scenario": {"Dimensions": [{"DimensionName": "QueryType", "DimensionValue": "Messages"}, {"DimensionName": "FormFactor", "DimensionValue": "general.web.reactSearch"}], "Name": "powerbar"}
        });
        http::request(Method::Post, "https://substrate.office.com/search/api/v2/query", &[("Authorization", &bearer)], Body::Json(&body))
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
        let mut found = Vec::new();
        for batch in ids.chunks(200) {
            let batch: Vec<&str> = batch.iter().map(|s| s.as_str()).collect();
            found.extend(parse::profiles(&self.short_profiles(&batch)?));
        }
        self.names.lock().unwrap_or_else(|e| e.into_inner()).extend(found);
        Ok(())
    }

    /// People's short profiles (name, sign-in address, …), by id (at most 200).
    fn short_profiles(&self, ids: &[&str]) -> Result<Value> {
        let (skype, bearer) = {
            let mut session = self.session();
            (session.skype()?, format!("Bearer {}", session.access(SPACES)?))
        };
        let url = format!(
            "{}/beta/users/fetchShortProfile?isMailAddress=false&canBeSmtpAddress=false&enableGuest=true&includeIBBarredUsers=true&skypeTeamsInfo=true&includeBots=true",
            skype.middle_tier
        );
        // The web client starts the list with an empty id.
        let body: Value = std::iter::once("").chain(ids.iter().copied()).collect::<Vec<_>>().into();
        http::request(Method::Post, &url, &[("Authorization", &bearer), ("X-Skypetoken", &skype.token)], Body::Json(&body))
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
        // The cards connectors post (Media_Card): their JSON, as structure.
        if let Some(channel) =
            parse::conversations(&list).into_iter().filter(|c| c.kind == Kind::Channel).max_by_key(|c| c.last.as_ref().map_or(0, |l| l.at))
        {
            let path = format!(
                "/v1/users/ME/conversations/{}/messages?view=msnp24Equivalent|supportsMessageProperties&pageSize={PAGE}&startTime=1",
                http::encode(&channel.id)
            );
            let answer = self.chat_service(&path)?;
            let swift: Vec<Value> = answer["messages"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|m| m["messagetype"].as_str() == Some("RichText/Media_Card"))
                .filter_map(|m| parse::swift(m["content"].as_str()?))
                .collect();
            out.push_str(&format!("== cards in Media_Card messages ({})\n{}", swift.len(), shape::describe(&Value::Array(swift), 12)));
            let outside: Vec<Value> = answer["messages"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|m| m["messagetype"].as_str() == Some("RichText/Media_Card"))
                .map(|m| {
                    let c = m["content"].as_str().unwrap_or("");
                    // Tag names only, to see what wraps the card.
                    let tags: Vec<String> = c
                        .split('<')
                        .skip(1)
                        .filter_map(|t| t.split([' ', '>', '/']).next())
                        .filter(|t| !t.is_empty())
                        .map(str::to_string)
                        .collect();
                    Value::from(tags.join(","))
                })
                .take(3)
                .collect();
            out.push_str(&format!("== Media_Card tags: {outside:?}\n"));
            // Where links sit in these messages: the path to each string holding one,
            // and the link's host (never the rest of it).
            fn links(v: &Value, path: String, out: &mut Vec<String>) {
                match v {
                    Value::Object(o) => o.iter().for_each(|(k, c)| links(c, format!("{path}.{k}"), out)),
                    Value::Array(a) => a.iter().enumerate().for_each(|(i, c)| links(c, format!("{path}[{i}]"), out)),
                    Value::String(s) => {
                        if let Some(at) = s.find("http") {
                            let host = s[at..].split("://").nth(1).and_then(|r| r.split(['/', '"', ')', ' ']).next()).unwrap_or("?");
                            let how = if s[..at].ends_with("](") {
                                "markdown"
                            } else if s[..at].ends_with("href=\"") {
                                "href"
                            } else {
                                "plain"
                            };
                            out.push(format!("{path} -> {host} ({how})"));
                        }
                    }
                    _ => {}
                }
            }
            if let Some(m) =
                answer["messages"].as_array().into_iter().flatten().find(|m| m["messagetype"].as_str() == Some("RichText/Media_Card"))
            {
                let mut found = Vec::new();
                let mut whole = m.clone();
                if let Some(card) = m["content"].as_str().and_then(parse::swift) {
                    whole["swift"] = card;
                }
                if let Some(c) = whole.get_mut("content") {
                    // The content outside the card: only whether and where it links.
                    let text = c.as_str().unwrap_or("").to_string();
                    let outside: String = text.split("<Swift").next().unwrap_or("").to_string()
                        + text.split("/>").skip(1).collect::<Vec<_>>().join("/>").as_str();
                    *c = Value::from(outside);
                }
                links(&whole, "m".into(), &mut found);
                out.push_str(&format!("== links in a Media_Card message:\n{}\n", found.join("\n")));
                let parsed = parse::messages(&answer, "");
                let cards: Vec<_> = parsed.iter().filter_map(|m| m.card.as_ref()).collect();
                out.push_str(&format!(
                    "== parsed cards: {}, with a link: {}\n",
                    cards.len(),
                    cards.iter().filter(|c| c.link.is_some()).count()
                ));
            }
        }
        {
            let chats: Vec<String> =
                parse::conversations(&list).into_iter().filter(|c| c.kind == Kind::Chat).take(15).map(|c| c.id).collect();
            let mut custom = None;
            for id in chats {
                let path = format!(
                    "/v1/users/ME/conversations/{}/messages?view=msnp24Equivalent|supportsMessageProperties&pageSize={PAGE}&startTime=1",
                    http::encode(&id)
                );
                if let Ok(answer) = self.chat_service(&path) {
                    custom = parse::messages(&answer, "").into_iter().flat_map(|m| m.reactions).map(|r| r.key).find(|k| k.contains(';'));
                }
                if custom.is_some() {
                    break;
                }
            }
            match custom {
                Some(key) => match self.picture(&Picture::Emoji(key)) {
                    Ok(bytes) => {
                        out.push_str(&format!("== a custom emoji: {} bytes, starts {:02x?}\n", bytes.len(), &bytes[..bytes.len().min(4)]))
                    }
                    Err(e) => out.push_str(&format!("== a custom emoji: {e}\n")),
                },
                None => out.push_str("== no custom emoji found\n"),
            }
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

        // Channels' fresh state (newest message, unread, pinned in drift) from the chat service, by id.
        let channel_state: std::collections::HashMap<String, (Option<crate::model::Preview>, bool, bool)> = listed
            .iter()
            .filter(|c| c.kind == Kind::Channel || channel_ids.contains(c.id.as_str()))
            .map(|c| (c.id.clone(), (c.last.clone(), c.unread, c.pinned)))
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
                    c.members = members
                        .iter()
                        .filter(|m| **m != me)
                        .filter_map(|m| Some(crate::model::Mention { id: m.clone(), name: names.get(m)?.clone() }))
                        .collect();
                }
                c
            })
            .collect();

        // Channels come from the roster's teams; the chat service says what's new. The
        // roster's own read flag isn't used: it stays "unread" for channels the user
        // never opened (the chat service doesn't list those), whatever happens there.
        for team in &teams {
            for channel in &team.channels {
                let (last, unread, favorite) = channel_state.get(&channel.id).cloned().unwrap_or((channel.last.clone(), false, false));
                conversations.push(Conversation {
                    id: channel.id.clone(),
                    kind: Kind::Channel,
                    title: channel.name.clone(),
                    last: last.or_else(|| channel.last.clone()),
                    unread,
                    read_up_to: None,
                    pinned: channel.pinned || favorite,
                    muted: channel.muted,
                    join_url: None,
                    picture: None,
                    members: Vec::new(),
                });
            }
        }
        // The chat with oneself, for notes: named for the user, with their picture;
        // listed even before anything's written in it.
        let my_name = names.get(&me).cloned().unwrap_or_else(|| Backend::me(self));
        let notes = format!("{my_name} (You)");
        let mine = Some(Picture::Person { id: me.clone(), name: my_name });
        match conversations.iter_mut().find(|c| c.id == parse::NOTES) {
            Some(c) => {
                c.title = notes;
                c.picture = mine;
                c.members.clear();
            }
            None => conversations.push(Conversation {
                id: parse::NOTES.to_string(),
                kind: Kind::Chat,
                title: notes,
                last: None,
                unread: false,
                read_up_to: None,
                pinned: false,
                muted: false,
                join_url: None,
                picture: mine,
                members: Vec::new(),
            }),
        }
        conversations.sort_by_key(|c| std::cmp::Reverse(c.last.as_ref().map_or(0, |l| l.at)));
        Ok(Directory { teams, conversations })
    }
    fn send(&self, conversation: &str, message: &Outgoing) -> Result<()> {
        if message.html.trim().is_empty() && message.files.is_empty() {
            return Err(Error::Service("empty message".into()));
        }
        let path = format!("/v1/users/ME/conversations/{}/messages", http::encode(conversation));
        self.chat_write(Method::Post, &path, &self.message_body(message))
    }

    fn typing(&self, conversation: &str) -> Result<()> {
        let path = format!("/v1/users/ME/conversations/{}/messages", http::encode(conversation));
        let body = serde_json::json!({
            "content": "",
            "messagetype": "Control/Typing",
            "contenttype": "Application/Message",
            "imdisplayname": Backend::me(self),
        });
        self.chat_write(Method::Post, &path, &body)
    }

    fn upload_file(&self, place: &FilePlace, name: &str, bytes: &[u8]) -> Result<Upload> {
        match place {
            FilePlace::Chat { members } => self.upload_to_onedrive(members, name, bytes),
            FilePlace::Channel { group, channel } => self.upload_to_channel(group, channel, name, bytes),
        }
    }

    fn upload_image(&self, conversation: &str, name: &str, bytes: &[u8], mime: &str) -> Result<String> {
        // Two steps: register the object (readable by the conversation), then its bytes.
        let skype = self.session().skype()?;
        let token = format!("skype_token {}", skype.token);
        let headers = [
            ("Authorization", token.as_str()),
            ("User-Agent", AMS_AGENT),
            ("X-Ms-Client-Type", "SkypeSpacesWeb"),
            ("X-Ms-Client-Version", "1.0.0.0"),
        ];
        let base = conversation.split(";messageid=").next().unwrap_or(conversation);
        let register =
            serde_json::json!({"type": "pish/image", "filename": name, "permissions": {base: ["read"]}, "sharingMode": "Inline"});
        let answer = http::request(Method::Post, &format!("{}/v1/objects", skype.ams), &headers, Body::Json(&register))?;
        let id = answer["id"].as_str().ok_or_else(|| Error::Service("no id for the upload".into()))?;
        http::request(Method::Put, &format!("{}/v1/objects/{id}/content/imgpsh", skype.ams), &headers, Body::Bytes(bytes, mime))?;
        Ok(format!("{}/v1/objects/{id}/views/imgo", skype.ams))
    }

    fn edit(&self, conversation: &str, message: &str, with: &Outgoing) -> Result<()> {
        let path = format!("/v1/users/ME/conversations/{}/messages/{message}", http::encode(conversation));
        self.chat_write(Method::Put, &path, &self.message_body(with))
    }

    fn delete(&self, conversation: &str, message: &str) -> Result<()> {
        let path = format!("/v1/users/ME/conversations/{}/messages/{message}?behavior=softDelete", http::encode(conversation));
        self.chat_write(Method::Delete, &path, &Value::Object(Default::default()))
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

    fn read_by(&self, conversation: &str) -> Result<Vec<(String, i64)>> {
        // Your own notes aren't a thread: nobody else reads them.
        if conversation.starts_with("48:") {
            return Ok(Vec::new());
        }
        Ok(parse::read_positions(&self.chat_service(&format!("/v1/threads/{}/consumptionhorizons", http::encode(conversation)))?))
    }

    fn calendar(&self, from: i64, to: i64) -> Result<Vec<Meeting>> {
        Ok(parse::meetings(&self.calendar_raw(from, to)?))
    }

    fn search(&self, query: &str) -> Result<Vec<SearchHit>> {
        Ok(parse::search_hits(&self.search_raw(query)?))
    }

    fn pin(&self, conversation: &str, pinned: bool) -> Result<()> {
        // The same property the conversation list reports as `favorite`.
        let path = format!("/v1/users/ME/conversations/{}/properties?name=favorite", http::encode(conversation));
        self.chat_write(Method::Put, &path, &serde_json::json!({"favorite": if pinned { "true" } else { "false" }}))
    }

    fn status(&self) -> Result<Status> {
        Ok(parse::status(&self.status_raw()?))
    }

    fn set_status(&self, to: Option<Availability>) -> Result<()> {
        match to {
            Some(a) => {
                let body = serde_json::json!({ "availability": a.key() });
                self.presence_request(Method::Put, "/v1/me/forceavailability/", Body::Json(&body)).map(|_| ())
            }
            None => self.presence_request(Method::Delete, "/v1/me/forceavailability/", Body::None).map(|_| ()),
        }
    }

    fn presence(&self, people: &[String]) -> Result<Vec<(String, Presence)>> {
        let mut all = Vec::new();
        // The service takes at most 650 at a time.
        for batch in people.chunks(650) {
            let body: Vec<Value> = batch.iter().map(|id| serde_json::json!({ "mri": id })).collect();
            all.extend(parse::presences(&self.presence_request(
                Method::Post,
                "/v1/presence/getpresence/",
                Body::Json(&Value::Array(body)),
            )?));
        }
        Ok(all)
    }

    fn custom_emoji(&self) -> Result<Vec<CustomEmoji>> {
        let (skype, bearer) = {
            let mut session = self.session();
            (session.skype()?, format!("Bearer {}", session.access(CHAT_AGGREGATOR)?))
        };
        let answer = http::request(
            Method::Get,
            &format!("{}/api/v1/customemoji/metadata", skype.aggregator),
            &[("Authorization", &bearer), ("X-Skypetoken", &skype.token), ("Accept", "application/json")],
            Body::None,
        )?;
        Ok(parse::custom_emoji(&answer, &skype.ams))
    }

    fn address(&self, person: &str) -> Result<Option<String>> {
        Ok(parse::address(&self.short_profiles(&[person])?, person))
    }

    fn report_activity(&self, active: bool) -> Result<()> {
        let id = trouter::endpoint_id();
        // The endpoint first (created the first time, renewed after): activity is
        // only taken from an endpoint the service knows.
        let endpoint = serde_json::json!({
            "id": id,
            "availability": "Available",
            "activity": "Available",
            "activityReporting": "Transport",
            "deviceType": "Desktop",
        });
        self.presence_request(Method::Put, "/v1/me/endpoints/", Body::Json(&endpoint))?;
        let activity = serde_json::json!({ "endpointId": id, "isActive": active });
        self.presence_request(Method::Post, "/v1/me/reportmyactivity/", Body::Json(&activity)).map(|_| ())
    }

    fn listen(self: Arc<Self>, to: Box<dyn Fn(Push) + Send>) {
        let teams = Arc::downgrade(&self);
        let spawned = std::thread::Builder::new().name("trouter".into()).spawn(move || trouter::run(teams, to));
        if let Err(e) = spawned {
            log_warn!("could not start listening for changes: {e}");
        }
    }

    fn messages(&self, conversation: &str) -> Result<Vec<Message>> {
        let path = format!(
            "/v1/users/ME/conversations/{}/messages?view=msnp24Equivalent|supportsMessageProperties&pageSize={PAGE}&startTime=1",
            http::encode(conversation)
        );
        Ok(parse::messages(&self.chat_service(&path)?, &self.my_id()))
    }

    fn picture(&self, picture: &Picture) -> Result<Vec<u8>> {
        match picture {
            // A tenant's own emoji is an object in AMS, by the id after the `;`.
            Picture::Emoji(key) if key.contains(';') => {
                let id = key.rsplit(';').next().unwrap_or("");
                let ams = self.session().skype()?.ams;
                // The animated view is a PNG (with transparency); the original is a JPEG.
                return self.ams_get(&format!("{ams}/v1/objects/{id}/views/imgt1_anim"));
            }
            Picture::Emoji(key) => return emoji(key),
            Picture::Image(url) => return self.ams_get(url),
            _ => {}
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
            Picture::Emoji(_) | Picture::Image(_) => unreachable!("fetched above"),
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

/// `s` percent-encoded, for a Teams link.
pub fn encode(s: &str) -> String {
    http::encode(s)
}

/// A meeting's join link as one that opens straight in Teams on the web (the plain
/// link stops at a page asking which app to use).
pub fn web_join(url: &str) -> String {
    match url.strip_prefix("https://teams.microsoft.com/l/") {
        Some(rest) => format!("https://teams.microsoft.com/_#/l/{rest}"),
        None => url.to_string(),
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

/// Where Teams keeps a chat's files in the sender's OneDrive.
const CHAT_FILES: &str = "Microsoft Teams Chat Files";

/// A file name OneDrive takes (it refuses some characters).
fn onedrive_name(name: &str) -> String {
    let clean: String = name.chars().map(|c| if "\"*:<>?/\\|".contains(c) || c.is_control() { '_' } else { c }).collect();
    let clean = clean.trim().trim_end_matches('.').to_string();
    if clean.is_empty() { "file".into() } else { clean }
}

/// An uploaded file as OneDrive or SharePoint answers it.
fn uploaded(item: &Value, site: &str) -> Result<Upload> {
    let ids = &item["sharepointIds"];
    let item_id = ids["listItemUniqueId"].as_str().ok_or_else(|| Error::Service("the upload has no id".into()))?;
    Ok(Upload {
        name: item["name"].as_str().unwrap_or("").to_string(),
        url: item["webUrl"].as_str().unwrap_or("").to_string(),
        site: format!("{}/", site.trim_end_matches('/')),
        site_id: ids["siteId"].as_str().unwrap_or("").to_string(),
        item_id: item_id.to_string(),
        share_url: String::new(),
        share_id: String::new(),
    })
}

/// `properties.files` of a message with files, as the Teams web client sends it.
fn file_cards(files: &[Upload]) -> Value {
    Value::Array(
        files
            .iter()
            .map(|f| {
                let kind = f.name.rsplit_once('.').map(|(_, e)| e.to_lowercase()).unwrap_or_default();
                serde_json::json!({
                    "@type": "http://schema.skype.com/File",
                    "version": 2,
                    "id": f.item_id,
                    "itemid": f.item_id,
                    "fileName": f.name,
                    "title": f.name,
                    "fileType": kind,
                    "type": kind,
                    "state": "active",
                    "baseUrl": f.site,
                    "objectUrl": f.url,
                    "providerData": "",
                    "botFileProperties": {},
                    "permissionScope": if f.share_id.is_empty() { Value::Null } else { Value::from("SpecificPeople") },
                    "fileInfo": {"fileUrl": f.url, "siteUrl": f.site, "serverRelativeUrl": "", "shareUrl": f.share_url, "shareId": f.share_id},
                    "fileChicletState": {"serviceName": "p2p", "state": "active"},
                    "filePreview": {"previewUrl": "", "previewHeight": 0, "previewWidth": 0},
                    "sharepointIds": {"listItemUniqueId": f.item_id, "siteId": f.site_id},
                })
            })
            .collect(),
    )
}

/// Whether `url` is AMS, where pictures in messages are kept and which takes the
/// skype token: HTTPS to one of its hosts, judged by the parsed host (a link in a
/// message is anyone's, and the token must go nowhere else).
fn is_ams(url: &str) -> bool {
    let Ok(url) = url::Url::parse(url) else { return false };
    let host = url.host_str().unwrap_or("");
    url.scheme() == "https"
        && url.username().is_empty()
        && url.password().is_none()
        && url.port().is_none()
        && (host.ends_with(".asyncgw.teams.microsoft.com")
            || host.ends_with(".asm.skype.com")
            || (host == "teams.microsoft.com" && url.path().starts_with("/api/amsMTProd/")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn files_go_as_teams_sends_them() {
        assert_eq!(onedrive_name("re: plan? <v2>.pdf"), "re_ plan_ _v2_.pdf");
        assert_eq!(onedrive_name(" .. "), "file");
        let item = json!({"name": "plan.pdf", "webUrl": "https://org-my.sharepoint.com/personal/a/Documents/plan.pdf",
            "sharepointIds": {"listItemUniqueId": "abc", "siteId": "site"}});
        let mut up = uploaded(&item, "https://org-my.sharepoint.com/personal/a").unwrap();
        (up.share_id, up.share_url) = ("s1".into(), "https://share".into());
        let cards = file_cards(&[up]);
        let card = &cards[0];
        assert_eq!(
            (card["fileName"].as_str(), card["fileType"].as_str(), card["itemid"].as_str()),
            (Some("plan.pdf"), Some("pdf"), Some("abc"))
        );
        assert_eq!(card["baseUrl"], "https://org-my.sharepoint.com/personal/a/");
        assert_eq!(card["fileInfo"]["shareId"], "s1");
        assert_eq!(card["permissionScope"], "SpecificPeople");
        // A message with only a file is still sent.
        let teams_body = json!({"properties": {"files": cards.to_string()}});
        assert!(teams_body["properties"]["files"].is_string());
        assert!(uploaded(&json!({"name": "x"}), "s").is_err());
    }

    #[test]
    fn the_token_goes_only_to_ams() {
        for ok in [
            "https://us-api.asm.skype.com/v1/objects/0-wus-d1-abc/views/imgo",
            "https://us-prod.asyncgw.teams.microsoft.com/v1/objects/0-weu-d1-abc/views/imgt1_anim",
            "https://teams.microsoft.com/api/amsMTProd/v1/objects/x/views/imgo",
        ] {
            assert!(is_ams(ok), "{ok}");
        }
        for bad in [
            "https://evil.example/api/amsMTProd/x",
            "https://evil.example?.asyncgw.teams.microsoft.com/",
            "https://evil.example#.asm.skype.com/",
            "https://x.asm.skype.com@evil.example/",
            "http://us-api.asm.skype.com/v1/objects/x",
            "https://us-api.asm.skype.com.evil.example/x",
            "https://us-api.asm.skype.com:8443/x",
            "https://teams.microsoft.com.evil.example/api/amsMTProd/x",
        ] {
            assert!(!is_ams(bad), "{bad}");
        }
    }
    use crate::model::Preview;

    #[test]
    fn joins_on_the_web() {
        assert_eq!(
            web_join("https://teams.microsoft.com/l/meetup-join/19%3ameeting_x/0?context=y"),
            "https://teams.microsoft.com/_#/l/meetup-join/19%3ameeting_x/0?context=y"
        );
        assert_eq!(web_join("https://example.com/x"), "https://example.com/x");
    }

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
            last: Some(Preview { author: "Lea".into(), author_id: String::new(), text: "hi".into(), at: 1 }),
            unread: false,
            read_up_to: None,
            pinned: false,
            muted: false,
            join_url: None,
            picture: None,
            members: Vec::new(),
        };
        let ids = |l: &[&str]| l.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(chat_title(&ids(&["8:orgid:me", "8:orgid:a"]), "8:orgid:me", &names, &chat), "Maya");
        assert_eq!(chat_title(&ids(&["8:orgid:a", "8:orgid:b", "8:orgid:c"]), "8:orgid:me", &names, &chat), "Maya, Jonas and 1 more");
        assert_eq!(chat_title(&[], "8:orgid:me", &names, &chat), "Lea");
    }
}
