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
        let mut fixture = None;
        let mut out = None;
        let mut it = args.into_iter();
        while let Some(arg) = it.next() {
            match arg.as_str() {
                "--fixture" => {
                    fixture = Some(it.next().ok_or("--fixture needs a fixture id")?);
                }
                "--out" => {
                    out = Some(PathBuf::from(it.next().ok_or("--out needs a directory")?));
                }
                other => return Err(format!("I do not know the argument {other:?}")),
            }
        }
        Ok(Self {
            fixture: fixture.ok_or("--fixture <id> is required")?,
            out: out.ok_or("--out <dir> is required")?,
        })
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
    /// Provenance only one engine has (the native archive acadsharp-rs ran
    /// against), as `(key, value)`. Empty for acadrust. Keys sort with the
    /// rest, so the file stays a function of the run.
    pub extra: Vec<(String, Option<String>)>,
}

impl RunInfo {
    pub fn to_json(&self) -> String {
        // Keys sort, so the file is a function of the run and nothing else.
        let mut value = serde_json::json!({
            "engine": self.engine,
            "engine_pin": self.engine_pin,
            "engine_head": self.engine_head,
            "fixture": self.fixture,
            "fixture_sha256": self.fixture_sha256,
            "target": self.target,
            "rayon_num_threads": self.rayon_num_threads,
        });
        if let Some(map) = value.as_object_mut() {
            for (k, v) in &self.extra {
                map.insert(k.clone(), serde_json::json!(v));
            }
        }
        let mut text = serde_json::to_string_pretty(&value).unwrap_or_default();
        text.push('\n');
        text
    }

    /// This run, for `fixture`: the target triple's pieces and the thread count
    /// as the process sees them.
    pub fn for_run(
        engine: &str,
        engine_pin: &str,
        engine_head: Option<String>,
        fixture: &crate::fixtures::DwgFixture,
    ) -> Self {
        Self::for_input(
            engine,
            engine_pin,
            engine_head,
            &fixture.id,
            &fixture.sha256,
        )
    }

    /// [`Self::for_run`] for any manifest row with an id and a sha256: a DWG
    /// fixture, or a DXF `[[oracle_input]]`.
    pub fn for_input(
        engine: &str,
        engine_pin: &str,
        engine_head: Option<String>,
        id: &str,
        sha256: &str,
    ) -> Self {
        Self {
            engine: engine.to_string(),
            engine_pin: engine_pin.to_string(),
            engine_head,
            fixture: id.to_string(),
            fixture_sha256: sha256.to_string(),
            target: format!("{}-{}", std::env::consts::ARCH, std::env::consts::OS),
            rayon_num_threads: std::env::var("RAYON_NUM_THREADS").ok(),
            extra: Vec::new(),
        }
    }
}

/// Where the three files for `fixture` and `engine` go under `out`.
pub fn output_paths(out: &Path, fixture: &str, engine: &str) -> [PathBuf; 3] {
    [
        out.join(format!("{fixture}.{engine}.jsonl")),
        out.join(format!("{fixture}.{engine}.src.jsonl")),
        out.join(format!("{fixture}.{engine}.run.json")),
    ]
}

/// The commit a checkout is at, read off `.git/HEAD` without running git: a
/// detached sha directly, or a branch followed through its ref file or
/// `packed-refs`. `None` when the directory is not a checkout this can read.
pub fn checkout_head(dir: &Path) -> Option<String> {
    let git = dir.join(".git");
    let head = std::fs::read_to_string(git.join("HEAD")).ok()?;
    let head = head.trim();
    let is_sha = |s: &str| {
        s.len() == 40
            && s.bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    };
    if is_sha(head) {
        return Some(head.to_string());
    }
    let reference = head.strip_prefix("ref: ")?;
    if let Ok(sha) = std::fs::read_to_string(git.join(reference)) {
        let sha = sha.trim();
        return is_sha(sha).then(|| sha.to_string());
    }
    let packed = std::fs::read_to_string(git.join("packed-refs")).ok()?;
    packed.lines().find_map(|l| {
        let (sha, name) = l.split_once(' ')?;
        (name == reference && is_sha(sha)).then(|| sha.to_string())
    })
}

/// Write `<id>.<engine>.jsonl`, `<id>.<engine>.src.jsonl` and
/// `<id>.<engine>.run.json`. Nothing is written if a record cannot be encoded.
pub fn write_emission(
    out: &Path,
    run: &RunInfo,
    emission: &Emission,
) -> Result<[PathBuf; 3], String> {
    // Everything is rendered before anything is written, so a record that
    // cannot be encoded leaves no half-written run behind.
    let jsonl = crate::canonical::render_stream(&emission.records).map_err(|e| e.to_string())?;
    let sidecar = crate::canonical::render_sidecar(&emission.sidecar);
    let paths = output_paths(out, &run.fixture, &run.engine);
    std::fs::create_dir_all(out).map_err(|e| format!("{}: {e}", out.display()))?;
    for (path, text) in paths.iter().zip([jsonl, sidecar, run.to_json()]) {
        std::fs::write(path, text).map_err(|e| format!("{}: {e}", path.display()))?;
    }
    Ok(paths)
}

/// What `run.json` says about the native archive an `acadsharp` run decoded
/// with, read from the unpacked archive the binary was linked against
/// (`metadata/LINKINFO.json`, `BUILDINFO.json`, `CHECKSUMS.txt`).
///
/// Two archives can claim the same `artifact_version`: a rebuilt one calls
/// itself `3.7.1-viprs.2`, which is the string a future release will carry too.
/// So the label that tells them apart is the version plus the commit and shim
/// hash where the archive has them, and the shared library's own sha256 from its
/// checksum list where it does not. `ACADSHARP_ARCHIVE_SHA256`, which the gate
/// sets after verifying the download, adds the tarball's hash. Everything here is
/// a function of the archive, so two runs on one machine write the same bytes.
pub fn native_provenance(dir: Option<&str>) -> Vec<(String, Option<String>)> {
    let mut out: Vec<(String, Option<String>)> = Vec::new();
    let read = |rel: &str| {
        dir.and_then(|d| std::fs::read_to_string(Path::new(d).join(rel)).ok())
            .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
    };
    let link = read("metadata/LINKINFO.json");
    let build = read("metadata/BUILDINFO.json");
    let field = |v: &Option<serde_json::Value>, k: &str| {
        v.as_ref()
            .and_then(|v| v.get(k))
            .and_then(|v| v.as_str())
            .map(str::to_string)
    };
    let lib_sha = link
        .as_ref()
        .and_then(|l| l.get("shared_library"))
        .and_then(|v| v.as_str())
        .and_then(|lib| {
            let sums =
                std::fs::read_to_string(Path::new(dir?).join("metadata/CHECKSUMS.txt")).ok()?;
            sums.lines().find_map(|l| {
                let (sha, name) = l.split_once("  ")?;
                (name.trim() == lib).then(|| sha.to_string())
            })
        });
    out.push((
        "native_abi_header_sha256".into(),
        field(&link, "abi_header_sha256"),
    ));
    out.push((
        "native_acadsharp_version".into(),
        field(&link, "acadsharp_version"),
    ));
    out.push((
        "native_archive_sha256".into(),
        std::env::var("ACADSHARP_ARCHIVE_SHA256")
            .ok()
            .filter(|s| !s.is_empty()),
    ));
    out.push((
        "native_artifact_version".into(),
        field(&link, "artifact_version"),
    ));
    out.push((
        "native_driver_commit".into(),
        field(&build, "driver_commit"),
    ));
    out.push(("native_library_sha256".into(), lib_sha));
    out.push(("native_shim_sha256".into(), field(&link, "shim_sha256")));
    out.push(("native_target".into(), field(&link, "target")));
    // A rebuilt archive built from `git archive` cannot read its own commit, so
    // the gate can say which one it is (`--dep-commit`); an archive that knows
    // wins.
    out.push((
        "native_viprs_dep_commit".into(),
        field(&link, "viprs_dep_commit").or_else(|| {
            std::env::var("ACADSHARP_VIPRS_DEP_COMMIT")
                .ok()
                .filter(|s| !s.is_empty())
        }),
    ));
    out
}
