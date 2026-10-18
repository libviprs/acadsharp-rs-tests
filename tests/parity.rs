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
    finish(run, engine, id, out, false)
}

/// The same, for bytes that are not a manifest row (a round-trip output): they go
/// in on standard input, so a reader that aborts on a writer's malformed file
/// takes only its own cell down.
fn emit_stdin(
    bin: &str,
    engine: &str,
    id: &str,
    kind: &str,
    bytes: &[u8],
    out: &Path,
) -> Result<Emitted, String> {
    use std::io::Write;
    use std::process::Stdio;
    let mut child = Command::new(bin)
        .args(["--fixture", id, "--stdin-as", kind, "--out"])
        .arg(out)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("the {engine} emitter ({bin}) could not be started: {e}"))?;
    let mut stdin = child.stdin.take().expect("stdin was piped");
    let data = bytes.to_vec();
    // From a thread: the child may exit before it has read everything.
    let feeder = std::thread::spawn(move || {
        let _ = stdin.write_all(&data);
    });
    let run = child
        .wait_with_output()
        .map_err(|e| format!("the {engine} emitter ({bin}) did not finish: {e}"))?;
    let _ = feeder.join();
    finish(run, engine, id, out, true)
}

/// `lenient`: a stream that is not schema 1 is the cell's outcome (a reader that
/// emitted garbage for a writer's output is a finding), not a failure of the
/// run. For the corpus it stays a failure of the run: our own emitter writing an
/// invalid stream there is a bug in the emitter.
fn finish(
    run: std::process::Output,
    engine: &str,
    id: &str,
    out: &Path,
    lenient: bool,
) -> Result<Emitted, String> {
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
    let records = match parse_stream(&text) {
        Ok(r) => r,
        Err(e) if lenient => {
            return Ok(Emitted::Failed(format!(
                "{engine} emitted a stream that is not schema 1: {e}"
            )));
        }
        Err(e) => return Err(format!("{}: {e}", jsonl.display())),
    };
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

const SAVES: [&str; 6] = [
    "acadsharp-ac1014",
    "acadsharp-ac1015",
    "acadsharp-ac1018",
    "acadsharp-ac1021",
    "acadsharp-ac1024",
    "acadsharp-ac1027",
];

fn as_ref(e: &Emitted) -> Result<&Side, &String> {
    match e {
        Emitted::Ok(s) => Ok(s),
        Emitted::Failed(m) => Err(m),
    }
}

/// ezdxf's frozen reading of one DXF, with its sidecar.
fn ezdxf_side(gt: &fixtures::GroundTruth) -> Result<Side, String> {
    let text = String::from_utf8(gt.load_jsonl().map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    let records = parse_stream(&text).map_err(|e| format!("{}: {e}", gt.id))?;
    let side = String::from_utf8(gt.load_sidecar().map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    let sidecar = Sidecar::parse(&side, &records).map_err(|e| format!("{}: {e}", gt.id))?;
    Ok(Side {
        label: format!("ezdxf:{}", gt.id),
        stream: Stream {
            records,
            sidecar: Some(sidecar),
        },
    })
}

/// One row of the run: judge the pair, or `OPEN_FAILED` when a side did not open.
fn put(
    computed: &mut Computed,
    tables: &BTreeMap<String, acadsharp_rs_tests::compare::ClassTable>,
    fixture: &str,
    pair: &str,
    a: Result<&Side, &String>,
    b: Result<&Side, &String>,
    explained: Option<&dyn Fn(&acadsharp_rs_tests::compare::Finding) -> bool>,
) -> Result<(), String> {
    let key = (fixture.to_string(), pair.to_string());
    match (a, b) {
        (Ok(a), Ok(b)) => {
            let j = match explained {
                Some(e) => {
                    acadsharp_rs_tests::parity_run::judge_self(fixture, pair, a, b, tables, e)?
                }
                None => judge_pair(fixture, pair, a, b, tables)?,
            };
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
    // Everything each engine said, by fixture, for the pairs built after the
    // per-fixture ones: ezdxf against each engine, and each engine against itself
    // across the seven saves.
    let mut held: BTreeMap<(&'static str, String), Emitted> = BTreeMap::new();
    let mut oracles: BTreeMap<String, Side> = BTreeMap::new();
    for (i, f) in all.iter().enumerate() {
        let (sharp_run, rust_run) = results.remove(&i).expect("every fixture was run");
        let sharp_run = sharp_run?;
        let rust_run = rust_run?;
        let oracle = frozen(f)?;
        put(
            &mut computed,
            &tables,
            &f.id,
            "oracle/acadsharp",
            Ok(&oracle),
            as_ref(&sharp_run),
            None,
        )?;
        put(
            &mut computed,
            &tables,
            &f.id,
            "oracle/acadrust",
            Ok(&oracle),
            as_ref(&rust_run),
            None,
        )?;
        put(
            &mut computed,
            &tables,
            &f.id,
            "acadsharp/acadrust",
            as_ref(&sharp_run),
            as_ref(&rust_run),
            None,
        )?;
        held.insert(("acadsharp", f.id.clone()), sharp_run);
        held.insert(("acadrust", f.id.clone()), rust_run);
        oracles.insert(f.id.clone(), oracle);
    }

    // ezdxf, reading AutoCAD's own DXF of a drawing, against each of the three.
    for gt in fixtures::ground_truths() {
        let Some(twin_of) = fixtures::oracle_inputs()
            .iter()
            .find(|i| i.id == gt.input)
            .and_then(|i| i.for_fixture.clone())
        else {
            // A DXF with no DWG twin (the AC1009 files) is an input for later work.
            continue;
        };
        let ez = ezdxf_side(gt)?;
        let oracle = oracles
            .get(&twin_of)
            .ok_or_else(|| format!("{}: its DWG twin {twin_of} is not a fixture", gt.input))?;
        put(
            &mut computed,
            &tables,
            &gt.input,
            "ezdxf/oracle",
            Ok(&ez),
            Ok(oracle),
            None,
        )?;
        for (engine, pair) in [
            ("acadsharp", "ezdxf/acadsharp"),
            ("acadrust", "ezdxf/acadrust"),
        ] {
            let theirs = held
                .get(&(engine, twin_of.clone()))
                .ok_or_else(|| format!("{twin_of}: no {engine} run"))?;
            put(
                &mut computed,
                &tables,
                &gt.input,
                pair,
                Ok(&ez),
                as_ref(theirs),
                None,
            )?;
        }
    }

    // Each engine against itself across the seven saves, baseline AC1032. The
    // oracle's differences are the exception list R3.4 committed; the other two
    // engines have none, so theirs are findings.
    let list =
        acadsharp_rs_tests::invariance::ExceptionList::oracle().map_err(|e| e.to_string())?;
    let baseline = list.baseline.clone();
    for save in SAVES {
        let o_base = oracles.get(&baseline).expect("the baseline is a fixture");
        let o_save = oracles.get(save).expect("a save is a fixture");
        let explained = |f: &acadsharp_rs_tests::compare::Finding| {
            list.row_for(save, f)
                .is_some_and(|r| r.kind == acadsharp_rs_tests::invariance::Kind::Exception)
        };
        put(
            &mut computed,
            &tables,
            save,
            "self/oracle",
            Ok(o_base),
            Ok(o_save),
            Some(&explained),
        )?;
        for engine in ["acadsharp", "acadrust"] {
            let base = held
                .get(&(engine, baseline.clone()))
                .ok_or("no baseline run")?;
            let this = held
                .get(&(engine, save.to_string()))
                .ok_or("no run of a save")?;
            put(
                &mut computed,
                &tables,
                save,
                &format!("self/{engine}"),
                as_ref(base),
                as_ref(this),
                None,
            )?;
        }
    }
    // R5.1: AutoCAD's own DXF, read by the oracle, acadrust and ezdxf. The twin of
    // a DWG keeps its handles, so each reader's DXF reading is held against its
    // own DWG reading of the same save, ASCII against binary, and reader against
    // reader. acadsharp-rs is EXCLUDED: its C ABI opens DWG only.
    dxf_lane(&mut computed, &tables, &out, rust, &oracles, &held)?;

    // R5.3: what each library writes, read back by the readers that are not it.
    roundtrip_lane(&mut computed, &tables, &out, sharp, rust, &oracles, &held)?;

    let digests = digests(&out)?;
    Ok((computed, digests))
}

// ---- the round-trip lane (R5.3) -------------------------------------------------------

/// The saves AutoCAD itself wrote at a lower version: the lowering table for an
/// output at that version is R3.4's invariance list for the save.
fn lowering_save(version: &str) -> String {
    format!("acadsharp-{}", version.to_ascii_lowercase())
}

/// One discrepancy, named by the reference record it is about.
type BaselineKey = (String, usize, String, String, String, String);

fn baseline_key(f: &acadsharp_rs_tests::compare::Finding) -> Option<BaselineKey> {
    use acadsharp_rs_tests::compare::FindingKind::{FieldMismatch, OnlyInA, RecordTypeMismatch};
    // Only kinds named by an A-side index: the reference stream is the same in
    // the baseline and in the cell, so its record index is the one coordinate that
    // cannot shift. A record only in B has no such name.
    if !matches!(f.kind, FieldMismatch | OnlyInA | RecordTypeMismatch) {
        return None;
    }
    Some((
        format!("{:?}", f.kind),
        f.index_a?,
        f.record_type.clone(),
        f.field.clone(),
        format!("{:?}", f.a),
        format!("{:?}", f.b),
    ))
}

/// What a reader already disagrees about with the writer's reading of the
/// *source* (R4.1's verdict for the pair), keyed by the reference record. A
/// round trip inherits those and is not charged for them; anything the output
/// adds is the writer's to answer for.
fn baseline_keys(
    table: &acadsharp_rs_tests::compare::ClassTable,
    source: &str,
    reference: &Side,
    reader_of_source: &Side,
) -> std::collections::BTreeSet<BaselineKey> {
    use acadsharp_rs_tests::compare::compare;
    compare(source, &reference.stream, &reader_of_source.stream, table)
        .map(|r| r.red().into_iter().filter_map(baseline_key).collect())
        .unwrap_or_default()
}

fn lowered_by_list(
    on: bool,
    list: &acadsharp_rs_tests::invariance::ExceptionList,
    save: &str,
    f: &acadsharp_rs_tests::compare::Finding,
) -> bool {
    on && list
        .row_for(save, f)
        .is_some_and(|r| r.kind == acadsharp_rs_tests::invariance::Kind::Exception)
}

/// What one reader made of one round-trip output.
type Reading<'a> = Result<&'a Side, &'a String>;

/// Judge one cell: `reader`'s reading of `o` against the writer's own reading of
/// its source.
fn put_cell(
    computed: &mut Computed,
    tables: &BTreeMap<String, acadsharp_rs_tests::compare::ClassTable>,
    o: &RtOutput,
    reference: &Side,
    explained: Option<&dyn Fn(&acadsharp_rs_tests::compare::Finding) -> bool>,
    reader: &str,
    got: Reading,
) -> Result<(), String> {
    let pair = format!("w-{}:{reader}", o.writer);
    put(computed, tables, &o.id, &pair, Ok(reference), got, explained)
}

/// One round-trip output, from wherever it came: a `[[written]]` row (stage one)
/// or the scratch directory of the nightly (stage two). Everything a cell needs is
/// here, so the judging does not care which.
struct RtOutput {
    id: String,
    writer: String,
    source: String,
    container: String,
    version: String,
    /// The bytes, or why the writer produced none.
    bytes: Result<Vec<u8>, String>,
    /// The oracle's reading, or why there is none.
    oracle: Result<Side, String>,
    /// ezdxf's reading of a DXF output, or why there is none (`None` for a DWG).
    ezdxf: Option<Result<Side, String>>,
}

/// The oracle's frozen reading of an output, as a side. A file so damaged that the
/// oracle's own stream breaks the schema (a corrupt string in a warning) is that
/// cell's outcome, not a failure of the run.
fn oracle_written_side(id: &str, text: &str) -> Result<Side, String> {
    match parse_stream(text) {
        Ok(records) => Ok(Side {
            label: format!("oracle-written:{id}"),
            stream: Stream::new(records),
        }),
        Err(e) => Err(format!("the oracle's reading is not schema 1: {e}")),
    }
}

/// Stage one: the rows of `fixtures/manifest.toml`.
fn stage_one_outputs() -> Result<Vec<RtOutput>, String> {
    let mut v = Vec::new();
    for w in fixtures::written() {
        // The bytes: stored (ACadSharp's) or made again here (acadrust's), and in
        // both cases the bytes the manifest froze.
        let bytes: Result<Vec<u8>, String> = if let Some(why) = &w.write_failed {
            Err(format!("the writer failed: {why}"))
        } else if w.is_stored() {
            w.load().map_err(|e| e.to_string())
        } else {
            let src = fixtures::load(&w.source).map_err(|e| e.to_string())?;
            let made = acadsharp_rs_tests::acadrust_emit::rewrite(&src, &w.container, &w.version)
                .map_err(|e| format!("{}: acadrust cannot write it any more: {e}", w.id))?;
            w.verify(&made)
                .map_err(|e| format!("the writer is not deterministic or has moved: {e}"))?;
            Ok(made)
        };
        let oracle = match (&w.reference_jsonl, &w.no_reference) {
            (Some(_), _) => {
                let text = String::from_utf8(
                    w.load_reference_jsonl()
                        .map_err(|e| format!("{}: {e}", w.id))?,
                )
                .map_err(|e| e.to_string())?;
                oracle_written_side(&w.id, &text)
            }
            (None, Some(why)) => Err(format!("the oracle cannot read it: {why}")),
            (None, None) => Err(format!(
                "{}: the oracle neither read it nor said why not",
                w.id
            )),
        };
        let ezdxf = if w.container == "dwg" || w.write_failed.is_some() {
            None
        } else {
            Some(match fixtures::ground_truth(&w.id) {
                Some(gt) => match &gt.failed {
                    Some(why) => Err(format!("ezdxf would not read it: {why}")),
                    None => ezdxf_side(gt),
                },
                None => return Err(format!("{}: no ezdxf reading of a DXF output", w.id)),
            })
        };
        v.push(RtOutput {
            id: w.id.clone(),
            writer: w.writer.clone(),
            source: w.source.clone(),
            container: w.container.clone(),
            version: w.version.clone(),
            bytes,
            oracle,
            ezdxf,
        });
    }
    Ok(v)
}

fn roundtrip_lane(
    computed: &mut Computed,
    base: &BTreeMap<String, acadsharp_rs_tests::compare::ClassTable>,
    out: &Path,
    sharp: &str,
    rust: &str,
    oracles: &BTreeMap<String, Side>,
    held: &BTreeMap<(&'static str, String), Emitted>,
) -> Result<(), String> {
    let outputs = stage_one_outputs()?;
    judge_outputs(computed, base, out, sharp, rust, oracles, held, &outputs)
}

#[allow(clippy::too_many_arguments)]
fn judge_outputs(
    computed: &mut Computed,
    base: &BTreeMap<String, acadsharp_rs_tests::compare::ClassTable>,
    out: &Path,
    sharp: &str,
    rust: &str,
    oracles: &BTreeMap<String, Side>,
    held: &BTreeMap<(&'static str, String), Emitted>,
    outputs: &[RtOutput],
) -> Result<(), String> {
    use acadsharp_rs_tests::invariance::ExceptionList;
    let list = ExceptionList::oracle().map_err(|e| e.to_string())?;

    // A cell inherits the divergences its reader already carries on the source:
    // the table for (writer, reader) starts from the R4.1 table for the same
    // engines, rows re-keyed to each output. One that stops holding after the
    // round trip is the comparator's stale-row finding, which is red.
    let mut tables = base.clone();
    for (cell, from) in [
        ("w-acadsharp:acadrust", "oracle/acadrust"),
        ("w-acadsharp:acadsharp", "oracle/acadsharp"),
    ] {
        let rows: Vec<_> = base.get(from).map(|t| t.known.clone()).unwrap_or_default();
        if let Some(t) = tables.get_mut(cell) {
            for o in outputs.iter().filter(|o| o.writer == "acadsharp") {
                for r in rows.iter().filter(|r| r.fixture == o.source) {
                    let mut r = r.clone();
                    r.fixture = o.id.clone();
                    t.known.push(r);
                }
            }
        }
    }

    for o in outputs {
        let kind = if o.container == "dwg" { "dwg" } else { "dxf" };
        let version_row = format!("w-{}:version", o.writer);
        let readers: &[&str] = if kind == "dwg" {
            &["oracle", "acadrust", "acadsharp"]
        } else {
            &["oracle", "acadrust", "ezdxf"]
        };

        // The writer's own reading of the source is the reference. When it has
        // none (the library could not read its own source) every cell says so.
        let reference: Result<&Side, String> = match o.writer.as_str() {
            "acadsharp" => oracles
                .get(&o.source)
                .ok_or_else(|| format!("{}: its source {} is not a fixture", o.id, o.source)),
            _ => match held.get(&("acadrust", o.source.clone())) {
                Some(Emitted::Ok(s)) => Ok(s),
                Some(Emitted::Failed(why)) => Err(format!(
                    "acadrust did not read the source {}, so there is no reference: {why}",
                    o.source
                )),
                None => Err(format!("{}: acadrust never read {}", o.id, o.source)),
            },
        };
        let reference = match reference {
            Ok(r) => r,
            Err(why) => {
                for r in readers {
                    computed.insert(
                        (o.id.clone(), format!("w-{}:{r}", o.writer)),
                        (Verdict::OpenFailed.to_string(), String::new(), why.clone()),
                    );
                }
                computed.insert(
                    (o.id.clone(), version_row.clone()),
                    (Verdict::OpenFailed.to_string(), String::new(), why),
                );
                continue;
            }
        };

        if let Err(why) = &o.bytes {
            // The writer produced nothing, so no reader had anything to open.
            for r in readers {
                put_cell(computed, &tables, o, reference, None, r, Err(why))?;
            }
            computed.insert(
                (o.id.clone(), version_row),
                (Verdict::OpenFailed.to_string(), String::new(), why.clone()),
            );
            continue;
        }
        let bytes = o.bytes.as_ref().expect("checked above");

        // The version it says it is: the oracle, a reader that did not write it,
        // is the witness. Every cell excludes `document.acad_version` (the
        // version is what was written), so this is where a writer that writes
        // the wrong one is caught.
        computed.insert(
            (o.id.clone(), version_row),
            match &o.oracle {
                Ok(side) => {
                    let said = match side.stream.records.first() {
                        Some(acadsharp_rs_tests::canonical::Record::Document(d)) => {
                            d.acad_version.clone()
                        }
                        _ => String::new(),
                    };
                    if said == o.version {
                        (Verdict::Match.to_string(), String::new(), String::new())
                    } else {
                        (
                            Verdict::Diff(1).to_string(),
                            format!("{} wrote {}, the oracle reads {said:?}", o.id, o.version),
                            String::new(),
                        )
                    }
                }
                Err(_) => (
                    Verdict::OpenFailed.to_string(),
                    String::new(),
                    "the oracle cannot read it, so nothing independent says its version"
                        .to_string(),
                ),
            },
        );

        // A finding is explained when R3.4 lists it as the format's own lowering
        // for this version (ACadSharp's rewrites of the oracle's reference), or
        // when the reader already disagrees with the writer's reading of the
        // *source* in exactly that way (R4.1's verdict, inherited).
        let lowered =
            o.writer == "acadsharp" && list.fixtures().contains(lowering_save(&o.version).as_str());
        let make_explained = |baseline: std::collections::BTreeSet<BaselineKey>| {
            let save = lowering_save(&o.version);
            let list = &list;
            let writer_is_acadsharp = o.writer == "acadsharp";
            move |f: &acadsharp_rs_tests::compare::Finding| {
                if baseline_key(f).is_some_and(|k| baseline.contains(&k)) {
                    return true;
                }
                lowered_by_list(writer_is_acadsharp && lowered, list, &save, f)
            }
        };
        // Each reader's reading of the source, for the baseline it inherits.
        let held_side = |engine: &'static str| -> Option<&Side> {
            match held.get(&(engine, o.source.clone())) {
                Some(Emitted::Ok(s)) => Some(s),
                _ => None,
            }
        };
        let judge = |computed: &mut Computed,
                     reader: &str,
                     of_source: Option<&Side>,
                     got: Reading|
         -> Result<(), String> {
            let pair = format!("w-{}:{reader}", o.writer);
            let baseline = match (of_source, tables.get(&pair)) {
                (Some(side), Some(table)) => baseline_keys(table, &o.source, reference, side),
                _ => Default::default(),
            };
            let explained = make_explained(baseline);
            put_cell(
                computed,
                &tables,
                o,
                reference,
                Some(&explained),
                reader,
                got,
            )
        };

        match &o.oracle {
            Ok(s) => judge(computed, "oracle", oracles.get(&o.source), Ok(s))?,
            Err(why) => judge(computed, "oracle", None, Err(why))?,
        }
        let rust_run = emit_stdin(rust, "acadrust", &o.id, kind, bytes, out)?;
        judge(
            computed,
            "acadrust",
            held_side("acadrust"),
            as_ref(&rust_run),
        )?;
        if kind == "dwg" {
            let sharp_run = emit_stdin(sharp, "acadsharp", &o.id, kind, bytes, out)?;
            judge(
                computed,
                "acadsharp",
                held_side("acadsharp"),
                as_ref(&sharp_run),
            )?;
        } else {
            // AutoCAD's own DXF of the source is ezdxf's baseline, when the
            // source has one (the saves do; the synthetics do not).
            let twin = fixtures::oracle_inputs()
                .iter()
                .find(|i| {
                    i.for_fixture.as_deref() == Some(o.source.as_str())
                        && i.encoding.as_deref() == Some("ascii")
                })
                .and_then(|i| fixtures::ground_truth(&i.id))
                .map(ezdxf_side)
                .transpose()?;
            match o.ezdxf.as_ref().ok_or_else(|| format!("{}: no ezdxf reading", o.id))? {
                Ok(side) => judge(computed, "ezdxf", twin.as_ref(), Ok(side))?,
                Err(why) => judge(computed, "ezdxf", twin.as_ref(), Err(why))?,
            }
        }
    }
    Ok(())
}

// ---- the DXF lane (R5.1) --------------------------------------------------------------

/// One DXF input as the three readers that open DXF read it.
struct DxfReadings {
    oracle: Side,
    acadrust: Emitted,
    ezdxf: Side,
}

/// The inputs the lane covers: every DXF the oracle reads. The two AC1009 files
/// are R5.2's (the oracle cannot read R12 into schema 1 yet, which is their
/// `no_reference` row), so they are not in this lane.
fn dxf_inputs() -> Vec<&'static fixtures::OracleInput> {
    fixtures::oracle_inputs()
        .iter()
        .filter(|i| i.no_reference.is_none())
        .collect()
}

fn oracle_dxf_side(i: &fixtures::OracleInput) -> Result<Side, String> {
    let bytes = i
        .load_reference_jsonl()
        .map_err(|e| format!("{}: the oracle's frozen DXF reading is missing: {e}", i.id))?;
    let text = String::from_utf8(bytes).map_err(|e| e.to_string())?;
    let records = parse_stream(&text).map_err(|e| format!("{}: {e}", i.id))?;
    Ok(Side {
        label: format!("oracle-dxf:{}", i.id),
        stream: Stream::new(records),
    })
}

fn dxf_lane(
    computed: &mut Computed,
    tables: &BTreeMap<String, acadsharp_rs_tests::compare::ClassTable>,
    out: &Path,
    rust: &str,
    oracles: &BTreeMap<String, Side>,
    held: &BTreeMap<(&'static str, String), Emitted>,
) -> Result<(), String> {
    let inputs = dxf_inputs();
    // acadrust over each DXF, a few at a time; the comparisons stay sequential.
    let next = std::sync::atomic::AtomicUsize::new(0);
    let runs: std::sync::Mutex<BTreeMap<usize, Result<Emitted, String>>> =
        std::sync::Mutex::new(BTreeMap::new());
    std::thread::scope(|s| {
        for _ in 0..4 {
            s.spawn(|| {
                loop {
                    let i = next.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    let Some(input) = inputs.get(i) else { break };
                    let r = emit(rust, "acadrust", &input.id, out);
                    runs.lock().unwrap().insert(i, r);
                }
            });
        }
    });
    let mut runs = runs.into_inner().unwrap();
    let mut read: BTreeMap<String, DxfReadings> = BTreeMap::new();
    for (i, input) in inputs.iter().enumerate() {
        let gt = fixtures::ground_truth(&input.id)
            .ok_or_else(|| format!("{}: no ezdxf ground truth", input.id))?;
        read.insert(
            input.id.clone(),
            DxfReadings {
                oracle: oracle_dxf_side(input)?,
                acadrust: runs.remove(&i).expect("every input was run")?,
                ezdxf: ezdxf_side(gt)?,
            },
        );
    }

    for input in &inputs {
        let r = &read[&input.id];
        let id = input.id.as_str();
        // Reader against reader on the DXF, the pairs the DWG has.
        put(
            computed,
            tables,
            id,
            "dxf:oracle/acadrust",
            Ok(&r.oracle),
            as_ref(&r.acadrust),
            None,
        )?;
        put(
            computed,
            tables,
            id,
            "dxf:ezdxf/oracle",
            Ok(&r.ezdxf),
            Ok(&r.oracle),
            None,
        )?;
        put(
            computed,
            tables,
            id,
            "dxf:ezdxf/acadrust",
            Ok(&r.ezdxf),
            as_ref(&r.acadrust),
            None,
        )?;
        // Each reader's DXF against its own DWG reading of the same drawing.
        if let Some(twin) = &input.for_fixture {
            let o_dwg = oracles
                .get(twin)
                .ok_or_else(|| format!("{id}: its DWG twin {twin} is not a fixture"))?;
            let a_dwg = held
                .get(&("acadrust", twin.clone()))
                .ok_or_else(|| format!("{twin}: no acadrust run"))?;
            put(
                computed,
                tables,
                id,
                "dxf-dwg:oracle",
                Ok(&r.oracle),
                Ok(o_dwg),
                None,
            )?;
            put(
                computed,
                tables,
                id,
                "dxf-dwg:acadrust",
                as_ref(&r.acadrust),
                as_ref(a_dwg),
                None,
            )?;
            computed.insert(
                (id.to_string(), "dxf-dwg:acadsharp".to_string()),
                (Verdict::Excluded.to_string(), String::new(), String::new()),
            );
        }
    }

    // ASCII against binary, per reader, one row per save.
    for ascii in inputs
        .iter()
        .filter(|i| i.encoding.as_deref() == Some("ascii"))
    {
        let binary_id = ascii.id.replace("-ascii", "-binary");
        let (Some(a), Some(b)) = (read.get(&ascii.id), read.get(&binary_id)) else {
            continue;
        };
        let save = ascii.id.replace("-ascii", "");
        put(
            computed,
            tables,
            &save,
            "dxf-ascii-binary:oracle",
            Ok(&a.oracle),
            Ok(&b.oracle),
            None,
        )?;
        put(
            computed,
            tables,
            &save,
            "dxf-ascii-binary:acadrust",
            as_ref(&a.acadrust),
            as_ref(&b.acadrust),
            None,
        )?;
        put(
            computed,
            tables,
            &save,
            "dxf-ascii-binary:ezdxf",
            Ok(&a.ezdxf),
            Ok(&b.ezdxf),
            None,
        )?;
    }
    Ok(())
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

// ---- the DXF lane's positive control (R5.1) -----------------------------------------------

/// R3.4's control, run through this lane: in a scratch copy of AutoCAD's own
/// twin, move the first LINE's group 10 by 1.0 (the extractor test in
/// `tools/ground_truth/tests` does the same to the file ezdxf reads). The lane's
/// DXF-against-DWG row for that twin must then leave the verdict the file
/// committed, and name a line. Without it the lane could be comparing a reading
/// with itself and nobody would know.
#[test]
fn a_moved_coordinate_in_a_scratch_copy_of_a_twin_turns_its_dxf_against_dwg_row_red() {
    use acadsharp_rs_tests::acadrust_emit::{extract_bytes, extract_dxf_bytes};
    use acadsharp_rs_tests::fixtures::ManifestFile;

    let twin = fixtures::oracle_inputs()
        .iter()
        .find(|i| i.id == "dxf-sample-ac1015-ascii")
        .expect("the AC1015 ASCII twin");
    let dwg = fixtures::by_id(twin.for_fixture.as_deref().unwrap())
        .unwrap()
        .load()
        .unwrap();
    let text = String::from_utf8(twin.load().unwrap()).expect("an ASCII DXF is text");

    let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
    let mut in_line = false;
    let mut moved = None;
    for i in (0..lines.len().saturating_sub(1)).step_by(2) {
        let (code, value) = (lines[i].trim().to_string(), lines[i + 1].trim().to_string());
        if code == "0" {
            in_line = value == "LINE";
        } else if in_line && code == "10" {
            let x: f64 = value.parse().unwrap();
            lines[i + 1] = format!("{}", x + 1.0);
            moved = Some(i);
            break;
        }
    }
    assert!(moved.is_some(), "no LINE with a group 10 in the twin");
    let scratch = lines.join("\n") + "\n";
    assert_ne!(scratch, text, "the mutation has to change the file");

    let side = |label: &str, e: acadsharp_rs_tests::emit::Emission| {
        let sidecar = Sidecar::parse(
            &acadsharp_rs_tests::canonical::render_sidecar(&e.sidecar),
            &e.records,
        )
        .unwrap();
        Side {
            label: label.into(),
            stream: Stream {
                records: e.records,
                sidecar: Some(sidecar),
            },
        }
    };
    let tables = tables::load_all().unwrap();
    let dwg_side = side("acadrust-dwg", extract_bytes(&dwg).unwrap());
    let judge = |label: &str, bytes: &[u8]| {
        let dxf = side(label, extract_dxf_bytes(bytes).unwrap());
        judge_pair(&twin.id, "dxf-dwg:acadrust", &dxf, &dwg_side, &tables).unwrap()
    };
    let untouched = judge("acadrust-dxf", text.as_bytes());
    let mutated = judge("acadrust-dxf-scratch", scratch.as_bytes());

    let committed = Table::parse(&std::fs::read_to_string(verdicts_path()).unwrap()).unwrap();
    let row = committed.by_key()[&(twin.id.clone(), "dxf-dwg:acadrust".to_string())]
        .verdict
        .clone();
    assert_eq!(
        untouched.verdict.to_string(),
        row,
        "the control starts from the committed row"
    );
    assert_ne!(
        mutated.verdict.to_string(),
        row,
        "a moved coordinate must move the verdict"
    );
    assert!(
        matches!(mutated.verdict, Verdict::Diff(n) if n >= 1),
        "{:?}",
        mutated.verdict
    );
    assert!(
        mutated.red.iter().any(|f| f.contains("line.")),
        "the finding names the line: {:?}",
        mutated.red
    );
}

// ---- the round-trip lane's control (R5.3) -------------------------------------------------

/// The control on the hashes: acadrust writes the same bytes twice, at every
/// version it claims, in every container. The lane regenerates its outputs on every
/// run and holds them to the manifest's sha256, which only means something while
/// this holds.
#[test]
fn acadrust_writes_the_same_bytes_twice() {
    use acadsharp_rs_tests::acadrust_emit::rewrite;
    let source = fixtures::load("acadsharp-ac1032").unwrap();
    for (container, version) in [
        ("dwg", "AC1015"),
        ("dwg", "AC1032"),
        ("dxf-ascii", "AC1018"),
        ("dxf-binary", "AC1027"),
    ] {
        let a = rewrite(&source, container, version).unwrap();
        let b = rewrite(&source, container, version).unwrap();
        assert!(a == b, "{container} {version}: two writes differ");
        assert!(a.len() > 1000, "{container} {version}: {} bytes", a.len());
    }
}

/// A manifest row is the bytes acadrust writes today: if the writer moves, the row
/// has to be regenerated on purpose.
#[test]
fn every_acadrust_row_is_what_acadrust_writes_now() {
    use acadsharp_rs_tests::acadrust_emit::rewrite;
    for w in fixtures::written()
        .iter()
        .filter(|w| w.writer == "acadrust" && w.write_failed.is_none())
    {
        let src = fixtures::load(&w.source).unwrap();
        let made =
            rewrite(&src, &w.container, &w.version).unwrap_or_else(|e| panic!("{}: {e}", w.id));
        w.verify(&made).unwrap_or_else(|e| panic!("{e}"));
    }
}

// ---- the code page lane (R5.4) -----------------------------------------------------------
//
// One authored drawing per code page family, written by both libraries as a DWG and as a
// pre-2007 ASCII DXF, read by the three readers that open each, and every reader's text
// held against the authored string, not against another reader. The verdicts are in
// `parity/CODEPAGE_VERDICTS.toml` under the same rules as the others; the words:
//
//   MATCH        every authored string comes back exactly
//   DIFF:n       n strings come back different (the first is in `first`, with the code page)
//   COUNT:a/b    the reader returned b text records where a were authored
//   OPEN_FAILED  the writer produced nothing, or the reader would not open it

fn codepage_verdicts_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("parity")
        .join("CODEPAGE_VERDICTS.toml")
}

fn texts_of(records: &[acadsharp_rs_tests::canonical::Record]) -> Vec<String> {
    records
        .iter()
        .filter_map(|r| match r {
            acadsharp_rs_tests::canonical::Record::Text(t) => Some(t.text.clone()),
            _ => None,
        })
        .collect()
}

/// `got` against `authored`: the verdict word and the first difference, naming the code
/// page, the kind of string and both strings with their code points.
fn judge_texts(
    page: &str,
    kinds: &[&str],
    authored: &[String],
    got: &[String],
) -> (String, String) {
    let cp = |s: &str| {
        s.chars()
            .map(|c| format!("U+{:04X}", c as u32))
            .collect::<Vec<_>>()
            .join(" ")
    };
    if authored.len() != got.len() {
        return (
            Verdict::Count(authored.len(), got.len()).to_string(),
            format!(
                "{page}: {} texts authored, {} read: {got:?}",
                authored.len(),
                got.len()
            ),
        );
    }
    let bad: Vec<usize> = (0..authored.len())
        .filter(|&i| authored[i] != got[i])
        .collect();
    match bad.first() {
        None => (Verdict::Match.to_string(), String::new()),
        Some(&i) => (
            Verdict::Diff(bad.len()).to_string(),
            format!(
                "{page} [{}] {}: authored {:?} ({}) came back {:?} ({})",
                bad.iter().map(|&j| kinds[j]).collect::<Vec<_>>().join(","),
                kinds[i],
                authored[i],
                cp(&authored[i]),
                got[i],
                cp(&got[i])
            ),
        ),
    }
}

fn codepage_computed(sharp: &str, rust: &str, out: &Path) -> Result<Computed, String> {
    let plan = acadsharp_rs_tests::codepage::plan();
    let mut computed = Computed::new();
    for a in fixtures::authored() {
        let family = plan
            .family
            .iter()
            .find(|f| f.page == a.page)
            .ok_or_else(|| format!("{}: no family {}", a.id, a.page))?;
        let authored_texts = family.texts(&plan.unheld.text);
        let kinds: Vec<&str> = authored_texts.iter().map(|t| t.kind).collect();
        let authored: Vec<String> = authored_texts.iter().map(|t| t.text.clone()).collect();
        let kind = if a.container == "dwg" { "dwg" } else { "dxf" };
        let readers: &[&str] = if kind == "dwg" {
            &["oracle", "acadrust", "acadsharp"]
        } else {
            &["oracle", "acadrust", "ezdxf"]
        };
        let put_open_failed = |computed: &mut Computed, reader: &str, why: String| {
            computed.insert(
                (a.id.clone(), format!("cp:{reader}")),
                (Verdict::OpenFailed.to_string(), String::new(), why),
            );
        };
        if let Some(why) = &a.write_failed {
            for r in readers {
                put_open_failed(&mut computed, r, format!("the writer failed: {why}"));
            }
            continue;
        }
        let bytes = a.load().map_err(|e| e.to_string())?;
        for reader in readers {
            let got: Result<Vec<String>, String> = match *reader {
                "oracle" => match (&a.reference_jsonl, &a.no_reference) {
                    (Some(_), _) => {
                        let t =
                            String::from_utf8(a.load_reference_jsonl().map_err(|e| e.to_string())?)
                                .map_err(|e| e.to_string())?;
                        parse_stream(&t)
                            .map(|r| texts_of(&r))
                            .map_err(|e| format!("the oracle's reading is not schema 1: {e}"))
                    }
                    (None, Some(why)) => Err(format!("the oracle cannot read it: {why}")),
                    (None, None) => Err("the oracle neither read it nor said why not".to_string()),
                },
                "ezdxf" => match fixtures::ground_truth(&a.id) {
                    Some(gt) => match &gt.failed {
                        Some(why) => Err(format!("ezdxf would not read it: {why}")),
                        None => ezdxf_side(gt).map(|s| texts_of(&s.stream.records)),
                    },
                    None => Err(format!("{}: no ezdxf reading", a.id)),
                },
                engine => {
                    let bin = if engine == "acadrust" { rust } else { sharp };
                    match emit_stdin(bin, engine, &a.id, kind, &bytes, out)? {
                        Emitted::Ok(s) => Ok(texts_of(&s.stream.records)),
                        Emitted::Failed(why) => Err(why),
                    }
                }
            };
            match got {
                Ok(g) => {
                    let (v, first) = judge_texts(&a.id, &kinds, &authored, &g);
                    // The page named in the first difference, not the id.
                    let first = first.replacen(&a.id, &family.header, 1);
                    computed.insert(
                        (a.id.clone(), format!("cp:{reader}")),
                        (v, first, String::new()),
                    );
                }
                Err(why) => put_open_failed(&mut computed, reader, why),
            }
        }
    }
    Ok(computed)
}

#[test]
fn the_code_page_lane_agrees_with_the_committed_verdicts() {
    if !require() && option_env!("ACADSHARP_LINKED_NATIVE_DIR").is_none() {
        eprintln!("skipped: no native archive was linked into this build");
        return;
    }
    let out = out_dir().join("codepage");
    std::fs::create_dir_all(&out).unwrap();
    let sharp = env!("CARGO_BIN_EXE_emit-acadsharp");
    let rust = env!("CARGO_BIN_EXE_emit-acadrust");
    let Some(computed) = gate(require(), codepage_computed(sharp, rust, &out)) else {
        return;
    };
    let path = codepage_verdicts_path();
    let regen = std::env::var("PARITY_REGEN").as_deref() == Ok("1");
    let committed = match std::fs::read_to_string(&path) {
        Ok(t) => Table::parse(&t).unwrap_or_else(|e| panic!("{}: {e}", path.display())),
        Err(_) if regen => Table::default(),
        Err(e) => panic!("{}: {e}", path.display()),
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
        std::fs::write(&path, table.render()).unwrap();
        eprintln!("rewrote {}", path.display());
        return;
    }
    let actual: BTreeMap<(String, String), (String, String)> = computed
        .iter()
        .map(|(k, (v, f, _))| (k.clone(), (v.clone(), f.clone())))
        .collect();
    let moved = committed.moved(&actual);
    assert!(
        moved.is_empty(),
        "{} code page verdict(s) moved against {}:\n  {}",
        moved.len(),
        path.display(),
        moved
            .iter()
            .take(40)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n  ")
    );
    let refusals = committed.refusals();
    assert!(
        refusals.is_empty(),
        "{}:\n  {}",
        path.display(),
        refusals
            .iter()
            .take(40)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n  ")
    );
}

// ---- the round-trip lane, stage two: the nightly (R5.3.1) ---------------------------------
//
// One more ignored function in this file, selected by `tools/run-tests.sh
// --stage-two <dir>` and by nothing else: the default `cargo test` and the PR gate
// never run it, and `tests/stage_two_policy.rs` holds the selector to the target.
// Its outputs are not in the repository. `tools/stage_two.py` makes them and every
// frozen reading of them in a scratch directory on the docker host (that is the
// nightly, run on MARS while Actions is off), and this reads that directory.

/// The cells of one source: three readers for each of the 48 outputs
/// (22 from ACadSharp and 26 from acadrust) and one version row each.
const CELLS_PER_SOURCE: usize = 48 * 3 + 48;

/// Stage two's sources and, written out per source, how many cells each makes. A
/// source that silently produces nothing is red, and the sum is the table's size.
const STAGE_TWO: [(&str, usize); 48] = [
    ("acadsharp-ac1014", CELLS_PER_SOURCE),
    ("acadsharp-ac1015", CELLS_PER_SOURCE),
    ("acadsharp-ac1018", CELLS_PER_SOURCE),
    ("acadsharp-ac1021", CELLS_PER_SOURCE),
    ("acadsharp-ac1024", CELLS_PER_SOURCE),
    ("acadsharp-ac1027", CELLS_PER_SOURCE),
    ("acadsharp-sample-base", CELLS_PER_SOURCE),
    ("g11_codepage", CELLS_PER_SOURCE),
    ("g11_shapes", CELLS_PER_SOURCE),
    ("g13_arc", CELLS_PER_SOURCE),
    ("g13_bad_extents", CELLS_PER_SOURCE),
    ("g13_circle", CELLS_PER_SOURCE),
    ("g13_deep_blocks", CELLS_PER_SOURCE),
    ("g13_dimension", CELLS_PER_SOURCE),
    ("g13_dimension_shallow", CELLS_PER_SOURCE),
    ("g13_ellipse", CELLS_PER_SOURCE),
    ("g13_empty_view", CELLS_PER_SOURCE),
    ("g13_face3d", CELLS_PER_SOURCE),
    ("g13_hatch", CELLS_PER_SOURCE),
    ("g13_insert", CELLS_PER_SOURCE),
    ("g13_leader", CELLS_PER_SOURCE),
    ("g13_line", CELLS_PER_SOURCE),
    ("g13_long_text", CELLS_PER_SOURCE),
    ("g13_mesh", CELLS_PER_SOURCE),
    ("g13_mesh_bad_faces", CELLS_PER_SOURCE),
    ("g13_mirrored_bulge", CELLS_PER_SOURCE),
    ("g13_mline", CELLS_PER_SOURCE),
    ("g13_nonuniform", CELLS_PER_SOURCE),
    ("g13_ocs_mirror", CELLS_PER_SOURCE),
    ("g13_ocs_plane", CELLS_PER_SOURCE),
    ("g13_ocs_rotated", CELLS_PER_SOURCE),
    ("g13_ocs_skew", CELLS_PER_SOURCE),
    ("g13_point", CELLS_PER_SOURCE),
    ("g13_polyface_mesh", CELLS_PER_SOURCE),
    ("g13_polygon_mesh", CELLS_PER_SOURCE),
    ("g13_polyline", CELLS_PER_SOURCE),
    ("g13_ray_xline", CELLS_PER_SOURCE),
    ("g13_scale_1x", CELLS_PER_SOURCE),
    ("g13_slot", CELLS_PER_SOURCE),
    ("g13_slot_block", CELLS_PER_SOURCE),
    ("g13_solid", CELLS_PER_SOURCE),
    ("g13_spline", CELLS_PER_SOURCE),
    ("g13_text", CELLS_PER_SOURCE),
    ("g13_tolerance", CELLS_PER_SOURCE),
    ("g13_two_entities", CELLS_PER_SOURCE),
    ("g13_unsupported", CELLS_PER_SOURCE),
    ("g13_wipeout", CELLS_PER_SOURCE),
    ("g13_xref", CELLS_PER_SOURCE),
];

/// Refusal fixtures have no cells: nothing opens them. Named, so they are not
/// counted as zero by accident.
const STAGE_TWO_REFUSALS: [&str; 2] = ["g13_ac1009", "g13_nan_bulge"];

const STAGE_TWO_DIR: &str = "ROUNDTRIP_STAGE_TWO_DIR";

#[derive(serde::Deserialize)]
struct IndexRow {
    id: String,
    writer: String,
    source: String,
    container: String,
    version: String,
    ext: String,
    #[serde(default)]
    sha256: String,
    #[serde(default)]
    write_failed: Option<String>,
    #[serde(default)]
    no_reference: Option<String>,
}

#[derive(serde::Deserialize)]
struct Index {
    #[serde(default)]
    output: Vec<IndexRow>,
}

fn stage_two_verdicts_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("parity")
        .join("VERDICTS_STAGE2.toml")
}

/// The scratch directory's outputs, as stage one's rows are.
fn stage_two_outputs(dir: &Path) -> Result<Vec<RtOutput>, String> {
    use sha2::{Digest, Sha256};
    let text = std::fs::read_to_string(dir.join("index.toml"))
        .map_err(|e| format!("{}: {e}", dir.join("index.toml").display()))?;
    let index: Index = toml::from_str(&text).map_err(|e| format!("index.toml: {e}"))?;
    let mut v = Vec::new();
    for row in index.output {
        let bytes: Result<Vec<u8>, String> = match &row.write_failed {
            Some(why) => Err(format!("the writer failed: {why}")),
            None => {
                let path = dir.join("files").join(format!("{}.{}", row.id, row.ext));
                let b = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
                let got = format!("{:x}", Sha256::digest(&b));
                if got != row.sha256 {
                    return Err(format!("{}: {got} on disk, the index froze {}", row.id, row.sha256));
                }
                Ok(b)
            }
        };
        let oracle = match (&row.write_failed, &row.no_reference) {
            (Some(_), _) => Err("the writer produced nothing".to_string()),
            (None, Some(why)) => Err(format!("the oracle cannot read it: {why}")),
            (None, None) => {
                let path = dir.join("read").join(format!("{}.reference.jsonl", row.id));
                let t = std::fs::read_to_string(&path)
                    .map_err(|e| format!("{}: {e}", path.display()))?;
                oracle_written_side(&row.id, &t)
            }
        };
        let ezdxf = if row.ext == "dxf" && row.write_failed.is_none() {
            let base = dir.join("ezdxf");
            let err = base.join(format!("{}.ezdxf.error.txt", row.id));
            Some(if let Ok(why) = std::fs::read_to_string(&err) {
                Err(format!("ezdxf would not read it: {}", why.trim()))
            } else {
                let j = base.join(format!("{}.ezdxf.jsonl", row.id));
                let sj = base.join(format!("{}.ezdxf.src.jsonl", row.id));
                let t = std::fs::read_to_string(&j).map_err(|e| format!("{}: {e}", j.display()))?;
                let st =
                    std::fs::read_to_string(&sj).map_err(|e| format!("{}: {e}", sj.display()))?;
                match parse_stream(&t) {
                    Ok(records) => {
                        let sidecar = Sidecar::parse(&st, &records)
                            .map_err(|e| format!("{}: {e}", sj.display()))?;
                        Ok(Side {
                            label: format!("ezdxf-written:{}", row.id),
                            stream: Stream {
                                records,
                                sidecar: Some(sidecar),
                            },
                        })
                    }
                    Err(e) => Err(format!("ezdxf's reading is not schema 1: {e}")),
                }
            })
        } else {
            None
        };
        v.push(RtOutput {
            id: row.id,
            writer: row.writer,
            source: row.source,
            container: row.container,
            version: row.version,
            bytes,
            oracle,
            ezdxf,
        });
    }
    Ok(v)
}

#[test]
#[ignore = "the nightly: tools/run-tests.sh --stage-two <dir> (libviprs/acadsharp-rs-tests#41)"]
fn roundtrip_stage_two() {
    let dir = std::env::var(STAGE_TWO_DIR).unwrap_or_else(|_| {
        panic!(
            "{STAGE_TWO_DIR} is not set. This is the nightly's test: tools/stage_two.py makes \
             the directory and tools/run-tests.sh --stage-two <dir> runs it."
        )
    });
    let dir = PathBuf::from(dir);
    let outputs = stage_two_outputs(&dir).unwrap_or_else(|e| panic!("{e}"));

    // The sources, and how many cells each should make, written out.
    let mut want: BTreeMap<&str, usize> = BTreeMap::new();
    for (s, n) in STAGE_TWO {
        assert!(want.insert(s, n).is_none(), "{s} is listed twice");
    }
    assert_eq!(STAGE_TWO.iter().map(|(_, n)| n).sum::<usize>(), 9216);
    for r in STAGE_TWO_REFUSALS {
        assert!(
            fixtures::refusals().iter().any(|f| f.id == r),
            "{r} is named as a refusal fixture and is not one"
        );
        assert!(!want.contains_key(r), "{r} is a refusal fixture: it has no cells");
    }
    let planned: Vec<String> = acadsharp_rs_tests::written::outputs("two")
        .iter()
        .map(|o| o.id())
        .collect();
    let have: std::collections::BTreeSet<&str> = outputs.iter().map(|o| o.id.as_str()).collect();
    for id in &planned {
        assert!(have.contains(id.as_str()), "{id} is planned and the directory has no row");
    }
    assert_eq!(outputs.len(), planned.len(), "the directory has outputs nobody planned");

    let tables = tables::load_all().unwrap();
    let out = out_dir().join("stage_two");
    std::fs::create_dir_all(&out).unwrap();
    let sharp = env!("CARGO_BIN_EXE_emit-acadsharp");
    let rust = env!("CARGO_BIN_EXE_emit-acadrust");

    // Each source, read by the two live readers and by the oracle's frozen output.
    let mut oracles: BTreeMap<String, Side> = BTreeMap::new();
    let mut held: BTreeMap<(&'static str, String), Emitted> = BTreeMap::new();
    for (id, _) in STAGE_TWO {
        let f = fixtures::by_id(id).unwrap_or_else(|| panic!("{id} is not a fixture"));
        oracles.insert(id.to_string(), frozen(f).unwrap_or_else(|e| panic!("{e}")));
        held.insert(("acadsharp", id.to_string()), emit(sharp, "acadsharp", id, &out).unwrap());
        held.insert(("acadrust", id.to_string()), emit(rust, "acadrust", id, &out).unwrap());
    }

    let mut computed = Computed::new();
    judge_outputs(&mut computed, &tables, &out, sharp, rust, &oracles, &held, &outputs)
        .unwrap_or_else(|e| panic!("{e}"));

    // Per source, the hand-written count.
    for (source, cells) in STAGE_TWO {
        let made = computed
            .keys()
            .filter(|(fx, _)| {
                outputs.iter().any(|o| &o.id == fx && o.source == source)
            })
            .count();
        assert_eq!(made, cells, "{source}: {made} cells, expected {cells}");
    }
    assert_eq!(computed.len(), 9216);

    let regen = std::env::var("PARITY_REGEN").as_deref() == Ok("1");
    let path = stage_two_verdicts_path();
    let committed = match std::fs::read_to_string(&path) {
        Ok(t) => Table::parse(&t).unwrap_or_else(|e| panic!("{}: {e}", path.display())),
        Err(_) if regen => Table::default(),
        Err(e) => panic!("{}: {e}", path.display()),
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
                    kept.map(|r| r.reason.clone()).unwrap_or_else(|| why.clone())
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
        std::fs::write(&path, table.render()).unwrap();
        eprintln!("rewrote {}", path.display());
        return;
    }
    let actual: BTreeMap<(String, String), (String, String)> = computed
        .iter()
        .map(|(k, (v, f, _))| (k.clone(), (v.clone(), f.clone())))
        .collect();
    let moved = committed.moved(&actual);
    assert!(
        moved.is_empty(),
        "{} verdict(s) moved against {}:\n  {}",
        moved.len(),
        path.display(),
        moved.iter().take(50).cloned().collect::<Vec<_>>().join("\n  ")
    );
    let refusals = committed.refusals();
    assert!(
        refusals.is_empty(),
        "{}:\n  {}",
        path.display(),
        refusals.iter().take(50).cloned().collect::<Vec<_>>().join("\n  ")
    );
}
