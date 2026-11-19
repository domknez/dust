//! DACP remote-control server. AirPlay receivers send their hardware button presses
//! (volume, play/pause, skip) back to the sender: they look up
//! `iTunes_Ctrl_<DACP-ID>._dacp._tcp` via mDNS and issue plain HTTP requests such as
//! `GET /ctrl-int/1/setproperty?dmcp.device-volume=-12.5`. Each request carries
//! the `Active-Remote` id we gave that receiver's session; others are refused, so
//! another computer on the network can't drive the player.

use mdns_sd::{IfKind, ServiceDaemon, ServiceInfo};
use std::io::{BufRead, BufReader, Write};
use std::net::{IpAddr, TcpListener, TcpStream};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

/// Process-wide DACP id, sent to receivers as `DACP-ID` / `Client-Instance`.
pub fn id() -> &'static str {
    static ID: OnceLock<String> = OnceLock::new();
    ID.get_or_init(|| format!("{:016X}", fastrand::u64(..)))
}

/// The `Active-Remote` id of a live AirPlay session: requests with it are taken
/// while it's kept, none after it's dropped.
pub struct ActiveRemote(String);

static ACTIVE_REMOTES: Mutex<Vec<String>> = Mutex::new(Vec::new());

impl ActiveRemote {
    pub fn new() -> Self {
        // From the OS's RNG, as it's what lets a request in. 32 bits: receivers keep
        // it as one (a 64-bit id came back cut, so every button press was refused).
        let mut bytes = [0u8; 4];
        getrandom::fill(&mut bytes).expect("OS RNG");
        let id = u32::from_le_bytes(bytes).to_string();
        ACTIVE_REMOTES.lock().unwrap_or_else(|e| e.into_inner()).push(id.clone());
        ActiveRemote(id)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Drop for ActiveRemote {
    fn drop(&mut self) {
        ACTIVE_REMOTES.lock().unwrap_or_else(|e| e.into_inner()).retain(|id| *id != self.0);
    }
}

fn is_active(id: &str) -> bool {
    ACTIVE_REMOTES.lock().unwrap_or_else(|e| e.into_inner()).iter().any(|a| a == id)
}

/// How long a receiver's connection may sit idle.
const IDLE: Duration = Duration::from_secs(600);
/// Volume change of one press of a receiver's volume button.
const VOLUME_STEP: f32 = 1.0 / 16.0;

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

fn parse(path: &str) -> Option<Remote> {
    let cmd = path.strip_prefix("/ctrl-int/1/")?;
    let (name, query) = cmd.split_once('?').unwrap_or((cmd, ""));
    Some(match name {
        "setproperty" => {
            let db = query.split('&').find_map(|kv| kv.strip_prefix("dmcp.device-volume="))?.parse::<f32>().ok()?;
            Remote::Volume(super::volume::from_db(db))
        }
        "volumeup" => Remote::VolumeStep(VOLUME_STEP),
        "volumedown" => Remote::VolumeStep(-VOLUME_STEP),
        "playpause" => Remote::PlayPause,
        "play" | "playresume" => Remote::Play,
        "pause" | "stop" => Remote::Pause,
        "nextitem" | "beginff" => Remote::Next,
        "previtem" | "beginrew" => Remote::Prev,
        _ => return None,
    })
}

/// Where button presses go: set by [`start`], also fed by the AirPlay 2 event channel.
type Handler = Box<dyn Fn(Remote) + Send>;
static HANDLER: Mutex<Option<Handler>> = Mutex::new(None);

/// Hand a button press to the app (no-op before [`start`]).
pub fn deliver(remote: Remote) {
    if let Some(handler) = HANDLER.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
        handler(remote);
    }
}

/// Our mDNS advertisement, shared so a new AirPlay session can narrow it down.
struct Advert {
    daemon: ServiceDaemon,
    port: u16,
    fullname: String,
}

static ADVERT: Mutex<Option<Advert>> = Mutex::new(None);

impl Advert {
    fn register(&mut self, ip: IpAddr) -> Result<(), String> {
        let host = format!("dust-{}.local.", id()[..8].to_ascii_lowercase());
        let props = [("txtvers", "1"), ("Ver", "131075"), ("DbId", id()), ("OSsi", "0x1F5")];
        let name = format!("iTunes_Ctrl_{}", id());
        let info = ServiceInfo::new("_dacp._tcp.local.", &name, &host, ip, self.port, &props[..]).map_err(|e| e.to_string())?;
        self.fullname = info.get_fullname().to_string();
        self.daemon.register(info).map_err(|e| e.to_string())
    }

    fn unregister(&self) {
        if let Ok(done) = self.daemon.unregister(&self.fullname) {
            let _ = done.recv_timeout(Duration::from_millis(500));
        }
    }
}

/// Advertise the remote-control service only on the interface that reaches the
/// receiver we just connected to, with just that address, and announce it now.
/// Advertising every interface (VPNs, container bridges, loopback) made receivers
/// such as Sonos take ~25 s to reach us, so button presses arrived in late bursts.
pub fn advertise_on(local_ip: IpAddr) {
    let mut advert = ADVERT.lock().unwrap_or_else(|e| e.into_inner());
    let Some(advert) = advert.as_mut() else { return };
    let _ = advert.daemon.disable_interface(IfKind::All);
    let _ = advert.daemon.enable_interface(IfKind::Addr(local_ip));
    if !advert.fullname.is_empty() {
        advert.unregister();
    }
    match advert.register(local_ip) {
        Ok(()) => log_debug!("dacp: advertised on {local_ip}:{}", advert.port),
        Err(e) => log_warn!("dacp: re-advertising failed: {e}"),
    }
}

/// Keeps the remote-control service running; withdraws its mDNS record on drop,
/// so receivers don't keep a dead remote around.
pub struct Server;

impl Drop for Server {
    fn drop(&mut self) {
        if let Some(advert) = ADVERT.lock().unwrap_or_else(|e| e.into_inner()).take() {
            if !advert.fullname.is_empty() {
                advert.unregister();
            }
            let _ = advert.daemon.shutdown();
        }
    }
}

/// Start listening and advertise ourselves. `on_command` runs on the server thread.
pub fn start(on_command: impl Fn(Remote) + Send + 'static) -> Result<Server, String> {
    *HANDLER.lock().unwrap_or_else(|e| e.into_inner()) = Some(Box::new(on_command));
    let listener = TcpListener::bind("0.0.0.0:0").map_err(|e| e.to_string())?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    let daemon = ServiceDaemon::new().map_err(|e| e.to_string())?;
    // Not advertised until an AirPlay session starts (see `advertise_on`): an
    // all-interfaces record would be cached by receivers, which then try its
    // unreachable addresses first.
    let advert = Advert { daemon, port, fullname: String::new() };
    *ADVERT.lock().unwrap_or_else(|e| e.into_inner()) = Some(advert);

    std::thread::Builder::new()
        .name("dacp".into())
        .spawn(move || {
            // A connection per receiver, kept open: they reuse it for later presses.
            for stream in listener.incoming().flatten() {
                log_debug!("dacp: connection from {:?}", stream.peer_addr().ok());
                let _ = std::thread::Builder::new().name("dacp-conn".into()).spawn(move || handle(stream));
            }
        })
        .map_err(|e| e.to_string())?;
    Ok(Server)
}

fn handle(stream: TcpStream) {
    stream.set_read_timeout(Some(IDLE)).ok();
    let Ok(read) = stream.try_clone() else { return };
    let mut reader = BufReader::new(read);
    let mut writer = stream;
    // Keep-alive: receivers may send several requests on one connection.
    while let Some((path, remote)) = read_request(&mut reader) {
        let command = parse(&path);
        let allowed = remote.as_deref().is_some_and(is_active);
        log_debug!("dacp: {path} -> {command:?}{}", if allowed { "" } else { " (no live session: refused)" });
        let status = match command {
            Some(c) if allowed => {
                deliver(c);
                "204 No Content"
            }
            Some(_) => "403 Forbidden",
            None => "404 Not Found",
        };
        let resp = format!("HTTP/1.1 {status}\r\nDAAP-Server: dust\r\nContent-Length: 0\r\n\r\n");
        if writer.write_all(resp.as_bytes()).is_err() {
            return;
        }
    }
}

/// The next request's path and `Active-Remote` header; None once the connection
/// closes or sends more than a request may hold.
fn read_request(reader: &mut impl BufRead) -> Option<(String, Option<String>)> {
    let head = super::head::read(reader).ok()?;
    let path = head.first.split_whitespace().nth(1).unwrap_or("").to_string();
    let remote = head.header("Active-Remote").map(str::to_string);
    Some((path, remote))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn active_remote_ids_fit_in_32_bits() {
        let remote = ActiveRemote::new();
        assert!(remote.as_str().parse::<u32>().is_ok(), "{}", remote.as_str());
        assert!(is_active(remote.as_str()));
        let id = remote.as_str().to_string();
        drop(remote);
        assert!(!is_active(&id));
    }

    #[test]
    fn parses_receiver_requests() {
        assert_eq!(parse("/ctrl-int/1/setproperty?dmcp.device-volume=-15.000000"), Some(Remote::Volume(0.5)));
        assert_eq!(parse("/ctrl-int/1/setproperty?dmcp.device-volume=-144.0"), Some(Remote::Volume(0.0)));
        assert_eq!(parse("/ctrl-int/1/setproperty?dmcp.device-volume=0"), Some(Remote::Volume(1.0)));
        assert_eq!(parse("/ctrl-int/1/nextitem"), Some(Remote::Next));
        assert_eq!(parse("/ctrl-int/1/playpause"), Some(Remote::PlayPause));
        assert_eq!(parse("/login"), None);
    }

    #[test]
    fn only_live_sessions_are_heard() {
        let request = |remote: &str| format!("GET /ctrl-int/1/nextitem HTTP/1.1\r\nHost: x\r\nActive-Remote: {remote}\r\n\r\n");
        let session = ActiveRemote::new();
        let read = |text: String| read_request(&mut std::io::Cursor::new(text.into_bytes()));
        let (path, remote) = read(request(session.as_str())).unwrap();
        assert_eq!(path, "/ctrl-int/1/nextitem");
        assert!(remote.as_deref().is_some_and(is_active));
        let id = session.as_str().to_string();
        drop(session);
        assert!(!is_active(&id), "gone with its session");
        let headers = "X: y\r\n".repeat(super::super::head::MAX_HEADERS + 1);
        assert_eq!(read(format!("GET / HTTP/1.1\r\n{headers}\r\n")), None, "too many headers");
    }
}
