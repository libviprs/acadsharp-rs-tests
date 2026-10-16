//! `src/canonical.rs`, the writer half (R3.2, #23).
//!
//! Both emitters write through this, so it is tested without either of them:
//! the schema's own examples come out byte for byte, every frozen stream the
//! oracle wrote survives a parse, a render and a parse, and what the schema
//! forbids is refused at the writer rather than left for the reader to find.

mod support;

use acadsharp_rs_tests::canonical::{
    Record, Sidecar, SidecarEntry, WriteError, format_real, parse_line, parse_stream, quote,
    render_record, render_sidecar, render_stream,
};
use acadsharp_rs_tests::fixtures;
use support::*;

#[test]
fn the_schemas_own_examples_come_out_byte_for_byte() {
    for example in [
        DOCUMENT, VIEW_MODEL, LINE, CIRCLE, ARC, POLYLINE, POINT, TEXT, SPLINE, POLYGON, ELLIPSE,
        WARN_A, WARN_B,
    ] {
        let record = parse_line(example, 1).expect("the example parses");
        assert_eq!(render_record(&record).expect("writes"), example);
    }
}

#[test]
fn a_double_is_shortest_round_trip_with_a_visible_fraction() {
    for (value, want) in [
        (0.0, "0.0"),
        (100.0, "100.0"),
        (-2.5, "-2.5"),
        (0.1, "0.1"),
        (1.0000000000000002, "1.0000000000000002"),
        (std::f64::consts::PI, "3.141592653589793"),
        (1.5707963267948966, "1.5707963267948966"),
    ] {
        assert_eq!(format_real(value).unwrap(), want);
    }
}

#[test]
fn negative_zero_is_written_as_zero() {
    assert_eq!(format_real(-0.0).unwrap(), "0.0");
    let mut record = parse_line(LINE, 1).unwrap();
    if let Record::Line(line) = &mut record {
        line.a[1] = -0.0;
    }
    let text = render_record(&record).unwrap();
    assert!(text.contains("\"y1\":0.0"), "{text}");
    assert!(!text.contains("-0.0"), "{text}");
    // And the strict reader takes what the writer wrote.
    assert!(parse_line(&text, 1).is_ok());
}

#[test]
fn exponent_forms_are_valid_doubles_to_the_strict_reader() {
    for value in [1e-5, 1e21, 5e-324, f64::MAX, 1.5e300, -3.25e-12] {
        let text = format_real(value).unwrap();
        assert!(
            text.contains('.') || text.contains('e'),
            "{text} would read as an integer"
        );
        let line = LINE.replace("\"x2\":100.0", &format!("\"x2\":{text}"));
        let parsed = parse_line(&line, 1).unwrap_or_else(|e| panic!("{text}: {e}"));
        match parsed {
            Record::Line(l) => assert_eq!(l.b[0], value, "{text}"),
            other => panic!("{other:?}"),
        }
    }
}

#[test]
fn nan_and_the_infinities_are_refused_naming_the_field() {
    for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert!(format_real(bad).is_err());
        let mut record = parse_line(CIRCLE, 1).unwrap();
        if let Record::Circle(c) = &mut record {
            c.radius = bad;
        }
        match render_record(&record) {
            Err(WriteError::NonFinite { field, .. }) => assert_eq!(field, "radius"),
            other => panic!("{bad}: {other:?}"),
        }
    }
}

#[test]
fn only_what_json_forbids_bare_is_escaped() {
    assert_eq!(quote("plain"), "\"plain\"");
    assert_eq!(quote("a\"b"), "\"a\\\"b\"");
    assert_eq!(quote("a\\b"), "\"a\\\\b\"");
    assert_eq!(quote("a\nb\tc\u{8}d\u{c}e\rf"), "\"a\\nb\\tc\\bd\\fe\\rf\"");
    assert_eq!(quote("\u{1}\u{1f}"), "\"\\u0001\\u001f\"");
    // Everything else, non-ASCII included, is UTF-8. A default JSON encoder
    // would escape these, which is why the schema hand-writes the policy.
    assert_eq!(quote("\u{c4}\u{d6}\u{dc}"), "\"\u{c4}\u{d6}\u{dc}\"");
    assert_eq!(quote("\u{2205}45,6 \u{1f600}"), "\"\u{2205}45,6 \u{1f600}\"");
    assert_eq!(quote("a/b\u{7f}"), "\"a/b\u{7f}\"");
}

#[test]
fn a_stream_is_one_record_per_line_lf_and_a_final_newline() {
    let records = parse(&base());
    let text = render_stream(&records).unwrap();
    assert!(text.ends_with('\n'));
    assert!(!text.contains('\r'));
    assert_eq!(text.lines().count(), records.len());
    assert!(text.lines().all(|l| !l.is_empty()));
    assert_eq!(parse_stream(&text).unwrap(), records);
}

#[test]
fn every_frozen_stream_survives_a_parse_a_render_and_a_parse() {
    let mut checked = 0usize;
    for fixture in fixtures::all() {
        let bytes = fixture.load_reference_jsonl().expect("frozen output");
        let original = parse_stream(&String::from_utf8(bytes).unwrap())
            .unwrap_or_else(|e| panic!("{}: {e}", fixture.id));
        let text = render_stream(&original).unwrap_or_else(|e| panic!("{}: {e}", fixture.id));
        let again = parse_stream(&text).unwrap_or_else(|e| panic!("{}: {e}", fixture.id));
        assert_eq!(again, original, "{}: the writer changed a value", fixture.id);
        // And the writer is a function of the records alone.
        assert_eq!(render_stream(&again).unwrap(), text, "{}", fixture.id);
        checked += 1;
    }
    assert_eq!(checked, fixtures::all().len());
    assert!(checked >= 70);
}

#[test]
fn a_sidecar_round_trips_through_the_strict_reader() {
    let records = parse(&base());
    let entries: Vec<SidecarEntry> = records
        .iter()
        .enumerate()
        .map(|(i, r)| SidecarEntry {
            index: i,
            handles: if i % 2 == 0 {
                vec![format!("{:X}", 0x100 + i)]
            } else {
                vec!["2B".to_string(), format!("{:X}", 0x100 + i)]
            },
            record_type: Some(r.kind().to_string()),
            from_insert: None,
            dxf_name: Some("LWPOLYLINE".to_string()),
        })
        .collect();
    let text = render_sidecar(&entries);
    assert!(text.ends_with('\n'));
    let parsed = Sidecar::parse(&text, &records).expect("the reader takes the writer's output");
    assert_eq!(parsed.entries, entries);
}
