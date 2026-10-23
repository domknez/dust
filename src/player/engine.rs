//! The playback loop: turns commands into queue changes, keeps one track decoding
//! into the active output, and publishes a [`Status`] snapshot for the UI.

use super::awake::{self, Awake};
use super::library::SharedLibrary;
use super::listens::ListenTracker;
use super::queue::{Flow, Queue, Removed};
use super::{Cmd, Output, Repeat, SLEEP_FADE, Sleep, State, Status};
use crate::media::{Decoded, OpenError, Playback};
use crate::model::{Quality, Track};
use crate::output::airplay::AirPlaySink;
use crate::output::local::LocalSink;
use crate::output::{RATE, Sink};
use eframe::egui;
use std::sync::mpsc::{Receiver, RecvTimeoutError, TryRecvError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// How often position updates reach the UI while playing.
const PUBLISH_EVERY: Duration = Duration::from_millis(250);
/// Minimum time between automatic reconnects of a lost output.
const RECOVER_EVERY: Duration = Duration::from_secs(3);
/// "Previous" restarts the track instead when past this point (seconds).
const RESTART_THRESHOLD: f64 = 3.0;
/// Volume changes per second while a timed sleep fades out.
const FADE_STEPS_PER_SEC: f32 = 2.0;
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
    /// Which source `library` is (see [`crate::provider::Provider::key`]).
    source: Option<&'static str>,
    quality: Quality,
    queue: Queue,
    listens: ListenTracker,
    output: Output,
    sink: Option<Box<dyn Sink>>,
    make_sink: SinkFactory,
    volume: f32,
    repeat: Repeat,
    sleep: Option<Sleep>,
    /// Share of the volume the sink plays at while a timed sleep fades out.
    sleep_fade: Option<f32>,
    stream: Option<Box<dyn Playback>>,
    playing: bool,
    /// Decoded samples the sink hasn't accepted yet, and the track time they end at.
    pending: Vec<i16>,
    pending_end: f64,
    /// Sinks with a remote buffer (AirPlay) lose it on pause, so resume re-seeks.
    resume_seek: bool,
    last_publish: Instant,
    alive: bool,
    /// The current track waiting for play (a restored queue, or one stopped by a
    /// network error): the position to start it at.
    restored_at: Option<f64>,
    /// Last automatic reconnect of a lost output (rate limit).
    last_recover: Option<Instant>,
    /// Held while playing so the OS doesn't throttle dust.
    awake: Option<Awake>,
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
            source: None,
            quality: Quality::Mp3_320,
            queue: Queue::default(),
            listens: ListenTracker::new(),
            output: Output::Local,
            sink: None,
            make_sink,
            volume,
            repeat: Repeat::Off,
            sleep: None,
            sleep_fade: None,
            stream: None,
            playing: false,
            pending: Vec::new(),
            pending_end: 0.0,
            resume_seek: false,
            last_publish: Instant::now(),
            alive: true,
            restored_at: None,
            last_recover: None,
            awake: None,
        }
    }

    pub fn run(mut self) {
        awake::high_priority_thread();
        let mut tick = Instant::now();
        let mut was_playing = false;
        while self.alive {
            // Count listened time only across intervals where audio was playing.
            let now = Instant::now();
            if was_playing {
                self.listens.heard(now - tick);
            }
            tick = now;
            self.recover_output();
            if self.sleep_due() {
                self.publish(true);
            }
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
        self.recover_output();
        self.sleep_due();
        match cmd {
            Cmd::Client(client) => self.set_source(client.key(), Arc::new(client)),
            Cmd::Quality(q) => self.quality = q,
            Cmd::ReportListens(on) => self.listens.enabled = on,
            Cmd::Output(o) => self.switch_output(o),

            Cmd::Play(tracks, index) => {
                self.queue.play(tracks, index);
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
            Cmd::Shuffle(on) => self.queue.set_shuffle(on),
            Cmd::Repeat(r) => self.repeat = r,
            Cmd::Sleep(s) => {
                self.sleep = s;
                self.end_fade();
            }

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
            Cmd::Restore { tracks, index, position, flow } => {
                if self.queue.is_empty() && self.stream.is_none() && index < tracks.len() {
                    self.queue.replace(tracks, index);
                    self.queue.flow = flow;
                    self.restored_at = Some(position);
                }
            }
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
                let at = self.restored_at.unwrap_or(0.0);
                self.start_track(at);
            }
        } else if self.playing {
            log_debug!("pause");
            self.playing = false;
            let pos = self.position();
            if let Some(s) = self.sink.as_mut() {
                s.pause();
                self.resume_seek = s.latency_frames() > 0;
            }
            if self.resume_seek {
                self.reposition(pos);
            }
        } else {
            log_debug!("resume");
            self.playing = true;
            if let Some(s) = self.sink.as_mut() {
                s.resume();
            }
        }
    }

    fn next(&mut self) {
        if self.forward() {
            self.start_track(0.0);
        }
    }

    /// Move to the next entry: more Flow when it runs out, back to the start with
    /// repeat on. Returns false at the end of the queue.
    fn forward(&mut self) -> bool {
        if !self.queue.has_next() {
            self.extend_flow();
        }
        self.queue.advance() || (self.repeat != Repeat::Off && self.queue.flow.is_none() && self.queue.jump_to(0))
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

    /// The output stopped working (e.g. the network path to an AirPlay speaker went
    /// away when Ethernet was unplugged): set it up again over whatever network is
    /// there now and carry on from the same position. At most every few seconds.
    fn recover_output(&mut self) {
        if self.sink.as_ref().is_none_or(|s| s.healthy()) {
            return;
        }
        if self.last_recover.is_some_and(|t| t.elapsed() < RECOVER_EVERY) {
            return;
        }
        self.last_recover = Some(Instant::now());
        log_warn!("{}: connection lost, reconnecting", self.output.name());
        self.switch_output(self.output.clone());
        self.publish(true);
    }

    fn switch_output(&mut self, output: Output) {
        log_debug!("output -> {} (volume {:.2})", output.name(), self.volume);
        let pos = self.position();
        self.sink = None; // tear down first: may be the same device
        self.output = output;
        if self.stream.is_some() {
            self.ensure_sink();
            self.reposition(pos);
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
    /// A network problem stops at the track instead (see [`Engine::halt`]).
    fn start_track(&mut self, at: f64) {
        self.restored_at = None;
        self.listens.finish(self.library.as_deref());
        self.stream = None;
        self.pending.clear();
        self.playing = true;
        self.status.lock().unwrap().error = None;
        self.publish(true);
        self.ensure_sink();
        log_debug!("start track at {at:.1} s on {} (flush)", self.output.name());
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
                Err(OpenError::Transient(e)) => {
                    self.error(format!("{} – {}: {e}", track.artist, track.title));
                    self.halt(at);
                    return;
                }
                Err(OpenError::Unavailable(e)) => {
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
        if self.sleep == Some(Sleep::EndOfTrack) {
            self.sleep_now();
            return;
        }
        if self.repeat != Repeat::One && !self.forward() {
            self.stop();
            return;
        }
        let Some(track) = self.queue.current().cloned() else {
            self.stop();
            return;
        };
        log_debug!("track ended, continuing on {} without a flush", self.output.name());
        self.listens.finish(self.library.as_deref());
        match self.open(&track, 0.0) {
            Ok(stream) => self.begin(stream, &track),
            Err(OpenError::Transient(e)) => {
                self.error(format!("{} – {}: {e}", track.artist, track.title));
                self.halt(0.0);
            }
            Err(OpenError::Unavailable(e)) => {
                self.error(format!("{} – {}: {e}", track.artist, track.title));
                self.start_track(0.0);
            }
        }
    }

    /// Opening the current track failed for a reason that isn't the track's (no
    /// network): stop there, with the track waiting at `at` so play tries again,
    /// rather than skipping tracks that would fail the same way.
    fn halt(&mut self, at: f64) {
        self.stop();
        self.restored_at = Some(at);
    }

    fn open(&self, track: &Track, at: f64) -> Result<Box<dyn Playback>, OpenError> {
        let library = self.library.as_ref().ok_or_else(|| OpenError::Transient("Not logged in".into()))?;
        let started = Instant::now();
        let opened = library.open(track, self.quality, at);
        let ms = started.elapsed().as_millis();
        match &opened {
            Ok(p) => log_info!("opened {} {} – {} at {at:.1} s in {ms} ms ({:?})", track.id, track.artist, track.title, p.format()),
            Err(e) => log_warn!("could not open {} {} – {} after {ms} ms: {e}", track.id, track.artist, track.title),
        }
        opened
    }

    fn begin(&mut self, stream: Box<dyn Playback>, track: &Track) {
        // Podcast episodes aren't songs: Deezer's listen reports (history, Flow,
        // Last.fm) are for its catalogue only.
        if !track.is_episode() {
            self.listens.start(self.library.as_deref(), stream.song_id(), stream.format(), track.duration);
        }
        self.stream = Some(stream);
    }

    /// The sleep timer ran out: pause. Returns whether it did (also when nothing was
    /// playing; the timer is cleared either way).
    fn sleep_due(&mut self) -> bool {
        let Some(Sleep::At(at)) = self.sleep else { return false };
        let now = Instant::now();
        if now < at {
            self.fade_towards(at - now);
            return false;
        }
        self.sleep = None;
        if self.playing && self.stream.is_some() {
            log_info!("sleep timer: pausing");
            self.toggle();
        }
        // Back to the listener's volume, so the next play isn't silent.
        self.end_fade();
        true
    }

    /// In the last [`SLEEP_FADE`] before a timed sleep, lower the sink's volume in
    /// steps (a speaker gets a change about twice a second, not every loop).
    fn fade_towards(&mut self, left: Duration) {
        if left >= SLEEP_FADE || !self.playing {
            return;
        }
        let steps = (SLEEP_FADE.as_secs_f32() * FADE_STEPS_PER_SEC).max(1.0);
        let level = (left.as_secs_f32() / SLEEP_FADE.as_secs_f32() * steps).ceil() / steps;
        if self.sleep_fade != Some(level) {
            self.sleep_fade = Some(level);
            if let Some(s) = self.sink.as_mut() {
                s.set_volume(self.volume * level);
            }
        }
    }

    /// Undo a fade: the sink plays at the listener's volume again.
    fn end_fade(&mut self) {
        if self.sleep_fade.take().is_some()
            && let Some(s) = self.sink.as_mut()
        {
            s.set_volume(self.volume);
        }
    }

    /// The track ended with the sleep timer set to its end: stop, with the next track
    /// waiting at its start for play.
    fn sleep_now(&mut self) {
        log_info!("sleep timer: stopping at the end of the track");
        self.sleep = None;
        self.stop();
        if let Some(s) = self.sink.as_mut() {
            s.pause();
        }
        if self.forward() {
            self.restored_at = Some(0.0);
        }
        self.publish(true);
    }

    /// Play from `library` from now on. Another source than before ends playback and
    /// empties the queue: its tracks belong to the old source.
    fn set_source(&mut self, key: &'static str, library: SharedLibrary) {
        if self.source.is_some_and(|k| k != key) {
            log_info!("music source changed: stopping and clearing the queue");
            self.stop();
            if let Some(s) = self.sink.as_mut() {
                s.pause();
            }
            self.pending.clear();
            self.queue.replace(Vec::new(), 0);
            self.restored_at = None;
        }
        self.source = Some(key);
        self.library = Some(library);
    }

    fn stop(&mut self) {
        self.listens.finish(self.library.as_deref());
        self.stream = None;
        self.playing = false;
    }

    fn seek(&mut self, t: f64) {
        // Restored and not started yet: just move where play will begin.
        if self.stream.is_none()
            && let Some(at) = self.restored_at.as_mut()
        {
            *at = t.max(0.0);
            return;
        }
        self.listens.seeked();
        self.reposition(t);
    }

    /// Move the stream to `t` without counting it as the listener seeking (pausing a
    /// speaker or reconnecting to it starts it again from where it was).
    fn reposition(&mut self, t: f64) {
        let Some(stream) = self.stream.as_mut() else { return };
        log_debug!("seek to {t:.1} s");
        let t = t.max(0.0);
        self.pending.clear();
        if stream.needs_reopen_for(t) {
            match stream.reopen(t) {
                Ok(reopened) => self.stream = Some(reopened),
                Err(OpenError::Transient(e)) => {
                    let track = self.queue.current().map_or_else(String::new, |t| format!("{} – {}: ", t.artist, t.title));
                    self.error(format!("{track}{e}"));
                    self.halt(t);
                    return;
                }
                Err(OpenError::Unavailable(_)) => {
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
        let position = self.restored_at.unwrap_or_else(|| self.position());
        let restored = self.restored_at.is_some();
        let mut st = self.status.lock().unwrap();
        st.position = position;
        st.track = self.queue.current().cloned().filter(|_| self.stream.is_some() || self.playing || restored);
        st.state = match (&self.stream, self.playing) {
            (None, true) => State::Loading,
            (None, false) if restored => State::Paused,
            (None, false) => State::Stopped,
            (Some(_), true) => State::Playing,
            (Some(_), false) => State::Paused,
        };
        st.output = self.output.name().to_string();
        st.volume = self.volume;
        st.shuffle = self.queue.shuffled();
        st.repeat = self.repeat;
        st.sleep = self.sleep;
        st.index = self.queue.index();
        if let Some(snapshot) = self.queue.take_snapshot() {
            st.queue = Arc::new(snapshot);
        }
        st.flow = self.queue.flow.clone();
        drop(st);
        if self.playing != self.awake.is_some() {
            log_debug!("{} keeping the system awake", if self.playing { "start" } else { "stop" });
            self.awake = self.playing.then(Awake::begin);
        }
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
    use crate::model::{DeezerSong, Format, Listen, Origin};
    use crate::player::library::Library;
    use std::sync::mpsc::{Sender, channel};

    fn track(id: u64, secs: u32) -> Track {
        Track {
            id,
            title: format!("t{id}"),
            artist: String::new(),
            artists: Vec::new(),
            album: String::new(),
            album_id: String::new(),
            duration: secs,
            art: None,
            available: true,
            available_from: String::new(),
            origin: Origin::Deezer(DeezerSong::default()),
        }
    }

    /// Tracks yield one "packet" per second; ids in `unplayable` fail to open, and
    /// every track fails while `offline` is set.
    #[derive(Default)]
    struct FakeLibrary {
        unplayable: Vec<u64>,
        offline: std::sync::atomic::AtomicBool,
        next_flow_id: Mutex<u64>,
        started: Mutex<Vec<u64>>,
        listens: Mutex<Vec<Listen>>,
    }

    impl Library for FakeLibrary {
        fn open(&self, track: &Track, _: Quality, at: f64) -> Result<Box<dyn Playback>, OpenError> {
            if self.offline.load(std::sync::atomic::Ordering::Relaxed) {
                return Err(OpenError::Transient("offline".into()));
            }
            if self.unplayable.contains(&track.id) {
                return Err(OpenError::Unavailable("not available".into()));
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
        fn reopen(&self, at: f64) -> Result<Box<dyn Playback>, OpenError> {
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

    /// A sink that stops working when `broken` is set; counts how many were made.
    struct FlakySink(Arc<std::sync::atomic::AtomicBool>);

    impl Sink for FlakySink {
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
        fn healthy(&self) -> bool {
            !self.0.load(std::sync::atomic::Ordering::Relaxed)
        }
    }

    #[test]
    fn lost_output_reconnects_and_keeps_playing() {
        use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
        let broken = Arc::new(AtomicBool::new(false));
        let made = Arc::new(AtomicUsize::new(0));
        let (flag, count) = (broken.clone(), made.clone());
        let (_tx, rx) = channel();
        let status = Arc::new(Mutex::new(Status::default()));
        let factory: SinkFactory = Box::new(move |_, _| {
            count.fetch_add(1, Ordering::Relaxed);
            flag.store(false, Ordering::Relaxed); // a fresh connection works
            Ok(Box::new(FlakySink(flag.clone())) as _)
        });
        let mut e = Engine::with_outputs(rx, status, egui::Context::default(), 0.5, factory);
        e.library = Some(Arc::new(FakeLibrary::default()));
        e.handle(Cmd::Play(vec![track(1, 60), track(2, 60)], 0));
        for _ in 0..6 {
            e.step();
        }
        assert_eq!(made.load(Ordering::Relaxed), 1);
        broken.store(true, Ordering::Relaxed);
        e.recover_output();
        assert_eq!(made.load(Ordering::Relaxed), 2, "set up again");
        assert_eq!(now_playing(&e), Some(1), "same track");
        assert!(e.playing);
        // Rate limited: a second failure right away waits.
        broken.store(true, Ordering::Relaxed);
        e.recover_output();
        assert_eq!(made.load(Ordering::Relaxed), 2);
    }

    #[test]
    fn reconnecting_is_not_a_skip() {
        use std::sync::atomic::AtomicBool;
        let broken = Arc::new(AtomicBool::new(false));
        let flag = broken.clone();
        let (_tx, rx) = channel();
        let status = Arc::new(Mutex::new(Status::default()));
        let factory: SinkFactory = Box::new(move |_, _| {
            flag.store(false, std::sync::atomic::Ordering::Relaxed);
            Ok(Box::new(FlakySink(flag.clone())) as _)
        });
        let mut e = Engine::with_outputs(rx, status, egui::Context::default(), 0.5, factory);
        let library = Arc::new(FakeLibrary::default());
        e.library = Some(library.clone());
        e.handle(Cmd::Play(vec![track(1, 60)], 0));
        broken.store(true, std::sync::atomic::Ordering::Relaxed);
        e.recover_output();
        e.listens.heard(Duration::from_secs(60));
        play_out(&mut e);
        let listens = library.listens.lock().unwrap();
        assert_eq!(listens.len(), 1);
        assert!(!listens[0].skipped, "heard to the end");
    }

    #[test]
    fn sleep_timer_at_end_of_track_waits_at_the_next() {
        let (mut e, _library, _tx) = engine(FakeLibrary::default());
        e.handle(Cmd::Play(vec![track(1, 5), track(2, 5)], 0));
        e.handle(Cmd::Sleep(Some(Sleep::EndOfTrack)));
        play_out(&mut e);
        {
            let st = e.status.lock().unwrap();
            assert_eq!((st.state, st.track.as_ref().map(|t| t.id), st.position), (State::Paused, Some(2), 0.0));
            assert_eq!(st.sleep, None, "used up");
        }
        e.handle(Cmd::Toggle);
        assert_eq!(now_playing(&e), Some(2));
        assert!(e.playing);
    }

    /// Remembers every volume it was given.
    struct VolumeSink(Arc<Mutex<Vec<f32>>>);

    impl Sink for VolumeSink {
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
        fn set_volume(&mut self, v: f32) {
            self.0.lock().unwrap().push(v);
        }
    }

    #[test]
    fn timed_sleep_fades_out_then_restores_the_volume() {
        let volumes = Arc::new(Mutex::new(Vec::new()));
        let seen = volumes.clone();
        let (_tx, rx) = channel();
        let status = Arc::new(Mutex::new(Status::default()));
        let factory: SinkFactory = Box::new(move |_, _| Ok(Box::new(VolumeSink(seen.clone())) as _));
        let mut e = Engine::with_outputs(rx, status, egui::Context::default(), 0.8, factory);
        e.library = Some(Arc::new(FakeLibrary::default()));
        e.handle(Cmd::Play(vec![track(1, 600)], 0));
        let last = || *volumes.lock().unwrap().last().unwrap();

        // Long before the deadline nothing changes.
        e.sleep = Some(Sleep::At(Instant::now() + Duration::from_secs(120)));
        let before = volumes.lock().unwrap().len();
        assert!(!e.sleep_due());
        assert_eq!(volumes.lock().unwrap().len(), before);

        // Halfway through the fade: about half the volume.
        e.sleep = Some(Sleep::At(Instant::now() + SLEEP_FADE / 2));
        e.sleep_due();
        assert!((last() - 0.4).abs() < 0.05, "half volume, got {}", last());
        // Repeated checks within the same step send nothing new.
        let sent = volumes.lock().unwrap().len();
        e.sleep_due();
        assert_eq!(volumes.lock().unwrap().len(), sent);

        // Just before the deadline: nearly silent.
        e.sleep = Some(Sleep::At(Instant::now() + Duration::from_millis(400)));
        e.sleep_due();
        assert!(last() < 0.05, "nearly silent, got {}", last());

        // At the deadline: paused, and the volume is back for the next play.
        e.sleep = Some(Sleep::At(Instant::now()));
        assert!(e.sleep_due());
        assert!(!e.playing);
        assert_eq!(last(), 0.8);
        assert_eq!(e.volume, 0.8);
    }

    #[test]
    fn turning_the_timer_off_mid_fade_restores_the_volume() {
        let volumes = Arc::new(Mutex::new(Vec::new()));
        let seen = volumes.clone();
        let (_tx, rx) = channel();
        let status = Arc::new(Mutex::new(Status::default()));
        let factory: SinkFactory = Box::new(move |_, _| Ok(Box::new(VolumeSink(seen.clone())) as _));
        let mut e = Engine::with_outputs(rx, status, egui::Context::default(), 0.6, factory);
        e.library = Some(Arc::new(FakeLibrary::default()));
        e.handle(Cmd::Play(vec![track(1, 600)], 0));
        e.sleep = Some(Sleep::At(Instant::now() + Duration::from_secs(5)));
        e.sleep_due();
        assert!(*volumes.lock().unwrap().last().unwrap() < 0.2);
        e.handle(Cmd::Sleep(None));
        assert_eq!(*volumes.lock().unwrap().last().unwrap(), 0.6);
        assert!(e.playing);
    }

    #[test]
    fn sleep_timer_pauses_when_due() {
        let (mut e, _library, _tx) = engine(FakeLibrary::default());
        e.handle(Cmd::Play(vec![track(1, 60)], 0));
        e.handle(Cmd::Sleep(Some(Sleep::At(Instant::now() + Duration::from_secs(60)))));
        assert!(!e.sleep_due() && e.playing, "not yet");
        e.sleep = Some(Sleep::At(Instant::now()));
        assert!(e.sleep_due());
        assert!(!e.playing && e.stream.is_some(), "paused, not stopped");
        assert_eq!(e.sleep, None);
    }

    #[test]
    fn episodes_are_not_reported_as_listens() {
        let (mut e, library, _tx) = engine(FakeLibrary::default());
        let episode = crate::model::Episode { url: "https://example.com/ep.mp3".into(), show_id: "1".into(), published: String::new() };
        e.handle(Cmd::Play(vec![Track { origin: Origin::Episode(episode), ..track(1, 5) }, track(2, 5)], 0));
        e.listens.heard(Duration::from_secs(5));
        e.handle(Cmd::Next);
        e.listens.heard(Duration::from_secs(5));
        play_out(&mut e);
        assert_eq!(*library.started.lock().unwrap(), vec![2]);
        assert_eq!(library.listens.lock().unwrap().iter().map(|l| l.song_id).collect::<Vec<_>>(), vec![2]);
    }

    #[test]
    fn switching_source_clears_the_queue() {
        let (mut e, library, _tx) = engine(FakeLibrary::default());
        e.set_source("", library.clone());
        e.handle(Cmd::Play(vec![track(1, 60), track(2, 60)], 0));
        assert_eq!(now_playing(&e), Some(1));
        // Logging in again to the same source keeps playing.
        e.set_source("", library.clone());
        assert_eq!(now_playing(&e), Some(1));
        e.set_source("navidrome", library);
        e.publish(true);
        assert_eq!(now_playing(&e), None);
        assert!(!e.playing && e.queue.is_empty());
        assert_eq!(e.status.lock().unwrap().state, State::Stopped);
    }

    #[test]
    fn restored_queue_waits_paused_then_resumes_at_its_position() {
        let (mut e, _library, _tx) = engine(FakeLibrary::default());
        let tracks = vec![track(1, 200), track(2, 200)];
        e.handle(Cmd::Restore { tracks, index: 1, position: 80.0, flow: Some(Flow { mood: Some("chill".into()) }) });
        {
            let st = e.status.lock().unwrap();
            assert_eq!((st.state, st.track.as_ref().map(|t| t.id), st.position), (State::Paused, Some(2), 80.0));
            assert_eq!(st.flow, Some(Flow { mood: Some("chill".into()) }));
        }
        assert_eq!(now_playing(&e), None, "nothing streams before play");
        e.handle(Cmd::Seek(95.0));
        assert_eq!(e.status.lock().unwrap().position, 95.0, "seeking while restored moves the start");
        e.handle(Cmd::Toggle);
        assert_eq!(now_playing(&e), Some(2));
        assert_eq!(e.stream.as_ref().map(|s| s.written()), Some(95.0), "starts where it was");
    }

    #[test]
    fn restore_never_replaces_a_live_queue() {
        let (mut e, _library, _tx) = engine(FakeLibrary::default());
        e.handle(Cmd::Play(vec![track(1, 200)], 0));
        e.handle(Cmd::Restore { tracks: vec![track(7, 200)], index: 0, position: 10.0, flow: None });
        assert_eq!(now_playing(&e), Some(1));
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

    /// Step until `n` tracks have started (bounded).
    fn play_until_started(e: &mut Engine, library: &FakeLibrary, n: usize) {
        for _ in 0..10_000 {
            if library.started.lock().unwrap().len() >= n || !e.playing {
                return;
            }
            e.step();
        }
        panic!("never got to {n} tracks");
    }

    #[test]
    fn repeat_all_starts_over_and_repeat_one_stays() {
        let (mut e, library, _tx) = engine(FakeLibrary::default());
        e.handle(Cmd::Repeat(Repeat::All));
        e.handle(Cmd::Play(vec![track(1, 2), track(2, 2)], 0));
        play_until_started(&mut e, &library, 4);
        assert_eq!(*library.started.lock().unwrap(), [1, 2, 1, 2]);
        e.handle(Cmd::Repeat(Repeat::One));
        play_until_started(&mut e, &library, 6);
        assert_eq!(library.started.lock().unwrap()[4..], [2, 2]);
        e.handle(Cmd::Next);
        assert_eq!(now_playing(&e), Some(1), "next still moves on (and wraps)");
        e.handle(Cmd::Repeat(Repeat::Off));
        play_out(&mut e);
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
    fn network_errors_stop_at_the_track_instead_of_skipping() {
        let offline = FakeLibrary { offline: true.into(), ..Default::default() };
        let (mut e, library, _tx) = engine(offline);
        e.handle(Cmd::Play(vec![track(1, 2), track(2, 2)], 0));
        assert_eq!((now_playing(&e), e.queue.index()), (None, 0), "stayed on the first track");
        assert!(!e.playing);
        assert!(e.status.lock().unwrap().error.as_deref().is_some_and(|m| m.contains("offline")));
        assert_eq!(e.status.lock().unwrap().state, State::Paused, "waits for play");

        // Back online: play tries the same track again.
        library.offline.store(false, std::sync::atomic::Ordering::Relaxed);
        e.handle(Cmd::Toggle);
        assert_eq!(now_playing(&e), Some(1));
        assert!(e.status.lock().unwrap().error.is_none());
    }

    #[test]
    fn network_error_on_the_next_track_keeps_the_queue_there() {
        let (mut e, library, _tx) = engine(FakeLibrary::default());
        e.handle(Cmd::Play(vec![track(1, 2), track(2, 2), track(3, 2)], 0));
        library.offline.store(true, std::sync::atomic::Ordering::Relaxed);
        play_out(&mut e);
        assert_eq!((now_playing(&e), e.queue.index()), (None, 1), "stopped at the second track, not past it");
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
        assert_eq!(e.status.lock().unwrap().flow, Some(Flow { mood: Some("chill".into()) }));
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
