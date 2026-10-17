//! Writes the seed corpora under `fuzz/corpus/<target>/`.
//!
//! ```text
//! cargo run --example fuzz_seeds
//! ```
//!
//! The corpus is committed, so a fuzzer starts from the interesting region
//! instead of from zero bytes, and so `tests/fuzz_corpus_replay.rs` has
//! something to replay on every PR. This program is how it was made and how it
//! is regenerated: deterministic, no clock, no randomness beyond a fixed
//! generator. It does not delete anything. Entries a fuzzer found and somebody
//! added by hand stay where they are.
//!
//! The batch seeds are built with the wire builder in `acadsharp-rs`'s own test
//! tree (`tests/wire/mod.rs`), pulled in by path, so the bytes are the ones that
//! crate's tests use and nothing here re-derives the layout.

#[path = "../../acadsharp-rs/tests/wire/mod.rs"]
#[allow(unused_imports)]
mod wire;

use std::fs;
use std::path::{Path, PathBuf};

use acadsharp_rs_tests::fixtures;
use wire::{Builder, canonical, frame_raw};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fuzz/corpus")
}

fn put(target: &str, name: &str, bytes: &[u8]) {
    let dir = root().join(target);
    fs::create_dir_all(&dir).expect("create corpus dir");
    fs::write(dir.join(name), bytes).expect("write seed");
}

fn batch_seeds() {
    let data = Path::new(env!("CARGO_MANIFEST_DIR")).join("../acadsharp-rs/tests/data");
    for name in ["syn_1v_1p", "syn_1v_12p", "syn_2v_3p", "syn_3v_40p"] {
        let bytes =
            fs::read(data.join(format!("{name}.bin"))).expect("the crate's captured batches");
        put("batch_reader", &format!("capture_{name}.bin"), &bytes);
    }

    let p3 = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [1.0, 1.0, 0.0]];
    let good = Builder::new()
        .record(wire::document_begin(1, 1032))
        .record(wire::view_begin(0, 0, [0.0, 0.0, 10.0, 10.0], 5, "Model"))
        .record(wire::line(1, [0.0, 0.0, 0.0, 1.0, 1.0, 0.0]))
        .record(wire::polyline(
            2,
            true,
            [0.0, 0.0, 1.0],
            &p3,
            &[0.0, 0.5, 0.0],
        ))
        .record(wire::circle(3, [0.0; 3], 2.0, [0.0, 0.0, 1.0]))
        .record(wire::text(4, [1.0, 2.0, 0.0], 2.5, 0.0, "ROOM 101 \u{e9}"))
        .record(wire::warning(100, 5, "POINT is not a primitive"))
        .record(wire::view_end(0, 5))
        .record(wire::document_end(8, 1))
        .build();
    put("batch_reader", "built_every_kind.bin", &good);
    put(
        "batch_reader",
        "built_forward_probe.bin",
        &Builder::new()
            .record(wire::forward_probe())
            .record(canonical(3))
            .build(),
    );

    let one = || Builder::new().record(canonical(3));
    put(
        "batch_reader",
        "bad_magic.bin",
        &one().magic(*b"NOPE").build(),
    );
    put(
        "batch_reader",
        "bad_wire_version_1.bin",
        &one().wire_version(1).build(),
    );
    put(
        "batch_reader",
        "bad_wire_version_future.bin",
        &one().wire_version(999).build(),
    );
    put("batch_reader", "short_header.bin", &good[..7]);
    put("batch_reader", "empty.bin", &[]);
    put(
        "batch_reader",
        "payload_past_buffer.bin",
        &one().declared_payload_length(1_000_000).build(),
    );
    put(
        "batch_reader",
        "short_record_header.bin",
        &one().raw(&[0xAA, 0xBB, 0xCC, 0xDD]).build(),
    );
    put(
        "batch_reader",
        "length_below_header.bin",
        &Builder::new()
            .record(frame_raw(3, 0, 4, &[0u8; 16]))
            .build(),
    );
    put(
        "batch_reader",
        "length_zero.bin",
        &Builder::new()
            .record(frame_raw(3, 0, 0, &[0u8; 16]))
            .build(),
    );
    put(
        "batch_reader",
        "length_not_multiple_of_four.bin",
        &Builder::new()
            .record(frame_raw(3, 0, 85, &[0u8; 77]))
            .build(),
    );
    put(
        "batch_reader",
        "length_past_payload.bin",
        &Builder::new()
            .record(frame_raw(3, 0, 4096, &[0u8; 72]))
            .build(),
    );
    put(
        "batch_reader",
        "reserved_nonzero.bin",
        &Builder::new()
            .record(frame_raw(3, 7, 80, &[0u8; 72]))
            .build(),
    );
    put(
        "batch_reader",
        "polyline_bulge_count.bin",
        &Builder::new()
            .record(wire::polyline_raw(
                1,
                2,
                0,
                1,
                0,
                &[0.0, 0.0, 1.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 0.5],
            ))
            .build(),
    );
    put(
        "batch_reader",
        "polyline_length_mismatch.bin",
        &Builder::new()
            .record(wire::polyline_raw(
                1,
                3,
                0,
                0,
                0,
                &[0.0, 0.0, 1.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0],
            ))
            .build(),
    );
    // Claims 2^31 points and carries three doubles. The reader has to refuse it
    // by arithmetic, not by trying to find room for them.
    put(
        "batch_reader",
        "polyline_two_pow_31_points.bin",
        &Builder::new()
            .record(wire::polyline_raw(
                1,
                0x8000_0000,
                0,
                0,
                0,
                &[0.0, 0.0, 1.0],
            ))
            .build(),
    );
    put(
        "batch_reader",
        "polyline_u32_wrap.bin",
        &Builder::new().record(wire::polyline_u32_wrap()).build(),
    );
}

/// A deterministic byte source, so a regenerated corpus is byte-identical.
struct Lcg(u64);

impl Lcg {
    fn bytes(&mut self, n: usize) -> Vec<u8> {
        (0..n)
            .map(|_| {
                self.0 = self
                    .0
                    .wrapping_mul(6364136223846793005)
                    .wrapping_add(1442695040888963407);
                (self.0 >> 33) as u8
            })
            .collect()
    }
}

/// One record with the final newline the strict reader insists on.
fn line(record: &[u8]) -> Vec<u8> {
    let mut v = record.to_vec();
    v.push(b'\n');
    v
}

fn canonical_seeds() {
    // The first lines of four frozen expected outputs: whole streams are
    // megabytes and a fuzzer gains nothing from the tail.
    let mut firsts: Vec<String> = Vec::new();
    for (n, f) in fixtures::all()
        .iter()
        .filter(|f| f.reference_jsonl.is_some())
        .take(4)
        .enumerate()
    {
        let text = String::from_utf8(f.load_reference_jsonl().expect("verified expected output"))
            .expect("utf-8");
        let head: String = text.lines().take(48).map(|l| format!("{l}\n")).collect();
        put(
            "canonical_parser",
            &format!("expected_{}.jsonl", f.id),
            head.as_bytes(),
        );
        if n == 0 {
            firsts = text.lines().take(12).map(str::to_string).collect();
        }
    }
    let base = firsts.join("\n") + "\n";
    let doc = &firsts[0];
    let mutate = |name: &str, from: &str, to: &str, text: &str| {
        assert!(
            text.contains(from),
            "{name}: the seed no longer contains {from:?}"
        );
        put(
            "canonical_parser",
            name,
            text.replacen(from, to, 1).as_bytes(),
        );
    };
    mutate("schema_2.jsonl", "\"schema\":1", "\"schema\":2", &base);
    mutate(
        "unknown_record.jsonl",
        "\"record\":\"document\"",
        "\"record\":\"hologram\"",
        &base,
    );
    mutate(
        "unknown_field.jsonl",
        "\"format\":\"dwg\"",
        "\"format\":\"dwg\",\"extra\":1",
        &base,
    );
    mutate("missing_field.jsonl", "\"format\":\"dwg\",", "", &base);
    mutate(
        "wrong_type.jsonl",
        "\"maintenance_version\":",
        "\"maintenance_version\":\"",
        &base,
    );
    put(
        "canonical_parser",
        "no_trailing_newline.jsonl",
        base.trim_end().as_bytes(),
    );
    put(
        "canonical_parser",
        "crlf.jsonl",
        base.replace('\n', "\r\n").as_bytes(),
    );
    put(
        "canonical_parser",
        "blank_line.jsonl",
        format!("{doc}\n\n{doc}\n").as_bytes(),
    );
    put("canonical_parser", "negative_zero.jsonl", &line(br#"{"schema":1,"record":"line","view":0,"x1":-0.0,"y1":0.0,"z1":0.0,"x2":1.0,"y2":0.0,"z2":0.0}"#));
    put("canonical_parser", "nan.jsonl", &line(br#"{"schema":1,"record":"line","view":0,"x1":NaN,"y1":0.0,"z1":0.0,"x2":1.0,"y2":0.0,"z2":0.0}"#));
    put("canonical_parser", "huge_exponent.jsonl", &line(br#"{"schema":1,"record":"circle","view":0,"cx":1e999,"cy":0.0,"cz":0.0,"radius":1.0,"nx":0.0,"ny":0.0,"nz":1.0}"#));
    put(
        "canonical_parser",
        "bulge_length_mismatch.jsonl",
        &line(br#"{"schema":1,"record":"polyline","view":0,"closed":false,"nx":0.0,"ny":0.0,"nz":1.0,"points":[[0.0,0.0,0.0],[1.0,0.0,0.0]],"bulges":[0.0]}"#),
    );
    put("canonical_parser", "not_json.jsonl", b"{{{{ not json");
    put(
        "canonical_parser",
        "invalid_utf8.jsonl",
        &[0xff, 0xfe, b'{', b'}'],
    );
    put("canonical_parser", "empty.jsonl", b"");
}

fn manifest_seeds() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    for (target, file) in [
        ("fixture_manifest", "fixtures/manifest.toml"),
        ("parity_manifest", "upstream_parity/manifest.toml"),
    ] {
        let text = fs::read_to_string(root.join(file)).expect("committed manifest");
        put(target, "committed.toml", text.as_bytes());
        // A prefix cut on a line boundary near the middle: valid TOML that stops
        // part way through a table.
        let cut = text[..text.len() / 2].rfind('\n').unwrap_or(0);
        put(target, "half.toml", &text.as_bytes()[..cut]);
        put(target, "empty.toml", b"");
        put(
            target,
            "unterminated_string.toml",
            b"[upstream]\nrepository = \"oops\n",
        );
        put(
            target,
            "wrong_type_row.toml",
            b"[[fixture]]\nid = 1\n[[test_file]]\npath = []\n",
        );
        put(
            target,
            "duplicate_key.toml",
            b"[upstream]\nversion = \"1\"\nversion = \"2\"\n",
        );
        put(
            target,
            "deep_inline_table.toml",
            format!("x = {}1{}\n", "{a=".repeat(40), "}".repeat(40)).as_bytes(),
        );
        put(target, "invalid_utf8.toml", &[b'[', 0xc3, 0x28, b']']);
    }
}

fn comparator_seeds() {
    let mut g = Lcg(0x5eed_acad);
    for n in 0..10 {
        let len = [0, 1, 8, 32, 64, 128, 256, 512, 1024, 2048][n];
        put("comparator", &format!("bytes_{n:02}.bin"), &g.bytes(len));
    }
    // The same bytes twice, so the two sequences come out related.
    let once = g.bytes(96);
    let twice = [once.clone(), once].concat();
    put("comparator", "related_pair.bin", &twice);
    put("comparator", "all_zero.bin", &[0u8; 256]);
    put("comparator", "all_ff.bin", &[0xffu8; 256]);
}

fn main() {
    batch_seeds();
    canonical_seeds();
    manifest_seeds();
    comparator_seeds();
    println!("seeds written under {}", root().display());
}
