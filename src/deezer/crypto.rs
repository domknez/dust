//! Deezer's `BF_CBC_STRIPE` stream cipher: every third full 2048-byte chunk is
//! Blowfish-CBC encrypted with a fixed IV and a per-song key; the rest is plaintext.

use blowfish::Blowfish;
use blowfish::cipher::{BlockDecrypt, KeyInit, generic_array::GenericArray};
use md5::{Digest, Md5};
use std::io::{self, Read};

pub const CHUNK: usize = 2048;
const SECRET: &[u8; 16] = b"g4el58wc0zvf9na1";
const IV: [u8; 8] = [0, 1, 2, 3, 4, 5, 6, 7];

fn song_key(song_id: u64) -> [u8; 16] {
    let hex: Vec<u8> = Md5::digest(song_id.to_string().as_bytes()).iter().flat_map(|b| format!("{b:02x}").into_bytes()).collect();
    std::array::from_fn(|i| hex[i] ^ hex[i + 16] ^ SECRET[i])
}

/// Decrypting reader over a stripe-encrypted stream.
pub struct StripeReader<R> {
    inner: R,
    cipher: Blowfish,
    chunk_index: u64,
    buf: Box<[u8; CHUNK]>,
    len: usize,
    pos: usize,
}

impl<R: Read> StripeReader<R> {
    /// `start_chunk` is the index of the first chunk `inner` yields (for range reads).
    pub fn new(inner: R, song_id: u64, start_chunk: u64) -> Self {
        Self {
            inner,
            cipher: Blowfish::new_from_slice(&song_key(song_id)).expect("16-byte key"),
            chunk_index: start_chunk,
            buf: Box::new([0; CHUNK]),
            len: 0,
            pos: 0,
        }
    }

    fn fill(&mut self) -> io::Result<()> {
        self.len = 0;
        self.pos = 0;
        while self.len < CHUNK {
            match self.inner.read(&mut self.buf[self.len..]) {
                Ok(0) => break,
                Ok(n) => self.len += n,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                Err(e) => return Err(e),
            }
        }
        if self.len == CHUNK && self.chunk_index.is_multiple_of(3) {
            let mut prev = IV;
            for block in self.buf.chunks_exact_mut(8) {
                let ciphertext: [u8; 8] = block.try_into().unwrap();
                self.cipher.decrypt_block(GenericArray::from_mut_slice(block));
                block.iter_mut().zip(prev).for_each(|(b, p)| *b ^= p);
                prev = ciphertext;
            }
        }
        self.chunk_index += 1;
        Ok(())
    }
}

impl<R: Read> Read for StripeReader<R> {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        if self.pos == self.len {
            self.fill()?;
            if self.len == 0 {
                return Ok(0);
            }
        }
        let n = out.len().min(self.len - self.pos);
        out[..n].copy_from_slice(&self.buf[self.pos..self.pos + n]);
        self.pos += n;
        Ok(n)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use blowfish::cipher::BlockEncrypt;

    #[test]
    fn stripe_roundtrip() {
        let id = 3135556;
        let plain: Vec<u8> = (0..CHUNK * 4 + 100).map(|i| (i * 7) as u8).collect();
        let mut enc = plain.clone();
        let cipher: Blowfish = Blowfish::new_from_slice(&song_key(id)).unwrap();
        for (ci, chunk) in enc.chunks_mut(CHUNK).enumerate() {
            if ci % 3 == 0 && chunk.len() == CHUNK {
                let mut prev = IV;
                for block in chunk.chunks_exact_mut(8) {
                    block.iter_mut().zip(prev).for_each(|(b, p)| *b ^= p);
                    cipher.encrypt_block(GenericArray::from_mut_slice(block));
                    prev = block.try_into().unwrap();
                }
            }
        }
        assert_ne!(enc, plain);
        let mut out = Vec::new();
        StripeReader::new(&enc[..], id, 0).read_to_end(&mut out).unwrap();
        assert_eq!(out, plain);
    }
}
