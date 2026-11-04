//! Taking a 1:1 call: the signalling Teams on the web does, from the call notice to
//! the end of the call. Media (the SDP and what flows) is the caller's business.
//!
//! A notice (event 107 through Trouter) has an `attach` link. Attaching says which
//! of drift's links the call controller is to tell about the call, and answers with
//! the call leg's own links and the caller's offer. Then it's accepted (with the SDP
//! answer) or declined (the call leg deleted). The controller's news (the call ended,
//! was answered elsewhere) comes back through Trouter, to the links given.

use super::Teams;
use super::http::{self, Body, Method};
use crate::backend::{Error, Push, Relay, Result};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::Mutex;

/// The relays' host, and their ports (UDP, TCP, TLS), as Teams on the web has them.
const RELAY_HOST: &str = "worldaz.relay.teams.microsoft.com";
const RELAY_PORTS: (u16, u16, u16) = (3478, 443, 443);
/// Where the relays' credentials come from, and the realm Teams' are in.
const TRAP_TOKENS: &str = "https://teams.microsoft.com/trap/tokens";
const REALM: &str = "rtcmedia";
const SDP: &str = "application/sdp-ngc-1.0";
/// What drift says it can do: nothing beyond a plain 1:1 call.
const CAPABILITIES: u64 = 0;
/// The call controller's reasons for an end.
const DECLINED: u64 = 603;
const ENDED: u64 = 0;
const LOCAL_USER: &str = "CallEndReasonLocalUserInitiated";

/// Calls heard of, by id, and where the controller reaches drift.
#[derive(Default)]
pub struct Calls {
    legs: Mutex<HashMap<String, Leg>>,
    /// The Trouter socket's address (the links the controller calls are under it).
    channel: Mutex<Option<String>>,
}

struct Leg {
    /// The notice's details (`cp`).
    notice: Value,
    /// This call's part of drift's links: `…/callAgent/<session>/`.
    session: String,
    /// The call leg's links, once attached.
    links: Option<Value>,
    /// The caller's offer (SDP), from the notice or the attach.
    offer: Option<String>,
}

impl Calls {
    fn legs(&self) -> std::sync::MutexGuard<'_, HashMap<String, Leg>> {
        self.legs.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The Trouter socket is up at `surl`.
    pub fn connected(&self, surl: &str) {
        *self.channel.lock().unwrap_or_else(|e| e.into_inner()) = Some(surl.to_string());
    }

    /// A call notice came: keep what it says, to attach to it.
    pub fn heard(&self, call: &str, notice: Value) {
        let mut legs = self.legs();
        // Those long over go (a call rings under a minute).
        if legs.len() > 16 {
            legs.clear();
        }
        let offer = offer_of(&notice);
        legs.entry(call.to_string()).or_insert(Leg { notice, session: super::trouter::uuid(), links: None, offer });
    }

    /// What a request to one of drift's call links says, if it ended a call.
    pub fn callback(&self, url: &str, body: &Value) -> Option<Push> {
        let after = url.split_once("/callAgent/")?.1;
        let (session, path) = after.split_once('/')?;
        // `<cause>/call/end/` and the like.
        let what = path.split_once('/').map_or(path, |(_, rest)| rest);
        log_debug!("calling: controller says {what}");
        let ended = what.starts_with("call/end") || what.starts_with("conversation/conversationEnd");
        if !ended {
            return None;
        }
        let mut legs = self.legs();
        let id = legs.iter().find(|(_, leg)| leg.session == session).map(|(id, _)| id.clone())?;
        legs.remove(&id);
        let end = &body["callEnd"];
        log_info!("calling: the call ended ({} {})", end["code"], end["subCode"]);
        Some(Push::CallEnded { id })
    }

    /// One of drift's links for a call: `<socket>callAgent/<session>/<cause><path>`.
    fn link(&self, session: &str, path: &str) -> Result<String> {
        let channel = self.channel.lock().unwrap_or_else(|e| e.into_inner()).clone();
        let channel = channel.ok_or_else(|| Error::Network("not connected for calls".into()))?;
        let base = if channel.ends_with('/') { channel } else { format!("{channel}/") };
        Ok(format!("{base}callAgent/{session}/{}{path}", super::trouter::uuid()))
    }
}

impl Teams {
    /// Who the user is to the call controller.
    fn participant(&self, notice: &Value) -> Value {
        json!({
            "id": self.my_id(),
            "displayName": self.session().identity().map(|i| i.name.clone()).unwrap_or_default(),
            "endpointId": super::trouter::endpoint_id(),
            "participantId": notice["callNotification"]["to"]["participantId"],
            "languageId": "en-US",
        })
    }

    /// A request to the call controller, with the headers Teams on the web sends.
    fn controller(&self, method: Method, url: &str, call: &str, body: &Value) -> Result<Value> {
        let skype = self.session().skype()?;
        let headers = [
            ("X-Skypetoken", skype.token.as_str()),
            ("X-MS-Migration", "True"),
            ("X-Microsoft-Skype-Chain-ID", call),
            ("Content-Type", "application/json"),
        ];
        http::request(method, url, &headers, Body::Json(body))
    }

    /// Attach to a call, once: its links and the offer come back.
    fn attach(&self, call: &str) -> Result<()> {
        let (notice, session) = {
            let legs = self.calls.legs();
            let leg = legs.get(call).ok_or_else(|| Error::Service("that call is over".into()))?;
            if leg.links.is_some() {
                return Ok(());
            }
            (leg.notice.clone(), leg.session.clone())
        };
        let url =
            notice["callNotification"]["links"]["attach"].as_str().ok_or_else(|| Error::Service("the call can't be attached to".into()))?;
        let link = |path: &str| self.calls.link(&session, path);
        let has_offer = self.calls.legs().get(call).is_some_and(|l| l.offer.is_some());
        let body = json!({
            "attach": {
                "requireMediaContent": !has_offer,
                "links": {"end": link("/call/end/")?},
                "locationContent": null, "networkContent": null, "areaContent": null,
            },
            "capabilities": null,
            "endpointCapabilities": CAPABILITIES,
            "additionalActions": [{
                "input": {
                    "capabilities": null,
                    "endpointCapabilities": CAPABILITIES,
                    "conversationRequest": {
                        "roster": {"type": "Delta", "rosterUpdate": link("/conversation/rosterUpdate/")?},
                        "links": {
                            "conversationEnd": link("/conversation/conversationEnd/")?,
                            "conversationUpdate": link("/conversation/conversationUpdate/")?,
                            "localParticipantUpdate": link("/conversation/localParticipantUpdate/")?,
                            "addParticipantSuccess": link("/conversation/addParticipantSuccess/")?,
                            "addParticipantFailure": link("/conversation/addParticipantFailure/")?,
                            "receiveMessage": link("/conversation/receiveMessage/")?,
                        },
                    },
                    "participants": {"from": self.participant(&notice)},
                },
                "name": "join",
                "url": notice["conversationInvitation"]["conversationController"],
                "waitForResponse": true,
            }],
        });
        let answer = self.controller(Method::Post, url, call, &body)?;
        for line in super::call_shape::shape(&answer) {
            log_debug!("calling: attach answer {line}");
        }
        let invitation = &answer["callInvitation"];
        let links = invitation["links"].clone();
        if !links.is_object() {
            return Err(Error::Service("the attach answer has no call links".into()));
        }
        // The controller is told it rings here too (as Teams on the web does).
        if let Some(progress) = links["progress"].as_str() {
            let body = json!({"callProgress": {"sender": self.participant(&notice), "status": "ringing", "phrase": "ringing"}});
            if let Err(e) = self.controller(Method::Post, progress, call, &body) {
                log_debug!("calling: progress: {e}");
            }
        }
        let mut legs = self.calls.legs();
        if let Some(leg) = legs.get_mut(call) {
            leg.offer = leg.offer.take().or_else(|| invitation["mediaContent"]["blob"].as_str().map(str::to_string));
            leg.links = Some(links);
        }
        Ok(())
    }

    fn leg_link(&self, call: &str, name: &str) -> Result<String> {
        let legs = self.calls.legs();
        let leg = legs.get(call).ok_or_else(|| Error::Service("that call is over".into()))?;
        leg.links
            .as_ref()
            .and_then(|l| l[name].as_str())
            .map(str::to_string)
            .ok_or_else(|| Error::Service(format!("the call has no {name} link")))
    }

    /// Delete the call leg: a decline before it's answered, a hang-up after.
    fn end_leg(&self, call: &str, code: u64) -> Result<()> {
        let url = self.leg_link(call, "callLeg")?;
        let body = json!({"callEnd": {"code": code, "subCode": 0, "phrase": LOCAL_USER, "resultCategories": ["Success"]}});
        let ended = self.controller(Method::Delete, &url, call, &body).map(|_| ());
        self.calls.legs().remove(call);
        ended
    }

    pub(super) fn decline(&self, call: &str) -> Result<()> {
        self.attach(call)?;
        self.end_leg(call, DECLINED)
    }

    pub(super) fn offer(&self, call: &str) -> Result<String> {
        self.attach(call)?;
        let legs = self.calls.legs();
        legs.get(call).and_then(|l| l.offer.clone()).ok_or_else(|| Error::Service("the call came with no offer".into()))
    }

    pub(super) fn accept(&self, call: &str, answer: &str) -> Result<()> {
        let url = self.leg_link(call, "acceptance")?;
        let (notice, session) = {
            let legs = self.calls.legs();
            let leg = legs.get(call).ok_or_else(|| Error::Service("that call is over".into()))?;
            (leg.notice.clone(), leg.session.clone())
        };
        let link = |path: &str| self.calls.link(&session, path);
        let body = json!({"callAcceptance": {
            "acceptedBy": self.participant(&notice),
            "acceptedCallModalities": ["Audio"],
            "capabilities": null,
            "endpointCapabilities": CAPABILITIES,
            "clientEndpointCapabilities": null,
            "links": {
                "mediaRenegotiation": link("/call/mediaRenegotiation/")?,
                "transfer": link("/call/transfer/")?,
                "replacement": link("/call/replacement/")?,
                "balanceUpdate": link("/call/balanceUpdate/")?,
                "retargetCompletion": link("/call/retargetCompletion/")?,
                "controlVideoStreaming": link("/call/controlVideoStreaming/")?,
                "updateMediaDescriptions": link("/call/updateMediaDescriptions")?,
            },
            "mediaContent": {"blob": answer, "contentType": SDP},
            "callKeepAliveInterval": null,
        }});
        let answered = self.controller(Method::Post, &url, call, &body)?;
        for line in super::call_shape::shape(&answered) {
            log_debug!("calling: acceptance answer {line}");
        }
        Ok(())
    }

    pub(super) fn hang_up(&self, call: &str) -> Result<()> {
        self.end_leg(call, ENDED)
    }

    /// The relays media can go through, with credentials good for a while.
    pub(super) fn relays(&self) -> Result<Vec<Relay>> {
        let skype = self.session().skype()?;
        let answer = http::request(Method::Get, TRAP_TOKENS, &[("X-Skypetoken", &skype.token)], Body::None)?;
        relays_of(&answer)
    }
}

/// The offer a notice may already carry.
fn offer_of(notice: &Value) -> Option<String> {
    let content = &notice["callNotification"]["mediaContent"];
    content["blob"].as_str().filter(|s| s.starts_with("v=0")).map(str::to_string)
}

/// TURN servers from the token service's answer: `{tokens: [{realm, username,
/// password}]}`, or one token on its own.
fn relays_of(answer: &Value) -> Result<Vec<Relay>> {
    let tokens = match answer["tokens"].as_array() {
        Some(tokens) => tokens.clone(),
        None => vec![answer.clone()],
    };
    let unquoted = |v: &Value| v.as_str().unwrap_or("").trim_matches('"').to_string();
    let token = tokens
        .iter()
        .find(|t| unquoted(&t["realm"]) == REALM)
        .or_else(|| tokens.first())
        .filter(|t| t["username"].is_string() && t["password"].is_string())
        .ok_or_else(|| Error::Service("no relay credentials".into()))?;
    let (udp, tcp, tls) = RELAY_PORTS;
    Ok(vec![Relay {
        urls: vec![
            format!("turn:{RELAY_HOST}:{udp}?transport=udp"),
            format!("turn:{RELAY_HOST}:{tcp}?transport=tcp"),
            format!("turns:{RELAY_HOST}:{tls}"),
        ],
        username: unquoted(&token["username"]),
        credential: unquoted(&token["password"]),
    }])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_relay_tokens() {
        let answer = json!({"expires": 3600, "tokens": [
            {"realm": "\"other\"", "username": "u0", "password": "p0"},
            {"realm": "\"rtcmedia\"", "username": "u1", "password": "p1"}]});
        let relays = relays_of(&answer).unwrap();
        assert_eq!((relays[0].username.as_str(), relays[0].credential.as_str()), ("u1", "p1"));
        assert!(relays[0].urls.iter().any(|u| u.starts_with("turns:")));
        assert_eq!(relays_of(&json!({"realm": "rtcmedia", "username": "a", "password": "b"})).unwrap()[0].username, "a");
        assert!(relays_of(&json!({})).is_err());
    }

    #[test]
    fn hears_a_call_end() {
        let calls = Calls::default();
        calls.connected("https://pub.trouter.example/v4/f/abc/");
        calls.heard("call-1", json!({"callNotification": {"links": {"attach": "https://cc.example/attach"}}}));
        let session = calls.legs()["call-1"].session.clone();
        let link = calls.link(&session, "/call/end/").unwrap();
        assert!(link.starts_with(&format!("https://pub.trouter.example/v4/f/abc/callAgent/{session}/")) && link.ends_with("/call/end/"));
        assert_eq!(calls.callback(&link.replace("/call/end/", "/conversation/rosterUpdate/"), &Value::Null), None);
        assert_eq!(calls.callback(&link, &json!({"callEnd": {"code": 487}})), Some(Push::CallEnded { id: "call-1".into() }));
        // Gone once ended.
        assert_eq!(calls.callback(&link, &Value::Null), None);
        assert_eq!(calls.callback("https://x/v4/f/abc/messaging", &Value::Null), None);
    }
}
