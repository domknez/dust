pub mod airplay;
pub mod ap2;
pub mod local;

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
}

/// Linear interpolating stereo resampler. Plenty for a music player's playback path
/// where device rate rarely differs from 44.1 kHz.
pub struct Resampler {
    step: f64,
    pos: f64,
    last: [f32; 2],
}

impl Resampler {
    pub fn new(from: u32, to: u32) -> Self {
        Self { step: from as f64 / to as f64, pos: 0.0, last: [0.0; 2] }
    }

    pub fn is_identity(&self) -> bool {
        self.step == 1.0
    }

    pub fn process(&mut self, input: &[f32], out: &mut Vec<f32>) {
        let n = input.len() / 2;
        if n == 0 {
            return;
        }
        let frame = |i: isize| if i < 0 { self.last } else { [input[i as usize * 2], input[i as usize * 2 + 1]] };
        while self.pos < (n - 1) as f64 {
            let i = self.pos.floor() as isize;
            let t = (self.pos - i as f64) as f32;
            let (a, b) = (frame(i), frame(i + 1));
            out.push(a[0] + (b[0] - a[0]) * t);
            out.push(a[1] + (b[1] - a[1]) * t);
            self.pos += self.step;
        }
        self.pos -= n as f64;
        self.last = [input[(n - 1) * 2], input[(n - 1) * 2 + 1]];
    }

    pub fn reset(&mut self) {
        self.pos = 0.0;
        self.last = [0.0; 2];
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resample_counts() {
        let mut r = Resampler::new(44_100, 48_000);
        let input = vec![0.5f32; 44_100 * 2];
        let mut out = Vec::new();
        for chunk in input.chunks(1152 * 2) {
            r.process(chunk, &mut out);
        }
        let frames = out.len() / 2;
        assert!((47_990..=48_000).contains(&frames), "{frames}");
        assert!(out.iter().all(|&s| (s - 0.5).abs() < 1e-6 || s == 0.25 || s.abs() <= 0.5));
    }
}
