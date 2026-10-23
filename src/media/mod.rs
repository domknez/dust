//! Turning a track's bytes into audio, whatever source they come from. The music
//! sources open their bytes ([`Opener`]) and hand back a [`Stream`]; the player
//! decodes it through [`Playback`] without knowing where it came from.
//!
//! - [`playback`]: what the player asks of a playing track, and why one didn't open
//! - [`stream`]: a byte stream's container reader and decoder (seeks, resampling)
//! - [`prefetch`]: stall-proof network reads for streams
//!
//! Files not at the output's rate go through [`crate::output::resample`].

mod playback;
mod prefetch;
mod stream;

pub use playback::{Decoded, OpenError, Playback};
pub use prefetch::Prefetch;
pub use stream::{Opened, Opener, Stream, StreamInfo};
