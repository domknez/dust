//! Audio outputs. Everything the player decodes goes to a [`Sink`]: the local sound
//! card ([`local`]) or an AirPlay speaker ([`airplay`]).

pub mod airplay;
pub mod local;
mod resample;

/// Every sink consumes interleaved stereo i16 at this rate.
pub const RATE: u32 = 44_100;

pub trait Sink {
    /// Queue interleaved stereo samples. Non-blocking: returns how many samples were
    /// accepted (either all or none).
    fn write(&mut self, samples: &[i16]) -> usize;
    /// Frames accepted but not yet handed to the device/network.
    fn pending_frames(&self) -> usize;
    /// Extra delay between hand-off and sound, in frames (for position display).
    fn latency_frames(&self) -> usize;
    fn pause(&mut self);
    fn resume(&mut self);
    /// Drop everything queued (seek / track change).
    fn flush(&mut self);
    /// 0.0 ..= 1.0
    fn set_volume(&mut self, volume: f32);
    /// False once the output can no longer play (e.g. the network path to an AirPlay
    /// receiver went away); the player then sets it up again.
    fn healthy(&self) -> bool {
        true
    }
}
