//! The playback loop: turns commands into queue changes, keeps one track decoding
//! into the active output, and publishes a [`Status`] snapshot for the UI.

use super::library::{Decoded, Playback, SharedLibrary};
use super::listens::ListenTracker;
use super::queue::{Flow, Queue, Removed};
use super::{Cmd, Output, State, Status};
use crate::deezer::{Quality, Track};
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

/// Builds the audio output for an [`Output`] at a volume (real devices in the app).
pub type SinkFactory = Box<dyn Fn(&Output, f32) -> Result<Box<dyn Sink>, String>>;

fn real_output(output: &Output, volume: f32) -> Result<Box<dyn Sink>, String> {
    match output {
        Output::Local => LocalSink::new(volume).map(|s| Box::new(s) as _),
        Output::AirPlay(d) => AirPlaySink::connect(d, volume).map(|s| Box::new(s) as _),
    }
}

pub struct Engine {
    rx: Receiver<Cmd>,
    status: Arc<Mutex<Status>>,
    ctx: egui::Context,
    library: Option<SharedLibrary>,
    quality: Quality,
    queue: Queue,
    listens: ListenTracker,
    output: Output,
    sink: Option<Box<dyn Sink>>,
    make_sink: SinkFactory,
    volume: f32,
    stream: Option<Box<dyn Playback>>,
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
        Self::with_outputs(rx, status, ctx, volume, Box::new(real_output))
    }

    pub fn with_outputs(rx: Receiver<Cmd>, status: Arc<Mutex<Status>>, ctx: egui::Context, volume: f32, make_sink: SinkFactory) -> Self {
        Self {
            rx,
            status,
            ctx,
            library: None,
            quality: Quality::Mp3_320,
            queue: Queue::default(),
            listens: ListenTracker::new(),
            output: Output::Local,
            sink: None,
            make_sink,
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
            Cmd::Client(client) => self.library = Some(Arc::new(client)),
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
        let (Some(flow), Some(library)) = (self.queue.flow.clone(), self.library.clone()) else { return false };
        match library.flow(flow.mood.as_deref()) {
            Ok(tracks) => self.queue.extend_unique(tracks),
            Err(e) => {
                self.error(format!("Flow: {e}"));
                false
            }
        }
    }

    /// (Re)start the current queue entry at `at` seconds, skipping unplayable tracks.
    fn start_track(&mut self, at: f64) {
        self.listens.finish(self.library.as_deref());
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
        self.listens.finish(self.library.as_deref());
        match self.open(&track, 0.0) {
            Ok(stream) => self.begin(stream, &track),
            Err(e) => {
                self.error(format!("{} – {}: {e}", track.artist, track.title));
                self.start_track(0.0);
            }
        }
    }

    fn open(&self, track: &Track, at: f64) -> Result<Box<dyn Playback>, String> {
        let library = self.library.as_ref().ok_or("Not logged in")?;
        library.open(track, self.quality, at)
    }

    fn begin(&mut self, stream: Box<dyn Playback>, track: &Track) {
        self.listens.start(self.library.as_deref(), stream.song_id(), stream.format(), track.duration);
        self.stream = Some(stream);
    }

    fn stop(&mut self) {
        self.listens.finish(self.library.as_deref());
        self.stream = None;
        self.playing = false;
    }

    fn seek(&mut self, t: f64) {
        self.listens.seeked();
        let Some(stream) = self.stream.as_mut() else { return };
        let t = t.max(0.0);
        self.pending.clear();
        if stream.needs_reopen_for(t) {
            match stream.reopen(t) {
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
        match (self.make_sink)(&self.output, self.volume) {
            Ok(s) => self.sink = Some(s),
            Err(e) => {
                self.error(format!("{}: {e}", self.output.name()));
                if self.output != Output::Local {
                    self.output = Output::Local;
                    self.sink = (self.make_sink)(&Output::Local, self.volume).ok();
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
                stream.set_written(self.pending_end);
                self.pending.clear();
            } else {
                self.wait(Duration::from_millis(10));
            }
            return;
        }
        if stream.finished() {
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
            Decoded::Nothing | Decoded::End => {}
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
        (s.written() - delay as f64 / RATE as f64).max(0.0)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::deezer::{Format, Listen};
    use crate::player::library::Library;
    use std::sync::mpsc::{Sender, channel};

    fn track(id: u64, secs: u32) -> Track {
        Track {
            id,
            title: format!("t{id}"),
            artist: String::new(),
            album: String::new(),
            duration: secs,
            token: String::new(),
            cover: String::new(),
            fallback: None,
        }
    }

    /// Tracks yield one "packet" per second; ids in `unplayable` fail to open.
    #[derive(Default)]
    struct FakeLibrary {
        unplayable: Vec<u64>,
        next_flow_id: Mutex<u64>,
        started: Mutex<Vec<u64>>,
        listens: Mutex<Vec<Listen>>,
    }

    impl Library for FakeLibrary {
        fn open(&self, track: &Track, _: Quality, at: f64) -> Result<Box<dyn Playback>, String> {
            if self.unplayable.contains(&track.id) {
                return Err("not available".into());
            }
            Ok(Box::new(FakePlayback { id: track.id, secs: track.duration as f64, written: at, done: false }))
        }

        fn flow(&self, _: Option<&str>) -> Result<Vec<Track>, String> {
            let mut next = self.next_flow_id.lock().unwrap();
            let batch = (0..3).map(|i| track(100 + *next + i, 2)).collect();
            *next += 3;
            Ok(batch)
        }

        fn report_listen_start(&self, song_id: u64) {
            self.started.lock().unwrap().push(song_id);
        }

        fn report_listen(&self, listen: Listen) {
            self.listens.lock().unwrap().push(listen);
        }
    }

    struct FakePlayback {
        id: u64,
        secs: f64,
        written: f64,
        done: bool,
    }

    impl Playback for FakePlayback {
        fn song_id(&self) -> u64 {
            self.id
        }
        fn format(&self) -> Format {
            Format::Mp3(320)
        }
        fn written(&self) -> f64 {
            self.written
        }
        fn set_written(&mut self, t: f64) {
            self.written = t;
        }
        fn finished(&self) -> bool {
            self.done
        }
        fn decode_next(&mut self, out: &mut Vec<i16>) -> Decoded {
            if self.written >= self.secs {
                self.done = true;
                return Decoded::End;
            }
            out.extend([0, 0]);
            Decoded::Audio { ends_at: self.written + 1.0 }
        }
        fn needs_reopen_for(&self, _: f64) -> bool {
            true
        }
        fn skip_to(&mut self, t: f64) {
            self.written = t;
        }
        fn reopen(&self, at: f64) -> Result<Box<dyn Playback>, String> {
            Ok(Box::new(FakePlayback { id: self.id, secs: self.secs, written: at, done: false }))
        }
    }

    /// Accepts everything immediately; no buffering, no latency.
    struct FakeSink;

    impl Sink for FakeSink {
        fn write(&mut self, samples: &[i16]) -> usize {
            samples.len()
        }
        fn pending_frames(&self) -> usize {
            0
        }
        fn latency_frames(&self) -> usize {
            0
        }
        fn pause(&mut self) {}
        fn resume(&mut self) {}
        fn flush(&mut self) {}
        fn set_volume(&mut self, _: f32) {}
    }

    fn engine(library: FakeLibrary) -> (Engine, Arc<FakeLibrary>, Sender<Cmd>) {
        let (tx, rx) = channel();
        let status = Arc::new(Mutex::new(Status::default()));
        let mut e = Engine::with_outputs(rx, status, egui::Context::default(), 0.5, Box::new(|_, _| Ok(Box::new(FakeSink) as _)));
        let library = Arc::new(library);
        e.library = Some(library.clone());
        (e, library, tx)
    }

    /// Run the engine until it stops playing (bounded).
    fn play_out(e: &mut Engine) {
        for _ in 0..10_000 {
            if !e.playing || e.stream.is_none() {
                return;
            }
            e.step();
        }
        panic!("engine never stopped");
    }

    fn now_playing(e: &Engine) -> Option<u64> {
        e.stream.as_ref().map(|s| s.song_id())
    }

    #[test]
    fn plays_through_the_queue_then_stops() {
        let (mut e, library, _tx) = engine(FakeLibrary::default());
        e.handle(Cmd::Play(vec![track(1, 2), track(2, 2)], 0));
        assert_eq!(now_playing(&e), Some(1));
        play_out(&mut e);
        assert_eq!(*library.started.lock().unwrap(), [1, 2]);
        assert_eq!(e.status.lock().unwrap().state, State::Stopped);
    }

    #[test]
    fn skips_unplayable_tracks() {
        let (mut e, _library, _tx) = engine(FakeLibrary { unplayable: vec![1], ..Default::default() });
        e.handle(Cmd::Play(vec![track(1, 2), track(2, 2)], 0));
        assert_eq!(now_playing(&e), Some(2));
        assert!(e.status.lock().unwrap().error.is_some());
    }

    #[test]
    fn gives_up_after_repeated_failures() {
        let (mut e, _library, _tx) = engine(FakeLibrary { unplayable: vec![1, 2, 3], ..Default::default() });
        e.handle(Cmd::Play(vec![track(1, 2), track(2, 2), track(3, 2), track(4, 2)], 0));
        assert_eq!(now_playing(&e), None);
        assert!(!e.playing);
    }

    #[test]
    fn previous_restarts_late_in_a_track_else_goes_back() {
        let (mut e, _library, _tx) = engine(FakeLibrary::default());
        e.handle(Cmd::Play(vec![track(1, 30), track(2, 30)], 1));
        for _ in 0..12 {
            e.step(); // decode + hand off ~6 s
        }
        e.handle(Cmd::Prev);
        assert_eq!((now_playing(&e), e.queue.index()), (Some(2), 1), "restarted the same track");
        e.handle(Cmd::Prev);
        assert_eq!(now_playing(&e), Some(1), "went back to the previous track");
    }

    #[test]
    fn flow_refills_the_queue() {
        let (mut e, _library, _tx) = engine(FakeLibrary::default());
        e.handle(Cmd::PlayFlow(Some("chill".into())));
        assert_eq!(now_playing(&e), Some(100));
        for _ in 0..200 {
            e.step();
        }
        assert!(e.playing, "Flow keeps going");
        assert!(e.queue.index() >= 3, "moved past the first batch");
        assert_eq!(e.status.lock().unwrap().flow.as_deref(), Some("chill"));
    }

    #[test]
    fn skipping_reports_a_skipped_listen() {
        let (mut e, library, _tx) = engine(FakeLibrary::default());
        e.handle(Cmd::Play(vec![track(1, 60), track(2, 60)], 0));
        e.listens.heard(Duration::from_secs(10));
        e.handle(Cmd::Next);
        let listens = library.listens.lock().unwrap();
        assert_eq!(listens.len(), 1);
        assert_eq!((listens[0].song_id, listens[0].listened_secs, listens[0].skipped), (1, 10, true));
    }

    #[test]
    fn removing_the_current_track_plays_the_next() {
        let (mut e, _library, _tx) = engine(FakeLibrary::default());
        e.handle(Cmd::Play(vec![track(1, 5), track(2, 5), track(3, 5)], 1));
        e.handle(Cmd::Remove(1));
        assert_eq!(now_playing(&e), Some(3));
        e.handle(Cmd::Remove(1));
        assert_eq!(now_playing(&e), None);
        assert!(!e.playing);
    }

    #[test]
    fn listen_reports_can_be_turned_off() {
        let (mut e, library, _tx) = engine(FakeLibrary::default());
        e.handle(Cmd::ReportListens(false));
        e.handle(Cmd::Play(vec![track(1, 2)], 0));
        e.listens.heard(Duration::from_secs(2));
        play_out(&mut e);
        assert!(library.started.lock().unwrap().is_empty());
        assert!(library.listens.lock().unwrap().is_empty());
    }
}
