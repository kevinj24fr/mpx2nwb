//! Error type for the conversion pipeline.

use std::fmt;

#[derive(Debug)]
pub enum Error {
    Io {
        path: String,
        source: std::io::Error,
    },
    Hdf5(hdf5_metno::Error),
    /// The file declares a map format this reader has not been verified against.
    UnsupportedFormat {
        path: String,
        version: u8,
    },
    /// No channels matched the requested stream.
    NoSuchStream {
        path: String,
        stream: String,
    },
    /// Channels were declared but none carried samples.
    NoData {
        path: String,
    },
    /// A continuation segment disagrees with the first segment.
    SegmentMismatch {
        path: String,
        detail: String,
    },
    Usage(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io { path, source } => write!(f, "{}: {}", path, source),
            Error::Hdf5(e) => write!(f, "hdf5: {}", e),
            Error::UnsupportedFormat { path, version } => write!(
                f, "{}: map format {} is not supported (this reader is verified against format 4 only)",
                path, version),
            Error::NoSuchStream { path, stream } => {
                write!(f, "{}: no '{}' channels declared", path, stream)
            }
            Error::NoData { path } => write!(f, "{}: channels declared but none carried samples", path),
            Error::SegmentMismatch { path, detail } => {
                write!(f, "{}: incompatible continuation segment: {}", path, detail)
            }
            Error::Usage(m) => write!(f, "{}", m),
        }
    }
}
impl std::error::Error for Error {}
impl From<hdf5_metno::Error> for Error {
    fn from(e: hdf5_metno::Error) -> Self {
        Error::Hdf5(e)
    }
}

pub type Result<T> = std::result::Result<T, Error>;

pub trait IoContext<T> {
    fn path(self, p: &str) -> Result<T>;
}
impl<T> IoContext<T> for std::io::Result<T> {
    fn path(self, p: &str) -> Result<T> {
        self.map_err(|source| Error::Io {
            path: p.to_string(),
            source,
        })
    }
}
