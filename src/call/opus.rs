//! Opus at 48 kHz mono, over libopus translated to Rust (no C toolchain needed).

use unsafe_libopus::{
    OPUS_APPLICATION_VOIP, OPUS_OK, OPUS_SET_BITRATE_REQUEST, OPUS_SET_INBAND_FEC_REQUEST, OPUS_SET_PACKET_LOSS_PERC_REQUEST, OpusDecoder,
    OpusEncoder, opus_decode_float, opus_decoder_create, opus_decoder_destroy, opus_encode_float, opus_encoder_create,
    opus_encoder_destroy,
};

/// Samples in one 20 ms frame at 48 kHz.
pub const FRAME: usize = 960;
/// The longest frame a packet can hold (120 ms).
const MAX_FRAME: usize = FRAME * 6;

/// Turns 20 ms frames of the microphone into Opus packets.
pub struct Encoder(*mut OpusEncoder);

// The encoder's state is only reached through `&mut self`.
unsafe impl Send for Encoder {}

impl Encoder {
    pub fn new() -> Result<Self, String> {
        let mut error = 0;
        let state = unsafe { opus_encoder_create(48_000, 1, OPUS_APPLICATION_VOIP, &mut error) };
        if state.is_null() || error != OPUS_OK {
            return Err(format!("opus encoder: error {error}"));
        }
        let encoder = Encoder(state);
        encoder.set(OPUS_SET_BITRATE_REQUEST, 32_000)?;
        // Forward error correction, which Teams asks for (`useinbandfec=1`).
        encoder.set(OPUS_SET_INBAND_FEC_REQUEST, 1)?;
        encoder.set(OPUS_SET_PACKET_LOSS_PERC_REQUEST, 10)?;
        Ok(encoder)
    }

    fn set(&self, request: i32, value: i32) -> Result<(), String> {
        let status = unsafe { unsafe_libopus::opus_encoder_ctl!(self.0, request, value) };
        if status == OPUS_OK { Ok(()) } else { Err(format!("opus encoder setting {request}: error {status}")) }
    }

    /// One packet for one frame of [`FRAME`] samples.
    pub fn encode(&mut self, frame: &[f32]) -> Result<Vec<u8>, String> {
        debug_assert_eq!(frame.len(), FRAME);
        let mut packet = vec![0u8; 1500];
        let n = unsafe { opus_encode_float(self.0, frame.as_ptr(), FRAME as i32, packet.as_mut_ptr(), packet.len() as i32) };
        if n < 0 {
            return Err(format!("opus encode: error {n}"));
        }
        packet.truncate(n as usize);
        Ok(packet)
    }
}

impl Drop for Encoder {
    fn drop(&mut self) {
        unsafe { opus_encoder_destroy(self.0) }
    }
}

/// Turns Opus packets (mono or stereo) back into 48 kHz mono samples.
pub struct Decoder(*mut OpusDecoder);

// The decoder's state is only reached through `&mut self`.
unsafe impl Send for Decoder {}

impl Decoder {
    pub fn new() -> Result<Self, String> {
        let mut error = 0;
        let state = unsafe { opus_decoder_create(48_000, 1, &mut error) };
        if state.is_null() || error != OPUS_OK {
            return Err(format!("opus decoder: error {error}"));
        }
        Ok(Decoder(state))
    }

    /// The samples in `packet`, or a guess at a lost frame's when there is none.
    pub fn decode(&mut self, packet: Option<&[u8]>) -> Result<Vec<f32>, String> {
        let mut pcm = vec![0f32; MAX_FRAME];
        let (data, len, frame) = match packet {
            Some(p) => (p.as_ptr(), p.len() as i32, MAX_FRAME as i32),
            None => (std::ptr::null(), 0, FRAME as i32),
        };
        let n = unsafe { opus_decode_float(self.0, data, len, pcm.as_mut_ptr(), frame, 0) };
        if n < 0 {
            return Err(format!("opus decode: error {n}"));
        }
        pcm.truncate(n as usize);
        Ok(pcm)
    }
}

impl Drop for Decoder {
    fn drop(&mut self) {
        unsafe { opus_decoder_destroy(self.0) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tone_survives_a_round_trip() {
        let mut encoder = Encoder::new().unwrap();
        let mut decoder = Decoder::new().unwrap();
        let mut energy = 0.0;
        for n in 0..10 {
            let frame: Vec<f32> =
                (0..FRAME).map(|i| (((n * FRAME + i) as f32) * 440.0 * std::f32::consts::TAU / 48_000.0).sin() * 0.5).collect();
            let packet = encoder.encode(&frame).unwrap();
            assert!(!packet.is_empty() && packet.len() < 400);
            let pcm = decoder.decode(Some(&packet)).unwrap();
            assert_eq!(pcm.len(), FRAME);
            energy = pcm.iter().map(|s| s * s).sum::<f32>() / FRAME as f32;
        }
        // A sine at 0.5 has a mean square of 0.125.
        assert!(energy > 0.05, "energy {energy}");
        assert_eq!(decoder.decode(None).unwrap().len(), FRAME);
    }
}
