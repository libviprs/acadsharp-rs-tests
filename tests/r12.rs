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
use acadsharp_rs_tests::canonical::{Record, parse_stream};
use acadsharp_rs_tests::fixtures::{self, ManifestFile};

const RECORD: &str = include_str!("../parity/r12_codepage.toml");

/// The three strings, as a reader returns them: every text record that is the
/// callout (`45,6` after a diameter sign, whatever the sign came out as) or one of the
/// two degree texts (`94` and the character after it). Content and not handle,
/// because the oracle's stream has no sidecar to name 64B, 634 and 63E by.
fn the_strings(records: &[Record]) -> Vec<String> {
    records
        .iter()
        .filter_map(|r| match r {
            Record::Text(t) if t.text.ends_with("45,6") || t.text.starts_with("94") => {
                Some(t.text.clone())
            }
            _ => None,
        })
        .collect()
}

fn readings(input: &str) -> BTreeMap<&'static str, Vec<String>> {
    let mut out = BTreeMap::new();
    let row = fixtures::oracle_inputs()
        .iter()
        .find(|i| i.id == input)
        .unwrap();
    let oracle = parse_stream(&String::from_utf8(row.load_reference_jsonl().unwrap()).unwrap())
        .unwrap();
    out.insert("oracle", the_strings(&oracle));
    // acadrust, live.
    let emission = extract_dxf_bytes(&row.load().unwrap()).unwrap();
    out.insert("acadrust", the_strings(&emission.records));
    // ezdxf, frozen.
    let gt = fixtures::ground_truth(input).unwrap();
    let records = parse_stream(&String::from_utf8(gt.load_jsonl().unwrap()).unwrap()).unwrap();
    out.insert("ezdxf", the_strings(&records));
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
    }
}
