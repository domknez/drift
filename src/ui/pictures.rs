//! People's, teams' and group chats' pictures: fetched and decoded off the UI
//! thread, uploaded as textures and kept; a picture that isn't there (no photo) is
//! remembered as missing, and its initials are drawn instead.

use crate::backend::Backend;
use crate::model::Picture;
use eframe::egui::{self, ColorImage, TextureHandle, TextureId, TextureOptions};
use std::collections::{HashMap, VecDeque};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Condvar, Mutex};
use zune_jpeg::JpegDecoder;
use zune_jpeg::zune_core::colorspace::ColorSpace;
use zune_jpeg::zune_core::options::DecoderOptions;

const WORKERS: usize = 3;
/// Pending fetches beyond this are dropped (oldest first) and asked for again if needed.
const MAX_QUEUE: usize = 64;

enum Slot {
    Loading,
    Missing,
    Ready(TextureHandle),
}

type Queue = Arc<(Mutex<VecDeque<Picture>>, Condvar)>;

pub struct Pictures {
    ctx: egui::Context,
    backend: Arc<dyn Backend>,
    slots: HashMap<String, Slot>,
    queue: Queue,
    done: Receiver<(String, Option<ColorImage>)>,
    /// For the workers, until they start (with the first picture asked for).
    done_tx: Option<Sender<(String, Option<ColorImage>)>>,
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
        }
    }

    /// Take in what the workers finished; call once per frame.
    pub fn begin_frame(&mut self) {
        while let Ok((key, image)) = self.done.try_recv() {
            let slot = match image {
                Some(image) => Slot::Ready(self.ctx.load_texture(&key, image, TextureOptions::LINEAR)),
                None => Slot::Missing,
            };
            self.slots.insert(key, slot);
        }
    }

    /// The picture's texture once it's loaded; until then (or without one) None,
    /// and it's asked for.
    pub fn get(&mut self, picture: &Picture) -> Option<TextureId> {
        let key = picture.key();
        match self.slots.get(&key) {
            Some(Slot::Ready(tex)) => Some(tex.id()),
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

fn worker(queue: &Queue, tx: &Sender<(String, Option<ColorImage>)>, backend: &dyn Backend, ctx: &egui::Context) {
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
            Ok(bytes) => decode(&bytes),
            Err(e) => {
                log_debug!("no picture for {}: {e}", picture.key());
                None
            }
        };
        if tx.send((picture.key(), image)).is_err() {
            return;
        }
        ctx.request_repaint();
    }
}

/// JPEG or PNG, by their signature.
fn decode(bytes: &[u8]) -> Option<ColorImage> {
    if bytes.starts_with(b"\x89PNG") { decode_png(bytes) } else { decode_jpeg(bytes) }
}

fn decode_jpeg(bytes: &[u8]) -> Option<ColorImage> {
    let options = DecoderOptions::default().jpeg_set_out_colorspace(ColorSpace::RGBA);
    let mut decoder = JpegDecoder::new_with_options(std::io::Cursor::new(bytes), options);
    let pixels = decoder.decode().ok()?;
    let info = decoder.info()?;
    image_from(&pixels, [info.width as usize, info.height as usize])
}

fn decode_png(bytes: &[u8]) -> Option<ColorImage> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    // 8-bit samples, palettes expanded.
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().ok()?;
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
    fn rejects_garbage() {
        assert!(decode(b"not an image").is_none());
    }
}
