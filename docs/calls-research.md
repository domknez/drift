# Calls in drift: research

Can drift answer a 1:1 Teams call itself, with audio and later video, with no
other Teams client running and no paid services? This note covers what is known
about how Teams calls work, which parts drift already has, what is missing, and a
staged plan.

Nothing here has been tried against a real call yet. Every request below that
acts on a call (attach, accept, reject) still needs a test the user approves.

## Sources

- drift's own Trouter code (`src/teams/trouter.rs`). drift already registers for
  calls and gets event 107 when someone calls.
- The public npm package `@azure/communication-calling` 2.0.0, read as data and
  never run. It holds the same call-signalling stack Teams on the web uses. Its
  service table has `Enterprise_*` entries that are Teams' own:
  `Enterprise_ConversationServiceUrl: https://api.flightproxy.teams.microsoft.com/api/v2/epconv`,
  `Enterprise_MdnTrapServiceTokenUrl: https://teams.microsoft.com/trap/tokens`.
- Teams web's public configuration bundle (`config-prod-*.js` on
  `teams.public.onecdn.static.microsoft`), read as data. It lists `relayConfig`
  (TRAP token URL, TURN host `default.relay.teams.microsoft.com`, UDP 3478 and
  TCP/TLS 443) and the `/NGCallManagerWin/auth` and `/NGCallManagerOsx/auth`
  Trouter paths.
- A Microsoft Q&A thread with a full outgoing-call request to `.../api/v2/cpconv`
  (body, callback links, SDP):
  https://learn.microsoft.com/en-us/answers/questions/1293865/getting-400-error-from-https-api-flightproxy-skype
- A desktop-client network analysis showing STUN and TURN relay traffic in calls:
  https://ithelance.medium.com/how-microsoft-teams-works-under-the-hood-a-binary-analysis-179bf6b8ab8b
- A 2026 write-up of a custom client that joins Teams meetings and carries data over
  their WebRTC media, which shows the media path can be reproduced outside Microsoft's
  clients: https://blog.scrt.ch/2026/04/21/from-a-regular-red-team-exercise-to-developing-a-custom-c2-channel-over-ms-teams/
- Capabilities of the official SDK for Teams users (for comparison):
  https://learn.microsoft.com/en-us/azure/communication-services/concepts/interop/teams-user-calling

## How an incoming call works

### 1. Ringing (drift has this)

The call controller sends a request through Trouter to the endpoint registered for
`NextGenCalling` (drift registers as `DesktopNgc_2.3:SkypeNgc` at
`…/NGCallManagerWin`). The body has `evt: 107`, a `callId`, and `cp`, which is
Base64 gzipped JSON. The SDK reads these from it:

- `callNotification.from` (`id`, `displayName`) and `callNotification.to.participantId`
- `callNotification.links.attach` and `callNotification.links.redirection`
- `callNotification.onBehalfOf`, `callQueueContext`, `callPhoneLineType`, `transferContext`
- `conversationInvitation.conversationController` (the conversation's URL),
  `isMultiParty`, `isHostless`, `subject`
- sometimes the media offer itself (`getInitialMediaOfferFromIncomingCallPayload`)

The `calls` branch logs the shape of this payload at debug level (key paths, kinds
of value, link hosts and paths with ids taken out). The next real call shows the
exact links. See "Next step for the user".

### 2. Attach

The client POSTs to `callNotification.links.attach`:

```
{
  "attach": {
    "requireMediaContent": true,          // true when the notice had no offer
    "links": { "end": "<callback>/call/end/" },
    "locationContent": …, "networkContent": …, "areaContent": …
  },
  "capabilities": null,
  "endpointCapabilities": <number>,
  "additionalActions": [ { "input": { "conversationRequest": {
      "roster": { "type": "Delta", "rosterUpdate": "<callback>/conversation/rosterUpdate/" },
      "links": { "conversationEnd": …, "conversationUpdate": …, "localParticipantUpdate": … } } } } ]
}
```

The answer carries the call leg's links and the offer:

- `callInvitation.links.mediaAnswer`, `.acceptance`, `.callLeg`, `.redirection`, `.newOffer`
- `callInvitation.mediaContent`: `{ blob: <SDP>, contentType: "application/sdp-ngc-1.0" }`
- `callInvitation.callModalities` (`Audio`, `Video`, …), `participants.from`

`<callback>` is a URL on the client's own Trouter socket, under
`…/callAgent/<endpoint>/<hash>/`. The controller POSTs call events to it
(`/call/acceptance/`, `/call/mediaAcknowledgement/`, `/call/end/`,
`/conversation/rosterUpdate/`, …), and drift has to answer each with 200, as it
already does for messages.

### 3a. Decline

`DELETE callInvitation.links.callLeg` with the body
`{ "callEnd": { "code": <reject>, "subCode": …, "phrase": … } }`. This is the
"attach + DELETE on the call leg" from earlier. A decline needs an attach first,
because the call leg's link only comes in the attach answer.

### 3b. Accept

`POST callInvitation.links.acceptance`:

```
{
  "callAcceptance": {
    "acceptedBy": { "id", "displayName", "endpointId", "participantId", "languageId" },
    "acceptedCallModalities": ["Audio"],
    "endpointCapabilities": <number>,
    "links": { "mediaRenegotiation", "transfer", "replacement", "balanceUpdate", … callbacks },
    "mediaContent": { "blob": <SDP answer>, "contentType": "application/sdp-ngc-1.0" }
  }
}
```

The answer may carry `callAcceptanceAcknowledgement` inline. If it doesn't, the
acknowledgement comes to the `/call/acceptance/` callback. Hanging up is a POST to
the `end` link. Media renegotiation (adding video, hold) comes as
`/call/mediaRenegotiation/` with a new offer, answered at its `mediaAnswer` link.

## Media

- **SDP**: browser WebRTC SDP in a JSON envelope (`application/sdp-ngc-1.0`). The
  cpconv sample has `a=group:BUNDLE` over 9 m-lines: audio, a send/receive video, 6
  receive-only videos, screen sharing, and `m=x-data` (SCTP data). It also has
  `ice-ufrag`/`ice-pwd`, `fingerprint:sha-256`, `setup:actpass`, `rtcp-mux`, and
  Microsoft extras (`a=x-ssrc-range`, `a=x-signaling-fb`).
- **Transport**: the SDK builds a plain browser `RTCPeerConnection`
  (`bundlePolicy: "max-bundle"`, `rtcpMuxPolicy: "require"`). Its ICE servers are
  TURN entries with username and password. So media is standard ICE, DTLS-SRTP and
  SRTP, which a non-browser WebRTC stack can speak.
- **Relay credentials**: GET `https://teams.microsoft.com/trap/tokens` with the
  user's Skype token or a bearer token returns TURN credentials (refreshed early,
  `earlyRefreshMinutes: 9720`). The relays are
  `worldaz.relay.teams.microsoft.com` and `default.relay.teams.microsoft.com`, on
  UDP 3478 and TCP/TLS 443.
- **Codecs**: audio is Opus (111) with RED, G.722, PCMU/PCMA, CN and
  telephone-event. Opus alone is enough. Video is H.264 (H.265 appears as an
  option). SILK is only in the old Skype stack, and the web path doesn't need it.

## What Rust has for each part

| Part | Crate | Notes |
|---|---|---|
| ICE, DTLS, SRTP, SCTP, SDP | `webrtc` (webrtc-rs) | Pure Rust and does TURN. Open: whether it takes Microsoft's 9-m-line offer with `m=x-data`. Rewriting the offer before it reaches the stack (keep audio, mark the rest inactive) is the fallback. |
| Microphone and speakers | `cpal` | CoreAudio, WASAPI, ALSA/Pulse |
| Opus | `opus` / `audiopus` | libopus, built statically |
| Echo cancellation, noise, gain | `webrtc-audio-processing` | Bindings to WebRTC's APM (C++, vendored build). Without it a call is only usable on headphones. |
| Resampling | `rubato` | 48 kHz Opus to and from the device rate |
| Camera | `nokhwa` | Stage 3 |
| H.264 | `openh264` | Cisco's codec. Stage 3. |
| Video in the window | egui textures | drift already draws images |

## Unknowns, riskiest first

1. **The SDP dialect.** Microsoft's offer has many BUNDLEd m-lines, receive-only
   video and `m=x-data`. If webrtc-rs rejects the offer or answers in a way the
   media controller rejects, it needs rewriting or patching. This is the biggest
   technical risk.
2. **Being let in as a calling endpoint.** drift registers like the desktop NGC
   client and gets the ring. It isn't known yet whether attach and accept take
   drift's Skype token and endpoint id, or need headers or capabilities only the
   real client sends (`endpointCapabilities`, `x-microsoft-skype-*`).
3. **Callbacks through Trouter.** drift has to give its socket's URL with the
   `callAgent/…` path, and handle and acknowledge those requests. The
   socket and the ack are already there, so this is medium risk.
4. **Media interop details.** These are SSRC ranges, `x-signaling-fb`, which RTCP
   feedback the media controller expects, and the DTLS role.
5. **Audio quality.** Echo cancellation, jitter and device changes are known
   engineering work, but a lot of it.
6. **Change and terms.** Microsoft can change any of this without notice. Teams'
   terms don't allow reverse engineering, though EU law permits it for
   interoperability. A client that isn't Microsoft's could also be noticed.

## Plan

| Stage | What | Estimate | Needs from the user |
|---|---|---|---|
| 0 | Log the shape of call notices (this branch) | done | one real call, then send the `call shape` lines |
| 1 | **Decline for real**: attach with drift's callbacks, then DELETE the call leg. Also learn "answered elsewhere" and "caller hung up" from the callbacks, so drift's bar goes away. | 2–4 days | approve test calls from a colleague |
| 2 | **Answer with audio**: TRAP credentials, a webrtc-rs peer, rewriting the offer, the answer, acceptance and its acknowledgement, Opus and cpal with echo cancellation, a bar with mute and hang-up, renegotiation (hold) | 3–6 weeks | approve tests, and try calls with headphones first |
| 3 | **Video**: camera, H.264 both ways, the call view, `x-ssrc-range` and simulcast details | 3–5 weeks | tests |
| later | Outgoing calls (`epconv`), group calls, meetings | weeks each, meetings most | |

Stage 1 is small and useful on its own, and it tests unknowns 2 and 3 before the
big work starts.

## Next step for the user

1. Run the `calls` branch build (`cargo run --release` on the branch) and have a
   colleague call. Don't answer.
2. Send the lines from `~/Library/Logs/drift/drift.log` that contain
   `trouter: call shape`. They hold no names, ids, tokens or queries, only keys,
   kinds of value and where links go.
3. Approve, or not:
   - (a) a read-only GET of `https://teams.microsoft.com/trap/tokens`, to see the
     relay credentials' format. It changes nothing.
   - (b) stage 1 tests: attach and decline on calls a colleague makes on purpose.

## Media engine

`src/call/` holds the media side of a call, behind the `calls` Cargo feature (off
by default; without it `Media::answer` returns an error). It has no Teams HTTP in
it: it takes the offer's SDP text and the TURN servers, and gives back the answer's
SDP text and a running `Media` (`set_muted`, `connected`, `ended`, `close`).

### Crates

| Part | Crate | Why |
|---|---|---|
| ICE, DTLS-SRTP, SDP, TURN client | `rtc` 0.21 | webrtc-rs's sans-I/O core (the async `webrtc` 0.21 is a wrapper over it). drift has no async runtime; `rtc` needs none: one thread owns the socket and drives it. `str0m`, the other sans-I/O stack, has no TURN client and its SDP parser takes only `UDP/TLS/RTP/SAVPF` and SCTP, not Teams' `RTP/SAVP`. Crypto is `ring` (built with `cc`, no cmake). |
| Opus | `unsafe-libopus` 0.2 | libopus 1.3.1 translated to Rust by c2rust: no C toolchain, cmake or system library, so it builds the same on macOS, Linux and Windows. The `opus` crate builds libopus with cmake. |
| Microphone and speakers | `cpal` 0.18 | On Linux it needs ALSA's headers (`libasound2-dev`) when built with `calls`. |
| Resampling | none | Linear interpolation to and from the device rate (usually 48 kHz already). Enough for speech; `rubato` is the step up. |

### What the stack makes of Teams' offer

`src/call/testdata/teams-offer.sdp` is the offer from the Microsoft Q&A sample above
(9 m-lines: audio, a send/receive video, 6 receive-only videos, `m=x-data`), with
its ids, addresses, ICE credentials and fingerprint made up. Tests feed it through
the stack:

- **Rejected as is**: `m=x-data`. The parser takes only the IANA media kinds
  (`audio`, `video`, `text`, `application`, `message`, `image`) and fails the whole
  offer. `sdp::prepare` renames that m-line to `application`; nothing else changes.
- **Parsed and taken as is**: `RTP/SAVP` (not `UDP/TLS/RTP/SAVPF`) with a DTLS fingerprint,
  `a=x-ssrc-range`, `a=x-signaling-fb`, `a=label`, `a=x-data-protocol`,
  `a=sctp-port` on an RTP m-line, Microsoft's header extensions, the BUNDLE group
  over all 9, `a=setup:actpass`, the `ice-ufrag` and `ice-pwd` in every m-line.
- **Answered wrong, so fixed** (`sdp::finish`): the stack answers every m-line with
  `UDP/TLS/RTP/SAVPF` and a rejected one with format `0` and no `rtpmap`. The fix
  gives each m-line the offer's kind and protocol back (`m=x-data`, `RTP/SAVP`),
  a rejected one the offer's first format and its `rtpmap`, and the audio
  `a=x-ssrc-range:<ssrc>-<ssrc>`, as Teams' own SDP has.

The answer then has the offer's 9 m-lines with the same mids in the same order:
audio `m=audio 9 RTP/SAVP 111` with `opus/48000/2`, `minptime=10;useinbandfec=1`,
`a=sendrecv`, `a=setup:active`, `a=rtcp-mux`, one SSRC; the other 8 on port 0.
`a=group:BUNDLE 0` lists only the audio, since rejected m-lines leave the group
(RFC 8843). Whether Microsoft's media controller takes exactly this is the next
thing a real call shows.

### How it runs

- `Media::answer` is blocking. It binds one UDP socket, allocates a relay on each
  UDP TURN server (`turn:host:port`, `?transport=udp` or none) for up to 4 s, lets
  the offer's candidate addresses through each relay, and answers with every
  candidate (host and relay) and `a=end-of-candidates`. There is no trickle: the
  acceptance carries the whole answer.
- The offer's own candidates are read from its SDP; there is no call for adding
  more later.
- drift's DTLS role is `active` (client), as the answer to `actpass`.
- A thread then owns the socket, the stack and the TURN clients, and every 20 ms
  encodes a frame from the microphone (silence when muted) and sends it once ICE
  and DTLS are up. Received Opus is decoded (a short gap is filled by the decoder's
  loss concealment) and played after 60 ms of buffering.
- Audio sits behind `Source`, `Sink` and `Processor` traits. Tests use a tone and a
  recorder. `Processor` sees what is played and what is recorded: echo
  cancellation (for example `webrtc-audio-processing`) goes there; today it does
  nothing, so headphones first.
- A loopback test runs a second stack as "Teams" on 127.0.0.1, its offer made
  Teams-like (`RTP/SAVP`, Microsoft attributes, an `m=x-data` m-line): both connect
  over host candidates, each hears the other's tone through Opus at the right
  pitch, and muting silences drift's side.

### Not done yet

- TURN over TCP or TLS on 443 (only UDP 3478), and server-reflexive candidates.
- Renegotiation (hold, video): a new offer means a new `Media` for now.
- Echo cancellation, noise suppression, gain control, device changes mid-call.
