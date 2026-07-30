//! Streaming reader for Alpha Omega AlphaLab SnR `.mpx`, map format 4.
//!
//! The format has no public specification; this layout was established by walking the
//! block stream and cross-checking decoded signal against physiology. Blocks are
//! `u16 length | u8 type | body`. Type `h` is the file header, `2` declares a channel,
//! `5` carries continuous samples.

use std::fs::File;
use std::io::{BufReader, Read};

#[derive(Debug, Clone)]
pub struct Channel {
    pub id: u16,
    pub name: String,
    pub stream: String,
    pub index: u32,
    pub rate_hz: f32,
    pub bit_uv: f32, // ADC bit resolution referred to the board input
    pub gain: f32,   // headstage gain; the electrode-referred scale is bit_uv/gain
}
impl Channel {
    /// Volts per stored integer. NWB stores raw ints plus this conversion factor.
    pub fn conversion_v(&self) -> f32 {
        self.bit_uv / self.gain * 1e-6
    }
}

#[derive(Debug, Clone, Default)]
pub struct Header {
    pub map_version: u8,
    pub application: String,
    pub year: u16,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
    pub t_min: f64,
    pub t_max: f64,
}
impl Header {
    pub fn iso8601(&self) -> String {
        format!(
            "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}+00:00",
            self.year, self.month, self.day, self.hour, self.minute, self.second
        )
    }
    pub fn duration_s(&self) -> f64 {
        self.t_max - self.t_min
    }
}

pub enum Item<'a> {
    Header(Header),
    Channel(Channel),
    Data { id: u16, samples: &'a [u8] },
    Other,
}

pub struct Reader {
    r: BufReader<File>,
    buf: Vec<u8>,
}

impl Reader {
    pub fn open(path: &str) -> std::io::Result<Self> {
        Ok(Self {
            r: BufReader::with_capacity(1 << 22, File::open(path)?),
            buf: vec![0u8; 1 << 16],
        })
    }

    /// Pull the next block. Returns None at end of stream.
    pub fn next_block(&mut self) -> Option<Item<'_>> {
        let mut lb = [0u8; 2];
        if self.r.read_exact(&mut lb).is_err() {
            return None;
        }
        let len = u16::from_le_bytes(lb) as usize;
        if len < 3 {
            return None;
        }
        let body = len - 2;
        if body > self.buf.len() {
            return None;
        }
        if self.r.read_exact(&mut self.buf[..body]).is_err() {
            return None;
        }
        let b = &self.buf[..body];
        match b[0] {
            b'h' if body >= 42 => {
                let e = b[39..]
                    .iter()
                    .position(|&c| c == 0)
                    .map(|p| 39 + p)
                    .unwrap_or(body);
                Some(Item::Header(Header {
                    hour: b[8],
                    minute: b[9],
                    second: b[10],
                    day: b[12],
                    month: b[13],
                    year: u16::from_le_bytes([b[14], b[15]]),
                    t_min: f64::from_le_bytes(b[18..26].try_into().unwrap()),
                    t_max: f64::from_le_bytes(b[26..34].try_into().unwrap()),
                    map_version: b[38],
                    application: String::from_utf8_lossy(&b[39..e]).trim().to_string(),
                }))
            }
            b'2' if body >= 34 => {
                let name = trailing_name(b);
                if name.is_empty() {
                    return Some(Item::Other);
                }
                let mut it = name.splitn(2, ' ');
                let stream = it.next().unwrap_or("").to_string();
                let index = it.next().unwrap_or("0").trim().parse::<u32>().unwrap_or(0);
                Some(Item::Channel(Channel {
                    id: u16::from_le_bytes([b[10], b[11]]),
                    name,
                    stream,
                    index,
                    bit_uv: f32le(b, 18),
                    rate_hz: f32le(b, 22) * 1000.0,
                    gain: f32le(b, 30),
                }))
            }
            b'5' if body >= 10 => {
                let id = u16::from_le_bytes([b[2], b[3]]);
                Some(Item::Data {
                    id,
                    samples: &self.buf[4..body - 4],
                })
            }
            _ => Some(Item::Other),
        }
    }
}

fn f32le(b: &[u8], o: usize) -> f32 {
    f32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}

/// The channel name is a null-terminated string before a 2-byte trailer. The field
/// preceding it can itself be printable, so anchor on a known stream prefix.
fn trailing_name(b: &[u8]) -> String {
    if b.len() < 4 {
        return String::new();
    }
    let mut e = b.len() - 2;
    while e > 0 && b[e - 1] == 0 {
        e -= 1
    }
    let mut s = e;
    while s > 0 && (32..127).contains(&b[s - 1]) {
        s -= 1
    }
    let run = String::from_utf8_lossy(&b[s..e]).to_string();
    const P: [&str; 8] = ["RAW ", "SPK ", "SEG ", "LFP ", "AI ", "Sync", "UD ", "Port"];
    let mut best: Option<usize> = None;
    for p in P {
        if let Some(i) = run.rfind(p) {
            best = Some(best.map_or(i, |b: usize| b.max(i)))
        }
    }
    match best {
        Some(i) => run[i..].trim().to_string(),
        None => String::new(),
    }
}

/// Read only the file header. Used to test segment contiguity before committing to a merge.
pub fn read_header(path: &str) -> std::io::Result<Header> {
    let mut rd = Reader::open(path)?;
    while let Some(it) = rd.next_block() {
        match it {
            Item::Header(h) if h.map_version > 0 => return Ok(h),
            Item::Data { .. } => break,
            _ => {}
        }
    }
    Ok(Header::default())
}
