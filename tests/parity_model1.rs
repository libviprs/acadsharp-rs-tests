//! The parity run for the model-1 stream (R5.5, docs/MODEL_1.md): the oracle and
//! acadrust over the seven AutoCAD saves, and ezdxf over the six DXF twins
//! against both.
//!
//! The same words and the same table type as `tests/parity.rs` and
//! `parity/VERDICTS.toml` (src/verdict.rs), in a file of their own,
//! `parity/MODEL1_VERDICTS.toml`, because this stream has no native lane: it
//! needs no archive, and it should not wait for one or conflict with the rows
//! of the schema 1 run. acadsharp-rs is `EXCLUDED` from it (the C ABI exposes
//! flattened primitives only, and growing it is libviprs-dep's call).
//!
//! - `PARITY_REQUIRE_CORPUS=1`: a missing emitter or frozen file is a failure.
//! - `MODEL1_REGEN=1`: rewrite `parity/MODEL1_VERDICTS.toml` from the run,
//!   keeping the reason and issue of every row it finds.
//! - `MODEL1_EXPLAIN=1`: print every finding that is not a match.
#![cfg(feature = "acadrust")]

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;

use acadsharp_rs_tests::model1::{
    self, Disposition, FindingKind, Record, Report, Rules, Side, compare,
};
use acadsharp_rs_tests::verdict::{Row, Table};

const SAVES: [&str; 7] = [
    "acadsharp-ac1014",
    "acadsharp-ac1015",
    "acadsharp-ac1018",
    "acadsharp-ac1021",
    "acadsharp-ac1024",
    "acadsharp-ac1027",
    "acadsharp-ac1032",
];
/// The ASCII twin of each save that has one.
const TWINS: [(&str, &str); 6] = [
    ("dxf-sample-ac1015-ascii", "acadsharp-ac1015"),
    ("dxf-sample-ac1018-ascii", "acadsharp-ac1018"),
    ("dxf-sample-ac1021-ascii", "acadsharp-ac1021"),
    ("dxf-sample-ac1024-ascii", "acadsharp-ac1024"),
    ("dxf-sample-ac1027-ascii", "acadsharp-ac1027"),
    ("dxf-sample-ac1032-ascii", "acadsharp-ac1032"),
];

fn require() -> bool {
    std::env::var("PARITY_REQUIRE_CORPUS").as_deref() == Ok("1")
}

fn out_dir() -> PathBuf {
    match std::env::var("MODEL1_OUT") {
        Ok(p) if !p.is_empty() => PathBuf::from(p),
        _ => Path::new(env!("CARGO_TARGET_TMPDIR")).join("..").join("model1"),
    }
}

fn verdicts_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("parity")
        .join("MODEL1_VERDICTS.toml")
}

/// What acadrust says about a save, by running the emitter as a process.
fn acadrust(id: &str, out: &Path) -> Result<Vec<Record>, String> {
    let run = Command::new(env!("CARGO_BIN_EXE_emit-acadrust"))
        .args(["--fixture", id, "--out"])
        .arg(out)
        .output()
        .map_err(|e| format!("emit-acadrust could not be started: {e}"))?;
    if !run.status.success() {
        return Err(format!(
            "emit-acadrust failed on {id}: {}",
            String::from_utf8_lossy(&run.stderr)
        ));
    }
    let path = model1::output_path(out, id, "acadrust");
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    model1::parse_stream(&text).map_err(|e| format!("{}: {e}", path.display()))
}

struct Run {
    /// (fixture, pair) -> report.
    reports: BTreeMap<(String, String), Report>,
}

fn compute(rules: &Rules) -> Result<Run, String> {
    let out = out_dir();
    std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
    let mut oracle = BTreeMap::new();
    let mut rust = BTreeMap::new();
    for s in SAVES {
        oracle.insert(s, model1::load_frozen("oracle", s)?);
        rust.insert(s, acadrust(s, &out)?);
    }
    let mut reports = BTreeMap::new();
    for s in SAVES {
        let r = compare(
            "oracle/acadrust",
            &Side { engine: "oracle", fixture: s, records: &oracle[s] },
            &Side { engine: "acadrust", fixture: s, records: &rust[s] },
            rules,
        );
        reports.insert((s.to_string(), "oracle/acadrust".to_string()), r);
    }
    for (twin, save) in TWINS {
        let ez = model1::load_frozen("ezdxf", twin)?;
        let r = compare(
            "ezdxf/oracle",
            &Side { engine: "ezdxf", fixture: twin, records: &ez },
            &Side { engine: "oracle", fixture: save, records: &oracle[save] },
            rules,
        );
        reports.insert((twin.to_string(), "ezdxf/oracle".to_string()), r);
        let r = compare(
            "ezdxf/acadrust",
            &Side { engine: "ezdxf", fixture: twin, records: &ez },
            &Side { engine: "acadrust", fixture: save, records: &rust[save] },
            rules,
        );
        reports.insert((twin.to_string(), "ezdxf/acadrust".to_string()), r);
    }
    Ok(Run { reports })
}

fn gate<T>(r: Result<T, String>) -> Option<T> {
    match r {
        Ok(v) => Some(v),
        Err(e) if require() => panic!("PARITY_REQUIRE_CORPUS=1 and {e}"),
        Err(e) => {
            eprintln!("skipped: {e}");
            None
        }
    }
}

#[test]
fn the_run_agrees_with_the_committed_verdicts() {
    let rules = model1::committed_rules().unwrap();
    let Some(run) = gate(compute(&rules)) else { return };

    if std::env::var("MODEL1_EXPLAIN").as_deref() == Ok("1") {
        for ((fixture, pair), r) in &run.reports {
            for f in r.findings.iter().filter(|f| f.disposition != Disposition::Excluded) {
                println!("EXPLAIN {fixture} {pair} {f}");
            }
            let ex: Vec<&str> = r
                .findings
                .iter()
                .filter(|f| f.disposition == Disposition::Excluded)
                .map(|f| f.key.as_str())
                .collect();
            println!("EXCLUDED {fixture} {pair} {} names: {}", ex.len(), ex.join(" "));
        }
    }

    let committed_text = std::fs::read_to_string(verdicts_path());
    let regen = std::env::var("MODEL1_REGEN").as_deref() == Ok("1");
    let committed = match committed_text {
        Ok(t) => Table::parse(&t).unwrap(),
        Err(_) if regen => Table::default(),
        Err(e) => panic!("{}: {e}", verdicts_path().display()),
    };
    let actual: BTreeMap<(String, String), (String, String)> = run
        .reports
        .iter()
        .map(|(k, r)| (k.clone(), (r.verdict().to_string(), r.first())))
        .collect();

    if regen {
        let old = committed.by_key();
        let mut table = Table { runs: committed.runs.clone(), rows: Vec::new() };
        for ((fixture, pair), (verdict, first)) in &actual {
            let kept = old.get(&(fixture.clone(), pair.clone()));
            let moved = kept.is_some_and(|r| &r.verdict != verdict);
            let needs = verdict != "MATCH";
            table.rows.push(Row {
                fixture: fixture.clone(),
                pair: pair.clone(),
                verdict: verdict.clone(),
                reason: if needs && !moved { kept.map(|r| r.reason.clone()).unwrap_or_default() } else { String::new() },
                issue: if needs && !moved { kept.map(|r| r.issue.clone()).unwrap_or_default() } else { String::new() },
                first: first.clone(),
            });
        }
        std::fs::write(verdicts_path(), table.render()).unwrap();
        // The run may be in a container whose tree is thrown away, so the table
        // is printed too, between two markers.
        println!("==MODEL1_VERDICTS==\n{}==END==", table.render());
        eprintln!("rewrote {}", verdicts_path().display());
        return;
    }

    let moved = committed.moved(&actual);
    assert!(
        moved.is_empty(),
        "{} verdict(s) moved:\n  {}\n\nA verdict that moves, better or worse, is red until parity/MODEL1_VERDICTS.toml changes.",
        moved.len(),
        moved.join("\n  ")
    );
    let refusals = committed.refusals();
    assert!(refusals.is_empty(), "parity/MODEL1_VERDICTS.toml:\n  {}", refusals.join("\n  "));
}

/// An exclusion row is a claim that an engine cannot produce a name for a file.
/// One the run never needed is stale, and a name that the engine *did* produce
/// is a wrong claim; both are red. Together with the comparator's rule that an
/// unlisted one-sided name is a finding, the lists are exactly the data.
#[test]
fn every_exclusion_is_used_and_none_excuses_a_name_the_engine_wrote() {
    let rules = model1::committed_rules().unwrap();
    let Some(run) = gate(compute(&rules)) else { return };
    let mut used: BTreeSet<(String, String, String)> = BTreeSet::new();
    for ((fixture, pair), r) in &run.reports {
        let (a, b) = pair.split_once('/').unwrap();
        for f in r.findings.iter().filter(|f| f.disposition == Disposition::Excluded) {
            // OnlyInA: the name is missing from b; OnlyInB: missing from a.
            let engine = if f.kind == FindingKind::OnlyInA { b } else { a };
            used.insert((engine.to_string(), fixture.clone(), f.key.clone()));
        }
    }
    let mut stale = Vec::new();
    for e in &rules.excluded {
        for n in &e.names {
            let hit = used.iter().any(|(eng, fx, name)| {
                eng == &e.engine && name == n && (e.fixtures.is_empty() || e.fixtures.contains(fx))
            });
            if !hit {
                stale.push(format!("{} {n}", e.engine));
            }
        }
    }
    assert!(stale.is_empty(), "exclusions the run never needed:\n  {}", stale.join("\n  "));
}

/// The same for `known`: a listed difference that no longer happens is red and
/// asks for its row to go. That is what makes a fix remove its own rows.
#[test]
fn every_known_row_is_still_a_difference_the_run_finds() {
    let rules = model1::committed_rules().unwrap();
    let Some(run) = gate(compute(&rules)) else { return };
    let mut stale = Vec::new();
    for k in &rules.known {
        for n in &k.names {
            let hit = run.reports.iter().any(|((fixture, pair), r)| {
                (k.pairs.is_empty() || k.pairs.contains(pair))
                    && (k.fixtures.is_empty() || k.fixtures.contains(fixture))
                    && r.findings.iter().any(|f| {
                        &f.key == n
                            && f.disposition == Disposition::Known
                            && matches!(f.kind, FindingKind::Value | FindingKind::Type)
                    })
            });
            if !hit {
                stale.push(format!("{n} ({})", k.issue));
            }
        }
    }
    assert!(stale.is_empty(), "known rows the run never needed:\n  {}", stale.join("\n  "));
}

/// The control: the run's own wiring, with one value changed in a scratch copy
/// of the oracle's stream, is `DIFF:1` and names the variable.
#[test]
fn one_changed_header_value_is_one_diff_and_names_the_variable() {
    let rules = model1::committed_rules().unwrap();
    let id = "acadsharp-ac1032";
    let Some(mine) = gate(acadrust(id, &out_dir())) else { return };
    let mut oracle = model1::load_frozen("oracle", id).unwrap();
    let untouched = compare(
        "oracle/acadrust",
        &Side { engine: "oracle", fixture: id, records: &oracle },
        &Side { engine: "acadrust", fixture: id, records: &mine },
        &rules,
    );
    // Pick a variable both sides hold and agree on, so the control starts clean.
    let name = "$LTSCALE";
    assert!(
        untouched.findings.iter().all(|f| f.key != name),
        "the control needs {name} to match to begin with"
    );
    let mut touched = 0;
    for r in &mut oracle {
        let Record::Header(h) = r;
        if h.name == name {
            if let model1::Value::Double(d) = &mut h.value {
                *d *= 1.001;
                touched += 1;
            }
        }
    }
    assert_eq!(touched, 1);
    let after = compare(
        "oracle/acadrust",
        &Side { engine: "oracle", fixture: id, records: &oracle },
        &Side { engine: "acadrust", fixture: id, records: &mine },
        &rules,
    );
    let red = after.red().len();
    let before_red = untouched.red().len();
    assert_eq!(red, before_red + 1, "{}", after.first());
    assert!(after.red().iter().any(|f| f.key == name));
}
