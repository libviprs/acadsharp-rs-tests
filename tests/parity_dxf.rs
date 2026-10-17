//! The DXF lane's offline checks (R5.1, #27).
//!
//! No emitters and no feature: these read the manifest, the frozen oracle
//! readings, `parity/TABLES.toml` and `parity/VERDICTS.toml`, so a default
//! `cargo test` holds the lane's inventory and its table keys even where the run
//! itself is skipped. The run is `tests/parity.rs`.

use std::collections::BTreeSet;

use acadsharp_rs_tests::canonical::{Record, parse_stream};
use acadsharp_rs_tests::compare::{BOUNDED, BOUNDED_TIGHT, Class};
use acadsharp_rs_tests::fixtures;
use acadsharp_rs_tests::tables;
use acadsharp_rs_tests::verdict::{Table, Verdict};

const VERDICTS: &str = include_str!("../parity/VERDICTS.toml");

/// How many DXFs the oracle reads into schema 1: the 26 in the manifest less the
/// two AC1009 files, which are R5.2's. Written out so a row that silently stops
/// being read is red.
const READ_BY_THE_ORACLE: usize = 24;

fn inputs() -> Vec<&'static fixtures::OracleInput> {
    fixtures::oracle_inputs()
        .iter()
        .filter(|i| i.no_reference.is_none())
        .collect()
}

#[test]
fn the_oracle_reads_twenty_four_dxfs_and_says_why_it_does_not_read_the_other_two() {
    assert_eq!(fixtures::oracle_inputs().len(), 26);
    assert_eq!(inputs().len(), READ_BY_THE_ORACLE);
    for i in fixtures::oracle_inputs()
        .iter()
        .filter(|i| i.no_reference.is_some())
    {
        assert!(i.id.contains("ac1009"), "{}: only R12 is deferred", i.id);
        assert!(i.reference_jsonl.is_none(), "{}", i.id);
    }
}

#[test]
fn every_frozen_dxf_reading_is_the_one_the_manifest_froze_and_says_dxf() {
    for i in inputs() {
        let text = String::from_utf8(
            i.load_reference_jsonl()
                .unwrap_or_else(|e| panic!("{}: {e}", i.id)),
        )
        .unwrap();
        let records = parse_stream(&text).unwrap_or_else(|e| panic!("{}: {e}", i.id));
        let Some(Record::Document(d)) = records.first() else {
            panic!("{}: the first record is not the document", i.id);
        };
        assert_eq!(d.format, "dxf", "{}: a DXF reading says dxf", i.id);
        if let Some(v) = &i.acad_version {
            assert_eq!(&d.acad_version, v, "{}", i.id);
        }
        // A reading that is only the framing compares as agreement with anything.
        if i.no_primitives.is_none() {
            assert!(records.len() > 10, "{}: {} records", i.id, records.len());
        }
    }
}

#[test]
fn the_dxf_pair_keys_load_with_the_float_class_the_issue_names() {
    let t = tables::load_all().unwrap();
    for e in ["oracle", "acadrust"] {
        let c = &t[&format!("dxf-dwg:{e}")];
        assert_eq!(
            (c.class, c.tolerance),
            (Class::Bounded, BOUNDED),
            "dxf-dwg:{e}"
        );
    }
    for e in ["oracle", "acadrust", "ezdxf"] {
        let c = &t[&format!("dxf-ascii-binary:{e}")];
        assert_eq!(
            (c.class, c.tolerance),
            (Class::BoundedTight, BOUNDED_TIGHT),
            "dxf-ascii-binary:{e}"
        );
    }
    for p in [
        "dxf:oracle/acadrust",
        "dxf:ezdxf/oracle",
        "dxf:ezdxf/acadrust",
    ] {
        assert!(t.contains_key(p), "{p}");
    }
    assert!(
        tables::parse_key("dxf-dwg:acadsharp").is_err(),
        "the C ABI opens DWG only"
    );
}

/// Every cell the lane computes, written out from the manifest.
fn cells() -> BTreeSet<(String, String)> {
    let mut want = BTreeSet::new();
    for i in inputs() {
        for p in [
            "dxf:oracle/acadrust",
            "dxf:ezdxf/oracle",
            "dxf:ezdxf/acadrust",
        ] {
            want.insert((i.id.clone(), p.to_string()));
        }
        if i.for_fixture.is_some() {
            for p in ["dxf-dwg:oracle", "dxf-dwg:acadrust", "dxf-dwg:acadsharp"] {
                want.insert((i.id.clone(), p.to_string()));
            }
        }
        if i.encoding.as_deref() == Some("ascii") {
            let save = i.id.replace("-ascii", "");
            for e in ["oracle", "acadrust", "ezdxf"] {
                want.insert((save.clone(), format!("dxf-ascii-binary:{e}")));
            }
        }
    }
    want
}

#[test]
fn the_lane_has_the_cells_the_issue_counts() {
    // 24 inputs x 3 reader pairs; 23 with a DWG twin (all but geoloc_AC1021) x 3 (two readers and the
    // excluded acadsharp-rs); 6 saves x 3 readers.
    let want = cells();
    assert_eq!(want.len(), 24 * 3 + 23 * 3 + 6 * 3, "{}", want.len());
}

#[test]
fn every_cell_of_the_lane_has_a_verdict_row_and_acadsharp_rs_is_excluded_not_missing() {
    let t = Table::parse(VERDICTS).unwrap();
    let have: BTreeSet<(String, String)> = t
        .rows
        .iter()
        .map(Table::key)
        .filter(|(_, p)| p.starts_with("dxf"))
        .collect();
    let want = cells();
    let missing: Vec<_> = want.difference(&have).collect();
    let extra: Vec<_> = have.difference(&want).collect();
    assert!(missing.is_empty(), "no verdict row for {missing:?}");
    assert!(
        extra.is_empty(),
        "a verdict row for a cell the lane does not compute: {extra:?}"
    );
    for r in t.rows.iter().filter(|r| r.pair == "dxf-dwg:acadsharp") {
        assert_eq!(
            Verdict::parse(&r.verdict),
            Some(Verdict::Excluded),
            "{}",
            r.fixture
        );
        assert!(r.reason.contains("DWG only"), "{}: {}", r.fixture, r.reason);
    }
}
