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
    self, Disposition, FindingKind, Record, Report, Rules, Side, Value, compare,
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
    /// acadrust's stream (or its refusal) for each of the thirteen inputs.
    rust: BTreeMap<String, Result<Vec<Record>, String>>,
}

/// The run, once per process: the tests share it, and they must not each start
/// `emit-acadrust` thirteen times.
fn the_run() -> &'static Result<Run, String> {
    static RUN: std::sync::OnceLock<Result<Run, String>> = std::sync::OnceLock::new();
    RUN.get_or_init(|| {
        let rules = model1::committed_rules()?;
        compute(&rules)
    })
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
    let rust = rust.into_iter().map(|(k, v)| (k.to_string(), v)).collect();
    Ok(Run {
        reports,
        failures,
        rust,
    })
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
    let Some(run) = gate(the_run().as_ref().map_err(String::clone)) else {
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
    let Some(run) = gate(the_run().as_ref().map_err(String::clone)) else {
        return;
    };
    let mut used: BTreeSet<(String, String, String)> = BTreeSet::new();
    for ((fixture, _), r) in &run.reports {
        for f in r
            .findings
            .iter()
            .filter(|f| f.disposition == Disposition::Excluded)
        {
            let engine = f
                .excluded
                .clone()
                .expect("an excluded finding names its engine");
            used.insert((engine, fixture.clone(), f.key.clone()));
        }
    }
    let mut stale = Vec::new();
    for e in &rules.excluded {
        for n in &e.names {
            let hit = used.iter().any(|(eng, fx, name)| {
                eng == &e.engine
                    && model1::glob(n, name)
                    && (e.fixtures.is_empty() || e.fixtures.contains(fx))
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
    let Some(run) = gate(the_run().as_ref().map_err(String::clone)) else {
        return;
    };
    let mut stale = Vec::new();
    for k in &rules.known {
        for n in &k.names {
            let hit = run.reports.iter().any(|((fixture, pair), r)| {
                (k.pairs.is_empty() || k.pairs.contains(pair))
                    && (k.fixtures.is_empty() || k.fixtures.contains(fixture))
                    && r.findings.iter().any(|f| {
                        model1::glob(n, &f.key)
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
    let Some(run) = gate(the_run().as_ref().map_err(String::clone)) else {
        return;
    };
    let mine = run.rust[id].as_ref().expect("acadrust reads this save");
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
            records: mine,
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
        let Record::Header(h) = r else { continue };
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
            records: mine,
        },
        &rules,
    );
    let red = after.red().len();
    let before_red = untouched.red().len();
    assert_eq!(red, before_red + 1, "{}", after.first());
    assert!(after.red().iter().any(|f| f.key == name));
}

/// The layer control: one layer's colour changed in a scratch copy of the
/// oracle's stream turns that save's row red, and the finding names the layer
/// and the field.
#[test]
fn one_changed_layer_colour_is_red_and_names_the_layer() {
    let rules = model1::committed_rules().unwrap();
    let id = "acadsharp-ac1032";
    let Some(run) = gate(the_run().as_ref().map_err(String::clone)) else {
        return;
    };
    let mine = run.rust[id].as_ref().expect("acadrust reads this save");
    let mut oracle = model1::load_frozen("oracle", id).unwrap();
    let before = compare(
        "oracle/acadrust",
        &Side {
            engine: "oracle",
            fixture: id,
            records: &oracle,
        },
        &Side {
            engine: "acadrust",
            fixture: id,
            records: mine,
        },
        &rules,
    );
    // A layer both sides hold and agree on the colour of, so the control starts clean.
    let name = oracle
        .iter()
        .find_map(|r| match r {
            Record::Entry(l)
                if l.kind == "layer"
                    && matches!(l.get("color"), Some(Value::Int(c)) if *c > 0)
                    && before
                        .findings
                        .iter()
                        .all(|f| f.key != format!("layer/{}.color", l.name)) =>
            {
                Some(l.name.clone())
            }
            _ => None,
        })
        .expect("the save has a layer with an index colour that matches");
    for r in &mut oracle {
        if let Record::Entry(l) = r
            && l.kind == "layer"
            && l.name == name
        {
            let was = l.get("color").cloned();
            l.set(
                "color",
                Value::Int(if was == Some(Value::Int(1)) { 2 } else { 1 }),
            );
        }
    }
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
            records: mine,
        },
        &rules,
    );
    let key = format!("layer/{name}.color");
    assert_eq!(
        after.red().len(),
        before.red().len() + 1,
        "{}",
        after.first()
    );
    assert!(after.red().iter().any(|f| f.key == key), "{key}");
}

/// The linetype control: one segment's length changed in a scratch copy of the
/// oracle's stream turns that save's row red, and the finding names the entry
/// and the segment index.
#[test]
fn one_changed_segment_length_is_red_and_names_the_entry_and_the_index() {
    let rules = model1::committed_rules().unwrap();
    let id = "acadsharp-ac1032";
    let Some(run) = gate(the_run().as_ref().map_err(String::clone)) else {
        return;
    };
    let mine = run.rust[id].as_ref().expect("acadrust reads this save");
    let mut oracle = model1::load_frozen("oracle", id).unwrap();
    fn side<'a>(id: &'a str, records: &'a [Record]) -> Side<'a> {
        Side {
            engine: "oracle",
            fixture: id,
            records,
        }
    }
    let theirs = Side {
        engine: "acadrust",
        fixture: id,
        records: mine,
    };
    let before_red = compare("oracle/acadrust", &side(id, &oracle), &theirs, &rules)
        .red()
        .len();
    let mut changed = None;
    for r in &mut oracle {
        let Record::Entry(e) = r else { continue };
        if e.kind != "linetype" || changed.is_some() {
            continue;
        }
        let Some(Value::List(segments)) = e.get("segments").cloned() else {
            continue;
        };
        if segments.len() < 2 {
            continue;
        }
        let mut segments = segments;
        let Value::Double(len) = segments[1][0] else {
            continue;
        };
        segments[1][0] = Value::Double(len * 1.5 + 1.0);
        e.set("segments", Value::List(segments));
        changed = Some(e.name.clone());
    }
    let name = changed.expect("a linetype with at least two segments");
    let after = compare("oracle/acadrust", &side(id, &oracle), &theirs, &rules);
    let key = format!("linetype/{name}.segments[1].length");
    assert_eq!(after.red().len(), before_red + 1, "{}", after.first());
    assert!(after.red().iter().any(|f| f.key == key), "{key}");
}

/// The textstyle control, on a DXF twin: one `width_factor` changed in a scratch
/// copy of the oracle's reading of the twin turns that twin's ezdxf row red, and
/// the finding names the entry.
#[test]
fn one_changed_width_factor_is_red_and_names_the_style() {
    let rules = model1::committed_rules().unwrap();
    let twin = "dxf-sample-ac1032-ascii";
    let ez = model1::load_frozen("ezdxf", twin).unwrap();
    let mut oracle = model1::load_frozen("oracle", twin).unwrap();
    fn side<'a>(engine: &'a str, id: &'a str, records: &'a [Record]) -> Side<'a> {
        Side {
            engine,
            fixture: id,
            records,
        }
    }
    let before = compare(
        "ezdxf/oracle",
        &side("ezdxf", twin, &ez),
        &side("oracle", twin, &oracle),
        &rules,
    )
    .red()
    .len();
    let mut touched = 0;
    for r in &mut oracle {
        if let Record::Entry(e) = r
            && e.kind == "textstyle"
            && e.name == "Standard"
            && let Some(Value::Double(w)) = e.get("width_factor").cloned()
        {
            e.set("width_factor", Value::Double(w * 1.5));
            touched += 1;
        }
    }
    assert_eq!(touched, 1, "the twin has a style Standard");
    let after = compare(
        "ezdxf/oracle",
        &side("ezdxf", twin, &ez),
        &side("oracle", twin, &oracle),
        &rules,
    );
    assert_eq!(after.red().len(), before + 1, "{}", after.first());
    assert!(
        after
            .red()
            .iter()
            .any(|f| f.key == "textstyle/Standard.width_factor")
    );
}

#[test]
fn every_file_has_a_style_called_standard() {
    use acadsharp_rs_tests::model1;
    for (engine, id) in [
        ("oracle", "acadsharp-ac1032"),
        ("oracle", "acadsharp-ac1014"),
        ("ezdxf", "dxf-sample-ac1032-ascii"),
    ] {
        let records = model1::load_frozen(engine, id).unwrap();
        assert!(
            records
                .iter()
                .any(|r| r.kind() == "textstyle" && r.key().eq_ignore_ascii_case("Standard")),
            "{engine} {id}: no style Standard"
        );
    }
}

// ---- the dictionary tree (R5.5.6, docs/MODEL_1.md) -------------------------------------

/// The `dictionary` record at `path` (`""` is the root), with its entries as
/// (key, class) pairs.
fn dictionary_at(records: &[Record], path: &str) -> Option<(Vec<(String, String)>, i64)> {
    records.iter().find_map(|r| match r {
        Record::Entry(e) if e.kind == "dictionary" && e.name == path => {
            let Some(Value::List(items)) = e.get("entries") else {
                return None;
            };
            let Some(Value::Int(count)) = e.get("entry_count") else {
                return None;
            };
            let pairs = items
                .iter()
                .map(|item| match item.as_slice() {
                    [Value::Str(k), Value::Str(c)] => (k.clone(), c.clone()),
                    other => panic!("an entry that is not (key, class): {other:?}"),
                })
                .collect();
            Some((pairs, *count))
        }
        _ => None,
    })
}

/// Every frozen reading of every input: the oracle over the thirteen files and
/// ezdxf over the six twins.
fn every_frozen_reading() -> Vec<(&'static str, &'static str)> {
    let mut all: Vec<(&str, &str)> = SAVES.iter().map(|s| ("oracle", *s)).collect();
    for (twin, _) in TWINS {
        all.push(("oracle", twin));
        all.push(("ezdxf", twin));
    }
    all
}

#[test]
fn every_reading_has_a_root_dictionary_with_the_standard_entries() {
    for (engine, id) in every_frozen_reading() {
        let records = model1::load_frozen(engine, id).unwrap();
        let (entries, count) = dictionary_at(&records, "")
            .unwrap_or_else(|| panic!("{engine} {id}: no dictionary record for the root"));
        assert_eq!(count as usize, entries.len(), "{engine} {id}: entry_count");
        for key in ["ACAD_GROUP", "ACAD_MLINESTYLE"] {
            assert!(
                entries.iter().any(|(k, c)| k == key && c == "DICTIONARY"),
                "{engine} {id}: the root has no dictionary entry {key}: {entries:?}"
            );
        }
        let keys: Vec<&String> = entries.iter().map(|e| &e.0).collect();
        let mut sorted = keys.clone();
        sorted.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
        assert_eq!(keys, sorted, "{engine} {id}: entries are sorted by key");
    }
}

/// A nested dictionary is a `DICTIONARY` entry of its parent and has its own
/// record under the longer path, so no dictionary is only a name in a list.
#[test]
fn every_nested_dictionary_has_its_own_record_under_the_longer_path() {
    for (engine, id) in every_frozen_reading() {
        let records = model1::load_frozen(engine, id).unwrap();
        let mut paths: Vec<String> = vec![String::new()];
        let mut visited = 0;
        while let Some(path) = paths.pop() {
            let (entries, _) = dictionary_at(&records, &path)
                .unwrap_or_else(|| panic!("{engine} {id}: no record for the dictionary {path:?}"));
            visited += 1;
            for (key, class) in entries {
                if class == "DICTIONARY" || class == "ACDBDICTIONARYWDFLT" {
                    paths.push(if path.is_empty() {
                        key
                    } else {
                        format!("{path}/{key}")
                    });
                }
            }
        }
        let recorded = records
            .iter()
            .filter(|r| r.kind() == "dictionary")
            .filter(|r| !r.key().starts_with("table/") && !r.key().starts_with("layout/"))
            .count();
        assert_eq!(
            visited, recorded,
            "{engine} {id}: a dictionary record that no entry leads to"
        );
    }
}

/// ACadSharp drops an object whose class it does not know unless the reader is
/// told to keep it, so without the switch the entry would be missing from the
/// oracle's list. ACAD_ASSOCPERSSUBENTMANAGER is such an object (ACadSharp has no
/// class for ACDBASSOCPERSSUBENTMANAGER), and the oracle names it in the DWG and
/// in the DXF twin alike. The DXF reader keeps it only when a second switch is on
/// as well (docs/MODEL_1.md), which is what this holds the oracle to.
#[test]
fn the_oracle_keeps_the_entries_acadsharp_has_no_class_for() {
    for id in ["acadsharp-ac1032", "dxf-sample-ac1032-ascii"] {
        let records = model1::load_frozen("oracle", id).unwrap();
        let (entries, _) = dictionary_at(&records, "").unwrap();
        assert!(
            entries
                .iter()
                .any(|(k, c)| k == "ACAD_ASSOCPERSSUBENTMANAGER"
                    && c == "ACDBASSOCPERSSUBENTMANAGER"),
            "{id}: the oracle lost the root entry ACAD_ASSOCPERSSUBENTMANAGER: {entries:?}"
        );
    }
    // The twin and the DWG are one drawing: every key ezdxf lists under the root
    // of the twin is a key the oracle lists under the root of its DXF.
    let ez = model1::load_frozen("ezdxf", "dxf-sample-ac1032-ascii").unwrap();
    let dxf = model1::load_frozen("oracle", "dxf-sample-ac1032-ascii").unwrap();
    let (theirs, _) = dictionary_at(&ez, "").unwrap();
    let (ours, _) = dictionary_at(&dxf, "").unwrap();
    for (key, _) in &theirs {
        assert!(
            ours.iter().any(|(k, _)| k == key),
            "the oracle lost the root entry {key}"
        );
    }
}

/// The dictionary control, on a DXF twin: one entry renamed in a scratch copy of
/// the oracle's reading turns that twin's row red, and the finding names the
/// path of the dictionary.
#[test]
fn one_renamed_dictionary_entry_is_red_and_names_the_path() {
    let rules = model1::committed_rules().unwrap();
    let twin = "dxf-sample-ac1032-ascii";
    let ez = model1::load_frozen("ezdxf", twin).unwrap();
    let mut oracle = model1::load_frozen("oracle", twin).unwrap();
    fn side<'a>(engine: &'a str, id: &'a str, records: &'a [Record]) -> Side<'a> {
        Side {
            engine,
            fixture: id,
            records,
        }
    }
    let before = compare(
        "ezdxf/oracle",
        &side("ezdxf", twin, &ez),
        &side("oracle", twin, &oracle),
        &rules,
    )
    .red()
    .len();
    let mut touched = 0;
    for r in &mut oracle {
        if let Record::Entry(e) = r
            && e.kind == "dictionary"
            && e.name == "ACAD_TABLESTYLE"
            && let Some(Value::List(items)) = e.get("entries").cloned()
        {
            let mut items = items;
            if let Some(Value::Str(k)) = items.get_mut(0).and_then(|i| i.get_mut(0)) {
                k.push_str("_RENAMED");
                touched += 1;
            }
            e.set("entries", Value::List(items));
        }
    }
    assert_eq!(touched, 1, "the twin has an ACAD_TABLESTYLE with an entry");
    let after = compare(
        "ezdxf/oracle",
        &side("ezdxf", twin, &ez),
        &side("oracle", twin, &oracle),
        &rules,
    );
    assert_eq!(after.red().len(), before + 1, "{}", after.first());
    assert!(
        after
            .red()
            .iter()
            .any(|f| f.key == "dictionary/ACAD_TABLESTYLE.entries[0].key"),
        "{:?}",
        after.red().iter().map(|f| &f.key).collect::<Vec<_>>()
    );
}

/// The dimstyle control, on a DXF twin: `$DIMASZ` of `ISO-25` changed in a
/// scratch copy of the oracle's reading of the AC1032 twin turns that twin's
/// ezdxf row one finding worse, naming the style and the field.
#[test]
fn one_changed_dimasz_is_red_and_names_the_style_and_the_field() {
    let rules = model1::committed_rules().unwrap();
    let twin = "dxf-sample-ac1032-ascii";
    let ez = model1::load_frozen("ezdxf", twin).unwrap();
    let mut oracle = model1::load_frozen("oracle", twin).unwrap();
    fn side<'a>(engine: &'a str, id: &'a str, records: &'a [Record]) -> Side<'a> {
        Side {
            engine,
            fixture: id,
            records,
        }
    }
    let before = compare(
        "ezdxf/oracle",
        &side("ezdxf", twin, &ez),
        &side("oracle", twin, &oracle),
        &rules,
    )
    .red()
    .len();
    let mut touched = 0;
    for r in &mut oracle {
        if let Record::Entry(e) = r
            && e.kind == "dimstyle"
            && e.name == "ISO-25"
            && let Some(Value::Double(v)) = e.get("$DIMASZ").cloned()
        {
            e.set("$DIMASZ", Value::Double(v * 1.5));
            touched += 1;
        }
    }
    assert_eq!(touched, 1, "the twin has a style ISO-25");
    let after = compare(
        "ezdxf/oracle",
        &side("ezdxf", twin, &ez),
        &side("oracle", twin, &oracle),
        &rules,
    );
    assert_eq!(after.red().len(), before + 1, "{}", after.first());
    assert!(
        after
            .red()
            .iter()
            .any(|f| f.key == "dimstyle/ISO-25.$DIMASZ")
    );
}

/// Every handle-valued field of the `Standard` style of the AC1032 save comes out
/// as a name on both DWG readers, and the names are ones the drawing's own tables
/// carry (the empty string is "no reference").
#[test]
fn the_handle_valued_dimstyle_fields_of_standard_are_names_the_tables_carry() {
    let id = "acadsharp-ac1032";
    let Some(run) = gate(the_run().as_ref().map_err(String::clone)) else {
        return;
    };
    let oracle = model1::load_frozen("oracle", id).unwrap();
    let rust = run.rust[id].as_ref().expect("acadrust reads this save");
    for (who, records) in [("oracle", &oracle), ("acadrust", rust)] {
        let names = |kind: &str| -> Vec<String> {
            records
                .iter()
                .filter(|r| r.kind() == kind)
                .map(|r| r.key().to_string())
                .collect()
        };
        let (styles, linetypes) = (names("textstyle"), names("linetype"));
        let Some(Record::Entry(std)) = records
            .iter()
            .find(|r| r.kind() == "dimstyle" && r.key() == "Standard")
        else {
            panic!("{who}: no dimstyle Standard");
        };
        let text = |f: &str| match std.get(f) {
            Some(Value::Str(s)) => s.clone(),
            other => panic!("{who}: {f} is {other:?}"),
        };
        assert!(styles.contains(&text("$DIMTXSTY")), "{who} $DIMTXSTY");
        for f in ["$DIMLTYPE", "$DIMLTEX1", "$DIMLTEX2"] {
            let v = text(f);
            assert!(v.is_empty() || linetypes.contains(&v), "{who} {f} = {v:?}");
        }
        for f in ["$DIMBLK", "$DIMBLK1", "$DIMBLK2", "$DIMLDRBLK"] {
            let v = text(f);
            // A handle is hexadecimal digits and nothing else.
            assert!(
                v.is_empty() || !v.chars().all(|c| c.is_ascii_hexdigit()) || v.len() < 2,
                "{who} {f} looks like a handle: {v:?}"
            );
        }
    }
}
