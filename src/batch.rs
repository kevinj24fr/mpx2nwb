//! Recording discovery and grouping.
//!
//! Acquisition splits a long recording at a size limit, producing `NAME_0001.mpx`,
//! `NAME_0002.mpx`, ... These are contiguous halves of one recording, not separate
//! trials, so they are grouped and converted into a single NWB file.

use crate::mpx;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct Recording {
    /// Segments in acquisition order.
    pub segments: Vec<PathBuf>,
    /// Basename with the segment suffix removed.
    pub stem: String,
    /// Path of the containing directory relative to the search root.
    pub rel_dir: PathBuf,
}

impl Recording {
    /// Subject taken from the Nth `_`-separated field of the stem, 1-based.
    pub fn subject_field(&self, n: usize) -> Option<String> {
        self.stem
            .split('_')
            .nth(n.saturating_sub(1))
            .map(|s| s.to_string())
    }
    pub fn total_bytes(&self) -> u64 {
        self.segments
            .iter()
            .filter_map(|p| std::fs::metadata(p).ok())
            .map(|m| m.len())
            .sum()
    }
}

/// Strip exactly one four-digit segment index, and a separating underscore if present.
///
/// Acquisition is inconsistent about the separator, so both `NAME_0001` and `NAME0001`
/// occur. Stripping is deliberately not greedy: `Control_R1_Right_Day10001` is
/// `Day1` + `0001`, not `Day` + `10001`, and a handful of files carry a doubled suffix
/// that no rule disambiguates safely. Under-grouping merely leaves a recording split;
/// over-grouping would merge unrelated recordings into one timeline, so when the name
/// is ambiguous this errs toward leaving it alone.
fn strip_segment(stem: &str) -> &str {
    let b = stem.as_bytes();
    if b.len() < 4 || !b[b.len() - 4..].iter().all(|c| c.is_ascii_digit()) {
        return stem;
    }
    let s = &stem[..stem.len() - 4];
    s.strip_suffix('_').unwrap_or(s)
}

/// Split a segment list into contiguous runs on the acquisition clock.
///
/// A shared basename does not guarantee continuity: an operator who stops and restarts
/// produces `_0003` sixty seconds after `_0002` ended. Those are separate recordings and
/// merging them would silently shift every later sample, so each contiguous run becomes
/// its own output file.
pub fn contiguous_runs(segments: &[PathBuf]) -> Vec<Vec<PathBuf>> {
    let mut runs: Vec<Vec<PathBuf>> = Vec::new();
    let mut prev_end: Option<f64> = None;
    for p in segments {
        let h = mpx::read_header(&p.display().to_string()).ok();
        let (tmin, tmax) = h
            .map(|h| (h.t_min, h.t_max))
            .unwrap_or((f64::NAN, f64::NAN));
        let starts_new = match (prev_end, tmin.is_finite()) {
            (Some(e), true) => (tmin - e).abs() > 0.001,
            (None, _) => true,
            (Some(_), false) => true,
        };
        if starts_new || runs.is_empty() {
            runs.push(Vec::new());
        }
        runs.last_mut().unwrap().push(p.clone());
        prev_end = if tmax.is_finite() { Some(tmax) } else { None };
    }
    runs
}

pub fn discover(root: &Path) -> std::io::Result<Vec<Recording>> {
    let mut files: Vec<PathBuf> = Vec::new();
    walk(root, &mut files)?;
    files.sort();
    let mut out: Vec<Recording> = Vec::new();
    for f in files {
        let stem = f
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        let key = strip_segment(&stem).to_string();
        let rel_dir = f
            .parent()
            .and_then(|p| p.strip_prefix(root).ok())
            .map(|p| p.to_path_buf())
            .unwrap_or_default();
        match out.last_mut() {
            Some(r) if r.stem == key && r.rel_dir == rel_dir => r.segments.push(f),
            _ => out.push(Recording {
                segments: vec![f],
                stem: key,
                rel_dir,
            }),
        }
    }
    Ok(out)
}

fn walk(dir: &Path, acc: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for e in std::fs::read_dir(dir)? {
        let p = e?.path();
        if p.is_dir() {
            walk(&p, acc)?;
        } else if p
            .extension()
            .map(|x| x.eq_ignore_ascii_case("mpx"))
            .unwrap_or(false)
        {
            acc.push(p);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::strip_segment;
    #[test]
    fn strips_segment_suffixes() {
        assert_eq!(
            strip_segment("LPS_R6_Left_Day_02_0001"),
            "LPS_R6_Left_Day_02"
        );
        assert_eq!(
            strip_segment("Control_R1_Right_Day10001"),
            "Control_R1_Right_Day1"
        );
        // ambiguous doubled suffix: left as its own group rather than risking a bad merge
        assert_eq!(
            strip_segment("LPS_R6_Left_Day_05_0010010002"),
            "LPS_R6_Left_Day_05_001001"
        );
        assert_eq!(strip_segment("no_digits_here"), "no_digits_here");
        assert_eq!(strip_segment("Rat01_L_0001"), "Rat01_L");
    }
}
