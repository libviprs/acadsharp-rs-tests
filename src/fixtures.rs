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
//! # Reading a fixture
//!
//! [`load`] is the door every test should use. It verifies size, DWG signature
//! and sha256 before it hands back a single byte, so a fixture whose bytes
//! drifted fails right there, naming itself, instead of surfacing later as an
//! unexplained difference in a decode comparison.
//!
//! ```no_run
//! let bytes = acadsharp_rs_tests::fixtures::load("acadsharp-ac1018")?;
//! assert_eq!(&bytes[..6], b"AC1018");
//! # Ok::<_, acadsharp_rs_tests::fixtures::FixtureError>(())
//! ```
//!
//! [`DwgFixture::read_unverified`] exists for exactly one caller: the integrity
//! test, which has to compare the hashes itself to report both sides. Anything
//! else reaching for it is skipping the check.
//!
//! Deliberately decoupled from any FFI: this layer knows about files on disk
//! and nothing about how they are decoded, so the differential harness can
//! drive both the .NET reference and `acadsharp-rs` from the same list.

use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use serde::Deserialize;
use sha2::{Digest, Sha256};

/// The manifest, embedded so the corpus is described identically whether the
/// tests run from the repo root or from somewhere else.
const MANIFEST_TOML: &str = include_str!("../fixtures/manifest.toml");

/// Where the fixture files live, relative to the crate root.
const FIXTURE_DIR: &str = "fixtures";

/// Smallest plausible DWG. The real samples are around a megabyte, so this only
/// has to be large enough to reject an error page or a truncated write.
const MIN_PLAUSIBLE_BYTES: usize = 1024;

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
    /// SPDX identifier for the licence these bytes are redistributed under.
    ///
    /// Recorded per fixture rather than only once for the corpus. Every current
    /// fixture is MIT from the same repository, so today it is the same string
    /// seven times, but the first fixture from anywhere else must not be able
    /// to inherit ACadSharp's licence by sitting in the same directory.
    pub license: String,
    /// Commit-pinned URL of the licence text itself, so the claim above is
    /// checkable rather than asserted.
    pub license_evidence: String,
    pub bytes: u64,
    pub sha256: String,

    /// Canonical output of the .NET reference for this input, relative to
    /// `fixtures/`. Filled by H2.4 (libviprs/acadsharp-rs-tests#6); `None`
    /// until then.
    ///
    /// Optional rather than an empty string on purpose: an empty string is a
    /// value that every check has to remember to special-case, and one of them
    /// eventually will not.
    #[serde(default)]
    pub reference_output: Option<String>,
    /// sha256 of [`Self::reference_output`], on the same terms.
    #[serde(default)]
    pub reference_sha256: Option<String>,
    /// Which ACadSharp version produced the reference output.
    #[serde(default)]
    pub acadsharp_reference_version: Option<String>,
    /// Schema version of the exporter that wrote it, so a reference generated
    /// by an older exporter is recognisable rather than silently compared.
    #[serde(default)]
    pub reference_exporter_schema: Option<String>,
}

/// What can be wrong with a fixture on disk.
///
/// Every variant names the fixture and says what to do about it, because the
/// person reading it is usually looking at a red suite and not at this file.
#[derive(Debug)]
pub enum FixtureError {
    /// No manifest entry with that id.
    UnknownId(String),
    /// The file the manifest points at could not be read.
    Unreadable {
        id: String,
        path: PathBuf,
        source: std::io::Error,
    },
    /// The bytes are not the bytes the manifest froze.
    Corrupt { id: String, detail: String },
}

impl fmt::Display for FixtureError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownId(id) => write!(
                f,
                "no fixture with id {id:?} in fixtures/manifest.toml. The manifest is the only \
                 list; a fixture that is not in it has not been vetted."
            ),
            Self::Unreadable { id, path, source } => write!(
                f,
                "{id}: cannot read {}: {source}. Run \
                 `python3 tools/fetch_acadsharp_fixtures.py fetch` to restore it from the pinned \
                 commit.",
                path.display()
            ),
            Self::Corrupt { id, detail } => write!(f, "{id}: {detail}"),
        }
    }
}

impl std::error::Error for FixtureError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Unreadable { source, .. } => Some(source),
            _ => None,
        }
    }
}

impl DwgFixture {
    /// Absolute path to the fixture on disk.
    pub fn path(&self) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(FIXTURE_DIR)
            .join(&self.file)
    }

    /// The fixture's bytes, verified against the manifest first.
    ///
    /// Checks size, the six-byte DWG signature and sha256, in that order, so
    /// the cheap discriminating failure is reported rather than a hash mismatch
    /// that says nothing about what actually landed on disk.
    pub fn load(&self) -> Result<Vec<u8>, FixtureError> {
        let data = self
            .read_unverified()
            .map_err(|source| FixtureError::Unreadable {
                id: self.id.clone(),
                path: self.path(),
                source,
            })?;
        self.verify(&data)?;
        Ok(data)
    }

    /// The bytes exactly as they are on disk, with nothing checked.
    ///
    /// Only `tests/fixture_integrity.rs` should call this, and only because it
    /// has to do the comparison itself to print both hashes. Use [`load`]
    /// everywhere else: reading a fixture without verifying it is how a drifted
    /// byte turns into a confusing decode difference three layers away.
    ///
    /// [`load`]: Self::load
    pub fn read_unverified(&self) -> std::io::Result<Vec<u8>> {
        std::fs::read(self.path())
    }

    /// Whether these bytes are the ones the manifest describes.
    pub fn verify(&self, data: &[u8]) -> Result<(), FixtureError> {
        let corrupt = |detail: String| FixtureError::Corrupt {
            id: self.id.clone(),
            detail,
        };

        if data.len() as u64 != self.bytes {
            return Err(corrupt(format!(
                "{} bytes on disk, manifest says {}",
                data.len(),
                self.bytes
            )));
        }
        if data.len() < MIN_PLAUSIBLE_BYTES {
            return Err(corrupt(format!(
                "{} bytes is too small to be a real DWG",
                data.len()
            )));
        }
        // The signature catches an HTML error page, a DXF renamed to .dwg, and
        // a file that is simply the wrong generation.
        let magic = &data[..6];
        if magic != self.acad_version.as_bytes() {
            return Err(corrupt(format!(
                "first six bytes are {:?}, expected {:?}",
                String::from_utf8_lossy(magic),
                self.acad_version
            )));
        }
        let got = format!("{:x}", Sha256::digest(data));
        if got != self.sha256 {
            return Err(corrupt(format!(
                "sha256 is {got}, manifest froze {}. These are not the same bytes.",
                self.sha256
            )));
        }
        Ok(())
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

/// Verified bytes for the fixture with this id.
///
/// The shared door for the whole suite. See [`DwgFixture::load`].
pub fn load(id: &str) -> Result<Vec<u8>, FixtureError> {
    by_id(id)
        .ok_or_else(|| FixtureError::UnknownId(id.to_string()))?
        .load()
}

/// Absolute path of the directory the fixture files live in.
///
/// Exposed so the integrity test can walk it looking for files the manifest
/// does not mention. A fixture nobody put in the manifest is a fixture nobody
/// vetted, and it would otherwise sit in the tree unnoticed.
pub fn fixture_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(FIXTURE_DIR)
}
