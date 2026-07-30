//! The conversion itself: read `.mpx`, write NWB.

use crate::error::{Error, IoContext, Result};
use crate::mpx::{Header, Item, Reader};
use crate::nwb::*;
use hdf5_metno as hdf5;
use std::collections::BTreeMap;

#[derive(Debug, Clone)]
pub struct ConvertOptions {
    pub stream: String,
    pub deflate: u8,
    pub subject: Option<String>,
    pub session_id: Option<String>,
    pub description: String,
}
impl Default for ConvertOptions {
    fn default() -> Self {
        ConvertOptions {
            stream: "RAW".into(),
            deflate: 4,
            subject: None,
            session_id: None,
            description: "Extracellular electrophysiology".into(),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Summary {
    pub samples: usize,
    pub channels: usize,
    pub seconds: f64,
}

/// Rows buffered before a chunk is written. Matches the HDF5 chunk height.
const CHUNK_ROWS: usize = 32768;
/// Blocks scanned when deciding which declared channels actually carry samples.
const PROBE_BLOCKS: usize = 40_000;

pub fn convert(inputs: &[String], output: &str, o: &ConvertOptions) -> Result<Summary> {
    let first = inputs
        .first()
        .ok_or_else(|| Error::Usage("no inputs".into()))?;

    // ---- header and channel declarations ----
    let mut rd = Reader::open(first).path(first)?;
    let mut hdr = Header::default();
    let mut chans = BTreeMap::new();
    let mut saw_data = false;
    while let Some(it) = rd.next_block() {
        match it {
            Item::Header(h) => {
                if h.map_version > 0 {
                    hdr = h
                }
            }
            Item::Channel(c) => {
                if c.stream == o.stream {
                    chans.insert(c.id, c);
                }
            }
            Item::Data { .. } => {
                saw_data = true;
                break;
            }
            Item::Other => {}
        }
    }
    if hdr.map_version != 4 {
        return Err(Error::UnsupportedFormat {
            path: first.clone(),
            version: hdr.map_version,
        });
    }
    if chans.is_empty() {
        return Err(Error::NoSuchStream {
            path: first.clone(),
            stream: o.stream.clone(),
        });
    }
    if !saw_data {
        return Err(Error::NoData {
            path: first.clone(),
        });
    }

    let ids: Vec<u16> = chans.keys().copied().collect();
    let rate = chans[&ids[0]].rate_hz as f64;
    let conversion = chans[&ids[0]].conversion_v();

    // ---- which declared channels carry samples ----
    // The acquisition template declares a fixed electrode count; a given rig may have
    // wired fewer, and empty channels should not become all-zero columns.
    let mut live: BTreeMap<u16, bool> = ids.iter().map(|i| (*i, false)).collect();
    let mut rd = Reader::open(first).path(first)?;
    let mut probed = 0usize;
    while let Some(it) = rd.next_block() {
        if let Item::Data { id, samples } = it {
            if !samples.is_empty() {
                live.entry(id).and_modify(|v| *v = true);
            }
            probed += 1;
            if probed > PROBE_BLOCKS {
                break;
            }
        }
    }
    let active: Vec<u16> = ids.iter().copied().filter(|i| live[i]).collect();
    if active.is_empty() {
        return Err(Error::NoData {
            path: first.clone(),
        });
    }
    let nch = active.len();

    // ---- NWB skeleton ----
    let f = hdf5::File::create(output)?;
    let root = f.as_group()?;
    let base = std::path::Path::new(first)
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    let seed = format!("{}|{}|{}", base, hdr.iso8601(), o.stream);

    attr_str(&root, "namespace", "core")?;
    attr_str(&root, "neurodata_type", "NWBFile")?;
    attr_str(&root, "object_id", &det_uuid(&seed))?;
    attr_str(&root, "nwb_version", NWB_VERSION)?;
    str_arr(&root, "file_create_date", &[hdr.iso8601()])?;
    str_ds(&root, "identifier", &det_uuid(&format!("id|{}", seed)))?;
    str_ds(&root, "session_description", &o.description)?;
    str_ds(&root, "session_start_time", &hdr.iso8601())?;
    str_ds(&root, "timestamps_reference_time", &hdr.iso8601())?;
    for g in [
        "analysis",
        "processing",
        "stimulus/presentation",
        "stimulus/templates",
    ] {
        root.create_group(g)?;
    }

    let general = root.create_group("general")?;
    if let Some(s) = &o.session_id {
        str_ds(&general, "session_id", s)?;
    }
    if let Some(sub) = &o.subject {
        let g = general.create_group("subject")?;
        typed(&g, "core", "Subject", &det_uuid(&format!("subj|{}", sub)))?;
        str_ds(&g, "subject_id", sub)?;
        str_ds(&g, "description", "see lab records")?;
    }
    let devices = general.create_group("devices")?;
    let dev = devices.create_group(&hdr.application)?;
    typed(&dev, "core", "Device", &det_uuid(&format!("dev|{}", seed)))?;
    attr_str(
        &dev,
        "description",
        "Alpha Omega AlphaLab SnR acquisition system",
    )?;
    attr_str(&dev, "manufacturer", "Alpha Omega Engineering")?;

    let ee = general.create_group("extracellular_ephys")?;
    let egrp = ee.create_group("array")?;
    typed(
        &egrp,
        "core",
        "ElectrodeGroup",
        &det_uuid(&format!("eg|{}", seed)),
    )?;
    attr_str(
        &egrp,
        "description",
        &format!("{} electrodes, stream {}", nch, o.stream),
    )?;
    attr_str(&egrp, "location", "unknown")?;
    // `device` must be an HDF5 soft link, not a scalar object reference: hdmf's
    // scalar-reference reader only dereferences Dataset targets.
    egrp.link_soft(&format!("/general/devices/{}", hdr.application), "device")?;

    let et = ee.create_group("electrodes")?;
    typed(
        &et,
        "hdmf-common",
        "DynamicTable",
        &det_uuid(&format!("et|{}", seed)),
    )?;
    attr_str(
        &et,
        "description",
        "electrodes carrying data in this recording",
    )?;
    let cols = ["location", "group", "group_name", "channel_name"]
        .iter()
        .map(|s| vstr(s))
        .collect::<Vec<_>>();
    et.new_attr::<hdf5::types::VarLenUnicode>()
        .shape([cols.len()])
        .create("colnames")?
        .write_raw(&cols)?;
    let idd = et.new_dataset::<i32>().shape([nch]).create("id")?;
    idd.write_raw(&(0..nch as i32).collect::<Vec<_>>())?;
    typed_ds(
        &idd,
        "hdmf-common",
        "ElementIdentifiers",
        &det_uuid(&format!("eid|{}", seed)),
    )?;

    for (name, vals, tag) in [
        ("location", vec!["unknown".to_string(); nch], "loc"),
        ("group_name", vec!["array".to_string(); nch], "gn"),
        (
            "channel_name",
            active
                .iter()
                .map(|i| chans[i].name.clone())
                .collect::<Vec<_>>(),
            "cn",
        ),
    ] {
        let d = str_arr(&et, name, &vals)?;
        typed_ds(
            &d,
            "hdmf-common",
            "VectorData",
            &det_uuid(&format!("{}|{}", tag, seed)),
        )?;
        attr_str_ds(&d, "description", name)?;
    }
    let gref: Vec<hdf5::ObjectReference1> = (0..nch)
        .map(|_| ee.reference("array"))
        .collect::<hdf5::Result<_>>()?;
    let gd = et
        .new_dataset::<hdf5::ObjectReference1>()
        .shape([nch])
        .create("group")?;
    gd.write_raw(&gref)?;
    typed_ds(
        &gd,
        "hdmf-common",
        "VectorData",
        &det_uuid(&format!("grp|{}", seed)),
    )?;
    attr_str_ds(&gd, "description", "electrode group reference")?;

    let acq = root.create_group("acquisition")?;
    let es = acq.create_group("ElectricalSeries")?;
    typed(
        &es,
        "core",
        "ElectricalSeries",
        &det_uuid(&format!("es|{}", seed)),
    )?;
    attr_str(
        &es,
        "description",
        &format!("{} stream, {} channels", o.stream, nch),
    )?;
    attr_str(
        &es,
        "comments",
        "raw int16; multiply by conversion for volts",
    )?;

    let mut ser = make_series(&es, nch, CHUNK_ROWS, o.deflate)?;
    let st = es.new_dataset::<f64>().shape(()).create("starting_time")?;
    st.write_scalar(&0.0f64)?;
    st.new_attr::<f64>().create("rate")?.write_scalar(&rate)?;
    attr_str_ds(&st, "unit", "seconds")?;

    let table_ref = et.reference::<hdf5::ObjectReference1>(".")?;
    let ed = es.new_dataset::<i64>().shape([nch]).create("electrodes")?;
    ed.write_raw(&(0..nch as i64).collect::<Vec<_>>())?;
    typed_ds(
        &ed,
        "hdmf-common",
        "DynamicTableRegion",
        &det_uuid(&format!("dtr|{}", seed)),
    )?;
    attr_str_ds(&ed, "description", "electrodes in this series")?;
    ed.new_attr::<hdf5::ObjectReference1>()
        .create("table")?
        .write_scalar(&table_ref)?;

    // ---- stream samples into [time, channel] ----
    let pos: BTreeMap<u16, usize> = active.iter().enumerate().map(|(i, c)| (*c, i)).collect();
    let mut buf: BTreeMap<u16, Vec<i16>> = active
        .iter()
        .map(|c| (*c, Vec::with_capacity(1 << 20)))
        .collect();
    let mut total = 0usize;

    let mut prev_end = hdr.t_max;
    for (n, path) in inputs.iter().enumerate() {
        if n > 0 {
            let h = verify_segment(path, &o.stream, rate, &active, &chans)?;
            // Segments must abut on the acquisition clock. Merging a gap would silently
            // shift every later sample, so refuse rather than produce a wrong timeline.
            let gap = (h.t_min - prev_end).abs();
            if gap > 1.0 / rate * 2.0 {
                return Err(Error::SegmentMismatch {
                    path: path.clone(),
                    detail: format!(
                        "starts {:.3} s from the end of the previous segment; \
                                     segments must be contiguous",
                        h.t_min - prev_end
                    ),
                });
            }
            prev_end = h.t_max;
        }
        let mut rd = Reader::open(path).path(path)?;
        while let Some(it) = rd.next_block() {
            if let Item::Data { id, samples } = it {
                if let Some(v) = buf.get_mut(&id) {
                    v.extend(
                        samples
                            .chunks_exact(2)
                            .map(|c| i16::from_le_bytes([c[0], c[1]])),
                    );
                }
                if min_len(&buf) >= (1 << 19) {
                    total += flush(&mut buf, &pos, &mut ser, false)?;
                }
            }
        }
        // Segments are contiguous, so the partial tail carries into the next file
        // rather than being flushed and realigned at each boundary.
    }
    total += flush(&mut buf, &pos, &mut ser, true)?;

    ser.data
        .new_attr::<f32>()
        .create("conversion")?
        .write_scalar(&conversion)?;
    ser.data
        .new_attr::<f32>()
        .create("resolution")?
        .write_scalar(&-1.0f32)?;
    attr_str_ds(&ser.data, "unit", "volts")?;

    Ok(Summary {
        samples: total,
        channels: nch,
        seconds: total as f64 / rate,
    })
}

fn min_len(buf: &BTreeMap<u16, Vec<i16>>) -> usize {
    buf.values().map(|v| v.len()).min().unwrap_or(0)
}

fn flush(
    buf: &mut BTreeMap<u16, Vec<i16>>,
    pos: &BTreeMap<u16, usize>,
    ser: &mut ElectricalSeries,
    all: bool,
) -> Result<usize> {
    let n = min_len(buf);
    let n = if all { n } else { n - n % CHUNK_ROWS };
    if n == 0 {
        return Ok(0);
    }
    let mut blk = vec![0i16; n * ser.cols];
    for (id, v) in buf.iter() {
        let c = pos[id];
        for t in 0..n {
            blk[t * ser.cols + c] = v[t];
        }
    }
    ser.append(&blk)?;
    for v in buf.values_mut() {
        v.drain(0..n);
    }
    Ok(n)
}

/// A continuation segment must be the same format, rate and channel set as the first.
fn verify_segment(
    path: &str,
    stream: &str,
    rate: f64,
    active: &[u16],
    chans: &BTreeMap<u16, crate::mpx::Channel>,
) -> Result<Header> {
    let mut rd = Reader::open(path).path(path)?;
    let mut hdr = Header::default();
    let mut seen: Vec<u16> = Vec::new();
    while let Some(it) = rd.next_block() {
        match it {
            Item::Header(h) => {
                if h.map_version > 0 {
                    hdr = h
                }
            }
            Item::Channel(c) => {
                if c.stream == stream {
                    if (c.rate_hz as f64 - rate).abs() > 1e-3 {
                        return Err(Error::SegmentMismatch {
                            path: path.into(),
                            detail: format!("sample rate {} != {}", c.rate_hz, rate),
                        });
                    }
                    seen.push(c.id)
                }
            }
            Item::Data { .. } => break,
            Item::Other => {}
        }
    }
    if hdr.map_version != 4 {
        return Err(Error::UnsupportedFormat {
            path: path.into(),
            version: hdr.map_version,
        });
    }
    for id in active {
        if !seen.contains(id) {
            return Err(Error::SegmentMismatch {
                path: path.into(),
                detail: format!("channel {} missing", chans[id].name),
            });
        }
    }
    Ok(hdr)
}
