//! Cover art cache: fetched and decoded off the UI thread, uploaded as textures,
//! evicted least-recently-used once a byte budget is exceeded. Pictures are
//! asked for as [`Art`]; the source in use says where they are.

use crate::model::{Art, ArtResolver};
use eframe::egui::{self, ColorImage, TextureHandle, TextureId, TextureOptions};
use std::collections::{HashMap, VecDeque};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};
use zune_jpeg::JpegDecoder;
use zune_jpeg::zune_core::colorspace::ColorSpace;
use zune_jpeg::zune_core::options::DecoderOptions;

/// Decoded RGBA kept on the GPU; ~16 MB is thousands of list thumbnails.
const BUDGET_BYTES: usize = 16 << 20;
/// Pending fetches beyond this are dropped (oldest first) and re-requested if needed.
const MAX_QUEUE: usize = 48;
const WORKERS: usize = 3;
/// Once over budget, covers go until this much is left.
const EVICT_TO: usize = BUDGET_BYTES * 3 / 4;
/// A cover drawn within this many frames stays (it's on screen).
const KEEP_FRAMES: u64 = 2;
/// How long one cover may take to download.
const FETCH_TIMEOUT: Duration = Duration::from_secs(15);
/// A cover that couldn't be downloaded is asked for again after this long (the
/// network may be back); one that downloaded but won't decode, never.
const RETRY_AFTER: Duration = Duration::from_secs(60);

enum Slot {
    Loading,
    /// Not to be asked for again (until `retry_at`, if any).
    Failed {
        retry_at: Option<Instant>,
    },
    Ready {
        tex: TextureHandle,
        bytes: usize,
        last_used: u64,
    },
}

enum Done {
    Image(String, ColorImage),
    /// Not downloaded (the network), so worth another try later.
    Unreached(String),
    /// Downloaded, but not a picture dust can show.
    Unreadable(String),
}

type Queue = Arc<(Mutex<VecDeque<String>>, Condvar)>;

pub struct Covers {
    ctx: egui::Context,
    slots: HashMap<String, Slot>,
    queue: Queue,
    done: Receiver<Done>,
    /// For the workers, until they start (with the first cover asked for).
    done_tx: Option<Sender<Done>>,
    frame: u64,
    bytes: usize,
    /// Turns pictures into URLs: the source in use.
    resolver: Option<Arc<dyn ArtResolver>>,
}

impl Covers {
    pub fn new(ctx: &egui::Context) -> Self {
        let queue: Queue = Arc::new((Mutex::new(VecDeque::new()), Condvar::new()));
        let (done_tx, done) = mpsc::channel();
        Self { ctx: ctx.clone(), slots: HashMap::new(), queue, done, done_tx: Some(done_tx), frame: 0, bytes: 0, resolver: None }
    }

    /// Where pictures come from from now on (the source switched).
    pub fn set_resolver(&mut self, resolver: Arc<dyn ArtResolver>) {
        self.resolver = Some(resolver);
    }

    /// The URL of `art`, about `size` px square; None before a source is in use.
    pub fn url(&self, art: &Art, size: u32) -> Option<String> {
        self.resolver.as_ref().map(|r| r.art_url(art, size))
    }

    /// Texture for `art` at about `size` px if loaded; otherwise starts loading it
    /// and returns None.
    pub fn art(&mut self, art: &Art, size: u32) -> Option<TextureId> {
        let url = self.url(art, size)?;
        self.get(&url)
    }

    /// Start the fetching threads, once.
    fn start_workers(&mut self) {
        let Some(tx) = self.done_tx.take() else { return };
        let agent = crate::http::agent(crate::http::USER_AGENT, Some(FETCH_TIMEOUT));
        for i in 0..WORKERS {
            let (queue, tx, agent, ctx) = (self.queue.clone(), tx.clone(), agent.clone(), self.ctx.clone());
            if let Err(e) = std::thread::Builder::new().name(format!("covers-{i}")).spawn(move || worker(queue, tx, agent, ctx)) {
                log_warn!("cover worker {i}: {e}");
            }
        }
    }

    /// Call once per frame before drawing.
    pub fn begin_frame(&mut self) {
        self.frame += 1;
        while let Ok(d) = self.done.try_recv() {
            match d {
                Done::Image(url, img) => {
                    let bytes = img.pixels.len() * 4;
                    let tex = self.ctx.load_texture(&url, img, TextureOptions::LINEAR);
                    self.bytes += bytes;
                    self.slots.insert(url, Slot::Ready { tex, bytes, last_used: self.frame });
                }
                Done::Unreached(url) => {
                    self.slots.insert(url, Slot::Failed { retry_at: Some(Instant::now() + RETRY_AFTER) });
                }
                Done::Unreadable(url) => {
                    self.slots.insert(url, Slot::Failed { retry_at: None });
                }
            }
        }
        if self.bytes > BUDGET_BYTES {
            self.evict();
        }
    }

    fn evict(&mut self) {
        let mut ready: Vec<(u64, String, usize)> = self
            .slots
            .iter()
            .filter_map(|(k, s)| match s {
                Slot::Ready { last_used, bytes, .. } if *last_used + KEEP_FRAMES < self.frame => Some((*last_used, k.clone(), *bytes)),
                _ => None,
            })
            .collect();
        ready.sort_unstable();
        for (_, url, bytes) in ready {
            if self.bytes <= EVICT_TO {
                break;
            }
            self.slots.remove(&url);
            self.bytes -= bytes;
        }
    }

    /// Texture for `url` if loaded; otherwise starts loading it and returns None.
    fn get(&mut self, url: &str) -> Option<TextureId> {
        match self.slots.get_mut(url) {
            Some(Slot::Ready { tex, last_used, .. }) => {
                *last_used = self.frame;
                Some(tex.id())
            }
            Some(Slot::Failed { retry_at: Some(at) }) if Instant::now() >= *at => {
                self.slots.remove(url);
                self.get(url)
            }
            Some(_) => None,
            None => {
                self.start_workers();
                self.slots.insert(url.to_string(), Slot::Loading);
                let (lock, cvar) = &*self.queue;
                let mut q = lock.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
                q.push_front(url.to_string());
                // Drop the stalest requests; they are re-requested if still visible.
                while q.len() > MAX_QUEUE {
                    if let Some(old) = q.pop_back() {
                        self.slots.remove(&old);
                    }
                }
                cvar.notify_one();
                None
            }
        }
    }
}

fn worker(queue: Queue, tx: Sender<Done>, agent: ureq::Agent, ctx: egui::Context) {
    loop {
        let url = {
            let (lock, cvar) = &*queue;
            let mut q = lock.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            loop {
                if let Some(u) = q.pop_front() {
                    break u;
                }
                q = cvar.wait(q).unwrap_or_else(std::sync::PoisonError::into_inner);
            }
        };
        let result = match fetch(&agent, &url) {
            Fetched::Image(img) => Done::Image(url, img),
            Fetched::Unreached => Done::Unreached(url),
            Fetched::Unreadable => Done::Unreadable(url),
        };
        if tx.send(result).is_err() {
            return;
        }
        ctx.request_repaint();
    }
}

enum Fetched {
    Image(ColorImage),
    Unreached,
    Unreadable,
}

fn fetch(agent: &ureq::Agent, url: &str) -> Fetched {
    if url.is_empty() {
        return Fetched::Unreadable;
    }
    let bytes = match agent.get(url).call().and_then(|r| r.into_body().read_to_vec()) {
        Ok(bytes) => bytes,
        // Gone for good: no use asking again.
        Err(ureq::Error::StatusCode(404 | 410)) => return Fetched::Unreadable,
        Err(_) => return Fetched::Unreached,
    };
    decode(&bytes).map_or(Fetched::Unreadable, Fetched::Image)
}

/// JPEG (Deezer, most servers), PNG or WebP (music servers), by their signature.
fn decode(bytes: &[u8]) -> Option<ColorImage> {
    if bytes.starts_with(b"\x89PNG") {
        return decode_png(bytes);
    }
    if bytes.len() > 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        return decode_webp(bytes);
    }
    decode_jpeg(bytes)
}

fn decode_jpeg(bytes: &[u8]) -> Option<ColorImage> {
    let mut dec =
        JpegDecoder::new_with_options(std::io::Cursor::new(bytes), DecoderOptions::default().jpeg_set_out_colorspace(ColorSpace::RGBA));
    let pixels = dec.decode().ok()?;
    let info = dec.info()?;
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
        // Grey with alpha: drop the alpha, artwork is opaque.
        png::ColorType::GrayscaleAlpha => image_from(&pixels.chunks_exact(2).map(|p| p[0]).collect::<Vec<_>>(), size),
        _ => image_from(&pixels, size),
    }
}

fn decode_webp(bytes: &[u8]) -> Option<ColorImage> {
    let mut decoder = image_webp::WebPDecoder::new(std::io::Cursor::new(bytes)).ok()?;
    let (width, height) = decoder.dimensions();
    let mut pixels = vec![0; decoder.output_buffer_size()?];
    decoder.read_image(&mut pixels).ok()?;
    image_from(&pixels, [width as usize, height as usize])
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

    /// A cache that fetches nothing (no workers), with `slots`.
    fn cache(ctx: &egui::Context) -> Covers {
        Covers { done_tx: None, ..Covers::new(ctx) }
    }

    #[test]
    fn a_cover_not_reached_is_asked_for_again_later_and_a_broken_one_never() {
        let mut c = cache(&egui::Context::default());
        let past = Instant::now() - Duration::from_secs(1);
        c.slots.insert("later".into(), Slot::Failed { retry_at: Some(Instant::now() + RETRY_AFTER) });
        c.slots.insert("again".into(), Slot::Failed { retry_at: Some(past) });
        c.slots.insert("broken".into(), Slot::Failed { retry_at: None });
        for url in ["later", "again", "broken"] {
            assert!(c.get(url).is_none());
        }
        assert!(matches!(c.slots["again"], Slot::Loading), "its time came: asked for");
        assert!(matches!(c.slots["later"], Slot::Failed { .. }) && matches!(c.slots["broken"], Slot::Failed { .. }));
    }

    #[test]
    fn the_covers_used_longest_ago_go_first() {
        let ctx = egui::Context::default();
        let mut c = cache(&ctx);
        let third = BUDGET_BYTES / 3 + 1;
        for (i, url) in ["old", "middle", "new"].into_iter().enumerate() {
            let tex = ctx.load_texture(url, ColorImage::new([1, 1], vec![egui::Color32::BLACK]), TextureOptions::LINEAR);
            c.slots.insert(url.into(), Slot::Ready { tex, bytes: third, last_used: i as u64 });
        }
        c.bytes = 3 * third;
        c.frame = 10;
        c.evict();
        assert!(c.bytes <= EVICT_TO);
        assert!(!c.slots.contains_key("old") && c.slots.contains_key("new"));
    }

    /// A 2×1 RGB PNG: one red, one green pixel.
    fn png_bytes() -> Vec<u8> {
        let mut out = Vec::new();
        let mut encoder = png::Encoder::new(&mut out, 2, 1);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.write_header().unwrap().write_image_data(&[255, 0, 0, 0, 255, 0]).unwrap();
        out
    }

    #[test]
    fn decodes_png() {
        let image = decode(&png_bytes()).unwrap();
        assert_eq!(image.size, [2, 1]);
        assert_eq!(image.pixels[0], egui::Color32::from_rgb(255, 0, 0));
        assert_eq!(image.pixels[1], egui::Color32::from_rgb(0, 255, 0));
    }

    #[test]
    fn rejects_garbage() {
        assert!(decode(b"not an image").is_none());
        assert!(decode(b"RIFF\0\0\0\0WEBPVP8 ").is_none());
    }
}
