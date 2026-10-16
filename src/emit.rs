//! The harness every emitter binary shares: arguments, the three output files,
//! and `run.json`.
//!
//! An emitter is a process, not a library call. A reader that aborts on a
//! malformed file takes only its own row down; each run stamps itself with
//! provenance; and the outputs are files a human can diff and CI can upload.
//! `emit-acadrust` (R3.2) and `emit-acadsharp` (R3.1) are each an adapter and
//! nothing else: they turn a document into an [`Emission`] and call
//! [`write_emission`].

use std::path::{Path, PathBuf};

use crate::canonical::{Record, SidecarEntry};

/// What an emitter produced for one fixture.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Emission {
    /// Schema 1 records, in stream order.
    pub records: Vec<Record>,
    /// One entry per record, naming the entity it came from.
    pub sidecar: Vec<SidecarEntry>,
}

/// What an emitter binary was asked to do.
#[derive(Debug, Clone, PartialEq)]
pub struct EmitArgs {
    /// A `[[fixture]]` id.
    pub fixture: String,
    /// The directory the three files go in.
    pub out: PathBuf,
}

impl EmitArgs {
    /// `--fixture <id> --out <dir>`, in either order.
    pub fn parse<I: IntoIterator<Item = String>>(args: I) -> Result<Self, String> {
        let _ = args;
        todo!("R3.2")
    }
}

/// What a run was, written beside its output as `<id>.<engine>.run.json`.
#[derive(Debug, Clone, PartialEq)]
pub struct RunInfo {
    pub engine: String,
    /// The commit the suite pins the engine at (`ACADRUST_REV`).
    pub engine_pin: String,
    /// The commit the engine checkout beside the suite is actually at, when
    /// that can be read.
    pub engine_head: Option<String>,
    pub fixture: String,
    pub fixture_sha256: String,
    pub target: String,
    /// `RAYON_NUM_THREADS` as the process saw it, or `None` when unset.
    pub rayon_num_threads: Option<String>,
}

impl RunInfo {
    pub fn to_json(&self) -> String {
        todo!("R3.2")
    }
}

/// Where the three files for `fixture` and `engine` go under `out`.
pub fn output_paths(out: &Path, fixture: &str, engine: &str) -> [PathBuf; 3] {
    let _ = (out, fixture, engine);
    todo!("R3.2")
}

/// Write `<id>.<engine>.jsonl`, `<id>.<engine>.src.jsonl` and
/// `<id>.<engine>.run.json`. Nothing is written if a record cannot be encoded.
pub fn write_emission(
    out: &Path,
    run: &RunInfo,
    emission: &Emission,
) -> Result<[PathBuf; 3], String> {
    let _ = (out, run, emission);
    todo!("R3.2")
}
