//! People's, teams' and group chats' pictures: fetched and decoded off the UI
//! thread, uploaded as textures and kept; a picture that isn't there (no photo) is
//! remembered as missing, and its initials are drawn instead.

use crate::backend::Backend;
use crate::model::Picture;
use eframe::egui::{self, ColorImage, TextureHandle, TextureId, TextureOptions};
use std::collections::{HashMap, VecDeque};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};
use zune_jpeg::JpegDecoder;
use zune_jpeg::zune_core::colorspace::ColorSpace;
use zune_jpeg::zune_core::options::DecoderOptions;

const WORKERS: usize = 3;
/// The biggest picture decoded, per side and in all: what a message sends is its
/// own say, and a few bytes can claim a huge one.
const MAX_SIDE: usize = 8192;
const MAX_PIXELS: usize = 40_000_000;
/// Pictures in messages not drawn for this long are let go (fetched again if needed).
const UNUSED_FOR: Duration = Duration::from_secs(180);
/// Pending fetches beyond this are dropped (oldest first) and asked for again if needed.
const MAX_QUEUE: usize = 64;
/// An animated GIF's frames, at most: this many, and this many bytes in all (the
/// frames beyond are left out, and it loops over those it has).
const MAX_FRAMES: usize = 600;
const MAX_FRAME_BYTES: usize = 128 * 1024 * 1024;
/// A frame shown for less than this (or not said for how long) is shown this long,
/// as browsers do.
const MIN_DELAY: Duration = Duration::from_millis(20);
const DEFAULT_DELAY: Duration = Duration::from_millis(100);

/// A picture's frames and how long each is shown: one, unless it's an animated GIF.
pub type Frames = Vec<(ColorImage, Duration)>;

enum Slot {
    Loading,
    Missing,
    /// Its frames (one, but for an animation), when it was last drawn, and when it
    /// came in (its animation runs from then).
    Ready(Vec<(TextureHandle, Duration)>, Instant, Instant),
}

type Queue = Arc<(Mutex<VecDeque<Picture>>, Condvar)>;

pub struct Pictures {
    ctx: egui::Context,
    backend: Arc<dyn Backend>,
    slots: HashMap<String, Slot>,
    queue: Queue,
    done: Receiver<(String, Option<Frames>)>,
    /// For the workers, until they start (with the first picture asked for).
    done_tx: Option<Sender<(String, Option<Frames>)>>,
    swept: Instant,
}

impl Pictures {
    pub fn new(ctx: &egui::Context, backend: Arc<dyn Backend>) -> Pictures {
        let (done_tx, done) = mpsc::channel();
        Pictures {
            ctx: ctx.clone(),
            backend,
            slots: HashMap::new(),
            queue: Arc::new((Mutex::new(VecDeque::new()), Condvar::new())),
            done,
            done_tx: Some(done_tx),
            swept: Instant::now(),
        }
    }

    /// Take in what the workers finished; call once per frame.
    pub fn begin_frame(&mut self) {
        while let Ok((key, frames)) = self.done.try_recv() {
            let slot = match frames {
                Some(frames) if !frames.is_empty() => {
                    let textures = frames
                        .into_iter()
                        .enumerate()
                        .map(|(i, (image, delay))| {
                            let name = if i == 0 { key.clone() } else { format!("{key}#{i}") };
                            (self.ctx.load_texture(name, image, TextureOptions::LINEAR), delay)
                        })
                        .collect();
                    Slot::Ready(textures, Instant::now(), Instant::now())
                }
                _ => Slot::Missing,
            };
            self.slots.insert(key, slot);
        }
        // Pictures in messages scrolled past long ago: their memory back. (People's,
        // teams' and emoji are small and seen all the time.)
        if self.swept.elapsed() >= Duration::from_secs(30) {
            self.swept = Instant::now();
            self.slots
                .retain(|key, slot| !(key.starts_with("image:") && matches!(slot, Slot::Ready(_, used, _) if used.elapsed() > UNUSED_FOR)));
        }
    }

    /// The picture's texture once it's loaded (an animation's frame for now, the
    /// next asked to be drawn when it's due); until then (or without one) None, and
    /// it's asked for.
    pub fn get(&mut self, picture: &Picture) -> Option<TextureId> {
        let key = picture.key();
        match self.slots.get_mut(&key) {
            Some(Slot::Ready(frames, used, since)) => {
                *used = Instant::now();
                let (frame, left) = frame_at(frames.iter().map(|(_, delay)| *delay), since.elapsed());
                if let Some(left) = left {
                    self.ctx.request_repaint_after(left);
                }
                frames.get(frame).map(|(tex, _)| tex.id())
            }
            Some(_) => None,
            None => {
                self.start_workers();
                self.slots.insert(key, Slot::Loading);
                let (lock, cvar) = &*self.queue;
                let mut queue = lock.lock().unwrap_or_else(|e| e.into_inner());
                queue.push_front(picture.clone());
                while queue.len() > MAX_QUEUE {
                    if let Some(old) = queue.pop_back() {
                        self.slots.remove(&old.key());
                    }
                }
                cvar.notify_one();
                None
            }
        }
    }

    /// Whether the picture is on its way (not loaded, and not known to be missing).
    pub fn loading(&self, picture: &Picture) -> bool {
        matches!(self.slots.get(&picture.key()), Some(Slot::Loading))
    }

    fn start_workers(&mut self) {
        let Some(tx) = self.done_tx.take() else { return };
        for i in 0..WORKERS {
            let (queue, tx, backend, ctx) = (self.queue.clone(), tx.clone(), self.backend.clone(), self.ctx.clone());
            std::thread::Builder::new()
                .name(format!("pictures-{i}"))
                .spawn(move || worker(&queue, &tx, backend.as_ref(), &ctx))
                .expect("spawn picture worker");
        }
    }
}

fn worker(queue: &Queue, tx: &Sender<(String, Option<Frames>)>, backend: &dyn Backend, ctx: &egui::Context) {
    loop {
        let picture = {
            let (lock, cvar) = &**queue;
            let mut q = lock.lock().unwrap_or_else(|e| e.into_inner());
            loop {
                if let Some(p) = q.pop_front() {
                    break p;
                }
                q = cvar.wait(q).unwrap_or_else(|e| e.into_inner());
            }
        };
        let image = match backend.picture(&picture) {
            Ok(bytes) => decode_frames(&bytes),
            Err(e) => {
                // Without a picture's query (signed addresses carry their key there).
                let key = picture.key();
                log_debug!("no picture for {}: {e}", key.split('?').next().unwrap_or(&key));
                None
            }
        };
        if tx.send((picture.key(), image)).is_err() {
            return;
        }
        ctx.request_repaint();
    }
}

/// A picture's width and height, and its MIME type (PNG, GIF or JPEG), if it's one.
/// Only the header is read (it's called on the UI thread, for attachments).
pub fn image_info(bytes: &[u8]) -> Option<(u32, u32, &'static str)> {
    let cursor = || std::io::Cursor::new(bytes);
    let (w, h, mime) = if bytes.starts_with(b"\x89PNG") {
        let reader = png::Decoder::new(cursor()).read_info().ok()?;
        (reader.info().width as usize, reader.info().height as usize, "image/png")
    } else if bytes.starts_with(b"GIF8") {
        let decoder = gif::DecodeOptions::new().read_info(cursor()).ok()?;
        (usize::from(decoder.width()), usize::from(decoder.height()), "image/gif")
    } else {
        let mut decoder = JpegDecoder::new(cursor());
        decoder.decode_headers().ok()?;
        let (w, h) = decoder.dimensions()?;
        (w, h, "image/jpeg")
    };
    (w > 0 && h > 0 && fits(w, h)).then_some((w as u32, h as u32, mime))
}

/// JPEG, PNG or GIF, by their signature (an animation's first frame).
pub fn decode(bytes: &[u8]) -> Option<ColorImage> {
    if bytes.starts_with(b"GIF8") { decode_gif(bytes, 1).and_then(|f| f.into_iter().next()).map(|(image, _)| image) } else { still(bytes) }
}

/// A picture's frames: an animated GIF's all (as many as are kept), else its one.
pub fn decode_frames(bytes: &[u8]) -> Option<Frames> {
    if bytes.starts_with(b"GIF8") { decode_gif(bytes, MAX_FRAMES) } else { still(bytes).map(|image| vec![(image, Duration::ZERO)]) }
}

fn still(bytes: &[u8]) -> Option<ColorImage> {
    if bytes.starts_with(b"\x89PNG") { decode_png(bytes) } else { decode_jpeg(bytes) }
}

/// Which frame shows `elapsed` into an animation looping over frames shown this
/// long each, and how long until the next; a still picture (or one that never
/// moves) has no next.
fn frame_at(delays: impl Iterator<Item = Duration> + Clone, elapsed: Duration) -> (usize, Option<Duration>) {
    let total: Duration = delays.clone().sum();
    if delays.clone().count() < 2 || total.is_zero() {
        return (0, None);
    }
    let mut at = Duration::from_nanos((elapsed.as_nanos() % total.as_nanos()) as u64);
    for (i, delay) in delays.enumerate() {
        if at < delay {
            return (i, Some(delay - at));
        }
        at -= delay;
    }
    (0, None)
}

/// A GIF's frames (up to `most`), each drawn on its canvas over what the ones
/// before left there.
fn decode_gif(bytes: &[u8], most: usize) -> Option<Frames> {
    let mut options = gif::DecodeOptions::new();
    options.set_color_output(gif::ColorOutput::RGBA);
    let mut decoder = options.read_info(std::io::Cursor::new(bytes)).ok()?;
    let (width, height) = (usize::from(decoder.width()), usize::from(decoder.height()));
    if !fits(width, height) {
        return None;
    }
    let mut canvas = vec![0u8; width * height * 4];
    let mut frames = Frames::new();
    while frames.len() < most && (frames.len() + 1) * canvas.len() <= MAX_FRAME_BYTES.max(canvas.len()) {
        let frame = match decoder.read_next_frame() {
            Ok(Some(frame)) => frame,
            // A broken frame after good ones: those are kept.
            Ok(None) | Err(_) => break,
        };
        let (left, top, w) = (usize::from(frame.left), usize::from(frame.top), usize::from(frame.width));
        let before = (frame.dispose == gif::DisposalMethod::Previous).then(|| canvas.clone());
        let mut drawn = Vec::new();
        if w > 0 {
            for (y, row) in frame.buffer.chunks_exact(w * 4).enumerate() {
                let y = top + y;
                if y >= height || left >= width {
                    continue;
                }
                let n = w.min(width - left);
                let at = (y * width + left) * 4;
                // Transparent pixels leave what's under them.
                for (dst, src) in canvas[at..at + n * 4].as_chunks_mut::<4>().0.iter_mut().zip(row.as_chunks::<4>().0) {
                    if src[3] != 0 {
                        dst.copy_from_slice(src);
                    }
                }
                drawn.push((at, n * 4));
            }
        }
        let delay = Some(Duration::from_millis(u64::from(frame.delay) * 10)).filter(|d| *d >= MIN_DELAY).unwrap_or(DEFAULT_DELAY);
        frames.push((image_from(&canvas, [width, height])?, delay));
        match (frame.dispose, before) {
            (gif::DisposalMethod::Background, _) => {
                for (at, n) in drawn {
                    canvas[at..at + n].fill(0);
                }
            }
            (gif::DisposalMethod::Previous, Some(before)) => canvas = before,
            _ => {}
        }
    }
    (!frames.is_empty()).then_some(frames)
}

fn decode_jpeg(bytes: &[u8]) -> Option<ColorImage> {
    let options = DecoderOptions::default().jpeg_set_out_colorspace(ColorSpace::RGBA).set_max_width(MAX_SIDE).set_max_height(MAX_SIDE);
    let mut decoder = JpegDecoder::new_with_options(std::io::Cursor::new(bytes), options);
    decoder.decode_headers().ok()?;
    let (w, h) = decoder.dimensions()?;
    if !fits(w, h) {
        return None;
    }
    let pixels = decoder.decode().ok()?;
    let info = decoder.info()?;
    image_from(&pixels, [info.width as usize, info.height as usize])
}

fn decode_png(bytes: &[u8]) -> Option<ColorImage> {
    let mut decoder = png::Decoder::new_with_limits(std::io::Cursor::new(bytes), png::Limits { bytes: MAX_PIXELS * 4 });
    // 8-bit samples, palettes expanded.
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().ok()?;
    if !fits(reader.info().width as usize, reader.info().height as usize) {
        return None;
    }
    let mut pixels = vec![0; reader.output_buffer_size()?];
    let frame = reader.next_frame(&mut pixels).ok()?;
    pixels.truncate(frame.buffer_size());
    let size = [frame.width as usize, frame.height as usize];
    match frame.color_type {
        png::ColorType::GrayscaleAlpha => {
            let rgba: Vec<u8> = pixels.as_chunks::<2>().0.iter().flat_map(|p| [p[0], p[0], p[0], p[1]]).collect();
            image_from(&rgba, size)
        }
        _ => image_from(&pixels, size),
    }
}

/// Whether a picture of this size is decoded at all.
fn fits(width: usize, height: usize) -> bool {
    width <= MAX_SIDE && height <= MAX_SIDE && width * height <= MAX_PIXELS
}

/// An image from 1 (grey), 3 (RGB) or 4 (RGBA) bytes per pixel.
fn image_from(pixels: &[u8], size: [usize; 2]) -> Option<ColorImage> {
    let area = size[0] * size[1];
    if area == 0 || !pixels.len().is_multiple_of(area) {
        return None;
    }
    match pixels.len() / area {
        1 => Some(ColorImage::from_gray(size, pixels)),
        3 => Some(ColorImage::from_rgb(size, pixels)),
        4 => Some(ColorImage::from_rgba_unmultiplied(size, pixels)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_png() {
        let mut out = Vec::new();
        let mut encoder = png::Encoder::new(&mut out, 2, 1);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.write_header().unwrap().write_image_data(&[255, 0, 0, 0, 255, 0]).unwrap();
        let image = decode(&out).unwrap();
        assert_eq!(image.size, [2, 1]);
        assert_eq!(image.pixels[1], egui::Color32::from_rgb(0, 255, 0));
    }

    #[test]
    fn decodes_gif() {
        let mut out = Vec::new();
        {
            let mut encoder = gif::Encoder::new(&mut out, 2, 1, &[255, 0, 0, 0, 255, 0]).unwrap();
            encoder
                .write_frame(&gif::Frame { width: 2, height: 1, buffer: std::borrow::Cow::Borrowed(&[0, 1]), ..Default::default() })
                .unwrap();
        }
        let image = decode(&out).unwrap();
        assert_eq!(image.size, [2, 1]);
        assert_eq!(image.pixels[1], egui::Color32::from_rgb(0, 255, 0));
    }

    #[test]
    fn decodes_every_frame_of_an_animation() {
        // Two frames: red all over, then green over the right pixel only (the left
        // stays red), shown 50 ms and (said as nothing) the default.
        let mut out = Vec::new();
        {
            let mut encoder = gif::Encoder::new(&mut out, 2, 1, &[255, 0, 0, 0, 255, 0]).unwrap();
            let first = gif::Frame { width: 2, height: 1, delay: 5, buffer: std::borrow::Cow::Borrowed(&[0, 0]), ..Default::default() };
            let second = gif::Frame { left: 1, width: 1, height: 1, buffer: std::borrow::Cow::Borrowed(&[1]), ..Default::default() };
            encoder.write_frame(&first).unwrap();
            encoder.write_frame(&second).unwrap();
        }
        let frames = decode_frames(&out).unwrap();
        assert_eq!(frames.len(), 2);
        assert_eq!(frames[0].1, Duration::from_millis(50));
        assert_eq!(frames[1].1, DEFAULT_DELAY);
        assert_eq!(frames[1].0.pixels[0], egui::Color32::from_rgb(255, 0, 0));
        assert_eq!(frames[1].0.pixels[1], egui::Color32::from_rgb(0, 255, 0));
        // The picture alone is the first frame.
        assert_eq!(decode(&out).unwrap().pixels[1], egui::Color32::from_rgb(255, 0, 0));
    }

    #[test]
    fn picks_the_frame_for_the_time() {
        let ms = Duration::from_millis;
        let delays = [ms(50), ms(100)];
        assert_eq!(frame_at(delays.iter().copied(), ms(0)), (0, Some(ms(50))));
        assert_eq!(frame_at(delays.iter().copied(), ms(60)), (1, Some(ms(90))));
        assert_eq!(frame_at(delays.iter().copied(), ms(160)), (0, Some(ms(40))));
        assert_eq!(frame_at([Duration::ZERO].iter().copied(), ms(500)), (0, None));
    }

    #[test]
    fn rejects_garbage() {
        assert!(decode(b"not an image").is_none());
    }

    #[test]
    fn refuses_pictures_claiming_to_be_huge() {
        // A GIF whose screen is 65535×65535, with one 1×1 frame: a few dozen bytes.
        let mut out = Vec::new();
        {
            let mut encoder = gif::Encoder::new(&mut out, 65535, 65535, &[255, 0, 0, 0, 255, 0]).unwrap();
            encoder
                .write_frame(&gif::Frame { width: 1, height: 1, buffer: std::borrow::Cow::Borrowed(&[0]), ..Default::default() })
                .unwrap();
        }
        assert!(out.len() < 100);
        assert!(decode(&out).is_none());
    }

    #[test]
    fn survives_truncated_pictures() {
        let mut png = Vec::new();
        let mut encoder = png::Encoder::new(&mut png, 64, 64);
        encoder.set_color(png::ColorType::Rgb);
        encoder.write_header().unwrap().write_image_data(&[7; 64 * 64 * 3]).unwrap();
        for cut in [8, 30, png.len() / 2, png.len() - 1] {
            let _ = decode(&png[..cut]);
        }
        let _ = decode(b"\xff\xd8\xff\xe0\x00\x10JFIF");
        let _ = decode(b"GIF89a\x01\x00\x01\x00");
    }
}
