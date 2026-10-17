//! AutoCAD's R12 twin as a code page case, and the DWG stub as a refusal (R5.2,
//! acadsharp-rs-tests#28).
//!
//! The R12 DXF declares no `$DWGCODEPAGE` (R12 has none) and carries one raw byte:
//! the diameter callout at handle 64B is `0xD8` then `45,6` (AutoCAD best-fitted
//! `∅` to `Ø`), and the two `94\U+00B0` texts at 634 and 63E came out as `94?`.
//! This is R5.4's question asked of a file AutoCAD wrote, so the answer lives here
//! and R5.4 links it: what each reader returns for those three strings, and which
//! code page it assumed. `parity/r12_codepage.toml` is that record, and these tests
//! hold it to what the readers say today.
#![cfg(feature = "acadrust")]

use std::collections::BTreeMap;

use acadsharp_rs_tests::acadrust_emit::extract_dxf_bytes;
use acadsharp_rs_tests::canonical::{Record, Sidecar, SidecarEntry, parse_stream};
use acadsharp_rs_tests::fixtures::{self, ManifestFile};

const RECORD: &str = include_str!("../parity/r12_codepage.toml");
/// The three handles the issue names: the callout and the two degree texts.
const HANDLES: [&str; 3] = ["64B", "634", "63E"];

/// Text of every text record whose leaf handle is one of [`HANDLES`], by handle.
fn by_handle(records: &[Record], sidecar: &[SidecarEntry]) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for (i, r) in records.iter().enumerate() {
        if let Record::Text(t) = r {
            // A dimension's text sits in its block: the handle path is the insert,
            // the dimension (64B) and the text, so match anywhere along it.
            if let Some(e) = sidecar.get(i) {
                if let Some(h) = e.handles.iter().find(|h| HANDLES.contains(&h.as_str())) {
                    out.insert(h.clone(), t.text.clone());
                }
            }
        }
    }
    out
}

fn readings(input: &str) -> BTreeMap<&'static str, BTreeMap<String, String>> {
    let mut out = BTreeMap::new();
    let row = fixtures::oracle_inputs()
        .iter()
        .find(|i| i.id == input)
        .unwrap();
    // acadrust, live.
    let emission = extract_dxf_bytes(&row.load().unwrap()).unwrap();
    out.insert("acadrust", by_handle(&emission.records, &emission.sidecar));
    // ezdxf, frozen.
    let gt = fixtures::ground_truth(input).unwrap();
    let records = parse_stream(&String::from_utf8(gt.load_jsonl().unwrap()).unwrap()).unwrap();
    let sidecar = Sidecar::parse(
        &String::from_utf8(gt.load_sidecar().unwrap()).unwrap(),
        &records,
    )
    .unwrap();
    out.insert("ezdxf", by_handle(&records, &sidecar.entries));
    out
}

#[test]
fn print_the_r12_strings() {
    let _ = RECORD;
    if std::env::var("R12_PRINT").as_deref() != Ok("1") {
        return;
    }
    for input in ["dxf-sample-ac1009-ascii", "dxf-sample-ac1009-binary"] {
        for (engine, m) in readings(input) {
            println!("R12 {input} {engine} {m:?}");
        }
        // The oracle has no sidecar: every text with 45,6 or 94 in it, in order.
        let row = fixtures::oracle_inputs().iter().find(|i| i.id == input).unwrap();
        let recs = parse_stream(&String::from_utf8(row.load_reference_jsonl().unwrap()).unwrap()).unwrap();
        for r in recs {
            if let Record::Text(t) = r {
                if t.text.contains("45") || t.text.starts_with("94") {
                    println!("R12 {input} oracle {:?}", t.text);
                }
            }
        }
    }
}
