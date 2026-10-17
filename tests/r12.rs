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

#[derive(serde::Deserialize)]
struct Row {
    input: String,
    reader: String,
    strings: Vec<String>,
    assumed: String,
}

#[derive(serde::Deserialize)]
struct Record12 {
    row: Vec<Row>,
}

#[test]
fn each_reader_returns_what_the_record_says_for_the_callout_and_the_degree_texts() {
    let record: Record12 = toml::from_str(RECORD).unwrap();
    assert_eq!(record.row.len(), 6, "two twins, three readers");
    for input in ["dxf-sample-ac1009-ascii", "dxf-sample-ac1009-binary"] {
        let got = readings(input);
        for engine in ["oracle", "acadrust", "ezdxf"] {
            let row = record
                .row
                .iter()
                .find(|r| r.input == input && r.reader == engine)
                .unwrap_or_else(|| panic!("no row for {engine} on {input}"));
            assert!(!row.assumed.trim().is_empty(), "{engine} {input}: which code page?");
            assert_eq!(
                got[engine], row.strings,
                "{engine} on {input} returns other strings than the record says (the code page, the raw bytes and both decodings are in parity/r12_codepage.toml)"
            );
        }
    }
}

#[test]
fn the_degree_texts_are_a_question_mark_in_every_reader_and_so_in_the_file() {
    // The file holds "94?", so no decoder can recover a sign that the lowering lost:
    // all three agree, and none of them is wrong.
    for input in ["dxf-sample-ac1009-ascii", "dxf-sample-ac1009-binary"] {
        for (engine, strings) in readings(input) {
            assert_eq!(
                strings.iter().filter(|s| s.as_str() == "94?").count(),
                2,
                "{engine} on {input}"
            );
        }
    }
}

#[test]
fn print_the_r12_strings() {
    if std::env::var("R12_PRINT").as_deref() != Ok("1") {
        return;
    }
    for input in ["dxf-sample-ac1009-ascii", "dxf-sample-ac1009-binary"] {
        for (engine, m) in readings(input) {
            println!("R12 {input} {engine} {m:?}");
        }
    }
}

// ---- the AC1009 DWG stub ---------------------------------------------------------------

/// Run an emitter on bytes, the way the parity run does (on stdin), and say whether
/// it read them and what it said.
fn run_emitter(bin: &str, name: &str, bytes: &[u8]) -> (bool, String) {
    use std::io::Write;
    use std::process::{Command, Stdio};
    let out = std::env::temp_dir().join(format!("r12-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&out).unwrap();
    let mut child = Command::new(bin)
        .args(["--fixture", "refusal-probe", "--stdin-as", "dwg", "--out"])
        .arg(&out)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let data = bytes.to_vec();
    let feeder = std::thread::spawn(move || {
        let _ = stdin.write_all(&data);
    });
    let run = child.wait_with_output().unwrap();
    let _ = feeder.join();
    let _ = std::fs::remove_dir_all(&out);
    (
        run.status.success(),
        String::from_utf8_lossy(&run.stderr).trim().to_string(),
    )
}

/// What each engine must say about the six-byte `AC1009` stub, written out. The
/// oracle's is the manifest's own (`oracle_exit`, frozen by `regenerate_reference.py`).
const REFUSALS: [(&str, &str); 2] = [("acadrust", "AC1009"), ("acadsharp", "does not read that drawing format")];

#[test]
fn the_ac1009_dwg_stub_is_refused_by_every_engine_that_is_asked() {
    let stub = fixtures::refusals()
        .iter()
        .find(|r| r.id == "g13_ac1009")
        .expect("the AC1009 stub is a refusal fixture");
    // The oracle (and so ACadSharp, which acadsharp-rs shares) refuses it with the
    // exit code the row froze; `regenerate_reference.py verify` is where that is run.
    assert_eq!(stub.oracle_exit, "UnsupportedVersion");
    assert_eq!(stub.oracle_exit_code, 4);
    let bytes = stub.load().unwrap();
    assert_eq!(&bytes[..6], b"AC1009", "a DWG stub that says R12");
    // ezdxf is not asked: it never opens a DWG.
    for (engine, must) in REFUSALS {
        let bin = match engine {
            "acadrust" => env!("CARGO_BIN_EXE_emit-acadrust"),
            _ => env!("CARGO_BIN_EXE_emit-acadsharp"),
        };
        let (read, said) = run_emitter(bin, engine, &bytes);
        assert!(
            !read,
            "{engine} read an AC1009 DWG, and a refusal row that comes back as a read is red"
        );
        if std::env::var("R12_PRINT").as_deref() == Ok("1") {
            println!("REFUSAL {engine}: {said}");
        }
        assert!(said.contains(must), "{engine} refused it, but said {said:?}");
    }
}

#[test]
fn the_refusal_harness_reads_a_good_drawing_so_a_refusal_means_something() {
    // The positive control: the same path, a real AC1032 DWG, is read by acadrust.
    let good = fixtures::load("g13_line").unwrap();
    let (read, said) = run_emitter(env!("CARGO_BIN_EXE_emit-acadrust"), "control", &good);
    assert!(read, "acadrust must read g13_line through the same door: {said}");
}
