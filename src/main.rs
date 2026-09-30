#![cfg_attr(windows, windows_subsystem = "windows")]
// Fixed-size chunk loops read clearer than as_chunks here.
#![allow(clippy::chunks_exact_to_as_chunks)]

//! dust — a free and open source, lightweight Deezer client with AirPlay output.

mod cli;
mod credentials;
mod deezer;
mod icon;
#[cfg(feature = "login-window")]
mod login;
mod output;
mod player;
mod settings;
mod ui;

fn main() -> eframe::Result {
    match cli::Command::parse(std::env::args().skip(1)) {
        Some(command) => {
            command.run();
            Ok(())
        }
        None => ui::run(),
    }
}
