# mpx2nwb

[![license](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

Convert Alpha Omega AlphaLab SnR `.mpx` electrophysiology recordings to
[NWB 2.7](https://nwb-schema.readthedocs.io/). Pure Rust — no Python in the
conversion path. Output validates against `pynwb` with zero errors.

```sh
mpx2nwb --batch ./raw --outdir ./nwb --subject-field 2 --jobs 5
```

Converts a 1 GB recording in ~13 s at 3.6× compression, merges split segments, and
refuses to merge across recording gaps.

## Why

`.mpx` has no public specification and no maintained Rust reader. The existing route
into open formats runs through Python (`neo` → `spikeinterface` → `neuroconv`), which
works but is slow over large archives and pulls a heavy dependency tree in just to
transcode. This does the transcode step alone, quickly, then gets out of the way so the
science runs on standard tooling.

## Install

### Prerequisites

[Rust](https://rustup.rs/) 1.74 or newer, and libhdf5:

```sh
brew install hdf5                    # macOS
sudo apt-get install libhdf5-dev     # Debian / Ubuntu
sudo dnf install hdf5-devel          # Fedora / RHEL
```

### From GitHub

Installs the binary to `~/.cargo/bin/mpx2nwb`, no clone needed:

```sh
cargo install --git https://github.com/USER/mpx2nwb
```

Pin to a tag or commit for a reproducible install:

```sh
cargo install --git https://github.com/USER/mpx2nwb --tag v0.1.0
cargo install --git https://github.com/USER/mpx2nwb --rev <commit-sha>
```

To update later, add `--force`. To remove it, `cargo uninstall mpx2nwb`.

Check it worked:

```sh
mpx2nwb --version
```

If the command isn't found, `~/.cargo/bin` is not on your `PATH`; rustup normally adds
it, but a new shell may be needed.

### From a clone

Preferable if you want to run the tests or modify anything:

```sh
git clone https://github.com/USER/mpx2nwb
cd mpx2nwb
cargo build --release        # binary at target/release/mpx2nwb
cargo test                   # optional
cargo install --path .       # optional, puts it on PATH
```

### If the build can't find HDF5

The build script locates libhdf5 through `pkg-config`, which works out of the box for
all three package managers above. If it fails — a non-standard prefix, or Homebrew on
an Intel Mac — point it at the install directly:

```sh
HDF5_DIR=$(brew --prefix hdf5) cargo install --git https://github.com/USER/mpx2nwb
```

Note that `hdf5-metno-sys` requires HDF5 1.10 or newer; the older `hdf5` crate rejects
1.14, which is why this uses the maintained fork.

## Use

Single recording:

```sh
mpx2nwb recording_0001.mpx -o recording.nwb --subject R6
```

Continuation segments merged into one file:

```sh
mpx2nwb rec_0001.mpx rec_0002.mpx -o rec.nwb
```

A whole archive, mirroring the session folder layout:

```sh
mpx2nwb --batch ./raw --outdir ./nwb --subject-field 2 --jobs 5
mpx2nwb --batch ./raw --outdir ./nwb --dry-run        # list first
```

## Behaviour worth knowing

**Segment merging.** Acquisition splits long recordings at a size limit into
`NAME_0001.mpx`, `NAME_0002.mpx`, … These are contiguous halves of one recording, not
separate trials. Batch mode groups them automatically; segments are verified to have the
same format, sample rate and channel set, **and to abut on the acquisition clock** — a
gap is an error rather than a silent timeline shift.

**Amplitude scaling.** Samples are stored as raw `int16` with the NWB `conversion`
attribute set to `bit_resolution / gain` in volts. The µV/bit field in the `.mpx` channel
block is referred to the ADC and must be divided by the gain field; using it directly
inflates every amplitude by the gain, which on a typical rig is 1000×. Storing raw
integers plus an explicit conversion keeps full precision and makes the scale auditable.

**Stream selection.** `--stream` defaults to `RAW`, the only genuinely unprocessed
stream. `SPK` and `LFP` are hardware-filtered copies whose filter settings are not
recorded in the file, and `SEG` is an operator-set live threshold that typically varies
between sessions. Derive those yourself from `RAW` if you want them reproducible.

**Empty channels are dropped.** The acquisition template declares a fixed electrode
count; a rig may have wired fewer. Channels that carry no samples are omitted rather
than written as zero columns.

**Format gating.** Only map format 4 is accepted. Other versions are rejected rather
than guessed at.

**Deterministic output.** Object identifiers are derived from filename, session start
and stream, so re-running on the same input reproduces the same file.

## Output layout

```
/acquisition/ElectricalSeries/data          int16 [time, channel], chunked, shuffle+deflate
                             /starting_time  f64 with `rate`
                             /electrodes     DynamicTableRegion -> electrodes table
/general/devices/<device>
/general/extracellular_ephys/array           ElectrodeGroup (device is a soft link)
/general/extracellular_ephys/electrodes      DynamicTable: location, group, group_name,
                                             channel_name
```

Typical compression is 3.6× (1020 MB → 284 MB) at `--deflate 4`.

## Format notes

`.mpx` blocks are `u16 length | u8 type | body`; type `h` is the file header, `2`
declares a channel, `5` carries continuous samples. Field offsets are documented in
[`src/mpx.rs`](src/mpx.rs). Two details that cost time to find:

- `ElectrodeGroup/device` must be an HDF5 **soft link**, not a scalar object reference.
  hdmf's scalar-reference reader only dereferences Dataset targets.
- HDF5 requires the shuffle filter to be declared **before** deflate.

## Verification

Samples read back through `pynwb` are bit-identical to a direct `.mpx` read, and
`pynwb.validate` reports no errors. `cargo test` covers CLI parsing and the segment
grouping rule.

## Publishing checklist

Before the first push, replace `USER` in `Cargo.toml` (`repository`, `homepage`) and in
the badge URLs above, and set a real `authors` entry. Then set the repository topics so
the tool is findable by the format names people actually search for:

```sh
gh repo edit --add-topic nwb \
             --add-topic electrophysiology \
             --add-topic neuroscience \
             --add-topic alpha-omega \
             --add-topic hdf5 \
             --add-topic rust \
             --add-topic neurodata-without-borders
```

`cargo publish --dry-run` before publishing to crates.io.

## License

MIT. See [LICENSE](LICENSE).
