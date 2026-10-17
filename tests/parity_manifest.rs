//! The upstream parity manifest has to describe the upstream it pins.
//!
//! Two directions, and both matter. A file upstream with no row means coverage
//! silently shrank on a bump; a row with no file upstream means the
//! classification is describing something that no longer exists. Either way the
//! number in the README stops being true and nobody finds out.
//!
//! Offline: the file list comes from a committed snapshot named after the pinned
//! commit, so this needs no network and no GitHub token.
//!
//! Every check here runs twice. Once against the real manifest, which is the
//! thing being protected, and once against a manifest built to be wrong, which
//! is the only way to know the check has teeth. A guard that has only ever seen
//! valid input is a guard nobody has tested.

use std::collections::BTreeSet;
use std::path::PathBuf;

use acadsharp_rs_tests::parity::{self, Manifest, Status};

/// What the classification found upstream at the pinned commit.
///
/// Written out rather than derived, for the same reason `EXPECTED_VERSIONS` is
/// in the fixture test: derive it and deleting rows also deletes the
/// expectation, and the suite stays green on a shrunken manifest.
const EXPECTED_ROWS: usize = 112;

fn synthetic(toml: &str) -> Manifest {
    toml::from_str(toml).expect("synthetic manifest must parse")
}

/// A minimal valid manifest, which every negative case below mutates one field
/// of. Keeping the mutation to one field is what makes each case say something.
const GOOD: &str = r#"
[upstream]
repository = "https://github.com/DomCR/ACadSharp"
version = "v3.7.1"
commit = "d7dc111023477d8a9fffc2153139459c95b4f345"
test_root = "src/ACadSharp.Tests"

[[test_file]]
path = "Entities/ArcTests.cs"
status = "differential"
reason = "covered by the JSONL comparison"
"#;

#[test]
fn the_real_manifest_is_valid() {
    let problems = parity::validate(parity::manifest(), parity::crate_root());
    assert!(
        problems.is_empty(),
        "the parity manifest has {} problems:\n  {}",
        problems.len(),
        problems.join("\n  ")
    );
}

#[test]
fn the_manifest_classifies_every_upstream_test_file_and_no_others() {
    let m = parity::manifest();
    let snapshot = m.snapshot_path();
    let listed = std::fs::read_to_string(&snapshot).unwrap_or_else(|e| {
        panic!(
            "{} must exist: {e}. It is the offline record of what upstream carried at the pinned \
             commit; without it this test cannot tell a complete manifest from an empty one.",
            snapshot.display()
        )
    });

    let upstream: BTreeSet<&str> = listed
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    let classified: BTreeSet<&str> = m.test_files.iter().map(|f| f.path.as_str()).collect();

    // The positive control. Both differences below are empty when both sets are
    // empty, so the sizes are checked before the comparison means anything.
    assert_eq!(
        upstream.len(),
        EXPECTED_ROWS,
        "the snapshot lists {} files, expected {EXPECTED_ROWS}",
        upstream.len()
    );
    assert_eq!(
        classified.len(),
        EXPECTED_ROWS,
        "the manifest classifies {} files, expected {EXPECTED_ROWS}",
        classified.len()
    );

    let unclassified: Vec<_> = upstream.difference(&classified).collect();
    let stale: Vec<_> = classified.difference(&upstream).collect();

    assert!(
        unclassified.is_empty(),
        "upstream carries these test files and the manifest says nothing about them: \
         {unclassified:?}. Every one needs a status and a reason before a bump lands, which is \
         the point of this manifest."
    );
    assert!(
        stale.is_empty(),
        "the manifest classifies these and upstream no longer has them: {stale:?}. They were \
         renamed or deleted; the rows have to follow."
    );
}

#[test]
fn coverage_never_counts_what_is_out_of_scope() {
    let m = parity::manifest();
    let c = m.coverage();

    assert_eq!(c.total(), EXPECTED_ROWS);
    assert_eq!(
        c.applicable(),
        c.total() - c.not_applicable,
        "applicable() has to exclude not_applicable, or a writer suite upstream adds would read \
         as a gap here"
    );
    assert!(
        !Status::NotApplicable.is_applicable(),
        "not_applicable counting toward coverage would make the number fall every time upstream \
         adds a writer test, which says nothing about this crate"
    );
    for s in [
        Status::Mirrored,
        Status::Differential,
        Status::NativeConformance,
    ] {
        assert!(s.is_applicable(), "{} must count", s.as_str());
    }

    // Not an assertion about a target, just a record of where this sits, so a
    // reviewer can see the shape without opening the manifest.
    println!(
        "mirrored={} differential={} native_conformance={} not_applicable={} applicable={} total={}",
        c.mirrored,
        c.differential,
        c.native_conformance,
        c.not_applicable,
        c.applicable(),
        c.total()
    );
}

#[test]
fn the_baselines_are_recorded_with_their_reason() {
    let m = parity::manifest();
    assert!(
        !m.baselines.is_empty(),
        "upstream's own per-generation document trees are a second oracle and have to be recorded \
         rather than left implicit"
    );
    for b in &m.baselines {
        assert!(!b.files.is_empty(), "baseline {:?} lists no files", b.kind);
        assert!(
            !b.reason.trim().is_empty(),
            "baseline {:?} has no reason",
            b.kind
        );
    }
}

#[test]
fn every_baseline_file_is_a_census_row_of_the_one_corpus_manifest() {
    // The baseline list used to be a second list of the same files. The corpus
    // manifest owns them now (with provenance and a hash), and this list points
    // at its rows rather than being another one. Upstream stores five of the
    // seven trees under several names with identical bytes, so a baseline file
    // is found by its primary or one of its `also_source_paths`.
    let m = parity::manifest();
    let mut resolved = 0usize;
    for b in m.baselines.iter().filter(|b| b.kind == "document_tree") {
        for file in &b.files {
            let upstream_path = format!("src/ACadSharp.Tests/{file}");
            let row = acadsharp_rs_tests::fixtures::censuses().iter().find(|c| {
                c.source_path == upstream_path || c.also_source_paths.contains(&upstream_path)
            });
            assert!(
                row.is_some(),
                "baseline file {file} is not a [[census]] row of fixtures/manifest.toml, so it \
                 has no provenance and no hash"
            );
            resolved += 1;
        }
    }
    assert_eq!(resolved, 7, "the seven per-generation document trees");
}

// ---------------------------------------------------------------------------
// The same checks, against manifests built to fail them. Without these, every
// assertion above is a claim that the real manifest happens to be fine, not a
// claim that a broken one would be caught.
// ---------------------------------------------------------------------------

fn problems_for(toml: &str) -> String {
    parity::validate(&synthetic(toml), parity::crate_root()).join("\n")
}

#[test]
fn validate_accepts_a_good_manifest() {
    // The positive control for every negative case below. A `validate` that
    // returned a problem for everything would pass all of them.
    assert_eq!(
        parity::validate(&synthetic(GOOD), parity::crate_root()),
        Vec::<String>::new()
    );
}

#[test]
fn validate_refuses_a_pin_that_is_not_a_pin() {
    let branch = GOOD.replace("d7dc111023477d8a9fffc2153139459c95b4f345", "master");
    assert!(problems_for(&branch).contains("not a 40-character"));

    let short = GOOD.replace("d7dc111023477d8a9fffc2153139459c95b4f345", "d7dc111");
    assert!(problems_for(&short).contains("not a 40-character"));
}

#[test]
fn validate_refuses_a_status_with_no_reason() {
    let no_reason = GOOD.replace(r#"reason = "covered by the JSONL comparison""#, "");
    let p = problems_for(&no_reason);
    assert!(p.contains("no reason"), "{p}");
    assert!(
        p.contains("ArcTests.cs"),
        "the problem must name the file: {p}"
    );
}

#[test]
fn validate_refuses_a_mirrored_row_pointing_at_nothing() {
    let missing = GOOD.replace(
        r#"status = "differential""#,
        "status = \"mirrored\"\nrust = [\"tests/does_not_exist.rs\"]",
    );
    let p = problems_for(&missing);
    assert!(p.contains("does not exist"), "{p}");

    let empty = GOOD.replace(r#"status = "differential""#, r#"status = "mirrored""#);
    assert!(problems_for(&empty).contains("lists no rust file"));
}

#[test]
fn validate_refuses_a_plan_that_has_already_happened() {
    // `Cargo.toml` stands in for "a file that exists". The rule is that once a
    // planned path is real the row has stopped being a plan, and leaving it
    // alone would understate coverage forever.
    let landed = GOOD.replace(
        r#"reason = "covered by the JSONL comparison""#,
        "reason = \"covered by the JSONL comparison\"\nplanned_rust = \"Cargo.toml\"",
    );
    let p = problems_for(&landed);
    assert!(p.contains("no longer a plan"), "{p}");
    assert!(p.contains("Promote it to mirrored"), "{p}");

    // and it stays quiet while the plan is still a plan
    let pending = GOOD.replace(
        r#"reason = "covered by the JSONL comparison""#,
        "reason = \"covered by the JSONL comparison\"\nplanned_rust = \"tests/entities/arc.rs\"",
    );
    assert_eq!(problems_for(&pending), "");
}

#[test]
fn validate_refuses_a_file_classified_twice() {
    // Two rows for one file means one of them is being ignored and nothing
    // says which, so the classification silently stops being what it reads as.
    let twice = format!(
        "{GOOD}\n[[test_file]]\npath = \"Entities/ArcTests.cs\"\nstatus = \"native_conformance\"\n         reason = \"a second, contradictory row\"\n"
    );
    let p = problems_for(&twice);
    assert!(p.contains("classified 2 times"), "{p}");
    assert!(
        p.contains("ArcTests.cs"),
        "the problem must name the file: {p}"
    );
}

#[test]
fn validate_refuses_an_empty_manifest() {
    let empty = GOOD.split("[[test_file]]").next().unwrap().to_string();
    assert!(problems_for(&empty).contains("classifies no files"));
}

#[test]
fn the_snapshot_is_named_after_the_commit_it_describes() {
    // A fixed filename would let a bump reuse the previous revision's file list
    // and compare the new manifest against the old upstream, which would pass
    // and mean nothing.
    let m = parity::manifest();
    let name = m.snapshot_path();
    let name: PathBuf = name.file_name().expect("a filename").into();
    assert_eq!(
        name.to_string_lossy(),
        format!("inventory.{}.txt", m.upstream.commit)
    );
}

// ---------------------------------------------------------------------------
// #49: a row may say something different for acadrust, and may name its evidence.
// ---------------------------------------------------------------------------

const DXF_ROW: &str = r#"
[[test_file]]
path = "IO/DXF/DxfReaderTests.cs"
status = "not_applicable"
reason = "DXF, and the crate opens only DWG"
acadrust = { status = "differential", reason = "acadrust reads DXF (R5.1)" }
"#;

#[test]
fn validate_refuses_an_acadrust_override_with_no_reason() {
    let bad = format!(
        "{GOOD}{}",
        DXF_ROW.replace(r#", reason = "acadrust reads DXF (R5.1)""#, "")
    );
    let p = problems_for(&bad);
    assert!(p.contains("DxfReaderTests.cs"), "{p}");
    assert!(p.contains("acadrust"), "{p}");
    assert!(p.contains("no reason"), "{p}");
}

#[test]
fn validate_refuses_an_acadrust_override_that_restates_the_row() {
    // An override equal to the row it sits on says nothing and hides which rows differ.
    let bad = format!(
        "{GOOD}{}",
        DXF_ROW.replace(
            r#"acadrust = { status = "differential", reason = "acadrust reads DXF (R5.1)" }"#,
            r#"acadrust = { status = "not_applicable", reason = "DXF, and the crate opens only DWG" }"#
        )
    );
    let p = problems_for(&bad);
    assert!(p.contains("restates"), "{p}");
}

#[test]
fn validate_accepts_an_acadrust_override() {
    assert_eq!(problems_for(&format!("{GOOD}{DXF_ROW}")), "");
}

#[test]
fn validate_refuses_evidence_that_resolves_to_nothing() {
    let with = |ev: &str| {
        GOOD.replace(
            r#"status = "differential""#,
            &format!("status = \"differential\"\nevidence = [{ev}]"),
        )
    };

    let p = problems_for(&with(r#""fixture:no-such-fixture""#));
    assert!(p.contains("no-such-fixture"), "{p}");
    assert!(p.contains("fixtures/manifest.toml"), "{p}");

    let p = problems_for(&with(r#""path:tests/does_not_exist.rs""#));
    assert!(p.contains("does_not_exist.rs"), "{p}");

    let p = problems_for(&with(r#""verdict:sample_AC1032:oracle-vs-acadrust""#));
    assert!(
        p.contains("verdict:"),
        "a kind with no source yet is refused, not trusted: {p}"
    );

    // controls: evidence that does resolve is quiet
    assert_eq!(problems_for(&with(r#""path:Cargo.toml""#)), "");
    let id = acadsharp_rs_tests::fixtures::all()[0].id.clone();
    assert_eq!(problems_for(&with(&format!("\"fixture:{id}\""))), "");
}

#[test]
fn the_rows_that_are_wrong_for_acadrust_carry_an_override() {
    let raw: toml::Value =
        toml::from_str(include_str!("../upstream_parity/manifest.toml")).unwrap();
    let rows = raw["test_file"].as_array().unwrap();
    let status_of = |p: &str| {
        let r = rows.iter().find(|r| r["path"].as_str() == Some(p)).unwrap();
        r.get("acadrust")
            .map(|o| o["status"].as_str().unwrap().to_string())
    };
    let over = rows.iter().filter(|r| r.get("acadrust").is_some()).count();
    // Nine DXF rows and seven writer rows, plus one per `mirrored` row: a mirrored
    // test (H4.2) goes through acadsharp-rs's safe API, so acadrust must not
    // inherit the status. It is `differential`, covered by the oracle comparison.
    let mirrored: Vec<&toml::Value> = rows
        .iter()
        .filter(|r| r["status"].as_str() == Some("mirrored"))
        .collect();
    for r in &mirrored {
        let o = r.get("acadrust").unwrap_or_else(|| {
            panic!(
                "{} is mirrored for acadsharp-rs and shares that with acadrust",
                r["path"]
            )
        });
        assert_ne!(
            o["status"].as_str(),
            Some("mirrored"),
            "{}: acadrust has no Rust mirror of this suite",
            r["path"]
        );
    }
    assert_eq!(
        over,
        16 + mirrored.len(),
        "nine DXF rows and seven writer rows, and one per mirrored row"
    );

    for p in [
        "IO/DXF/DxfReaderTests.cs",
        "IO/DXF/DxfWriterTests.cs",
        "IO/DXF/DxfWriterSingleObjectTests.cs",
        "IO/DWG/DwgWriterTests.cs",
        "IO/DWG/DwgWriterSingleObjectTests.cs",
        "IO/RewriteSamplesTests.cs",
        "IO/WriterSingleObjectTests.cs",
    ] {
        assert_eq!(
            status_of(p).as_deref(),
            Some("differential"),
            "{p} applies to acadrust"
        );
    }
    // ACadSharp internals stay out for both, with a reason that says so.
    for p in [
        "DxfMapTests.cs",
        "DxfPropertyTests.cs",
        "GroupCodeValueTests.cs",
        "Internal/DxfMapTests.cs",
        "IO/DXF/DxfAcdsDataSectionTests.cs",
        "IO/DXF/DxfAcisSatTextTests.cs",
        "Internal/DwgHandleWriterTests.cs",
        "Internal/DwgHeaderWriterTests.cs",
        "Internal/DwgSectionWriterTestBase.cs",
    ] {
        assert_eq!(
            status_of(p).as_deref(),
            Some("not_applicable"),
            "{p} stays out"
        );
    }
}
