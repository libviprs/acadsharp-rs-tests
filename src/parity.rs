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
    /// What acadrust does with this file when that differs from the row. No
    /// override means acadrust shares the row's status and reason.
    #[serde(default)]
    pub acadrust: Option<Override>,
    /// What covers the row, one entry per piece of evidence:
    ///
    /// - `fixture:<id>`, a row of `fixtures/manifest.toml`;
    /// - `path:<relative path>`, a file in this crate;
    /// - `record:<kind>@<fixture>`, a fixture whose canonical reading holds at
    ///   least one record of that kind and which `parity/VERDICTS.toml` compares;
    /// - `verdict:<fixture>@<pair>`, a row of `parity/VERDICTS.toml`;
    /// - `model1:<kind>`, `header` or a kind of the model-1 schema.
    ///
    /// Anything else is refused.
    #[serde(default)]
    pub evidence: Vec<String>,
    /// Why an applicable row has no evidence yet: the thing that would cover
    /// it, and what is missing. An applicable row has evidence or this, never
    /// both and never neither.
    #[serde(default)]
    pub exclusion: Option<String>,
}

/// One engine's own answer for a row.
#[derive(Debug, Clone, Deserialize)]
pub struct Override {
    pub status: Status,
    #[serde(default)]
    pub reason: Option<String>,
    /// For acadrust, in place of the row's own: an override is the whole answer
    /// for the engine, evidence and exclusion included.
    #[serde(default)]
    pub evidence: Vec<String>,
    #[serde(default)]
    pub exclusion: Option<String>,
}

/// An engine a row can be read for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Provider {
    Acadsharp,
    Acadrust,
}

impl TestFile {
    /// The status this row has for `p`: the override for acadrust if there is
    /// one, the row's own otherwise.
    pub fn status_for(&self, p: Provider) -> Status {
        match (p, &self.acadrust) {
            (Provider::Acadrust, Some(o)) => o.status,
            _ => self.status,
        }
    }

    /// The evidence `p` has for this row. An acadrust override replaces the
    /// row's evidence; a mirrored row's `rust` files are evidence for the engine
    /// whose API they go through.
    pub fn evidence_for(&self, p: Provider) -> Vec<String> {
        match (p, &self.acadrust) {
            (Provider::Acadrust, Some(o)) => o.evidence.clone(),
            _ => {
                let mut v = self.evidence.clone();
                v.extend(self.rust.iter().map(|r| format!("path:{r}")));
                v
            }
        }
    }

    /// The reasoned exclusion `p` has for this row, if any.
    pub fn exclusion_for(&self, p: Provider) -> Option<&str> {
        let e = match (p, &self.acadrust) {
            (Provider::Acadrust, Some(o)) => o.exclusion.as_deref(),
            _ => self.exclusion.as_deref(),
        };
        e.map(str::trim).filter(|s| !s.is_empty())
    }
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

/// The burn-down for one engine: of the applicable rows, how many name
/// evidence and how many carry a reasoned exclusion.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Burndown {
    pub total: usize,
    pub not_applicable: usize,
    pub applicable: usize,
    pub evidenced: usize,
    pub excluded: usize,
}

impl Manifest {
    pub fn burndown(&self, p: Provider) -> Burndown {
        let mut b = Burndown::default();
        for f in &self.test_files {
            b.total += 1;
            if f.status_for(p) == Status::NotApplicable {
                b.not_applicable += 1;
                continue;
            }
            b.applicable += 1;
            if !f.evidence_for(p).is_empty() {
                b.evidenced += 1;
            } else if f.exclusion_for(p).is_some() {
                b.excluded += 1;
            }
        }
        b
    }

    pub fn coverage(&self) -> Coverage {
        self.coverage_for(Provider::Acadsharp)
    }

    /// Counts per status as `p` sees the rows.
    pub fn coverage_for(&self, p: Provider) -> Coverage {
        let mut c = Coverage::default();
        for f in &self.test_files {
            match f.status_for(p) {
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

        if let Some(o) = &f.acadrust {
            if o.reason.as_deref().map(str::trim).unwrap_or("").is_empty() {
                problems.push(format!(
                    "{}: acadrust override ({}) with no reason. An override is a claim that the \
                     engines differ, and the reason is the claim.",
                    f.path,
                    o.status.as_str()
                ));
            }
            if o.status == f.status && o.reason == f.reason {
                problems.push(format!(
                    "{}: the acadrust override restates the row, so it says nothing and hides \
                     which rows really differ. Drop it.",
                    f.path
                ));
            }
        }

        for (who, p) in [("acadsharp-rs", Provider::Acadsharp), ("acadrust", Provider::Acadrust)] {
            let status = f.status_for(p);
            let evidence = f.evidence_for(p);
            let exclusion = f.exclusion_for(p);
            if status == Status::NotApplicable {
                if !evidence.is_empty() || exclusion.is_some() {
                    problems.push(format!(
                        "{}: not_applicable for {who} but names evidence or an exclusion. A row \
                         that is out of scope has nothing to cover.",
                        f.path
                    ));
                }
                continue;
            }
            match (evidence.is_empty(), exclusion) {
                (true, None) => problems.push(format!(
                    "{}: applicable for {who} with no evidence and no reasoned exclusion. Name \
                     what covers it, or say what is missing in `exclusion`.",
                    f.path
                )),
                (false, Some(_)) => problems.push(format!(
                    "{}: {who} has evidence and an exclusion. It is covered or it is not.",
                    f.path
                )),
                _ => {}
            }
        }

        let mut named: Vec<&String> = f.evidence.iter().collect();
        if let Some(o) = &f.acadrust {
            named.extend(o.evidence.iter());
        }
        for e in named {
            if let Some(why) = unresolved(e, crate_root) {
                problems.push(format!("{}: evidence {e:?} {why}", f.path));
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

/// Why an evidence entry resolves to nothing, or `None` when it resolves.
fn unresolved(e: &str, crate_root: &Path) -> Option<String> {
    let Some((kind, rest)) = e.split_once(':') else {
        return Some("has no kind (fixture:, path:, record:, verdict: or model1:).".into());
    };
    match kind {
        "fixture" => (!fixture_ids().contains(rest))
            .then(|| "names a fixture that is not a row of fixtures/manifest.toml.".to_string()),
        "path" => (!crate_root.join(rest).exists())
            .then(|| "is a path that does not exist.".to_string()),
        "verdict" => {
            let Some((fixture, pair)) = rest.split_once('@') else {
                return Some("is not verdict:<fixture>@<pair>.".into());
            };
            (!verdicts().contains(&(fixture.to_string(), pair.to_string()))).then(|| {
                "is not a row of parity/VERDICTS.toml (fixture and pair as the table spells them)."
                    .to_string()
            })
        }
        "record" => {
            let Some((record, fixture)) = rest.split_once('@') else {
                return Some("is not record:<kind>@<fixture>.".into());
            };
            if !verdicts().iter().any(|(f, _)| f == fixture) {
                return Some(format!(
                    "names {fixture}, which parity/VERDICTS.toml does not compare, so nothing \
                     would notice the engines disagreeing on it."
                ));
            }
            let Some(fx) = crate::fixtures::all().iter().find(|f| f.id == fixture) else {
                return Some(format!("names {fixture}, which is not a corpus fixture."));
            };
            let bytes = match fx.load_reference_jsonl() {
                Ok(b) => b,
                Err(err) => return Some(format!("cannot read {fixture}'s canonical reading: {err}")),
            };
            let needle = format!("\"record\":\"{record}\"");
            (!String::from_utf8_lossy(&bytes).contains(&needle)).then(|| {
                format!("names a fixture whose canonical reading holds no {record} record.")
            })
        }
        "model1" => (rest != "header" && crate::model1::schema(rest).is_none()).then(|| {
            "is not `header` or a kind of the model-1 schema (src/model1.rs).".to_string()
        }),
        _ => Some(
            "has a kind this crate cannot resolve (fixture:, path:, record:, verdict: and \
             model1: only). A kind with no source to check it against is a claim, not evidence."
                .to_string(),
        ),
    }
}

/// Every `(fixture, pair)` of `parity/VERDICTS.toml`.
fn verdicts() -> &'static std::collections::BTreeSet<(String, String)> {
    static V: OnceLock<std::collections::BTreeSet<(String, String)>> = OnceLock::new();
    V.get_or_init(|| {
        let t = crate::verdict::Table::parse(include_str!("../parity/VERDICTS.toml"))
            .expect("parity/VERDICTS.toml must parse");
        t.rows
            .iter()
            .map(|r| (r.fixture.clone(), r.pair.clone()))
            .collect()
    })
}

/// Parses any text as a parity manifest and returns how many test files it
/// classifies, or the parse error. The fuzz target calls this.
pub fn parse_manifest_text(text: &str) -> Result<usize, toml::de::Error> {
    let m: Manifest = toml::from_str(text)?;
    Ok(m.test_files.len() + m.baselines.len())
}

/// Every id any table of the corpus manifest carries.
fn fixture_ids() -> std::collections::BTreeSet<&'static str> {
    use crate::fixtures;
    let mut ids: std::collections::BTreeSet<&'static str> =
        fixtures::all().iter().map(|f| f.id.as_str()).collect();
    ids.extend(fixtures::oracle_inputs().iter().map(|f| f.id.as_str()));
    ids.extend(fixtures::refusals().iter().map(|f| f.id.as_str()));
    ids.extend(fixtures::censuses().iter().map(|f| f.id.as_str()));
    ids.extend(fixtures::recordings().iter().map(|f| f.id.as_str()));
    ids
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
