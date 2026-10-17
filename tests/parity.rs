//! The parity run (R4.1, #25): both engines and the oracle over one corpus, one
//! comparator, a verdict with a reason on every row that is not a `MATCH`.
//!
//! Compiled only under `--features acadrust`, because `emit-acadrust` is.
//! `emit-acadsharp` has no feature, but it needs a native archive to run, and a
//! missing one is the run's failure under `PARITY_REQUIRE_CORPUS=1` and never a
//! row.
//!
//! The test runs both emitter binaries once per corpus row into
//! `target/parity/` (`PARITY_OUT` overrides), reads the oracle's frozen output,
//! compares each pair under the tables in `parity/TABLES.toml`, and holds the
//! verdicts to `parity/VERDICTS.toml`. It never runs Python, .NET or Docker: the
//! ground truths are files, produced offline.
//!
//! - `PARITY_REQUIRE_CORPUS=1`: a missing fixture, frozen output or emitter is a
//!   failure that names which. Unset, the test skips and prints why, so a default
//!   `cargo test` stays green.
//! - `PARITY_VERDICTS_BASE=<path>`: compare the run against that file instead of
//!   the committed one. The pre-push hook runs the test a second time against
//!   `git show <remote oid>:parity/VERDICTS.toml`, so a change that moves a
//!   verdict and re-records it in the same commit is red at push time with the
//!   moved rows in the log.
//! - `PARITY_REGEN_DIGESTS=1`: write this archive's digest table under
//!   `parity/emit/` (the committed one is otherwise held to).
//! - `PARITY_REGEN=1`: rewrite `parity/VERDICTS.toml` from the run, keeping the
//!   reason and issue of every row it finds, leaving new ones blank (which the
//!   test then refuses until a person fills them in).
#![cfg(feature = "acadrust")]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use acadsharp_rs_tests::canonical::{Sidecar, parse_stream};
use acadsharp_rs_tests::compare::Stream;
use acadsharp_rs_tests::digest;
use acadsharp_rs_tests::fixtures::{self, DwgFixture};
use acadsharp_rs_tests::parity_run::{Side, judge_pair, open_failed_reason};
use acadsharp_rs_tests::tables;
use acadsharp_rs_tests::verdict::{Row, Table, Verdict};

fn require() -> bool {
    std::env::var("PARITY_REQUIRE_CORPUS").as_deref() == Ok("1")
}

/// What a problem with the corpus (a missing fixture, a missing frozen output,
/// a missing emitter) does: under `PARITY_REQUIRE_CORPUS=1` it is a failure that
/// names which, and without it the test skips and prints why, so a default
/// `cargo test` stays green.
fn gate<T>(required: bool, r: Result<T, String>) -> Option<T> {
    match r {
        Ok(v) => Some(v),
        Err(e) if required => panic!("PARITY_REQUIRE_CORPUS=1 and {e}"),
        Err(e) => {
            eprintln!("skipped: {e}");
            None
        }
    }
}

fn out_dir() -> PathBuf {
    match std::env::var("PARITY_OUT") {
        Ok(p) if !p.is_empty() => PathBuf::from(p),
        _ => Path::new(env!("CARGO_TARGET_TMPDIR"))
            .join("..")
            .join("parity"),
    }
}

fn verdicts_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("parity")
        .join("VERDICTS.toml")
}

/// What an emitter run came to for one fixture.
enum Emitted {
    Ok(Side),
    Failed(String),
}

fn emit(bin: &str, engine: &str, id: &str, out: &Path) -> Result<Emitted, String> {
    let run = Command::new(bin)
        .args(["--fixture", id, "--out"])
        .arg(out)
        .output()
        .map_err(|e| format!("the {engine} emitter ({bin}) could not be started: {e}"))?;
    let stderr = String::from_utf8_lossy(&run.stderr).to_string();
    if !run.status.success() {
        if stderr.contains("Unlinked") {
            return Err(format!(
                "emit-acadsharp answered Error::Unlinked on {id}: no native archive is linked, and a run that decoded nothing is not a row. {stderr}"
            ));
        }
        return Ok(Emitted::Failed(open_failed_reason(
            run.status.code(),
            &stderr,
        )));
    }
    let jsonl = out.join(format!("{id}.{engine}.jsonl"));
    let text = std::fs::read_to_string(&jsonl).map_err(|e| format!("{}: {e}", jsonl.display()))?;
    let records = parse_stream(&text).map_err(|e| format!("{}: {e}", jsonl.display()))?;
    let side = out.join(format!("{id}.{engine}.src.jsonl"));
    let sidecar = std::fs::read_to_string(&side)
        .ok()
        .map(|t| Sidecar::parse(&t, &records).map_err(|e| format!("{}: {e}", side.display())))
        .transpose()?;
    Ok(Emitted::Ok(Side {
        label: jsonl.display().to_string(),
        stream: Stream { records, sidecar },
    }))
}

fn frozen(f: &DwgFixture) -> Result<Side, String> {
    let bytes = f
        .load_reference_jsonl()
        .map_err(|e| format!("{}: the oracle's frozen output is missing: {e}", f.id))?;
    let text = String::from_utf8(bytes).map_err(|e| e.to_string())?;
    let records = parse_stream(&text).map_err(|e| format!("{}: {e}", f.id))?;
    Ok(Side {
        label: format!("oracle:{}", f.id),
        stream: Stream::new(records),
    })
}

/// The whole run: `(fixture, pair) -> (verdict, first, reason-if-open-failed)`.
type Computed = BTreeMap<(String, String), (String, String, String)>;

/// What the acadsharp emitter wrote for every fixture it could read, as a digest
/// table for the archive and target that produced it.
struct Digests {
    file_name: String,
    header: String,
    rows: Vec<digest::Row>,
}

fn digests(out: &Path) -> Result<Option<Digests>, String> {
    let mut rows = Vec::new();
    let mut label: Option<(String, String, String)> = None;
    for f in fixtures::all() {
        let jsonl = out.join(format!("{}.acadsharp.jsonl", f.id));
        let Ok(text) = std::fs::read_to_string(&jsonl) else {
            continue;
        };
        rows.push(digest::row(&f.id, &text)?);
        if label.is_none() {
            let run = std::fs::read_to_string(out.join(format!("{}.acadsharp.run.json", f.id)))
                .map_err(|e| e.to_string())?;
            let run: serde_json::Value = serde_json::from_str(&run).map_err(|e| e.to_string())?;
            let get = |k: &str| run[k].as_str().map(str::to_string);
            let (Some(v), Some(l), Some(t)) = (
                get("native_artifact_version"),
                get("native_library_sha256"),
                get("target"),
            ) else {
                return Ok(None);
            };
            label = Some((v, l, t));
        }
    }
    let Some((version, lib, target)) = label else {
        return Ok(None);
    };
    Ok(Some(Digests {
        file_name: digest::file_name(&version, &lib, &target),
        header: format!("archive {version} library {lib} target {target}"),
        rows,
    }))
}

fn compute() -> Result<(Computed, Option<Digests>), String> {
    let tables = tables::load_all()?;
    let out = out_dir();
    std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
    let sharp = env!("CARGO_BIN_EXE_emit-acadsharp");
    let rust = env!("CARGO_BIN_EXE_emit-acadrust");

    // One emitter run per fixture and engine, in parallel; the comparison is
    // sequential and cheap.
    let all = fixtures::all();
    let workers = std::thread::available_parallelism()
        .map_or(4, usize::from)
        .min(8);
    let next = std::sync::atomic::AtomicUsize::new(0);
    type Both = (Result<Emitted, String>, Result<Emitted, String>);
    let results: std::sync::Mutex<BTreeMap<usize, Both>> = std::sync::Mutex::new(BTreeMap::new());
    std::thread::scope(|s| {
        for _ in 0..workers {
            s.spawn(|| {
                loop {
                    let i = next.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    let Some(f) = all.get(i) else { break };
                    let a = emit(sharp, "acadsharp", &f.id, &out);
                    let b = emit(rust, "acadrust", &f.id, &out);
                    results.lock().unwrap().insert(i, (a, b));
                }
            });
        }
    });
    let mut results = results.into_inner().unwrap();

    let mut computed = Computed::new();
    for (i, f) in all.iter().enumerate() {
        let (sharp_run, rust_run) = results.remove(&i).expect("every fixture was run");
        let sharp_run = sharp_run?;
        let rust_run = rust_run?;
        let oracle = frozen(f)?;
        let mut put = |pair: &str,
                       a: Result<&Side, &String>,
                       b: Result<&Side, &String>|
         -> Result<(), String> {
            let key = (f.id.clone(), pair.to_string());
            match (a, b) {
                (Ok(a), Ok(b)) => {
                    let j = judge_pair(&f.id, pair, a, b, &tables)?;
                    if std::env::var("PARITY_EXPLAIN").as_deref() == Ok("1") {
                        for line in &j.red {
                            println!("EXPLAIN {pair} {line}");
                        }
                    }
                    computed.insert(key, (j.verdict.to_string(), j.first, String::new()));
                }
                (a, b) => {
                    let why = [a.err(), b.err()]
                        .into_iter()
                        .flatten()
                        .cloned()
                        .collect::<Vec<_>>()
                        .join("; ");
                    computed.insert(key, (Verdict::OpenFailed.to_string(), String::new(), why));
                }
            }
            Ok(())
        };
        fn as_ref(e: &Emitted) -> Result<&Side, &String> {
            match e {
                Emitted::Ok(s) => Ok(s),
                Emitted::Failed(m) => Err(m),
            }
        }
        put("oracle/acadsharp", Ok(&oracle), as_ref(&sharp_run))?;
        put("oracle/acadrust", Ok(&oracle), as_ref(&rust_run))?;
        put("acadsharp/acadrust", as_ref(&sharp_run), as_ref(&rust_run))?;
    }
    let digests = digests(&out)?;
    Ok((computed, digests))
}

#[test]
fn the_run_agrees_with_the_committed_verdicts() {
    if !require() {
        eprintln!(
            "skipped: PARITY_REQUIRE_CORPUS is not 1, so a missing archive would not fail this run"
        );
        // Still cheap and still useful when everything is there: carry on only
        // if the native lane is live, otherwise there is nothing to compare.
        if option_env!("ACADSHARP_LINKED_NATIVE_DIR").is_none() {
            eprintln!("skipped: no native archive was linked into this build");
            return;
        }
    }
    let Some((computed, digests)) = gate(require(), compute()) else {
        return;
    };

    // The digest table for this archive on this target: reproduced exactly when
    // there is one, written when asked to, noted when there is not.
    let mut digest_failure: Option<String> = None;
    if let Some(d) = &digests {
        let committed = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("parity")
            .join("emit")
            .join(&d.file_name);
        if std::env::var("PARITY_REGEN_DIGESTS").as_deref() == Ok("1") {
            std::fs::create_dir_all(committed.parent().unwrap()).unwrap();
            std::fs::write(&committed, digest::render(&d.header, &d.rows)).unwrap();
            eprintln!("wrote {}", committed.display());
        } else if let Ok(text) = std::fs::read_to_string(&committed) {
            let want =
                digest::parse(&text).unwrap_or_else(|e| panic!("{}: {e}", committed.display()));
            let diff = digest::differences(&want, &d.rows);
            if !diff.is_empty() {
                digest_failure = Some(format!(
                    "emit-acadsharp no longer reproduces {}:\n  {}",
                    committed.display(),
                    diff.join("\n  ")
                ));
            }
        } else {
            eprintln!(
                "note: no digest table {} for this archive on this target",
                d.file_name
            );
        }
    }

    let committed_path = match std::env::var("PARITY_VERDICTS_BASE") {
        Ok(p) if !p.is_empty() => PathBuf::from(p),
        _ => verdicts_path(),
    };
    let regen = std::env::var("PARITY_REGEN").as_deref() == Ok("1");
    let committed = match std::fs::read_to_string(&committed_path) {
        Ok(t) => Table::parse(&t).unwrap_or_else(|e| panic!("{}: {e}", committed_path.display())),
        Err(_) if regen => Table::default(),
        Err(e) => panic!("{}: {e}", committed_path.display()),
    };

    if regen {
        let old = committed.by_key();
        let mut table = Table {
            runs: committed.runs.clone(),
            rows: Vec::new(),
        };
        for ((fixture, pair), (verdict, first, why)) in &computed {
            let kept = old.get(&(fixture.clone(), pair.clone()));
            let moved = kept.is_some_and(|r| &r.verdict != verdict);
            let needs = verdict != "MATCH";
            table.rows.push(Row {
                fixture: fixture.clone(),
                pair: pair.clone(),
                verdict: verdict.clone(),
                reason: if needs && !moved {
                    kept.map(|r| r.reason.clone())
                        .unwrap_or_else(|| why.clone())
                } else if verdict == "OPEN_FAILED" {
                    why.clone()
                } else {
                    String::new()
                },
                issue: if needs && !moved {
                    kept.map(|r| r.issue.clone()).unwrap_or_default()
                } else {
                    String::new()
                },
                first: first.clone(),
            });
        }
        std::fs::write(verdicts_path(), table.render()).unwrap();
        eprintln!("rewrote {}", verdicts_path().display());
        return;
    }

    let actual: BTreeMap<(String, String), (String, String)> = computed
        .iter()
        .map(|(k, (v, f, _))| (k.clone(), (v.clone(), f.clone())))
        .collect();
    let moved = committed.moved(&actual);
    assert!(
        moved.is_empty(),
        "{} verdict(s) moved against {}:\n  {}\n\nA verdict that moves, better or worse, is red until parity/VERDICTS.toml changes.",
        moved.len(),
        committed_path.display(),
        moved.join("\n  ")
    );
    let refusals = committed.refusals();
    assert!(
        refusals.is_empty(),
        "parity/VERDICTS.toml:\n  {}",
        refusals.join("\n  ")
    );
    // After the verdicts, so a moved row is what the log leads with.
    if let Some(f) = digest_failure {
        panic!("{f}");
    }
}

// ---- the properties of the test itself ------------------------------------------------

#[test]
fn a_problem_with_the_corpus_is_red_when_required_and_a_printed_skip_when_not() {
    assert_eq!(gate::<u8>(false, Err("a fixture is missing".into())), None);
    assert_eq!(gate(false, Ok(3)), Some(3));
    let caught = std::panic::catch_unwind(|| {
        gate::<u8>(true, Err("the frozen output of g13_line is missing".into()))
    });
    let msg = *caught
        .expect_err("required means a failure")
        .downcast::<String>()
        .unwrap();
    assert!(msg.contains("g13_line"), "{msg}");
}

#[test]
fn a_missing_emitter_is_named() {
    let err = match emit(
        "/no/such/dir/emit-acadsharp",
        "acadsharp",
        "g13_line",
        &out_dir(),
    ) {
        Err(e) => e,
        Ok(_) => panic!("a binary that is not there cannot have run"),
    };
    assert!(err.contains("emit-acadsharp"), "{err}");
}

#[test]
fn a_missing_frozen_output_is_named() {
    let mut f = fixtures::by_id("g13_line").unwrap().clone();
    f.reference_jsonl = Some("expected/no-such-file.jsonl.zst".into());
    let err = match frozen(&f) {
        Err(e) => e,
        Ok(_) => panic!("a file that is not there cannot have been read"),
    };
    assert!(err.contains("g13_line") && err.contains("frozen"), "{err}");
}

/// The end-to-end control: the comparator's sweep proves the comparator, this
/// proves the wiring. One circle's radius times 1.001 in a scratch copy of the
/// oracle's own output, through the same emit-and-judge path the run uses.
#[test]
fn one_radius_times_1_001_in_the_oracle_output_is_one_diff_through_the_parity_path() {
    let tables = tables::load_all().unwrap();
    let fx = fixtures::by_id("g13_circle").unwrap();
    let out = out_dir();
    let Some(Emitted::Ok(mine)) = gate(
        require(),
        emit(
            env!("CARGO_BIN_EXE_emit-acadrust"),
            "acadrust",
            &fx.id,
            &out,
        ),
    ) else {
        panic!("emit-acadrust failed on g13_circle");
    };
    let mut oracle = frozen(fx).unwrap();
    let before = judge_pair(&fx.id, "oracle/acadrust", &oracle, &mine, &tables).unwrap();
    assert_eq!(
        before.verdict,
        Verdict::Match,
        "the control needs a clean starting point: {}",
        before.first
    );
    let mut touched = 0;
    for r in &mut oracle.stream.records {
        if let acadsharp_rs_tests::canonical::Record::Circle(c) = r {
            c.radius *= 1.001;
            touched += 1;
            break;
        }
    }
    assert_eq!(touched, 1, "g13_circle has a circle");
    oracle.label = "oracle:g13_circle:scratch".into();
    let after = judge_pair(&fx.id, "oracle/acadrust", &oracle, &mine, &tables).unwrap();
    assert_eq!(after.verdict, Verdict::Diff(1), "{}", after.first);
    assert!(after.first.contains("circle.radius"), "{}", after.first);
}

/// `PARITY_VERDICTS_BASE` makes the run compare against that file instead of the
/// committed one, which is how a push that moves a verdict and re-records it in
/// the same change is caught: the hook runs this second pass against the
/// remote's copy. Run here as a child, so the first pass cannot hide it.
#[test]
fn a_base_with_one_row_moved_is_red_and_names_that_row() {
    if !require() && option_env!("ACADSHARP_LINKED_NATIVE_DIR").is_none() {
        eprintln!("skipped: no native archive was linked into this build");
        return;
    }
    let committed = Table::parse(&std::fs::read_to_string(verdicts_path()).unwrap()).unwrap();
    let mut moved = committed.clone();
    let row = moved
        .rows
        .iter_mut()
        .find(|r| r.verdict == "MATCH" && r.pair == "oracle/acadrust")
        .expect("a MATCH row to move");
    let (fixture, pair) = (row.fixture.clone(), row.pair.clone());
    row.verdict = "DIFF:1".into();
    let base = Path::new(env!("CARGO_TARGET_TMPDIR")).join("parity-base-moved.toml");
    std::fs::write(&base, moved.render()).unwrap();

    let me = std::env::current_exe().unwrap();
    let run = |base: Option<&Path>| {
        let mut cmd = Command::new(&me);
        cmd.args([
            "--exact",
            "the_run_agrees_with_the_committed_verdicts",
            "--nocapture",
        ]);
        // Its own output directory: two runs writing one `target/parity` at the
        // same time would read each other's half-written sidecars.
        let own =
            Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("parity-child-{}", base.is_some()));
        cmd.env("PARITY_OUT", own);
        cmd.env_remove("PARITY_VERDICTS_BASE")
            .env_remove("PARITY_REGEN")
            .env_remove("PARITY_REGEN_DIGESTS");
        if let Some(b) = base {
            cmd.env("PARITY_VERDICTS_BASE", b);
        }
        cmd.output().unwrap()
    };
    let with_base = run(Some(&base));
    let text = String::from_utf8_lossy(&with_base.stdout).to_string()
        + &String::from_utf8_lossy(&with_base.stderr);
    assert!(
        !with_base.status.success(),
        "a moved row in the base has to be red:\n{text}"
    );
    assert!(
        text.contains(&fixture) && text.contains(&pair),
        "the failure names the row:\n{text}"
    );
    assert!(
        text.contains("committed DIFF:1, run says MATCH"),
        "both verdicts:\n{text}"
    );

    // And with the variable unset the committed file is used, and the run passes.
    let plain = run(None);
    assert!(
        plain.status.success(),
        "{}",
        String::from_utf8_lossy(&plain.stderr)
    );
}
