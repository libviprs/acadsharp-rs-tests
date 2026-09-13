//! Where every ACadSharp test file stands relative to this suite.
//!
//! ACadSharp's own tests are the closest thing to a specification of what a DWG
//! reader has to do. This module reads `upstream_parity/manifest.toml`, which
//! classifies all of them, so upstream coverage stops drifting silently: a bump
//! that adds a test file fails the inventory until somebody decides what that
//! file means here.
//!
//! It classifies, it does not port. H4.2 turns rows into real Rust tests.
//!
//! The interesting part is [`validate`], which is a plain function over a parsed
//! manifest rather than a pile of assertions inside a test. That is deliberate:
//! a check that only ever runs against the one real manifest cannot be shown to
//! work, and this repository has shipped that shape of test before. Feeding it
//! a manifest built to be wrong is how its teeth get demonstrated.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use serde::Deserialize;

const MANIFEST_TOML: &str = include_str!("../upstream_parity/manifest.toml");
const PARITY_DIR: &str = "upstream_parity";

/// What a classified upstream test file can be.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    /// A Rust test reproduces it. `rust` says which files.
    Mirrored,
    /// Covered end to end by the JSONL comparison against the .NET oracle.
    Differential,
    /// Checked against ACadSharp itself rather than through anything rendered.
    NativeConformance,
    /// Out of scope, with a reason. Never counts toward coverage.
    NotApplicable,
}

impl Status {
    /// Whether this status counts in the coverage denominator.
    ///
    /// `not_applicable` does not, and that is the entire reason it exists as a
    /// separate status. Writer suites and DXF suites are not gaps. A coverage
    /// number that counted them would fall every time upstream added a writer
    /// test, which tells nobody anything about this crate.
    pub fn is_applicable(self) -> bool {
        self != Self::NotApplicable
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Mirrored => "mirrored",
            Self::Differential => "differential",
            Self::NativeConformance => "native_conformance",
            Self::NotApplicable => "not_applicable",
        }
    }
}

/// The upstream revision this classification was made against.
#[derive(Debug, Clone, Deserialize)]
pub struct Upstream {
    pub repository: String,
    pub version: String,
    /// A full 40-character commit sha. Never a branch.
    pub commit: String,
    /// Where test files live in the upstream repository. Every `path` below is
    /// relative to this.
    pub test_root: String,
}

/// One classified upstream test file.
#[derive(Debug, Clone, Deserialize)]
pub struct TestFile {
    /// Path relative to [`Upstream::test_root`].
    pub path: String,
    pub status: Status,
    /// The Rust files that reproduce it. Required when `mirrored`.
    #[serde(default)]
    pub rust: Vec<String>,
    /// Where the mirrored test will go when H4.2 writes it.
    ///
    /// Separate from `rust` on purpose. A row is `mirrored` only once real
    /// files exist, so this field records the plan without letting the manifest
    /// claim coverage that is not there yet. When the planned file appears, the
    /// row has to be promoted, and [`validate`] refuses the row until it is.
    #[serde(default)]
    pub planned_rust: Option<String>,
    /// Why, for every status other than `mirrored`.
    #[serde(default)]
    pub reason: Option<String>,
}

/// Upstream data files usable as a second oracle.
#[derive(Debug, Clone, Deserialize)]
pub struct Baseline {
    pub kind: String,
    pub files: Vec<String>,
    pub status: Status,
    pub reason: String,
}

#[derive(Debug, Deserialize)]
pub struct Manifest {
    pub upstream: Upstream,
    #[serde(default, rename = "test_file")]
    pub test_files: Vec<TestFile>,
    #[serde(default, rename = "baseline")]
    pub baselines: Vec<Baseline>,
}

/// How many files sit in each status.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Coverage {
    pub mirrored: usize,
    pub differential: usize,
    pub native_conformance: usize,
    pub not_applicable: usize,
}

impl Coverage {
    pub fn total(&self) -> usize {
        self.mirrored + self.differential + self.native_conformance + self.not_applicable
    }

    /// Files that could be covered, so everything except `not_applicable`.
    pub fn applicable(&self) -> usize {
        self.total() - self.not_applicable
    }
}

impl Manifest {
    pub fn coverage(&self) -> Coverage {
        let mut c = Coverage::default();
        for f in &self.test_files {
            match f.status {
                Status::Mirrored => c.mirrored += 1,
                Status::Differential => c.differential += 1,
                Status::NativeConformance => c.native_conformance += 1,
                Status::NotApplicable => c.not_applicable += 1,
            }
        }
        c
    }

    /// The committed inventory snapshot for this manifest's pinned commit.
    ///
    /// Named after the commit rather than being a fixed filename, so a bump
    /// cannot reuse the previous revision's file list by accident.
    pub fn snapshot_path(&self) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(PARITY_DIR)
            .join(format!("inventory.{}.txt", self.upstream.commit))
    }
}

/// Everything wrong with a manifest, in a stable order.
///
/// Returned rather than panicked so the caller decides what to do, and so the
/// checks can be exercised against a manifest built to fail them.
pub fn validate(m: &Manifest, crate_root: &Path) -> Vec<String> {
    let mut problems = Vec::new();

    let commit = &m.upstream.commit;
    if commit.len() != 40
        || !commit
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
    {
        problems.push(format!(
            "upstream.commit is {commit:?}, which is not a 40-character lowercase sha. A branch \
             name is not a pin, and a classification made against a moving target describes nothing."
        ));
    }

    if m.test_files.is_empty() {
        problems.push(
            "the manifest classifies no files at all, so every per-row check below would pass \
             over nothing"
                .to_string(),
        );
    }

    let mut seen: BTreeMap<&str, usize> = BTreeMap::new();
    for f in &m.test_files {
        *seen.entry(f.path.as_str()).or_default() += 1;
    }
    for (path, n) in seen.iter().filter(|(_, n)| **n > 1) {
        problems.push(format!(
            "{path} is classified {n} times. Two rows for one file means one of them is being \
             ignored, and nothing says which."
        ));
    }

    for f in &m.test_files {
        match f.status {
            Status::Mirrored => {
                if f.rust.is_empty() {
                    problems.push(format!(
                        "{}: mirrored, but lists no rust file. A claim of coverage with nothing \
                         behind it is worse than an honest gap.",
                        f.path
                    ));
                }
                for r in &f.rust {
                    if !crate_root.join(r).exists() {
                        problems.push(format!(
                            "{}: mirrored by {r}, which does not exist.",
                            f.path
                        ));
                    }
                }
            }
            _ => {
                if f.reason.as_deref().map(str::trim).unwrap_or("").is_empty() {
                    problems.push(format!(
                        "{}: {} with no reason. The reason is the whole value of the row: without \
                         it nobody can tell a decision from an oversight.",
                        f.path,
                        f.status.as_str()
                    ));
                }
                if !f.rust.is_empty() {
                    problems.push(format!(
                        "{}: {} but lists rust files {:?}. Only a mirrored row carries those.",
                        f.path,
                        f.status.as_str(),
                        f.rust
                    ));
                }
            }
        }

        // The forcing function. Once the planned file exists the row is no
        // longer a plan, and leaving it alone would quietly understate coverage
        // forever. Going red is how H4.2 gets told to promote the row.
        if let Some(planned) = &f.planned_rust {
            if f.status == Status::Mirrored {
                problems.push(format!(
                    "{}: mirrored rows carry `rust`, not `planned_rust`.",
                    f.path
                ));
            } else if crate_root.join(planned).exists() {
                problems.push(format!(
                    "{}: {planned} exists now, so this row is no longer a plan. Promote it to \
                     mirrored with rust = [\"{planned}\"].",
                    f.path
                ));
            }
        }
    }

    for b in &m.baselines {
        if b.files.is_empty() {
            problems.push(format!("baseline {:?} lists no files", b.kind));
        }
        if b.reason.trim().is_empty() {
            problems.push(format!("baseline {:?} has no reason", b.kind));
        }
    }

    problems
}

fn parsed() -> &'static Manifest {
    static M: OnceLock<Manifest> = OnceLock::new();
    M.get_or_init(|| {
        toml::from_str(MANIFEST_TOML).expect("upstream_parity/manifest.toml must parse")
    })
}

/// The parity manifest.
pub fn manifest() -> &'static Manifest {
    parsed()
}

/// The crate root, which `mirrored` and `planned_rust` paths are relative to.
pub fn crate_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}
