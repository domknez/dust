//! Finding AirPlay speakers on the local network (mDNS `_raop._tcp`).

use mdns_sd::{ServiceDaemon, ServiceEvent, ServiceInfo};
use std::net::{IpAddr, SocketAddr};
use std::sync::{Arc, Mutex};

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
    fn from_service(info: &ServiceInfo, local_ips: &[IpAddr]) -> Option<Device> {
        let mut addrs: Vec<SocketAddr> = info.get_addresses_v4().into_iter().map(|ip| SocketAddr::new(IpAddr::V4(*ip), info.get_port())).collect();
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
    _daemon: ServiceDaemon,
}

impl Discovery {
    /// Start browsing; `on_change` runs (on a background thread) when the list changes.
    pub fn start(on_change: impl Fn() + Send + 'static) -> Result<Self, String> {
        let daemon = ServiceDaemon::new().map_err(|e| e.to_string())?;
        let events = daemon.browse(SERVICE).map_err(|e| e.to_string())?;
        let devices = Arc::new(Mutex::new(Vec::<Device>::new()));
        let list = devices.clone();
        let local_ips: Vec<IpAddr> = if_addrs::get_if_addrs().map(|v| v.into_iter().map(|i| i.ip()).collect()).unwrap_or_default();
        std::thread::Builder::new()
            .name("airplay-discovery".into())
            .spawn(move || {
                while let Ok(event) = events.recv() {
                    let changed = match event {
                        ServiceEvent::ServiceResolved(info) => match Device::from_service(&info, &local_ips) {
                            Some(device) => upsert(&mut list.lock().unwrap(), device),
                            None => false,
                        },
                        ServiceEvent::ServiceRemoved(_, full) => {
                            list.lock().unwrap().retain(|d| d.id != full);
                            true
                        }
                        _ => false,
                    };
                    if changed {
                        on_change();
                    }
                }
            })
            .map_err(|e| e.to_string())?;
        Ok(Self { devices, _daemon: daemon })
    }

    pub fn devices(&self) -> Vec<Device> {
        self.devices.lock().unwrap().clone()
    }
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
