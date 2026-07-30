//! Convert Alpha Omega AlphaLab SnR `.mpx` electrophysiology recordings to NWB 2.7.
//!
//! The `.mpx` format has no public specification. The block layout implemented in
//! [`mpx`] was established by walking the block stream and cross-checking decoded
//! signal against physiology; it is verified against map format 4 only, and other
//! versions are rejected rather than guessed at.
//!
//! ```no_run
//! use mpx2nwb::{convert, ConvertOptions};
//! let opts = ConvertOptions { subject: Some("R6".into()), ..Default::default() };
//! let summary = convert(&["rec_0001.mpx".into(), "rec_0002.mpx".into()], "rec.nwb", &opts)?;
//! println!("{} channels x {} samples", summary.channels, summary.samples);
//! # Ok::<(), mpx2nwb::Error>(())
//! ```

pub mod batch;
pub mod cli;
mod convert;
pub mod error;
pub mod mpx;
pub mod nwb;

pub use convert::{convert, ConvertOptions, Summary};
pub use error::{Error, Result};
