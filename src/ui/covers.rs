//! Cover art cache: fetched and decoded off the UI thread, uploaded as textures,
//! evicted least-recently-used once a byte budget is exceeded.

use eframe::egui::{self, ColorImage, TextureHandle, TextureId, TextureOptions};
use std::collections::{HashMap, VecDeque};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Condvar, Mutex};
use zune_jpeg::JpegDecoder;
use zune_jpeg::zune_core::colorspace::ColorSpace;
use zune_jpeg::zune_core::options::DecoderOptions;

/// Decoded RGBA kept on the GPU; ~16 MB is thousands of list thumbnails.
const BUDGET_BYTES: usize = 16 << 20;
/// Pending fetches beyond this are dropped (oldest first) and re-requested if needed.
const MAX_QUEUE: usize = 48;
const WORKERS: usize = 3;

enum Slot {
    Loading,
    Failed,
    Ready { tex: TextureHandle, bytes: usize, last_used: u64 },
}

enum Done {
    Image(String, ColorImage),
    Failed(String),
}

type Queue = Arc<(Mutex<VecDeque<String>>, Condvar)>;

pub struct Covers {
    ctx: egui::Context,
    slots: HashMap<String, Slot>,
    queue: Queue,
    done: Receiver<Done>,
    frame: u64,
    bytes: usize,
}

impl Covers {
    pub fn new(ctx: &egui::Context) -> Self {
        let queue: Queue = Arc::new((Mutex::new(VecDeque::new()), Condvar::new()));
        let (tx, done) = mpsc::channel();
        let agent = ureq::AgentBuilder::new().timeout(std::time::Duration::from_secs(15)).build();
        for i in 0..WORKERS {
            let (queue, tx, agent, ctx) = (queue.clone(), tx.clone(), agent.clone(), ctx.clone());
            std::thread::Builder::new()
                .name(format!("covers-{i}"))
                .spawn(move || worker(queue, tx, agent, ctx))
                .expect("spawn cover worker");
        }
        Self { ctx: ctx.clone(), slots: HashMap::new(), queue, done, frame: 0, bytes: 0 }
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
                Done::Failed(url) => {
                    self.slots.insert(url, Slot::Failed);
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
                Slot::Ready { last_used, bytes, .. } if *last_used + 2 < self.frame => Some((*last_used, k.clone(), *bytes)),
                _ => None,
            })
            .collect();
        ready.sort_unstable();
        for (_, url, bytes) in ready {
            if self.bytes <= BUDGET_BYTES * 3 / 4 {
                break;
            }
            self.slots.remove(&url);
            self.bytes -= bytes;
        }
    }

    /// Texture for `url` if loaded; otherwise starts loading it and returns None.
    pub fn get(&mut self, url: &str) -> Option<TextureId> {
        match self.slots.get_mut(url) {
            Some(Slot::Ready { tex, last_used, .. }) => {
                *last_used = self.frame;
                Some(tex.id())
            }
            Some(_) => None,
            None => {
                self.slots.insert(url.to_string(), Slot::Loading);
                let (lock, cvar) = &*self.queue;
                let mut q = lock.lock().unwrap();
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
            let mut q = lock.lock().unwrap();
            loop {
                if let Some(u) = q.pop_front() {
                    break u;
                }
                q = cvar.wait(q).unwrap();
            }
        };
        let result = fetch(&agent, &url).map(|img| Done::Image(url.clone(), img)).unwrap_or(Done::Failed(url));
        if tx.send(result).is_err() {
            return;
        }
        ctx.request_repaint();
    }
}

fn fetch(agent: &ureq::Agent, url: &str) -> Option<ColorImage> {
    let mut bytes = Vec::new();
    std::io::Read::read_to_end(&mut agent.get(url).call().ok()?.into_reader(), &mut bytes).ok()?;
    let mut dec = JpegDecoder::new_with_options(&bytes[..], DecoderOptions::default().jpeg_set_out_colorspace(ColorSpace::RGBA));
    let pixels = dec.decode().ok()?;
    let info = dec.info()?;
    Some(ColorImage::from_rgba_unmultiplied([info.width as usize, info.height as usize], &pixels))
}
