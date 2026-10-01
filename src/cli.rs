//! Command-line modes besides the app itself: the login helper and diagnostics.
//!
//! - `--tone [speaker | ip:port]`  4 s test tone on an output
//! - `--debug-home`                home page structure for the stored session
//! - `--debug-tracks <kind> <id>`  what a home item plays (flow|mix|album|artist|playlist)
//! - `--debug-stream <playlist> [n]`  stream resolution per track, incl. fallbacks

use crate::credentials;
use crate::deezer::{Deezer, Quality};
use crate::output::airplay::{AirPlaySink, Device, Discovery};
use crate::output::local::LocalSink;
use crate::output::{RATE, Sink};
use std::time::{Duration, Instant};

pub enum Command {
    #[cfg(feature = "login-window")]
    LoginWindow,
    Tone(Option<String>),
    DebugHome,
    DebugTracks {
        kind: String,
        id: String,
    },
    DebugStream {
        playlist: u64,
        count: usize,
    },
}

impl Command {
    /// None means: run the app.
    pub fn parse(mut args: impl Iterator<Item = String>) -> Option<Command> {
        let cmd = match args.next()?.as_str() {
            #[cfg(feature = "login-window")]
            crate::login::ARG => Command::LoginWindow,
            "--tone" => Command::Tone(args.next()),
            "--debug-home" => Command::DebugHome,
            "--debug-tracks" => Command::DebugTracks { kind: args.next().unwrap_or_default(), id: args.next().unwrap_or_default() },
            "--debug-stream" => Command::DebugStream {
                playlist: args.next().and_then(|a| a.parse().ok()).unwrap_or(0),
                count: args.next().and_then(|a| a.parse().ok()).unwrap_or(10),
            },
            _ => return None,
        };
        Some(cmd)
    }

    pub fn run(self) {
        match self {
            #[cfg(feature = "login-window")]
            Command::LoginWindow => crate::login::run_window(),
            Command::Tone(target) => tone(target),
            Command::DebugHome => debug_home(&logged_in()),
            Command::DebugTracks { kind, id } => debug_tracks(&logged_in(), &kind, &id),
            Command::DebugStream { playlist, count } => debug_stream(&logged_in(), playlist, count),
        }
    }
}

fn logged_in() -> Deezer {
    let arl = credentials::load().expect("no stored session; log in with the app first");
    Deezer::login(&arl).expect("login")
}

/// Play a 440 Hz tone for 4 s on the local output or an AirPlay speaker.
fn tone(target: Option<String>) {
    let mut sink: Box<dyn Sink> = match target {
        None => Box::new(LocalSink::new(0.5).expect("local output")),
        Some(target) => Box::new(AirPlaySink::connect(&find_speaker(&target), 0.3).expect("AirPlay connect")),
    };
    if std::env::var_os("DUST_TONE_FLUSH").is_some() {
        sink.flush(); // what the player does right after switching output (seek)
    }
    let mut n = 0u32;
    let mut chunk = Vec::with_capacity(2048);
    while n < RATE * 4 {
        chunk.clear();
        for _ in 0..1024 {
            let v = ((n as f32 * 440.0 * std::f32::consts::TAU / RATE as f32).sin() * 8000.0) as i16;
            chunk.extend([v, v]);
            n += 1;
        }
        while sink.write(&chunk) == 0 {
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    while sink.pending_frames() > 0 {
        std::thread::sleep(Duration::from_millis(20));
    }
    std::thread::sleep(Duration::from_millis(sink.latency_frames() as u64 * 1000 / RATE as u64));
    drop(sink);
    println!("done");
}

/// A speaker by name (via mDNS), or directly by `ip:port` (for test receivers;
/// DUST_AP2=1 / DUST_PTP=1 select the AirPlay 2 / PTP paths).
fn find_speaker(target: &str) -> Device {
    if let Ok(addr) = target.parse() {
        return Device {
            id: target.into(),
            name: target.into(),
            addrs: vec![addr],
            supported: true,
            password: false,
            auth_setup: false,
            ap2: std::env::var_os("DUST_AP2").is_some(),
            ptp: std::env::var_os("DUST_PTP").is_some(),
        };
    }
    let discovery = Discovery::start(|| {}).expect("mDNS");
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(d) = discovery.devices().into_iter().find(|d| d.name == target) {
            println!("connecting to {d:?}");
            return d;
        }
        assert!(Instant::now() < deadline, "AirPlay device {target:?} not found");
        std::thread::sleep(Duration::from_millis(100));
    }
}

fn debug_home(client: &Deezer) {
    for sec in client.home().expect("home") {
        println!("\n## {} [{}] ({} items)", sec.title, sec.layout, sec.items.len());
        for it in sec.items.iter().take(8) {
            println!("  - {}:{} | {} | {} | pic={:?}", it.kind, it.id, it.title, it.subtitle, it.picture);
        }
    }
}

fn debug_tracks(client: &Deezer, kind: &str, id: &str) {
    let tracks = match kind {
        "flow" => client.flow(Some(id).filter(|i| !i.is_empty() && *i != "default")),
        "mix" => client.mix(id),
        "album" => client.album(id),
        "artist" => client.artist_top(id),
        _ => client.playlist(id.parse().unwrap_or(0)),
    };
    match tracks {
        Ok(t) => {
            println!("{} tracks", t.len());
            t.iter().take(5).for_each(|t| println!("  {} – {}", t.artist, t.title));
        }
        Err(e) => println!("error: {e}"),
    }
}

fn debug_stream(client: &Deezer, playlist: u64, count: usize) {
    for t in client.playlist(playlist).expect("playlist").iter().take(count) {
        let how = match client.stream_source(t, Quality::Mp3_320) {
            Ok(s) if s.song_id != t.id => format!("OK via fallback {} ({:?})", s.song_id, s.format),
            Ok(s) => format!("OK ({:?})", s.format),
            Err(e) => format!("FAIL {e} (fallback: {:?})", t.fallback.as_ref().map(|f| f.0)),
        };
        println!("{:<50} {how}", format!("{} – {}", t.artist, t.title));
    }
}
