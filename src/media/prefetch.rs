//! Reads a network stream on its own thread, so a dead connection (Wi-Fi drop,
//! sleep/wake) turns into a timeout instead of blocking the decoder forever.
//! The bounded channel is backpressure: a paused player stops reading, the
//! reader thread waits on the channel, and nothing times out.

use std::io::{self, Read};
use std::sync::Mutex;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::time::Duration;

const BLOCK: usize = 16 * 1024;
/// Blocks buffered ahead of the decoder.
const AHEAD: usize = 8;

pub struct Prefetch {
    /// `Mutex` only for `Sync` (symphonia's sources must be); reads use `get_mut`.
    blocks: Mutex<Receiver<io::Result<Vec<u8>>>>,
    current: Vec<u8>,
    pos: usize,
    stall: Duration,
    done: bool,
}

impl Prefetch {
    /// Start reading `inner`; a read fails if no data arrives for `stall`.
    pub fn new(mut inner: impl Read + Send + 'static, stall: Duration) -> Self {
        let (tx, blocks) = mpsc::sync_channel(AHEAD);
        let spawned = std::thread::Builder::new().name("stream-prefetch".into()).spawn(move || {
            loop {
                let mut block = vec![0; BLOCK];
                let result = match inner.read(&mut block) {
                    Ok(0) => break,
                    Ok(n) => {
                        block.truncate(n);
                        Ok(block)
                    }
                    Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                    Err(e) => Err(e),
                };
                let failed = result.is_err();
                // A send error means the stream was dropped (track change, seek).
                if tx.send(result).is_err() || failed {
                    break;
                }
            }
        });
        if let Err(e) = spawned {
            log_warn!("stream prefetch thread: {e}");
        }
        Self { blocks: Mutex::new(blocks), current: Vec::new(), pos: 0, stall, done: false }
    }
}

impl Read for Prefetch {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        while self.pos == self.current.len() {
            if self.done {
                return Ok(0);
            }
            let blocks = self.blocks.get_mut().unwrap_or_else(|e| e.into_inner());
            match blocks.recv_timeout(self.stall) {
                Ok(block) => {
                    self.current = block?;
                    self.pos = 0;
                }
                Err(RecvTimeoutError::Disconnected) => self.done = true,
                Err(RecvTimeoutError::Timeout) => return Err(io::Error::new(io::ErrorKind::TimedOut, "stream stalled")),
            }
        }
        let n = out.len().min(self.current.len() - self.pos);
        out[..n].copy_from_slice(&self.current[self.pos..self.pos + n]);
        self.pos += n;
        Ok(n)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passes_bytes_through() {
        let data: Vec<u8> = (0..100_000).map(|i| i as u8).collect();
        let mut out = Vec::new();
        Prefetch::new(io::Cursor::new(data.clone()), Duration::from_secs(5)).read_to_end(&mut out).unwrap();
        assert_eq!(out, data);
    }

    #[test]
    fn stalled_source_times_out() {
        struct Stalled;
        impl Read for Stalled {
            fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
                std::thread::sleep(Duration::from_secs(5));
                Ok(0)
            }
        }
        let err = Prefetch::new(Stalled, Duration::from_millis(50)).read(&mut [0; 16]).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::TimedOut);
    }

    #[test]
    fn source_errors_surface() {
        struct Broken;
        impl Read for Broken {
            fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
                Err(io::Error::new(io::ErrorKind::ConnectionReset, "reset"))
            }
        }
        let err = Prefetch::new(Broken, Duration::from_secs(5)).read(&mut [0; 16]).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::ConnectionReset);
    }
}
