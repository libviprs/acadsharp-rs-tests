//! `src/compare.rs`: the comparator and its matrix (R2.1, #19; supersedes H1.4, #4).
//!
//! Every row of the matrix is its own test, on parsed records, over a stream
//! where the only difference is the named one. Each asserts exactly one finding
//! (or none, where the row is "inside tolerance"), because a comparator that
//! reports the difference and also three more is as wrong as one that misses it.

mod support;

use acadsharp_rs_tests::canonical::Sidecar;
use acadsharp_rs_tests::compare::{
    BOUNDED, BOUNDED_TIGHT, Class, ClassTable, Disposition, DualOracle, Engine, Exclusion,
    FieldKind, FindingKind, KnownDivergence, Pair, RecordKey, Scope, TableError, Val, WarningRow,
    compare,
};
use support::*;

fn only(report: &acadsharp_rs_tests::compare::Report) -> &acadsharp_rs_tests::compare::Finding {
    assert_eq!(
        report.findings.len(),
        1,
        "expected exactly one finding, got:\n{}",
        report
            .findings
            .iter()
            .map(|f| f.render())
            .collect::<Vec<_>>()
            .join("\n")
    );
    &report.findings[0]
}

fn extra_line() -> String {
    LINE.replace("\"x2\":100.0", "\"x2\":7.0")
}

// ---- identical inputs, and that the comparator looks at something -------------

#[test]
fn identical_streams_compare_equal_and_the_comparator_compared_something() {
    let report = diff(&base(), &base());
    assert!(report.findings.is_empty(), "{:?}", report.findings);
    assert!(report.is_match());
    assert_eq!(report.records, (12, 12));
    assert_eq!(report.aligned.len(), 12);
    // Positive control for every "no findings" test below. A comparator that
    // compared zero fields would pass the line above.
    assert!(report.scalars_compared > 100, "{}", report.scalars_compared);
    assert!(report.floats_compared > 40, "{}", report.floats_compared);
}

#[test]
fn number_spelling_is_never_compared() {
    let mut b = base();
    set(&mut b, 3, "\"x2\":100.0", "\"x2\":1E+2");
    set(&mut b, 4, "\"radius\":5.0", "\"radius\":0.5e1");
    assert!(diff(&base(), &b).findings.is_empty());
}

// ---- record count: start, middle, end -----------------------------------------

#[test]
fn an_extra_record_at_the_start_is_one_finding() {
    let mut b = base();
    b.insert(0, extra_line());
    let report = diff(&base(), &b);
    let f = only(&report);
    assert_eq!(f.kind, FindingKind::OnlyInB);
    assert_eq!(f.index_b, Some(0));
    assert_eq!(f.index_a, None);
    assert_eq!(f.record_type, "line");
    assert_eq!(report.records, (12, 13));
}

#[test]
fn an_extra_record_in_the_middle_is_one_finding() {
    let mut b = base();
    b.insert(6, extra_line());
    let report = diff(&base(), &b);
    let f = only(&report);
    assert_eq!(f.kind, FindingKind::OnlyInB);
    assert_eq!(f.index_b, Some(6));
}

#[test]
fn an_extra_record_at_the_end_is_one_finding() {
    let mut b = base();
    b.push(extra_line());
    let report = diff(&base(), &b);
    let f = only(&report);
    assert_eq!(f.kind, FindingKind::OnlyInB);
    assert_eq!(f.index_b, Some(12));
}

#[test]
fn a_missing_record_at_the_start_is_one_finding() {
    let mut b = base();
    b.remove(0);
    let report = diff(&base(), &b);
    let f = only(&report);
    assert_eq!(f.kind, FindingKind::OnlyInA);
    assert_eq!(f.index_a, Some(0));
    assert_eq!(f.record_type, "document");
}

#[test]
fn a_missing_record_in_the_middle_is_one_finding() {
    let mut b = base();
    b.remove(5);
    let report = diff(&base(), &b);
    let f = only(&report);
    assert_eq!(f.kind, FindingKind::OnlyInA);
    assert_eq!(f.index_a, Some(5));
    assert_eq!(f.record_type, "arc");
}

#[test]
fn a_missing_record_at_the_end_is_one_finding() {
    let mut b = base();
    b.pop();
    let report = diff(&base(), &b);
    let f = only(&report);
    assert_eq!(f.kind, FindingKind::OnlyInA);
    assert_eq!(f.index_a, Some(11));
}

// ---- record type, views --------------------------------------------------------

#[test]
fn a_different_record_type_at_one_index_is_one_finding() {
    let mut b = base();
    b[4] = POLYGON.replace("\"view\":1", "\"view\":0");
    let report = diff(&base(), &b);
    let f = only(&report);
    assert_eq!(f.kind, FindingKind::RecordTypeMismatch);
    assert_eq!(f.a, Some(Val::Str("circle".into())));
    assert_eq!(f.b, Some(Val::Str("polygon".into())));
    assert_eq!((f.index_a, f.index_b), (Some(4), Some(4)));
}

#[test]
fn a_different_view_name_is_one_finding() {
    let mut b = base();
    set(&mut b, 2, "Layout1", "Layout2");
    let report = diff(&base(), &b);
    let f = only(&report);
    assert_eq!((f.record_type.as_str(), f.field.as_str()), ("view", "name"));
    assert_eq!(f.class, Class::Exact);
}

#[test]
fn a_different_view_kind_is_one_finding() {
    let mut b = base();
    set(&mut b, 2, "\"kind\":\"layout\"", "\"kind\":\"unknown\"");
    let f_report = diff(&base(), &b);
    let f = only(&f_report);
    assert_eq!((f.record_type.as_str(), f.field.as_str()), ("view", "kind"));
}

#[test]
fn a_different_view_count_is_one_finding() {
    let mut b = base();
    b.remove(2);
    let report = diff(&base(), &b);
    let f = only(&report);
    assert_eq!(f.kind, FindingKind::OnlyInA);
    assert_eq!(f.record_type, "view");
}

// ---- one integer, one flag, one string ----------------------------------------

#[test]
fn one_integer_field_is_one_finding() {
    let mut b = base();
    set(
        &mut b,
        0,
        "\"maintenance_version\":228",
        "\"maintenance_version\":229",
    );
    let report = diff(&base(), &b);
    let f = only(&report);
    assert_eq!(f.field, "maintenance_version");
    assert_eq!(
        (f.a.clone(), f.b.clone()),
        (Some(Val::Int(228)), Some(Val::Int(229)))
    );
    assert_eq!(f.class, Class::Exact);
}

#[test]
fn one_flag_is_one_finding() {
    let mut b = base();
    set(&mut b, 6, "\"closed\":false", "\"closed\":true");
    let report = diff(&base(), &b);
    let f = only(&report);
    assert_eq!(
        (f.record_type.as_str(), f.field.as_str()),
        ("polyline", "closed")
    );
    assert_eq!(f.a, Some(Val::Bool(false)));
}

#[test]
fn one_string_is_one_finding() {
    let mut b = base();
    set(&mut b, 7, "ROOM 101", "ROOM 102");
    let report = diff(&base(), &b);
    assert_eq!(only(&report).field, "text");
}

#[test]
fn a_unicode_only_difference_is_a_finding() {
    // NFC against NFD. Same text to a reader, different bytes, and an
    // emitter that normalised on the way out is exactly what this catches.
    let mut a = base();
    let mut b = base();
    set(&mut a, 7, "ROOM 101", "CAF\u{e9}");
    set(&mut b, 7, "ROOM 101", "CAFE\u{301}");
    let report = diff(&a, &b);
    let f = only(&report);
    assert_eq!(f.field, "text");
    assert_eq!(f.class, Class::Exact);
}

#[test]
fn a_trailing_space_is_a_finding() {
    let mut b = base();
    set(&mut b, 7, "ROOM 101", "ROOM 101 ");
    let report = diff(&base(), &b);
    assert_eq!(only(&report).field, "text");
}

// ---- arrays one element shorter ------------------------------------------------

#[test]
fn a_knots_array_one_element_shorter_is_one_finding_about_its_length() {
    let mut b = base();
    set(
        &mut b,
        8,
        "[0.0,0.0,0.0,0.0,1.0,1.0,1.0,1.0]",
        "[0.0,0.0,0.0,0.0,1.0,1.0,1.0]",
    );
    let report = diff(&base(), &b);
    let f = only(&report);
    assert_eq!(
        (f.record_type.as_str(), f.field.as_str()),
        ("spline", "knots.len")
    );
    assert_eq!(
        (f.a.clone(), f.b.clone()),
        (Some(Val::Int(8)), Some(Val::Int(7)))
    );
}

#[test]
fn a_points_array_one_element_shorter_is_one_finding_about_its_length() {
    let mut b = base();
    set(&mut b, 9, ",[1.0,1.0,0.0]]", "]");
    let report = diff(&base(), &b);
    let f = only(&report);
    assert_eq!(
        (f.record_type.as_str(), f.field.as_str()),
        ("polygon", "points.len")
    );
}

// ---- floats: inside, just outside absolute, just outside relative -------------

#[test]
fn a_float_inside_tolerance_passes() {
    let mut b = base();
    set(&mut b, 3, "\"x2\":100.0", "\"x2\":100.0000000001");
    let report = diff(&base(), &b);
    assert!(report.findings.is_empty(), "{:?}", report.findings);
    // And the comparator saw it: the error is recorded even though it passed.
    let max = report.max_errors[&FieldKind::Coordinate];
    assert!(max.abs > 0.0 && max.abs < 1e-9, "{max:?}");
}

#[test]
fn a_small_value_just_outside_the_absolute_bound_is_one_finding() {
    let mut b = base();
    set(&mut b, 3, "\"x1\":0.0", "\"x1\":2e-9");
    let report = diff(&base(), &b);
    let f = only(&report);
    assert_eq!(f.field, "x1");
    assert_eq!(f.class, Class::BoundedTight);
    assert!((f.abs_error.unwrap() - 2e-9).abs() < 1e-15);
}

#[test]
fn a_small_value_just_inside_the_absolute_bound_passes() {
    let mut b = base();
    set(&mut b, 3, "\"x1\":0.0", "\"x1\":9e-10");
    assert!(diff(&base(), &b).findings.is_empty());
}

#[test]
fn a_large_value_just_outside_the_relative_bound_is_one_finding() {
    let mut a = base();
    let mut b = base();
    set(&mut a, 3, "\"x2\":100.0", "\"x2\":1000000.0");
    set(&mut b, 3, "\"x2\":100.0", "\"x2\":1000000.000002");
    let report = diff(&a, &b);
    let f = only(&report);
    assert_eq!(f.field, "x2");
    assert!(f.rel_error.unwrap() > BOUNDED_TIGHT.rel);
}

#[test]
fn a_large_value_just_inside_the_relative_bound_passes() {
    let mut a = base();
    let mut b = base();
    set(&mut a, 3, "\"x2\":100.0", "\"x2\":1000000.0");
    set(&mut b, 3, "\"x2\":100.0", "\"x2\":1000000.0000005");
    assert!(diff(&a, &b).findings.is_empty());
}

#[test]
fn the_rounding_boundary_is_a_match_under_both_bounded_classes() {
    // acadrust's harness compares {:.6}-formatted strings, so these two are a
    // DIFF there: 1.0000005 rounds one way and 1.0000004999 the other. They are
    // 1e-10 apart.
    let mut a = base();
    let mut b = base();
    set(&mut a, 3, "\"x2\":100.0", "\"x2\":1.0000005");
    set(&mut b, 3, "\"x2\":100.0", "\"x2\":1.0000004999");
    assert!(diff(&a, &b).findings.is_empty(), "BOUNDED_TIGHT");
    assert!(diff_loose(&a, &b).findings.is_empty(), "BOUNDED");
}

#[test]
fn under_bounded_a_small_value_just_outside_the_absolute_bound_is_one_finding() {
    let mut b = base();
    set(&mut b, 3, "\"x1\":0.0", "\"x1\":2e-9");
    let report = diff_loose(&base(), &b);
    let f = only(&report);
    assert_eq!((f.field.as_str(), f.class), ("x1", Class::Bounded));
}

#[test]
fn under_bounded_a_large_value_just_outside_the_relative_bound_is_one_finding() {
    let mut a = base();
    let mut b = base();
    set(&mut a, 3, "\"x2\":100.0", "\"x2\":1000000.0");
    set(&mut b, 3, "\"x2\":100.0", "\"x2\":1000002.0");
    let report = diff_loose(&a, &b);
    assert_eq!(only(&report).field, "x2");
    // And 0.5 on a million is inside 1e-6.
    set(&mut b, 3, "\"x2\":1000002.0", "\"x2\":1000000.5");
    assert!(diff_loose(&a, &b).findings.is_empty());
}

#[test]
fn the_loose_class_accepts_what_the_tight_class_refuses() {
    let mut a = base();
    let mut b = base();
    set(&mut a, 3, "\"x2\":100.0", "\"x2\":1000.0");
    set(&mut b, 3, "\"x2\":100.0", "\"x2\":1000.0000005");
    // 5e-7 on 1000: relative 5e-10, inside 1e-6, outside 1e-12.
    assert_eq!(only(&diff(&a, &b)).class, Class::BoundedTight);
    assert!(diff_loose(&a, &b).findings.is_empty());
}

#[test]
fn the_pairs_get_the_classes_the_issue_names() {
    let oracle_sharp = ClassTable::for_pair(ORACLE_SHARP, WarningRow::Compare);
    assert_eq!(oracle_sharp.class, Class::BoundedTight);
    assert_eq!(oracle_sharp.tolerance, BOUNDED_TIGHT);

    for (a, b) in [
        (Engine::Oracle, Engine::Acadrust),
        (Engine::Acadsharp, Engine::Acadrust),
        (Engine::Oracle, Engine::Ezdxf),
        (Engine::Acadsharp, Engine::Ezdxf),
        (Engine::Acadrust, Engine::Acadrust),
    ] {
        let t = ClassTable::for_pair(Pair::new(a, b), WarningRow::Compare);
        assert_eq!(t.class, Class::Bounded, "{a:?} against {b:?}");
        assert_eq!(t.tolerance, BOUNDED, "{a:?} against {b:?}");
    }
    let self_pair = ClassTable::for_pair(
        Pair::new(Engine::Acadsharp, Engine::Acadsharp),
        WarningRow::Compare,
    );
    assert_eq!(self_pair.class, Class::BoundedTight);
}

#[test]
fn the_bound_is_symmetric_and_two_zeros_are_equal() {
    assert!(BOUNDED_TIGHT.within(0.0, 0.0));
    assert_eq!(BOUNDED_TIGHT.bound(3.0, 5.0), BOUNDED_TIGHT.bound(5.0, 3.0));
    assert_eq!(BOUNDED.bound(0.0, 0.0), 1e-9);
    assert!((BOUNDED.bound(1e6, -1e6) - 1.0).abs() < 1e-12);
}

// ---- angles --------------------------------------------------------------------

fn arc_with(start: &str, end: &str) -> Vec<String> {
    let mut v = base();
    set(
        &mut v,
        5,
        "\"start\":0.5,\"end\":1.5",
        &format!("\"start\":{start},\"end\":{end}"),
    );
    v
}

#[test]
fn a_start_just_below_two_pi_matches_one_just_above_zero() {
    // The same angle, written either side of the wrap. The end moves with it,
    // since the schema spells it start + sweep.
    let tau = std::f64::consts::TAU;
    let a = arc_with(
        &format!("{:?}", tau - 1e-10),
        &format!("{:?}", tau - 1e-10 + 1.0),
    );
    let b = arc_with("1e-10", &format!("{:?}", 1e-10 + 1.0));
    let report = diff(&a, &b);
    assert!(report.findings.is_empty(), "{:?}", report.findings);
}

#[test]
fn an_end_one_full_turn_apart_is_a_finding() {
    // An unwrapped end is the point of ANGLE: wrap it and a full turn is a
    // match, and an arc that sweeps 360 degrees more is a different arc.
    let tau = std::f64::consts::TAU;
    let a = arc_with("0.5", "1.5");
    let b = arc_with("0.5", &format!("{:?}", 1.5 + tau));
    let report = diff(&a, &b);
    let f = only(&report);
    assert_eq!((f.record_type.as_str(), f.field.as_str()), ("arc", "end"));
    assert!((f.abs_error.unwrap() - tau).abs() < 1e-9);
}

#[test]
fn a_start_outside_the_bound_is_a_finding_judged_as_an_angle() {
    let a = arc_with("0.5", "1.5");
    let b = arc_with("0.500001", "1.5");
    let report = diff(&a, &b);
    let f = only(&report);
    assert_eq!(f.field, "start");
    assert_eq!(f.class, Class::Angle);
}

#[test]
fn a_text_rotation_is_wrapped() {
    let tau = std::f64::consts::TAU;
    let mut a = base();
    let mut b = base();
    set(
        &mut a,
        7,
        "\"rotation\":0.0",
        &format!("\"rotation\":{:?}", tau - 1e-10),
    );
    set(&mut b, 7, "\"rotation\":0.0", "\"rotation\":1e-10");
    assert!(diff(&a, &b).findings.is_empty());
}

// ---- warnings ------------------------------------------------------------------

#[test]
fn a_different_warning_category_is_one_finding() {
    let mut b = base();
    set(&mut b, 10, "unsupported_entity", "invalid_entity");
    let report = diff(&base(), &b);
    let f = only(&report);
    assert_eq!(
        (f.record_type.as_str(), f.field.as_str()),
        ("warning", "category")
    );
}

#[test]
fn a_different_warning_message_is_one_finding() {
    let mut b = base();
    set(&mut b, 10, "no_canonical_mapping", "something_else");
    let report = diff(&base(), &b);
    assert_eq!(only(&report).field, "detail");
}

#[test]
fn a_pairs_table_can_exclude_warnings_with_a_reason() {
    let mut b = base();
    set(&mut b, 10, "unsupported_entity", "invalid_entity");
    let table = ClassTable::for_pair(
        ORACLE_RUST,
        WarningRow::Excluded {
            reason: "entity_type is an ACadSharp type name".into(),
            issue: "acadsharp-rs-tests#19".into(),
        },
    );
    let report = compare("fixture", &stream(&base()), &stream(&b), &table).unwrap();
    assert!(report.findings.is_empty());
    assert_eq!(report.excluded_records["warning"], (2, 2));
}

// ---- order ---------------------------------------------------------------------

#[test]
fn two_records_swapped_is_one_order_finding_and_no_field_mismatch() {
    let mut b = base();
    b.swap(3, 4);
    let report = diff(&base(), &b);
    let f = only(&report);
    assert_eq!(f.kind, FindingKind::Order);
    assert!(matches!(f.record_type.as_str(), "line" | "circle"));
    assert!(f.index_a.is_some() && f.index_b.is_some() && f.index_a != f.index_b);
}

// ---- alignment, with and without sidecars -------------------------------------

fn long_stream(n: usize) -> Vec<String> {
    let mut v = vec![DOCUMENT.to_string(), VIEW_MODEL.to_string()];
    for i in 0..n {
        v.push(format!(
            "{{\"schema\":1,\"record\":\"line\",\"view\":0,\"x1\":{}.5,\"y1\":{}.25,\"z1\":0.0,\"x2\":{}.0,\"y2\":1.0,\"z2\":0.0}}",
            i * 7,
            i * 3,
            i * 7 + 4
        ));
    }
    v
}

#[test]
fn one_dropped_record_in_a_300_record_stream_is_one_finding_and_the_rest_compared() {
    let a = long_stream(298);
    assert_eq!(a.len(), 300);
    let mut b = a.clone();
    b.remove(150);
    let report = diff(&a, &b);
    let f = only(&report);
    assert_eq!(f.kind, FindingKind::OnlyInA);
    assert_eq!(f.index_a, Some(150));
    assert_eq!(f.handles_a, None, "no sidecar on either side");
    assert_eq!(report.aligned.len(), 299);
    assert!(report.aligned.contains(&(151, 150)), "the rest realigned");
    // Every aligned pair was compared, not just the first 150.
    assert!(
        report.floats_compared >= 297 * 6,
        "{}",
        report.floats_compared
    );
}

fn sidecar_text(records: usize) -> String {
    (0..records)
        .map(|i| {
            let t = match i {
                0 => "document",
                1 => "view",
                _ => "line",
            };
            format!(
                "{{\"i\":{i},\"h\":[\"{:X}\",\"{:X}\"],\"t\":\"{t}\"}}\n",
                0x100 + i,
                0x900 + i
            )
        })
        .collect()
}

#[test]
fn with_a_sidecar_the_finding_names_the_handle() {
    let a = long_stream(298);
    let mut b = a.clone();
    b.remove(150);
    let sa = stream(&a);
    let sidecar = Sidecar::parse(&sidecar_text(300), &sa.records).expect("sidecar");
    let sa = acadsharp_rs_tests::compare::Stream {
        sidecar: Some(sidecar),
        ..sa
    };
    let report = compare("fixture", &sa, &stream(&b), &tight()).unwrap();
    let f = only(&report);
    assert_eq!(
        f.handles_a.as_deref(),
        Some(&["196".to_string(), "996".to_string()][..])
    );
    assert!(f.render().contains("196/996"), "{}", f.render());
}

#[test]
fn a_perturbation_a_tolerance_question_away_keeps_its_alignment() {
    let a = long_stream(298);
    let mut b = a.clone();
    set(&mut b, 100, "\"x1\":686.5", "\"x1\":686.500000002");
    let report = diff(&a, &b);
    let f = only(&report);
    assert_eq!(f.kind, FindingKind::FieldMismatch);
    assert_eq!(f.field, "x1");
    assert_eq!(report.aligned.len(), 300);
}

// ---- findings carry every column ----------------------------------------------

#[test]
fn a_finding_names_everything_somebody_will_ask() {
    let mut b = base();
    set(&mut b, 4, "\"radius\":5.0", "\"radius\":5.5");
    let report = diff(&base(), &b);
    let f = only(&report);
    assert_eq!(f.fixture, "fixture");
    assert_eq!(f.pair, ORACLE_SHARP);
    assert_eq!(f.view, Some(0));
    assert_eq!((f.index_a, f.index_b), (Some(4), Some(4)));
    assert_eq!(f.record_type, "circle");
    assert_eq!(f.field, "radius");
    assert_eq!(
        (f.a.clone(), f.b.clone()),
        (Some(Val::Float(5.0)), Some(Val::Float(5.5)))
    );
    assert_eq!(f.abs_error, Some(0.5));
    assert!((f.rel_error.unwrap() - 0.5 / 5.5).abs() < 1e-15);
    assert_eq!(f.class, Class::BoundedTight);

    let line = f.render();
    for needle in [
        "fixture",
        "oracle-acadsharp",
        "view 0",
        "#4",
        "circle",
        "radius",
        "5.0",
        "5.5",
        "BOUNDED_TIGHT",
    ] {
        assert!(line.contains(needle), "{needle:?} missing from {line:?}");
    }
    assert!(!line.contains('\n'), "one line");
}

#[test]
fn the_report_carries_the_largest_error_per_field_kind() {
    let mut b = base();
    set(&mut b, 3, "\"x2\":100.0", "\"x2\":100.0000000002");
    set(&mut b, 4, "\"radius\":5.0", "\"radius\":5.0000000003");
    let report = diff(&base(), &b);
    assert!(report.findings.is_empty());
    let coordinate = report.max_errors[&FieldKind::Coordinate];
    let scalar = report.max_errors[&FieldKind::Scalar];
    assert!((coordinate.abs - 2e-10).abs() < 1e-13, "{coordinate:?}");
    assert!((scalar.abs - 3e-10).abs() < 1e-13, "{scalar:?}");
    assert!(report.max_errors.contains_key(&FieldKind::Angle));
}

// ---- KNOWN_DIVERGENCE ----------------------------------------------------------

fn radius_differs() -> (Vec<String>, Vec<String>) {
    let a = base();
    let mut b = base();
    set(&mut b, 4, "\"radius\":5.0", "\"radius\":5.5");
    (a, b)
}

fn known(a: f64, b: f64) -> KnownDivergence {
    KnownDivergence {
        fixture: "fixture".into(),
        record: RecordKey::OracleIndex(4),
        record_type: "circle".into(),
        field: "radius".into(),
        a: Val::Float(a),
        b: Val::Float(b),
        reason: "acadrust reads the circle's radius from the wrong group".into(),
        issue: "libviprs/acadrust#99".into(),
    }
}

fn with_known(k: KnownDivergence) -> ClassTable {
    let mut t = loose();
    t.known.push(k);
    t
}

#[test]
fn a_known_divergence_that_still_diverges_exactly_as_listed_is_not_red() {
    let (a, b) = radius_differs();
    let report = compare(
        "fixture",
        &stream(&a),
        &stream(&b),
        &with_known(known(5.0, 5.5)),
    )
    .unwrap();
    let f = only(&report);
    assert_eq!(f.class, Class::KnownDivergence);
    assert_eq!(
        f.disposition,
        Disposition::Known {
            issue: "libviprs/acadrust#99".into()
        }
    );
    assert!(!f.is_red());
    assert!(report.is_match());
}

#[test]
fn a_known_divergence_that_no_longer_diverges_is_red_and_asks_for_the_row_to_be_tightened() {
    let report = compare(
        "fixture",
        &stream(&base()),
        &stream(&base()),
        &with_known(known(5.0, 5.5)),
    )
    .unwrap();
    let f = only(&report);
    assert_eq!(f.disposition, Disposition::KnownVanished);
    assert!(f.is_red());
    assert!(!report.is_match());
    assert!(f.render().contains("tighten"), "{}", f.render());
}

#[test]
fn a_known_divergence_whose_values_moved_is_red_and_asks_to_be_re_argued() {
    let (a, b) = radius_differs();
    let report = compare(
        "fixture",
        &stream(&a),
        &stream(&b),
        &with_known(known(5.0, 6.0)),
    )
    .unwrap();
    let f = only(&report);
    assert_eq!(f.disposition, Disposition::KnownMoved);
    assert!(f.is_red());
    assert!(f.render().contains("re-argue"), "{}", f.render());
}

#[test]
fn a_known_divergence_is_found_by_handle_path_when_there_is_no_oracle_index() {
    let (a, b) = radius_differs();
    let mut sb = stream(&b);
    let text: String = (0..12)
        .map(|i| format!("{{\"i\":{i},\"h\":[\"{:X}\"]}}\n", 0x40 + i))
        .collect();
    sb.sidecar = Some(Sidecar::parse(&text, &sb.records).unwrap());
    let mut k = known(5.0, 5.5);
    k.record = RecordKey::Handles(vec!["44".into()]);
    let mut t = ClassTable::for_pair(
        Pair::new(Engine::Acadsharp, Engine::Acadrust),
        WarningRow::Compare,
    );
    t.known.push(k);
    let report = compare("fixture", &stream(&a), &sb, &t).unwrap();
    assert_eq!(
        only(&report).disposition,
        Disposition::Known {
            issue: "libviprs/acadrust#99".into()
        }
    );
}

#[test]
fn a_known_divergence_for_another_fixture_is_ignored() {
    let (a, b) = radius_differs();
    let mut k = known(5.0, 5.5);
    k.fixture = "elsewhere".into();
    let report = compare("fixture", &stream(&a), &stream(&b), &with_known(k)).unwrap();
    assert_eq!(only(&report).disposition, Disposition::Open);
}

// ---- EXCLUDED, DUAL_ORACLE, and rows without a reason --------------------------

fn exclusion(scope: Scope, reason: &str, issue: &str) -> Exclusion {
    Exclusion {
        scope,
        reason: reason.into(),
        issue: issue.into(),
    }
}

#[test]
fn an_excluded_record_kind_is_dropped_from_both_sides_and_counted() {
    let mut a = base();
    let mut b = base();
    a.insert(5, POINT.to_string());
    b.insert(5, POINT.replace("\"x\":1.0", "\"x\":9.0"));
    b.insert(6, POINT.to_string());
    let mut t = loose();
    t.excluded.push(exclusion(
        Scope::Record("point".into()),
        "acadsharp-rs has no point until wire 3",
        "libviprs/libviprs-dep#83",
    ));
    let report = compare("fixture", &stream(&a), &stream(&b), &t).unwrap();
    assert!(report.findings.is_empty(), "{:?}", report.findings);
    assert_eq!(report.excluded_records["point"], (1, 2));
}

#[test]
fn an_excluded_field_is_skipped_and_counted() {
    let mut b = base();
    set(
        &mut b,
        0,
        "\"acad_version\":\"AC1032\"",
        "\"acad_version\":\"AC1027\"",
    );
    let mut t = loose();
    t.excluded.push(exclusion(
        Scope::Field {
            record: "document".into(),
            field: "acad_version".into(),
        },
        "the cross-save pair differs by design",
        "acadsharp-rs-tests#19",
    ));
    let report = compare("fixture", &stream(&base()), &stream(&b), &t).unwrap();
    assert!(report.findings.is_empty(), "{:?}", report.findings);
    assert_eq!(report.excluded_fields, 1);
}

#[test]
fn an_excluded_array_field_excludes_its_length_and_elements() {
    let mut b = base();
    set(
        &mut b,
        8,
        "[0.0,0.0,0.0,0.0,1.0,1.0,1.0,1.0]",
        "[0.0,0.0,0.0,0.0,1.0,1.0,1.0]",
    );
    let mut t = loose();
    t.excluded.push(exclusion(
        Scope::Field {
            record: "spline".into(),
            field: "knots".into(),
        },
        "a reason",
        "an issue",
    ));
    let report = compare("fixture", &stream(&base()), &stream(&b), &t).unwrap();
    assert!(report.findings.is_empty(), "{:?}", report.findings);
}

#[test]
fn an_excluded_row_with_no_reason_is_red() {
    let mut t = loose();
    t.excluded
        .push(exclusion(Scope::Record("point".into()), "", "an issue"));
    assert!(matches!(t.validate(), Err(TableError::NoReason(_))));
    assert!(compare("f", &stream(&base()), &stream(&base()), &t).is_err());
}

#[test]
fn an_excluded_row_with_no_issue_is_red() {
    let mut t = loose();
    t.excluded
        .push(exclusion(Scope::Record("point".into()), "a reason", "  "));
    assert!(matches!(t.validate(), Err(TableError::NoIssue(_))));
}

#[test]
fn a_known_divergence_row_with_no_reason_or_no_issue_is_red() {
    let mut k = known(5.0, 5.5);
    k.reason = String::new();
    assert!(matches!(
        with_known(k).validate(),
        Err(TableError::NoReason(_))
    ));
    let mut k = known(5.0, 5.5);
    k.issue = String::new();
    assert!(matches!(
        with_known(k).validate(),
        Err(TableError::NoIssue(_))
    ));
}

#[test]
fn a_warning_row_that_excludes_without_a_reason_or_issue_is_red() {
    let t = ClassTable::for_pair(
        ORACLE_RUST,
        WarningRow::Excluded {
            reason: String::new(),
            issue: "x".into(),
        },
    );
    assert!(matches!(t.validate(), Err(TableError::NoReason(_))));
    let t = ClassTable::for_pair(
        ORACLE_RUST,
        WarningRow::Excluded {
            reason: "x".into(),
            issue: String::new(),
        },
    );
    assert!(matches!(t.validate(), Err(TableError::NoIssue(_))));
}

#[test]
fn a_dual_oracle_disagreement_is_listed_and_not_scored() {
    let (a, b) = radius_differs();
    let mut t = loose();
    t.dual_oracle.push(DualOracle {
        fixture: "fixture".into(),
        record_type: "circle".into(),
        field: "radius".into(),
        reason: "neither reader and no ground truth agree".into(),
        issue: "acadsharp-rs-tests#19".into(),
    });
    let report = compare("fixture", &stream(&a), &stream(&b), &t).unwrap();
    let f = only(&report);
    assert_eq!(f.class, Class::DualOracle);
    assert!(!f.is_red());
    assert!(report.is_match());
}

#[test]
fn a_dual_oracle_row_with_no_reason_is_red() {
    let mut t = loose();
    t.dual_oracle.push(DualOracle {
        fixture: "f".into(),
        record_type: "circle".into(),
        field: "radius".into(),
        reason: String::new(),
        issue: "x".into(),
    });
    assert!(matches!(t.validate(), Err(TableError::NoReason(_))));
}
