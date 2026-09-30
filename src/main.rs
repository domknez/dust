#![cfg_attr(windows, windows_subsystem = "windows")]

mod deezer;
mod output;
mod player;
mod ui;

use eframe::egui;

/// `dust --tone [airplay device name]`: play a short 440 Hz tone to check an output.
fn tone(target: Option<String>) {
    use output::Sink;
    use std::time::{Duration, Instant};
    let mut sink: Box<dyn Sink> = match target {
        None => Box::new(output::local::LocalSink::new(0.5).expect("local output")),
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
    println!("done");
}

fn main() -> eframe::Result {
    let mut args = std::env::args().skip(1);
    if args.next().as_deref() == Some("--tone") {
        tone(args.next());
        return Ok(());
    }
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("dust")
            .with_inner_size([960.0, 620.0])
            .with_min_inner_size([560.0, 360.0]),
        ..Default::default()
    };
    eframe::run_native("dust", options, Box::new(|cc| Ok(Box::new(ui::App::new(cc)))))
}
