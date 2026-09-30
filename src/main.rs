#![cfg_attr(windows, windows_subsystem = "windows")]
// Fixed-size chunk loops read clearer than as_chunks here.
#![allow(clippy::chunks_exact_to_as_chunks)]

mod deezer;
#[cfg(feature = "login-window")]
mod login;
mod output;
mod player;
mod settings;
mod ui;

use eframe::egui;

/// `dust --tone [airplay device name | ip:port]`: play a short 440 Hz tone to check an output.
fn tone(target: Option<String>) {
    use output::Sink;
    use std::time::{Duration, Instant};
    let mut sink: Box<dyn Sink> = match target {
        None => Box::new(output::local::LocalSink::new(0.5).expect("local output")),
        Some(addr) if addr.parse::<std::net::SocketAddr>().is_ok() => {
            let device = output::airplay::Device {
                id: addr.clone(),
                name: addr.clone(),
                addrs: vec![addr.parse().unwrap()],
                supported: true,
                password: false,
                auth_setup: false,
                ap2: std::env::var_os("DUST_AP2").is_some(),
                ptp: std::env::var_os("DUST_PTP").is_some(),
            };
            Box::new(output::airplay::AirPlaySink::connect(&device, 0.3).expect("AirPlay connect"))
        }
        Some(name) => {
            let discovery = output::airplay::Discovery::start(|| {}).expect("mDNS");
            let deadline = Instant::now() + Duration::from_secs(5);
            let device = loop {
                if let Some(d) = discovery.devices().into_iter().find(|d| d.name == name) {
                    break d;
                }
                assert!(Instant::now() < deadline, "AirPlay device {name:?} not found");
                std::thread::sleep(Duration::from_millis(100));
            };
            println!("connecting to {device:?}");
            Box::new(output::airplay::AirPlaySink::connect(&device, 0.3).expect("AirPlay connect"))
        }
    };
    if std::env::var_os("DUST_TONE_FLUSH").is_some() {
        sink.flush(); // what the player does right after switching output (seek)
    }
    let rate = output::RATE as f32;
    let mut n = 0u32;
    let total = output::RATE * 4;
    let mut chunk = Vec::with_capacity(2048);
    while n < total {
        chunk.clear();
        for _ in 0..1024 {
            let v = ((n as f32 * 440.0 * std::f32::consts::TAU / rate).sin() * 8000.0) as i16;
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
    std::thread::sleep(Duration::from_millis(sink.latency_frames() as u64 * 1000 / output::RATE as u64));
    drop(sink);
    println!("done");
}

/// `dust --debug-home`: print the home page structure for the stored session
/// (section titles, layouts, item types/ids; no tokens).
fn debug_home() {
    let arl = keyring::Entry::new("dust", "arl").and_then(|k| k.get_password()).expect("no stored session; log in first");
    let client = deezer::Deezer::login(&arl).expect("login");
    for sec in client.home().expect("home") {
        println!("\n## {} [{}] ({} items)", sec.title, sec.layout, sec.items.len());
        for it in sec.items.iter().take(8) {
            println!("  - {}:{} | {} | {} | pic={:?}", it.kind, it.id, it.title, it.subtitle, it.picture);
        }
        if let Some(it) = sec.items.iter().find(|i| i.kind == "flow" || i.kind == "smarttracklist") {
            let keys: Vec<&String> = it.data.as_object().map(|o| o.keys().collect()).unwrap_or_default();
            println!("    data keys of {}: {:?}", it.kind, keys);
            println!("    data: {}", it.data.to_string().chars().take(400).collect::<String>());
        }
    }
}

/// `dust --debug-tracks <flow|mix|album|artist|playlist> <id>`: list what a home item plays.
fn debug_tracks(kind: String, id: String) {
    let arl = keyring::Entry::new("dust", "arl").and_then(|k| k.get_password()).expect("no stored session; log in first");
    let client = deezer::Deezer::login(&arl).expect("login");
    let tracks = match kind.as_str() {
        "flow" => client.flow_mood(Some(&id).filter(|i| !i.is_empty() && *i != "default").map(|s| s.as_str())),
        "mix" => client.mix(&id),
        "album" => client.album(&id),
        "artist" => client.artist_top(&id),
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

fn main() -> eframe::Result {
    let mut args = std::env::args().skip(1);
    match args.next().as_deref() {
        Some("--tone") => {
            tone(args.next());
            return Ok(());
        }
        Some("--debug-tracks") => {
            debug_tracks(args.next().unwrap_or_default(), args.next().unwrap_or_default());
            return Ok(());
        }
        Some("--debug-stream") => {
            let arl = keyring::Entry::new("dust", "arl").and_then(|k| k.get_password()).expect("no stored session");
            let client = deezer::Deezer::login(&arl).expect("login");
            let tracks = client.playlist(args.next().and_then(|a| a.parse().ok()).unwrap_or(0)).expect("playlist");
            for t in tracks.iter().take(args.next().and_then(|a| a.parse().ok()).unwrap_or(10)) {
                let r = client.stream_url(t, deezer::Quality::Mp3_320);
                let how = match &r {
                    Ok((_, f, id)) if *id != t.id => format!("OK via fallback {id} ({f:?})"),
                    Ok((_, f, _)) => format!("OK ({f:?})"),
                    Err(e) => format!("FAIL {e} (fallback: {:?})", t.fallback.as_ref().map(|f| f.0)),
                };
                println!("{:<50} {}", format!("{} – {}", t.artist, t.title), how);
            }
            return Ok(());
        }
        Some("--debug-home") => {
            debug_home();
            return Ok(());
        }
        #[cfg(feature = "login-window")]
        Some(login::ARG) => {
            login::run_window();
            return Ok(());
        }
        _ => {}
    }
    let options = eframe::NativeOptions {
        viewport: {
            let v = egui::ViewportBuilder::default()
                .with_title("dust")
                .with_inner_size([1180.0, 760.0])
                .with_min_inner_size([760.0, 480.0]);
            // Content under a transparent title bar, traffic lights floating over the sidebar.
            #[cfg(target_os = "macos")]
            let v = v.with_fullsize_content_view(true).with_title_shown(false).with_titlebar_shown(false);
            v
        },
        ..Default::default()
    };
    eframe::run_native("dust", options, Box::new(|cc| Ok(Box::new(ui::App::new(cc)))))
}
