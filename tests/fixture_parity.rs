//! The differential's lane test: what each native cell runs against the oracle.
//!
//! The comparison itself, per fixture and per pair, with a reason and an issue
//! on every row that is not a match, is `tests/parity.rs` and
//! `parity/VERDICTS.toml` (R4.1). This file is the part that is about *lanes*:
//! it runs in every cell of the native matrix (every published target, shared
//! and, where the archive is certified, static), and it is what makes a green
//! cell mean something.
//!
//! * **Version lockstep.** `acadsharp_reference_version` in every manifest row
//!   has to be an ACadSharp version the crate's `COMPAT.toml` lists, and, in a
//!   cell with an archive, the one the archive's own `LINKINFO.json` reports. A
//!   differential between two different ACadSharp versions is not a
//!   differential, and this fails before anything is decoded.
//! * **The comparator sees a non-trivial stream.** Every drawing with a
//!   reference output yields at least one primitive from the native side (unless
//!   its row says it has none), and comparing it with the oracle looks at real
//!   fields.
//! * **The test can fail.** `acadsharp-ac1032` decoded natively against
//!   `acadsharp-ac1024`'s frozen output must report a mismatch. If the two
//!   drawings ever became identical this control would fail loudly instead of
//!   the differential passing vacuously.
//! * **The cell is the cell it says it is.** `VIPRS_CELL_MODE` names the link
//!   kind the job intends, and the test fails if the binary was linked the other
//!   way, so a static cell cannot quietly be a shared one.
//! * **Cell output.** With `VIPRS_CELL_OUT` set, the canonical JSONL the native
//!   side produced for each fixture is written there, which is what
//!   `tools/compare_cells.py` holds byte-identical between a target's shared and
//!   static cells.
//!
//! The exporter that turns the native stream into schema 1 is
//! `src/acadsharp_emit.rs` (R3.1). This file adds no exporter of its own.

use std::path::PathBuf;

use acadsharp_rs_tests::fixtures::{self, DwgFixture};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// The fixtures that have a frozen oracle output, which is what a differential
/// needs on its other side.
fn with_reference() -> Vec<&'static DwgFixture> {
    fixtures::all()
        .iter()
        .filter(|f| f.reference_jsonl.is_some())
        .collect()
}

/// The ACadSharp versions the crate declares it has been run against.
fn crate_versions() -> Vec<String> {
    let path = root().join("..").join("acadsharp-rs").join("COMPAT.toml");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("I could not read {}: {e}", path.display()));
    let table: toml::Table = text
        .parse()
        .unwrap_or_else(|e| panic!("{} is not TOML: {e}", path.display()));
    match &table["acadsharp_versions"] {
        toml::Value::String(s) => vec![s.clone()],
        toml::Value::Array(a) => a
            .iter()
            .map(|v| v.as_str().expect("a version string").to_string())
            .collect(),
        other => panic!("acadsharp_versions in COMPAT.toml is {other:?}"),
    }
}

#[test]
fn every_reference_row_names_an_acadsharp_version_the_crate_lists() {
    let listed = crate_versions();
    assert!(!listed.is_empty(), "COMPAT.toml lists no ACadSharp version");
    let rows = with_reference();
    assert!(
        rows.len() >= 70,
        "only {} rows have a reference output; the corpus is 70 drawings",
        rows.len()
    );
    for f in rows {
        let version = f.acadsharp_reference_version.as_deref().unwrap_or_else(|| {
            panic!(
                "{} has a reference output and no acadsharp_reference_version",
                f.id
            )
        });
        assert!(
            listed.iter().any(|v| v == version),
            "{} was frozen with ACadSharp {version}, and the crate's COMPAT.toml lists {listed:?}. \
             A differential between two ACadSharp versions is not a differential.",
            f.id
        );
    }
}

#[cfg(all(feature = "native", not(native_linked)))]
#[test]
fn the_native_half_of_the_lane_test_needs_an_archive() {
    if std::env::var("VIPRS_REQUIRE_NATIVE").is_ok_and(|v| v == "1") {
        panic!(
            "VIPRS_REQUIRE_NATIVE=1 but no native archive was linked. Set ACADSHARP_NATIVE_DIR to an \
             unpacked libviprs-dep archive; it is {:?} here.",
            std::env::var_os("ACADSHARP_NATIVE_DIR")
        );
    }
    eprintln!("skipped: no native archive was linked (ACADSHARP_NATIVE_DIR)");
}

#[cfg(all(feature = "native", native_linked))]
mod native {
    use super::*;
    use acadsharp_rs_tests::acadsharp_emit::extract_bytes;
    use acadsharp_rs_tests::canonical::{parse_stream, render_stream};
    use acadsharp_rs_tests::compare::{ClassTable, Engine, Pair, Stream, WarningRow, compare};
    use acadsharp_rs_tests::emit::Emission;

    /// The link kind this binary was built with, from `build.rs`.
    const LINKED: &str = env!("SUITE_NATIVE_LINK_KIND");

    fn linkinfo() -> serde_json::Value {
        let dir = std::env::var_os("ACADSHARP_NATIVE_DIR")
            .map(PathBuf::from)
            .expect("ACADSHARP_NATIVE_DIR is unset, yet the library linked");
        let path = dir.join("metadata").join("LINKINFO.json");
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
        serde_json::from_str(&text).expect("LINKINFO.json is JSON")
    }

    /// Drawings the reader refuses by one of its own bounds, by design of the
    /// fixture (`tests/emit_acadsharp.rs` lists the same one). Wire 2 stops a
    /// nested expansion at `max_block_depth` with a terminal `LimitExceeded`,
    /// where schema 1 warns and carries on (acadsharp-rs-tests#68).
    const REFUSED_BY_A_BOUND: &[&str] = &["g13_dimension_deep"];

    /// Drawings the published archive (`viprs.1`) can only warn about: it
    /// predates the lowering of SOLID and 3DFACE and the POINT record, so the
    /// stream is warnings and no geometry. The rebuilt archive draws some of them.
    const WARNING_ONLY_ON_VIPRS_1: &[&str] =
        &["g13_face3d", "g13_point", "g13_solid", "g13_unsupported"];

    /// The native side's output for one fixture, or `None` for a drawing the
    /// reader refuses by a bound (anything else that fails is a failure).
    fn actual(f: &DwgFixture) -> Option<Emission> {
        let bytes = f.load().unwrap_or_else(|e| panic!("{}: {e}", f.id));
        match extract_bytes(&bytes) {
            Ok(e) => Some(e),
            Err(e) if REFUSED_BY_A_BOUND.contains(&f.id.as_str()) => {
                assert!(e.to_string().contains("bound"), "{}: {e}", f.id);
                None
            }
            Err(e) => panic!("{} did not decode natively: {e}", f.id),
        }
    }

    /// Whether the lane test should expect geometry from this drawing.
    fn expects_geometry(f: &DwgFixture) -> bool {
        f.no_primitives.is_none()
            && f.tier.as_deref() != Some("C")
            && !WARNING_ONLY_ON_VIPRS_1.contains(&f.id.as_str())
    }

    fn expected(f: &DwgFixture) -> Stream {
        let bytes = f
            .load_reference_jsonl()
            .unwrap_or_else(|e| panic!("{}: {e}", f.id));
        let text = String::from_utf8(bytes).expect("UTF-8 reference");
        Stream::new(parse_stream(&text).unwrap_or_else(|e| panic!("{}: {e}", f.id)))
    }

    fn pair_table() -> ClassTable {
        ClassTable::for_pair(
            Pair::new(Engine::Oracle, Engine::Acadsharp),
            WarningRow::Compare,
        )
    }

    fn primitives(e: &Emission) -> usize {
        e.records
            .iter()
            .filter(|r| !matches!(r.kind(), "document" | "view" | "warning"))
            .count()
    }

    #[test]
    fn the_archive_reports_the_acadsharp_version_every_reference_row_was_frozen_with() {
        let info = linkinfo();
        let archive = info["acadsharp_version"]
            .as_str()
            .expect("LINKINFO.json has an acadsharp_version");
        for f in with_reference() {
            assert_eq!(
                f.acadsharp_reference_version.as_deref(),
                Some(archive),
                "{} was frozen with a different ACadSharp than the archive carries",
                f.id
            );
        }
    }

    #[test]
    fn the_cell_is_linked_the_way_it_says_it_is() {
        match std::env::var("VIPRS_CELL_MODE") {
            Ok(mode) => assert_eq!(
                mode, LINKED,
                "this cell is meant to be a {mode} link and the binary was linked {LINKED}"
            ),
            Err(_) => eprintln!("VIPRS_CELL_MODE is unset; this run is a {LINKED} link"),
        }
        let info = linkinfo();
        if LINKED == "static" {
            assert_eq!(
                info["static_certified"].as_bool(),
                Some(true),
                "a static link was made against an archive whose manifest does not certify it"
            );
        }
    }

    #[test]
    fn every_drawing_with_a_reference_yields_a_primitive_and_a_comparison_that_looks_at_fields() {
        let table = pair_table();
        let mut compared = 0;
        for f in with_reference() {
            let Some(got) = actual(f) else { continue };
            if expects_geometry(f) {
                assert!(
                    primitives(&got) >= 1,
                    "{} decoded natively to no primitive at all, so there is nothing to compare",
                    f.id
                );
            }
            let want = expected(f);
            let report = compare(&f.id, &want, &Stream::new(got.records.clone()), &table)
                .expect("the pair table validates");
            compared += 1;
            if expects_geometry(f) {
                assert!(
                    report.scalars_compared > 0 && !report.aligned.is_empty(),
                    "{}: the comparison looked at {} scalars over {} aligned records, so a green \
                     verdict would mean nothing",
                    f.id,
                    report.scalars_compared,
                    report.aligned.len()
                );
            }
        }
        assert!(compared >= 60, "only {compared} drawings were compared");
    }

    #[test]
    fn control_a_drawing_compared_with_another_versions_output_must_mismatch() {
        // If sample_AC1032 and sample_AC1024 ever became the same drawing, the
        // differential could pass because both sides were empty or equal by
        // construction. This is the case that has to be red, and it has to be red
        // in both directions: a duplicate in the corpus is symmetric.
        let ac1032 = fixtures::by_id("acadsharp-ac1032").expect("the AC1032 sample");
        let ac1024 = fixtures::by_id("acadsharp-ac1024").expect("the AC1024 sample");
        for (native, oracle) in [(ac1032, ac1024), (ac1024, ac1032)] {
            let got = Stream::new(actual(native).expect("the sample decodes").records);
            let want = expected(oracle);
            let report = compare("control", &want, &got, &pair_table()).expect("validates");
            assert!(
                !report.is_match(),
                "{} decoded natively matched {}'s frozen output, so the differential cannot fail",
                native.id,
                oracle.id
            );
            assert!(
                report.red().len() >= 10,
                "only {} red findings of {} against {}; a mismatch this small could be noise",
                report.red().len(),
                native.id,
                oracle.id
            );
            for finding in report.red().iter().take(2) {
                eprintln!("control finding: {}", finding.render());
            }
        }
    }

    #[test]
    fn cell_output() {
        let Some(out) = std::env::var_os("VIPRS_CELL_OUT") else {
            eprintln!("VIPRS_CELL_OUT is unset, so this run writes no cell output");
            return;
        };
        let out = PathBuf::from(out);
        std::fs::create_dir_all(&out).expect("create the cell output directory");
        let mut written = 0;
        for f in with_reference() {
            let Some(got) = actual(f) else { continue };
            let text = render_stream(&got.records)
                .unwrap_or_else(|e| panic!("{}: cannot render: {e:?}", f.id));
            std::fs::write(out.join(format!("{}.acadsharp.jsonl", f.id)), text)
                .expect("write the fixture's output");
            written += 1;
        }
        assert!(written >= 60, "wrote only {written} files");
        eprintln!(
            "cell output: {written} files in {} ({LINKED} link)",
            out.display()
        );
    }
}
