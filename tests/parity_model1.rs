//! The parity run for the model-1 stream (R5.5, docs/MODEL_1.md): the oracle and
//! acadrust over the seven AutoCAD saves, and all three over the six ASCII DXF
//! twins, each DWG reader's DXF also held against its own DWG.
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
        // One directory per process: a second `cargo test` on the same target
        // directory (another job, another checkout) must not write into the
        // files this one is about to read.
        _ => Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("model1-{}", std::process::id())),
    }
}

const VERDICTS_HEADER: &str = "# parity/MODEL1_VERDICTS.toml: one row per (fixture, pair) of the model-1 stream.\n\
#\n\
# The words are the ones src/verdict.rs defines and parity/VERDICTS.toml uses.\n\
# `tests/parity_model1.rs` recomputes every verdict from a real run and is red on\n\
# any row that moved, better or worse, until this file changes. A row that is not\n\
# MATCH has a reason and an issue; for KNOWN the reason is that every difference\n\
# is a row of parity/MODEL1.toml, and the issues are the ones those rows name.\n\
#\n\
# `MODEL1_REGEN=1 cargo test --features acadrust --test parity_model1` rewrites it.\n\n";

fn verdicts_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("parity")
        .join("MODEL1_VERDICTS.toml")
}

/// What acadrust says about a save, by running the emitter as a process.
///
/// `Ok(Err(why))` is acadrust refusing the file, which is a row (`OPEN_FAILED`);
/// `Err` is the emitter not running at all, which is the run's failure.
fn acadrust(id: &str, out: &Path) -> Result<Result<Vec<Record>, String>, String> {
    let run = Command::new(env!("CARGO_BIN_EXE_emit-acadrust"))
        .args(["--fixture", id, "--out"])
        .arg(out)
        .output()
        .map_err(|e| format!("emit-acadrust could not be started: {e}"))?;
    if !run.status.success() {
        let stderr = String::from_utf8_lossy(&run.stderr);
        if stderr.contains("could not read") {
            let line = stderr.lines().find(|l| !l.trim().is_empty()).unwrap_or("");
            return Ok(Err(format!(
                "exit {}: {line}",
                run.status.code().unwrap_or(-1)
            )));
        }
        return Err(format!("emit-acadrust failed on {id}: {stderr}"));
    }
    let path = model1::output_path(out, id, "acadrust");
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    model1::parse_stream(&text)
        .map(Ok)
        .map_err(|e| format!("{}: {e}", path.display()))
}

struct Run {
    /// (fixture, pair) -> report.
    reports: BTreeMap<(String, String), Report>,
    /// (fixture, pair) -> why a side could not be read: `OPEN_FAILED` rows.
    failures: BTreeMap<(String, String), String>,
}

fn compute(rules: &Rules) -> Result<Run, String> {
    let out = out_dir();
    std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
    // acadrust's stream for each of the thirteen inputs, once.
    let mut rust: BTreeMap<&str, Result<Vec<Record>, String>> = BTreeMap::new();
    for id in SAVES.iter().copied().chain(TWINS.iter().map(|t| t.0)) {
        rust.insert(id, acadrust(id, &out)?);
    }
    let mut reports = BTreeMap::new();
    let mut failures = BTreeMap::new();
    let mut put = |fixture: &str, pair: &str, a: Side, b: Side| {
        reports.insert(
            (fixture.to_string(), pair.to_string()),
            compare(pair, &a, &b, rules),
        );
    };
    let mut fail = |fixture: &str, pair: &str, why: &str| {
        failures.insert((fixture.to_string(), pair.to_string()), why.to_string());
    };

    // Both DWG readers over each save.
    for s in SAVES {
        let o = model1::load_frozen("oracle", s)?;
        match &rust[s] {
            Ok(r) => put(
                s,
                "oracle/acadrust",
                Side {
                    engine: "oracle",
                    fixture: s,
                    records: &o,
                },
                Side {
                    engine: "acadrust",
                    fixture: s,
                    records: r,
                },
            ),
            Err(why) => fail(s, "oracle/acadrust", why),
        }
    }

    // All three over each DXF twin, and each DWG reader's DXF against its own
    // DWG of the same drawing (where the two files were written at different
    // moments, so the known list applies).
    for (twin, save) in TWINS {
        let ez = model1::load_frozen("ezdxf", twin)?;
        let o = model1::load_frozen("oracle", twin)?;
        let o_dwg = model1::load_frozen("oracle", save)?;
        let side = |engine: &'static str, records| Side {
            engine,
            fixture: twin,
            records,
        };
        put(twin, "ezdxf/oracle", side("ezdxf", &ez), side("oracle", &o));
        put(
            twin,
            "dxf-dwg:oracle",
            side("oracle", &o),
            side("oracle", &o_dwg),
        );
        match (&rust[twin], &rust[save]) {
            (Ok(r), Ok(r_dwg)) => {
                put(
                    twin,
                    "ezdxf/acadrust",
                    side("ezdxf", &ez),
                    side("acadrust", r),
                );
                put(
                    twin,
                    "oracle/acadrust",
                    side("oracle", &o),
                    side("acadrust", r),
                );
                put(
                    twin,
                    "dxf-dwg:acadrust",
                    side("acadrust", r),
                    side("acadrust", r_dwg),
                );
            }
            (a, b) => {
                let why = a.as_ref().err().or(b.as_ref().err()).unwrap();
                for pair in ["ezdxf/acadrust", "oracle/acadrust", "dxf-dwg:acadrust"] {
                    fail(twin, pair, why);
                }
            }
        }
    }
    Ok(Run { reports, failures })
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
    let Some(run) = gate(compute(&rules)) else {
        return;
    };

    if std::env::var("MODEL1_EXPLAIN").as_deref() == Ok("1") {
        for ((fixture, pair), r) in &run.reports {
            for f in r
                .findings
                .iter()
                .filter(|f| f.disposition != Disposition::Excluded)
            {
                println!("EXPLAIN {fixture} {pair} {f}");
            }
            let ex: Vec<&str> = r
                .findings
                .iter()
                .filter(|f| f.disposition == Disposition::Excluded)
                .map(|f| f.key.as_str())
                .collect();
            println!(
                "EXCLUDED {fixture} {pair} {} names: {}",
                ex.len(),
                ex.join(" ")
            );
        }
    }

    let committed_text = std::fs::read_to_string(verdicts_path());
    let regen = std::env::var("MODEL1_REGEN").as_deref() == Ok("1");
    let committed = match committed_text {
        Ok(t) => Table::parse(&t).unwrap(),
        Err(_) if regen => Table::default(),
        Err(e) => panic!("{}: {e}", verdicts_path().display()),
    };
    let mut actual: BTreeMap<(String, String), (String, String)> = run
        .reports
        .iter()
        .map(|(k, r)| (k.clone(), (r.verdict().to_string(), r.first())))
        .collect();
    for k in run.failures.keys() {
        actual.insert(k.clone(), ("OPEN_FAILED".to_string(), String::new()));
    }
    let mut issues: BTreeMap<(String, String), String> = run
        .reports
        .iter()
        .map(|(k, r)| (k.clone(), r.issues()))
        .collect();
    for k in run.failures.keys() {
        // The refusal is R5.1's finding (acadrust cannot read these DXFs); the
        // schema 1 table already carries it under this issue.
        issues.insert(k.clone(), "libviprs/acadsharp-rs-tests#95".to_string());
    }

    if regen {
        let mut table = Table {
            runs: committed.runs.clone(),
            rows: Vec::new(),
        };
        for ((fixture, pair), (verdict, first)) in &actual {
            let needs = verdict != "MATCH";
            table.rows.push(Row {
                fixture: fixture.clone(),
                pair: pair.clone(),
                verdict: verdict.clone(),
                reason: match run.failures.get(&(fixture.clone(), pair.clone())) {
                    Some(why) => why.clone(),
                    None if needs => "listed in parity/MODEL1.toml".to_string(),
                    None => String::new(),
                },
                issue: if needs {
                    issues[&(fixture.clone(), pair.clone())].clone()
                } else {
                    String::new()
                },
                first: first.clone(),
            });
        }
        let rendered = table.render();
        let body = rendered
            .split_once("\n\n")
            .map_or(rendered.as_str(), |(_, b)| b);
        let rendered = format!("{VERDICTS_HEADER}{body}");
        std::fs::write(verdicts_path(), &rendered).unwrap();
        // The run may be in a container whose tree is thrown away, so the table
        // is printed too, between two markers.
        println!("\n==MODEL1_VERDICTS==\n{rendered}==END==");
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
    assert!(
        refusals.is_empty(),
        "parity/MODEL1_VERDICTS.toml:\n  {}",
        refusals.join("\n  ")
    );
}

/// An exclusion row is a claim that an engine cannot produce a name for a file.
/// One the run never needed is stale, and a name that the engine *did* produce
/// is a wrong claim; both are red. Together with the comparator's rule that an
/// unlisted one-sided name is a finding, the lists are exactly the data.
#[test]
fn every_exclusion_is_used_and_none_excuses_a_name_the_engine_wrote() {
    let rules = model1::committed_rules().unwrap();
    let Some(run) = gate(compute(&rules)) else {
        return;
    };
    let mut used: BTreeSet<(String, String, String)> = BTreeSet::new();
    for ((fixture, pair), r) in &run.reports {
        let (a, b) = match pair.split_once('/') {
            Some(ab) => ab,
            // `dxf-dwg:oracle`: the same engine on both sides.
            None => {
                let e = pair.split_once(':').unwrap().1;
                (e, e)
            }
        };
        for f in r
            .findings
            .iter()
            .filter(|f| f.disposition == Disposition::Excluded)
        {
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
    assert!(
        stale.is_empty(),
        "exclusions the run never needed:\n  {}",
        stale.join("\n  ")
    );
}

/// The same for `known`: a listed difference that no longer happens is red and
/// asks for its row to go. That is what makes a fix remove its own rows.
#[test]
fn every_known_row_is_still_a_difference_the_run_finds() {
    let rules = model1::committed_rules().unwrap();
    let Some(run) = gate(compute(&rules)) else {
        return;
    };
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
    assert!(
        stale.is_empty(),
        "known rows the run never needed:\n  {}",
        stale.join("\n  ")
    );
}

/// The control: the run's own wiring, with one value changed in a scratch copy
/// of the oracle's stream, is `DIFF:1` and names the variable.
#[test]
fn one_changed_header_value_is_one_diff_and_names_the_variable() {
    let rules = model1::committed_rules().unwrap();
    let id = "acadsharp-ac1032";
    let Some(mine) = gate(acadrust(id, &out_dir())) else {
        return;
    };
    let mine = mine.expect("acadrust reads this save");
    let mut oracle = model1::load_frozen("oracle", id).unwrap();
    let untouched = compare(
        "oracle/acadrust",
        &Side {
            engine: "oracle",
            fixture: id,
            records: &oracle,
        },
        &Side {
            engine: "acadrust",
            fixture: id,
            records: &mine,
        },
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
        if h.name == name
            && let model1::Value::Double(d) = &mut h.value
        {
            *d *= 1.001;
            touched += 1;
        }
    }
    assert_eq!(touched, 1);
    let after = compare(
        "oracle/acadrust",
        &Side {
            engine: "oracle",
            fixture: id,
            records: &oracle,
        },
        &Side {
            engine: "acadrust",
            fixture: id,
            records: &mine,
        },
        &rules,
    );
    let red = after.red().len();
    let before_red = untouched.red().len();
    assert_eq!(red, before_red + 1, "{}", after.first());
    assert!(after.red().iter().any(|f| f.key == name));
}
