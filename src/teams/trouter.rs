//! Trouter: the websocket the Teams services push changes through, so new messages
//! show as they're sent rather than at the next poll.
//!
//! The lifecycle: `POST /v4/a` says where to connect, `GET socket.io/1/` gives a
//! session, the websocket opens on it (socket.io 0.9), `user.authenticate` signs it
//! in, and the registrar is told to send the chat service's events to it. Services
//! then POST to the socket's URL and Trouter forwards each request as a `3:::` frame,
//! which is answered with a 200. A dropped socket starts over after a pause.

use super::Teams;
use super::auth::{SPACES, Skype};
use super::http::{self, Body, Method, USER_AGENT};
use crate::backend::{Error, Push, Result};
use serde_json::{Value, json};
use std::io::Read;
use std::net::TcpStream;
use std::sync::Weak;
use std::time::{Duration, Instant};
use tungstenite::client::IntoClientRequest;
use tungstenite::stream::MaybeTlsStream;
use tungstenite::{Message, WebSocket};

const HOST: &str = "https://go.trouter.teams.microsoft.com/";
const REGISTRAR: &str = "https://teams.microsoft.com/registrar/prod/V2/registrations";
/// How long a registration lasts (s); it's renewed a minute before.
const REGISTRATION_TTL: u64 = 86_400;
const PING_EVERY: Duration = Duration::from_secs(30);
/// The pause before connecting again, growing to the longest after repeated failures.
const RETRY: Duration = Duration::from_secs(5);
const RETRY_LONGEST: Duration = Duration::from_secs(120);
/// The version the web client gives Trouter.
const CLIENT_VERSION: &str = "27/24070119616";
const CLIENT_CV: &str = "2024.23.01.2";

type Socket = WebSocket<MaybeTlsStream<TcpStream>>;

/// Where `POST /v4/a` says to connect.
struct Info {
    socketio: String,
    surl: String,
    connect_params: serde_json::Map<String, Value>,
    ccid: Option<String>,
}

/// Keep a Trouter socket open for as long as `teams` lives and `to` takes pushes.
pub fn run(teams: Weak<Teams>, to: Box<dyn Fn(Push) + Send>) {
    let endpoint = endpoint_id();
    let mut pause = RETRY;
    let mut connections = 0u64;
    loop {
        let Some(t) = teams.upgrade() else { return };
        let started = Instant::now();
        let ended = session(&t, &endpoint, &mut connections, &to);
        drop(t);
        to(Push::Live(false));
        match ended {
            Ok(()) => {}
            Err(Error::SignedOut) => log_info!("trouter: the sign-in was refused; trying again later"),
            Err(e) => log_warn!("trouter: {e}"),
        }
        if teams.strong_count() == 0 {
            return;
        }
        // A socket that stayed up a while starts the backoff over.
        pause = if started.elapsed() > RETRY_LONGEST { RETRY } else { (pause * 2).min(RETRY_LONGEST) };
        std::thread::sleep(pause);
        // What changed while the socket was down is fetched by the next poll.
        to(Push::Everything);
    }
}

/// One socket from allocation to its end; Ok when `to` no longer takes pushes.
fn session(teams: &Teams, endpoint: &str, connections: &mut u64, to: &dyn Fn(Push)) -> Result<()> {
    let skype = teams.session().skype()?;
    let info = allocate(&skype, endpoint)?;
    *connections += 1;
    let (sid, heartbeat) = handshake(&skype, &info, endpoint, *connections)?;
    // The server's heartbeats and the answers to drift's pings stop: the socket is dead.
    let silent_for = heartbeat.max(PING_EVERY * 2) + Duration::from_secs(15);
    *connections += 1;
    let mut socket = dial(&skype, &info, &sid, endpoint, *connections)?;
    log_info!("trouter: connected");
    let mut pinged = Instant::now();
    // When anything last came: pings are answered, so a long silence means the socket
    // died quietly (the computer slept, the network changed) and it's opened again.
    let mut heard = Instant::now();
    let mut registered: Option<Instant> = None;
    let mut sequence = 0u64;
    loop {
        if pinged.elapsed() >= PING_EVERY {
            sequence += 1;
            send(&mut socket, format!("5:{sequence}+::{{\"name\":\"ping\"}}"))?;
            pinged = Instant::now();
        }
        if heard.elapsed() >= silent_for {
            return Err(Error::Network("no answer from the push service".into()));
        }
        if registered.is_some_and(|at| at.elapsed() >= Duration::from_secs(REGISTRATION_TTL - 60)) {
            register(teams, &info.surl, endpoint)?;
            registered = Some(Instant::now());
        }
        let read = socket.read();
        if read.is_ok() {
            heard = Instant::now();
        }
        let frame = match read {
            Ok(Message::Text(text)) => text.to_string(),
            Ok(Message::Close(_)) => return Err(Error::Network("the socket was closed".into())),
            Ok(_) => continue,
            Err(tungstenite::Error::Io(e)) if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) => continue,
            Err(e) => return Err(Error::Network(e.to_string())),
        };
        match frame.as_bytes().first() {
            // Session up: sign it in, then have the chat service's events sent here.
            Some(b'1') => {
                let bearer = teams.session().access(SPACES)?;
                let authenticate = json!({"name": "user.authenticate", "args": [{
                    "headers": {"X-Ms-Test-User": "False", "Authorization": format!("Bearer {bearer}"), "X-MS-Migration": "True"},
                    "connectparams": info.connect_params,
                }]});
                send(&mut socket, format!("5:::{authenticate}"))?;
                sequence += 1;
                let activity =
                    json!({"name": "user.activity", "args": [{"state": "active", "cv": format!("{}.0.1", correlation_vector())}]});
                send(&mut socket, format!("5:{sequence}+::{activity}"))?;
                register(teams, &info.surl, endpoint)?;
                registered = Some(Instant::now());
                to(Push::Live(true));
            }
            Some(b'2') => send(&mut socket, "2::".into())?,
            // A service's request, forwarded: answer it, then see what it says.
            Some(b'3') => {
                let Some(request) = payload(&frame).and_then(|p| serde_json::from_str::<Value>(p).ok()) else { continue };
                let reply = json!({"id": request["id"], "status": 200, "body": ""});
                send(&mut socket, format!("3:::{reply}"))?;
                let url = request["url"].as_str().unwrap_or("");
                log_debug!("trouter: request for {}", url.rsplit('/').next().unwrap_or(""));
                let body = request_body(&request);
                for push in pushes(url, &body) {
                    to(push);
                }
            }
            // Trouter's own events, which it sends again until they're acknowledged.
            Some(b'5') => {
                if let Some(id) = ack_id(&frame) {
                    send(&mut socket, format!("6:{id}::"))?;
                }
                let event = payload(&frame).and_then(|p| serde_json::from_str::<Value>(p).ok()).unwrap_or_default();
                log_debug!("trouter: event {}", event["name"].as_str().unwrap_or("?"));
                if event["name"] == "trouter.message_loss" {
                    to(Push::Everything);
                }
            }
            _ => {}
        }
    }
}

/// Ask Trouter for an endpoint to connect to.
fn allocate(skype: &Skype, endpoint: &str) -> Result<Info> {
    let url = format!("{HOST}v4/a?epid={}", http::encode(endpoint));
    let answer = http::request(Method::Post, &url, &[("X-Skypetoken", &skype.token)], Body::None)?;
    let surl = answer["surl"].as_str().filter(|s| !s.is_empty()).ok_or_else(|| Error::Service("no surl from trouter".into()))?;
    Ok(Info {
        socketio: answer["socketio"].as_str().filter(|s| !s.is_empty()).unwrap_or(HOST).trim_end_matches('/').to_string(),
        surl: surl.to_string(),
        connect_params: answer["connectparams"].as_object().cloned().unwrap_or_default(),
        ccid: answer["ccid"].as_str().map(str::to_string),
    })
}

/// The query string the handshake and the socket both carry.
fn query(info: &Info, endpoint: &str, connection: u64) -> String {
    let mut pairs: Vec<(String, String)> = vec![("v".into(), "v4".into())];
    for (k, v) in &info.connect_params {
        pairs.push((k.clone(), v.as_str().map_or_else(|| v.to_string(), str::to_string)));
    }
    let tc = json!({"cv": CLIENT_CV, "ua": "TeamsCDL", "hr": "", "v": CLIENT_VERSION});
    pairs.push(("tc".into(), tc.to_string()));
    pairs.push(("con_num".into(), format!("{}_{connection}", crate::model::time::now_ms())));
    pairs.push(("epid".into(), endpoint.into()));
    if let Some(ccid) = &info.ccid {
        pairs.push(("ccid".into(), ccid.clone()));
    }
    pairs.push(("auth".into(), "true".into()));
    pairs.push(("timeout".into(), "40".into()));
    pairs.iter().map(|(k, v)| format!("{}={}", http::encode(k), http::encode(v))).collect::<Vec<_>>().join("&")
}

/// The socket.io session id.
/// A socket.io session: its id, and how long the server may stay silent (its
/// heartbeat timeout).
fn handshake(skype: &Skype, info: &Info, endpoint: &str, connection: u64) -> Result<(String, Duration)> {
    let url = format!("{}/socket.io/1/?{}", info.socketio, query(info, endpoint, connection));
    let answer = http::fetch(&url, &[("X-Skypetoken", &skype.token)])?;
    // `<sid>:<heartbeat>:<close timeout>:<transports>`
    let text = String::from_utf8_lossy(&answer);
    let sid = text.split(':').next().unwrap_or("").trim();
    if sid.is_empty() {
        return Err(Error::Service("no socket.io session from trouter".into()));
    }
    let heartbeat = text.split(':').nth(1).and_then(|h| h.trim().parse::<u64>().ok()).filter(|h| *h > 0).unwrap_or(60);
    Ok((sid.to_string(), Duration::from_secs(heartbeat)))
}

fn dial(skype: &Skype, info: &Info, sid: &str, endpoint: &str, connection: u64) -> Result<Socket> {
    let base = info.socketio.replacen("https://", "wss://", 1);
    let url = format!("{base}/socket.io/1/websocket/{sid}?{}", query(info, endpoint, connection));
    let mut request = url.into_client_request().map_err(|e| Error::Network(e.to_string()))?;
    let headers = request.headers_mut();
    for (k, v) in [("X-Skypetoken", skype.token.as_str()), ("User-Agent", USER_AGENT)] {
        headers.insert(k, v.parse().map_err(|_| Error::Service("a header that can't be sent".into()))?);
    }
    let (socket, _) = tungstenite::connect(request).map_err(|e| Error::Network(e.to_string()))?;
    // Reads wake up now and then, so pings go out on time.
    let tcp = match socket.get_ref() {
        MaybeTlsStream::Plain(s) => s,
        MaybeTlsStream::Rustls(s) => s.get_ref(),
        _ => return Err(Error::Network("an unexpected kind of socket".into())),
    };
    tcp.set_read_timeout(Some(Duration::from_secs(5))).map_err(|e| Error::Network(e.to_string()))?;
    Ok(socket)
}

fn send(socket: &mut Socket, frame: String) -> Result<()> {
    socket.send(Message::text(frame)).map_err(|e| Error::Network(e.to_string()))
}

/// Tell the registrar to send the chat service's events to this socket.
fn register(teams: &Teams, surl: &str, endpoint: &str) -> Result<()> {
    let skype = teams.session().skype()?;
    let bearer = format!("Bearer {}", teams.session().access(SPACES)?);
    let body = json!({
        "clientDescription": {
            "appId": "TeamsCDLWebWorker",
            "aesKey": "",
            "languageId": "en-US",
            "platform": "edge",
            "templateKey": "TeamsCDLWebWorker_2.1",
            "platformUIVersion": CLIENT_VERSION,
        },
        "registrationId": endpoint,
        "nodeId": "",
        "transports": {"TROUTER": [{"context": "", "path": surl, "ttl": REGISTRATION_TTL}]},
    });
    http::request(
        Method::Post,
        REGISTRAR,
        &[("X-Skypetoken", &skype.token), ("Authorization", &bearer), ("Content-Type", "application/json")],
        Body::Json(&body),
    )
    .map(|_| ())
}

/// What a forwarded request says changed.
fn pushes(url: &str, body: &Value) -> Vec<Push> {
    if !url.ends_with("/messaging") || body["type"] != "EventMessage" {
        return Vec::new();
    }
    let resource = &body["resource"];
    let conversation = match body["resourceType"].as_str().unwrap_or("") {
        "NewMessage" | "MessageUpdate" => {
            // Typing comes as messages too.
            if resource["messagetype"].as_str().is_some_and(|t| t.starts_with("Control/")) {
                return Vec::new();
            }
            resource["conversationLink"].as_str().and_then(conversation_of)
        }
        "ConversationUpdate" | "ThreadUpdate" => resource["id"].as_str().map(str::to_string),
        _ => None,
    };
    conversation.map(Push::Conversation).into_iter().collect()
}

/// The conversation of a link like `…/conversations/19:abc@thread.v2;messageid=1`.
fn conversation_of(link: &str) -> Option<String> {
    let id = link.rsplit_once("/conversations/")?.1.split(';').next()?;
    (!id.is_empty()).then(|| id.to_string())
}

/// A forwarded request's body as JSON; some come gzipped and in base64.
fn request_body(request: &Value) -> Value {
    let body = request["body"].as_str().unwrap_or("");
    let gzipped = request["headers"]
        .as_object()
        .is_some_and(|h| h.iter().any(|(k, v)| k.eq_ignore_ascii_case("X-Microsoft-Skype-Content-Encoding") && v == "gzip"));
    if !gzipped {
        return serde_json::from_str(body).unwrap_or_default();
    }
    let mut text = String::new();
    super::auth::base64url(body)
        .and_then(|bytes| flate2::read::GzDecoder::new(bytes.as_slice()).read_to_string(&mut text).ok())
        .and_then(|_| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

/// What follows a socket.io frame's third `:`.
fn payload(frame: &str) -> Option<&str> {
    let mut at = 0;
    for _ in 0..3 {
        at += frame[at..].find(':')? + 1;
    }
    Some(&frame[at..])
}

/// The id of a `5:<id>+::…` frame, to acknowledge.
fn ack_id(frame: &str) -> Option<&str> {
    let id = frame.strip_prefix("5:")?.split(':').next()?.trim_end_matches('+');
    (!id.is_empty() && id.bytes().all(|b| b.is_ascii_digit())).then_some(id)
}

/// The endpoint id, kept across runs: a new one each time piles up endpoints until
/// Teams refuses more.
pub(super) fn endpoint_id() -> String {
    let path = crate::settings::dir().map(|d| d.join("endpoint"));
    if let Some(id) = path.as_ref().and_then(|p| std::fs::read_to_string(p).ok()).map(|s| s.trim().to_string()).filter(|s| s.len() == 36) {
        return id;
    }
    let id = uuid();
    if let Some(path) = path {
        let _ = path.parent().map(std::fs::create_dir_all);
        let _ = std::fs::write(path, &id);
    }
    id
}

fn uuid() -> String {
    format!(
        "{:08x}-{:04x}-4{:03x}-{:04x}-{:012x}",
        fastrand::u32(..),
        fastrand::u16(..),
        fastrand::u16(..0xfff),
        fastrand::u16(0x8000..0xc000),
        fastrand::u64(..0xffff_ffff_ffff)
    )
}

/// A tracing id the web client sends: 22 characters of base64.
fn correlation_vector() -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    (0..22).map(|_| ALPHABET[fastrand::usize(..64)] as char).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_frames() {
        assert_eq!(payload("3:::{\"id\":1}"), Some("{\"id\":1}"));
        assert_eq!(payload("5:7+::{\"name\":\"x\"}"), Some("{\"name\":\"x\"}"));
        assert_eq!(payload("1::"), None);
        assert_eq!(ack_id("5:7+::{}"), Some("7"));
        assert_eq!(ack_id("5:::{}"), None);
    }

    #[test]
    fn finds_what_changed() {
        let new = json!({"type": "EventMessage", "resourceType": "NewMessage", "resource": {
            "messagetype": "RichText/Html",
            "conversationLink": "https://emea.ng.msg.teams.microsoft.com/v1/users/ME/conversations/19:abc@thread.v2;messageid=17",
        }});
        assert!(matches!(&pushes("/v4/f/x/messaging", &new)[..], [Push::Conversation(c)] if c == "19:abc@thread.v2"));
        let typing = json!({"type": "EventMessage", "resourceType": "NewMessage", "resource": {"messagetype": "Control/Typing", "conversationLink": "x/conversations/19:a"}});
        assert!(pushes("/messaging", &typing).is_empty());
        assert!(pushes("/unifiedPresenceService", &new).is_empty());
    }

    #[test]
    fn reads_gzipped_bodies() {
        use std::io::Write;
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        gz.write_all(b"{\"type\":\"EventMessage\"}").unwrap();
        let bytes = gz.finish().unwrap();
        const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut encoded = String::new();
        for chunk in bytes.chunks(3) {
            let n = chunk.iter().enumerate().fold(0u32, |n, (i, b)| n | (*b as u32) << (16 - 8 * i));
            for i in 0..=chunk.len() {
                encoded.push(ALPHABET[(n >> (18 - 6 * i) & 63) as usize] as char);
            }
        }
        let request = json!({"headers": {"X-Microsoft-Skype-Content-Encoding": "gzip"}, "body": encoded});
        assert_eq!(request_body(&request)["type"], "EventMessage");
    }
}
