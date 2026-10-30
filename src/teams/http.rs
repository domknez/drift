//! HTTP for the Teams services: one agent, the headers the web client sends, and
//! answers turned into JSON or a `backend::Error`.

use crate::backend::{Error, Result};
use serde_json::Value;
use std::sync::LazyLock;
use std::time::Duration;

/// The web client's user agent; some services answer other clients differently.
pub const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Safari/537.36 Edg/126.0.0.0 Teams/24165.1410.2974.6689/49";
/// The largest answer read (the teams roster of a big tenant runs to megabytes).
const MAX_BODY: u64 = 32 << 20;

static AGENT: LazyLock<ureq::Agent> = LazyLock::new(|| {
    ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(30)))
        .http_status_as_error(false)
        .user_agent(USER_AGENT)
        .build()
        .into()
});

pub enum Body<'a> {
    None,
    Json(&'a Value),
    Form(&'a [(&'a str, &'a str)]),
    /// A file's bytes, with their MIME type.
    Bytes(&'a [u8], &'a str),
}

#[derive(Clone, Copy)]
pub enum Method {
    Get,
    Post,
    Put,
    Delete,
}

/// Send a request and read the answer as JSON (`Null` for an empty one).
pub fn request(method: Method, url: &str, headers: &[(&str, &str)], body: Body) -> Result<Value> {
    let path = url.split('?').next().unwrap_or(url);
    let verb = match method {
        Method::Get => "GET",
        Method::Post => "POST",
        Method::Put => "PUT",
        Method::Delete => "DELETE",
    };
    log_debug!("{verb} {path}");
    let response = if matches!(method, Method::Get) {
        let mut req = AGENT.get(url);
        for (k, v) in headers {
            req = req.header(*k, *v);
        }
        req.call()
    } else {
        let mut req = match method {
            Method::Put => AGENT.put(url),
            Method::Delete => AGENT.delete(url).force_send_body(),
            _ => AGENT.post(url),
        };
        for (k, v) in headers {
            req = req.header(*k, *v);
        }
        match body {
            Body::None => req.send_empty(),
            Body::Json(v) => req.send_json(v),
            Body::Form(f) => req.send_form(f.iter().copied()),
            Body::Bytes(bytes, mime) => req.header("Content-Type", mime).send(bytes),
        }
    };
    let response = response.map_err(|e| Error::Network(e.to_string()))?;
    let status = response.status().as_u16();
    let text = response.into_body().with_config().limit(MAX_BODY).read_to_string().map_err(|e| Error::Network(e.to_string()))?;
    let json = if text.trim().is_empty() { Value::Null } else { serde_json::from_str(&text).unwrap_or(Value::Null) };
    match status {
        200..=299 => Ok(json),
        401 => Err(Error::SignedOut),
        _ => {
            log_warn!("{path}: HTTP {status}: {}", error_text(&json, &text));
            Err(Error::Service(format!("HTTP {status}")))
        }
    }
}

/// GET a file (an image): its bytes.
pub fn fetch(url: &str, headers: &[(&str, &str)]) -> Result<Vec<u8>> {
    let mut req = AGENT.get(url);
    for (k, v) in headers {
        req = req.header(*k, *v);
    }
    let response = req.call().map_err(|e| Error::Network(e.to_string()))?;
    let status = response.status().as_u16();
    if !(200..300).contains(&status) {
        return Err(if status == 401 { Error::SignedOut } else { Error::Service(format!("HTTP {status}")) });
    }
    response.into_body().with_config().limit(16 << 20).read_to_vec().map_err(|e| Error::Network(e.to_string()))
}

/// What an error answer says, short, for the log (never the whole body).
fn error_text(json: &Value, raw: &str) -> String {
    let said = ["error_description", "message", "errorCode", "error"].iter().find_map(|k| match &json[*k] {
        Value::String(s) => Some(s.clone()),
        Value::Object(o) => o.get("message").and_then(Value::as_str).map(str::to_string),
        _ => None,
    });
    said.unwrap_or_else(|| raw.chars().take(200).collect()).lines().next().unwrap_or("").to_string()
}

/// `s` percent-encoded for a URL's query or path.
pub fn encode(s: &str) -> String {
    s.bytes()
        .map(|b| if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") })
        .collect()
}

/// The inverse of [`encode`] (`+` stays as it is).
pub fn decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%'
            && let Some(v) = s.get(i + 1..i + 3).and_then(|h| u8::from_str_radix(h, 16).ok())
        {
            out.push(v);
            i += 3;
            continue;
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_and_decodes() {
        assert_eq!(encode("19:a b@thread.v2"), "19%3Aa%20b%40thread.v2");
        assert_eq!(decode(&encode("x/y?z=1&ä")), "x/y?z=1&ä");
        assert_eq!(decode("100%"), "100%");
    }
}
