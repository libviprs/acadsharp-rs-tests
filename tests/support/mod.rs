//! Hand-written canonical streams for the parser and comparator tests.
//!
//! Every test that wants "this stream, with exactly one thing different" starts
//! from [`base`] and calls [`set`], which refuses to run if the text it was
//! asked to replace is not there. A test whose mutation silently did nothing
//! would compare a stream with itself and pass, which is the failure this
//! exists to make loud.
#![allow(dead_code)]

use acadsharp_rs_tests::canonical::{self, Record};
use acadsharp_rs_tests::compare::{ClassTable, Engine, Pair, Report, Stream, WarningRow, compare};

pub const DOCUMENT: &str = r#"{"schema":1,"record":"document","format":"dwg","acad_version":"AC1032","maintenance_version":228}"#;
pub const VIEW_MODEL: &str =
    r#"{"schema":1,"record":"view","index":0,"name":"Model","kind":"model"}"#;
pub const VIEW_LAYOUT: &str =
    r#"{"schema":1,"record":"view","index":1,"name":"Layout1","kind":"layout"}"#;
pub const LINE: &str = r#"{"schema":1,"record":"line","view":0,"x1":0.0,"y1":0.0,"z1":0.0,"x2":100.0,"y2":0.0,"z2":0.0}"#;
pub const CIRCLE: &str = r#"{"schema":1,"record":"circle","view":0,"cx":10.0,"cy":20.0,"cz":0.0,"radius":5.0,"nx":0.0,"ny":0.0,"nz":1.0}"#;
pub const ARC: &str = r#"{"schema":1,"record":"arc","view":0,"cx":50.0,"cy":50.0,"cz":0.0,"radius":25.0,"start":0.5,"end":1.5,"nx":0.0,"ny":0.0,"nz":1.0}"#;
pub const POLYLINE: &str = r#"{"schema":1,"record":"polyline","view":0,"closed":false,"nx":0.0,"ny":0.0,"nz":1.0,"points":[[0.0,0.0,0.0],[1.0,0.0,0.0],[1.0,1.0,0.0]],"bulges":[0.0,0.5,0.0]}"#;
pub const POINT: &str = r#"{"schema":1,"record":"point","view":0,"x":1.0,"y":2.0,"z":0.0}"#;
pub const TEXT: &str = r#"{"schema":1,"record":"text","view":1,"x":20.0,"y":30.0,"z":0.0,"height":2.5,"rotation":0.0,"width_factor":1.0,"halign":"left","valign":"baseline","style":"Standard","text":"ROOM 101"}"#;
pub const SPLINE: &str = r#"{"schema":1,"record":"spline","view":1,"degree":3,"closed":false,"periodic":false,"control_points":[[0.0,0.0,0.0],[1.0,2.0,0.0],[2.0,2.0,0.0],[3.0,0.0,0.0]],"knots":[0.0,0.0,0.0,0.0,1.0,1.0,1.0,1.0],"weights":[],"fit_points":[]}"#;
pub const POLYGON: &str = r#"{"schema":1,"record":"polygon","view":1,"points":[[0.0,0.0,0.0],[1.0,0.0,0.0],[1.0,1.0,0.0]]}"#;
pub const ELLIPSE: &str = r#"{"schema":1,"record":"ellipse","view":0,"cx":0.0,"cy":0.0,"cz":0.0,"mx":4.0,"my":0.0,"mz":0.0,"ratio":0.25,"start":0.0,"end":3.0,"nx":0.0,"ny":0.0,"nz":1.0}"#;
pub const WARN_A: &str = r#"{"schema":1,"record":"warning","category":"unsupported_entity","entity_type":"MultiLeader","detail":"no_canonical_mapping"}"#;
pub const WARN_B: &str =
    r#"{"schema":1,"record":"warning","category":"missing_font","entity_type":"","detail":""}"#;

/// Twelve records, two views, one of each kind the matrix needs.
pub fn base() -> Vec<String> {
    [
        DOCUMENT,
        VIEW_MODEL,
        VIEW_LAYOUT,
        LINE,
        CIRCLE,
        ARC,
        POLYLINE,
        TEXT,
        SPLINE,
        POLYGON,
        WARN_A,
        WARN_B,
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}

/// Replace `from` with `to` inside line `index`, and insist `from` was there.
pub fn set(lines: &mut [String], index: usize, from: &str, to: &str) {
    assert!(
        lines[index].contains(from),
        "support::set: line {index} does not contain {from:?}:\n{}",
        lines[index]
    );
    lines[index] = lines[index].replacen(from, to, 1);
}

pub fn parse(lines: &[String]) -> Vec<Record> {
    let mut text = lines.join("\n");
    text.push('\n');
    canonical::parse_stream(&text).unwrap_or_else(|e| panic!("support::parse: {e}"))
}

pub fn stream(lines: &[String]) -> Stream {
    Stream::new(parse(lines))
}

pub const ORACLE_SHARP: Pair = Pair::new(Engine::Oracle, Engine::Acadsharp);
pub const ORACLE_RUST: Pair = Pair::new(Engine::Oracle, Engine::Acadrust);

pub fn tight() -> ClassTable {
    ClassTable::for_pair(ORACLE_SHARP, WarningRow::Compare)
}

pub fn loose() -> ClassTable {
    ClassTable::for_pair(ORACLE_RUST, WarningRow::Compare)
}

/// Compare two line lists under the tight table.
pub fn diff(a: &[String], b: &[String]) -> Report {
    compare("fixture", &stream(a), &stream(b), &tight()).expect("table is valid")
}

pub fn diff_loose(a: &[String], b: &[String]) -> Report {
    compare("fixture", &stream(a), &stream(b), &loose()).expect("table is valid")
}
