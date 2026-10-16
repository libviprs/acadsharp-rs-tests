//! `src/canonical.rs`: the strict schema 1 reader (R2.1, #19).
//!
//! The parser's job is to refuse. Each refusal here is its own
//! [`ParseErrorKind`] with the line and, where there is one, the field named, so
//! an emitter that writes something the schema forbids is told what and where
//! instead of being tidied. A refusal that only ever says "invalid" is the one
//! thing the issue rules out.

mod support;

use std::collections::BTreeSet;

use acadsharp_rs_tests::canonical::{
    ParseError, ParseErrorKind, Record, Sidecar, SidecarErrorKind, parse_line, parse_stream,
};
use acadsharp_rs_tests::fixtures;
use support::*;

fn refuse(text: &str) -> ParseError {
    parse_stream(text).expect_err(&format!("the parser accepted {text:?}"))
}

fn refuse_line(json: &str) -> ParseError {
    refuse(&format!("{json}\n"))
}

#[test]
fn every_record_type_parses_into_its_typed_form() {
    let kinds: Vec<&str> = [
        DOCUMENT,
        VIEW_MODEL,
        LINE,
        POLYLINE,
        CIRCLE,
        ARC,
        ELLIPSE,
        SPLINE,
        POLYGON,
        POINT,
        TEXT,
        WARN_A,
    ]
    .iter()
    .map(|json| parse_line(json, 1).expect("parses").kind())
    .collect();
    assert_eq!(
        kinds,
        [
            "document", "view", "line", "polyline", "circle", "arc", "ellipse", "spline",
            "polygon", "point", "text", "warning"
        ]
    );

    match parse_line(CIRCLE, 1).unwrap() {
        Record::Circle(c) => {
            assert_eq!(c.view, 0);
            assert_eq!(c.center, [10.0, 20.0, 0.0]);
            assert_eq!(c.radius, 5.0);
            assert_eq!(c.normal, [0.0, 0.0, 1.0]);
        }
        other => panic!("a circle line parsed as {other:?}"),
    }
    match parse_line(POLYLINE, 1).unwrap() {
        Record::Polyline(p) => {
            assert!(!p.closed);
            assert_eq!(p.points.len(), 3);
            assert_eq!(p.bulges, [0.0, 0.5, 0.0]);
        }
        other => panic!("a polyline line parsed as {other:?}"),
    }
    match parse_line(TEXT, 1).unwrap() {
        Record::Text(t) => {
            assert_eq!(t.text, "ROOM 101");
            assert_eq!(t.at, [20.0, 30.0, 0.0]);
            assert_eq!(t.style, "Standard");
        }
        other => panic!("a text line parsed as {other:?}"),
    }
    match parse_line(DOCUMENT, 1).unwrap() {
        Record::Document(d) => assert_eq!(d.maintenance_version, 228),
        other => panic!("a document line parsed as {other:?}"),
    }
}

#[test]
fn number_spelling_is_not_part_of_the_value() {
    // The oracle's `R` formatting can write 1E-05 where a Rust emitter writes
    // 1e-5. Both are one double, and the parser must not care which it got.
    let a = parse_line(&LINE.replace("\"x2\":100.0", "\"x2\":1E-05"), 1).unwrap();
    let b = parse_line(&LINE.replace("\"x2\":100.0", "\"x2\":1e-5"), 1).unwrap();
    assert_eq!(a, b);
}

#[test]
fn an_unknown_record_type_is_refused_by_name() {
    let e = refuse_line(r#"{"schema":1,"record":"blob","view":0}"#);
    assert_eq!(e.kind, ParseErrorKind::UnknownRecord("blob".into()));
    assert_eq!(e.line, 1);
}

#[test]
fn an_unknown_field_is_refused_by_name() {
    let line = CIRCLE.replace("\"radius\":5.0", "\"radius\":5.0,\"bogus\":1.0");
    let e = refuse_line(&line);
    assert_eq!(e.kind, ParseErrorKind::UnknownField);
    assert_eq!(e.field.as_deref(), Some("bogus"));
}

#[test]
fn a_missing_field_is_refused_by_name() {
    let line = CIRCLE.replace("\"radius\":5.0,", "");
    let e = refuse_line(&line);
    assert_eq!(e.kind, ParseErrorKind::MissingField);
    assert_eq!(e.field.as_deref(), Some("radius"));
}

#[test]
fn an_integer_token_where_a_double_belongs_is_refused() {
    let e = refuse_line(&CIRCLE.replace("\"radius\":5.0", "\"radius\":100"));
    assert!(
        matches!(e.kind, ParseErrorKind::WrongType { .. }),
        "100 in a double field: {e}"
    );
    assert_eq!(e.field.as_deref(), Some("radius"));
}

#[test]
fn a_double_token_where_an_integer_belongs_is_refused() {
    let e = refuse_line(&CIRCLE.replace("\"view\":0", "\"view\":0.0"));
    assert!(
        matches!(e.kind, ParseErrorKind::WrongType { .. }),
        "0.0 in an integer field: {e}"
    );
    assert_eq!(e.field.as_deref(), Some("view"));
}

#[test]
fn a_string_where_a_number_belongs_is_refused() {
    let e = refuse_line(&CIRCLE.replace("\"radius\":5.0", "\"radius\":\"5.0\""));
    assert!(matches!(e.kind, ParseErrorKind::WrongType { .. }), "{e}");
    assert_eq!(e.field.as_deref(), Some("radius"));
}

#[test]
fn an_integer_inside_a_point_array_is_refused_with_the_element_named() {
    let e = refuse_line(&POLYGON.replace("[1.0,0.0,0.0]", "[1,0.0,0.0]"));
    assert!(matches!(e.kind, ParseErrorKind::WrongType { .. }), "{e}");
    assert_eq!(e.field.as_deref(), Some("points[1][0]"));
}

#[test]
fn a_carriage_return_is_refused_whether_raw_or_escaped() {
    // Raw: the stream was written with CRLF.
    let raw = format!("{DOCUMENT}\r\n");
    assert_eq!(refuse(&raw).kind, ParseErrorKind::CarriageReturn);

    // Escaped: a text string carrying a CR. Schema 1 normalises line endings to
    // LF inside CAD text, so an emitter that did not is refused.
    let escaped = TEXT.replace("ROOM 101", "ROOM\\r101");
    let e = refuse_line(&escaped);
    assert_eq!(e.kind, ParseErrorKind::CarriageReturn);
    assert_eq!(e.field.as_deref(), Some("text"));
}

#[test]
fn negative_zero_is_refused_with_the_field_named() {
    let e = refuse_line(&LINE.replace("\"z1\":0.0", "\"z1\":-0.0"));
    assert_eq!(e.kind, ParseErrorKind::NegativeZero);
    assert_eq!(e.field.as_deref(), Some("z1"));

    let e = refuse_line(&POLYGON.replace("[1.0,1.0,0.0]", "[1.0,-0.0,0.0]"));
    assert_eq!(e.kind, ParseErrorKind::NegativeZero);
    assert_eq!(e.field.as_deref(), Some("points[2][1]"));
}

#[test]
fn a_number_too_large_for_a_double_is_refused_not_passed_through() {
    let e = refuse_line(&CIRCLE.replace("\"radius\":5.0", "\"radius\":1e999"));
    assert!(matches!(e.kind, ParseErrorKind::NonFinite(_)), "{e}");
    assert_eq!(e.field.as_deref(), Some("radius"));
}

#[test]
fn nan_and_the_infinities_are_refused_with_the_field_named() {
    for token in ["NaN", "Infinity", "-Infinity"] {
        let e = refuse_line(&CIRCLE.replace("\"radius\":5.0", &format!("\"radius\":{token}")));
        assert!(
            matches!(e.kind, ParseErrorKind::NonFinite(_)),
            "{token}: {e}"
        );
        assert_eq!(e.field.as_deref(), Some("radius"), "{token}: {e}");
    }
}

#[test]
fn a_schema_other_than_one_is_refused() {
    let e = refuse_line(&CIRCLE.replace("\"schema\":1", "\"schema\":2"));
    assert_eq!(e.kind, ParseErrorKind::SchemaVersion("2".into()));
}

#[test]
fn a_record_without_a_schema_field_is_refused() {
    let e = refuse_line(&CIRCLE.replace("\"schema\":1,", ""));
    assert_eq!(e.kind, ParseErrorKind::MissingField);
    assert_eq!(e.field.as_deref(), Some("schema"));
}

#[test]
fn points_and_bulges_of_unequal_length_are_refused() {
    let e = refuse_line(&POLYLINE.replace("\"bulges\":[0.0,0.5,0.0]", "\"bulges\":[0.0,0.5]"));
    assert_eq!(
        e.kind,
        ParseErrorKind::LengthMismatch {
            points: 3,
            bulges: 2
        }
    );
}

#[test]
fn a_blank_line_a_missing_final_newline_and_a_non_object_are_each_refused() {
    assert_eq!(
        refuse(&format!("{DOCUMENT}\n\n{VIEW_MODEL}\n")).kind,
        ParseErrorKind::EmptyLine
    );
    assert_eq!(
        refuse(&format!("{DOCUMENT}\n{VIEW_MODEL}")).kind,
        ParseErrorKind::MissingFinalNewline
    );
    assert_eq!(refuse_line("[1,2]").kind, ParseErrorKind::NotAnObject);
    assert!(matches!(
        refuse_line("not json").kind,
        ParseErrorKind::NotJson(_)
    ));
}

#[test]
fn the_error_names_the_line_it_is_about() {
    let text = format!("{DOCUMENT}\n{VIEW_MODEL}\n{}\n", CIRCLE.replace("5.0", "5"));
    let e = refuse(&text);
    assert_eq!(e.line, 3);
    let shown = e.to_string();
    assert!(shown.contains("line 3"), "{shown}");
    assert!(shown.contains("radius"), "{shown}");
}

#[test]
fn every_refusal_is_a_different_error() {
    // The issue asks for "each with its own error". Collecting the variants and
    // asking for as many distinct ones as there were refusals is how a parser
    // that funnels everything into one generic error gets caught.
    let refusals: Vec<ParseError> = vec![
        refuse_line(r#"{"schema":1,"record":"blob"}"#),
        refuse_line(&CIRCLE.replace("5.0", "5.0,\"bogus\":1.0")),
        refuse_line(&CIRCLE.replace("\"radius\":5.0,", "")),
        refuse_line(&CIRCLE.replace("\"radius\":5.0", "\"radius\":5")),
        refuse(&format!("{DOCUMENT}\r\n")),
        refuse_line(&LINE.replace("\"z1\":0.0", "\"z1\":-0.0")),
        refuse_line(&CIRCLE.replace("5.0", "1e999")),
        refuse_line(&CIRCLE.replace("\"schema\":1", "\"schema\":2")),
        refuse_line(&POLYLINE.replace("[0.0,0.5,0.0]", "[0.0]")),
        refuse(&format!("{DOCUMENT}\n\n")),
        refuse(DOCUMENT),
        refuse_line("[]"),
        refuse_line("nope"),
    ];
    let distinct: BTreeSet<_> = refusals
        .iter()
        .map(|e| std::mem::discriminant(&e.kind))
        .map(|d| format!("{d:?}"))
        .collect();
    assert_eq!(
        distinct.len(),
        refusals.len(),
        "{} refusals but only {} different error kinds: {refusals:#?}",
        refusals.len(),
        distinct.len()
    );
}

#[test]
fn every_frozen_stream_parses_and_matches_its_summary() {
    // Positive control: the strict parser has to accept what the oracle wrote,
    // all of it, or it is refusing the very thing it exists to read.
    let mut parsed = 0usize;
    for fixture in fixtures::all() {
        let bytes = fixture.load_reference_jsonl().expect("frozen output");
        let text = String::from_utf8(bytes).expect("utf-8");
        let records = parse_stream(&text).unwrap_or_else(|e| panic!("{}: {e}", fixture.id));
        assert_eq!(
            records.len(),
            text.lines().count(),
            "{}: one record per line",
            fixture.id
        );
        assert!(
            matches!(&records[0], Record::Document(d) if d.acad_version == fixture.acad_version),
            "{}: opens with its document record",
            fixture.id
        );
        parsed += 1;
    }
    assert_eq!(parsed, 7, "the corpus holds seven AutoCAD saves today");
}

fn three_records() -> Vec<Record> {
    parse(&[DOCUMENT.to_string(), LINE.to_string(), CIRCLE.to_string()])
}

fn good_sidecar() -> String {
    format!(
        "{{\"i\":0,\"h\":[\"1A0\"],\"t\":\"document\"}}\n{{\"i\":1,\"h\":[\"2B\",\"1A1\"],\"t\":\"line\"}}\n{{\"i\":2,\"h\":[\"1A2\"],\"t\":\"circle\"}}\n"
    )
}

#[test]
fn a_well_formed_sidecar_names_its_entities() {
    let records = three_records();
    let sidecar = Sidecar::parse(&good_sidecar(), &records).expect("parses");
    assert_eq!(sidecar.entries.len(), 3);
    assert_eq!(sidecar.handles(1).unwrap(), ["2B", "1A1"]);
}

#[test]
fn a_gap_in_the_sidecar_index_is_refused_by_name() {
    let text = good_sidecar().replace("\"i\":2", "\"i\":3");
    let e = Sidecar::parse(&text, &three_records()).expect_err("gap");
    assert_eq!(
        e.kind,
        SidecarErrorKind::IndexGap {
            expected: 2,
            found: 3
        }
    );
    assert_eq!(e.line, 3);
}

#[test]
fn a_sidecar_with_the_wrong_line_count_is_refused_by_name() {
    let short: String = good_sidecar().lines().take(2).map(|l| format!("{l}\n")).collect();
    let e = Sidecar::parse(&short, &three_records()).expect_err("short");
    assert_eq!(
        e.kind,
        SidecarErrorKind::CountMismatch {
            lines: 2,
            records: 3
        }
    );
}

#[test]
fn a_sidecar_type_that_disagrees_with_the_record_is_refused_by_name() {
    let text = good_sidecar().replace("\"t\":\"circle\"", "\"t\":\"arc\"");
    let e = Sidecar::parse(&text, &three_records()).expect_err("disagrees");
    assert_eq!(
        e.kind,
        SidecarErrorKind::TypeDisagrees {
            sidecar: "arc".into(),
            record: "circle".into()
        }
    );
    assert_eq!(e.line, 3);
}

#[test]
fn a_sidecar_handle_must_be_uppercase_hex_without_a_prefix() {
    for bad in ["0x1A2", "1a2", "", "G12"] {
        let text = good_sidecar().replace("\"1A2\"", &format!("\"{bad}\""));
        let e = Sidecar::parse(&text, &three_records()).expect_err(bad);
        assert!(
            matches!(e.kind, SidecarErrorKind::BadHandle(_)),
            "{bad:?}: {e}"
        );
    }
}

#[test]
fn a_sidecar_with_an_unknown_field_is_refused() {
    let text = good_sidecar().replace("\"t\":\"line\"", "\"t\":\"line\",\"q\":1");
    let e = Sidecar::parse(&text, &three_records()).expect_err("unknown");
    assert_eq!(e.kind, SidecarErrorKind::UnknownField);
}

#[test]
fn a_partial_sidecar_without_a_type_is_accepted() {
    // acadsharp-rs's wire 2 carries one handle and a from-insert flag and no
    // type, so a missing `t` is not a disagreement.
    let text = "{\"i\":0,\"h\":[\"1\"],\"x\":false}\n{\"i\":1,\"h\":[\"2\"],\"x\":true}\n{\"i\":2,\"h\":[\"3\"],\"x\":false}\n";
    let sidecar = Sidecar::parse(text, &three_records()).expect("partial sidecar");
    assert_eq!(sidecar.entries[1].from_insert, Some(true));
    assert_eq!(sidecar.entries[1].record_type, None);
}
