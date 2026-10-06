//! [`AirPlaySink`]: connects to a speaker and implements the audio [`Sink`].

use super::ap2::ptp::PtpPeer;
use super::discovery::Device;
use super::negotiate::{self, Local, Negotiated};
use super::rtsp::Rtsp;
use super::transport::{self, AudioOut, Shared, Timing};
use super::{LATENCY, LEAD, RATE};
use crate::output::Sink;
use chacha20poly1305::ChaCha20Poly1305;
use chacha20poly1305::aead::KeyInit;
use std::net::{IpAddr, Ipv6Addr, SocketAddr, TcpStream, UdpSocket};
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(2);
const REPLY_TIMEOUT: Duration = Duration::from_secs(10);
/// How long to wait for the audio thread to park or drain.
const HANDOFF_TIMEOUT: Duration = Duration::from_millis(200);

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
        let stream = connect_any(device)?;
        let local_ip = stream.local_addr().map_err(|e| e.to_string())?.ip();
        let remote_ip = stream.peer_addr().map_err(|e| e.to_string())?.ip();
        let session_id = fastrand::u32(..);
        let user_agent = if device.ap2 { "AirPlay/381.13" } else { "iTunes/11.0.4 (Windows; N)" };
        super::dacp::advertise_on(local_ip);
        let mut rtsp = Rtsp::new(stream, format!("rtsp://{local_ip}/{session_id}"), user_agent, super::dacp::id().to_string())?;

        let any: IpAddr = if local_ip.is_ipv4() { [0, 0, 0, 0].into() } else { Ipv6Addr::UNSPECIFIED.into() };
        let bind = || UdpSocket::bind(SocketAddr::new(any, 0)).map_err(|e| e.to_string());
        let (control, timing, audio) = (bind()?, bind()?, bind()?);
        for s in [&control, &timing] {
            s.set_read_timeout(Some(Duration::from_millis(200))).ok();
        }
        let local = Local {
            ip: local_ip,
            session_id,
            control_port: control.local_addr().map_err(|e| e.to_string())?.port(),
            timing_port: timing.local_addr().map_err(|e| e.to_string())?.port(),
            seq: fastrand::u16(..),
            rtptime: fastrand::u32(..),
        };
        let shared = Arc::new(Shared::new(local.seq, local.rtptime));
        // Receivers sync clocks with us during SETUP, so answer timing requests first.
        let sh = shared.clone();
        let mut threads = vec![transport::spawn("airplay-timing", move || transport::timing_loop(timing, &sh))];

        let negotiated = if device.ap2 {
            negotiate::airplay2(&mut rtsp, device, &local, remote_ip)
        } else {
            negotiate::airplay1(&mut rtsp, device, &local, remote_ip)
        };
        let Negotiated { data_port, control_port, audio_key, events, ptp } = negotiated.map_err(|e| {
            shared.running.store(false, Ordering::Release);
            format!("{}: {e}", device.name)
        })?;

        audio.connect(SocketAddr::new(remote_ip, data_port)).map_err(|e| e.to_string())?;
        let control_dst = SocketAddr::new(remote_ip, control_port);
        // ~1 s of audio between the player and network pacing.
        let (producer, consumer) = rtrb::RingBuffer::new(RATE as usize * 2);
        let sh = shared.clone();
        let retransmits = control.try_clone().map_err(|e| e.to_string())?;
        threads.push(transport::spawn("airplay-control", move || transport::control_loop(retransmits, control_dst, &sh)));
        let out = AudioOut {
            socket: audio,
            control,
            control_dst,
            cipher: audio_key.map(|k| ChaCha20Poly1305::new(&k.into())),
            timing: ptp.as_ref().map_or(Timing::Ntp, |p| Timing::Ptp { clock_id: p.clock_id() }),
        };
        let sh = shared.clone();
        threads.push(transport::spawn("airplay-audio", move || transport::audio_loop(out, consumer, &sh)));

        let mut sink = Self { rtsp, _events: events, _ptp: ptp, shared, producer, threads };
        sink.set_volume(volume);
        Ok(sink)
    }

    /// Stop the audio thread and drop the receiver's buffer.
    fn halt(&mut self) {
        self.shared.playing.store(false, Ordering::Release);
        wait_until(|| self.shared.idle.load(Ordering::Acquire));
        let seq = self.shared.seq.load(Ordering::Acquire);
        let rtptime = self.shared.rtptime.load(Ordering::Acquire);
        let _ = self.rtsp.request("FLUSH", None, &[("RTP-Info", format!("seq={seq};rtptime={rtptime}"))], None);
    }
}

/// Try each advertised address until one accepts (hosts list VPN/bridge addresses too).
fn connect_any(device: &Device) -> Result<TcpStream, String> {
    let mut last_err = String::from("no address");
    let stream = device
        .addrs
        .iter()
        .find_map(|a| TcpStream::connect_timeout(a, CONNECT_TIMEOUT).map_err(|e| last_err = e.to_string()).ok())
        .ok_or_else(|| format!("{}: {last_err}", device.name))?;
    stream.set_read_timeout(Some(REPLY_TIMEOUT)).ok();
    stream.set_nodelay(true).ok();
    Ok(stream)
}

fn wait_until(done: impl Fn() -> bool) {
    let deadline = Instant::now() + HANDOFF_TIMEOUT;
    while !done() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(1));
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
        wait_until(|| !self.shared.drain.load(Ordering::Acquire));
        if was_playing {
            self.resume();
        }
    }

    fn set_volume(&mut self, volume: f32) {
        // AirPlay volume: -30.0 (quiet) ..= 0.0 dB, -144 = mute.
        let db = if volume <= 0.001 { -144.0 } else { -30.0 + 30.0 * volume.clamp(0.0, 1.0) };
        log_debug!("airplay: volume {volume:.3} -> {db:.1} dB");
        let body = format!("volume: {db:.6}\r\n");
        let _ = self.rtsp.request("SET_PARAMETER", None, &[], Some(("text/parameters", body.as_bytes())));
    }
}

impl Drop for AirPlaySink {
    fn drop(&mut self) {
        self.shared.playing.store(false, Ordering::Release);
        let _ = self.rtsp.request("TEARDOWN", None, &[], None);
        self.shared.running.store(false, Ordering::Release);
        // Ends the event channel reader, which holds its own handle to the socket.
        if let Some(events) = &self._events {
            let _ = events.shutdown(std::net::Shutdown::Both);
        }
        for t in self.threads.drain(..) {
            let _ = t.join();
        }
        let s = &self.shared.stats;
        log_debug!(
            "airplay: {} packets sent ({} audible), {} timing requests answered, {} packets retransmitted",
            s.packets.load(Ordering::Relaxed),
            s.audible.load(Ordering::Relaxed),
            s.timing_requests.load(Ordering::Relaxed),
            s.resent.load(Ordering::Relaxed)
        );
    }
}
