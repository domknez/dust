//! AirPlay sender: RTSP control, RTP/UDP audio as uncompressed ALAC, NTP-style
//! timing replies, sync packets and retransmits.
//!
//! Two handshakes share that streaming core:
//! - AirPlay 2 (devices with CoreUtils pairing, e.g. Sonos Era, HomePod): transient
//!   HomeKit pairing, encrypted RTSP, binary-plist SETUP, ChaCha20-Poly1305 audio.
//! - AirPlay 1 / RAOP with unencrypted audio (`et=0`): AirPort Express, older
//!   speakers, shairport-sync classic.

use super::ap2::bplist::{self, Value, dict};
use super::ap2::pairing::{self, DecryptReader, Encryptor, SrpClient, tlv};
use super::ap2::ptp::{self, PtpPeer};
use super::{RATE, Sink};
use chacha20poly1305::ChaCha20Poly1305;
use chacha20poly1305::aead::KeyInit;
use mdns_sd::{ServiceDaemon, ServiceEvent};
use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{IpAddr, SocketAddr, TcpStream, UdpSocket};
use std::sync::atomic::{AtomicBool, AtomicU16, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const FRAMES_PER_PACKET: usize = 352;
/// Receiver-side buffer we ask for (2 s), expressed in frames.
const LATENCY: u32 = 88_200;
/// How far ahead of real time we push audio.
const LEAD: u64 = RATE as u64 / 10;
const HISTORY: usize = 1024;

// ---------------------------------------------------------------- discovery

#[derive(Clone, Debug, PartialEq)]
pub struct Device {
    pub id: String,
    pub name: String,
    /// Every advertised IPv4 address; multi-homed hosts list bridges/VPNs too.
    pub addrs: Vec<SocketAddr>,
    /// Receiver accepts unencrypted audio.
    pub supported: bool,
    pub password: bool,
    /// Receiver expects the MFi-SAP `auth-setup` handshake (et=4).
    pub auth_setup: bool,
    /// Speak AirPlay 2 (transient pairing + encryption) instead of RAOP.
    pub ap2: bool,
    /// Receiver supports PTP timing (required for audio by most AirPlay 2 devices).
    pub ptp: bool,
}

/// `ft`/`features` TXT value: "0xLOW" or "0xLOW,0xHIGH".
fn parse_features(v: &str) -> u64 {
    let mut parts = v.split(',').map(|p| u64::from_str_radix(p.trim().trim_start_matches("0x").trim_start_matches("0X"), 16).unwrap_or(0));
    let lo = parts.next().unwrap_or(0);
    let hi = parts.next().unwrap_or(0);
    (hi << 32) | (lo & 0xffff_ffff)
}

const FEATURE_AUDIO: u64 = 1 << 9;
const FEATURE_PTP: u64 = 1 << 41;
const FEATURE_COREUTILS_PAIRING: u64 = 1 << 48;

pub struct Discovery {
    devices: Arc<Mutex<Vec<Device>>>,
    _daemon: ServiceDaemon,
}

impl Discovery {
    pub fn start(on_change: impl Fn() + Send + 'static) -> Result<Self, String> {
        let daemon = ServiceDaemon::new().map_err(|e| e.to_string())?;
        let rx = daemon.browse("_raop._tcp.local.").map_err(|e| e.to_string())?;
        let devices = Arc::new(Mutex::new(Vec::<Device>::new()));
        let list = devices.clone();
        let local_ips: Vec<IpAddr> =
            if_addrs::get_if_addrs().map(|v| v.into_iter().map(|i| i.ip()).collect()).unwrap_or_default();
        std::thread::Builder::new()
            .name("airplay-discovery".into())
            .spawn(move || {
                while let Ok(event) = rx.recv() {
                    let mut list = list.lock().unwrap();
                    match event {
                        ServiceEvent::ServiceResolved(info) => {
                            let mut addrs: Vec<SocketAddr> =
                                info.get_addresses_v4().into_iter().map(|ip| SocketAddr::new(IpAddr::V4(*ip), info.get_port())).collect();
                            // Skip ourselves (e.g. macOS "AirPlay Receiver" on this machine).
                            if addrs.is_empty() || addrs.iter().any(|a| local_ips.contains(&a.ip())) {
                                continue;
                            }
                            addrs.sort_by_key(|a| (a.ip().is_loopback(), a.ip().to_string().ends_with(".0"), *a));
                            let full = info.get_fullname().to_string();
                            let label = full.split("._raop").next().unwrap_or(&full);
                            let name = label.split_once('@').map_or(label, |(_, n)| n).to_string();
                            let et = info.get_property_val_str("et").unwrap_or("0");
                            let ft = parse_features(info.get_property_val_str("ft").or(info.get_property_val_str("sf")).unwrap_or("0"));
                            let ap2 = ft & FEATURE_AUDIO != 0 && ft & FEATURE_COREUTILS_PAIRING != 0;
                            let device = Device {
                                name,
                                addrs,
                                ap2,
                                ptp: ft & FEATURE_PTP != 0,
                                supported: ap2 || et.split(',').any(|e| e.trim() == "0"),
                                auth_setup: et.split(',').any(|e| e.trim() == "4"),
                                password: info.get_property_val_str("pw").is_some_and(|p| p == "true"),
                                id: full,
                            };
                            match list.iter_mut().find(|d| d.id == device.id) {
                                Some(d) if *d == device => continue,
                                Some(d) => *d = device,
                                None => list.push(device),
                            }
                            list.sort_by(|a, b| a.name.cmp(&b.name));
                        }
                        ServiceEvent::ServiceRemoved(_, full) => list.retain(|d| d.id != full),
                        _ => continue,
                    }
                    drop(list);
                    on_change();
                }
            })
            .map_err(|e| e.to_string())?;
        Ok(Self { devices, _daemon: daemon })
    }

    pub fn devices(&self) -> Vec<Device> {
        self.devices.lock().unwrap().clone()
    }
}

// ---------------------------------------------------------------- RTSP

struct Rtsp {
    writer: TcpStream,
    /// Set once AirPlay 2 pairing completes; everything after is encrypted.
    encryptor: Option<Encryptor>,
    reader: BufReader<DecryptReader<TcpStream>>,
    user_agent: &'static str,
    cseq: u32,
    url: String,
    session: Option<String>,
    instance: String,
    active_remote: String,
}

struct Response {
    status: u16,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

impl Response {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
    }
}

impl Rtsp {
    fn request(&mut self, method: &str, uri: Option<&str>, headers: &[(&str, String)], body: Option<(&str, &[u8])>) -> Result<Response, String> {
        self.cseq += 1;
        let mut req = format!(
            "{method} {} RTSP/1.0\r\nCSeq: {}\r\nUser-Agent: {}\r\nClient-Instance: {}\r\nDACP-ID: {}\r\nActive-Remote: {}\r\n",
            uri.unwrap_or(&self.url),
            self.cseq,
            self.user_agent,
            self.instance,
            self.instance,
            self.active_remote
        );
        if let Some(s) = &self.session {
            req += &format!("Session: {s}\r\n");
        }
        for (k, v) in headers {
            req += &format!("{k}: {v}\r\n");
        }
        if let Some((ct, b)) = body {
            req += &format!("Content-Type: {ct}\r\nContent-Length: {}\r\n", b.len());
        }
        req += "\r\n";
        let mut bytes = req.into_bytes();
        if let Some((_, b)) = body {
            bytes.extend_from_slice(b);
        }
        if let Some(e) = self.encryptor.as_mut() {
            bytes = e.seal(&bytes);
        }
        self.writer.write_all(&bytes).map_err(|e| format!("{method}: {e}"))?;

        let mut line = String::new();
        self.reader.read_line(&mut line).map_err(|e| format!("{method}: {e}"))?;
        let status = line.split_whitespace().nth(1).and_then(|s| s.parse().ok()).ok_or_else(|| format!("{method}: bad response {line:?}"))?;
        let mut headers = Vec::new();
        loop {
            line.clear();
            self.reader.read_line(&mut line).map_err(|e| format!("{method}: {e}"))?;
            let l = line.trim_end();
            if l.is_empty() {
                break;
            }
            if let Some((k, v)) = l.split_once(':') {
                headers.push((k.trim().to_string(), v.trim().to_string()));
            }
        }
        let mut resp = Response { status, headers, body: Vec::new() };
        if let Some(len) = resp.header("Content-Length").and_then(|l| l.parse::<usize>().ok()) {
            resp.body = vec![0; len];
            self.reader.read_exact(&mut resp.body).map_err(|e| format!("{method}: {e}"))?;
        }
        match resp.status {
            200 => Ok(resp),
            401 => Err("AirPlay device requires a password (not supported)".into()),
            470 => Err("AirPlay device requires PIN pairing (not supported)".into()),
            s => Err(format!("{method} failed: RTSP {s}")),
        }
    }
}

fn transport_param(t: &str, key: &str) -> Option<u16> {
    t.split(';').find_map(|p| p.strip_prefix(key)?.strip_prefix('=')?.parse().ok())
}

// ---------------------------------------------------------------- wire formats

fn ntp_now() -> u64 {
    let d = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
    ((d.as_secs() + 2_208_988_800) << 32) | ((d.subsec_nanos() as u64) << 32) / 1_000_000_000
}

struct BitWriter {
    buf: Vec<u8>,
    bits: u32,
}

impl BitWriter {
    fn put(&mut self, value: u32, n: u32) {
        for i in (0..n).rev() {
            if self.bits % 8 == 0 {
                self.buf.push(0);
            }
            if (value >> i) & 1 == 1 {
                *self.buf.last_mut().unwrap() |= 0x80 >> (self.bits % 8);
            }
            self.bits += 1;
        }
    }
}

/// One ALAC frame using the "escape" (uncompressed) path: CPE header, raw 16-bit
/// interleaved samples, END tag. Every ALAC decoder accepts this.
fn alac_frame(samples: &[i16]) -> Vec<u8> {
    debug_assert_eq!(samples.len(), FRAMES_PER_PACKET * 2);
    let mut w = BitWriter { buf: Vec::with_capacity(FRAMES_PER_PACKET * 4 + 4), bits: 0 };
    w.put(1, 3); // ID_CPE (stereo pair)
    w.put(0, 4); // element instance
    w.put(0, 12); // unused
    w.put(0, 1); // partial frame: no, always 352 frames
    w.put(0, 2); // bytes shifted
    w.put(1, 1); // escape flag: uncompressed
    for &s in samples {
        w.put(s as u16 as u32, 16);
    }
    w.put(7, 3); // ID_END
    w.buf
}

// ---------------------------------------------------------------- sink

struct Shared {
    running: AtomicBool,
    playing: AtomicBool,
    restart: AtomicBool,
    drain: AtomicBool,
    idle: AtomicBool,
    seq: AtomicU16,
    rtptime: AtomicU32,
    history: Mutex<VecDeque<(u16, Vec<u8>)>>,
    // Diagnostics, printed on drop when DUST_DEBUG is set.
    timing_requests: AtomicU64,
    resent: AtomicU64,
    packets: AtomicU64,
    audible: AtomicU64,
}

pub struct AirPlaySink {
    rtsp: Rtsp,
    /// AirPlay 2 reverse event channel; held open for the session.
    _events: Option<TcpStream>,
    /// Keeps our PTP grandmaster talking to this receiver.
    _ptp: Option<PtpPeer>,
    shared: Arc<Shared>,
    producer: rtrb::Producer<i16>,
    threads: Vec<JoinHandle<()>>,
}

impl AirPlaySink {
    pub fn connect(device: &Device, volume: f32) -> Result<Self, String> {
        if !device.supported {
            return Err(format!("{} requires encrypted AirPlay (not supported)", device.name));
        }
        let mut last_err = String::from("no address");
        let stream = device
            .addrs
            .iter()
            .find_map(|a| TcpStream::connect_timeout(a, Duration::from_secs(2)).map_err(|e| last_err = e.to_string()).ok())
            .ok_or_else(|| format!("{}: {last_err}", device.name))?;
        stream.set_read_timeout(Some(Duration::from_secs(10))).ok();
        stream.set_nodelay(true).ok();
        let local_ip = stream.local_addr().map_err(|e| e.to_string())?.ip();
        let remote_ip = stream.peer_addr().map_err(|e| e.to_string())?.ip();
        let sid = fastrand::u32(..);
        let mut rtsp = Rtsp {
            reader: BufReader::new(DecryptReader::new(stream.try_clone().map_err(|e| e.to_string())?)),
            writer: stream,
            encryptor: None,
            user_agent: if device.ap2 { "AirPlay/381.13" } else { "iTunes/11.0.4 (Windows; N)" },
            cseq: 0,
            url: format!("rtsp://{local_ip}/{sid}"),
            session: None,
            instance: format!("{:016X}", fastrand::u64(..)),
            active_remote: fastrand::u32(..).to_string(),
        };

        let bind = |ip: IpAddr| UdpSocket::bind(SocketAddr::new(ip, 0)).map_err(|e| e.to_string());
        let any: IpAddr = if local_ip.is_ipv4() { [0, 0, 0, 0].into() } else { std::net::Ipv6Addr::UNSPECIFIED.into() };
        let control = bind(any)?;
        let timing = bind(any)?;
        let audio = bind(any)?;
        for s in [&control, &timing] {
            s.set_read_timeout(Some(Duration::from_millis(200))).ok();
        }

        let seq: u16 = fastrand::u16(..);
        let rtptime: u32 = fastrand::u32(..);
        let shared = Arc::new(Shared {
            running: AtomicBool::new(true),
            playing: AtomicBool::new(true),
            restart: AtomicBool::new(true),
            drain: AtomicBool::new(false),
            idle: AtomicBool::new(false),
            seq: AtomicU16::new(seq),
            rtptime: AtomicU32::new(rtptime),
            history: Mutex::new(VecDeque::with_capacity(HISTORY)),
            timing_requests: AtomicU64::new(0),
            resent: AtomicU64::new(0),
            packets: AtomicU64::new(0),
            audible: AtomicU64::new(0),
        });
        // Receivers sync clocks with us during SETUP, so answer timing requests first.
        let sh = shared.clone();
        let timing_port = timing.local_addr().map_err(|e| e.to_string())?.port();
        let mut threads = vec![spawn("airplay-timing", move || timing_loop(timing, sh))];

        let control_port_local = control.local_addr().map_err(|e| e.to_string())?.port();
        let negotiated = if device.ap2 {
            negotiate_ap2(&mut rtsp, device, local_ip, remote_ip, control_port_local, timing_port)
        } else {
            let ports = Ports { control: control_port_local, timing: timing_port };
            negotiate_ap1(&mut rtsp, device, sid, local_ip, remote_ip, ports, seq, rtptime).map(|(s, c)| Negotiated {
                server_port: s,
                control_port: c,
                audio_key: None,
                events: None,
                ptp: None,
            })
        };
        let Negotiated { server_port, control_port, audio_key, events, ptp } = match negotiated {
            Ok(n) => n,
            Err(e) => {
                shared.running.store(false, Ordering::Release);
                return Err(format!("{}: {e}", device.name));
            }
        };

        audio.connect(SocketAddr::new(remote_ip, server_port)).map_err(|e| e.to_string())?;
        let control_dst = SocketAddr::new(remote_ip, control_port);
        // ~1 s of audio between player and network pacing.
        let (producer, consumer) = rtrb::RingBuffer::new(RATE as usize * 2);
        let sh = shared.clone();
        let ctl = control.try_clone().map_err(|e| e.to_string())?;
        threads.push(spawn("airplay-control", move || control_loop(ctl, control_dst, sh)));
        let sh = shared.clone();
        let cipher = audio_key.map(|k| ChaCha20Poly1305::new(&k.into()));
        let clock_id = ptp.as_ref().map(PtpPeer::clock_id);
        threads.push(spawn("airplay-audio", move || audio_loop(audio, control, control_dst, consumer, cipher, clock_id, sh)));

        let mut sink = Self { rtsp, _events: events, _ptp: ptp, shared, producer, threads };
        sink.set_volume(volume);
        Ok(sink)
    }

    /// Stop the audio thread and drop the receiver's buffer.
    fn halt(&mut self) {
        self.shared.playing.store(false, Ordering::Release);
        let deadline = Instant::now() + Duration::from_millis(200);
        while !self.shared.idle.load(Ordering::Acquire) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(1));
        }
        let seq = self.shared.seq.load(Ordering::Acquire);
        let rtptime = self.shared.rtptime.load(Ordering::Acquire);
        let _ = self.rtsp.request("FLUSH", None, &[("RTP-Info", format!("seq={seq};rtptime={rtptime}"))], None);
    }
}

struct Ports {
    control: u16,
    timing: u16,
}

struct Negotiated {
    server_port: u16,
    control_port: u16,
    /// AirPlay 2 per-packet audio key (`shk`).
    audio_key: Option<[u8; 32]>,
    events: Option<TcpStream>,
    ptp: Option<PtpPeer>,
}

#[allow(clippy::too_many_arguments)]
fn negotiate_ap1(
    rtsp: &mut Rtsp,
    device: &Device,
    sid: u32,
    local_ip: IpAddr,
    remote_ip: IpAddr,
    ports: Ports,
    seq: u16,
    rtptime: u32,
) -> Result<(u16, u16), String> {
    rtsp.request("OPTIONS", Some("*"), &[], None)?;
    if device.auth_setup {
        // MFi-SAP handshake: type byte 0x01 + a Curve25519 public key. Receivers
        // only need it to happen; the reply (their key + cert) is not used for RAOP.
        let mut body = vec![0x01];
        body.extend((0..32).map(|_| fastrand::u8(..)));
        rtsp.request("POST", Some("/auth-setup"), &[], Some(("application/octet-stream", &body)))?;
    }
    let ip4 = |ip: IpAddr| if ip.is_ipv4() { "IP4" } else { "IP6" };
    let sdp = format!(
        "v=0\r\no=iTunes {sid} 0 IN {} {local_ip}\r\ns=iTunes\r\nc=IN {} {remote_ip}\r\nt=0 0\r\nm=audio 0 RTP/AVP 96\r\na=rtpmap:96 AppleLossless\r\na=fmtp:96 {FRAMES_PER_PACKET} 0 16 40 10 14 2 255 0 0 {RATE}\r\n",
        ip4(local_ip),
        ip4(remote_ip)
    );
    rtsp.request("ANNOUNCE", None, &[], Some(("application/sdp", sdp.as_bytes())))?;
    let transport = format!(
        "RTP/AVP/UDP;unicast;interleaved=0-1;mode=record;control_port={};timing_port={}",
        ports.control, ports.timing
    );
    let resp = rtsp.request("SETUP", None, &[("Transport", transport)], None)?;
    rtsp.session = resp.header("Session").map(|s| s.split(';').next().unwrap_or(s).to_string());
    let t = resp.header("Transport").ok_or("SETUP: no Transport")?;
    let server_port = transport_param(t, "server_port").ok_or("SETUP: no server_port")?;
    let control_port = transport_param(t, "control_port").unwrap_or(server_port + 1);
    rtsp.request("RECORD", None, &[("Range", "npt=0-".into()), ("RTP-Info", format!("seq={seq};rtptime={rtptime}"))], None)?;
    Ok((server_port, control_port))
}

const PLIST: &str = "application/x-apple-binary-plist";

/// Transient HomeKit pair-setup (M1..M4). Returns the 64-byte SRP session key.
fn pair_transient(rtsp: &mut Rtsp) -> Result<[u8; 64], String> {
    let hkp = [("X-Apple-HKP", "4".to_string())];
    let octets = "application/octet-stream";
    let m1 = tlv::encode(&[(tlv::STATE, &[1]), (tlv::METHOD, &[0]), (tlv::FLAGS, &[0x10])]);
    let r = rtsp.request("POST", Some("/pair-setup"), &hkp, Some((octets, &m1)))?;
    let m2 = tlv::decode(&r.body);
    if let Some(e) = tlv::get(&m2, tlv::ERROR) {
        return Err(format!("pair-setup refused (error {e:?})"));
    }
    let salt = tlv::get(&m2, tlv::SALT).ok_or("pair-setup: no salt")?;
    let b_pub = tlv::get(&m2, tlv::PUBLIC_KEY).ok_or("pair-setup: no public key")?;
    let proof = SrpClient::new().process(salt, b_pub, pairing::TRANSIENT_PIN)?;

    let m3 = tlv::encode(&[(tlv::STATE, &[3]), (tlv::PUBLIC_KEY, &proof.a_pub), (tlv::PROOF, &proof.m1)]);
    let r = rtsp.request("POST", Some("/pair-setup"), &hkp, Some((octets, &m3)))?;
    let m4 = tlv::decode(&r.body);
    if let Some(e) = tlv::get(&m4, tlv::ERROR) {
        return Err(format!("pair-setup rejected our proof (error {e:?})"));
    }
    if tlv::get(&m4, tlv::PROOF) != Some(&proof.expected_m2[..]) {
        return Err("pair-setup: device proof mismatch".into());
    }
    Ok(proof.session_key)
}

fn uuid_v4() -> String {
    let mut u = random_bytes::<16>();
    u[6] = (u[6] & 0x0f) | 0x40;
    u[8] = (u[8] & 0x3f) | 0x80;
    let hex: String = u.iter().map(|b| format!("{b:02X}")).collect();
    format!("{}-{}-{}-{}-{}", &hex[..8], &hex[8..12], &hex[12..16], &hex[16..20], &hex[20..])
}

fn random_bytes<const N: usize>() -> [u8; N] {
    let mut b = [0u8; N];
    getrandom::getrandom(&mut b).expect("OS RNG");
    b
}

fn negotiate_ap2(
    rtsp: &mut Rtsp,
    device: &Device,
    local_ip: IpAddr,
    remote_ip: IpAddr,
    control_port: u16,
    timing_port: u16,
) -> Result<Negotiated, String> {
    let key = pair_transient(rtsp)?;
    rtsp.encryptor = Some(Encryptor::new(&pairing::hkdf32(&key, "Control-Salt", "Control-Write-Encryption-Key")));
    rtsp.reader.get_mut().enable(&pairing::hkdf32(&key, "Control-Salt", "Control-Read-Encryption-Key"));

    let id = random_bytes::<6>();
    let device_id = id.iter().map(|b| format!("{b:02X}")).collect::<Vec<_>>().join(":");
    let uuid = uuid_v4();
    // PTP when the receiver supports it (AirPlay 2 receivers generally won't play
    // NTP-timed streams); NTP only as a fallback, e.g. if ports 319/320 are taken.
    let ptp = if device.ptp {
        match ptp::join(remote_ip) {
            Ok(p) => Some(p),
            Err(e) => {
                eprintln!("dust: {e}; falling back to NTP timing");
                None
            }
        }
    } else {
        None
    };
    let setup = match &ptp {
        Some(p) => {
            let peer = dict([
                ("ID", Value::String(uuid.clone())),
                ("DeviceType", Value::Int(0)),
                ("ClockID", Value::Int(p.clock_id())), // encoded as signed 64-bit, as Apple does
                ("SupportsClockPortMatchingOverride", Value::Bool(false)),
                ("Addresses", Value::Array(vec![Value::String(local_ip.to_string())])),
            ]);
            dict([
                ("name", Value::String("dust".into())),
                ("deviceID", Value::String(device_id.clone())),
                ("sessionUUID", Value::String(uuid)),
                ("timingProtocol", Value::String("PTP".into())),
                ("macAddress", Value::String(device_id)),
                ("groupUUID", Value::String(uuid_v4())),
                ("groupContainsGroupLeader", Value::Bool(false)),
                ("timingPeerInfo", peer.clone()),
                ("timingPeerList", Value::Array(vec![peer])),
            ])
        }
        None => dict([
            ("deviceID", Value::String(device_id)),
            ("sessionUUID", Value::String(uuid)),
            ("timingPort", Value::Int(timing_port as u64)),
            ("timingProtocol", Value::String("NTP".into())),
        ]),
    };
    let r = rtsp.request("SETUP", None, &[], Some((PLIST, &bplist::encode(&setup))))?;
    rtsp.session = r.header("Session").map(|s| s.split(';').next().unwrap_or(s).to_string());
    let reply = bplist::decode(&r.body)?;
    let events = reply
        .get("eventPort")
        .and_then(Value::as_u64)
        .filter(|&p| p > 0)
        .and_then(|p| TcpStream::connect_timeout(&SocketAddr::new(remote_ip, p as u16), Duration::from_secs(2)).ok());

    rtsp.request("RECORD", None, &[], None)?;
    if ptp.is_some() {
        let peers = Value::Array(vec![Value::String(remote_ip.to_string()), Value::String(local_ip.to_string())]);
        rtsp.request("SETPEERS", None, &[], Some(("/peer-list-changed", &bplist::encode(&peers))))?;
    }

    let shk: [u8; 32] = key[..32].try_into().unwrap();
    let stream = dict([
        ("audioFormat", Value::Int(0x40000)), // ALAC 44100/16/2
        ("audioMode", Value::String("default".into())),
        ("controlPort", Value::Int(control_port as u64)),
        ("ct", Value::Int(2)), // ALAC
        ("isMedia", Value::Bool(true)),
        ("latencyMax", Value::Int(LATENCY as u64)),
        ("latencyMin", Value::Int(11_025)),
        ("shk", Value::Data(shk.to_vec())),
        ("spf", Value::Int(FRAMES_PER_PACKET as u64)),
        ("sr", Value::Int(RATE as u64)),
        ("type", Value::Int(96)), // realtime
        ("supportsDynamicStreamID", Value::Bool(false)),
        ("streamConnectionID", Value::Int(u32::from_be_bytes(random_bytes::<4>()) as u64)),
    ]);
    let body = bplist::encode(&dict([("streams", Value::Array(vec![stream]))]));
    let r = rtsp.request("SETUP", None, &[], Some((PLIST, &body)))?;
    let reply = bplist::decode(&r.body)?;
    let s0 = reply.get("streams").and_then(|s| s.index(0)).ok_or("SETUP stream: no streams in reply")?;
    let data = s0.get("dataPort").and_then(Value::as_u64).ok_or("SETUP stream: no dataPort")?;
    let ctl = s0.get("controlPort").and_then(Value::as_u64).ok_or("SETUP stream: no controlPort")?;
    Ok(Negotiated { server_port: data as u16, control_port: ctl as u16, audio_key: Some(shk), events, ptp })
}

fn spawn(name: &str, f: impl FnOnce() + Send + 'static) -> JoinHandle<()> {
    std::thread::Builder::new().name(name.into()).spawn(f).expect("spawn thread")
}

fn timing_loop(sock: UdpSocket, sh: Arc<Shared>) {
    let mut buf = [0u8; 128];
    while sh.running.load(Ordering::Relaxed) {
        let Ok((n, from)) = sock.recv_from(&mut buf) else { continue };
        if n < 32 || buf[1] & 0x7f != 0x52 {
            continue;
        }
        sh.timing_requests.fetch_add(1, Ordering::Relaxed);
        let now = ntp_now().to_be_bytes();
        let mut reply = [0u8; 32];
        reply[..4].copy_from_slice(&[0x80, 0xd3, 0x00, 0x07]);
        reply[8..16].copy_from_slice(&buf[24..32]); // their transmit time -> our origin
        reply[16..24].copy_from_slice(&now);
        reply[24..32].copy_from_slice(&now);
        let _ = sock.send_to(&reply, from);
    }
}

fn control_loop(sock: UdpSocket, dst: SocketAddr, sh: Arc<Shared>) {
    let mut buf = [0u8; 64];
    while sh.running.load(Ordering::Relaxed) {
        let Ok((n, _)) = sock.recv_from(&mut buf) else { continue };
        // Retransmit request: 0x80 0xd5 <seq> <first missing> <count>
        if n < 8 || buf[1] & 0x7f != 0x55 {
            continue;
        }
        let first = u16::from_be_bytes([buf[4], buf[5]]);
        let count = u16::from_be_bytes([buf[6], buf[7]]);
        let history = sh.history.lock().unwrap();
        for i in 0..count {
            let want = first.wrapping_add(i);
            if let Some((_, pkt)) = history.iter().find(|(s, _)| *s == want) {
                let mut out = Vec::with_capacity(pkt.len() + 4);
                out.extend_from_slice(&[0x80, 0xd6]);
                out.extend_from_slice(&want.to_be_bytes());
                out.extend_from_slice(pkt);
                let _ = sock.send_to(&out, dst);
                sh.resent.fetch_add(1, Ordering::Relaxed);
            }
        }
    }
}

/// PTP flavour (AirPlay 2): rtptime playing at `now`, PTP time, stream anchor, clock id.
fn send_sync_ptp(sock: &UdpSocket, dst: SocketAddr, now_rtp: u32, head_rtp: u32, clock_id: u64, first: bool) {
    let mut p = [0u8; 28];
    p[..4].copy_from_slice(&[if first { 0x90 } else { 0x80 }, 0xd7, 0x00, 0x06]);
    p[4..8].copy_from_slice(&now_rtp.wrapping_sub(LATENCY).to_be_bytes());
    p[8..16].copy_from_slice(&ptp::now_ns().to_be_bytes());
    p[16..20].copy_from_slice(&head_rtp.wrapping_sub(11_025).to_be_bytes());
    p[20..28].copy_from_slice(&clock_id.to_be_bytes());
    let _ = sock.send_to(&p, dst);
}

fn send_sync(sock: &UdpSocket, dst: SocketAddr, now_rtp: u32, first: bool) {
    let mut p = [0u8; 20];
    p[..4].copy_from_slice(&[if first { 0x90 } else { 0x80 }, 0xd4, 0x00, 0x07]);
    p[4..8].copy_from_slice(&now_rtp.wrapping_sub(LATENCY).to_be_bytes());
    p[8..16].copy_from_slice(&ntp_now().to_be_bytes());
    p[16..20].copy_from_slice(&now_rtp.to_be_bytes());
    let _ = sock.send_to(&p, dst);
}

fn audio_loop(
    audio: UdpSocket,
    control: UdpSocket,
    control_dst: SocketAddr,
    mut consumer: rtrb::Consumer<i16>,
    cipher: Option<ChaCha20Poly1305>,
    ptp_clock: Option<u64>,
    sh: Arc<Shared>,
) {
    let ssrc = fastrand::u32(..);
    let mut frame = vec![0i16; FRAMES_PER_PACKET * 2];
    let mut start = Instant::now();
    let mut start_rtp = 0u32;
    let mut sent: u64 = 0;
    let mut next_sync: u64 = 0;
    let mut first = true;
    while sh.running.load(Ordering::Relaxed) {
        if sh.drain.swap(false, Ordering::AcqRel) {
            let n = consumer.slots();
            if let Ok(c) = consumer.read_chunk(n) {
                c.commit_all();
            }
        }
        if !sh.playing.load(Ordering::Acquire) {
            sh.idle.store(true, Ordering::Release);
            std::thread::sleep(Duration::from_millis(5));
            continue;
        }
        sh.idle.store(false, Ordering::Release);
        if sh.restart.swap(false, Ordering::AcqRel) {
            start = Instant::now();
            start_rtp = sh.rtptime.load(Ordering::Acquire);
            sent = 0;
            next_sync = 0;
            first = true;
        }
        let elapsed = (start.elapsed().as_secs_f64() * RATE as f64) as u64;
        if elapsed >= next_sync {
            let now_rtp = start_rtp.wrapping_add(elapsed as u32);
            match ptp_clock {
                Some(id) => send_sync_ptp(&control, control_dst, now_rtp, sh.rtptime.load(Ordering::Acquire), id, first),
                None => send_sync(&control, control_dst, now_rtp, first),
            }
            next_sync = elapsed + RATE as u64;
        }
        if sent >= elapsed + LEAD {
            std::thread::sleep(Duration::from_millis(2));
            continue;
        }
        // Take a whole packet if available; pad with silence on underrun.
        let avail = consumer.slots().min(frame.len()) & !1;
        match consumer.read_chunk(avail) {
            Ok(chunk) => {
                let (a, b) = chunk.as_slices();
                frame[..a.len()].copy_from_slice(a);
                frame[a.len()..a.len() + b.len()].copy_from_slice(b);
                chunk.commit_all();
            }
            Err(_) => unreachable!("slots checked"),
        }
        frame[avail..].fill(0);

        let seq = sh.seq.load(Ordering::Acquire);
        let ts = sh.rtptime.load(Ordering::Acquire);
        let mut header = [0u8; 12];
        header[0] = 0x80;
        header[1] = if first { 0xe0 } else { 0x60 };
        header[2..4].copy_from_slice(&seq.to_be_bytes());
        header[4..8].copy_from_slice(&ts.to_be_bytes());
        header[8..12].copy_from_slice(&ssrc.to_be_bytes());
        let payload = alac_frame(&frame);
        let pkt = match &cipher {
            Some(c) => pairing::seal_audio(c, &header, &payload, seq),
            None => [&header[..], &payload].concat(),
        };
        if let Err(e) = audio.send(&pkt) {
            eprintln!("dust: airplay send: {e}");
        }
        sh.packets.fetch_add(1, Ordering::Relaxed);
        if frame.iter().any(|&s| s != 0) {
            sh.audible.fetch_add(1, Ordering::Relaxed);
        }
        {
            let mut h = sh.history.lock().unwrap();
            if h.len() == HISTORY {
                h.pop_front();
            }
            h.push_back((seq, pkt));
        }
        first = false;
        sent += FRAMES_PER_PACKET as u64;
        sh.seq.store(seq.wrapping_add(1), Ordering::Release);
        sh.rtptime.store(ts.wrapping_add(FRAMES_PER_PACKET as u32), Ordering::Release);
    }
}

impl Sink for AirPlaySink {
    fn write(&mut self, samples: &[i16]) -> usize {
        if self.producer.slots() < samples.len() {
            return 0;
        }
        if let Ok(chunk) = self.producer.write_chunk_uninit(samples.len()) {
            chunk.fill_from_iter(samples.iter().copied());
        }
        samples.len()
    }

    fn pending_frames(&self) -> usize {
        (self.producer.buffer().capacity() - self.producer.slots()) / 2
    }

    fn latency_frames(&self) -> usize {
        LATENCY as usize + LEAD as usize
    }

    fn pause(&mut self) {
        self.halt();
    }

    fn resume(&mut self) {
        self.shared.restart.store(true, Ordering::Release);
        self.shared.playing.store(true, Ordering::Release);
    }

    fn flush(&mut self) {
        let was_playing = self.shared.playing.load(Ordering::Acquire);
        self.halt();
        self.shared.drain.store(true, Ordering::Release);
        // Wait for the audio thread to drain so pending_frames() is accurate.
        let deadline = Instant::now() + Duration::from_millis(100);
        while self.shared.drain.load(Ordering::Acquire) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(1));
        }
        if was_playing {
            self.resume();
        }
    }

    fn set_volume(&mut self, volume: f32) {
        // AirPlay volume: -30.0 (quiet) ..= 0.0 dB, -144 = mute.
        let db = if volume <= 0.001 { -144.0 } else { -30.0 + 30.0 * volume.clamp(0.0, 1.0) };
        let body = format!("volume: {db:.6}\r\n");
        let _ = self.rtsp.request("SET_PARAMETER", None, &[], Some(("text/parameters", body.as_bytes())));
    }
}

impl Drop for AirPlaySink {
    fn drop(&mut self) {
        self.shared.playing.store(false, Ordering::Release);
        let _ = self.rtsp.request("TEARDOWN", None, &[], None);
        self.shared.running.store(false, Ordering::Release);
        for t in self.threads.drain(..) {
            let _ = t.join();
        }
        if std::env::var_os("DUST_DEBUG").is_some() {
            let sh = &self.shared;
            eprintln!(
                "airplay: {} packets sent ({} audible), {} timing requests answered, {} packets retransmitted",
                sh.packets.load(Ordering::Relaxed),
                sh.audible.load(Ordering::Relaxed),
                sh.timing_requests.load(Ordering::Relaxed),
                sh.resent.load(Ordering::Relaxed)
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alac_layout() {
        let mut s = vec![0i16; FRAMES_PER_PACKET * 2];
        s[0] = 0x1234;
        s[1] = -1;
        let f = alac_frame(&s);
        // 23 header bits + 352*32 sample bits + 3 end bits = 11290 bits -> 1412 bytes
        assert_eq!(f.len(), 1412);
        // header: 001 0000 000000000000 0 00 1 -> first 23 bits
        assert_eq!(f[0], 0b0010_0000);
        assert_eq!(f[1], 0);
        assert_eq!(f[2] & 0b1111_1110, 0b0000_0010);
        // first sample 0x1234 starts at bit 23
        let bits = |from: usize, n: usize| (from..from + n).fold(0u32, |acc, b| (acc << 1) | ((f[b / 8] >> (7 - b % 8)) & 1) as u32);
        assert_eq!(bits(23, 16), 0x1234);
        assert_eq!(bits(39, 16), 0xffff);
        assert_eq!(bits(23 + 352 * 32, 3), 7);
    }

    /// Decode our escape frames with a real ALAC decoder.
    #[test]
    fn alac_decodes() {
        use symphonia::core::audio::SampleBuffer;
        use symphonia::core::codecs::{CODEC_TYPE_ALAC, CodecParameters, DecoderOptions};
        use symphonia::core::formats::Packet;
        // ALACSpecificConfig matching our SDP fmtp line.
        let mut cookie = Vec::new();
        cookie.extend(352u32.to_be_bytes());
        cookie.extend([0, 16, 40, 10, 14, 2]);
        cookie.extend(255u16.to_be_bytes());
        cookie.extend(0u32.to_be_bytes());
        cookie.extend(0u32.to_be_bytes());
        cookie.extend(44100u32.to_be_bytes());
        let mut params = CodecParameters::new();
        params.for_codec(CODEC_TYPE_ALAC).with_extra_data(cookie.into_boxed_slice()).with_sample_rate(44100);
        let mut dec = symphonia::default::get_codecs().make(&params, &DecoderOptions::default()).unwrap();
        let input: Vec<i16> = (0..FRAMES_PER_PACKET * 2).map(|i| ((i as f32 * 0.05).sin() * 8000.0) as i16).collect();
        let pkt = Packet::new_from_slice(0, 0, 352, &alac_frame(&input));
        let buf = dec.decode(&pkt).unwrap();
        let mut out = SampleBuffer::<i16>::new(buf.capacity() as u64, *buf.spec());
        out.copy_interleaved_ref(buf);
        assert_eq!(out.samples(), &input[..]);
    }

    #[test]
    fn transport_parse() {
        let t = "RTP/AVP/UDP;unicast;mode=record;server_port=53561;control_port=63379;timing_port=50607";
        assert_eq!(transport_param(t, "server_port"), Some(53561));
        assert_eq!(transport_param(t, "control_port"), Some(63379));
    }
}
