//! Finding AirPlay speakers on the local network (mDNS `_raop._tcp`).

use mdns_sd::{ResolvedService, ServiceDaemon, ServiceEvent};
use std::net::{IpAddr, SocketAddr};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

const SERVICE: &str = "_raop._tcp.local.";

// Bits of the `ft` (features) TXT record.
const FEATURE_AUDIO: u64 = 1 << 9;
const FEATURE_PTP: u64 = 1 << 41;
const FEATURE_COREUTILS_PAIRING: u64 = 1 << 48;

#[derive(Clone, Debug, PartialEq)]
pub struct Device {
    pub id: String,
    pub name: String,
    /// Every advertised IPv4 address; multi-homed hosts list bridges/VPNs too.
    pub addrs: Vec<SocketAddr>,
    /// We can stream to it (AirPlay 2, or AirPlay 1 accepting unencrypted audio).
    pub supported: bool,
    pub password: bool,
    /// Receiver expects the MFi-SAP `auth-setup` handshake (et=4).
    pub auth_setup: bool,
    /// Speak AirPlay 2 (transient pairing + encryption) instead of RAOP.
    pub ap2: bool,
    /// Receiver supports PTP timing (required for audio by most AirPlay 2 devices).
    pub ptp: bool,
}

impl Device {
    fn from_service(info: &ResolvedService, local_ips: &[IpAddr]) -> Option<Device> {
        let mut addrs: Vec<SocketAddr> =
            info.get_addresses_v4().into_iter().map(|ip| SocketAddr::new(IpAddr::V4(ip), info.get_port())).collect();
        // Skip ourselves (e.g. macOS "AirPlay Receiver" on this machine).
        if addrs.is_empty() || addrs.iter().any(|a| local_ips.contains(&a.ip())) {
            return None;
        }
        // Likely-reachable addresses first: not loopback, not bridge/VPN ".0" hosts.
        addrs.sort_by_key(|a| (a.ip().is_loopback(), a.ip().to_string().ends_with(".0"), *a));

        let full = info.get_fullname().to_string();
        let label = full.split("._raop").next().unwrap_or(&full);
        let name = label.split_once('@').map_or(label, |(_, n)| n).to_string();
        let encryption = info.get_property_val_str("et").unwrap_or("0");
        let has_encryption = |kind: &str| encryption.split(',').any(|e| e.trim() == kind);
        let features = parse_features(info.get_property_val_str("ft").or(info.get_property_val_str("sf")).unwrap_or("0"));
        let ap2 = features & FEATURE_AUDIO != 0 && features & FEATURE_COREUTILS_PAIRING != 0;
        Some(Device {
            name,
            addrs,
            ap2,
            ptp: features & FEATURE_PTP != 0,
            supported: ap2 || has_encryption("0"),
            auth_setup: has_encryption("4"),
            password: info.get_property_val_str("pw").is_some_and(|p| p == "true"),
            id: full,
        })
    }
}

/// `ft`/`features` TXT value: "0xLOW" or "0xLOW,0xHIGH".
fn parse_features(v: &str) -> u64 {
    let mut parts = v.split(',').map(|p| u64::from_str_radix(p.trim().trim_start_matches("0x").trim_start_matches("0X"), 16).unwrap_or(0));
    let lo = parts.next().unwrap_or(0);
    let hi = parts.next().unwrap_or(0);
    (hi << 32) | (lo & 0xffff_ffff)
}

/// Keeps an up-to-date, name-sorted list of speakers in the background.
pub struct Discovery {
    devices: Arc<Mutex<Vec<Device>>>,
    refresh: Sender<()>,
    _daemon: ServiceDaemon,
}

/// How often the browse thread checks for sleep and network changes.
const WATCH_INTERVAL: Duration = Duration::from_secs(2);
/// Wall-clock time passing this much faster than the process's own clock means the
/// machine slept (the monotonic clock stops during sleep).
const SLEEP_GAP: Duration = Duration::from_secs(10);

impl Discovery {
    /// Start browsing; `on_change` runs (on a background thread) when the list changes.
    ///
    /// mDNS re-asks the network with growing pauses (up to an hour), and speakers
    /// only announce themselves when something changes on their side. So after sleep
    /// or a network change, when the cached speakers have expired, a speaker could
    /// stay missing for a long time: the search restarts then, and on [`refresh`].
    ///
    /// [`refresh`]: Discovery::refresh
    pub fn start(on_change: impl Fn() + Send + 'static) -> Result<Self, String> {
        let daemon = ServiceDaemon::new().map_err(|e| e.to_string())?;
        let mut events = daemon.browse(SERVICE).map_err(|e| e.to_string())?;
        let devices = Arc::new(Mutex::new(Vec::<Device>::new()));
        let (refresh, refresh_rx) = mpsc::channel::<()>();
        let (list, browser) = (devices.clone(), daemon.clone());
        std::thread::Builder::new()
            .name("airplay-discovery".into())
            .spawn(move || {
                let mut own_ips = local_ips();
                let mut watch = Watch::new();
                loop {
                    let mut lost = false;
                    let changed = match events.recv_timeout(WATCH_INTERVAL) {
                        Ok(ServiceEvent::ServiceResolved(info)) => match Device::from_service(&info, &own_ips) {
                            Some(device) => upsert(&mut list.lock().unwrap(), device),
                            None => false,
                        },
                        Ok(ServiceEvent::ServiceRemoved(_, full)) => {
                            list.lock().unwrap().retain(|d| d.id != full);
                            true
                        }
                        Ok(_) => false,
                        Err(mdns_sd::RecvTimeoutError::Disconnected) => {
                            // The browse ended (daemon hiccup): pause, then start over.
                            std::thread::sleep(WATCH_INTERVAL);
                            lost = true;
                            false
                        }
                        Err(_) => false,
                    };
                    if changed {
                        on_change();
                    }
                    let asked = refresh_rx.try_recv().is_ok();
                    if let Some(reason) = watch.check(asked).or(lost.then_some("browse ended")) {
                        log_debug!("airplay discovery: searching again ({reason})");
                        own_ips = local_ips();
                        let _ = browser.stop_browse(SERVICE);
                        match browser.browse(SERVICE) {
                            Ok(fresh) => events = fresh,
                            Err(e) => log_warn!("airplay discovery: {e}"),
                        }
                    }
                }
            })
            .map_err(|e| e.to_string())?;
        Ok(Self { devices, refresh, _daemon: daemon })
    }

    pub fn devices(&self) -> Vec<Device> {
        self.devices.lock().unwrap().clone()
    }

    /// Search again now (e.g. the speaker menu was opened), instead of waiting for
    /// mDNS's next, possibly much later, query.
    pub fn refresh(&self) {
        let _ = self.refresh.send(());
    }
}

fn local_ips() -> Vec<IpAddr> {
    if_addrs::get_if_addrs().map(|v| v.into_iter().map(|i| i.ip()).collect()).unwrap_or_default()
}

/// Notices when the search should start over: the machine woke from sleep, its
/// network addresses changed, or someone asked (at most every few seconds).
struct Watch {
    clock: (Instant, SystemTime),
    addresses: Vec<IpAddr>,
    last_search: Instant,
}

impl Watch {
    fn new() -> Self {
        Self { clock: (Instant::now(), SystemTime::now()), addresses: sorted_ips(), last_search: Instant::now() }
    }

    fn check(&mut self, asked: bool) -> Option<&'static str> {
        let (mono, wall) = (self.clock.0.elapsed(), self.clock.1.elapsed().unwrap_or_default());
        let slept = wall > mono + SLEEP_GAP;
        let mut reason = slept.then_some("woke from sleep");
        if mono >= WATCH_INTERVAL || slept {
            self.clock = (Instant::now(), SystemTime::now());
            let addresses = sorted_ips();
            if addresses != self.addresses {
                self.addresses = addresses;
                reason = reason.or(Some("network changed"));
            }
        }
        // A menu opened twice in a row shouldn't restart the search twice.
        if asked && self.last_search.elapsed() > Duration::from_secs(3) {
            reason = reason.or(Some("asked"));
        }
        if reason.is_some() {
            self.last_search = Instant::now();
        }
        reason
    }
}

fn sorted_ips() -> Vec<IpAddr> {
    let mut ips = local_ips();
    ips.sort();
    ips
}

/// Insert or update `device`, keeping the list sorted. Returns whether it changed.
fn upsert(list: &mut Vec<Device>, device: Device) -> bool {
    match list.iter_mut().find(|d| d.id == device.id) {
        Some(existing) if *existing == device => return false,
        Some(existing) => *existing = device,
        None => list.push(device),
    }
    list.sort_by(|a, b| a.name.cmp(&b.name));
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn features_from_txt() {
        // Sonos Era 100: AirPlay audio, PTP, CoreUtils pairing.
        let f = parse_features("0x445F8A00,0x801C340");
        assert!(f & FEATURE_AUDIO != 0 && f & FEATURE_PTP != 0 && f & FEATURE_COREUTILS_PAIRING != 0);
        assert_eq!(parse_features("0x10"), 0x10);
        assert_eq!(parse_features("junk"), 0);
    }
}
