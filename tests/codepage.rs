//! The code page lane's inventory (R5.4, acadsharp-rs-tests#30): what the plan asks for,
//! what the manifest holds, and what the committed verdict table covers. Offline, no run.
//! The counts are written out by hand so a change to the plan is a change to this file.

use acadsharp_rs_tests::codepage::{outputs, plan};
use acadsharp_rs_tests::fixtures;
use std::collections::BTreeSet;

fn table() -> toml::Table {
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("parity/CODEPAGE_VERDICTS.toml");
    std::fs::read_to_string(p).unwrap().parse().unwrap()
}

#[test]
fn the_plan_has_fourteen_families_and_112_outputs() {
    let p = plan();
    assert_eq!(p.family.len(), 14);
    assert_eq!(p.writers, ["acadsharp", "acadrust"].map(String::from));
    assert_eq!(p.containers, ["dwg", "dxf-ascii"].map(String::from));
    assert_eq!(p.versions, ["AC1015", "AC1018"].map(String::from));
    assert_eq!(outputs().len(), 14 * 2 * 2 * 2);
}

#[test]
fn every_family_has_held_unheld_and_literal_text() {
    let p = plan();
    for f in &p.family {
        let kinds: Vec<_> = f.texts(&p.unheld.text).iter().map(|a| a.kind).collect();
        assert!(kinds.starts_with(&["held", "unheld", "literal"]), "{}", f.header);
    }
    let cjk = p.family.iter().filter(|f| f.mif.is_some()).count();
    assert_eq!(cjk, 4, "932, 936, 949 and 950 carry a MIF string");
}

#[test]
fn the_manifest_holds_one_authored_drawing_per_output() {
    let want: BTreeSet<String> = outputs().iter().map(|o| o.id()).collect();
    let have: BTreeSet<String> = fixtures::authored().iter().map(|a| a.id.clone()).collect();
    assert_eq!(want, have);
}

#[test]
fn the_verdict_table_has_a_row_per_drawing_and_reader() {
    let t = table();
    let rows = t["row"].as_array().unwrap();
    // 56 DWG drawings read by acadrust, acadsharp-rs and the oracle; 56 DXF drawings
    // read by acadrust, the oracle and ezdxf.
    assert_eq!(rows.len(), 56 * 3 + 56 * 3);
    let keys: BTreeSet<_> = rows
        .iter()
        .map(|r| (r["fixture"].as_str().unwrap(), r["pair"].as_str().unwrap()))
        .collect();
    assert_eq!(keys.len(), rows.len(), "no row twice");
}

#[test]
fn every_row_that_is_not_a_match_has_a_reason_and_an_issue() {
    for r in table()["row"].as_array().unwrap() {
        if r["verdict"].as_str().unwrap() != "MATCH" {
            for k in ["reason", "issue", "first"] {
                assert!(
                    !r.get(k).and_then(|v| v.as_str()).unwrap_or("").trim().is_empty(),
                    "{} {} has no {k}",
                    r["fixture"],
                    r["pair"]
                );
            }
        }
    }
}
