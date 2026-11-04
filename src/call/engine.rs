//! The media side of a call: one UDP socket, the WebRTC stack, TURN relays and the
//! audio, run on a thread of their own.

use super::audio::{self, Audio, Opener};
use super::opus::{Decoder, Encoder, FRAME};
use super::{IceServer, sdp};
use bytes::{Bytes, BytesMut};
use rtc::interceptor::Registry;
use rtc::media_stream::MediaStreamTrack;
use rtc::peer_connection::configuration::interceptor_registry::register_default_interceptors;
use rtc::peer_connection::configuration::media_engine::{MIME_TYPE_OPUS, MediaEngine};
use rtc::peer_connection::event::RTCPeerConnectionEvent;
use rtc::peer_connection::message::{RTCMessage, TaggedRTCMessage};
use rtc::peer_connection::sdp::RTCSessionDescription;
use rtc::peer_connection::state::RTCPeerConnectionState;
use rtc::peer_connection::transport::{CandidateConfig, CandidateHostConfig, CandidateRelayConfig, RTCIceCandidate, RTCIceCandidateInit};
use rtc::peer_connection::{RTCPeerConnection, RTCPeerConnectionBuilder};
use rtc::rtp_transceiver::RTCRtpSenderId;
use rtc::rtp_transceiver::rtp_sender::{
    RTCRtpCodec, RTCRtpCodecParameters, RTCRtpCodingParameters, RTCRtpEncodingParameters, RtpCodecKind,
};
use rtc::sansio::Protocol;
use rtc::shared::{TaggedBytesMut, TransportContext, TransportProtocol};
use rtc::turn::client::{Client, ClientConfig, Event as TurnEvent};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, ToSocketAddrs, UdpSocket};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// How long to wait for TURN relays before answering without them.
const GATHER: Duration = Duration::from_secs(4);
const TICK: Duration = Duration::from_millis(20);

/// A call's media, running until it's closed or dropped.
pub struct Media {
    state: Arc<State>,
    thread: Option<JoinHandle<()>>,
}

#[derive(Default)]
struct State {
    muted: AtomicBool,
    connected: AtomicBool,
    ended: AtomicBool,
    stop: AtomicBool,
}

/// How [`Media::answer_with`] differs from a real call's.
pub struct Options {
    /// The address to offer as the host candidate; by default the one the system
    /// routes to the relay (or the caller) from.
    pub host: Option<IpAddr>,
    pub audio: Opener,
}

impl Media {
    /// Takes Teams' offer (SDP text) and the TURN relays, gathers candidates (up to a
    /// few seconds, as the answer carries them all), and returns the running media and
    /// the answer (SDP text).
    pub fn answer(offer: &str, ice: &[IceServer]) -> Result<(Media, String), String> {
        Self::answer_with(offer, ice, Options { host: None, audio: Box::new(audio::devices) })
    }

    pub fn answer_with(offer: &str, ice: &[IceServer], options: Options) -> Result<(Media, String), String> {
        let prepared = sdp::prepare(offer)?;
        let relays = relay_servers(ice);
        let socket = UdpSocket::bind((options.host.unwrap_or(Ipv4Addr::UNSPECIFIED.into()), 0)).map_err(|e| format!("socket: {e}"))?;
        let toward = relays.first().map(|(addr, _)| *addr).or(prepared.candidates.first().copied());
        let host_ip = match (options.host, toward) {
            (Some(ip), _) => ip,
            (None, Some(addr)) => route_to(addr)?,
            (None, None) => return Err("no relay or candidate to reach the caller by".into()),
        };
        let host = SocketAddr::new(host_ip, socket.local_addr().map_err(|e| e.to_string())?.port());
        let turns = gather(&socket, host, relays, &prepared.candidates);

        let ssrc = fastrand::u32(1..);
        let (mut pc, sender) = negotiate(&prepared, ssrc)?;
        add_candidate(&mut pc, host_candidate(host))?;
        for relay in turns.iter().filter_map(|t| t.relay) {
            add_candidate(&mut pc, relay_candidate(relay, host))?;
        }
        pc.add_local_candidate(RTCIceCandidateInit::default()).map_err(|e| e.to_string())?;
        let answer = pc.create_answer(None).map_err(|e| format!("answer: {e}"))?;
        pc.set_local_description(Instant::now(), answer).map_err(|e| format!("answer: {e}"))?;
        let answer = pc.local_description().ok_or("answer: none")?.sdp;
        let answer = sdp::finish(&answer, &prepared, ssrc)?;

        let peer = Peer::new(pc, socket, host, turns, sender, ssrc, prepared.opus);
        Ok((start(peer, options.audio), answer))
    }

    pub fn set_muted(&self, muted: bool) {
        self.state.muted.store(muted, Ordering::Relaxed);
    }

    /// Whether media flows: ICE and DTLS are up.
    pub fn connected(&self) -> bool {
        self.state.connected.load(Ordering::Relaxed)
    }

    /// Whether the media failed or the other side closed it.
    pub fn ended(&self) -> bool {
        self.state.ended.load(Ordering::Relaxed)
    }

    /// Stops the media, as dropping it does.
    pub fn close(self) {}
}

impl Drop for Media {
    fn drop(&mut self) {
        self.state.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn start(peer: Peer, audio: Opener) -> Media {
    let state = Arc::new(State::default());
    let shared = state.clone();
    let thread = std::thread::Builder::new()
        .name("call media".into())
        .spawn(move || peer.run(audio(), &shared))
        .expect("spawn the call's media thread");
    Media { state, thread: Some(thread) }
}

/// A peer connection that takes `prepared` and sends Opus from `ssrc`.
fn negotiate(prepared: &sdp::Prepared, ssrc: u32) -> Result<(RTCPeerConnection, RTCRtpSenderId), String> {
    let codec = opus_codec();
    let mut pc = peer_connection(prepared.opus)?;
    let offer = RTCSessionDescription::offer(prepared.sdp.clone()).map_err(|e| format!("offer: {e}"))?;
    pc.set_remote_description(Instant::now(), offer).map_err(|e| format!("offer: {e}"))?;
    let sender = pc.add_track(audio_track(ssrc, codec)).map_err(|e| format!("audio track: {e}"))?;
    Ok((pc, sender))
}

fn opus_codec() -> RTCRtpCodec {
    RTCRtpCodec {
        mime_type: MIME_TYPE_OPUS.to_owned(),
        clock_rate: 48_000,
        channels: 2,
        sdp_fmtp_line: "minptime=10;useinbandfec=1".into(),
        rtcp_feedback: vec![],
    }
}

/// A peer connection that knows Opus only, as payload type `opus`.
fn peer_connection(opus: u8) -> Result<RTCPeerConnection, String> {
    let mut media = MediaEngine::default();
    media
        .register_codec(RTCRtpCodecParameters { rtp_codec: opus_codec(), payload_type: opus }, RtpCodecKind::Audio)
        .map_err(|e| e.to_string())?;
    let registry = register_default_interceptors(Registry::new(), &mut media).map_err(|e| e.to_string())?;
    RTCPeerConnectionBuilder::new()
        .with_media_engine(media)
        .with_interceptor_registry(registry)
        .build(Instant::now())
        .map_err(|e| e.to_string())
}

fn audio_track(ssrc: u32, codec: RTCRtpCodec) -> MediaStreamTrack {
    let encoding = RTCRtpEncodingParameters {
        rtp_coding_parameters: RTCRtpCodingParameters { ssrc: Some(ssrc), ..Default::default() },
        codec,
        ..Default::default()
    };
    MediaStreamTrack::new("drift".into(), "drift-audio".into(), "main-audio".into(), RtpCodecKind::Audio, vec![encoding])
}

fn host_candidate(addr: SocketAddr) -> Result<RTCIceCandidate, String> {
    let config = CandidateHostConfig { base_config: candidate_config(addr), ..Default::default() };
    config.new_candidate_host().map(|c| RTCIceCandidate::from(&c)).map_err(|e| e.to_string())
}

fn relay_candidate(addr: SocketAddr, base: SocketAddr) -> Result<RTCIceCandidate, String> {
    let config = CandidateRelayConfig {
        base_config: candidate_config(addr),
        rel_addr: base.ip().to_string(),
        rel_port: base.port(),
        ..Default::default()
    };
    config.new_candidate_relay().map(|c| RTCIceCandidate::from(&c)).map_err(|e| e.to_string())
}

fn candidate_config(addr: SocketAddr) -> CandidateConfig {
    CandidateConfig { network: "udp".to_owned(), address: addr.ip().to_string(), port: addr.port(), component: 1, ..Default::default() }
}

fn add_candidate(pc: &mut RTCPeerConnection, candidate: Result<RTCIceCandidate, String>) -> Result<(), String> {
    let init = candidate?.to_json().map_err(|e| e.to_string())?;
    pc.add_local_candidate(init).map_err(|e| format!("candidate: {e}"))
}

/// The local address the system would send to `addr` from (nothing is sent).
fn route_to(addr: SocketAddr) -> Result<IpAddr, String> {
    let any: IpAddr = if addr.is_ipv4() { Ipv4Addr::UNSPECIFIED.into() } else { std::net::Ipv6Addr::UNSPECIFIED.into() };
    let socket = UdpSocket::bind((any, 0)).map_err(|e| e.to_string())?;
    socket.connect(addr).map_err(|e| format!("no route to {addr}: {e}"))?;
    Ok(socket.local_addr().map_err(|e| e.to_string())?.ip())
}

/// The UDP TURN servers among `ice`, resolved, with their credentials.
fn relay_servers(ice: &[IceServer]) -> Vec<(SocketAddr, &IceServer)> {
    let mut servers: Vec<(SocketAddr, &IceServer)> = Vec::new();
    for server in ice {
        for url in &server.urls {
            let Some(rest) = url.strip_prefix("turn:") else { continue };
            let (host, query) = rest.split_once('?').unwrap_or((rest, ""));
            if !query.is_empty() && !query.split('&').any(|q| q.eq_ignore_ascii_case("transport=udp")) {
                continue;
            }
            let host = if host.rsplit_once(':').is_some_and(|(_, p)| p.parse::<u16>().is_ok()) {
                host.to_string()
            } else {
                format!("{host}:3478")
            };
            match host.to_socket_addrs().map(|mut a| a.find(SocketAddr::is_ipv4)) {
                Ok(Some(addr)) if !servers.iter().any(|(a, _)| *a == addr) => servers.push((addr, server)),
                Ok(_) => {}
                Err(e) => log_warn!("call: relay {host}: {e}"),
            }
        }
    }
    servers
}

/// One TURN server and the relay address it gave, if it did.
struct Turn {
    server: SocketAddr,
    client: Client,
    relay: Option<SocketAddr>,
    failed: bool,
}

/// Allocates a relay on each server, waiting at most [`GATHER`], and lets the
/// caller's candidates through each one that answered.
fn gather(socket: &UdpSocket, local: SocketAddr, servers: Vec<(SocketAddr, &IceServer)>, peers: &[SocketAddr]) -> Vec<Turn> {
    let mut turns = Vec::new();
    for (server, ice) in servers {
        let config = ClientConfig {
            turn_serv_addr: server.to_string(),
            local_addr: local,
            transport_protocol: TransportProtocol::UDP,
            username: ice.username.clone(),
            password: ice.credential.clone(),
            ..Default::default()
        };
        let client =
            rtc::crypto::default_provider().map_err(|e| e.to_string()).and_then(|p| Client::new(config, p).map_err(|e| e.to_string()));
        match client {
            Ok(mut client) => match client.allocate(Instant::now()) {
                Ok(_) => turns.push(Turn { server, client, relay: None, failed: false }),
                Err(e) => log_warn!("call: relay {server}: {e}"),
            },
            Err(e) => log_warn!("call: relay {server}: {e}"),
        }
    }
    let deadline = Instant::now() + GATHER;
    let mut buf = vec![0u8; 2048];
    while turns.iter().any(|t| t.relay.is_none() && !t.failed) && Instant::now() < deadline {
        for turn in &mut turns {
            while let Some(out) = turn.client.poll_write() {
                let _ = socket.send_to(&out.message, out.transport.peer_addr);
            }
            while let Some(event) = turn.client.poll_event() {
                match event {
                    TurnEvent::AllocateResponse(_, relay) => {
                        log_info!("call: relay allocated on {}", turn.server);
                        turn.relay = Some(relay);
                    }
                    TurnEvent::AllocateError(_, e) => {
                        log_warn!("call: relay {}: {e}", turn.server);
                        turn.failed = true;
                    }
                    TurnEvent::TransactionTimeout(_) => turn.failed = true,
                    _ => {}
                }
            }
        }
        let _ = socket.set_read_timeout(Some(Duration::from_millis(20)));
        if let Ok((n, from)) = socket.recv_from(&mut buf)
            && let Some(turn) = turns.iter_mut().find(|t| t.server == from)
        {
            let _ = turn.client.handle_read(tagged(local, from, &buf[..n]));
        }
        for turn in &mut turns {
            let _ = turn.client.handle_timeout(Instant::now());
        }
    }
    turns.retain(|t| t.relay.is_some());
    for turn in &mut turns {
        let relay = turn.relay.unwrap();
        for &peer in peers {
            if let Ok(mut r) = turn.client.relay(relay) {
                let _ = r.create_permission(Instant::now(), peer);
            }
        }
    }
    turns
}

fn tagged(local: SocketAddr, peer: SocketAddr, data: &[u8]) -> TaggedBytesMut {
    TaggedBytesMut {
        now: Instant::now(),
        transport: TransportContext { local_addr: local, peer_addr: peer, ecn: None, transport_protocol: TransportProtocol::UDP },
        message: BytesMut::from(data),
    }
}

/// Everything the media thread owns.
struct Peer {
    pc: RTCPeerConnection,
    socket: UdpSocket,
    host: SocketAddr,
    turns: Vec<Turn>,
    sender: RTCRtpSenderId,
    ssrc: u32,
    payload_type: u8,
    sequence: u16,
    timestamp: u32,
    talking: bool,
    /// The last sequence number received, for spotting lost packets.
    received: Option<u16>,
}

impl Peer {
    fn new(
        pc: RTCPeerConnection,
        socket: UdpSocket,
        host: SocketAddr,
        turns: Vec<Turn>,
        sender: RTCRtpSenderId,
        ssrc: u32,
        payload_type: u8,
    ) -> Self {
        Peer {
            pc,
            socket,
            host,
            turns,
            sender,
            ssrc,
            payload_type,
            sequence: fastrand::u16(..),
            timestamp: fastrand::u32(..),
            talking: false,
            received: None,
        }
    }

    fn run(mut self, mut audio: Audio, state: &State) {
        let (mut encoder, mut decoder) = match (Encoder::new(), Decoder::new()) {
            (Ok(e), Ok(d)) => (e, d),
            (Err(e), _) | (_, Err(e)) => {
                log_error!("call: {e}");
                state.ended.store(true, Ordering::Relaxed);
                return;
            }
        };
        let mut frame = vec![0f32; FRAME];
        let mut buf = vec![0u8; 2048];
        let mut tick = Instant::now() + TICK;
        while !state.stop.load(Ordering::Relaxed) {
            self.flush();
            self.events(state);
            self.receive(&mut decoder, &mut audio);

            let now = Instant::now();
            let deadline = [self.pc.poll_timeout(), Some(tick)]
                .into_iter()
                .chain(self.turns.iter_mut().map(|t| t.client.poll_timeout()))
                .flatten()
                .min()
                .unwrap_or(tick);
            let wait = deadline.saturating_duration_since(now).max(Duration::from_millis(1));
            let _ = self.socket.set_read_timeout(Some(wait));
            if let Ok((n, from)) = self.socket.recv_from(&mut buf) {
                self.incoming(from, &buf[..n]);
            }

            let now = Instant::now();
            if self.pc.poll_timeout().is_some_and(|t| t <= now) {
                let _ = self.pc.handle_timeout(now);
            }
            for turn in &mut self.turns {
                if turn.client.poll_timeout().is_some_and(|t| t <= now) {
                    let _ = turn.client.handle_timeout(now);
                }
            }
            if now >= tick {
                tick = if now > tick + TICK * 5 { now + TICK } else { tick + TICK };
                audio.source.read(&mut frame);
                audio.processor.near(&mut frame);
                if state.muted.load(Ordering::Relaxed) {
                    frame.fill(0.0);
                }
                if state.connected.load(Ordering::Relaxed) {
                    match encoder.encode(&frame) {
                        Ok(packet) => self.send(packet),
                        Err(e) => log_warn!("call: {e}"),
                    }
                }
            }
        }
        let _ = self.pc.close();
        self.flush();
    }

    fn incoming(&mut self, from: SocketAddr, data: &[u8]) {
        if let Some(turn) = self.turns.iter_mut().find(|t| t.server == from) {
            let _ = turn.client.handle_read(tagged(self.host, from, data));
            while let Some(event) = turn.client.poll_event() {
                match event {
                    TurnEvent::DataIndicationOrChannelData(_, peer, data) => {
                        let relay = turn.relay.unwrap_or(self.host);
                        if let Err(e) = self.pc.handle_read(tagged(relay, peer, &data)) {
                            log_debug!("call: relayed packet: {e}");
                        }
                    }
                    TurnEvent::CreatePermissionError(_, e) => log_warn!("call: relay permission: {e}"),
                    _ => {}
                }
            }
        } else if let Err(e) = self.pc.handle_read(tagged(self.host, from, data)) {
            log_debug!("call: packet: {e}");
        }
    }

    /// Sends what the stack and the relays have queued.
    fn flush(&mut self) {
        while let Some(out) = self.pc.poll_write() {
            let to = out.transport.peer_addr;
            match self.turns.iter_mut().find(|t| t.relay == Some(out.transport.local_addr)) {
                Some(turn) => {
                    let relay = turn.relay.unwrap();
                    let Ok(mut r) = turn.client.relay(relay) else { continue };
                    if r.send_to(out.now, &out.message, to).is_err() {
                        // No permission yet: ask, and let ICE send again.
                        let _ = r.create_permission(out.now, to);
                    }
                }
                None => {
                    let _ = self.socket.send_to(&out.message, to);
                }
            }
        }
        for turn in &mut self.turns {
            while let Some(out) = turn.client.poll_write() {
                let _ = self.socket.send_to(&out.message, out.transport.peer_addr);
            }
        }
    }

    fn events(&mut self, state: &State) {
        while let Some(event) = self.pc.poll_event() {
            if let RTCPeerConnectionEvent::OnConnectionStateChangeEvent(s) = event {
                log_info!("call: media {s}");
                state.connected.store(s == RTCPeerConnectionState::Connected, Ordering::Relaxed);
                if matches!(s, RTCPeerConnectionState::Failed | RTCPeerConnectionState::Closed) {
                    state.ended.store(true, Ordering::Relaxed);
                }
            }
        }
    }

    fn receive(&mut self, decoder: &mut Decoder, audio: &mut Audio) {
        while let Some(TaggedRTCMessage { message, .. }) = self.pc.poll_read() {
            let RTCMessage::RtpPacket(_, packet) = message else { continue };
            let sequence = packet.header.sequence_number;
            let lost = self.received.map_or(0, |last| sequence.wrapping_sub(last).wrapping_sub(1));
            if lost > 0x8000 {
                continue;
            }
            self.received = Some(sequence);
            for _ in 0..lost.min(3) {
                if let Ok(pcm) = decoder.decode(None) {
                    audio.processor.far(&pcm);
                    audio.sink.write(&pcm);
                }
            }
            match decoder.decode(Some(&packet.payload)) {
                Ok(pcm) => {
                    audio.processor.far(&pcm);
                    audio.sink.write(&pcm);
                }
                Err(e) => log_debug!("call: {e}"),
            }
        }
    }

    fn send(&mut self, payload: Vec<u8>) {
        let packet = rtc::rtp::Packet {
            header: rtc::rtp::Header {
                version: 2,
                marker: !self.talking,
                payload_type: self.payload_type,
                sequence_number: self.sequence,
                timestamp: self.timestamp,
                ssrc: self.ssrc,
                ..Default::default()
            },
            payload: Bytes::from(payload),
        };
        self.talking = true;
        self.sequence = self.sequence.wrapping_add(1);
        self.timestamp = self.timestamp.wrapping_add(FRAME as u32);
        if let Some(mut sender) = self.pc.rtp_sender(self.sender)
            && let Err(e) = sender.write_rtp(Instant::now(), packet)
        {
            log_debug!("call: send: {e}");
        }
    }
}

#[cfg(test)]
mod tests;
