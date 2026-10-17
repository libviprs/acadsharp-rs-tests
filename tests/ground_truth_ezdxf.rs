//! The ezdxf ground truths (R3.4): what they are, that they are the files the
//! manifest froze, and that the strict parser reads every one.
//!
//! The extractor itself is tested inside its image (`tools/ground_truth/tests`),
//! because it needs ezdxf. These are the offline checks that run everywhere.

use std::collections::BTreeSet;
use std::path::Path;

use acadsharp_rs_tests::canonical::{Record, Sidecar, parse_stream};
use acadsharp_rs_tests::compare::{
    ClassTable, Engine, Exclusion, Pair, Scope, Stream, WarningRow, compare,
};
use acadsharp_rs_tests::fixtures;
use sha2::{Digest, Sha256};

fn requirements_sha256() -> String {
    let bytes = std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tools/ground_truth/requirements.txt"),
    )
    .unwrap();
    format!("{:x}", Sha256::digest(&bytes))
}

#[test]
fn every_dxf_in_the_manifest_has_exactly_one_ground_truth() {
    let inputs: BTreeSet<&str> = fixtures::oracle_inputs()
        .iter()
        .map(|i| i.id.as_str())
        .collect();
    let truths: Vec<&str> = fixtures::ground_truths()
        .iter()
        .filter(|g| !g.is_written())
        .map(|g| g.input.as_str())
        .collect();
    assert_eq!(truths.len(), inputs.len(), "one row per DXF");
    assert_eq!(truths.iter().copied().collect::<BTreeSet<_>>(), inputs);
}

#[test]
fn the_dxf_twins_of_the_dwgs_and_the_r5_inputs_are_all_extracted() {
    // Every DXF with a DWG twin (sample_AC1015 to AC1032, the ten dynamic
    // blocks, geoloc), and the AC1009 and geoloc_AC1021 files, which are
    // inputs for R5.1 and R5.2 and are compared with nothing yet.
    let have = |id: &str| fixtures::ground_truth(id).is_some();
    for v in ["1015", "1018", "1021", "1024", "1027", "1032"] {
        assert!(
            have(&format!("dxf-sample-ac{v}-ascii")) && have(&format!("dxf-sample-ac{v}-binary")),
            "AC{v}"
        );
    }
    for name in [
        "alignment",
        "basepoint",
        "flip",
        "linear",
        "lookup",
        "point",
        "polar",
        "rotation",
        "visibility",
        "xy",
    ] {
        assert!(have(&format!("dxf-block{name}parameter")), "{name}");
    }
    for id in [
        "dxf-geoloc",
        "dxf-geoloc-ac1021",
        "dxf-sample-ac1009-ascii",
        "dxf-sample-ac1009-binary",
    ] {
        assert!(have(id), "{id}");
    }
}

#[test]
fn every_file_is_the_one_the_manifest_froze_and_the_strict_parser_reads_it() {
    for g in fixtures::ground_truths()
        .iter()
        .filter(|g| g.failed.is_none())
    {
        let jsonl =
            String::from_utf8(g.load_jsonl().unwrap_or_else(|e| panic!("{}: {e}", g.id))).unwrap();
        let records = parse_stream(&jsonl).unwrap_or_else(|e| panic!("{}: {e}", g.id));
        assert_eq!(records.len() as u64, g.records, "{}", g.id);
        assert!(matches!(records[0], Record::Document(_)), "{}", g.id);
        let side = String::from_utf8(g.load_sidecar().unwrap()).unwrap();
        Sidecar::parse(&side, &records).unwrap_or_else(|e| panic!("{}: {e}", g.id));
        assert_eq!(g.schema, 1);
    }
}

#[test]
fn run_json_carries_the_version_the_requirements_hash_and_the_image_digest() {
    let want = requirements_sha256();
    for g in fixtures::ground_truths()
        .iter()
        .filter(|g| g.failed.is_none())
    {
        let run: serde_json::Value = serde_json::from_slice(&g.load_run().unwrap()).unwrap();
        assert_eq!(run["ezdxf_version"], "1.4.4", "{}", g.id);
        assert_eq!(
            run["requirements_sha256"],
            want.as_str(),
            "{}: the requirements moved without a freeze",
            g.id
        );
        let image = run["image"].as_str().unwrap();
        assert!(
            image.contains("@sha256:") && image.len() > 70,
            "{}: image {image:?} is not a digest",
            g.id
        );
        assert_eq!(run["input"], g.input.as_str());
        let input_sha = match fixtures::oracle_inputs().iter().find(|i| i.id == g.input) {
            Some(i) => i.sha256.as_str(),
            None => fixtures::written_by_id(&g.input)
                .unwrap_or_else(|| panic!("{}: no such DXF", g.input))
                .sha256
                .as_str(),
        };
        assert_eq!(
            run["input_sha256"], input_sha,
            "{}: made from other bytes",
            g.id
        );
        // The manifest repeats what run.json says, and the two cannot differ.
        assert_eq!(run["ezdxf_version"], g.ezdxf_version.as_str());
        assert_eq!(run["requirements_sha256"], g.requirements_sha256.as_str());
        assert_eq!(run["image"], g.image.as_str());
    }
}

#[test]
fn the_ascii_and_binary_twins_of_a_save_read_the_same() {
    // The same drawing in two encodings: ezdxf's loader for each, and the
    // extraction has to agree to the comparator's bound. A binary DXF keeps the
    // doubles AutoCAD held and an ASCII one keeps sixteen digits, so the last
    // digit may differ; anything more is the loader's, not the drawing's.
    let read = |id: &str| {
        let g = fixtures::ground_truth(id).unwrap();
        let records = parse_stream(&String::from_utf8(g.load_jsonl().unwrap()).unwrap()).unwrap();
        let sidecar = Sidecar::parse(
            &String::from_utf8(g.load_sidecar().unwrap()).unwrap(),
            &records,
        )
        .unwrap();
        Stream {
            records,
            sidecar: Some(sidecar),
        }
    };
    let table = ClassTable::for_pair(Pair::new(Engine::Ezdxf, Engine::Ezdxf), WarningRow::Compare);
    // The one place AutoCAD's own two files for a save differ. From the R2004
    // save on, its ASCII DXF writes the paragraph break of four MTEXTs as a line
    // feed (`^J`) and its binary DXF as the escape `\P`. Both are AutoCAD's
    // writing, the DWG reads as a line feed, and the schema carries what the
    // file stores, so it is listed and not smoothed over.
    let known: Vec<(String, String, String)> = ["1018", "1021", "1024", "1027", "1032"]
        .iter()
        .flat_map(|v| {
            ["3EC", "3F5", "3F6", "779"]
                .iter()
                .map(move |h| (format!("AC{v}"), h.to_string(), "text.text".to_string()))
        })
        .collect();
    let mut seen = Vec::new();
    for v in ["1015", "1018", "1021", "1024", "1027", "1032"] {
        let a = read(&format!("dxf-sample-ac{v}-ascii"));
        let b = read(&format!("dxf-sample-ac{v}-binary"));
        let save = format!("AC{v}");
        let report = compare(&save, &a, &b, &table).unwrap();
        for f in report.red() {
            let handle = f
                .handles_a
                .as_ref()
                .and_then(|h| h.last())
                .cloned()
                .unwrap_or_default();
            seen.push((
                save.clone(),
                handle,
                format!("{}.{}", f.record_type, f.field),
            ));
        }
    }
    assert_eq!(
        seen, known,
        "the ASCII and binary DXF of a save differ only where the list above says"
    );
}

#[test]
fn ezdxf_over_autocads_dxf_agrees_with_the_oracle_on_the_ac1015_save() {
    // The anchor that says the extractor reads what the oracle reads: two
    // readers that share no code, one record each for the same drawing. Only
    // the container name differs.
    let oracle = fixtures::by_id("acadsharp-ac1015")
        .unwrap()
        .load_reference_jsonl()
        .unwrap();
    let a = Stream::new(parse_stream(&String::from_utf8(oracle).unwrap()).unwrap());
    let g = fixtures::ground_truth("dxf-sample-ac1015-ascii").unwrap();
    let records = parse_stream(&String::from_utf8(g.load_jsonl().unwrap()).unwrap()).unwrap();
    let sidecar = Sidecar::parse(
        &String::from_utf8(g.load_sidecar().unwrap()).unwrap(),
        &records,
    )
    .unwrap();
    let b = Stream {
        records,
        sidecar: Some(sidecar),
    };

    let mut table = ClassTable::for_pair(
        Pair::new(Engine::Oracle, Engine::Ezdxf),
        WarningRow::Excluded {
            reason: "ezdxf names an unsupported entity by its DXF name, the oracle by its ACadSharp class".into(),
            issue: "libviprs/acadsharp-rs-tests#25".into(),
        },
    );
    table.excluded.push(Exclusion {
        scope: Scope::Field {
            record: "document".into(),
            field: "format".into(),
        },
        reason: "one reads a DWG and the other a DXF".into(),
        issue: "libviprs/acadsharp-rs-tests#24".into(),
    });
    let report = compare("acadsharp-ac1015", &a, &b, &table).unwrap();
    assert!(
        report.is_match(),
        "{}",
        report
            .red()
            .iter()
            .map(|f| f.render())
            .collect::<Vec<_>>()
            .join("\n")
    );
    // Not vacuous: hundreds of records took part.
    assert!(report.aligned.len() > 250, "{}", report.aligned.len());
}
