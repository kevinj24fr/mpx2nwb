//! Command-line parsing. Deliberately dependency-free: the argument surface is small
//! enough that a parser crate would add more to the build than it removes from here.

use crate::error::{Error, Result};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub const HELP: &str = "\
mpx2nwb -- convert Alpha Omega AlphaLab SnR .mpx recordings to NWB 2.7

USAGE:
    mpx2nwb <INPUT.mpx>... -o <OUTPUT.nwb> [OPTIONS]
    mpx2nwb --batch <INPUT_DIR> --outdir <OUTPUT_DIR> [OPTIONS]

Multiple inputs are concatenated in the order given. Use this for _0001/_0002
continuation segments, which are contiguous halves of a single recording rather
than separate trials. Batch mode discovers and groups them automatically.

OPTIONS:
    -o, --output <FILE>       Output path (single-conversion mode)
        --batch <DIR>         Convert every recording found under DIR, recursively
        --outdir <DIR>        Destination for batch mode; session folders are mirrored
        --stream <NAME>       Acquisition stream to export [default: RAW]
        --deflate <0-9>       Compression level [default: 4]
        --subject <ID>        Subject identifier
        --subject-field <N>   Batch mode: take subject from the Nth '_'-separated
                              field of the filename (1-based)
        --session-id <ID>     Session identifier (single-conversion mode)
        --description <TEXT>  Session description
        --dry-run             Batch mode: list what would be converted, convert nothing
    -j, --jobs <N>            Batch mode: parallel conversions [default: 4]
    -h, --help                Print help
    -V, --version             Print version

NOTES:
    Only map format 4 is accepted; other versions are rejected rather than guessed at.
    Samples are stored as raw int16 with an NWB `conversion` factor of
    bit_resolution / gain in volts. The uV/bit field in the .mpx channel block is
    referred to the ADC and must be divided by the gain field -- using it directly
    inflates every amplitude by the gain.
";

#[derive(Debug, Clone)]
pub struct Args {
    pub inputs: Vec<String>,
    pub output: Option<String>,
    pub batch: Option<String>,
    pub outdir: Option<String>,
    pub stream: String,
    pub deflate: u8,
    pub subject: Option<String>,
    pub subject_field: Option<usize>,
    pub session_id: Option<String>,
    pub description: String,
    pub dry_run: bool,
    pub jobs: usize,
}

impl Default for Args {
    fn default() -> Self {
        Args {
            inputs: Vec::new(),
            output: None,
            batch: None,
            outdir: None,
            stream: "RAW".into(),
            deflate: 4,
            subject: None,
            subject_field: None,
            session_id: None,
            description: "Extracellular electrophysiology".into(),
            dry_run: false,
            jobs: 4,
        }
    }
}

/// Returns `Ok(None)` when the program should exit after printing help or version.
pub fn parse<I: Iterator<Item = String>>(argv: I) -> Result<Option<Args>> {
    let a: Vec<String> = argv.collect();
    if a.len() < 2 {
        print!("{}", HELP);
        return Ok(None);
    }
    let mut r = Args::default();
    let mut i = 1;
    while i < a.len() && !a[i].starts_with('-') {
        r.inputs.push(a[i].clone());
        i += 1
    }
    let need = |i: usize, a: &[String], what: &str| -> Result<String> {
        a.get(i + 1)
            .cloned()
            .ok_or_else(|| Error::Usage(format!("{} needs a value", what)))
    };
    while i < a.len() {
        match a[i].as_str() {
            "-h" | "--help" => {
                print!("{}", HELP);
                return Ok(None);
            }
            "-V" | "--version" => {
                println!("mpx2nwb {}", VERSION);
                return Ok(None);
            }
            "-o" | "--output" => {
                r.output = Some(need(i, &a, "-o")?);
                i += 1
            }
            "--batch" => {
                r.batch = Some(need(i, &a, "--batch")?);
                i += 1
            }
            "--outdir" => {
                r.outdir = Some(need(i, &a, "--outdir")?);
                i += 1
            }
            "--stream" => {
                r.stream = need(i, &a, "--stream")?;
                i += 1
            }
            "--deflate" => {
                r.deflate = need(i, &a, "--deflate")?
                    .parse()
                    .map_err(|_| Error::Usage("--deflate expects 0-9".into()))?;
                i += 1
            }
            "--subject" => {
                r.subject = Some(need(i, &a, "--subject")?);
                i += 1
            }
            "--subject-field" => {
                r.subject_field = Some(
                    need(i, &a, "--subject-field")?
                        .parse()
                        .map_err(|_| Error::Usage("--subject-field expects a number".into()))?,
                );
                i += 1
            }
            "--session-id" => {
                r.session_id = Some(need(i, &a, "--session-id")?);
                i += 1
            }
            "--description" => {
                r.description = need(i, &a, "--description")?;
                i += 1
            }
            "--dry-run" => r.dry_run = true,
            "-j" | "--jobs" => {
                r.jobs = need(i, &a, "-j")?
                    .parse::<usize>()
                    .map_err(|_| Error::Usage("--jobs expects a number".into()))?
                    .max(1);
                i += 1
            }
            other => return Err(Error::Usage(format!("unknown argument: {}", other))),
        }
        i += 1;
    }
    if r.batch.is_some() && r.outdir.is_none() {
        return Err(Error::Usage("--batch requires --outdir".into()));
    }
    if r.batch.is_none() && r.inputs.is_empty() {
        return Err(Error::Usage("no input files given (see --help)".into()));
    }
    Ok(Some(r))
}
