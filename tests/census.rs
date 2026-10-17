//! AutoCAD's object census as a ground truth for what a reader found (R3.4).
//!
//! The census files are dumped from the live session straight after each
//! `SAVEAS`, so they are equal across the saves by construction. What is not
//! vacuous is the check against each reader: its model-space entities, by
//! handle and by class, have to be the ones AutoCAD says are there.

use acadsharp_rs_tests::canonical::{Record, Sidecar, parse_stream};
use acadsharp_rs_tests::census::{Census, Finding, check_full, check_weak, dxf_names};

/// Census JSON in the shape upstream writes, with a model space of the given
/// (class, handle, invisible) triples.
fn census_json(entities: &[(&str, u64, bool)]) -> String {
    let rows: Vec<String> = entities
        .iter()
        .map(|(class, handle, invisible)| {
            format!(
                r#"{{"IsInvisible":{invisible},"ACadName":"Autodesk.AutoCAD.DatabaseServices.{class}","Handle":{handle},"OwnerHandle":31,"DictionaryHandle":0,"Children":[]}}"#
            )
        })
        .collect();
    format!(
        r#"{{"BlocksTable":{{"Entries":[{{"Name":"*Paper_Space","Entities":[{{"IsInvisible":false,"ACadName":"Autodesk.AutoCAD.DatabaseServices.Viewport","Handle":999,"Children":[]}}]}},{{"Name":"*Model_Space","Entities":[{}]}}]}}}}"#,
        rows.join(",")
    )
}

const DOC: &str = r#"{"schema":1,"record":"document","format":"dxf","acad_version":"AC1032","maintenance_version":0}"#;
const MODEL: &str = r#"{"schema":1,"record":"view","index":0,"name":"Model","kind":"model"}"#;
const LAYOUT: &str = r#"{"schema":1,"record":"view","index":1,"name":"Layout1","kind":"layout"}"#;
fn line(view: u32) -> String {
    format!(
        r#"{{"schema":1,"record":"line","view":{view},"x1":0.0,"y1":0.0,"z1":0.0,"x2":1.0,"y2":0.0,"z2":0.0}}"#
    )
}
fn text(view: u32) -> String {
    format!(
        r#"{{"schema":1,"record":"text","view":{view},"x":0.0,"y":0.0,"z":0.0,"height":1.0,"rotation":0.0,"width_factor":1.0,"halign":"left","valign":"baseline","style":"Standard","text":"x"}}"#
    )
}

/// A reader's stream and sidecar from (record json, sidecar json) pairs.
fn reader(rows: &[(String, String)]) -> (Vec<Record>, Sidecar) {
    let stream: String = rows.iter().map(|(r, _)| format!("{r}\n")).collect();
    let side: String = rows.iter().map(|(_, s)| format!("{s}\n")).collect();
    let records = parse_stream(&stream).expect("test stream parses");
    let sidecar = Sidecar::parse(&side, &records).expect("test sidecar parses");
    (records, sidecar)
}

fn framing() -> Vec<(String, String)> {
    vec![
        (DOC.into(), r#"{"i":0,"h":["0"],"t":"document"}"#.into()),
        (MODEL.into(), r#"{"i":1,"h":["0"],"t":"view"}"#.into()),
        (LAYOUT.into(), r#"{"i":2,"h":["0"],"t":"view"}"#.into()),
    ]
}

fn row(rec: String, i: usize, handles: &str, t: &str, d: &str) -> (String, String) {
    (rec, format!(r#"{{"i":{i},"h":[{handles}],"t":"{t}","d":"{d}"}}"#))
}

// handles: 0x1A = 26, 0x1B = 27, 0x1C = 28
fn census3() -> Census {
    Census::parse(&census_json(&[
        ("Line", 26, false),
        ("DBText", 27, false),
        ("Line", 28, false),
    ]))
    .unwrap()
}

fn good_rows() -> Vec<(String, String)> {
    let mut rows = framing();
    rows.push(row(line(0), 3, r#""1A""#, "line", "LINE"));
    rows.push(row(text(0), 4, r#""1B""#, "text", "TEXT"));
    rows.push(row(line(0), 5, r#""1C""#, "line", "LINE"));
    // paper space is not model space and must not count
    rows.push(row(line(1), 6, r#""FF""#, "line", "LINE"));
    rows
}

#[test]
fn the_census_parser_reads_only_model_space() {
    let c = census3();
    assert_eq!(c.model_space.len(), 3);
    assert_eq!(c.model_space[1].class, "DBText");
    assert_eq!(c.model_space[1].handle, 27);
}

#[test]
fn the_census_parser_reads_the_invisible_flag() {
    let c = Census::parse(&census_json(&[("Line", 26, true)])).unwrap();
    assert!(c.model_space[0].invisible);
}

#[test]
fn a_census_with_no_model_space_is_refused() {
    assert!(Census::parse(r#"{"BlocksTable":{"Entries":[]}}"#).is_err());
    assert!(Census::parse("not json").is_err());
}

#[test]
fn a_reader_that_found_exactly_the_census_is_clean() {
    let (records, sidecar) = reader(&good_rows());
    assert_eq!(check_full(&census3(), &records, &sidecar), []);
}

#[test]
fn a_reader_one_short_is_red_and_names_the_handle() {
    let mut rows = good_rows();
    rows.remove(5); // the entity 0x1C
    let (records, sidecar) = {
        // keep the index run gapless
        let renumbered: Vec<(String, String)> = rows
            .into_iter()
            .enumerate()
            .map(|(i, (r, s))| (r, s.replacen(&format!("\"i\":{}", i + (i >= 5) as usize), &format!("\"i\":{i}"), 1)))
            .collect();
        reader(&renumbered)
    };
    let findings = check_full(&census3(), &records, &sidecar);
    assert_eq!(
        findings,
        [Finding::Missing { handle: 28, class: "Line".into() }],
        "{findings:?}"
    );
    assert!(findings[0].to_string().contains("1C"), "the message names the handle in hex: {}", findings[0]);
}

#[test]
fn a_reader_with_one_too_many_is_red_and_names_the_handle() {
    let mut rows = good_rows();
    rows.push(row(line(0), 7, r#""AA""#, "line", "LINE"));
    let (records, sidecar) = reader(&rows);
    let findings = check_full(&census3(), &records, &sidecar);
    assert_eq!(findings, [Finding::Unexpected { handle: 0xAA }], "{findings:?}");
    assert!(findings[0].to_string().contains("AA"));
}

#[test]
fn an_expanded_insert_counts_once_by_its_outermost_handle() {
    let c = Census::parse(&census_json(&[("BlockReference", 26, false)])).unwrap();
    let mut rows = framing();
    rows.push(row(line(0), 3, r#""1A","50""#, "line", "LINE"));
    rows.push(row(line(0), 4, r#""1A","51""#, "line", "LINE"));
    let (records, sidecar) = reader(&rows);
    assert_eq!(check_full(&c, &records, &sidecar), []);
}

#[test]
fn a_wrong_class_is_red_for_a_top_level_record() {
    let mut rows = framing();
    rows.push(row(line(0), 3, r#""1A""#, "line", "LINE"));
    rows.push(row(text(0), 4, r#""1B""#, "text", "MTEXT")); // census says DBText
    rows.push(row(line(0), 5, r#""1C""#, "line", "LINE"));
    let (records, sidecar) = reader(&rows);
    let findings = check_full(&census3(), &records, &sidecar);
    assert_eq!(
        findings,
        [Finding::WrongClass { handle: 27, census: "DBText".into(), reader: "MTEXT".into() }],
        "{findings:?}"
    );
}

#[test]
fn an_invisible_entity_may_be_absent_and_a_visible_one_may_not() {
    let c = Census::parse(&census_json(&[("Line", 26, false), ("Line", 27, true)])).unwrap();
    let mut rows = framing();
    rows.push(row(line(0), 3, r#""1A""#, "line", "LINE"));
    let (records, sidecar) = reader(&rows);
    assert_eq!(check_full(&c, &records, &sidecar), [], "the invisible one is allowed to be missing");

    let c = Census::parse(&census_json(&[("Line", 26, false), ("Line", 27, false)])).unwrap();
    assert_eq!(
        check_full(&c, &records, &sidecar),
        [Finding::Missing { handle: 27, class: "Line".into() }]
    );
}

#[test]
fn an_empty_model_space_in_the_reader_is_refused_as_vacuous() {
    let (records, sidecar) = reader(&framing());
    assert_eq!(check_full(&census3(), &records, &sidecar), [Finding::Vacuous]);
    assert_eq!(check_weak(&census3(), &records, &sidecar), [Finding::Vacuous]);
}

#[test]
fn a_class_the_name_map_does_not_know_is_a_finding_not_a_pass() {
    let c = Census::parse(&census_json(&[("SomethingNew", 26, false)])).unwrap();
    let mut rows = framing();
    rows.push(row(line(0), 3, r#""1A""#, "line", "LINE"));
    let (records, sidecar) = reader(&rows);
    assert!(
        check_full(&c, &records, &sidecar)
            .contains(&Finding::UnmappedClass { class: "SomethingNew".into() })
    );
}

fn weak_rows(entities: &[(&str, &str)]) -> Vec<(String, String)> {
    // acadsharp-rs sidecars: `h` is [leaf], no `t`-independent type, `x` marks an expansion.
    let mut rows = framing();
    for (i, (handle, x)) in entities.iter().enumerate() {
        rows.push((
            line(0),
            format!(r#"{{"i":{},"h":["{handle}"],"t":"line","x":{x}}}"#, i + 3),
        ));
    }
    rows
}

#[test]
fn the_weak_check_passes_when_top_level_handles_are_in_and_cover_the_census() {
    // 1A and 1C top level; 1B is a block reference's expansion leaf (x true)
    let c = Census::parse(&census_json(&[("Line", 26, false), ("BlockReference", 27, false), ("Line", 28, false)])).unwrap();
    let rows = weak_rows(&[("1A", "false"), ("50", "true"), ("1C", "false")]);
    let (records, sidecar) = reader(&rows);
    assert_eq!(check_weak(&c, &records, &sidecar), []);
}

#[test]
fn the_weak_check_is_red_for_a_top_level_handle_the_census_lacks() {
    let c = census3();
    let rows = weak_rows(&[("1A", "false"), ("1B", "false"), ("1C", "false"), ("EE", "false")]);
    let (records, sidecar) = reader(&rows);
    assert_eq!(check_weak(&c, &records, &sidecar), [Finding::Unexpected { handle: 0xEE }]);
}

#[test]
fn the_weak_check_is_red_for_a_census_entity_it_never_saw() {
    let c = census3();
    let rows = weak_rows(&[("1A", "false"), ("1C", "false")]);
    let (records, sidecar) = reader(&rows);
    assert_eq!(
        check_weak(&c, &records, &sidecar),
        [Finding::Missing { handle: 27, class: "DBText".into() }]
    );
}

#[test]
fn the_weak_check_does_not_ask_for_block_references_or_dimensions() {
    let c = Census::parse(&census_json(&[("Line", 26, false), ("BlockReference", 27, false), ("RotatedDimension", 28, false)])).unwrap();
    let rows = weak_rows(&[("1A", "false")]);
    let (records, sidecar) = reader(&rows);
    assert_eq!(check_weak(&c, &records, &sidecar), []);
}

#[test]
fn the_expansion_leaves_of_an_insert_are_not_unexpected_for_the_weak_check() {
    let c = Census::parse(&census_json(&[("Line", 26, false)])).unwrap();
    let rows = weak_rows(&[("1A", "false"), ("7777", "true")]);
    let (records, sidecar) = reader(&rows);
    assert_eq!(check_weak(&c, &records, &sidecar), []);
}

#[test]
fn the_name_map_knows_the_common_classes_and_not_a_made_up_one() {
    assert_eq!(dxf_names("DBText"), Some(&["TEXT"][..]));
    assert!(dxf_names("BlockReference").unwrap().contains(&"INSERT"));
    assert!(dxf_names("Point3AngularDimension").unwrap().contains(&"DIMENSION"));
    assert!(dxf_names("NotAClass").is_none());
}
