//! A call's media (audio for now): Teams' SDP offer in, the answer out, then Opus
//! over ICE, DTLS-SRTP and TURN between the devices and the caller. Nothing here
//! talks to Teams; the signalling around it does.
//!
//! The engine needs the `calls` feature. Without it, [`Media::answer`] says so.

#[cfg(feature = "calls")]
mod audio;
#[cfg(feature = "calls")]
mod engine;
#[cfg(feature = "calls")]
mod opus;
#[cfg(feature = "calls")]
mod sdp;

#[cfg(feature = "calls")]
pub use engine::Media;

/// A STUN or TURN server, as Teams' relay tokens list them.
#[derive(Debug, Clone, Default)]
#[cfg_attr(not(feature = "calls"), allow(dead_code, reason = "only the engine reads them"))]
pub struct IceServer {
    pub urls: Vec<String>,
    pub username: String,
    pub credential: String,
}

/// Stands in for the engine in builds without the `calls` feature.
#[cfg(not(feature = "calls"))]
pub struct Media;

#[cfg(not(feature = "calls"))]
impl Media {
    pub fn answer(_offer: &str, _ice: &[IceServer]) -> Result<(Media, String), String> {
        Err("this build of drift can't answer calls (it was built without the `calls` feature)".into())
    }

    pub fn set_muted(&self, _muted: bool) {}

    pub fn connected(&self) -> bool {
        false
    }

    pub fn ended(&self) -> bool {
        true
    }

    /// Stops the media, as dropping it does.
    pub fn close(self) {}
}
