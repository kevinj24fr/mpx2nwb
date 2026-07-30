use mpx2nwb::{batch, cli, convert, ConvertOptions, Error, Result};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

fn main() {
    let args = match cli::parse(std::env::args()) {
        Ok(Some(a)) => a,
        Ok(None) => return,
        Err(e) => {
            eprintln!("mpx2nwb: {}", e);
            std::process::exit(2)
        }
    };
    let code = match run(&args) {
        Ok(fail) => {
            if fail == 0 {
                0
            } else {
                1
            }
        }
        Err(e) => {
            eprintln!("mpx2nwb: {}", e);
            1
        }
    };
    std::process::exit(code);
}

fn run(a: &cli::Args) -> Result<usize> {
    let opts = ConvertOptions {
        stream: a.stream.clone(),
        deflate: a.deflate,
        subject: a.subject.clone(),
        session_id: a.session_id.clone(),
        description: a.description.clone(),
    };
    match &a.batch {
        None => {
            let out = a
                .output
                .clone()
                .unwrap_or_else(|| a.inputs[0].trim_end_matches(".mpx").to_string() + ".nwb");
            let s = convert(&a.inputs, &out, &opts)?;
            eprintln!(
                "{}  [{} ch x {} samples, {:.1} s, {} segment(s)]",
                out,
                s.channels,
                s.samples,
                s.seconds,
                a.inputs.len()
            );
            Ok(0)
        }
        Some(dir) => run_batch(a, &opts, Path::new(dir)),
    }
}

fn run_batch(a: &cli::Args, opts: &ConvertOptions, root: &Path) -> Result<usize> {
    let outdir = PathBuf::from(a.outdir.as_ref().unwrap());
    let found = batch::discover(root).map_err(|e| Error::Io {
        path: root.display().to_string(),
        source: e,
    })?;

    // A shared basename does not imply contiguity: an operator who stops and restarts
    // produces a segment that begins well after the previous one ended. Split first,
    // so each contiguous run becomes its own recording rather than a bad merge.
    let mut recs: Vec<(String, Vec<PathBuf>, PathBuf)> = Vec::new();
    for r in &found {
        let runs = batch::contiguous_runs(&r.segments);
        let n = runs.len();
        for run in runs {
            let name = if n == 1 {
                r.stem.clone()
            } else {
                let stem = run[0]
                    .file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_default();
                let idx = stem.rsplit('_').next().unwrap_or("").to_string();
                format!("{}__{}", r.stem, idx)
            };
            recs.push((name, run, r.rel_dir.clone()));
        }
    }

    let seg_total: usize = recs.iter().map(|r| r.1.len()).sum();
    let total_bytes: u64 = recs
        .iter()
        .flat_map(|r| &r.1)
        .filter_map(|p| std::fs::metadata(p).ok())
        .map(|m| m.len())
        .sum();
    eprintln!(
        "discovered {} contiguous recordings in {} files ({:.1} GB)",
        recs.len(),
        seg_total,
        total_bytes as f64 / 1e9
    );

    if a.dry_run {
        for (name, segs, rel) in &recs {
            let mb: u64 = segs
                .iter()
                .filter_map(|p| std::fs::metadata(p).ok())
                .map(|m| m.len())
                .sum();
            eprintln!(
                "  {:<52} {:>2} segment(s)  {:>7.1} MB  -> {}",
                name,
                segs.len(),
                mb as f64 / 1e6,
                outdir.join(rel).join(format!("{}.nwb", name)).display()
            );
        }
        return Ok(0);
    }

    let next = AtomicUsize::new(0);
    let done = AtomicUsize::new(0);
    let failed = AtomicUsize::new(0);
    let n = recs.len();
    std::thread::scope(|sc| {
        for _ in 0..a.jobs.min(n.max(1)) {
            sc.spawn(|| loop {
                let i = next.fetch_add(1, Ordering::SeqCst);
                if i >= n {
                    break;
                }
                let (name, segs, rel) = &recs[i];
                let dst_dir = outdir.join(rel);
                if let Err(e) = std::fs::create_dir_all(&dst_dir) {
                    eprintln!("  FAIL {}: {}", name, e);
                    failed.fetch_add(1, Ordering::SeqCst);
                    continue;
                }
                let dst = dst_dir.join(format!("{}.nwb", name));
                let mut o = opts.clone();
                o.session_id = Some(name.clone());
                if let Some(f) = a.subject_field {
                    o.subject = name
                        .split('_')
                        .nth(f.saturating_sub(1))
                        .map(|s| s.to_string());
                }
                let ins: Vec<String> = segs.iter().map(|p| p.display().to_string()).collect();
                match convert(&ins, &dst.display().to_string(), &o) {
                    Ok(s) => {
                        let d = done.fetch_add(1, Ordering::SeqCst) + 1;
                        eprintln!(
                            "  [{}/{}] {:<48} {} ch  {:>7.1} s",
                            d, n, name, s.channels, s.seconds
                        );
                    }
                    Err(e) => {
                        let _ = std::fs::remove_file(&dst);
                        eprintln!("  FAIL {}: {}", name, e);
                        failed.fetch_add(1, Ordering::SeqCst);
                    }
                }
            });
        }
    });
    let f = failed.load(Ordering::SeqCst);
    eprintln!(
        "converted {} of {} recordings{}",
        done.load(Ordering::SeqCst),
        n,
        if f > 0 {
            format!(", {} failed", f)
        } else {
            String::new()
        }
    );
    Ok(f)
}
