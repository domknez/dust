//! Playback engine. Runs on its own thread, owns the queue so playback continues
//! while the UI sleeps, and streams decrypted audio straight into the active sink.

use crate::deezer::{Deezer, Format, Quality, Track};
use crate::output::airplay::{AirPlaySink, Device};
use crate::output::local::LocalSink;
use crate::output::{RATE, Sink};
use eframe::egui;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender, TryRecvError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::{CODEC_TYPE_NULL, Decoder, DecoderOptions};
use symphonia::core::errors::Error as SymError;
use symphonia::core::formats::{FormatOptions, FormatReader};
use symphonia::core::io::{MediaSourceStream, ReadOnlySource};
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;
use symphonia::core::units::TimeBase;

#[derive(Clone, Debug, PartialEq)]
pub enum Output {
    Local,
    AirPlay(Device),
}

impl Output {
    pub fn name(&self) -> &str {
        match self {
            Output::Local => "This computer",
            Output::AirPlay(d) => &d.name,
        }
    }
}

pub enum Cmd {
    Client(Deezer),
    Play(Vec<Track>, usize),
    Toggle,
    Next,
    Prev,
    Seek(f64),
    Volume(f32),
    VolumeStep(f32),
    Resume,
    Pause,
    Output(Output),
    Quality(Quality),
}

#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum State {
    #[default]
    Stopped,
    Loading,
    Playing,
    Paused,
}

#[derive(Clone, Default)]
pub struct Status {
    pub state: State,
    pub track: Option<Track>,
    pub position: f64,
    pub volume: f32,
    pub output: String,
    pub error: Option<String>,
}

#[derive(Clone)]
pub struct PlayerHandle {
    tx: Sender<Cmd>,
    status: Arc<Mutex<Status>>,
}

impl PlayerHandle {
    pub fn spawn(ctx: egui::Context, volume: f32) -> Self {
        let (tx, rx) = mpsc::channel();
        let status = Arc::new(Mutex::new(Status { output: Output::Local.name().into(), volume, ..Default::default() }));
        let shared = status.clone();
        let make = move || Player {
            rx,
            status: shared,
            ctx,
            client: None,
            quality: Quality::Mp3_320,
            queue: Vec::new(),
            index: 0,
            sink: None,
            output: Output::Local,
            volume,
            stream: None,
            playing: false,
            pending: Vec::new(),
            pending_end: 0.0,
            resume_seek: false,
            last_publish: Instant::now(),
            alive: true,
        };
        std::thread::Builder::new().name("player".into()).spawn(move || make().run()).expect("spawn player");
        Self { tx, status }
    }

    pub fn send(&self, cmd: Cmd) {
        let _ = self.tx.send(cmd);
    }

    pub fn status(&self) -> Status {
        self.status.lock().unwrap().clone()
    }
}

struct Stream {
    url: String,
    format_kind: Format,
    reader: Box<dyn FormatReader>,
    decoder: Box<dyn Decoder>,
    track_id: u32,
    time_base: TimeBase,
    /// Track time (s) at which this byte stream starts (non-zero after an MP3 range seek).
    base: f64,
    /// Discard packets before this track time (FLAC seeks).
    skip_until: f64,
    /// Track time (s) at the end of the audio handed to the sink.
    written: f64,
    samples: Option<SampleBuffer<i16>>,
    eof: bool,
}

struct Player {
    rx: Receiver<Cmd>,
    status: Arc<Mutex<Status>>,
    ctx: egui::Context,
    client: Option<Deezer>,
    quality: Quality,
    queue: Vec<Track>,
    index: usize,
    sink: Option<Box<dyn Sink>>,
    output: Output,
    volume: f32,
    stream: Option<Stream>,
    playing: bool,
    pending: Vec<i16>,
    pending_end: f64,
    /// Sinks with a remote buffer (AirPlay) lose it on pause, so resume re-seeks.
    resume_seek: bool,
    last_publish: Instant,
    alive: bool,
}

impl Player {
    fn run(mut self) {
        while self.alive {
            let busy = self.playing && self.stream.is_some();
            if busy {
                match self.rx.try_recv() {
                    Ok(cmd) => self.handle(cmd),
                    Err(TryRecvError::Empty) => self.step(),
                    Err(TryRecvError::Disconnected) => return,
                }
                if self.last_publish.elapsed() > Duration::from_millis(250) {
                    self.publish(false);
                }
            } else {
                match self.rx.recv() {
                    Ok(cmd) => self.handle(cmd),
                    Err(_) => return,
                }
            }
        }
    }

    /// Sleep up to `d` but wake immediately for commands.
    fn wait(&mut self, d: Duration) {
        match self.rx.recv_timeout(d) {
            Ok(cmd) => self.handle(cmd),
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => self.alive = false,
        }
    }

    fn position(&self) -> f64 {
        let Some(s) = &self.stream else { return 0.0 };
        let delay = self.sink.as_ref().map_or(0, |k| k.pending_frames() + k.latency_frames());
        (s.written - delay as f64 / RATE as f64).max(0.0)
    }

    fn publish(&mut self, repaint: bool) {
        self.last_publish = Instant::now();
        let position = self.position();
        let mut st = self.status.lock().unwrap();
        st.position = position;
        st.track = self.queue.get(self.index).cloned().filter(|_| self.stream.is_some() || self.playing);
        st.state = match (&self.stream, self.playing) {
            (None, true) => State::Loading,
            (None, false) => State::Stopped,
            (Some(_), true) => State::Playing,
            (Some(_), false) => State::Paused,
        };
        st.output = self.output.name().to_string();
        st.volume = self.volume;
        drop(st);
        if repaint {
            self.ctx.request_repaint();
        }
    }

    fn error(&mut self, e: String) {
        eprintln!("dust: {e}");
        self.status.lock().unwrap().error = Some(e);
        self.ctx.request_repaint();
    }

    fn handle(&mut self, cmd: Cmd) {
        match cmd {
            Cmd::Client(c) => self.client = Some(c),
            Cmd::Quality(q) => self.quality = q,
            Cmd::Play(queue, index) => {
                self.queue = queue;
                self.index = index;
                self.start_track(0.0);
            }
            Cmd::Resume if self.playing || self.stream.is_none() => {}
            Cmd::Pause if !self.playing => {}
            Cmd::Resume | Cmd::Pause => self.handle(Cmd::Toggle),
            Cmd::VolumeStep(d) => self.handle(Cmd::Volume((self.volume + d).clamp(0.0, 1.0))),
            Cmd::Toggle => {
                if self.stream.is_none() {
                    if !self.queue.is_empty() {
                        self.start_track(0.0);
                    }
                } else if self.playing {
                    self.playing = false;
                    let pos = self.position();
                    if let Some(s) = self.sink.as_mut() {
                        s.pause();
                        self.resume_seek = s.latency_frames() > 0;
                    }
                    if self.resume_seek {
                        self.seek(pos);
                    }
                } else {
                    self.playing = true;
                    if let Some(s) = self.sink.as_mut() {
                        s.resume();
                    }
                }
            }
            Cmd::Next => {
                if self.index + 1 < self.queue.len() {
                    self.index += 1;
                    self.start_track(0.0);
                }
            }
            Cmd::Prev => {
                if self.position() > 3.0 || self.index == 0 {
                    self.seek(0.0);
                } else {
                    self.index -= 1;
                    self.start_track(0.0);
                }
            }
            Cmd::Seek(t) => self.seek(t),
            Cmd::Volume(v) => {
                self.volume = v;
                if let Some(s) = self.sink.as_mut() {
                    s.set_volume(v);
                }
            }
            Cmd::Output(o) => {
                let pos = self.position();
                self.sink = None; // tear down first: may be the same device
                self.output = o;
                if self.stream.is_some() {
                    self.ensure_sink();
                    self.seek(pos);
                    if !self.playing
                        && let Some(s) = self.sink.as_mut()
                    {
                        s.pause();
                    }
                }
            }
        }
        self.publish(true);
    }

    fn ensure_sink(&mut self) {
        if self.sink.is_some() {
            return;
        }
        let sink: Result<Box<dyn Sink>, String> = match &self.output {
            Output::Local => LocalSink::new(self.volume).map(|s| Box::new(s) as _),
            Output::AirPlay(d) => AirPlaySink::connect(d, self.volume).map(|s| Box::new(s) as _),
        };
        match sink {
            Ok(s) => self.sink = Some(s),
            Err(e) => {
                self.error(format!("{}: {e}", self.output.name()));
                if self.output != Output::Local {
                    self.output = Output::Local;
                    self.sink = LocalSink::new(self.volume).ok().map(|s| Box::new(s) as _);
                }
            }
        }
    }

    fn start_track(&mut self, at: f64) {
        self.stream = None;
        self.pending.clear();
        self.playing = true;
        self.status.lock().unwrap().error = None;
        self.publish(true);
        self.ensure_sink();
        if let Some(s) = self.sink.as_mut() {
            s.flush();
            s.resume();
        }
        let mut failures = 0;
        while let Some(track) = self.queue.get(self.index).cloned() {
            match self.open(&track, at) {
                Ok(s) => {
                    self.stream = Some(s);
                    return;
                }
                Err(e) => {
                    self.error(format!("{} – {}: {e}", track.artist, track.title));
                    failures += 1;
                    if failures >= 3 || self.index + 1 >= self.queue.len() {
                        break;
                    }
                    self.index += 1;
                }
            }
        }
        self.playing = false;
    }

    fn open(&self, track: &Track, at: f64) -> Result<Stream, String> {
        let client = self.client.as_ref().ok_or("Not logged in")?;
        let (url, format) = client.stream_url(track, self.quality)?;
        open_stream(client, url, format, track.id, at)
    }

    fn seek(&mut self, t: f64) {
        let Some(s) = self.stream.as_mut() else { return };
        let t = t.max(0.0);
        self.pending.clear();
        let reopen = match s.format_kind {
            Format::Mp3(_) => true,
            // FLAC over a non-seekable HTTP stream: skip forward, or restart and skip.
            Format::Flac => t < s.written,
        };
        if reopen {
            let (url, format, id) = (s.url.clone(), s.format_kind, self.queue[self.index].id);
            let client = self.client.clone().expect("stream implies client");
            match open_stream(&client, url, format, id, t) {
                Ok(new) => self.stream = Some(new),
                Err(_) => {
                    // CDN URL may have expired; resolve again.
                    self.start_track(t);
                    return;
                }
            }
        } else {
            s.skip_until = t;
            s.written = t;
        }
        if let Some(sink) = self.sink.as_mut() {
            sink.flush();
        }
    }

    fn step(&mut self) {
        let Some(sink) = self.sink.as_mut() else {
            self.playing = false;
            return;
        };
        let Some(stream) = self.stream.as_mut() else { return };

        if !self.pending.is_empty() {
            if sink.write(&self.pending) > 0 {
                stream.written = self.pending_end;
                self.pending.clear();
            } else {
                self.wait(Duration::from_millis(10));
            }
            return;
        }

        if stream.eof {
            if sink.pending_frames() == 0 {
                if self.index + 1 < self.queue.len() {
                    self.index += 1;
                    let track = self.queue[self.index].clone();
                    // Keep the sink running (no flush) for a tight transition.
                    match self.open(&track, 0.0) {
                        Ok(s) => self.stream = Some(s),
                        Err(e) => {
                            self.error(format!("{} – {}: {e}", track.artist, track.title));
                            self.start_track(0.0);
                        }
                    }
                } else {
                    self.stream = None;
                    self.playing = false;
                }
                self.publish(true);
            } else {
                self.wait(Duration::from_millis(20));
            }
            return;
        }

        match stream.reader.next_packet() {
            Ok(packet) => {
                if packet.track_id() != stream.track_id {
                    return;
                }
                let t = stream.time_base.calc_time(packet.ts);
                let start = stream.base + t.seconds as f64 + t.frac;
                if start + 0.05 < stream.skip_until {
                    return;
                }
                let decoded = match stream.decoder.decode(&packet) {
                    Ok(d) => d,
                    Err(SymError::DecodeError(_)) => return,
                    Err(e) => {
                        eprintln!("dust: decode: {e}");
                        stream.eof = true;
                        return;
                    }
                };
                let spec = *decoded.spec();
                let frames = decoded.frames();
                if frames == 0 {
                    return;
                }
                let buf = stream.samples.get_or_insert_with(|| SampleBuffer::new(decoded.capacity() as u64, spec));
                if buf.capacity() < decoded.capacity() * spec.channels.count() {
                    *buf = SampleBuffer::new(decoded.capacity() as u64, spec);
                }
                buf.copy_interleaved_ref(decoded);
                let ch = spec.channels.count();
                let s = &buf.samples()[..frames * ch];
                match ch {
                    2 => self.pending.extend_from_slice(s),
                    1 => self.pending.extend(s.iter().flat_map(|&x| [x, x])),
                    _ => self.pending.extend(s.chunks_exact(ch).flat_map(|f| [f[0], f[1]])),
                }
                self.pending_end = start + frames as f64 / spec.rate as f64;
            }
            Err(SymError::ResetRequired) => stream.decoder.reset(),
            Err(SymError::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => stream.eof = true,
            Err(e) => {
                eprintln!("dust: stream: {e}");
                stream.eof = true;
            }
        }
    }
}

fn open_stream(client: &Deezer, url: String, format_kind: Format, track_id: u64, at: f64) -> Result<Stream, String> {
    // CBR MP3 maps time to bytes, so jump with an HTTP range; FLAC must start at 0.
    let (offset, skip_until) = match format_kind {
        Format::Mp3(kbps) if at > 0.0 => ((at * kbps as f64 * 125.0) as u64, 0.0),
        _ => (0, at),
    };
    let (reader, offset) = client.open(&url, track_id, offset)?;
    let base = match format_kind {
        Format::Mp3(kbps) => offset as f64 / (kbps as f64 * 125.0),
        Format::Flac => 0.0,
    };
    let mss = MediaSourceStream::new(Box::new(ReadOnlySource::new(reader)), Default::default());
    let mut hint = Hint::new();
    hint.with_extension(if format_kind == Format::Flac { "flac" } else { "mp3" });
    let probed = symphonia::default::get_probe()
        .format(&hint, mss, &FormatOptions::default(), &MetadataOptions::default())
        .map_err(|e| format!("probe: {e}"))?;
    let reader = probed.format;
    let track = reader.tracks().iter().find(|t| t.codec_params.codec != CODEC_TYPE_NULL).ok_or("No audio track")?;
    let decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .map_err(|e| format!("decoder: {e}"))?;
    if track.codec_params.sample_rate.is_some_and(|r| r != RATE) {
        return Err(format!("Unsupported sample rate {:?}", track.codec_params.sample_rate));
    }
    Ok(Stream {
        url,
        format_kind,
        track_id: track.id,
        time_base: track.codec_params.time_base.unwrap_or(TimeBase::new(1, RATE)),
        reader,
        decoder,
        base,
        skip_until,
        written: base.max(skip_until),
        samples: None,
        eof: false,
    })
}
