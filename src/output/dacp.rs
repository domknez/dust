//! DACP remote-control server. AirPlay receivers send their hardware button presses
//! (volume, play/pause, skip) back to the sender: they look up
//! `iTunes_Ctrl_<DACP-ID>._dacp._tcp` via mDNS and issue plain HTTP requests such as
//! `GET /ctrl-int/1/setproperty?dmcp.device-volume=-12.5`.

use mdns_sd::{ServiceDaemon, ServiceInfo};
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::OnceLock;
use std::time::Duration;

/// Process-wide DACP id, sent to receivers as `DACP-ID` / `Client-Instance`.
pub fn id() -> &'static str {
    static ID: OnceLock<String> = OnceLock::new();
    ID.get_or_init(|| format!("{:016X}", fastrand::u64(..)))
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Remote {
    /// 0.0 ..= 1.0
    Volume(f32),
    VolumeStep(f32),
    PlayPause,
    Play,
    Pause,
    Next,
    Prev,
}

/// AirPlay volume in dB (-30..=0, -144 = mute) to 0..=1.
pub fn volume_from_db(db: f32) -> f32 {
    if db <= -30.0 { 0.0 } else { ((db + 30.0) / 30.0).clamp(0.0, 1.0) }
}

fn parse(path: &str) -> Option<Remote> {
    let cmd = path.strip_prefix("/ctrl-int/1/")?;
    let (name, query) = cmd.split_once('?').unwrap_or((cmd, ""));
    Some(match name {
        "setproperty" => {
            let db = query.split('&').find_map(|kv| kv.strip_prefix("dmcp.device-volume="))?.parse::<f32>().ok()?;
            Remote::Volume(volume_from_db(db))
        }
        "volumeup" => Remote::VolumeStep(1.0 / 16.0),
        "volumedown" => Remote::VolumeStep(-1.0 / 16.0),
        "playpause" => Remote::PlayPause,
        "play" | "playresume" => Remote::Play,
        "pause" | "stop" => Remote::Pause,
        "nextitem" | "beginff" => Remote::Next,
        "previtem" | "beginrew" => Remote::Prev,
        _ => return None,
    })
}

pub struct Server {
    _daemon: ServiceDaemon,
}

/// Start listening and advertise ourselves. `on_command` runs on the server thread.
pub fn start(on_command: impl Fn(Remote) + Send + 'static) -> Result<Server, String> {
    let listener = TcpListener::bind("0.0.0.0:0").map_err(|e| e.to_string())?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    let daemon = ServiceDaemon::new().map_err(|e| e.to_string())?;
    let host = format!("dust-{}.local.", &id()[..8].to_ascii_lowercase());
    let props = [("txtvers", "1"), ("Ver", "131075"), ("DbId", id()), ("OSsi", "0x1F5")];
    let info = ServiceInfo::new("_dacp._tcp.local.", &format!("iTunes_Ctrl_{}", id()), &host, "", port, &props[..])
        .map_err(|e| e.to_string())?
        .enable_addr_auto();
    daemon.register(info).map_err(|e| e.to_string())?;

    std::thread::Builder::new()
        .name("dacp".into())
        .spawn(move || {
            for stream in listener.incoming().flatten() {
                handle(stream, &on_command);
            }
        })
        .map_err(|e| e.to_string())?;
    Ok(Server { _daemon: daemon })
}

fn handle(stream: TcpStream, on_command: &impl Fn(Remote)) {
    stream.set_read_timeout(Some(Duration::from_secs(5))).ok();
    let Ok(read) = stream.try_clone() else { return };
    let mut reader = BufReader::new(read);
    let mut writer = stream;
    // Keep-alive: receivers may send several requests on one connection.
    loop {
        let mut request = String::new();
        if reader.read_line(&mut request).unwrap_or(0) == 0 {
            return;
        }
        let mut line = String::new();
        while reader.read_line(&mut line).unwrap_or(0) > 0 && line.trim_end().len() > 0 {
            line.clear();
        }
        let path = request.split_whitespace().nth(1).unwrap_or("");
        let command = parse(path);
        if std::env::var_os("DUST_DEBUG").is_some() {
            eprintln!("dacp: {path} -> {command:?}");
        }
        if let Some(c) = command {
            on_command(c);
        }
        let status = if command.is_some() { "204 No Content" } else { "404 Not Found" };
        let resp = format!("HTTP/1.1 {status}\r\nDAAP-Server: dust\r\nContent-Length: 0\r\n\r\n");
        if writer.write_all(resp.as_bytes()).is_err() {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_receiver_requests() {
        assert_eq!(parse("/ctrl-int/1/setproperty?dmcp.device-volume=-15.000000"), Some(Remote::Volume(0.5)));
        assert_eq!(parse("/ctrl-int/1/setproperty?dmcp.device-volume=-144.0"), Some(Remote::Volume(0.0)));
        assert_eq!(parse("/ctrl-int/1/setproperty?dmcp.device-volume=0"), Some(Remote::Volume(1.0)));
        assert_eq!(parse("/ctrl-int/1/nextitem"), Some(Remote::Next));
        assert_eq!(parse("/ctrl-int/1/playpause"), Some(Remote::PlayPause));
        assert_eq!(parse("/login"), None);
    }
}
