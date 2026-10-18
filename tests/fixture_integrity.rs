//! The committed DWG corpus is what every differential test compares against,
//! so its bytes have to be exactly what the manifest says before any of those
//! comparisons mean anything.
//!
//! These run offline under an ordinary `cargo test`: no network, no .NET, no
//! ACadSharp. The committed corpus is authoritative, and a fetch that
//! disagrees with it is a decision for a human rather than something CI
//! resolves on its own.
//!
//! Each check below is paired with the state that makes it fail, because a
//! fixture guard that cannot go red is worse than none: it would let an empty
//! `fixtures/dwg/` read as a healthy corpus.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use acadsharp_rs_tests::fixtures::{self, ManifestFile};
use sha2::{Digest, Sha256};

/// Every DWG generation ACadSharp's `DwgReader` documents as readable.
///
/// Spelled out rather than derived from the manifest on purpose. Deriving it
/// would make the check vacuous: deleting a fixture would delete the
/// expectation with it and the suite would stay green on a shrunken corpus,
/// which is exactly the accident this exists to catch.
const EXPECTED_VERSIONS: &[&str] = &[
    "AC1014", "AC1015", "AC1018", "AC1021", "AC1024", "AC1027", "AC1032",
];

/// How many rows each table and tier holds, written out by hand.
///
/// Spelled out for the same reason `EXPECTED_VERSIONS` is: deriving a count from
/// the manifest would make a shrunken manifest agree with itself. Moving one of
/// these is a decision, and it comes with the reason in the manifest header.
///
/// Tier A is the seven AutoCAD saves plus fifteen more from ACadSharp's
/// `samples/` (`sample_base`, `empty`, the ten dynamic blocks, `AecObjects`,
/// `geoloc`, `export_sample`). Tier B is libviprs-dep's regression drawings and
/// Tier C its stress drawings. The two libviprs-dep `real_*` drawings are the
/// same bytes as two Tier A saves and are not rows of their own.
const EXPECTED_TIER_A: usize = 22;
/// libviprs-dep's 41 plus the seventeen drawings this repository generates
/// (`reference/ACadSharp.Generator`, source `generated`).
const EXPECTED_GENERATED: usize = 17;
const EXPECTED_TIER_B: usize = 41 + EXPECTED_GENERATED;
const EXPECTED_TIER_C: usize = 7;
/// libviprs-dep's recordings of those drawings (two of them point at Tier A).
const EXPECTED_RECORDINGS: usize = 45;
/// AutoCAD's own DXF twins and the other DXFs: 12 + 2 + 10 + 2.
const EXPECTED_ORACLE_INPUTS: usize = 26;
/// Distinct census blobs: `sample_base`, AC1014, AC1024, and the one blob five
/// other saves share byte for byte.
const EXPECTED_CENSUS: usize = 4;
/// Drawings the oracle refuses.
const EXPECTED_REFUSALS: usize = 2;

/// The seven stress drawings, by hand.
const TIER_C_IDS: [&str; 7] = [
    "g13_dimension_deep",
    "g13_many_inserts",
    "g13_scale_16x",
    "g13_scale_4x",
    "g13_wide_polyline",
    "g13_wide_spline",
    "g13_xref_long",
];

/// Smallest plausible DWG. The real samples are around a megabyte, so this
/// only has to be large enough to reject an error page or a truncated write.
const MIN_PLAUSIBLE_BYTES: u64 = 1024;

#[test]
fn the_corpus_covers_exactly_the_supported_versions() {
    let got: BTreeSet<&str> = fixtures::all()
        .iter()
        .map(|f| f.acad_version.as_str())
        .collect();
    let want: BTreeSet<&str> = EXPECTED_VERSIONS.iter().copied().collect();

    let missing: Vec<_> = want.difference(&got).collect();
    let extra: Vec<_> = got.difference(&want).collect();

    assert!(
        missing.is_empty(),
        "the corpus is missing {missing:?}. A deleted fixture silently reduces \
         differential coverage, which is the whole reason this list is written \
         out rather than derived."
    );
    assert!(
        extra.is_empty(),
        "the corpus carries {extra:?}, which is not in the supported set. Add it \
         to EXPECTED_VERSIONS with a reason, or remove the fixture."
    );
}

#[test]
fn the_upstream_pin_is_an_exact_commit() {
    let up = fixtures::upstream();
    assert_eq!(
        up.commit.len(),
        40,
        "upstream.commit is {:?}, which is not a full sha. An abbreviated or \
         symbolic ref is not a pin.",
        up.commit
    );
    assert!(
        up.commit
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()),
        "upstream.commit {:?} is not lowercase hex",
        up.commit
    );
    assert_eq!(
        fixtures::sources()
            .get("acadsharp")
            .map(|s| s.commit.as_str()),
        Some(up.commit.as_str()),
        "[upstream] and [sources.acadsharp] are two copies of one pin and must agree"
    );
}

/// `(id, source, source_url, license_evidence)` for every row of every table.
fn every_pinned_row() -> Vec<(String, String, String, String)> {
    let mut rows = Vec::new();
    for f in fixtures::all() {
        rows.push((
            f.id.clone(),
            f.source.clone().unwrap_or_default(),
            f.source_url.clone(),
            f.license_evidence.clone(),
        ));
    }
    for r in fixtures::refusals() {
        rows.push((
            r.id.clone(),
            r.source.clone(),
            r.source_url.clone(),
            r.license_evidence.clone(),
        ));
    }
    for r in fixtures::recordings() {
        rows.push((
            r.id.clone(),
            r.source.clone(),
            r.source_url.clone(),
            r.license_evidence.clone(),
        ));
    }
    for r in fixtures::oracle_inputs() {
        rows.push((
            r.id.clone(),
            r.source.clone(),
            r.source_url.clone(),
            r.license_evidence.clone(),
        ));
    }
    for r in fixtures::censuses() {
        rows.push((
            r.id.clone(),
            r.source.clone(),
            r.source_url.clone(),
            r.license_evidence.clone(),
        ));
    }
    rows
}

#[test]
fn every_row_is_pinned_to_its_own_sources_commit() {
    assert_eq!(
        fixtures::sources().len(),
        3,
        "acadsharp, libviprs_dep and generated"
    );
    let mut checked = 0usize;
    for (id, source, url, licence) in every_pinned_row() {
        let src = fixtures::sources()
            .get(&source)
            .unwrap_or_else(|| panic!("{id}: source {source:?} is not defined"));
        assert_eq!(src.commit.len(), 40, "{source}: commit is not a full sha");
        for floating in ["/master/", "/main/", "/HEAD/"] {
            assert!(
                !url.contains(floating),
                "{id}: source_url uses the floating ref {floating}"
            );
        }
        assert!(
            url.starts_with(&src.url_prefix) && url.contains(&format!("/{}/", src.commit)),
            "{id}: source_url is not on {source}'s prefix at {source}'s commit: {url}"
        );
        assert!(
            licence.starts_with(&src.url_prefix) && licence.contains(&src.commit),
            "{id}: license_evidence is not pinned to {source}'s commit: {licence}"
        );
        checked += 1;
    }
    assert_eq!(
        checked,
        EXPECTED_TIER_A
            + EXPECTED_TIER_B
            + EXPECTED_TIER_C
            + EXPECTED_RECORDINGS
            + EXPECTED_ORACLE_INPUTS
            + EXPECTED_CENSUS
            + EXPECTED_REFUSALS
    );
}

#[test]
fn every_fixture_is_the_file_the_manifest_describes() {
    // The positive control. An empty corpus would make every per-fixture
    // assertion below pass over nothing, so the count is checked first.
    assert_eq!(
        fixtures::all().len(),
        EXPECTED_TIER_A + EXPECTED_TIER_B + EXPECTED_TIER_C,
        "expected {} drawings, found {}",
        EXPECTED_TIER_A + EXPECTED_TIER_B + EXPECTED_TIER_C,
        fixtures::all().len()
    );

    for fx in fixtures::all() {
        let path = fx.path();
        // The one place `read_unverified` is legitimate: this test has to do
        // the comparison itself so a failure can print both hashes.
        let bytes = fx.read_unverified().unwrap_or_else(|e| {
            panic!(
                "{}: cannot read {}: {e}. Run `python3 tools/fetch_acadsharp_fixtures.py fetch` \
                 to restore it from the pinned commit.",
                fx.id,
                path.display()
            )
        });

        assert_eq!(
            bytes.len() as u64,
            fx.bytes,
            "{}: {} bytes on disk, manifest says {}",
            fx.id,
            bytes.len(),
            fx.bytes
        );
        assert!(
            bytes.len() as u64 >= MIN_PLAUSIBLE_BYTES,
            "{}: {} bytes is too small to be a real DWG",
            fx.id,
            bytes.len()
        );

        // The six-byte signature catches an HTML error page, a DXF renamed to
        // .dwg, and a file that is simply the wrong generation.
        let magic = &bytes[..6];
        assert_eq!(
            magic,
            fx.acad_version.as_bytes(),
            "{}: first six bytes are {:?}, expected {:?}",
            fx.id,
            String::from_utf8_lossy(magic),
            fx.acad_version
        );

        let got = format!("{:x}", Sha256::digest(&bytes));
        assert_eq!(
            got, fx.sha256,
            "{}: sha256 mismatch. These are not the bytes the manifest froze.",
            fx.id
        );
    }
}

/// Every file row in every table: id, relative path, sha256.
fn every_file_row() -> Vec<(String, String, String)> {
    let mut rows = Vec::new();
    for f in fixtures::all() {
        rows.push((f.id.clone(), f.file.clone(), f.sha256.clone()));
    }
    for r in fixtures::refusals() {
        rows.push((r.id.clone(), r.file.clone(), r.sha256.clone()));
    }
    for r in fixtures::recordings() {
        rows.push((r.id.clone(), r.file.clone(), r.sha256.clone()));
    }
    for r in fixtures::oracle_inputs() {
        rows.push((r.id.clone(), r.file.clone(), r.sha256.clone()));
    }
    for r in fixtures::censuses() {
        rows.push((r.id.clone(), r.file.clone(), r.sha256.clone()));
    }
    rows
}

#[test]
fn no_two_rows_are_the_same_file() {
    // Across every table, so a drawing is never stored twice under two names:
    // libviprs-dep's `real_AC1018` and `real_AC1032` are the same bytes as two
    // Tier A saves, and are rows of those, not of their own.
    let mut by_hash: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut by_path: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (id, file, sha) in every_file_row() {
        by_hash.entry(sha).or_default().push(id.clone());
        by_path.entry(file).or_default().push(id);
    }
    let dupes: Vec<_> = by_hash.values().filter(|ids| ids.len() > 1).collect();
    assert!(
        dupes.is_empty(),
        "these rows share a sha256, so a file is stored twice or one of them is not the file it \
         claims to be: {dupes:?}"
    );
    let shared: Vec<_> = by_path.values().filter(|ids| ids.len() > 1).collect();
    assert!(shared.is_empty(), "these rows share a path: {shared:?}");
}

#[test]
fn the_corpus_has_the_rows_it_should_per_table_and_tier() {
    assert_eq!(
        fixtures::by_tier("A").len(),
        EXPECTED_TIER_A,
        "tier A drawings"
    );
    assert_eq!(
        fixtures::by_tier("B").len(),
        EXPECTED_TIER_B,
        "tier B drawings"
    );
    assert_eq!(
        fixtures::by_tier("C").len(),
        EXPECTED_TIER_C,
        "tier C drawings"
    );
    assert_eq!(
        fixtures::recordings().len(),
        EXPECTED_RECORDINGS,
        "recordings"
    );
    assert_eq!(
        fixtures::oracle_inputs().len(),
        EXPECTED_ORACLE_INPUTS,
        "oracle inputs"
    );
    assert_eq!(fixtures::censuses().len(), EXPECTED_CENSUS, "census files");
    assert_eq!(fixtures::refusals().len(), EXPECTED_REFUSALS, "refusals");
}

#[test]
fn every_drawing_has_exactly_one_tier_and_a_source() {
    for fx in fixtures::all() {
        let tier = fx.tier.as_deref().unwrap_or("");
        assert!(
            matches!(tier, "A" | "B" | "C"),
            "{}: tier is {:?}, and a drawing has exactly one of A, B or C",
            fx.id,
            fx.tier
        );
        let source = fx.source.as_deref().unwrap_or("");
        assert!(
            fixtures::sources().contains_key(source),
            "{}: source is {:?}, which no [sources.*] table defines",
            fx.id,
            fx.source
        );
    }
}

#[test]
fn tier_c_is_exactly_the_seven_stress_drawings() {
    let mut got: Vec<&str> = fixtures::by_tier("C")
        .iter()
        .map(|f| f.id.as_str())
        .collect();
    got.sort_unstable();
    assert_eq!(got, TIER_C_IDS);
}

#[test]
fn tier_a_is_the_acadsharp_drawings_and_nothing_else() {
    for fx in fixtures::by_tier("A") {
        assert_eq!(
            fx.source.as_deref(),
            Some("acadsharp"),
            "{}: Tier A is AutoCAD-authored, which is ACadSharp's samples",
            fx.id
        );
    }
    for fx in fixtures::by_tier("B")
        .into_iter()
        .chain(fixtures::by_tier("C"))
    {
        // Tier B is also where this repository's own generated drawings go
        // (`tests/generated_fixtures.rs` holds those rows to their provenance).
        let generated =
            fx.tier.as_deref() == Some("B") && fx.source.as_deref() == Some("generated");
        assert!(
            generated || fx.source.as_deref() == Some("libviprs_dep"),
            "{}: Tiers B and C are libviprs-dep's writer-made drawings, or generated here",
            fx.id
        );
    }
}

#[test]
fn the_two_shared_drawings_are_one_row_each_and_both_tiers_point_at_them() {
    // `real_AC1018` and `real_AC1032` are `sample_AC1018` and `sample_AC1032`.
    assert!(
        fixtures::all().iter().all(|f| !f.id.starts_with("real_")),
        "a `real_*` drawing has its own row, which stores the same bytes twice"
    );
    for (stem, id) in [
        ("real_AC1018", "acadsharp-ac1018"),
        ("real_AC1032", "acadsharp-ac1032"),
    ] {
        let rec = fixtures::recordings()
            .iter()
            .find(|r| r.file.ends_with(&format!("/{stem}.txt")))
            .unwrap_or_else(|| panic!("no recording named {stem}"));
        assert_eq!(
            rec.drawing, id,
            "{stem}'s recording must point at the Tier A row"
        );
        assert_eq!(
            fixtures::by_id(id).and_then(|f| f.tier.as_deref()),
            Some("A")
        );
    }
}

#[test]
fn every_recording_names_a_drawing_or_a_refusal_that_exists() {
    for rec in fixtures::recordings() {
        let known = fixtures::by_id(&rec.drawing).is_some()
            || fixtures::refusals().iter().any(|r| r.id == rec.drawing);
        assert!(
            known,
            "{}: names the drawing {:?}, which is neither a [[fixture]] nor a [[refusal]]",
            rec.id, rec.drawing
        );
    }
    // And no drawing has two recordings.
    let mut seen = BTreeSet::new();
    for rec in fixtures::recordings() {
        assert!(
            seen.insert(&rec.drawing),
            "{}: a second recording of {}",
            rec.id,
            rec.drawing
        );
    }
}

#[test]
fn every_other_table_points_at_rows_that_exist() {
    for dxf in fixtures::oracle_inputs() {
        if let Some(twin) = &dxf.for_fixture {
            assert!(
                fixtures::by_id(twin).is_some(),
                "{}: is the DXF twin of {twin}, which is not a [[fixture]]",
                dxf.id
            );
        }
    }
    for c in fixtures::censuses() {
        assert!(!c.for_fixtures.is_empty(), "{}: describes no drawing", c.id);
        for twin in &c.for_fixtures {
            assert!(
                fixtures::by_id(twin).is_some(),
                "{}: is the census of {twin}, which is not a [[fixture]]",
                c.id
            );
        }
        assert_eq!(
            c.also_source_paths.len(),
            c.also_source_urls.len(),
            "{}: every extra upstream path needs its URL",
            c.id
        );
    }
}

#[test]
fn every_refusal_names_the_oracle_exit_code_it_expects() {
    // The oracle's `ExitCode` enum, by hand (reference/ACadSharp.Reference/
    // ExportResult.cs). A refusal row whose name and number disagree is a
    // refusal nobody ran.
    let known = [
        ("ReaderFailure", 3),
        ("UnsupportedVersion", 4),
        ("CanonicalSerializationFailure", 6),
    ];
    for r in fixtures::refusals() {
        assert!(
            known.contains(&(r.oracle_exit.as_str(), r.oracle_exit_code)),
            "{}: {} / {} is not a pair of the oracle's exit codes",
            r.id,
            r.oracle_exit,
            r.oracle_exit_code
        );
    }
    let by_id = |id: &str| fixtures::refusals().iter().find(|r| r.id == id);
    assert_eq!(
        by_id("g13_ac1009").map(|r| r.oracle_exit.as_str()),
        Some("UnsupportedVersion")
    );
    assert_eq!(
        by_id("g13_nan_bulge").map(|r| r.oracle_exit.as_str()),
        Some("CanonicalSerializationFailure")
    );
}

#[test]
fn every_row_of_every_table_loads_and_a_missing_file_does_not() {
    // Every row, verified. The count is the positive control: a loop over no
    // rows loads nothing and passes.
    let mut loaded = 0usize;
    for r in fixtures::refusals() {
        r.load().unwrap_or_else(|e| panic!("{e}"));
        loaded += 1;
    }
    for r in fixtures::recordings() {
        r.load().unwrap_or_else(|e| panic!("{e}"));
        loaded += 1;
    }
    for r in fixtures::oracle_inputs() {
        r.load().unwrap_or_else(|e| panic!("{e}"));
        loaded += 1;
    }
    for r in fixtures::censuses() {
        r.load().unwrap_or_else(|e| panic!("{e}"));
        loaded += 1;
    }
    for f in fixtures::all() {
        f.load().unwrap_or_else(|e| panic!("{e}"));
        loaded += 1;
    }
    assert_eq!(
        loaded,
        EXPECTED_TIER_A
            + EXPECTED_TIER_B
            + EXPECTED_TIER_C
            + EXPECTED_RECORDINGS
            + EXPECTED_ORACLE_INPUTS
            + EXPECTED_CENSUS
            + EXPECTED_REFUSALS
    );

    // A row whose file is gone is an error naming the row.
    let mut ghost = fixtures::recordings()[0].clone();
    ghost.file = "libviprs-dep/recordings/not-there.txt".to_string();
    let err = ghost.load().expect_err("a missing file must not load");
    assert!(err.to_string().contains(&ghost.id), "{err}");
    // And so is one whose bytes are not the frozen ones.
    let mut wrong = fixtures::recordings()[0].clone();
    wrong.sha256 = "0".repeat(64);
    assert!(
        wrong
            .load()
            .expect_err("wrong sha")
            .to_string()
            .contains("sha256")
    );
}

#[test]
fn the_dxf_twins_are_dxf_and_the_census_files_are_json() {
    let mut binary = 0usize;
    for dxf in fixtures::oracle_inputs() {
        let bytes = dxf.load().unwrap_or_else(|e| panic!("{e}"));
        let is_binary = bytes.starts_with(b"AutoCAD Binary DXF");
        if is_binary {
            binary += 1;
        } else {
            let head = String::from_utf8_lossy(&bytes[..bytes.len().min(64)]).to_string();
            assert!(
                head.trim_start().starts_with('0'),
                "{}: not a DXF (an ASCII DXF opens with group code 0): {head:?}",
                dxf.id
            );
        }
        if let Some(enc) = &dxf.encoding {
            assert_eq!(
                enc == "binary",
                is_binary,
                "{}: the row says {enc} and the bytes say otherwise",
                dxf.id
            );
        }
    }
    assert_eq!(binary, 7, "seven binary DXFs: AC1009 and AC1015 to AC1032");
    for c in fixtures::censuses() {
        let bytes = c.load().unwrap_or_else(|e| panic!("{e}"));
        serde_json::from_slice::<serde_json::Value>(&bytes)
            .unwrap_or_else(|e| panic!("{}: not JSON: {e}", c.id));
    }
}

#[test]
fn refusals_are_not_drawings_the_oracle_would_freeze() {
    // The two files the oracle refuses must not be [[fixture]] rows, because
    // `--all` reads every one and would exit non-zero.
    for r in fixtures::refusals() {
        assert!(
            fixtures::by_id(&r.id).is_none(),
            "{}: is both a refusal and a fixture",
            r.id
        );
    }
}

#[test]
fn the_upstream_licence_travels_with_the_fixtures() {
    let up = fixtures::upstream();
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures")
        .join(&up.license_file);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{} must be readable: {e}", path.display()));
    assert!(
        text.contains("MIT License"),
        "{} does not look like the MIT licence the manifest claims",
        path.display()
    );
    assert_eq!(up.license, "MIT");
}

#[test]
fn nothing_sits_in_the_corpus_without_a_manifest_entry() {
    // Any file, anywhere under fixtures/. The documents, the licence texts and
    // `expected/` (claimed by the reference fields, checked below) are the only
    // things that need no row.
    let root = fixtures::fixture_root();
    let mut claimed: BTreeSet<PathBuf> = BTreeSet::new();
    for (_, file, _) in every_file_row() {
        claimed.insert(root.join(file));
    }
    let mut referenced: BTreeSet<PathBuf> = BTreeSet::new();
    for f in fixtures::all() {
        referenced.extend(f.reference_jsonl_path());
        referenced.extend(f.reference_svg_path());
    }
    // The oracle's frozen reading of a DXF is claimed by the `[[oracle_input]]`
    // row it is a reading of (R5.1).
    for i in fixtures::oracle_inputs() {
        if let Some(r) = &i.reference_jsonl {
            referenced.insert(root.join(r));
        }
    }
    // A round-trip output and the oracle's reading of it are claimed by the
    // `[[written]]` row (R5.3).
    for w in fixtures::written() {
        for file in w.files() {
            referenced.insert(root.join(file));
        }
    }
    // A code page drawing and the oracle's reading of it (R5.4).
    for a in fixtures::authored() {
        for file in a.files() {
            referenced.insert(root.join(file));
        }
    }
    // The ezdxf ground truths are claimed by their own rows, which carry the
    // hashes (R3.4).
    for g in fixtures::ground_truths() {
        for file in g.files() {
            referenced.insert(root.join(file));
        }
    }

    // The model-1 ground truths (R5.5) are claimed by fixtures/model1/MANIFEST.toml,
    // which carries their hashes, and the list of names ezdxf left out sits beside
    // each ezdxf stream.
    for f in acadsharp_rs_tests::model1::frozen_files().expect("fixtures/model1/MANIFEST.toml") {
        let path = root.join("model1").join(&f.path);
        if f.engine == "ezdxf" {
            referenced.insert(path.with_extension("skipped"));
        }
        referenced.insert(path);
    }

    let mut orphans = Vec::new();
    let mut found = 0usize;
    for path in files_under(&root, &|_| true) {
        let rel = path.strip_prefix(&root).unwrap_or(&path).to_path_buf();
        let name = rel.to_string_lossy().replace('\\', "/");
        let is_doc = matches!(
            name.as_str(),
            "manifest.toml" | "README.md" | "model1/MANIFEST.toml" | "written_plan.toml" | "codepage_plan.toml"
        ) || (name.starts_with("LICENSE") && name.ends_with(".txt"));
        if is_doc {
            continue;
        }
        if name == "expected/SUMMARY.md" || referenced.contains(&path) {
            continue;
        }
        found += 1;
        if !claimed.contains(&path) {
            orphans.push(name);
        }
    }

    // Positive control. Without it a walker that returned nothing would make
    // the orphan check pass by never looking at anything.
    assert_eq!(
        found,
        claimed.len(),
        "walked {found} files under {} but the manifest claims {}",
        root.display(),
        claimed.len()
    );
    assert!(
        orphans.is_empty(),
        "these files are in the tree but not in fixtures/manifest.toml: {orphans:?}. A file \
         nobody added to the manifest is a file nobody vetted: no provenance, no licence, no \
         hash. Add a row or delete the file."
    );
}

#[test]
fn every_row_carries_its_own_licence_evidence() {
    for (id, source, _url, evidence) in every_pinned_row() {
        assert!(
            evidence.starts_with("https://"),
            "{id}: license_evidence is {evidence:?}, which is not a URL anyone can check"
        );
        assert!(
            fixtures::sources().contains_key(&source),
            "{id}: no such source {source:?}"
        );
    }
    for fx in fixtures::all() {
        assert!(
            !fx.license.trim().is_empty(),
            "{}: no licence recorded, so there is nothing saying these bytes may be redistributed",
            fx.id
        );
    }
    // The drawings that are not ACadSharp's own say where their provenance is
    // written down, pinned to a commit.
    for fx in fixtures::all() {
        if fx.source.as_deref() == Some("libviprs_dep") {
            let prov = fx.provenance_evidence.as_deref().unwrap_or("");
            assert!(
                prov.contains(&fixtures::sources()["libviprs_dep"].commit),
                "{}: provenance_evidence {prov:?} is not pinned to libviprs-dep's commit",
                fx.id
            );
        }
    }
}

#[test]
fn each_sources_licence_text_travels_with_it() {
    for (name, src) in fixtures::sources() {
        let path = fixtures::fixture_root().join(&src.license_file);
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("{name}: {} must be readable: {e}", path.display()));
        assert!(
            text.contains("MIT License"),
            "{name}: {} does not look like the MIT licence the manifest claims",
            path.display()
        );
        assert_eq!(src.license, "MIT", "{name}");
    }
}

#[test]
fn load_refuses_bytes_that_are_not_the_fixture() {
    let fx = fixtures::by_version("AC1018").expect("AC1018 is in the corpus");

    // Proves the verifier is doing something. Without a case like this, `load`
    // could return early on every input and every other test here would still
    // be green, because they all read the real, correct files.
    let mut damaged = fx.load().expect("the committed fixture is intact");
    damaged[fx.bytes as usize / 2] ^= 0xff;
    let err = fx
        .verify(&damaged)
        .expect_err("a flipped byte must not verify");
    let msg = err.to_string();
    assert!(
        msg.contains(&fx.id),
        "the error should name the fixture: {msg}"
    );
    assert!(
        msg.contains("sha256"),
        "the error should say what disagreed: {msg}"
    );

    let truncated = &fx.load().expect("intact")[..512];
    assert!(
        fx.verify(truncated).is_err(),
        "a truncated fixture must not verify"
    );

    let err = fixtures::load("no-such-fixture").expect_err("unknown ids must not resolve");
    assert!(
        err.to_string().contains("no-such-fixture"),
        "the error should name the id it was given: {err}"
    );
}

#[test]
fn no_test_reads_the_corpus_behind_the_loaders_back() {
    // `fixtures::load` verifies before returning. That guarantee is worth
    // exactly as much as the number of callers who use it, so the two ways
    // around it are checked here rather than left to review.
    const BYPASSES: &[(&str, &str)] = &[
        ("fixtures/dwg", "a hardcoded path into the corpus"),
        ("include_bytes!", "a fixture embedded at compile time"),
        ("read_unverified", "the documented escape hatch"),
    ];
    // `fixtures.rs` defines them and this file has to use one to compare
    // hashes. Everything else goes through `load`.
    const ALLOWED: &[&str] = &["src/fixtures.rs", "tests/fixture_integrity.rs"];

    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut offences = Vec::new();
    let mut scanned = 0usize;
    for dir in ["src", "tests"] {
        for path in rust_files_under(&root.join(dir)) {
            let rel = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .display()
                .to_string();
            let rel = rel.replace('\\', "/");
            if ALLOWED.contains(&rel.as_str()) {
                continue;
            }
            scanned += 1;
            let text = std::fs::read_to_string(&path).expect("source file readable");
            for (needle, what) in BYPASSES {
                if text.contains(needle) {
                    offences.push(format!("{rel}: {what} ({needle})"));
                }
            }
        }
    }

    assert!(
        offences.is_empty(),
        "these read the corpus without verifying it: {offences:?}. Use \
         `fixtures::load(id)`, which checks size, signature and sha256 first. If a case \
         genuinely needs raw bytes, add the file to ALLOWED here with a reason.\n\
         (scanned {scanned} files)"
    );
}

/// Every `.rs` under `dir`, recursively.
fn rust_files_under(dir: &std::path::Path) -> Vec<PathBuf> {
    files_under(dir, &|p| p.extension().is_some_and(|e| e == "rs"))
}

fn files_under(dir: &std::path::Path, keep: &dyn Fn(&std::path::Path) -> bool) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            out.extend(files_under(&path, keep));
        } else if keep(&path) {
            out.push(path);
        }
    }
    out.sort();
    out
}
