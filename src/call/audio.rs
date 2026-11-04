//! Where a call's sound comes from and goes to: 20 ms frames of 48 kHz mono, from
//! the microphone and to the speakers, or from and to anything else in tests.

use super::opus::FRAME;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

pub const RATE: u32 = 48_000;
/// Sound kept waiting for the speakers before they start, against network jitter.
const PREBUFFER: usize = FRAME * 3;
/// The most sound kept waiting either way; older sound is dropped to stay in time.
const MOST: usize = FRAME * 10;

/// Sound to send: fills each frame of [`FRAME`] samples.
pub trait Source {
    fn read(&mut self, frame: &mut [f32]);
}

/// Sound received: takes decoded frames of any length.
pub trait Sink {
    fn write(&mut self, samples: &[f32]);
}

/// What runs between the devices and the codec. Echo cancellation goes here: it
/// sees what the speakers play (`far`) and cleans what the microphone picks up
/// (`near`) before it's sent.
pub trait Processor {
    fn far(&mut self, _samples: &[f32]) {}
    fn near(&mut self, _frame: &mut [f32]) {}
}

/// No processing, for headphones.
pub struct Passthrough;

impl Processor for Passthrough {}

/// Nothing to send.
pub struct Silence;

impl Source for Silence {
    fn read(&mut self, frame: &mut [f32]) {
        frame.fill(0.0);
    }
}

/// Nowhere to play.
pub struct Discard;

impl Sink for Discard {
    fn write(&mut self, _samples: &[f32]) {}
}

/// A sine wave.
pub struct Tone {
    pub hz: f32,
    phase: f32,
}

impl Tone {
    pub fn new(hz: f32) -> Self {
        Tone { hz, phase: 0.0 }
    }
}

impl Source for Tone {
    fn read(&mut self, frame: &mut [f32]) {
        for s in frame {
            *s = self.phase.sin() * 0.5;
            self.phase = (self.phase + self.hz * std::f32::consts::TAU / RATE as f32) % std::f32::consts::TAU;
        }
    }
}

/// What a call plays and records with, opened on the thread that uses it.
pub struct Audio {
    pub source: Box<dyn Source>,
    pub sink: Box<dyn Sink>,
    pub processor: Box<dyn Processor>,
}

/// Opens the sound a call uses, on the media thread (some systems' streams must
/// stay on the thread that made them).
pub type Opener = Box<dyn FnOnce() -> Audio + Send>;

/// The default microphone and speakers, or silence for whichever can't be opened.
pub fn devices() -> Audio {
    let source: Box<dyn Source> = match Microphone::open() {
        Ok(mic) => Box::new(mic),
        Err(e) => {
            log_warn!("call: no microphone: {e}");
            Box::new(Silence)
        }
    };
    let sink: Box<dyn Sink> = match Speakers::open() {
        Ok(speakers) => Box::new(speakers),
        Err(e) => {
            log_warn!("call: no speakers: {e}");
            Box::new(Discard)
        }
    };
    Audio { source, sink, processor: Box::new(Passthrough) }
}

/// The default input device, resampled to 48 kHz mono.
pub struct Microphone {
    _stream: cpal::Stream,
    buffer: Arc<Mutex<VecDeque<f32>>>,
}

impl Microphone {
    pub fn open() -> Result<Self, String> {
        let device = cpal::default_host().default_input_device().ok_or("no input device")?;
        let supported = device.default_input_config().map_err(|e| e.to_string())?;
        let config = supported.config();
        let channels = config.channels as usize;
        let buffer = Arc::new(Mutex::new(VecDeque::with_capacity(MOST)));
        let mut resampler = Resampler::new(config.sample_rate, RATE);
        let into = buffer.clone();
        let mut mono = Vec::new();
        let mut out = Vec::new();
        let mut take = move |samples: &mut dyn Iterator<Item = f32>| {
            mono.clear();
            let mut sum = 0.0;
            for (i, s) in samples.enumerate() {
                sum += s;
                if i % channels == channels - 1 {
                    mono.push(sum / channels as f32);
                    sum = 0.0;
                }
            }
            out.clear();
            resampler.process(&mono, &mut out);
            let mut buffer = into.lock().unwrap();
            buffer.extend(out.iter());
            let over = buffer.len().saturating_sub(MOST);
            buffer.drain(..over);
        };
        let error = |e| log_warn!("call: microphone: {e}");
        let stream = match supported.sample_format() {
            cpal::SampleFormat::F32 => {
                device.build_input_stream(config, move |data: &[f32], _: &_| take(&mut data.iter().copied()), error, None)
            }
            cpal::SampleFormat::I16 => device.build_input_stream(
                config,
                move |data: &[i16], _: &_| take(&mut data.iter().map(|&s| s as f32 / 32_768.0)),
                error,
                None,
            ),
            cpal::SampleFormat::I32 => device.build_input_stream(
                config,
                move |data: &[i32], _: &_| take(&mut data.iter().map(|&s| s as f32 / 2_147_483_648.0)),
                error,
                None,
            ),
            other => return Err(format!("input format {other} isn't supported")),
        }
        .map_err(|e| e.to_string())?;
        stream.play().map_err(|e| e.to_string())?;
        Ok(Microphone { _stream: stream, buffer })
    }
}

impl Source for Microphone {
    fn read(&mut self, frame: &mut [f32]) {
        let mut buffer = self.buffer.lock().unwrap();
        // Devices deliver in bursts of up to about 100 ms; more waiting than that means
        // this one runs ahead of the call: catch up.
        let over = buffer.len().saturating_sub(frame.len() * 5);
        buffer.drain(..over);
        let have = buffer.len().min(frame.len());
        let missing = frame.len() - have;
        frame[..missing].fill(0.0);
        for (s, b) in frame[missing..].iter_mut().zip(buffer.drain(..have)) {
            *s = b;
        }
    }
}

/// The default output device, fed 48 kHz mono resampled to its rate and copied
/// to all its channels.
pub struct Speakers {
    _stream: cpal::Stream,
    buffer: Arc<Mutex<Playout>>,
}

/// Received sound waiting for the speakers.
#[derive(Default)]
struct Playout {
    samples: VecDeque<f32>,
    playing: bool,
}

impl Speakers {
    pub fn open() -> Result<Self, String> {
        let device = cpal::default_host().default_output_device().ok_or("no output device")?;
        let supported = device.default_output_config().map_err(|e| e.to_string())?;
        let config = supported.config();
        let channels = config.channels as usize;
        let buffer = Arc::new(Mutex::new(Playout::default()));
        let mut resampler = Resampler::new(RATE, config.sample_rate);
        let from = buffer.clone();
        let mut pending: VecDeque<f32> = VecDeque::new();
        let mut chunk = Vec::new();
        let mut out = Vec::new();
        let mut next = move |frames: usize| -> Vec<f32> {
            while pending.len() < frames {
                chunk.clear();
                {
                    let mut playout = from.lock().unwrap();
                    if !playout.playing && playout.samples.len() >= PREBUFFER {
                        playout.playing = true;
                    }
                    if playout.playing {
                        let n = playout.samples.len().min(FRAME / 4);
                        chunk.extend(playout.samples.drain(..n));
                        if n == 0 {
                            playout.playing = false;
                        }
                    }
                }
                if chunk.is_empty() {
                    pending.extend(std::iter::repeat_n(0.0, frames - pending.len()));
                    break;
                }
                out.clear();
                resampler.process(&chunk, &mut out);
                pending.extend(out.iter());
            }
            pending.drain(..frames).collect()
        };
        let error = |e| log_warn!("call: speakers: {e}");
        let stream = match supported.sample_format() {
            cpal::SampleFormat::F32 => device.build_output_stream(
                config,
                move |data: &mut [f32], _: &_| {
                    let mono = next(data.len() / channels);
                    for (frame, s) in data.chunks_mut(channels).zip(mono) {
                        frame.fill(s);
                    }
                },
                error,
                None,
            ),
            cpal::SampleFormat::I16 => device.build_output_stream(
                config,
                move |data: &mut [i16], _: &_| {
                    let mono = next(data.len() / channels);
                    for (frame, s) in data.chunks_mut(channels).zip(mono) {
                        frame.fill((s.clamp(-1.0, 1.0) * 32_767.0) as i16);
                    }
                },
                error,
                None,
            ),
            other => return Err(format!("output format {other} isn't supported")),
        }
        .map_err(|e| e.to_string())?;
        stream.play().map_err(|e| e.to_string())?;
        Ok(Speakers { _stream: stream, buffer })
    }
}

impl Sink for Speakers {
    fn write(&mut self, samples: &[f32]) {
        let mut playout = self.buffer.lock().unwrap();
        playout.samples.extend(samples);
        let over = playout.samples.len().saturating_sub(MOST);
        playout.samples.drain(..over);
    }
}

/// Linear interpolation from one rate to another, over a stream of chunks. Enough
/// for speech; a windowed-sinc resampler would be the step up.
pub struct Resampler {
    /// Input samples per output sample.
    step: f64,
    /// Where the next output sample falls after `last`, in input samples.
    at: f64,
    last: f32,
}

impl Resampler {
    pub fn new(from: u32, to: u32) -> Self {
        Resampler { step: from as f64 / to as f64, at: 0.0, last: 0.0 }
    }

    pub fn process(&mut self, input: &[f32], out: &mut Vec<f32>) {
        if self.step == 1.0 {
            out.extend_from_slice(input);
            return;
        }
        for &s in input {
            while self.at < 1.0 {
                out.push(self.last + (s - self.last) * self.at as f32);
                self.at += self.step;
            }
            self.at -= 1.0;
            self.last = s;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resampling_keeps_time() {
        for (from, to) in [(44_100, 48_000), (48_000, 44_100), (16_000, 48_000), (48_000, 48_000)] {
            let mut resampler = Resampler::new(from, to);
            let mut out = Vec::new();
            for _ in 0..100 {
                resampler.process(&vec![0.25; from as usize / 100], &mut out);
            }
            // One second in is one second out, give or take a sample.
            assert!((out.len() as i64 - to as i64).abs() <= 1, "{from} -> {to}: {}", out.len());
            assert!(out[10..].iter().all(|&s| (s - 0.25).abs() < 1e-6));
        }
    }

    #[test]
    fn a_tone_is_a_tone() {
        let mut frame = [0.0; FRAME];
        Tone::new(1_000.0).read(&mut frame);
        // 1 kHz at 48 kHz: 48 samples a period, so 20 periods a frame.
        let crossings = frame.windows(2).filter(|w| w[0] < 0.0 && w[1] >= 0.0).count();
        assert!((19..=21).contains(&crossings), "{crossings}");
    }
}
