//! The shape of a call notice, for the log: which keys it has, what kind of value
//! each holds, and where its links go, with nothing that says who or which call.
//! Answering a call means using the links the notice carries; this shows them.

use serde_json::Value;

/// Hosts a link may be shown with (Microsoft's own); any other is `{host}`.
const KNOWN_HOSTS: &[&str] =
    &["microsoft.com", "skype.com", "office.com", "office.net", "trouter.io", "lync.com", "azure.com", "microsoft.net"];
/// Keys whose values may say who someone is: only their kind is shown.
const PRIVATE: &[&str] = &[
    "name", "id", "mri", "user", "email", "phone", "token", "key", "secret", "tenant", "subject", "title", "upn", "sip", "tel", "cookie",
    "auth",
];

/// One line per value: `path: kind`.
pub fn shape(value: &Value) -> Vec<String> {
    let mut lines = Vec::new();
    walk(value, "", &mut lines);
    lines
}

fn walk(value: &Value, path: &str, lines: &mut Vec<String>) {
    match value {
        Value::Object(map) => {
            if map.is_empty() {
                lines.push(format!("{path}: {{}}"));
            }
            for (key, v) in map {
                walk(v, &format!("{path}.{}", key_text(key)), lines);
            }
        }
        Value::Array(items) => {
            lines.push(format!("{path}: array[{}]", items.len()));
            // The first stands for the rest (they're alike).
            if let Some(first) = items.first() {
                walk(first, &format!("{path}[0]"), lines);
            }
        }
        Value::String(s) => {
            // A string can hold more JSON (Teams nests it).
            if let Ok(inner @ (Value::Object(_) | Value::Array(_))) = serde_json::from_str::<Value>(s) {
                walk(&inner, &format!("{path}(json)"), lines);
                return;
            }
            let private = is_private(path);
            lines.push(format!("{path}: {}", string_kind(s, private)));
        }
        Value::Bool(b) => lines.push(format!("{path}: bool {b}")),
        Value::Number(_) => lines.push(format!("{path}: number")),
        Value::Null => lines.push(format!("{path}: null")),
    }
}

/// A key as it is, unless it's itself an id (a map keyed by ids).
fn key_text(key: &str) -> &str {
    if has_id_characters(key) { "{id}" } else { key }
}

fn is_private(path: &str) -> bool {
    let last = path.rsplit('.').next().unwrap_or(path).to_lowercase();
    PRIVATE.iter().any(|p| last.contains(p))
}

/// What kind of string: a link (where to), an SDP offer (what media), a short word
/// (when it can't say who), or just its length.
fn string_kind(s: &str, private: bool) -> String {
    if let Some(link) = link(s) {
        return format!("url {link}");
    }
    if s.starts_with("v=0") {
        return sdp(s);
    }
    let word = s.len() <= 32 && !s.is_empty() && s.chars().all(|c| c.is_ascii_alphabetic() || c == '_' || c == '-');
    if word && !private {
        return format!("\"{s}\"");
    }
    format!("string({})", s.chars().count())
}

/// `https://host/path` with ids in the path as `{id}`, no query; `None` if not a link.
fn link(s: &str) -> Option<String> {
    let url = url::Url::parse(s).ok()?;
    if !matches!(url.scheme(), "https" | "http" | "wss" | "ws") {
        return None;
    }
    let host = url.host_str().unwrap_or("");
    let known = KNOWN_HOSTS.iter().any(|k| host == *k || host.ends_with(&format!(".{k}")));
    let host = if known { host } else { "{host}" };
    let path: Vec<&str> = url.path().split('/').map(|segment| if looks_like_id(segment) { "{id}" } else { segment }).collect();
    let query = if url.query().is_some() { "?…" } else { "" };
    Some(format!("{}://{host}{}{query}", url.scheme(), path.join("/")))
}

/// A path segment that's an id rather than a word: long, or with digits or the
/// characters ids have (a version like `v2` is a word).
fn looks_like_id(s: &str) -> bool {
    let version = s.strip_prefix('v').is_some_and(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit() || c == '.'));
    !version && (s.len() > 24 || has_id_characters(s))
}

fn has_id_characters(s: &str) -> bool {
    s.chars().any(|c| c.is_ascii_digit() || matches!(c, ':' | '@' | '%' | '=' | '{' | '}'))
}

/// An SDP: its media, codecs and how many ICE candidates (no addresses).
fn sdp(s: &str) -> String {
    let mut media = Vec::new();
    let mut codecs = Vec::new();
    let mut candidates = 0;
    let mut other = Vec::new();
    for line in s.lines() {
        if let Some(m) = line.strip_prefix("m=") {
            media.push(m.split(' ').next().unwrap_or("").to_string());
        } else if let Some(map) = line.strip_prefix("a=rtpmap:") {
            let codec = map.split(' ').nth(1).unwrap_or("").to_string();
            if !codecs.contains(&codec) {
                codecs.push(codec);
            }
        } else if line.starts_with("a=candidate:") || line.starts_with("a=x-candidate") {
            candidates += 1;
        } else if let Some(attribute) = line.strip_prefix("a=") {
            // Which attributes (crypto, fingerprint, ice-ufrag, …), never their values.
            let name = attribute.split([':', ' ']).next().unwrap_or("").to_string();
            if !other.contains(&name) {
                other.push(name);
            }
        }
    }
    format!("sdp media={media:?} codecs={codecs:?} candidates={candidates} attributes={other:?}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn shows_structure_and_links_but_no_one() {
        let notice = json!({
            "callNotification": {
                "from": {"id": "8:orgid:3f1c9a2e-0000-4000-8000-000000000001", "displayName": "Maya Lindqvist"},
                "callerMri": "8:orgid:3f1c9a2e-0000-4000-8000-000000000001",
                "links": {
                    "attach": "https://api.flightproxy.teams.microsoft.com/api/v2/ep/conv-euno-01.conv.skype.com/conv/AbC123xyz/attach?i=10&e=638",
                    "reject": "https://evil.example.org/calls/42/reject"
                },
                "mediaType": "audio",
                "isVideo": false,
                "participants": [{"id": "8:orgid:x1", "role": "caller"}, {"id": "8:orgid:x2", "role": "callee"}]
            },
            "conversationInvitation": "{\"isMultiParty\":false,\"subject\":\"Budget review\"}",
            "offer": "v=0\r\no=- 1 1 IN IP4 10.0.0.1\r\nm=audio 50000 RTP/SAVP 111 9\r\na=rtpmap:111 OPUS/48000/2\r\na=rtpmap:9 G722/8000\r\na=candidate:1 1 UDP 2130706431 10.0.0.1 50000 typ host\r\na=crypto:1 AES_CM_128_HMAC_SHA1_80 inline:secretkey\r\na=ice-ufrag:abcd\r\n",
            "{9b1f2c3d-aaaa-bbbb-cccc-1234567890ab}": {"state": "ringing"}
        });
        let lines = shape(&notice).join("\n");
        // The links: where they go, ids and queries out.
        assert!(
            lines.contains(
                ".callNotification.links.attach: url https://api.flightproxy.teams.microsoft.com/api/v2/ep/{id}/conv/{id}/attach?…"
            ),
            "{lines}"
        );
        assert!(lines.contains(".callNotification.links.reject: url https://{host}/calls/{id}/reject"), "{lines}");
        // Plain words show; people and ids don't.
        assert!(lines.contains(".callNotification.mediaType: \"audio\""));
        assert!(lines.contains(".callNotification.isVideo: bool false"));
        assert!(lines.contains(".callNotification.participants: array[2]"));
        assert!(lines.contains(".callNotification.participants[0].role: \"caller\""));
        assert!(lines.contains(".conversationInvitation(json).isMultiParty: bool false"));
        assert!(lines.contains(".offer: sdp media=[\"audio\"] codecs=[\"OPUS/48000/2\", \"G722/8000\"] candidates=1"), "{lines}");
        assert!(lines.contains(".{id}.state: \"ringing\""));
        for secret in [
            "Maya",
            "Lindqvist",
            "orgid",
            "3f1c9a2e",
            "AbC123xyz",
            "evil.example",
            "10.0.0.1",
            "secretkey",
            "abcd",
            "Budget",
            "i=10",
            "9b1f2c3d",
        ] {
            assert!(!lines.contains(secret), "{secret} leaked:\n{lines}");
        }
    }

    #[test]
    fn private_words_stay_hidden() {
        let lines = shape(&json!({"displayName": "Maya", "userName": "maya", "kind": "audio"})).join("\n");
        assert!(lines.contains(".displayName: string(4)") && lines.contains(".userName: string(4)") && lines.contains(".kind: \"audio\""));
    }
}
