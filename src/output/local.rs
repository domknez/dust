//! Local sound card output via cpal, fed through a lock-free ring buffer.

use super::{RATE, Resampler, Sink};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

struct Shared {
    volume: AtomicU32,
    paused: AtomicBool,
    flush: AtomicBool,
}

pub struct LocalSink {
    _stream: cpal::Stream,
    producer: rtrb::Producer<f32>,
    shared: Arc<Shared>,
    resampler: Resampler,
    rate: u32,
    scratch: Vec<f32>,
    pending: Vec<f32>,
}

impl LocalSink {
    pub fn new(volume: f32) -> Result<Self, String> {
        let device = cpal::default_host().default_output_device().ok_or("No audio output device")?;
        let default = device.default_output_config().map_err(|e| e.to_string())?;
        // Prefer native 44.1 kHz to avoid resampling.
        let config = device
            .supported_output_configs()
            .map_err(|e| e.to_string())?
            .filter(|c| c.sample_format() == default.sample_format() && c.channels() >= 2)
            .find(|c| c.min_sample_rate().0 <= RATE && RATE <= c.max_sample_rate().0)
            .map(|c| c.with_sample_rate(cpal::SampleRate(RATE)))
            .unwrap_or(default);
        let rate = config.sample_rate().0;
        let channels = config.channels() as usize;
        // ~200 ms of stereo audio at device rate.
        let (producer, consumer) = rtrb::RingBuffer::new((rate as usize * 2 / 5) & !1);
        let shared = Arc::new(Shared {
            volume: AtomicU32::new(volume.to_bits()),
            paused: AtomicBool::new(false),
            flush: AtomicBool::new(false),
        });
        let stream = match config.sample_format() {
            SampleFormat::F32 => build::<f32>(&device, &config.into(), channels, consumer, shared.clone()),
            SampleFormat::I16 => build::<i16>(&device, &config.into(), channels, consumer, shared.clone()),
            SampleFormat::U16 => build::<u16>(&device, &config.into(), channels, consumer, shared.clone()),
            SampleFormat::I32 => build::<i32>(&device, &config.into(), channels, consumer, shared.clone()),
            f => return Err(format!("Unsupported sample format {f}")),
        }?;
        stream.play().map_err(|e| e.to_string())?;
        Ok(Self {
            _stream: stream,
            producer,
            shared,
            resampler: Resampler::new(RATE, rate),
            rate,
            scratch: Vec::new(),
            pending: Vec::new(),
        })
    }

    fn drain_pending(&mut self) {
        let n = self.pending.len().min(self.producer.slots()) & !1; // whole frames only
        if n == 0 {
            return;
        }
        if let Ok(chunk) = self.producer.write_chunk_uninit(n) {
            chunk.fill_from_iter(self.pending.drain(..n));
        }
    }
}

fn build<T: SizedSample + FromSample<f32>>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    channels: usize,
    mut consumer: rtrb::Consumer<f32>,
    shared: Arc<Shared>,
) -> Result<cpal::Stream, String> {
    device
        .build_output_stream(
            config,
            move |out: &mut [T], _| {
                if shared.flush.swap(false, Ordering::AcqRel) {
                    let n = consumer.slots();
                    if let Ok(c) = consumer.read_chunk(n) {
                        c.commit_all();
                    }
                }
                let silent = shared.paused.load(Ordering::Relaxed);
                let v = f32::from_bits(shared.volume.load(Ordering::Relaxed));
                let gain = v * v; // rough perceptual curve
                for frame in out.chunks_mut(channels) {
                    let (l, r) = if !silent && consumer.slots() >= 2 {
                        (consumer.pop().unwrap_or(0.0) * gain, consumer.pop().unwrap_or(0.0) * gain)
                    } else {
                        (0.0, 0.0)
                    };
                    for (i, s) in frame.iter_mut().enumerate() {
                        *s = T::from_sample(match i {
                            0 => l,
                            1 => r,
                            _ => 0.0,
                        });
                    }
                }
            },
            |e| eprintln!("audio stream error: {e}"),
            None,
        )
        .map_err(|e| e.to_string())
}

impl Sink for LocalSink {
    fn write(&mut self, samples: &[i16]) -> usize {
        self.drain_pending();
        if !self.pending.is_empty() {
            return 0;
        }
        self.scratch.clear();
        self.scratch.extend(samples.iter().map(|&s| s as f32 / 32768.0));
        if self.resampler.is_identity() {
            std::mem::swap(&mut self.pending, &mut self.scratch);
        } else {
            self.resampler.process(&self.scratch, &mut self.pending);
        }
        self.drain_pending();
        samples.len()
    }

    fn pending_frames(&self) -> usize {
        let queued = self.producer.buffer().capacity() - self.producer.slots() + self.pending.len();
        queued / 2 * RATE as usize / self.rate as usize
    }

    fn latency_frames(&self) -> usize {
        0
    }

    fn pause(&mut self) {
        self.shared.paused.store(true, Ordering::Relaxed);
    }

    fn resume(&mut self) {
        self.shared.paused.store(false, Ordering::Relaxed);
    }

    fn flush(&mut self) {
        self.pending.clear();
        self.resampler.reset();
        self.shared.flush.store(true, Ordering::Release);
    }

    fn set_volume(&mut self, volume: f32) {
        self.shared.volume.store(volume.to_bits(), Ordering::Relaxed);
    }
}
