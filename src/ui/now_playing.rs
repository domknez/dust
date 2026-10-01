//! The OS media session: media keys, headset buttons and the "Now Playing" widget
//! (macOS Control Center, Windows media overlay, Linux MPRIS).

use crate::deezer::image_url;
use crate::player::{Cmd, PlayerHandle, State, Status};
use souvlaki::{MediaControlEvent, MediaControls, MediaMetadata, MediaPlayback, MediaPosition, PlatformConfig, SeekDirection};
use std::time::{Duration, Instant};

/// Seek step for "skip forward/back" buttons that don't say how far.
const SEEK_STEP: f64 = 10.0;
/// Re-announce the position when it drifts this far from where the OS would
/// extrapolate it (i.e. after a seek).
const DRIFT: f64 = 1.5;
const COVER_SIZE: u32 = 500;

pub struct NowPlaying {
    controls: MediaControls,
    /// Track id whose metadata was announced.
    track: Option<u64>,
    /// Last announced state, position and when.
    state: State,
    position: f64,
    at: Instant,
}

impl NowPlaying {
    pub fn start(cc: &eframe::CreationContext<'_>, player: PlayerHandle) -> Option<Self> {
        let config = PlatformConfig { display_name: "dust", dbus_name: "dust", hwnd: window_handle(cc) };
        let mut controls = MediaControls::new(config).inspect_err(|e| log_warn!("media keys unavailable: {e:?}")).ok()?;
        controls.attach(move |event| on_event(&player, event)).inspect_err(|e| log_warn!("media keys unavailable: {e:?}")).ok()?;
        Some(Self { controls, track: None, state: State::Stopped, position: 0.0, at: Instant::now() })
    }

    /// Mirror the player into the OS session; cheap when nothing changed.
    pub fn update(&mut self, st: &Status) {
        let track_id = st.track.as_ref().map(|t| t.id);
        if track_id != self.track {
            self.track = track_id;
            let metadata = match &st.track {
                Some(t) => {
                    let cover = (!t.cover.is_empty()).then(|| image_url("cover", &t.cover, COVER_SIZE));
                    let metadata = MediaMetadata {
                        title: Some(&t.title),
                        artist: Some(&t.artist),
                        album: Some(&t.album),
                        cover_url: cover.as_deref(),
                        duration: Some(Duration::from_secs(t.duration.into())),
                    };
                    self.controls.set_metadata(metadata)
                }
                None => self.controls.set_metadata(MediaMetadata::default()),
            };
            if let Err(e) = metadata {
                log_debug!("now playing metadata: {e:?}");
            }
            self.announce(st);
            return;
        }
        let expected = match self.state {
            State::Playing => self.position + self.at.elapsed().as_secs_f64(),
            _ => self.position,
        };
        if st.state != self.state || (st.position - expected).abs() > DRIFT {
            self.announce(st);
        }
    }

    fn announce(&mut self, st: &Status) {
        let progress = Some(MediaPosition(Duration::from_secs_f64(st.position.max(0.0))));
        let playback = match st.state {
            State::Stopped => MediaPlayback::Stopped,
            // Loading counts as playing so the widget doesn't flicker between tracks.
            State::Playing | State::Loading => MediaPlayback::Playing { progress },
            State::Paused => MediaPlayback::Paused { progress },
        };
        if let Err(e) = self.controls.set_playback(playback) {
            log_debug!("now playing state: {e:?}");
        }
        (self.state, self.position, self.at) = (st.state, st.position, Instant::now());
    }
}

fn on_event(player: &PlayerHandle, event: MediaControlEvent) {
    log_debug!("media key: {event:?}");
    let seek_by = |delta: f64| Cmd::Seek((player.status().position + delta).max(0.0));
    let step = |dir| if dir == SeekDirection::Forward { 1.0 } else { -1.0 };
    player.send(match event {
        MediaControlEvent::Play => Cmd::Resume,
        MediaControlEvent::Pause | MediaControlEvent::Stop => Cmd::Pause,
        MediaControlEvent::Toggle => Cmd::Toggle,
        MediaControlEvent::Next => Cmd::Next,
        MediaControlEvent::Previous => Cmd::Prev,
        MediaControlEvent::Seek(dir) => seek_by(step(dir) * SEEK_STEP),
        MediaControlEvent::SeekBy(dir, by) => seek_by(step(dir) * by.as_secs_f64()),
        MediaControlEvent::SetPosition(MediaPosition(at)) => Cmd::Seek(at.as_secs_f64()),
        MediaControlEvent::SetVolume(v) => Cmd::Volume(v.clamp(0.0, 1.0) as f32),
        MediaControlEvent::OpenUri(_) | MediaControlEvent::Raise | MediaControlEvent::Quit => return,
    });
}

/// Windows' media session is tied to a window.
#[cfg(windows)]
fn window_handle(cc: &eframe::CreationContext<'_>) -> Option<*mut std::ffi::c_void> {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    match cc.window_handle().ok()?.as_raw() {
        RawWindowHandle::Win32(h) => Some(h.hwnd.get() as *mut std::ffi::c_void),
        _ => None,
    }
}

#[cfg(not(windows))]
fn window_handle(_: &eframe::CreationContext<'_>) -> Option<*mut std::ffi::c_void> {
    None
}
