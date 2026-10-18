//! The round-trip matrix's inventory (R5.3, acadsharp-rs-tests#29).
//!
//! No emitters and no feature: these read the plan, the `[[written]]` rows, the
//! ezdxf readings and the verdict table, so a default `cargo test` holds the
//! matrix's shape even where the run is skipped. The run is `tests/parity.rs`.
//!
//! The counts are written out by hand, the way `EXPECTED_ROWS` is: derive them
//! from the manifest and deleting rows deletes the expectation with them.

use std::collections::BTreeSet;

use acadsharp_rs_tests::fixtures;
use acadsharp_rs_tests::tables;
use acadsharp_rs_tests::verdict::Table;
use acadsharp_rs_tests::written;

const VERDICTS: &str = include_str!("../parity/VERDICTS.toml");

/// ACadSharp: 6 DWG versions, 8 DXF versions in two encodings.
const ACADSHARP_OUTPUTS: usize = 6 + 8 * 2;
/// acadrust: 8 DWG versions, 9 DXF versions in two encodings.
const ACADRUST_OUTPUTS: usize = 8 + 9 * 2;
/// Each DWG output is read by the oracle, acadrust and acadsharp-rs; each DXF
/// output by the oracle, acadrust and ezdxf.
const READERS_PER_OUTPUT: usize = 3;
/// Outputs whose frozen oracle reading is not schema 1 (ACadSharp's reader emits a
/// warning with garbage in it). acadrust's AC1014 and AC1018 DWGs were two, until
/// the objects of classes the older saves do not register stopped being written as
/// type 500 (libviprs/acadsharp-rs-tests#112); the list stays so that a new one is
/// looked at.
const DAMAGED_READINGS: [&str; 0] = [];
/// DWG outputs: 6 from ACadSharp and 8 from acadrust.
const DWG_OUTPUTS: usize = 6 + 8;

#[test]
fn the_plan_names_the_outputs_the_issue_counts() {
    let outputs = written::outputs("one");
    assert_eq!(
        outputs.len(),
        ACADSHARP_OUTPUTS + ACADRUST_OUTPUTS,
        "22 from ACadSharp and 26 from acadrust"
    );
    let ids: BTreeSet<String> = outputs.iter().map(|o| o.id()).collect();
    assert_eq!(ids.len(), outputs.len(), "an id names one output");
    assert_eq!(
        outputs.iter().filter(|o| o.container == "dwg").count(),
        DWG_OUTPUTS
    );
}

#[test]
fn every_planned_output_is_a_row_and_every_row_is_planned() {
    let plan: BTreeSet<String> = written::outputs("one").iter().map(|o| o.id()).collect();
    let rows: BTreeSet<String> = fixtures::written().iter().map(|w| w.id.clone()).collect();
    assert_eq!(rows.len(), fixtures::written().len(), "a row appears once");
    assert_eq!(
        plan.difference(&rows).collect::<Vec<_>>(),
        Vec::<&String>::new(),
        "planned and not in the manifest"
    );
    assert_eq!(
        rows.difference(&plan).collect::<Vec<_>>(),
        Vec::<&String>::new(),
        "in the manifest and not planned"
    );
}

#[test]
fn a_stored_output_is_the_bytes_the_manifest_froze_and_acadrusts_are_not_stored() {
    let mut stored = 0;
    for w in fixtures::written() {
        if w.writer == "acadrust" {
            assert!(!w.is_stored(), "{}: made by the test, not stored", w.id);
        }
        if w.write_failed.is_some() {
            assert!(w.load().is_err());
            continue;
        }
        if w.is_stored() {
            let bytes = w.load().unwrap_or_else(|e| panic!("{}: {e}", w.id));
            w.verify(&bytes).unwrap();
            stored += 1;
        }
    }
    assert_eq!(
        stored, ACADSHARP_OUTPUTS,
        "every ACadSharp output is stored"
    );
}

#[test]
fn every_output_the_oracle_reads_has_a_frozen_reading_and_the_rest_say_why() {
    for w in fixtures::written()
        .iter()
        .filter(|w| w.write_failed.is_none())
    {
        match (&w.reference_jsonl, &w.no_reference) {
            (Some(_), None) => {
                let text = String::from_utf8(
                    w.load_reference_jsonl()
                        .unwrap_or_else(|e| panic!("{}: {e}", w.id)),
                )
                .unwrap();
                match acadsharp_rs_tests::canonical::parse_stream(&text) {
                    Ok(records) => {
                        assert!(records.len() > 10, "{}: {} records", w.id, records.len());
                        assert!(!DAMAGED_READINGS.contains(&w.id.as_str()), "{}", w.id);
                    }
                    // The oracle read a damaged file and wrote a warning whose
                    // entity type is garbage with a control character in it; the
                    // strict parser refuses it, and the cell says so. Listed by
                    // hand so a new one is looked at.
                    Err(e) => {
                        assert!(
                            DAMAGED_READINGS.contains(&w.id.as_str()),
                            "{}: {e}: not a listed damaged reading",
                            w.id
                        );
                        assert!(e.to_string().contains("carriage return"), "{}: {e}", w.id);
                    }
                }
            }
            (None, Some(why)) => assert!(!why.trim().is_empty(), "{}", w.id),
            other => panic!(
                "{}: {other:?}: a reading or a reason, not both and not neither",
                w.id
            ),
        }
    }
}

#[test]
fn ezdxf_has_a_reading_or_a_message_for_every_dxf_output() {
    for w in fixtures::written()
        .iter()
        .filter(|w| w.container.starts_with("dxf") && w.write_failed.is_none())
    {
        let gt = fixtures::ground_truth(&w.id)
            .unwrap_or_else(|| panic!("{}: ezdxf was never asked", w.id));
        assert!(gt.is_written());
        assert!(
            gt.failed.is_some() || gt.load_jsonl().is_ok(),
            "{}: neither a reading nor a message",
            w.id
        );
    }
}

#[test]
fn the_table_keys_of_the_matrix_load() {
    let t = tables::load_all().unwrap();
    for w in ["acadsharp", "acadrust"] {
        for r in ["oracle", "acadrust", "acadsharp", "ezdxf"] {
            assert!(t.contains_key(&format!("w-{w}:{r}")), "w-{w}:{r}");
        }
    }
    assert!(tables::parse_key("w-libredwg:oracle").is_err());
}

/// Every cell of the matrix, written out from the manifest.
fn cells() -> BTreeSet<(String, String)> {
    let mut want = BTreeSet::new();
    for w in fixtures::written() {
        let readers: [&str; 3] = if w.container == "dwg" {
            ["oracle", "acadrust", "acadsharp"]
        } else {
            ["oracle", "acadrust", "ezdxf"]
        };
        for r in readers {
            want.insert((w.id.clone(), format!("w-{}:{r}", w.writer)));
        }
        want.insert((w.id.clone(), format!("w-{}:version", w.writer)));
    }
    want
}

#[test]
fn the_stage_one_table_has_the_cells_the_issue_counts() {
    let outputs = ACADSHARP_OUTPUTS + ACADRUST_OUTPUTS;
    // 144 reader cells, and one row per output for the version it says it is.
    assert_eq!(cells().len(), outputs * READERS_PER_OUTPUT + outputs);
    assert_eq!(cells().len(), 192);
}

#[test]
fn every_cell_has_a_verdict_row_and_nothing_else_is_in_the_matrix() {
    let t = Table::parse(VERDICTS).unwrap();
    let have: BTreeSet<(String, String)> = t
        .rows
        .iter()
        .map(Table::key)
        .filter(|(f, _)| f.starts_with("w-"))
        .collect();
    let want = cells();
    let missing: Vec<_> = want.difference(&have).collect();
    let extra: Vec<_> = have.difference(&want).collect();
    assert!(missing.is_empty(), "no verdict row for {missing:?}");
    assert!(
        extra.is_empty(),
        "a verdict row for a cell nobody computes: {extra:?}"
    );
}

#[test]
fn a_cell_that_loses_a_divergence_its_reader_carries_is_red_and_says_so() {
    // R4.1 lists a divergence acadrust carries on the source (libviprs-dep#112:
    // the embedded-MTEXT attribute). ACadSharp's writers drop that text, so the
    // listed row stops holding in the output, and a row that stops holding is a
    // finding. At least one committed cell shows it, with the writer's issue.
    let t = Table::parse(VERDICTS).unwrap();
    let gone: Vec<_> = t
        .rows
        .iter()
        .filter(|r| r.fixture.starts_with("w-acadsharp-") && r.pair == "w-acadsharp:acadrust")
        .filter(|r| r.first.contains("the listed divergence is gone"))
        .collect();
    assert!(
        !gone.is_empty(),
        "no cell shows a vanished known divergence"
    );
    for r in gone {
        assert!(
            r.issue.contains("acadsharp-rs-tests#114"),
            "{}: {}",
            r.fixture,
            r.issue
        );
    }
}
