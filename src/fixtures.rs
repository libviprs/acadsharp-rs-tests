//! The committed DWG fixture corpus, read from `fixtures/manifest.toml`.
//!
//! The manifest is the single source of truth. Nothing here hardcodes a
//! sha256, a filename or a version, so there is no second copy to drift out of
//! step with it, and a fixture cannot be added to the tree without appearing
//! in the manifest that the integrity test checks.
//!
//! The corpus is ACadSharp's own `samples/*.dwg`, pinned to an exact upstream
//! commit. See `fixtures/README.md` for provenance and licensing.
//!
//! # Using it
//!
//! ```no_run
//! for fixture in acadsharp_rs_tests::fixtures::all() {
//!     let bytes = fixture.read().expect("fixture readable");
//!     assert_eq!(&bytes[..6], fixture.acad_version.as_bytes());
//! }
//! ```
//!
//! Deliberately decoupled from any FFI: this layer knows about files on disk
//! and nothing about how they are decoded, so the differential harness can
//! drive both the .NET reference and `acadsharp-rs` from the same list.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use serde::Deserialize;

/// The manifest, embedded so the corpus is described identically whether the
/// tests run from the repo root or from somewhere else.
const MANIFEST_TOML: &str = include_str!("../fixtures/manifest.toml");

/// Where the fixture files live, relative to the crate root.
const FIXTURE_DIR: &str = "fixtures";

/// The upstream revision every fixture was taken from.
#[derive(Debug, Clone, Deserialize)]
pub struct Upstream {
    pub repository: String,
    pub version: String,
    /// A full 40-character commit sha. Never a branch, never abbreviated.
    pub commit: String,
    pub license: String,
    pub license_file: String,
}

/// One DWG fixture.
#[derive(Debug, Clone, Deserialize)]
pub struct DwgFixture {
    /// Stable identifier, for naming a failing case in a differential report.
    pub id: String,
    /// Path relative to `fixtures/`.
    pub file: String,
    /// The six ASCII bytes the file must begin with, for example `AC1032`.
    pub acad_version: String,
    /// Human-facing AutoCAD generation, for report output.
    pub autocad_generation: String,
    /// Path within the upstream repository.
    pub source_path: String,
    /// Commit-pinned raw URL the bytes came from.
    pub source_url: String,
    pub bytes: u64,
    pub sha256: String,
}

impl DwgFixture {
    /// Absolute path to the fixture on disk.
    pub fn path(&self) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(FIXTURE_DIR)
            .join(&self.file)
    }

    /// The fixture's bytes.
    pub fn read(&self) -> std::io::Result<Vec<u8>> {
        std::fs::read(self.path())
    }
}

#[derive(Debug, Deserialize)]
struct Manifest {
    upstream: Upstream,
    #[serde(default, rename = "fixture")]
    fixtures: Vec<DwgFixture>,
}

fn manifest() -> &'static Manifest {
    static PARSED: OnceLock<Manifest> = OnceLock::new();
    PARSED.get_or_init(|| toml::from_str(MANIFEST_TOML).expect("fixtures/manifest.toml must parse"))
}

/// Every fixture in the corpus, in manifest order.
pub fn all() -> &'static [DwgFixture] {
    &manifest().fixtures
}

/// The upstream revision the corpus was taken from.
pub fn upstream() -> &'static Upstream {
    &manifest().upstream
}

/// The fixture with this id, if there is one.
pub fn by_id(id: &str) -> Option<&'static DwgFixture> {
    all().iter().find(|f| f.id == id)
}

/// The fixture for this DWG version, for example `AC1032`.
pub fn by_version(acad_version: &str) -> Option<&'static DwgFixture> {
    all().iter().find(|f| f.acad_version == acad_version)
}
