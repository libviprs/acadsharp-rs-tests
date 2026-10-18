//! Stage two of the round-trip matrix runs in the nightly and nowhere else
//! (R5.3.1, acadsharp-rs-tests#41).
//!
//! No emitters and no feature. libviprs-tests holds the same line with
//! `nightly_selects_ignored_tests_by_target_not_crate_wide`: a crate-wide
//! `--ignored` runs everything anybody ever marked slow, so the selector has to
//! name its target, and what is ignored has to stay ignored on the PR gate.

use std::collections::BTreeSet;
use std::path::Path;

use acadsharp_rs_tests::fixtures;
use acadsharp_rs_tests::verdict::Table;
use acadsharp_rs_tests::written;

fn read(rel: &str) -> String {
    std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join(rel))
        .unwrap_or_else(|e| panic!("{rel}: {e}"))
}

/// Cells of one stage-two source: three readers for each of 48 outputs, and a
/// version row each. Written out; `tests/parity.rs` holds the same figure per source.
const CELLS_PER_SOURCE: usize = 48 * 3 + 48;
/// Sources: the six other saves, `sample_base.dwg`, and the 41 Tier B synthetics.
const SOURCES: usize = 6 + 1 + 41;
/// Refusal fixtures have no cells; named so they are not counted as zero by accident.
const REFUSALS: [&str; 2] = ["g13_ac1009", "g13_nan_bulge"];

#[test]
fn the_stage_two_function_is_ignored_and_the_only_one() {
    let parity = read("tests/parity.rs");
    let at = parity
        .find("fn roundtrip_stage_two()")
        .expect("the stage two function");
    let before = &parity[..at];
    let attrs = before.rsplit("\n\n").next().unwrap();
    assert!(
        attrs.contains("#[test]") && attrs.contains("#[ignore"),
        "roundtrip_stage_two must carry #[ignore], or the PR gate runs the nightly:\n{attrs}"
    );
    // Nothing else in the parity target is ignored: one nightly, one selector.
    assert_eq!(
        parity.matches("#[ignore").count(),
        1,
        "exactly one ignored test in tests/parity.rs"
    );
}

#[test]
fn the_selector_names_its_target_and_the_default_lines_never_run_ignored_tests() {
    let script = read("tools/run-tests.sh");
    let mut selectors = 0;
    for line in script.lines().filter(|l| l.contains("--ignored")) {
        let code = line.trim_start();
        if code.starts_with('#') {
            continue;
        }
        selectors += 1;
        assert!(
            line.contains("--test parity") && line.contains("--exact roundtrip_stage_two"),
            "an --ignored that is not `--test parity ... --exact roundtrip_stage_two`: {line}"
        );
    }
    assert_eq!(selectors, 1, "one selector, in the --stage-two branch");
    // The branch is behind the switch and is not the default.
    let branch = script
        .find("if [ -n \"$STAGE_TWO_DIR\" ]")
        .expect("the --stage-two branch");
    let first_default = script.find("cargo test \"${LOCKED[@]}\" --no-fail-fast --all-targets").unwrap();
    assert!(branch < first_default, "the switch returns before the default lines");
}

#[test]
fn no_workflow_runs_ignored_tests_while_actions_is_off() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join(".github/workflows");
    for entry in std::fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|e| e == "yml") {
            let text = std::fs::read_to_string(&path).unwrap();
            assert!(
                !text.lines().any(|l| l.trim_start().starts_with("run:") && l.contains("--ignored"))
                    && !text.contains("-- --ignored"),
                "{}: runs ignored tests; stage two is the nightly's, through tools/run-tests.sh --stage-two",
                path.display()
            );
        }
    }
}

#[test]
fn stage_two_is_the_sources_the_issue_names_and_none_of_it_is_committed() {
    let plan = written::plan();
    let two = &plan.stage["two"];
    assert!(!two.committed, "stage two's outputs are not rows of the manifest");
    assert!(plan.stage["one"].committed);
    assert_eq!(two.sources.len(), SOURCES);
    let ids: BTreeSet<&str> = two.sources.iter().map(String::as_str).collect();
    assert_eq!(ids.len(), SOURCES, "a source is listed once");
    // The saves other than the stage-one one, sample_base, and every Tier B
    // synthetic from libviprs-dep.
    for s in ["ac1014", "ac1015", "ac1018", "ac1021", "ac1024", "ac1027", "sample-base"] {
        assert!(ids.contains(format!("acadsharp-{s}").as_str()), "{s}");
    }
    let tier_b: BTreeSet<&str> = fixtures::all()
        .iter()
        .filter(|f| f.source.as_deref() == Some("libviprs_dep") && f.tier.as_deref() == Some("B"))
        .map(|f| f.id.as_str())
        .collect();
    assert_eq!(tier_b.len(), 41);
    for id in &tier_b {
        assert!(ids.contains(id), "Tier B {id} is not in stage two");
    }
    for r in REFUSALS {
        assert!(!ids.contains(r), "{r} is a refusal fixture and has no cells");
        assert!(fixtures::refusals().iter().any(|f| f.id == r), "{r}");
    }
    assert!(
        written::outputs("two").len() == SOURCES * 48,
        "48 outputs per source"
    );
    assert_eq!(SOURCES * CELLS_PER_SOURCE, 9216);
}

#[test]
fn the_committed_stage_two_table_is_the_plans_cells_or_empty_and_every_row_has_its_reason() {
    let text = read("parity/VERDICTS_STAGE2.toml");
    let t = Table::parse(&text).unwrap_or_else(|e| panic!("VERDICTS_STAGE2.toml: {e}"));
    let refusals = t.refusals();
    assert!(refusals.is_empty(), "{}", refusals.join("\n"));
    if t.rows.is_empty() {
        return;
    }
    let mut want = BTreeSet::new();
    for o in written::outputs("two") {
        let readers: [&str; 3] = if o.container == "dwg" {
            ["oracle", "acadrust", "acadsharp"]
        } else {
            ["oracle", "acadrust", "ezdxf"]
        };
        for r in readers {
            want.insert((o.id(), format!("w-{}:{r}", o.writer)));
        }
        want.insert((o.id(), format!("w-{}:version", o.writer)));
    }
    let have: BTreeSet<(String, String)> = t.rows.iter().map(Table::key).collect();
    assert_eq!(want.len(), 9216);
    assert_eq!(
        want.difference(&have).count(),
        0,
        "cells with no verdict row"
    );
    assert_eq!(have.difference(&want).count(), 0, "rows for cells nobody computes");
    assert!(!t.runs.is_empty(), "a table that was never confirmed by a run");
}
