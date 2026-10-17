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
//! # The reference side
//!
//! Each row also records the frozen output of the .NET reference oracle for
//! that input: [`DwgFixture::load_reference_jsonl`] and
//! [`DwgFixture::load_reference_svg`] verify those the same way, against the
//! hashes in the same manifest. The JSONL hash is of the **uncompressed**
//! stream; see `docs/CANONICAL_SCHEMA.md` for why.
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

    /// Which `[sources.*]` table this row's pin comes from. Every row names one.
    #[serde(default)]
    pub source: Option<String>,
    /// `A` (AutoCAD-authored, decision grade), `B` (regression grade) or `C`
    /// (stress: both engines open it and agree on counts). Single-valued.
    #[serde(default)]
    pub tier: Option<String>,
    /// Set, with a reason, on a drawing that is meant to hold no geometry: an
    /// empty drawing, or a refused-entity fixture whose stream is the warnings
    /// that say so. A reference with no primitives is otherwise refused, since a
    /// differential over nothing cannot fail; this is the explicit, reasoned
    /// exemption, and it is checked from both sides (a row that claims it while
    /// the stream holds primitives is refused too).
    #[serde(default)]
    pub no_primitives: Option<String>,
    /// Where the provenance of a file that is not ACadSharp's own is written
    /// down, pinned to a commit.
    #[serde(default)]
    pub provenance_evidence: Option<String>,

    /// For a drawing this suite generated (`source = "generated"`): the
    /// generator project that wrote it, relative to the repository root.
    #[serde(default)]
    pub generated_by: Option<String>,
    /// The one file under the generator that defines how this drawing is built.
    #[serde(default)]
    pub generator_source: Option<String>,
    /// sha256 of `generator_source` when the row was written. A test recomputes
    /// it, so a drawing whose definition changed without its row changing is
    /// caught.
    #[serde(default)]
    pub generator_source_sha256: Option<String>,
    /// The ACadSharp version the generator wrote the drawing with.
    #[serde(default)]
    pub acadsharp_generator_version: Option<String>,

    /// The compressed canonical reference output, relative to `fixtures/`.
    ///
    /// Optional rather than an empty string on purpose: an empty string is a
    /// value that every check has to remember to special-case, and one of them
    /// eventually will not. A fixture with no reference is a fixture the
    /// differential cannot use, and `tests/expected_outputs.rs` refuses one.
    #[serde(default)]
    pub reference_jsonl: Option<String>,
    /// sha256 of the reference JSONL **before** compression.
    ///
    /// Deliberately not the hash of the `.zst`. Zstandard frames are
    /// reproducible for one library version and one set of frame parameters,
    /// and `tools/regenerate_reference.py` pins both, but they are not a
    /// cross-implementation guarantee. The uncompressed bytes are, so the
    /// contract lives there and the compression is storage.
    #[serde(default)]
    pub reference_jsonl_uncompressed_sha256: Option<String>,
    /// The visual reference, relative to `fixtures/`.
    #[serde(default)]
    pub reference_svg: Option<String>,
    /// sha256 of the visual reference, which needs no such caveat.
    #[serde(default)]
    pub reference_svg_sha256: Option<String>,
    /// Which ACadSharp version produced the reference output.
    #[serde(default)]
    pub acadsharp_reference_version: Option<String>,
    /// The upstream ACadSharp commit that version was tagged at.
    #[serde(default)]
    pub acadsharp_reference_commit: Option<String>,
    /// The exact .NET SDK the accepted regeneration ran on.
    #[serde(default)]
    pub dotnet_sdk: Option<String>,
    /// Canonical schema version of the records in the reference output, so a
    /// reference written by an older exporter is recognisable rather than
    /// silently compared against a newer reader.
    #[serde(default)]
    pub reference_schema: Option<u32>,
    /// Schema version of the SVG dialect, on the same terms.
    #[serde(default)]
    pub svg_schema: Option<u32>,
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
    /// The fixture has no frozen reference output recorded.
    NoReference { id: String, what: String },
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
            Self::NoReference { id, what } => write!(
                f,
                "{id}: fixtures/manifest.toml records no {what}. Run \
                 `python3 tools/regenerate_reference.py regenerate --accept` to produce one."
            ),
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

impl DwgFixture {
    /// Absolute path of the compressed canonical reference output.
    ///
    /// The manifest owns the path. Nothing else in the suite builds one, so a
    /// fixture whose artefacts move needs one line changed rather than a grep.
    pub fn reference_jsonl_path(&self) -> Option<PathBuf> {
        self.reference_jsonl
            .as_ref()
            .map(|relative| fixture_root().join(relative))
    }

    /// Absolute path of the visual reference.
    pub fn reference_svg_path(&self) -> Option<PathBuf> {
        self.reference_svg
            .as_ref()
            .map(|relative| fixture_root().join(relative))
    }

    /// The uncompressed canonical reference output, verified against the manifest.
    ///
    /// Decompresses the committed `.zst` and checks the sha256 of what came out
    /// against [`Self::reference_jsonl_uncompressed_sha256`] before returning a
    /// byte. That ordering is the point: the hash is a claim about the
    /// uncompressed stream, so checking the compressed file instead would tie
    /// the suite to one zstd implementation for no gain.
    pub fn load_reference_jsonl(&self) -> Result<Vec<u8>, FixtureError> {
        let path = self
            .reference_jsonl_path()
            .ok_or_else(|| FixtureError::NoReference {
                id: self.id.clone(),
                what: "reference_jsonl".to_string(),
            })?;
        let want = self
            .reference_jsonl_uncompressed_sha256
            .as_ref()
            .ok_or_else(|| FixtureError::NoReference {
                id: self.id.clone(),
                what: "reference_jsonl_uncompressed_sha256".to_string(),
            })?;

        let compressed = std::fs::read(&path).map_err(|source| FixtureError::Unreadable {
            id: self.id.clone(),
            path: path.clone(),
            source,
        })?;

        let mut decoder =
            ruzstd::decoding::StreamingDecoder::new(compressed.as_slice()).map_err(|e| {
                FixtureError::Corrupt {
                    id: self.id.clone(),
                    detail: format!("{} is not a zstd frame: {e}", path.display()),
                }
            })?;
        let mut data = Vec::new();
        std::io::Read::read_to_end(&mut decoder, &mut data).map_err(|e| FixtureError::Corrupt {
            id: self.id.clone(),
            detail: format!("{} did not decompress: {e}", path.display()),
        })?;

        let got = format!("{:x}", Sha256::digest(&data));
        if &got != want {
            return Err(FixtureError::Corrupt {
                id: self.id.clone(),
                detail: format!(
                    "uncompressed reference sha256 is {got}, manifest froze {want}. The \
                     expected output is not the one the manifest describes, so every \
                     comparison against it would be against an unknown baseline."
                ),
            });
        }
        Ok(data)
    }

    /// The visual reference, verified against the manifest.
    pub fn load_reference_svg(&self) -> Result<Vec<u8>, FixtureError> {
        let path = self
            .reference_svg_path()
            .ok_or_else(|| FixtureError::NoReference {
                id: self.id.clone(),
                what: "reference_svg".to_string(),
            })?;
        let want = self
            .reference_svg_sha256
            .as_ref()
            .ok_or_else(|| FixtureError::NoReference {
                id: self.id.clone(),
                what: "reference_svg_sha256".to_string(),
            })?;

        let data = std::fs::read(&path).map_err(|source| FixtureError::Unreadable {
            id: self.id.clone(),
            path: path.clone(),
            source,
        })?;
        let got = format!("{:x}", Sha256::digest(&data));
        if &got != want {
            return Err(FixtureError::Corrupt {
                id: self.id.clone(),
                detail: format!("reference SVG sha256 is {got}, manifest froze {want}"),
            });
        }
        Ok(data)
    }
}

/// A source the corpus is taken from, pinned to one commit.
///
/// `[upstream]` is the original ACadSharp pin and the reference oracle's; this
/// table is the per-source version of it, so a row cannot borrow another
/// source's commit or host.
#[derive(Debug, Clone, Deserialize)]
pub struct Source {
    pub repository: String,
    /// A full 40-character commit sha.
    pub commit: String,
    /// Every `source_url` of a row from this source starts with this and then
    /// carries `commit`.
    pub url_prefix: String,
    pub license: String,
    /// The licence text, relative to `fixtures/`.
    pub license_file: String,
    /// `true` for the source whose files this repository writes itself
    /// (`reference/ACadSharp.Generator`): there is nothing to download, so the
    /// fetch tool skips its rows and the generator is what reproduces them.
    #[serde(default)]
    pub generated: bool,
}

/// A row that is a file but not a drawing the oracle reads.
///
/// `[[fixture]]` stays what it always was, a DWG the oracle reads, because four
/// things iterate that table and expect a drawing. The other roles get sibling
/// tables, each with the provenance every row has.
pub trait ManifestFile {
    fn id(&self) -> &str;
    /// Path relative to `fixtures/`.
    fn file(&self) -> &str;
    fn bytes(&self) -> u64;
    fn sha256(&self) -> &str;
    fn source(&self) -> &str;
    fn source_url(&self) -> &str;

    fn path(&self) -> PathBuf {
        fixture_root().join(self.file())
    }

    /// The bytes, verified against the manifest (size, then sha256) first.
    fn load(&self) -> Result<Vec<u8>, FixtureError> {
        let data = std::fs::read(self.path()).map_err(|source| FixtureError::Unreadable {
            id: self.id().to_string(),
            path: self.path(),
            source,
        })?;
        let corrupt = |detail: String| FixtureError::Corrupt {
            id: self.id().to_string(),
            detail,
        };
        if data.len() as u64 != self.bytes() {
            return Err(corrupt(format!(
                "{} bytes on disk, manifest says {}",
                data.len(),
                self.bytes()
            )));
        }
        let got = format!("{:x}", Sha256::digest(&data));
        if got != self.sha256() {
            return Err(corrupt(format!(
                "sha256 is {got}, manifest froze {}. These are not the same bytes.",
                self.sha256()
            )));
        }
        Ok(data)
    }
}

macro_rules! manifest_file {
    ($ty:ty) => {
        impl ManifestFile for $ty {
            fn id(&self) -> &str {
                &self.id
            }
            fn file(&self) -> &str {
                &self.file
            }
            fn bytes(&self) -> u64 {
                self.bytes
            }
            fn sha256(&self) -> &str {
                &self.sha256
            }
            fn source(&self) -> &str {
                &self.source
            }
            fn source_url(&self) -> &str {
                &self.source_url
            }
        }
    };
}

/// A libviprs-dep recording of one drawing: the six-decimal `.txt` the fork's
/// harness compares against.
#[derive(Debug, Clone, Deserialize)]
pub struct Recording {
    pub id: String,
    pub file: String,
    /// The `[[fixture]]` or `[[refusal]]` row this is a recording of.
    pub drawing: String,
    pub source: String,
    pub source_path: String,
    pub source_url: String,
    pub license: String,
    pub license_evidence: String,
    pub provenance_evidence: String,
    pub bytes: u64,
    pub sha256: String,
}
manifest_file!(Recording);

/// A file the oracle's inputs are adjudicated against: AutoCAD's own DXF twins.
#[derive(Debug, Clone, Deserialize)]
pub struct OracleInput {
    pub id: String,
    pub file: String,
    pub source: String,
    #[serde(default)]
    pub acad_version: Option<String>,
    /// `ascii` or `binary`, when the file name says.
    #[serde(default)]
    pub encoding: Option<String>,
    /// The drawing this is the DXF twin of, when there is one.
    #[serde(default)]
    pub for_fixture: Option<String>,
    pub source_path: String,
    pub source_url: String,
    pub license: String,
    pub license_evidence: String,
    pub bytes: u64,
    pub sha256: String,
}
manifest_file!(OracleInput);

/// AutoCAD's own object census, recorded by upstream.
///
/// One row per distinct blob. Upstream stores the same bytes under several
/// names (five of the seven `sample_AC10xx_tree.json` are identical), and a
/// corpus that stores one blob twice has two rows to keep honest, so the extra
/// upstream locations are `also_source_*` on the one row and the fetch tool
/// checks that each of them serves the same bytes.
#[derive(Debug, Clone, Deserialize)]
pub struct Census {
    pub id: String,
    pub file: String,
    pub source: String,
    pub for_fixtures: Vec<String>,
    #[serde(default)]
    pub also_source_paths: Vec<String>,
    #[serde(default)]
    pub also_source_urls: Vec<String>,
    pub source_path: String,
    pub source_url: String,
    pub license: String,
    pub license_evidence: String,
    pub bytes: u64,
    pub sha256: String,
}
manifest_file!(Census);

/// A drawing the oracle refuses, with the exit code it is expected to refuse
/// it with.
#[derive(Debug, Clone, Deserialize)]
pub struct Refusal {
    pub id: String,
    pub file: String,
    pub source: String,
    pub acad_version: String,
    /// The oracle's `ExitCode` name, for example `UnsupportedVersion`.
    pub oracle_exit: String,
    pub oracle_exit_code: u32,
    pub source_path: String,
    pub source_url: String,
    pub license: String,
    pub license_evidence: String,
    pub provenance_evidence: String,
    pub bytes: u64,
    pub sha256: String,
}
manifest_file!(Refusal);

/// A ground truth neither reader produced: ezdxf's reading of one of
/// AutoCAD's own DXF files, frozen the way the oracle's outputs are.
///
/// The three files are compressed streams (`jsonl`, `sidecar`) and a plain
/// `run.json`; the manifest holds the sha256 of each one's **uncompressed**
/// bytes. Written by `tools/ground_truth/extract_ezdxf.py freeze --accept`,
/// never by hand.
#[derive(Debug, Clone, Deserialize)]
pub struct GroundTruth {
    pub id: String,
    /// The `[[oracle_input]]` row this is a reading of.
    pub input: String,
    pub engine: String,
    pub jsonl: String,
    pub jsonl_uncompressed_sha256: String,
    pub sidecar: String,
    pub sidecar_uncompressed_sha256: String,
    pub run: String,
    pub run_sha256: String,
    pub ezdxf_version: String,
    pub requirements_sha256: String,
    pub image: String,
    pub records: u64,
    pub schema: u64,
}

impl GroundTruth {
    fn read_packed(&self, relative: &str, want: &str) -> Result<Vec<u8>, FixtureError> {
        let path = fixture_root().join(relative);
        let compressed = std::fs::read(&path).map_err(|source| FixtureError::Unreadable {
            id: self.id.clone(),
            path: path.clone(),
            source,
        })?;
        let mut decoder =
            ruzstd::decoding::StreamingDecoder::new(compressed.as_slice()).map_err(|e| {
                FixtureError::Corrupt {
                    id: self.id.clone(),
                    detail: format!("{} is not a zstd frame: {e}", path.display()),
                }
            })?;
        let mut data = Vec::new();
        std::io::Read::read_to_end(&mut decoder, &mut data).map_err(|e| FixtureError::Corrupt {
            id: self.id.clone(),
            detail: format!("{} did not decompress: {e}", path.display()),
        })?;
        let got = format!("{:x}", Sha256::digest(&data));
        if got != want {
            return Err(FixtureError::Corrupt {
                id: self.id.clone(),
                detail: format!(
                    "{relative} hashes to {got} uncompressed, the manifest froze {want}"
                ),
            });
        }
        Ok(data)
    }

    /// The schema 1 stream, verified against the manifest.
    pub fn load_jsonl(&self) -> Result<Vec<u8>, FixtureError> {
        self.read_packed(&self.jsonl, &self.jsonl_uncompressed_sha256)
    }

    /// The sidecar, verified against the manifest.
    pub fn load_sidecar(&self) -> Result<Vec<u8>, FixtureError> {
        self.read_packed(&self.sidecar, &self.sidecar_uncompressed_sha256)
    }

    /// `run.json`, verified against the manifest.
    pub fn load_run(&self) -> Result<Vec<u8>, FixtureError> {
        let path = fixture_root().join(&self.run);
        let data = std::fs::read(&path).map_err(|source| FixtureError::Unreadable {
            id: self.id.clone(),
            path: path.clone(),
            source,
        })?;
        let got = format!("{:x}", Sha256::digest(&data));
        if got != self.run_sha256 {
            return Err(FixtureError::Corrupt {
                id: self.id.clone(),
                detail: format!(
                    "run.json hashes to {got}, the manifest froze {}",
                    self.run_sha256
                ),
            });
        }
        Ok(data)
    }

    /// Every file this row claims, relative to the fixture root.
    pub fn files(&self) -> [&str; 3] {
        [&self.jsonl, &self.sidecar, &self.run]
    }
}

#[derive(Debug, Deserialize)]
struct Manifest {
    upstream: Upstream,
    #[serde(default)]
    sources: std::collections::BTreeMap<String, Source>,
    #[serde(default, rename = "fixture")]
    fixtures: Vec<DwgFixture>,
    #[serde(default, rename = "recording")]
    recordings: Vec<Recording>,
    #[serde(default, rename = "oracle_input")]
    oracle_inputs: Vec<OracleInput>,
    #[serde(default, rename = "census")]
    censuses: Vec<Census>,
    #[serde(default, rename = "refusal")]
    refusals: Vec<Refusal>,
    #[serde(default, rename = "ground_truth")]
    ground_truths: Vec<GroundTruth>,
}

fn manifest() -> &'static Manifest {
    static PARSED: OnceLock<Manifest> = OnceLock::new();
    PARSED.get_or_init(|| toml::from_str(MANIFEST_TOML).expect("fixtures/manifest.toml must parse"))
}

/// Every fixture in the corpus, in manifest order.
pub fn all() -> &'static [DwgFixture] {
    &manifest().fixtures
}

/// Every `[sources.*]` table, by name.
pub fn sources() -> &'static std::collections::BTreeMap<String, Source> {
    &manifest().sources
}

/// The libviprs-dep recordings.
pub fn recordings() -> &'static [Recording] {
    &manifest().recordings
}

/// The DXF twins and other inputs the oracle's output is adjudicated against.
pub fn oracle_inputs() -> &'static [OracleInput] {
    &manifest().oracle_inputs
}

/// AutoCAD's recorded object census files.
pub fn censuses() -> &'static [Census] {
    &manifest().censuses
}

/// The ground truths neither reader produced (ezdxf over AutoCAD's DXF).
pub fn ground_truths() -> &'static [GroundTruth] {
    &manifest().ground_truths
}

/// The ground truth that reads the `[[oracle_input]]` row `input`.
pub fn ground_truth(input: &str) -> Option<&'static GroundTruth> {
    ground_truths().iter().find(|g| g.input == input)
}

/// Drawings the oracle is expected to refuse.
pub fn refusals() -> &'static [Refusal] {
    &manifest().refusals
}

/// The drawings of one tier, in manifest order.
pub fn by_tier(tier: &str) -> Vec<&'static DwgFixture> {
    all()
        .iter()
        .filter(|f| f.tier.as_deref() == Some(tier))
        .collect()
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
