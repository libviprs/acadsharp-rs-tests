//! The committed verdict table and the summary generated from it (R4.1, #25).
//!
//! No emitters and no feature: these read `parity/VERDICTS.toml`,
//! `parity/SUMMARY.md` and `parity/TABLES.toml`, so a default `cargo test`
//! holds the committed table to its own rules even where the run itself is
//! skipped.

use std::collections::BTreeSet;

use acadsharp_rs_tests::fixtures;
use acadsharp_rs_tests::tables;
use acadsharp_rs_tests::verdict::{Table, Verdict, summary};

const VERDICTS: &str = include_str!("../parity/VERDICTS.toml");
const SUMMARY: &str = include_str!("../parity/SUMMARY.md");

fn table() -> Table {
    Table::parse(VERDICTS).unwrap_or_else(|e| panic!("parity/VERDICTS.toml: {e}"))
}

#[test]
fn a_row_that_is_not_a_match_has_a_reason_and_an_issue() {
    let refusals = table().refusals();
    assert!(refusals.is_empty(), "{}", refusals.join("\n"));
}

#[test]
fn the_committed_summary_is_what_the_generator_writes() {
    assert_eq!(
        SUMMARY,
        summary(&table()),
        "parity/SUMMARY.md is stale: regenerate it from parity/VERDICTS.toml (PARITY_REGEN_SUMMARY=1 cargo test --test parity_table)"
    );
}

#[test]
fn regenerate_the_summary_when_asked() {
    if std::env::var("PARITY_REGEN_SUMMARY").as_deref() == Ok("1") {
        std::fs::write(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("parity/SUMMARY.md"),
            summary(&table()),
        )
        .unwrap();
    }
}

#[test]
fn every_fixture_has_a_row_for_every_engine_pair_and_nothing_else_is_in_the_table() {
    let t = table();
    let mut want = BTreeSet::new();
    for f in fixtures::all() {
        for pair in ["oracle/acadsharp", "oracle/acadrust", "acadsharp/acadrust"] {
            want.insert((f.id.clone(), pair.to_string()));
        }
    }
    let have: BTreeSet<(String, String)> = t
        .rows
        .iter()
        .map(Table::key)
        .filter(|(_, p)| {
            ["oracle/acadsharp", "oracle/acadrust", "acadsharp/acadrust"].contains(&p.as_str())
        })
        .collect();
    let missing: Vec<_> = want.difference(&have).collect();
    let extra: Vec<_> = have.difference(&want).collect();
    assert!(missing.is_empty(), "rows missing: {missing:?}");
    assert!(
        extra.is_empty(),
        "rows for nothing in the manifest: {extra:?}"
    );
}

#[test]
fn the_real_drawings_against_acadrust_are_known_one_and_cite_the_two_issues() {
    let t = table();
    let by = t.by_key();
    for (fixture, issue) in [
        ("acadsharp-ac1018", "libviprs/libviprs-dep#111"),
        ("acadsharp-ac1032", "libviprs/libviprs-dep#112"),
    ] {
        let r = by
            .get(&(fixture.to_string(), "oracle/acadrust".to_string()))
            .unwrap_or_else(|| panic!("no row for {fixture}"));
        assert_eq!(
            Verdict::parse(&r.verdict),
            Some(Verdict::Known(1)),
            "{fixture}"
        );
        assert!(r.issue.contains(issue), "{fixture} cites {:?}", r.issue);
    }
}

#[test]
fn the_table_names_a_run_for_each_architecture_it_was_confirmed_on() {
    let t = table();
    assert!(
        !t.runs.is_empty(),
        "SUMMARY.md lists the runs that confirmed the table"
    );
    for r in &t.runs {
        assert!(
            r.acadsharp_rs.len() == 40 && r.acadrust.len() == 40,
            "{r:?}"
        );
        assert!(!r.archive.is_empty() && !r.date.is_empty(), "{r:?}");
    }
}

#[test]
fn every_issue_a_row_cites_is_written_as_a_repository_and_a_number() {
    let t = table();
    for r in &t.rows {
        for issue in r.issue.split(',').map(str::trim).filter(|s| !s.is_empty()) {
            let (repo, n) = issue
                .rsplit_once('#')
                .unwrap_or_else(|| panic!("{} {}: {issue}", r.fixture, r.pair));
            assert!(
                repo.starts_with("libviprs/") && n.chars().all(|c| c.is_ascii_digit()),
                "{} {}: {issue}",
                r.fixture,
                r.pair
            );
        }
    }
}

#[test]
fn the_pair_tables_load_and_every_issue_in_them_is_a_repository_and_a_number() {
    let tables = tables::load_all().unwrap();
    for pair in ["oracle/acadsharp", "oracle/acadrust", "acadsharp/acadrust"] {
        assert!(tables.contains_key(pair), "{pair}");
    }
    for t in tables.values() {
        let issues = t
            .excluded
            .iter()
            .map(|x| x.issue.clone())
            .chain(t.known.iter().map(|k| k.issue.clone()));
        for i in issues {
            assert!(
                i.starts_with("libviprs/") && i.contains('#') && !i.contains("TBD"),
                "{i}"
            );
        }
    }
}

#[test]
fn every_dxf_twin_has_a_row_against_each_engine_and_every_save_one_against_itself() {
    let t = table();
    let have: BTreeSet<(String, String)> = t.rows.iter().map(Table::key).collect();
    let mut twins = 0;
    for gt in fixtures::ground_truths() {
        let has_dwg = fixtures::oracle_inputs()
            .iter()
            .any(|i| i.id == gt.input && i.for_fixture.is_some());
        if !has_dwg {
            continue;
        }
        twins += 1;
        for pair in ["ezdxf/oracle", "ezdxf/acadsharp", "ezdxf/acadrust"] {
            assert!(
                have.contains(&(gt.input.clone(), pair.to_string())),
                "no {pair} row for the DXF {}",
                gt.input
            );
        }
    }
    assert!(twins >= 20, "only {twins} DXF twins with a DWG twin");
    for save in [
        "acadsharp-ac1014",
        "acadsharp-ac1015",
        "acadsharp-ac1018",
        "acadsharp-ac1021",
        "acadsharp-ac1024",
        "acadsharp-ac1027",
    ] {
        for pair in ["self/oracle", "self/acadsharp", "self/acadrust"] {
            assert!(
                have.contains(&(save.to_string(), pair.to_string())),
                "no {pair} row for {save}"
            );
        }
    }
}
