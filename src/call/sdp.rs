//! Teams' SDP, made fit for the WebRTC stack and back.
//!
//! The stack takes Teams' offer except for `m=x-data`, a media kind it doesn't know,
//! and answers every m-line with `UDP/TLS/RTP/SAVPF` where Teams offers `RTP/SAVP`.
//! [`prepare`] renames those m-lines to `application` (which the stack then rejects,
//! as drift wants), and [`finish`] puts each m-line's own kind and protocol back in
//! the answer, so it has the offer's m-lines in the offer's order.

/// Media kinds the stack parses.
const KNOWN: &[&str] = &["audio", "video", "text", "application", "message", "image"];

/// What an offer's m-line was before [`prepare`] touched it.
#[derive(Debug, Clone, PartialEq)]
pub struct Section {
    pub media: String,
    pub proto: String,
    pub formats: Vec<String>,
    pub mid: Option<String>,
    /// The `a=rtpmap:` line of the first format, if it has one.
    pub rtpmap: Option<String>,
}

/// An offer the stack can read, and what it was.
#[derive(Debug)]
pub struct Prepared {
    pub sdp: String,
    pub sections: Vec<Section>,
    /// Addresses in the offer's candidates, which a TURN relay must be told to let through.
    pub candidates: Vec<std::net::SocketAddr>,
    /// Opus's payload type.
    pub opus: u8,
}

/// The offer with unknown media kinds renamed, and what each m-line was.
pub fn prepare(offer: &str) -> Result<Prepared, String> {
    let mut lines = Vec::new();
    let mut sections: Vec<Section> = Vec::new();
    let mut candidates = Vec::new();
    for line in offer.lines().map(str::trim_end).filter(|l| !l.is_empty()) {
        if let Some(m) = line.strip_prefix("m=") {
            let section = media_line(m).ok_or_else(|| format!("offer: bad m-line `{line}`"))?;
            if KNOWN.contains(&section.media.as_str()) {
                lines.push(line.to_string());
            } else {
                lines.push(format!("m=application{}", &m[section.media.len()..]));
            }
            sections.push(section);
        } else {
            if let Some(section) = sections.last_mut() {
                if let Some(mid) = line.strip_prefix("a=mid:") {
                    section.mid = Some(mid.to_string());
                }
                if line.strip_prefix("a=rtpmap:").and_then(|m| m.split_once(' ')).is_some_and(|(pt, _)| pt == section.formats[0]) {
                    section.rtpmap = Some(line.to_string());
                }
            }
            if let Some(addr) = line.strip_prefix("a=candidate:").and_then(candidate_addr) {
                candidates.push(addr);
            }
            lines.push(line.to_string());
        }
    }
    if !sections.iter().any(|s| s.media == "audio") {
        return Err("offer: no audio".into());
    }
    let opus = lines
        .iter()
        .filter_map(|l| l.strip_prefix("a=rtpmap:"))
        .find_map(|m| m.split_once(' ').filter(|(_, c)| c.to_ascii_lowercase().starts_with("opus/48000")))
        .and_then(|(pt, _)| pt.parse().ok())
        .ok_or("offer: no Opus")?;
    candidates.dedup();
    Ok(Prepared { sdp: crlf(&lines), sections, candidates, opus })
}

/// The stack's answer with each m-line's kind and protocol as offered, a rejected
/// m-line's format (and its `rtpmap`) taken from the offer, and the audio's SSRC
/// range, as Teams' own SDP has it.
pub fn finish(answer: &str, prepared: &Prepared, ssrc: u32) -> Result<String, String> {
    let mut lines = Vec::new();
    let mut index = 0;
    // What goes at the end of the m-line being copied.
    let mut tail: Option<String> = None;
    for line in answer.lines().map(str::trim_end).filter(|l| !l.is_empty()) {
        let Some(m) = line.strip_prefix("m=") else {
            lines.push(line.to_string());
            continue;
        };
        lines.extend(tail.take());
        let ours = media_line(m).ok_or_else(|| format!("answer: bad m-line `{line}`"))?;
        let offered = prepared.sections.get(index).ok_or("answer: more m-lines than the offer")?;
        index += 1;
        let port = m.split_whitespace().nth(1).unwrap_or("0");
        let formats = if port == "0" {
            tail = offered.rtpmap.clone();
            offered.formats[0].clone()
        } else {
            if offered.media == "audio" {
                tail = Some(format!("a=x-ssrc-range:{ssrc}-{ssrc}"));
            }
            ours.formats.join(" ")
        };
        lines.push(format!("m={} {port} {} {formats}", offered.media, offered.proto));
    }
    lines.extend(tail);
    if index != prepared.sections.len() {
        return Err(format!("answer: {index} m-lines for the offer's {}", prepared.sections.len()));
    }
    Ok(crlf(&lines))
}

/// `<media> <port> <proto> <fmt>…`
fn media_line(m: &str) -> Option<Section> {
    let mut fields = m.split_whitespace();
    let media = fields.next()?.to_string();
    fields.next()?;
    let proto = fields.next()?.to_string();
    let formats: Vec<String> = fields.map(str::to_string).collect();
    if formats.is_empty() {
        return None;
    }
    Some(Section { media, proto, formats, mid: None, rtpmap: None })
}

/// The address of `<foundation> <component> <transport> <priority> <address> <port> typ …`.
fn candidate_addr(candidate: &str) -> Option<std::net::SocketAddr> {
    let fields: Vec<&str> = candidate.split_whitespace().collect();
    if !fields.get(2)?.eq_ignore_ascii_case("udp") {
        return None;
    }
    Some(std::net::SocketAddr::new(fields.get(4)?.parse().ok()?, fields.get(5)?.parse().ok()?))
}

fn crlf(lines: &[String]) -> String {
    lines.iter().map(|l| format!("{l}\r\n")).collect()
}

/// An offer from Teams (from a public sample, with its ids, addresses and keys made up).
#[cfg(test)]
pub const TEAMS_OFFER: &str = include_str!("testdata/teams-offer.sdp");

/// Each m-line's lines, for tests.
#[cfg(test)]
pub fn sections_of(sdp: &str) -> Vec<Vec<String>> {
    let mut sections: Vec<Vec<String>> = Vec::new();
    for line in sdp.lines().map(str::trim_end) {
        if line.starts_with("m=") {
            sections.push(vec![line.to_string()]);
        } else if let Some(section) = sections.last_mut() {
            section.push(line.to_string());
        }
    }
    sections
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renames_only_x_data() {
        let prepared = prepare(TEAMS_OFFER).unwrap();
        assert_eq!(prepared.sections.len(), 9);
        let media: Vec<&str> = prepared.sections.iter().map(|s| s.media.as_str()).collect();
        assert_eq!(media, ["audio", "video", "video", "video", "video", "video", "video", "video", "x-data"]);
        let mids: Vec<&str> = prepared.sections.iter().filter_map(|s| s.mid.as_deref()).collect();
        assert_eq!(mids, ["0", "1", "2", "3", "4", "5", "6", "7", "8"]);
        assert!(prepared.sections.iter().all(|s| s.proto == "RTP/SAVP"));
        assert!(prepared.sdp.contains("\r\nm=application 50000 RTP/SAVP 127 126\r\n"));
        assert!(!prepared.sdp.contains("x-data 5"));
        // Everything but that one line is as it was.
        let before: Vec<&str> = TEAMS_OFFER.lines().collect();
        let after: Vec<&str> = prepared.sdp.lines().collect();
        assert_eq!(before.len(), after.len());
        assert_eq!(before.iter().zip(&after).filter(|(a, b)| a != b).count(), 1);
        assert_eq!(prepared.candidates, ["192.0.2.10:50000".parse::<std::net::SocketAddr>().unwrap()]);
        assert_eq!(prepared.opus, 111);
    }

    #[test]
    fn wants_audio_with_opus() {
        assert!(prepare("v=0\r\nm=video 9 RTP/SAVP 102\r\n").is_err());
        assert!(prepare("v=0\r\nm=audio 9 RTP/SAVP 0\r\na=rtpmap:0 PCMU/8000\r\n").is_err());
    }

    #[test]
    fn puts_kinds_and_protocols_back() {
        let prepared = prepare(TEAMS_OFFER).unwrap();
        let mut answer = String::from("v=0\r\na=group:BUNDLE 0\r\nm=audio 9 UDP/TLS/RTP/SAVPF 111\r\na=mid:0\r\na=sendrecv\r\n");
        for mid in 1..9 {
            let kind = if mid == 8 { "application" } else { "video" };
            answer.push_str(&format!("m={kind} 0 UDP/TLS/RTP/SAVPF 0\r\nc=IN IP4 0.0.0.0\r\na=mid:{mid}\r\n"));
        }
        let fixed = finish(&answer, &prepared, 42).unwrap();
        let sections = sections_of(&fixed);
        assert_eq!(sections[0][..4], ["m=audio 9 RTP/SAVP 111", "a=mid:0", "a=sendrecv", "a=x-ssrc-range:42-42"]);
        assert_eq!(sections[1], ["m=video 0 RTP/SAVP 102", "c=IN IP4 0.0.0.0", "a=mid:1", "a=rtpmap:102 H264/90000"]);
        assert_eq!(sections[2][0], "m=video 0 RTP/SAVP 102");
        assert_eq!(sections[8], ["m=x-data 0 RTP/SAVP 127", "c=IN IP4 0.0.0.0", "a=mid:8", "a=rtpmap:127 x-data/90000"]);
        assert_eq!(fixed.matches("x-ssrc-range").count(), 1);
        assert!(finish("v=0\r\nm=audio 9 UDP/TLS/RTP/SAVPF 111\r\n", &prepared, 42).is_err());
    }
}
