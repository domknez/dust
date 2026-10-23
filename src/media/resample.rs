//! Streaming linear resampler for interleaved stereo i16, used for podcast files
//! whose sample rate differs from the output's. Linear interpolation is plenty
//! for speech and keeps no more state than the last frame.

pub struct Resampler {
    /// Input frames per output frame.
    step: f64,
    /// Position of the next output frame, in input frames relative to the start of
    /// the next block (-1.0 is `last`).
    pos: f64,
    /// Last frame of the previous block.
    last: Option<[i16; 2]>,
}

impl Resampler {
    pub fn new(from_rate: u32, to_rate: u32) -> Self {
        Self { step: from_rate as f64 / to_rate as f64, pos: 0.0, last: None }
    }

    /// Resample a block of interleaved stereo samples, appending to `out`.
    pub fn process(&mut self, input: &[i16], out: &mut Vec<i16>) {
        let frames = input.len() / 2;
        if frames == 0 {
            return;
        }
        let at = |i: isize| -> [i16; 2] {
            if i < 0 {
                self.last.unwrap_or([input[0], input[1]])
            } else {
                let i = i as usize * 2;
                [input[i], input[i + 1]]
            }
        };
        // Interpolate between frame floor(pos) and the one after, both available.
        while self.pos < frames as f64 - 1.0 {
            let i = self.pos.floor();
            let frac = self.pos - i;
            let (a, b) = (at(i as isize), at(i as isize + 1));
            for c in 0..2 {
                out.push((a[c] as f64 + (b[c] as f64 - a[c] as f64) * frac).round() as i16);
            }
            self.pos += self.step;
        }
        self.pos -= frames as f64;
        self.last = Some(at(frames as isize - 1));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Resample a ramp of `frames` frames (left rises by `slope` per frame, right
    /// falls), fed in blocks of `block` frames.
    fn run(from: u32, to: u32, frames: usize, block: usize, slope: f64) -> Vec<i16> {
        let input: Vec<i16> = (0..frames).map(|i| (i as f64 * slope) as i16).flat_map(|x| [x, -x]).collect();
        let mut r = Resampler::new(from, to);
        let mut out = Vec::new();
        for chunk in input.chunks(block * 2) {
            r.process(chunk, &mut out);
        }
        out
    }

    #[test]
    fn doubles_by_interpolating() {
        let out = run(22_050, 44_100, 4, 4, 10.0);
        assert_eq!(out, vec![0, 0, 5, -5, 10, -10, 15, -15, 20, -20, 25, -25]);
    }

    #[test]
    fn rate_and_continuity_hold_across_blocks() {
        // 1 s at 48 kHz becomes ~1 s at 44.1 kHz, the same in one block or many.
        let whole = run(48_000, 44_100, 48_000, 48_000, 0.5);
        let pieces = run(48_000, 44_100, 48_000, 1_152, 0.5);
        assert!((whole.len() as i64 / 2 - 44_100).abs() <= 2, "{} frames", whole.len() / 2);
        assert_eq!(whole, pieces);
        // A ramp stays a ramp: no jumps at block edges.
        assert!(pieces.chunks(2).zip(pieces.chunks(2).skip(1)).all(|(a, b)| (b[0] - a[0]).abs() <= 1));
    }

    #[test]
    fn same_rate_passes_through() {
        assert_eq!(run(44_100, 44_100, 5, 2, 10.0), vec![0, 0, 10, -10, 20, -20, 30, -30]);
    }
}
