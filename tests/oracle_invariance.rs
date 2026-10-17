//! Cross-version invariance of the oracle's frozen outputs (R3.4).
//!
//! The seven saves are one drawing saved seven times. The oracle's output has
//! to be the same across them except where the format cannot carry it, and this
//! is the list of where that is, with a reason and an issue per row. The
//! numbers below were measured on the frozen outputs and the issue quotes them:
//! 296 non-warning records in every save, and against AC1032, AC1014 differs in
//! 70 records, AC1015 in 8, AC1018 in 3, AC1021 and AC1024 in 2, AC1027 in 1.

use std::collections::{BTreeMap, BTreeSet};

use acadsharp_rs_tests::canonical::{Record, parse_stream};
use acadsharp_rs_tests::compare::{
    ClassTable, Engine, Exclusion, Finding, Pair, Report, Scope, Stream, Val, WarningRow, compare,
};
use acadsharp_rs_tests::fixtures;
use acadsharp_rs_tests::invariance::{ExceptionList, Kind, ListError};

const BASELINE: &str = "acadsharp-ac1032";
const OTHERS: [&str; 6] = [
    "acadsharp-ac1014",
    "acadsharp-ac1015",
    "acadsharp-ac1018",
    "acadsharp-ac1021",
    "acadsharp-ac1024",
    "acadsharp-ac1027",
];

fn oracle(id: &str) -> Stream {
    let bytes = fixtures::by_id(id).unwrap().load_reference_jsonl().unwrap();
    Stream::new(parse_stream(&String::from_utf8(bytes).unwrap()).unwrap())
}

/// The oracle against itself. The document's version and maintenance number are
/// different across saves by definition, and the warning stream is a different
/// kind of finding (it is a count of things that went wrong, compared in the
/// parity run), so both are excluded with their reasons.
fn table() -> ClassTable {
    let by_definition = "a save's version and maintenance number are what make it a different save";
    let mut t = ClassTable::for_pair(
        Pair::new(Engine::Oracle, Engine::Oracle),
        WarningRow::Excluded {
            reason: "warnings are compared by the parity run, as counts per category".into(),
            issue: "libviprs/acadsharp-rs-tests#25".into(),
        },
    );
    for field in ["acad_version", "maintenance_version"] {
        t.excluded.push(Exclusion {
            scope: Scope::Field { record: "document".into(), field: field.into() },
            reason: by_definition.into(),
            issue: "libviprs/acadsharp-rs-tests#24".into(),
        });
    }
    t
}

fn against_baseline(save: &str) -> Report {
    compare(save, &oracle(BASELINE), &oracle(save), &table()).expect("the table is valid")
}

fn records_that_differ(report: &Report) -> usize {
    report
        .red()
        .iter()
        .filter_map(|f| f.index_a.or(f.index_b))
        .collect::<BTreeSet<_>>()
        .len()
}

#[test]
fn every_save_has_296_non_warning_records() {
    for id in std::iter::once(BASELINE).chain(OTHERS) {
        let n = oracle(id)
            .records
            .iter()
            .filter(|r| !matches!(r, Record::Warning(_) | Record::Document(_)))
            .count();
        assert_eq!(n, 296, "{id}");
    }
}

#[test]
fn the_saves_differ_from_ac1032_in_the_numbers_the_issue_measured() {
    let want = BTreeMap::from([
        ("acadsharp-ac1014", 70),
        ("acadsharp-ac1015", 8),
        ("acadsharp-ac1018", 3),
        ("acadsharp-ac1021", 2),
        ("acadsharp-ac1024", 2),
        ("acadsharp-ac1027", 1),
    ]);
    for (save, n) in want {
        assert_eq!(records_that_differ(&against_baseline(save)), n, "{save}");
    }
}

#[test]
fn every_difference_is_on_the_list_and_every_row_is_still_a_difference() {
    let list = ExceptionList::oracle().expect("parity/oracle_invariance.toml loads");
    assert_eq!(list.baseline, BASELINE);
    let mut unexplained = Vec::new();
    let mut hit = BTreeSet::new();
    for save in OTHERS {
        for f in against_baseline(save).red() {
            match list.row_for(save, f) {
                Some(row) => {
                    hit.insert((row.fixture.clone(), row.record, row.field.clone()));
                }
                None => unexplained.push(f.render()),
            }
        }
    }
    assert!(
        unexplained.is_empty(),
        "{} differences between saves have no row in parity/oracle_invariance.toml:\n{}",
        unexplained.len(),
        unexplained.join("\n")
    );
    let stale: Vec<_> = list
        .rows
        .iter()
        .filter(|r| !hit.contains(&(r.fixture.clone(), r.record, r.field.clone())))
        .map(|r| format!("{} record {} {}.{}", r.fixture, r.record, r.record_type, r.field))
        .collect();
    assert!(stale.is_empty(), "rows for differences that are gone: {stale:?}");
}

#[test]
fn a_row_with_no_reason_is_red() {
    let row = |reason: &str, issue: &str| {
        format!(
            r#"baseline = "acadsharp-ac1032"
[[exception]]
fixture = "acadsharp-ac1014"
record = 42
record_type = "text"
field = "style"
a = "Standard"
b = "STANDARD"
reason = "{reason}"
issue = "{issue}"
"#
        )
    };
    assert!(ExceptionList::parse(&row("R14 upper-cases table names", "libviprs/acadsharp-rs-tests#24")).is_ok());
    assert!(matches!(
        ExceptionList::parse(&row("", "libviprs/acadsharp-rs-tests#24")),
        Err(ListError::NoReason(_))
    ));
    assert!(matches!(
        ExceptionList::parse(&row("R14 upper-cases table names", "")),
        Err(ListError::NoIssue(_))
    ));
    assert!(matches!(
        ExceptionList::parse(&row("R14 upper-cases table names", "later")),
        Err(ListError::NoIssue(_))
    ));
}

#[test]
fn the_two_reader_bugs_are_pinned_as_failures_and_not_excused() {
    let list = ExceptionList::oracle().unwrap();
    let failures: Vec<_> = list.rows.iter().filter(|r| r.kind == Kind::Failure).collect();
    assert!(!failures.is_empty());
    let issues: BTreeSet<&str> = failures.iter().map(|r| r.issue.as_str()).collect();
    // The multiline ATTRIB only AC1032 loses (libviprs-dep#112), and the spline
    // that AC1027 and AC1032 read as fit points alone.
    assert!(issues.contains("libviprs/libviprs-dep#112"), "{issues:?}");
    assert_eq!(issues.len(), 2, "{issues:?}");
    // Failures are never exceptions: the same rows would otherwise read as
    // something the format cannot carry.
    for r in &failures {
        assert!(
            r.reason.to_lowercase().contains("bug") || r.reason.to_lowercase().contains("wrong"),
            "a failure row says what is wrong: {}",
            r.reason
        );
    }
}

/// An exception that names a ground truth is only as good as the reading of
/// AutoCAD's own DXF it points at: the save's DXF has to hold the value the save
/// holds, which is the proof that the format carries it.
#[test]
fn an_exception_that_names_a_dxf_twin_is_backed_by_what_the_twin_holds() {
    let list = ExceptionList::oracle().unwrap();
    let mut checked = 0;
    for row in list.rows.iter().filter(|r| r.ground_truth.is_some()) {
        let input = row.ground_truth.as_deref().unwrap();
        let gt = fixtures::ground_truth(input).unwrap_or_else(|| panic!("no ground truth {input}"));
        let records =
            parse_stream(&String::from_utf8(gt.load_jsonl().unwrap()).unwrap()).unwrap();
        let holds = records.iter().any(|r| match (r, row.record_type.as_str(), row.field.as_str(), &row.b) {
            (Record::Text(t), "text", "text", Val::Str(v)) => &t.text == v,
            (Record::Text(t), "text", "style", Val::Str(v)) => &t.style == v,
            (Record::Spline(s), "spline", "control_points.len", Val::Int(n)) => s.control_points.len() as u64 == *n,
            (Record::Spline(s), "spline", "knots.len", Val::Int(n)) => s.knots.len() as u64 == *n,
            (Record::Spline(s), "spline", "fit_points.len", Val::Int(n)) => s.fit_points.len() as u64 == *n,
            (Record::Text(t), "text", "valign", Val::Str(v)) => &t.valign == v,
            _ => false,
        });
        assert!(holds, "{input} does not hold {:?} for {}.{} ({})", row.b, row.record_type, row.field, row.fixture);
        checked += 1;
    }
    assert!(checked > 10, "only {checked} rows name a ground truth");
}

fn toml_str(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn toml_val(v: &Val) -> String {
    match v {
        Val::Str(s) => toml_str(s),
        Val::Int(i) => i.to_string(),
        Val::Bool(b) => b.to_string(),
        Val::Float(x) => format!("{x:?}"),
    }
}

/// How each difference is classified, by rule. Printed, not asserted: the list
/// is committed, and this is how it was first written down.
fn classify(save: &str, f: &Finding) -> (Kind, String, String, Option<String>) {
    let twin = |v: &str| Some(format!("dxf-sample-ac{v}-ascii"));
    let ver = save.trim_start_matches("acadsharp-ac");
    let b = f.b.as_ref().map(ToString::to_string).unwrap_or_default();
    match (f.record_type.as_str(), f.field.as_str()) {
        ("text", "style") => (
            Kind::Exception,
            "AC1014 stores table names upper-cased, so the style of a text reads STANDARD where later saves read Standard. The format cannot carry the case. AC1014 has no DXF twin, so this one is argued from the format and not shown.".into(),
            "libviprs/acadsharp-rs-tests#24".into(),
            None,
        ),
        ("text", "text") if f.record_type == "text" && b.contains("\\\\U+") => (
            Kind::Exception,
            "Saves before 2007 keep text in the code page's MIF form, and the character the code page cannot hold stays an escape (\\U+2205). AutoCAD's own DXF of this save holds the same escape, so it is in the file, and AutoCAD resolved it only when it converted to R2007 and later.".into(),
            "libviprs/libviprs-dep#111".into(),
            if ver == "1014" { None } else { twin(ver) },
        ),
        ("text", "text") if b.contains("\\\\P") => (
            Kind::Exception,
            "Saves before 2007 store the paragraph break of an MTEXT as the escape \\P and later saves store a line feed. The schema carries what the drawing stores, and AutoCAD's own DXF of this save holds the escape.".into(),
            "libviprs/acadsharp-rs-tests#24".into(),
            if ver == "1014" { None } else { twin(ver) },
        ),
        ("text", "text") if b.contains("\\\\S1/4;") => (
            Kind::Exception,
            "The R2000 save stores the stacked fraction as \\S1/4; where the later saves keep the formatted run. AutoCAD's own DXF of this save holds \\S1/4; too.".into(),
            "libviprs/acadsharp-rs-tests#24".into(),
            if ver == "1014" { None } else { twin(ver) },
        ),
        ("spline", _) => (
            Kind::Failure,
            "A reader bug: the oracle reads this spline as two fit points and no control points in the AC1027 and AC1032 saves, and as four control points and eight knots in the older ones. AutoCAD's own DXF of every one of the six saves with a twin holds four control points, eight knots and no fit points, so the older reading is the right one and the AC1032 baseline is wrong.".into(),
            "libviprs/libviprs-dep#150".into(),
            twin(if ver == "1014" { "1015" } else { ver }),
        ),
        _ => (
            Kind::Failure,
            "A reader bug: the multiline block attribute (handle 79D) is dropped by the oracle at AC1032, the only save that stores its text in an embedded MTEXT it does not read. The position and the vertical alignment follow from the same record. AutoCAD's own DXF of the AC1032 save holds the text.".into(),
            "libviprs/libviprs-dep#112".into(),
            None,
        ),
    }
}

#[test]
#[ignore = "prints the list"]
fn print_the_list() {
    let mut exceptions = String::new();
    let mut failures = String::new();
    for save in OTHERS {
        for f in against_baseline(save).red() {
            let (kind, reason, issue, truth) = classify(save, f);
            let mut row = format!(
                "[[{}]]\nfixture = {}\nrecord = {}\nrecord_type = {}\nfield = {}\na = {}\nb = {}\nreason = {}\nissue = {}\n",
                if kind == Kind::Exception { "exception" } else { "failure" },
                toml_str(save),
                f.index_a.unwrap(),
                toml_str(&f.record_type),
                toml_str(&f.field),
                toml_val(f.a.as_ref().unwrap()),
                toml_val(f.b.as_ref().unwrap()),
                toml_str(&reason),
                toml_str(&issue),
            );
            if let Some(t) = truth {
                row.push_str(&format!("ground_truth = {}\n", toml_str(&t)));
            }
            row.push('\n');
            if kind == Kind::Exception { exceptions.push_str(&row) } else { failures.push_str(&row) }
        }
    }
    println!("=====BEGIN=====\n{exceptions}{failures}=====END=====");
}
