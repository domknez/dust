//! Command-line modes besides the app itself: the login helper and diagnostics.
//!
//! - `--tone [speaker | ip:port]`  4 s test tone on an output
//! - `--list-speakers`            AirPlay speakers found on the network (silent)
//! - `--check-update`             is a newer release out, and can this copy install it
//! - `--self-update`              install the latest release in place (no relaunch)
//! - `--debug-home`                home page structure for the stored session
//! - `--debug-search <query>`      search results: track count and card sections
//! - `--debug-tracks <kind> <id>`  what a home item plays (flow|mix|album|artist|playlist)
//! - `--debug-stream <playlist> [n]`  stream resolution per track, incl. fallbacks
//! - `--debug-decode <playlist> [quality] [start s]`  decrypt and decode 10 s of the first track (no sound)

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
    ListSpeakers,
    CheckUpdate,
    SelfUpdate,
    DebugHome,
    DebugSearch(String),
    DebugTracks {
        kind: String,
        id: String,
    },
    DebugStream {
        playlist: u64,
        count: usize,
    },
    DebugDecode {
        playlist: u64,
        quality: Quality,
        start: f64,
    },
}

impl Command {
    /// None means: run the app.
    pub fn parse(mut args: impl Iterator<Item = String>) -> Option<Command> {
        let cmd = match args.next()?.as_str() {
            #[cfg(feature = "login-window")]
            crate::login::ARG => Command::LoginWindow,
            "--tone" => Command::Tone(args.next()),
            "--list-speakers" => Command::ListSpeakers,
            "--check-update" => Command::CheckUpdate,
            "--self-update" => Command::SelfUpdate,
            "--debug-home" => Command::DebugHome,
            "--debug-search" => Command::DebugSearch(args.collect::<Vec<_>>().join(" ")),
            "--debug-tracks" => Command::DebugTracks { kind: args.next().unwrap_or_default(), id: args.next().unwrap_or_default() },
            "--debug-stream" => Command::DebugStream {
                playlist: args.next().and_then(|a| a.parse().ok()).unwrap_or(0),
                count: args.next().and_then(|a| a.parse().ok()).unwrap_or(10),
            },
            "--debug-decode" => Command::DebugDecode {
                playlist: args.next().and_then(|a| a.parse().ok()).unwrap_or(0),
                quality: args.next().and_then(|q| Quality::from_key(&q)).unwrap_or(Quality::Mp3_320),
                start: args.next().and_then(|a| a.parse().ok()).unwrap_or(0.0),
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
            Command::ListSpeakers => list_speakers(),
            Command::CheckUpdate => check_update(),
            Command::SelfUpdate => self_update(),
            Command::DebugHome => debug_home(&logged_in()),
            Command::DebugSearch(query) => debug_search(&logged_in(), &query),
            Command::DebugTracks { kind, id } => debug_tracks(&logged_in(), &kind, &id),
            Command::DebugStream { playlist, count } => debug_stream(&logged_in(), playlist, count),
            Command::DebugDecode { playlist, quality, start } => debug_decode(&logged_in(), playlist, quality, start),
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

/// Browse for 3 s and print what was found, with the protocol each uses.
fn list_speakers() {
    let discovery = Discovery::start(|| {}).expect("mDNS");
    std::thread::sleep(Duration::from_secs(3));
    for d in discovery.devices() {
        let protocol = if d.ap2 { "AirPlay 2" } else { "AirPlay 1" };
        let addrs: Vec<String> = d.addrs.iter().map(|a| a.to_string()).collect();
        let note = if d.supported { "" } else { "  (unsupported)" };
        println!("{:<24} {protocol:<10} {}{note}", d.name, addrs.join(", "));
    }
}

fn check_update() {
    match crate::update::check() {
        Ok(Some(release)) => {
            println!("dust {} is available (you have {})", release.version, crate::update::CURRENT);
            println!("installs in place: {}", crate::update::can_install(&release));
            println!("{}", release.page);
        }
        Ok(None) => println!("dust {} is the latest version", crate::update::CURRENT),
        Err(e) => println!("check failed: {e}"),
    }
}

fn self_update() {
    let release = match crate::update::check() {
        Ok(Some(release)) => release,
        Ok(None) => return println!("dust {} is the latest version", crate::update::CURRENT),
        Err(e) => return println!("check failed: {e}"),
    };
    let progress = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
    match crate::update::install(&release, &progress) {
        Ok(_) => println!("installed dust {}; start it again to use it", release.version),
        Err(e) => println!("update failed: {e}"),
    }
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

fn debug_search(client: &Deezer, query: &str) {
    let results = client.search(query).expect("search");
    println!("{} tracks", results.tracks.len());
    results.tracks.iter().take(3).for_each(|t| println!("  {} – {}", t.artist, t.title));
    for sec in results.sections {
        println!("\n## {} ({} items)", sec.title, sec.items.len());
        for it in sec.items.iter().take(4) {
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

/// End-to-end check of the playback pipeline: resolve, download, decrypt and decode
/// ~10 s of the first playable track, without an audio device.
fn debug_decode(client: &Deezer, playlist: u64, quality: Quality, start: f64) {
    use crate::player::library::{Decoded, Library};
    for track in client.playlist(playlist).expect("playlist").iter().take(5) {
        log_info!("opening {} – {}", track.artist, track.title);
        let mut playback = match Library::open(client, track, quality, start) {
            Ok(p) => p,
            Err(e) => {
                println!("{} – {}: {e}", track.artist, track.title);
                continue;
            }
        };
        log_info!("opened as {:?}", playback.format());
        let (mut samples, mut seconds, mut first) = (Vec::new(), start, None);
        while seconds < start + 10.0 {
            match playback.decode_next(&mut samples) {
                Decoded::Audio { ends_at } => {
                    first.get_or_insert(ends_at);
                    seconds = ends_at
                }
                Decoded::Nothing => {}
                Decoded::End => break,
            }
        }
        let peak = samples.iter().map(|s| s.unsigned_abs()).max().unwrap_or(0);
        println!(
            "{} – {}: decoded {:.1}–{seconds:.1} s as {:?}, {} samples, peak {peak}",
            track.artist,
            track.title,
            first.unwrap_or(start),
            playback.format(),
            samples.len()
        );
        return;
    }
}
