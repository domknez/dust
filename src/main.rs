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

fn main() -> eframe::Result {
    let mut args = std::env::args().skip(1);
    match args.next().as_deref() {
        Some("--tone") => {
            tone(args.next());
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
