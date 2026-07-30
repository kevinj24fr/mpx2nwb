//! Behaviour that must hold regardless of what data is available locally.

use mpx2nwb::cli;

fn args(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

#[test]
fn help_and_version_exit_without_work() {
    assert!(cli::parse(args(&["mpx2nwb", "--help"]).into_iter())
        .unwrap()
        .is_none());
    assert!(cli::parse(args(&["mpx2nwb", "-V"]).into_iter())
        .unwrap()
        .is_none());
}

#[test]
fn collects_multiple_inputs_before_flags() {
    let a = cli::parse(args(&["mpx2nwb", "a.mpx", "b.mpx", "-o", "out.nwb"]).into_iter())
        .unwrap()
        .unwrap();
    assert_eq!(a.inputs, vec!["a.mpx", "b.mpx"]);
    assert_eq!(a.output.as_deref(), Some("out.nwb"));
}

#[test]
fn batch_requires_outdir() {
    assert!(cli::parse(args(&["mpx2nwb", "--batch", "in"]).into_iter()).is_err());
    assert!(cli::parse(args(&["mpx2nwb", "--batch", "in", "--outdir", "out"]).into_iter()).is_ok());
}

#[test]
fn rejects_unknown_flags_rather_than_ignoring_them() {
    assert!(cli::parse(args(&["mpx2nwb", "a.mpx", "--nope"]).into_iter()).is_err());
}

#[test]
fn defaults_to_the_only_unprocessed_stream() {
    let a = cli::parse(args(&["mpx2nwb", "a.mpx"]).into_iter())
        .unwrap()
        .unwrap();
    assert_eq!(a.stream, "RAW");
}
