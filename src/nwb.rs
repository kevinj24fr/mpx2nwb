//! Minimal NWB 2.7 writer: enough of the schema to hold an ecephys acquisition,
//! written directly so no Python is needed in the conversion path.

use hdf5::types::VarLenUnicode;
use hdf5::{Dataset, Extent, Extents, Group};
use hdf5::{Location, ObjectReference1};
use hdf5_metno as hdf5;
use std::str::FromStr;

pub const NWB_VERSION: &str = "2.7.0";

pub fn vstr(s: &str) -> VarLenUnicode {
    VarLenUnicode::from_str(s).unwrap()
}

/// Write a scalar string dataset (NWB stores most metadata this way).
pub fn str_ds(g: &Group, name: &str, val: &str) -> hdf5::Result<Dataset> {
    let d = g.new_dataset::<VarLenUnicode>().shape(()).create(name)?;
    d.write_scalar(&vstr(val))?;
    Ok(d)
}
pub fn str_arr(g: &Group, name: &str, vals: &[String]) -> hdf5::Result<Dataset> {
    let v: Vec<VarLenUnicode> = vals.iter().map(|s| vstr(s)).collect();
    let d = g
        .new_dataset::<VarLenUnicode>()
        .shape([v.len()])
        .create(name)?;
    d.write_raw(&v)?;
    Ok(d)
}
pub fn attr_str(o: &Location, k: &str, v: &str) -> hdf5::Result<()> {
    o.new_attr::<VarLenUnicode>()
        .create(k)?
        .write_scalar(&vstr(v))
}

/// Stamp the attributes that make an HDF5 object an NWB neurodata_type.
pub fn typed(g: &Group, ns: &str, ty: &str, oid: &str) -> hdf5::Result<()> {
    attr_str(g, "namespace", ns)?;
    attr_str(g, "neurodata_type", ty)?;
    attr_str(g, "object_id", oid)?;
    Ok(())
}

/// Deterministic UUID-shaped id derived from a seed, so re-running the converter on
/// the same input reproduces the same file byte-for-byte in these fields.
pub fn det_uuid(seed: &str) -> String {
    let mut h: u128 = 0x6c62_272e_07bb_0142_62b8_2175_6295_c58du128;
    for b in seed.as_bytes() {
        h ^= *b as u128;
        h = h.wrapping_mul(0x0000_0000_0100_0000_0000_0000_0000_013bu128);
    }
    let x = h.to_be_bytes();
    let hx = |r: &[u8]| r.iter().map(|b| format!("{:02x}", b)).collect::<String>();
    format!(
        "{}-{}-4{}-a{}-{}",
        hx(&x[0..4]),
        hx(&x[4..6]),
        &hx(&x[6..8])[1..],
        &hx(&x[8..10])[1..],
        hx(&x[10..16])
    )
}

pub struct ElectricalSeries {
    pub data: Dataset,
    pub rows: usize,
    pub cols: usize,
}

/// Create a growable [time, channel] int16 dataset with chunking and deflate.
pub fn make_series(
    g: &Group,
    cols: usize,
    chunk_rows: usize,
    level: u8,
) -> hdf5::Result<ElectricalSeries> {
    let ext: Extents = vec![Extent::resizable(0), Extent::fixed(cols)].into();
    let d = g
        .new_dataset::<i16>()
        .shape(ext)
        .chunk([chunk_rows, cols])
        .shuffle()
        .deflate(level)
        .create("data")?;
    Ok(ElectricalSeries {
        data: d,
        rows: 0,
        cols,
    })
}

impl ElectricalSeries {
    /// Append a [n, cols] row-major block.
    pub fn append(&mut self, block: &[i16]) -> hdf5::Result<()> {
        let n = block.len() / self.cols;
        if n == 0 {
            return Ok(());
        }
        let view = ndarray::ArrayView2::from_shape((n, self.cols), block)
            .expect("block length is a multiple of cols");
        self.data.resize([self.rows + n, self.cols])?;
        self.data
            .write_slice(view, (self.rows..self.rows + n, 0..self.cols))?;
        self.rows += n;
        Ok(())
    }
}

pub fn objref(loc: &Group, name: &str) -> hdf5::Result<ObjectReference1> {
    loc.reference(name)
}

/// Same as `typed`/`attr_str` but for datasets, which are a distinct handle type.
pub fn typed_ds(d: &Dataset, ns: &str, ty: &str, oid: &str) -> hdf5::Result<()> {
    attr_str_ds(d, "namespace", ns)?;
    attr_str_ds(d, "neurodata_type", ty)?;
    attr_str_ds(d, "object_id", oid)
}
pub fn attr_str_ds(d: &Dataset, k: &str, v: &str) -> hdf5::Result<()> {
    d.new_attr::<VarLenUnicode>()
        .create(k)?
        .write_scalar(&vstr(v))
}
