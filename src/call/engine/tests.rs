use super::*;
use crate::call::audio::{Passthrough, Sink, Tone};
use crate::call::sdp::{TEAMS_OFFER, sections_of};
use rtc::rtp_transceiver::{RTCRtpTransceiverDirection, RTCRtpTransceiverInit};
use std::sync::Mutex;

/// The stack's answer to Teams' offer, before candidates.
fn answer_teams_offer() -> String {
    let prepared = sdp::prepare(TEAMS_OFFER).unwrap();
    let (mut pc, _) = negotiate(&prepared, 4242).unwrap();
    let answer = pc.create_answer(None).unwrap();
    pc.set_local_description(Instant::now(), answer).unwrap();
    sdp::finish(&pc.local_description().unwrap().sdp, &prepared, 4242).unwrap()
}

#[test]
fn the_stack_needs_x_data_renamed() {
    let raw = TEAMS_OFFER.replace('\n', "\r\n");
    assert!(RTCSessionDescription::offer(raw).is_err());
}

#[test]
fn answers_the_teams_offer() {
    let answer = answer_teams_offer();
    let sections = sections_of(&answer);
    let offered = sections_of(TEAMS_OFFER);
    assert_eq!(sections.len(), offered.len());

    for (ours, theirs) in sections.iter().zip(&offered) {
        let kind = |s: &[String]| s[0].split_whitespace().next().unwrap().to_string();
        assert_eq!(kind(ours), kind(theirs));
        let mid = |s: &[String]| s.iter().find(|l| l.starts_with("a=mid:")).cloned();
        assert_eq!(mid(ours), mid(theirs));
    }

    let audio = &sections[0];
    assert_eq!(audio[0], "m=audio 9 RTP/SAVP 111");
    for line in ["a=rtpmap:111 opus/48000/2", "a=sendrecv", "a=setup:active", "a=rtcp-mux", "a=x-ssrc-range:4242-4242"] {
        assert!(audio.iter().any(|l| l == line), "audio has no `{line}`:\n{answer}");
    }
    assert!(audio.iter().any(|l| l.starts_with("a=ice-ufrag:")));
    assert!(audio.iter().any(|l| l == "a=ssrc:4242 cname:drift"));

    for section in &sections[1..] {
        let mut fields = section[0].split_whitespace();
        let (_, port, proto) = (fields.next(), fields.next(), fields.next());
        assert_eq!((port, proto), (Some("0"), Some("RTP/SAVP")), "{}", section[0]);
        assert_eq!(fields.count(), 1);
    }
    assert_eq!(sections[8][0], "m=x-data 0 RTP/SAVP 127");

    assert!(answer.contains("\r\na=fingerprint:sha-256 "));
    let bundle: Vec<&str> = answer.lines().find_map(|l| l.strip_prefix("a=group:BUNDLE ")).unwrap().split(' ').collect();
    assert_eq!(bundle, ["0"]);
    assert!(answer.ends_with("\r\n") && !answer.contains("\n\n"));
}

/// Samples a call played.
#[derive(Clone, Default)]
struct Record(Arc<Mutex<Vec<f32>>>);

impl Sink for Record {
    fn write(&mut self, samples: &[f32]) {
        self.0.lock().unwrap().extend_from_slice(samples);
    }
}

fn tone(hz: f32, record: &Record) -> Opener {
    let record = record.clone();
    Box::new(move || Audio { source: Box::new(Tone::new(hz)), sink: Box::new(record), processor: Box::new(Passthrough) })
}

/// Rising zero crossings a second in the last tenth of a second played.
fn pitch(record: &Record) -> usize {
    let samples = record.0.lock().unwrap();
    let tail = &samples[samples.len().saturating_sub(4_800)..];
    tail.windows(2).filter(|w| w[0] < 0.0 && w[1] >= 0.0).count() * 10
}

/// Teams-style SDP from the stack's own offer: `RTP/SAVP`, Microsoft's attributes,
/// and a data m-line of the kind only Teams has.
fn teamsify(offer: &str) -> String {
    let mut lines = Vec::new();
    let mut ice = Vec::new();
    for line in offer.lines() {
        if line.starts_with("a=ice-ufrag:") || line.starts_with("a=ice-pwd:") {
            ice.push(line.to_string());
        }
        if let Some(group) = line.strip_prefix("a=group:BUNDLE ") {
            lines.push(format!("a=group:BUNDLE {group} x"));
            continue;
        }
        lines.push(line.replace("UDP/TLS/RTP/SAVPF", "RTP/SAVP"));
        if line.starts_with("a=mid:") {
            lines.push("a=x-signaling-fb:* x-message app send:src recv:src,vc".into());
        }
        if line == "a=mid:0" {
            lines.push("a=label:main-audio".into());
            lines.push("a=x-ssrc-range:7777-7777".into());
        }
    }
    lines.extend(["m=x-data 9 RTP/SAVP 127", "c=IN IP4 0.0.0.0", "a=rtpmap:127 x-data/90000", "a=mid:x", "a=sendrecv"].map(String::from));
    lines.extend(ice.into_iter().take(2));
    lines.extend(["a=rtcp-mux", "a=sctp-port:5000", "a=x-data-protocol:sctp"].map(String::from));
    lines.iter().map(|l| format!("{l}\r\n")).collect()
}

/// The answer as the stand-in's stack takes it: without the data m-line.
fn untamsify(answer: &str) -> String {
    let end = answer.find("m=x-data").unwrap();
    answer[..end].to_string()
}

/// Two peers on this machine: one standing in for Teams (the stack as offerer, with
/// its SDP made Teams-like), and drift's engine answering it. Each sends a tone and
/// must hear the other's.
#[test]
fn loopback_carries_audio_both_ways() {
    let localhost: IpAddr = Ipv4Addr::LOCALHOST.into();
    let mut media = MediaEngine::default();
    media.register_codec(RTCRtpCodecParameters { rtp_codec: opus_codec(), payload_type: 111 }, RtpCodecKind::Audio).unwrap();
    let h264 = RTCRtpCodec {
        mime_type: "video/H264".into(),
        clock_rate: 90_000,
        sdp_fmtp_line: "level-asymmetry-allowed=1;packetization-mode=1;profile-level-id=42e01f".into(),
        ..Default::default()
    };
    media.register_codec(RTCRtpCodecParameters { rtp_codec: h264, payload_type: 102 }, RtpCodecKind::Video).unwrap();
    let registry = register_default_interceptors(Registry::new(), &mut media).unwrap();
    let mut teams =
        RTCPeerConnectionBuilder::new().with_media_engine(media).with_interceptor_registry(registry).build(Instant::now()).unwrap();
    let sender = teams.add_track(audio_track(7777, opus_codec())).unwrap();
    for _ in 0..2 {
        let init = RTCRtpTransceiverInit { direction: RTCRtpTransceiverDirection::Recvonly, ..Default::default() };
        teams.add_transceiver_from_kind(RtpCodecKind::Video, Some(init)).unwrap();
    }
    let offer = teams.create_offer(None).unwrap();
    teams.set_local_description(Instant::now(), offer).unwrap();
    let socket = UdpSocket::bind((localhost, 0)).unwrap();
    let addr = socket.local_addr().unwrap();
    add_candidate(&mut teams, host_candidate(addr)).unwrap();
    teams.add_local_candidate(RTCIceCandidateInit::default()).unwrap();
    let offer = teamsify(&teams.local_description().unwrap().sdp);
    assert_eq!(sections_of(&offer).len(), 4);

    let heard_by_drift = Record::default();
    let (drift, answer) = Media::answer_with(&offer, &[], Options { host: Some(localhost), audio: tone(440.0, &heard_by_drift) }).unwrap();
    assert_eq!(sections_of(&answer).len(), 4);
    assert!(answer.contains("a=candidate:"));

    let answer = RTCSessionDescription::answer(untamsify(&answer)).unwrap();
    teams.set_remote_description(Instant::now(), answer).unwrap();
    let heard_by_teams = Record::default();
    let teams = start(Peer::new(teams, socket, addr, Vec::new(), sender, 7777, 111), tone(1_000.0, &heard_by_teams));

    let deadline = Instant::now() + Duration::from_secs(15);
    let enough = |r: &Record| r.0.lock().unwrap().len() >= 48_000;
    while !(enough(&heard_by_drift) && enough(&heard_by_teams)) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(drift.connected() && teams.connected());
    assert!(enough(&heard_by_drift) && enough(&heard_by_teams), "too little audio");
    // Each hears the other's tone, through Opus: about 1 kHz and 440 Hz.
    let (drift_hears, teams_hears) = (pitch(&heard_by_drift), pitch(&heard_by_teams));
    assert!((900..=1_100).contains(&drift_hears), "drift hears {drift_hears} Hz");
    assert!((400..=480).contains(&teams_hears), "teams hears {teams_hears} Hz");

    drift.set_muted(true);
    std::thread::sleep(Duration::from_millis(500));
    let samples = heard_by_teams.0.lock().unwrap();
    let tail = &samples[samples.len() - 4_800..];
    assert!(tail.iter().all(|s| s.abs() < 0.01), "muted, yet teams hears sound");
    drop(samples);
    drift.close();
    teams.close();
}
