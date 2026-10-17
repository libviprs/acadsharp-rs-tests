//! The census check over real readers (R3.4): acadrust over the DWGs, ezdxf over
//! AutoCAD's own DXF twins.
//!
//! AutoCAD's census lists 163 model-space entities of 36 classes, identical in
//! all seven saves. A reader that reports a different set is wrong about what
//! is in the drawing, and these are the tests that say so by handle.

use std::collections::BTreeSet;

use acadsharp_rs_tests::canonical::{Record, Sidecar, parse_stream};
use acadsharp_rs_tests::census::{Census, Finding, check_full, dxf_names};
use acadsharp_rs_tests::fixtures;

const SAVES: [&str; 7] = [
    "acadsharp-ac1014",
    "acadsharp-ac1015",
    "acadsharp-ac1018",
    "acadsharp-ac1021",
    "acadsharp-ac1024",
    "acadsharp-ac1027",
    "acadsharp-ac1032",
];

/// The six saves that have a DXF twin (R14 has none).
const TWINS: [&str; 6] = ["1015", "1018", "1021", "1024", "1027", "1032"];

fn ezdxf_stream(input: &str) -> (Vec<Record>, Sidecar) {
    let gt = fixtures::ground_truth(input).expect("a ground truth row for every DXF twin");
    let records = parse_stream(&String::from_utf8(gt.load_jsonl().unwrap()).unwrap()).unwrap();
    let sidecar = Sidecar::parse(
        &String::from_utf8(gt.load_sidecar().unwrap()).unwrap(),
        &records,
    )
    .unwrap();
    (records, sidecar)
}

fn show(findings: &[Finding]) -> String {
    findings.iter().map(|f| format!("\n  {f}")).collect()
}

#[test]
fn the_census_is_163_entities_of_36_classes_and_the_same_in_every_save() {
    let first = Census::for_fixture(SAVES[6]).unwrap();
    assert_eq!(first.model_space.len(), 163);
    let classes: BTreeSet<&str> = first.model_space.iter().map(|e| e.class.as_str()).collect();
    assert_eq!(classes.len(), 36, "{classes:?}");
    let set = |c: &Census| -> BTreeSet<(u64, String)> {
        c.model_space
            .iter()
            .map(|e| (e.handle, e.class.clone()))
            .collect()
    };
    for id in SAVES {
        let c = Census::for_fixture(id).unwrap();
        assert_eq!(set(&c), set(&first), "{id}'s census is not the others'");
    }
}

#[test]
fn the_name_map_knows_every_class_in_every_census_file() {
    for row in fixtures::censuses() {
        let c = Census::for_fixture(&row.for_fixtures[0]).unwrap();
        for e in &c.model_space {
            assert!(
                dxf_names(&e.class).is_some(),
                "{}: the class name map has no entry for {}",
                row.id,
                e.class
            );
        }
    }
}

#[test]
fn ezdxf_reports_exactly_the_census_on_every_save_that_has_a_dxf_twin() {
    for v in TWINS {
        for enc in ["ascii", "binary"] {
            let (records, sidecar) = ezdxf_stream(&format!("dxf-sample-ac{v}-{enc}"));
            let census = Census::for_fixture(&format!("acadsharp-ac{v}")).unwrap();
            let findings = check_full(&census, &records, &sidecar);
            assert!(
                findings.is_empty(),
                "ezdxf, AC{v} {enc}:{}",
                show(&findings)
            );
        }
    }
}

/// A copy of a stream without every record the sidecar attributes to `handle`
/// at its outermost level.
fn without(records: &[Record], sidecar: &Sidecar, handle: u64) -> (Vec<Record>, Sidecar) {
    let hex = format!("{handle:X}");
    let keep: Vec<usize> = (0..records.len())
        .filter(|&i| sidecar.entries[i].handles.first().map(String::as_str) != Some(&hex))
        .collect();
    assert!(
        keep.len() < records.len(),
        "{hex} names no record, so nothing was removed"
    );
    let recs: Vec<Record> = keep.iter().map(|&i| records[i].clone()).collect();
    let mut entries = Vec::new();
    for (n, &i) in keep.iter().enumerate() {
        let mut e = sidecar.entries[i].clone();
        e.index = n;
        entries.push(e);
    }
    (recs, Sidecar { entries })
}

#[test]
fn a_reader_that_loses_one_entity_is_red_naming_its_handle() {
    // The positive control, on real data: the same check, one entity gone.
    let (records, sidecar) = ezdxf_stream("dxf-sample-ac1032-ascii");
    let census = Census::for_fixture("acadsharp-ac1032").unwrap();
    // Pick a model-space entity that has records of its own.
    let victim = census
        .model_space
        .iter()
        .find(|e| e.class == "Circle")
        .expect("the drawing has a circle");
    let (records, sidecar) = without(&records, &sidecar, victim.handle);
    let findings = check_full(&census, &records, &sidecar);
    assert_eq!(
        findings,
        [Finding::Missing {
            handle: victim.handle,
            class: "Circle".into()
        }],
        "{}",
        show(&findings)
    );
    assert!(
        findings[0]
            .to_string()
            .contains(&format!("{:X}", victim.handle))
    );
}

#[test]
fn the_check_is_not_vacuous_on_the_real_streams() {
    let (records, sidecar) = ezdxf_stream("dxf-sample-ac1032-ascii");
    let tops: BTreeSet<&str> = records
        .iter()
        .zip(&sidecar.entries)
        .filter(|(r, _)| r.view() == Some(0))
        .filter_map(|(_, e)| e.handles.first().map(String::as_str))
        .collect();
    let census = Census::for_fixture("acadsharp-ac1032").unwrap();
    let visible = census.model_space.iter().filter(|e| !e.invisible).count();
    assert!(
        tops.len() > 100 && tops.len() <= visible,
        "{} outermost handles in model space against {visible} visible census entities",
        tops.len()
    );
}

#[cfg(feature = "acadrust")]
mod acadrust {
    use super::*;
    use acadsharp_rs_tests::acadrust_emit::extract_bytes;
    use acadsharp_rs_tests::canonical::render_sidecar;
    use acadsharp_rs_tests::census::check_weak;
    use acadsharp_rs_tests::emit::Emission;

    fn emission(fixture: &str) -> (Emission, Sidecar) {
        let e = extract_bytes(&fixtures::load(fixture).unwrap()).unwrap();
        let sidecar = Sidecar::parse(&render_sidecar(&e.sidecar), &e.records).unwrap();
        (e, sidecar)
    }

    #[test]
    fn acadrust_reports_exactly_the_census_on_every_save_and_on_sample_base() {
        for id in SAVES.iter().copied().chain(["acadsharp-sample-base"]) {
            let (e, sidecar) = emission(id);
            let findings = check_full(&Census::for_fixture(id).unwrap(), &e.records, &sidecar);
            assert!(findings.is_empty(), "acadrust, {id}:{}", show(&findings));
        }
    }

    #[test]
    fn an_unsupported_entity_is_reported_through_its_warning_not_lost() {
        // The census counts viewports, multileaders and the rest; acadrust turns
        // them into warnings, and the sidecar has to carry the entity's handle
        // there or the count could never agree with AutoCAD's.
        let (e, sidecar) = emission("acadsharp-ac1032");
        let named = e
            .records
            .iter()
            .zip(&sidecar.entries)
            .filter(|(r, s)| matches!(r, Record::Warning(_)) && s.handles != ["0"])
            .count();
        assert!(named > 0, "no warning names an entity");
    }

    #[test]
    fn the_weak_check_passes_for_a_sidecar_cut_down_to_what_acadsharp_rs_writes() {
        // acadsharp-rs's sidecar has the leaf handle only, no type and no DXF
        // name, and `x` for "came out of an insert". Cut acadrust's down to
        // that shape; the real archive is wired in with emit-acadsharp.
        let (e, _) = emission("acadsharp-ac1032");
        let mut cut = e.sidecar.clone();
        for entry in &mut cut {
            entry.from_insert = Some(entry.handles.len() > 1);
            entry.handles = vec![entry.handles.last().cloned().unwrap()];
            entry.record_type = None;
            entry.dxf_name = None;
        }
        let sidecar = Sidecar::parse(&render_sidecar(&cut), &e.records).unwrap();
        let census = Census::for_fixture("acadsharp-ac1032").unwrap();
        let findings = check_weak(&census, &e.records, &sidecar);
        // Unsupported entities carry no record of their own, so the weak check
        // names them: that is what it is for.
        for f in &findings {
            assert!(matches!(f, Finding::Missing { .. }), "{f}");
        }
        assert!(findings.len() < 40, "{}", show(&findings));
    }
}
