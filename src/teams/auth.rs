//! Signing in the way the Teams web client does: a Microsoft sign-in in a webview
//! gives an authorization code, the code a refresh token (kept in the keychain), and
//! that refresh token the access tokens each Teams service wants, plus the skype
//! token the chat service takes.

use super::http::{self, Body, Method};
use crate::backend::{Error, Result};
use crate::model::time::now_ms;
use serde_json::Value;
use std::collections::HashMap;

/// The Teams web client's public client id.
pub const CLIENT_ID: &str = "1fec8e78-bce4-4aaf-ab1b-5451cc387264";
/// Where Microsoft sends the code; the login window catches the redirect.
pub const REDIRECT: &str = "https://login.microsoftonline.com/common/oauth2/nativeclient";
const AUTHORITY: &str = "https://login.microsoftonline.com/organizations/oauth2/v2.0";
const AUTHZ: &str = "https://teams.microsoft.com/api/authsvc/v1.0/authz";
/// The chat service's audience; the sign-in asks for it first.
pub const SPACES: &str = "https://api.spaces.skype.com";
/// The audience of Microsoft 365 search (finding messages).
pub const SEARCH: &str = "https://outlook.office.com/search";
/// The audience of the chat service aggregator (teams, channels, chats).
pub const CHAT_AGGREGATOR: &str = "https://chatsvcagg.teams.microsoft.com";
/// Microsoft Graph (the user's OneDrive, channels' files).
pub const GRAPH: &str = "https://graph.microsoft.com";
/// The audience of the presence service (status).
pub const PRESENCE: &str = "https://presence.teams.microsoft.com";
/// Tokens are renewed when they have less than this left (ms).
const MARGIN: i64 = 5 * 60 * 1000;
/// The keychain key of the refresh token.
pub const LOGIN_KEY: &str = "teams";

fn scope(resource: &str) -> String {
    format!("{resource}/.default openid profile offline_access")
}

/// The page the login window opens.
pub fn authorize_url() -> String {
    format!(
        "{AUTHORITY}/authorize?client_id={CLIENT_ID}&response_type=code&prompt=select_account&redirect_uri={}&scope={}",
        http::encode(REDIRECT),
        http::encode(&scope(SPACES))
    )
}

/// The authorization code in the redirect the login window caught.
pub fn code_from_redirect(url: &str) -> Option<String> {
    let query = url.strip_prefix(REDIRECT)?.trim_start_matches('?');
    query.split('&').find_map(|p| p.strip_prefix("code=")).map(http::decode).filter(|c| !c.is_empty())
}

/// The chat service's endpoints and token, from the authz service.
#[derive(Clone, Debug)]
pub struct Skype {
    pub token: String,
    /// `https://<region>.ng.msg.teams.microsoft.com`
    pub chat_service: String,
    /// `https://teams.microsoft.com/api/csa/<geo>`
    pub aggregator: String,
    /// `https://teams.microsoft.com/api/mt/part/<partition>`
    pub middle_tier: String,
    /// Where pictures and files live: `https://<region>-prod.asyncgw.teams.microsoft.com`
    pub ams: String,
    /// The presence service (status) for this user's region:
    /// `https://teams.microsoft.com/ups/<geo>`. (`presence.teams.microsoft.com`
    /// refuses the same requests.)
    pub presence: String,
    expires: i64,
}

struct Token {
    value: String,
    expires: i64,
}

/// Who signed in, from the sign-in's id token.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Identity {
    /// The chat service's id for the user: `8:orgid:<object id>`.
    pub id: String,
    pub name: String,
}

/// A signed-in session: the refresh token and the tokens it gave, renewed as they run out.
pub struct Session {
    refresh: String,
    tokens: HashMap<String, Token>,
    skype: Option<Skype>,
    identity: Option<Identity>,
}

impl Session {
    /// The session stored in the keychain, if there is one.
    pub fn stored() -> Option<Session> {
        crate::credentials::load(LOGIN_KEY).map(Session::with_refresh)
    }

    fn with_refresh(refresh: String) -> Session {
        Session { refresh, tokens: HashMap::new(), skype: None, identity: None }
    }

    /// Redeem the login window's code, and keep the refresh token it gives.
    pub fn sign_in(code: &str) -> Result<Session> {
        let mut session = Session::with_refresh(String::new());
        let answer =
            token_request(&[("grant_type", "authorization_code"), ("code", code), ("redirect_uri", REDIRECT), ("scope", &scope(SPACES))])?;
        session.keep(SPACES, &answer)?;
        Ok(session)
    }

    /// Forget the stored sign-in.
    pub fn sign_out() {
        crate::credentials::clear(LOGIN_KEY);
    }

    /// An access token for a service's audience (`SPACES`, `CHAT_AGGREGATOR`).
    pub fn access(&mut self, resource: &str) -> Result<String> {
        if let Some(t) = self.tokens.get(resource).filter(|t| t.expires - MARGIN > now_ms()) {
            return Ok(t.value.clone());
        }
        let answer = token_request(&[("grant_type", "refresh_token"), ("refresh_token", &self.refresh), ("scope", &scope(resource))])?;
        self.keep(resource, &answer)
    }

    /// The skype token and the chat service's endpoints for this user's region.
    pub fn skype(&mut self) -> Result<Skype> {
        if let Some(s) = self.skype.as_ref().filter(|s| s.expires - MARGIN > now_ms()) {
            return Ok(s.clone());
        }
        let bearer = format!("Bearer {}", self.access(SPACES)?);
        let answer = http::request(Method::Post, AUTHZ, &[("Authorization", &bearer), ("Accept", "application/json")], Body::None)?;
        let skype = parse_authz(&answer).ok_or_else(|| Error::Service("no chat service in the sign-in answer".into()))?;
        log_info!("chat service {} (region {})", skype.chat_service, answer["region"].as_str().unwrap_or("?"));
        self.skype = Some(skype.clone());
        Ok(skype)
    }

    /// Who signed in; known once a token has been fetched.
    pub fn identity(&self) -> Option<&Identity> {
        self.identity.as_ref()
    }

    /// Forget the skype token (after the chat service refused it).
    pub fn drop_skype(&mut self) {
        self.skype = None;
    }

    fn keep(&mut self, resource: &str, answer: &Value) -> Result<String> {
        let value = answer["access_token"].as_str().ok_or(Error::SignedOut)?.to_string();
        let expires = now_ms() + answer["expires_in"].as_i64().unwrap_or(3600) * 1000;
        // The refresh token rotates: the newest one is the one to keep.
        if let Some(refresh) = answer["refresh_token"].as_str().filter(|r| *r != self.refresh) {
            self.refresh = refresh.to_string();
            crate::credentials::save(LOGIN_KEY, refresh);
        }
        if let Some(identity) = answer["id_token"].as_str().and_then(identity) {
            self.identity = Some(identity);
        }
        self.tokens.insert(resource.to_string(), Token { value: value.clone(), expires });
        Ok(value)
    }
}

fn token_request(form: &[(&str, &str)]) -> Result<Value> {
    let mut form = form.to_vec();
    form.push(("client_id", CLIENT_ID));
    let url = format!("{AUTHORITY}/token");
    // A busy or throttling sign-in service is asked again, a little later.
    let mut answer = Err(Error::Network(String::new()));
    for pause in [0, 1, 3] {
        std::thread::sleep(std::time::Duration::from_secs(pause));
        answer = http::request(Method::Post, &url, &[("Origin", "https://teams.microsoft.com")], Body::Form(&form));
        match &answer {
            Err(Error::Service(e)) if passing(e) => continue,
            Err(Error::Network(_)) => continue,
            _ => break,
        }
    }
    answer.map_err(token_error)
}

/// A status the sign-in service may answer differently soon (throttled, down).
fn passing(error: &str) -> bool {
    ["HTTP 429", "HTTP 500", "HTTP 502", "HTTP 503", "HTTP 504"].iter().any(|s| error.starts_with(s))
}

/// Only an expired or revoked refresh token (400 invalid_grant) means signing in
/// again; anything else (an outage, a scope the tenant refuses) keeps the sign-in.
fn token_error(e: Error) -> Error {
    match e {
        Error::Service(s) if s.ends_with(" invalid_grant") => Error::SignedOut,
        e => e,
    }
}

/// The user's id and name from an id token (a JWT: its claims are the middle part).
fn identity(jwt: &str) -> Option<Identity> {
    let claims: Value = serde_json::from_slice(&base64url(jwt.split('.').nth(1)?)?).ok()?;
    let oid = claims["oid"].as_str()?;
    let name = claims["name"].as_str().or(claims["preferred_username"].as_str()).unwrap_or("");
    Some(Identity { id: format!("8:orgid:{oid}"), name: name.to_string() })
}

/// Base64, standard or URL-safe, padded or not.
pub(super) fn base64url(s: &str) -> Option<Vec<u8>> {
    let value = |c: u8| match c {
        b'A'..=b'Z' => Some(c - b'A'),
        b'a'..=b'z' => Some(c - b'a' + 26),
        b'0'..=b'9' => Some(c - b'0' + 52),
        b'-' | b'+' => Some(62),
        b'_' | b'/' => Some(63),
        _ => None,
    };
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    let (mut acc, mut bits) = (0u32, 0);
    for c in s.bytes().take_while(|&c| c != b'=') {
        acc = (acc << 6) | u32::from(value(c)?);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    Some(out)
}

fn parse_authz(v: &Value) -> Option<Skype> {
    let token = v["tokens"]["skypeToken"].as_str()?.to_string();
    let expires = now_ms() + v["tokens"]["expiresIn"].as_i64().unwrap_or(3600) * 1000;
    let gtms = &v["regionGtms"];
    let region = v["region"].as_str().unwrap_or("emea");
    let chat_service = gtms["chatService"].as_str().map_or_else(|| format!("https://{region}.ng.msg.teams.microsoft.com"), str::to_string);
    let aggregator = gtms["chatSvcAggAfd"].as_str().map_or_else(|| "https://teams.microsoft.com/api/csa".to_string(), str::to_string);
    let middle_tier = gtms["middleTier"].as_str().map_or_else(|| format!("https://teams.microsoft.com/api/mt/{region}"), str::to_string);
    let ams = gtms["ams"].as_str().or(gtms["amsV2"].as_str()).unwrap_or("https://teams.microsoft.com/api/amsMTProd").to_string();
    let presence =
        gtms["presenceUPS"].as_str().or(gtms["unifiedPresence"].as_str()).unwrap_or("https://presence.teams.microsoft.com").to_string();
    let trim = |s: String| s.trim_end_matches('/').to_string();
    Some(Skype {
        token,
        chat_service: trim(chat_service),
        aggregator: trim(aggregator),
        middle_tier: trim(middle_tier),
        ams: trim(ams),
        presence: trim(presence),
        expires,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_revoked_sign_in_signs_out() {
        assert!(matches!(token_error(Error::Service("HTTP 400 invalid_grant".into())), Error::SignedOut));
        for kept in ["HTTP 503", "HTTP 429", "HTTP 400 invalid_scope", "HTTP 400 interaction_required", "HTTP 500 server_error"] {
            assert!(matches!(token_error(Error::Service(kept.into())), Error::Service(_)), "{kept}");
        }
        assert!(matches!(token_error(Error::Network("down".into())), Error::Network(_)));
        assert!(passing("HTTP 503") && passing("HTTP 429 throttled") && !passing("HTTP 400 invalid_grant"));
    }

    #[test]
    fn finds_the_code_in_the_redirect() {
        let url = format!("{REDIRECT}?code=0.AX%2Fabc&session_state=s1");
        assert_eq!(code_from_redirect(&url).as_deref(), Some("0.AX/abc"));
        assert_eq!(code_from_redirect(&format!("{REDIRECT}?error=access_denied")), None);
        assert_eq!(code_from_redirect("https://example.com/?code=x"), None);
    }

    #[test]
    fn reads_who_signed_in() {
        // {"oid":"abc-1","name":"Alex Morgan"}
        let jwt = "e30.eyJvaWQiOiJhYmMtMSIsIm5hbWUiOiJBbGV4IE1vcmdhbiJ9.sig";
        assert_eq!(identity(jwt), Some(Identity { id: "8:orgid:abc-1".into(), name: "Alex Morgan".into() }));
        assert_eq!(identity("not a token"), None);
    }

    #[test]
    fn reads_the_authz_answer() {
        let v = serde_json::json!({
            "tokens": {"skypeToken": "t", "expiresIn": 86_400},
            "region": "amer",
            "regionGtms": {"chatService": "https://amer.ng.msg.teams.microsoft.com/", "chatSvcAggAfd": "https://teams.microsoft.com/api/csa/amer", "middleTier": "https://teams.microsoft.com/api/mt/part/amer-03"}
        });
        let s = parse_authz(&v).unwrap();
        assert_eq!(s.chat_service, "https://amer.ng.msg.teams.microsoft.com");
        assert_eq!(
            (s.aggregator.as_str(), s.middle_tier.as_str()),
            ("https://teams.microsoft.com/api/csa/amer", "https://teams.microsoft.com/api/mt/part/amer-03")
        );
        assert!(parse_authz(&serde_json::json!({})).is_none());
    }
}
