//! The playback loop: turns commands into queue changes, keeps one track decoding
//! into the active output, and publishes a [`Status`] snapshot for the UI.

use super::listens::ListenTracker;
use super::queue::{Flow, Queue, Removed};
use super::stream::{Decoded, Stream};
use super::{Cmd, Output, State, Status};
use crate::deezer::{Deezer, Quality, Track};
use crate::output::airplay::AirPlaySink;
use crate::output::local::LocalSink;
use crate::output::{RATE, Sink};
use eframe::egui;
use std::sync::mpsc::{Receiver, RecvTimeoutError, TryRecvError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// How often position updates reach the UI while playing.
const PUBLISH_EVERY: Duration = Duration::from_millis(250);
/// "Previous" restarts the track instead when past this point (seconds).
const RESTART_THRESHOLD: f64 = 3.0;
/// Consecutive unplayable tracks skipped before giving up.
const MAX_OPEN_FAILURES: usize = 3;

pub struct Engine {
    rx: Receiver<Cmd>,
    status: Arc<Mutex<Status>>,
    ctx: egui::Context,
    client: Option<Deezer>,
    quality: Quality,
    queue: Queue,
    listens: ListenTracker,
    output: Output,
    sink: Option<Box<dyn Sink>>,
    volume: f32,
    stream: Option<Stream>,
    playing: bool,
    /// Decoded samples the sink hasn't accepted yet, and the track time they end at.
    pending: Vec<i16>,
    pending_end: f64,
    /// Sinks with a remote buffer (AirPlay) lose it on pause, so resume re-seeks.
    resume_seek: bool,
    last_publish: Instant,
    alive: bool,
}

impl Engine {
    pub fn new(rx: Receiver<Cmd>, status: Arc<Mutex<Status>>, ctx: egui::Context, volume: f32) -> Self {
        Self {
            rx,
            status,
            ctx,
            client: None,
            quality: Quality::Mp3_320,
            queue: Queue::default(),
            listens: ListenTracker::new(),
            output: Output::Local,
            sink: None,
            volume,
            stream: None,
            playing: false,
            pending: Vec::new(),
            pending_end: 0.0,
            resume_seek: false,
            last_publish: Instant::now(),
            alive: true,
        }
    }

    pub fn run(mut self) {
        let mut tick = Instant::now();
        let mut was_playing = false;
        while self.alive {
            // Count listened time only across intervals where audio was playing.
            let now = Instant::now();
            if was_playing {
                self.listens.heard(now - tick);
            }
            tick = now;
            let busy = self.playing && self.stream.is_some();
            was_playing = busy;
            if busy {
                match self.rx.try_recv() {
                    Ok(cmd) => self.handle(cmd),
                    Err(TryRecvError::Empty) => self.step(),
                    Err(TryRecvError::Disconnected) => return,
                }
                if self.last_publish.elapsed() > PUBLISH_EVERY {
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

    // ------------------------------------------------------------ commands

    fn handle(&mut self, cmd: Cmd) {
        match cmd {
            Cmd::Client(c) => self.client = Some(c),
            Cmd::Quality(q) => self.quality = q,
            Cmd::ReportListens(on) => self.listens.enabled = on,
            Cmd::Output(o) => self.switch_output(o),

            Cmd::Play(tracks, index) => {
                self.queue.replace(tracks, index);
                self.start_track(0.0);
            }
            Cmd::PlayFlow(mood) => self.play_flow(mood),
            Cmd::Toggle => self.toggle(),
            Cmd::Resume if self.playing || self.stream.is_none() => {}
            Cmd::Pause if !self.playing => {}
            Cmd::Resume | Cmd::Pause => self.toggle(),
            Cmd::Next => self.next(),
            Cmd::Prev => self.previous(),
            Cmd::Seek(t) => self.seek(t),
            Cmd::Volume(v) => self.set_volume(v),
            Cmd::VolumeStep(d) => self.set_volume((self.volume + d).clamp(0.0, 1.0)),

            Cmd::Enqueue(tracks) => self.queue.enqueue(tracks),
            Cmd::PlayNext(tracks) => self.queue.play_next(tracks),
            Cmd::Remove(i) => match self.queue.remove(i) {
                Removed::Current { replaced: true } => self.start_track(0.0),
                Removed::Current { replaced: false } => self.stop(),
                Removed::Other | Removed::Nothing => {}
            },
            Cmd::Move(from, to) => self.queue.move_entry(from, to),
            Cmd::JumpTo(i) => {
                if self.queue.jump_to(i) {
                    self.start_track(0.0);
                }
            }
            Cmd::ClearUpcoming => self.queue.clear_upcoming(),
        }
        self.publish(true);
    }

    fn play_flow(&mut self, mood: Option<String>) {
        self.queue.start_flow(Flow { mood });
        if self.extend_flow() {
            self.start_track(0.0);
        } else {
            self.queue.flow = None;
        }
    }

    fn toggle(&mut self) {
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

    fn next(&mut self) {
        if !self.queue.has_next() {
            self.extend_flow();
        }
        if self.queue.advance() {
            self.start_track(0.0);
        }
    }

    fn previous(&mut self) {
        if self.position() > RESTART_THRESHOLD || !self.queue.back() {
            self.seek(0.0);
        } else {
            self.start_track(0.0);
        }
    }

    fn set_volume(&mut self, v: f32) {
        self.volume = v;
        if let Some(s) = self.sink.as_mut() {
            s.set_volume(v);
        }
    }

    fn switch_output(&mut self, output: Output) {
        log_debug!("output -> {} (volume {:.2})", output.name(), self.volume);
        let pos = self.position();
        self.sink = None; // tear down first: may be the same device
        self.output = output;
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

    // ------------------------------------------------------------ tracks

    /// Append the next batch of Flow tracks. Returns whether anything was added.
    fn extend_flow(&mut self) -> bool {
        let (Some(flow), Some(client)) = (self.queue.flow.clone(), self.client.clone()) else { return false };
        match client.flow(flow.mood.as_deref()) {
            Ok(tracks) => self.queue.extend_unique(tracks),
            Err(e) => {
                self.error(format!("Flow: {e}"));
                false
            }
        }
    }

    /// (Re)start the current queue entry at `at` seconds, skipping unplayable tracks.
    fn start_track(&mut self, at: f64) {
        self.listens.finish(self.client.as_ref());
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
        while let Some(track) = self.queue.current().cloned() {
            match self.open(&track, at) {
                Ok(stream) => {
                    self.begin(stream, &track);
                    return;
                }
                Err(e) => {
                    self.error(format!("{} – {}: {e}", track.artist, track.title));
                    failures += 1;
                    if failures >= MAX_OPEN_FAILURES || !self.queue.advance() {
                        break;
                    }
                }
            }
        }
        self.playing = false;
    }

    /// Continue with the next track after the current one ended, without flushing the
    /// sink so the transition stays tight.
    fn continue_after_end(&mut self) {
        if !self.queue.has_next() {
            self.extend_flow();
        }
        if !self.queue.advance() {
            self.stop();
            return;
        }
        let track = self.queue.current().cloned().expect("advanced to an entry");
        self.listens.finish(self.client.as_ref());
        match self.open(&track, 0.0) {
            Ok(stream) => self.begin(stream, &track),
            Err(e) => {
                self.error(format!("{} – {}: {e}", track.artist, track.title));
                self.start_track(0.0);
            }
        }
    }

    fn open(&self, track: &Track, at: f64) -> Result<Stream, String> {
        let client = self.client.as_ref().ok_or("Not logged in")?;
        let source = client.stream_source(track, self.quality).map_err(|e| e.to_string())?;
        Stream::open(client, source, at)
    }

    fn begin(&mut self, stream: Stream, track: &Track) {
        self.listens.start(self.client.as_ref(), stream.source.song_id, stream.source.format, track.duration);
        self.stream = Some(stream);
    }

    fn stop(&mut self) {
        self.listens.finish(self.client.as_ref());
        self.stream = None;
        self.playing = false;
    }

    fn seek(&mut self, t: f64) {
        self.listens.seeked();
        let Some(stream) = self.stream.as_mut() else { return };
        let t = t.max(0.0);
        self.pending.clear();
        if stream.needs_reopen_for(t) {
            let client = self.client.clone().expect("a stream implies a client");
            match Stream::open(&client, stream.source.clone(), t) {
                Ok(reopened) => self.stream = Some(reopened),
                Err(_) => {
                    // The CDN URL may have expired; resolve it again.
                    self.start_track(t);
                    return;
                }
            }
        } else {
            stream.skip_to(t);
        }
        if let Some(sink) = self.sink.as_mut() {
            sink.flush();
        }
    }

    // ------------------------------------------------------------ audio

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

    /// One unit of work while playing: feed the sink, or decode the next packet.
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
                self.continue_after_end();
                self.publish(true);
            } else {
                self.wait(Duration::from_millis(20));
            }
            return;
        }
        match stream.decode_next(&mut self.pending) {
            Decoded::Audio { ends_at } => self.pending_end = ends_at,
            Decoded::Nothing => {}
            Decoded::End => stream.eof = true,
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

    // ------------------------------------------------------------ status

    /// Playback position of what is audible now (sink buffering and latency removed).
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
        st.track = self.queue.current().cloned().filter(|_| self.stream.is_some() || self.playing);
        st.state = match (&self.stream, self.playing) {
            (None, true) => State::Loading,
            (None, false) => State::Stopped,
            (Some(_), true) => State::Playing,
            (Some(_), false) => State::Paused,
        };
        st.output = self.output.name().to_string();
        st.volume = self.volume;
        st.index = self.queue.index();
        if let Some(snapshot) = self.queue.take_snapshot() {
            st.queue = Arc::new(snapshot);
        }
        st.flow = self.queue.flow.as_ref().map(Flow::id);
        drop(st);
        if repaint {
            self.ctx.request_repaint();
        }
    }

    fn error(&mut self, e: String) {
        log_error!("{e}");
        self.status.lock().unwrap().error = Some(e);
        self.ctx.request_repaint();
    }
}
