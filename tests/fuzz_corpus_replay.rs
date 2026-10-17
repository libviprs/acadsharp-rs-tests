//! Every committed corpus entry, replayed through the fuzz targets' own bodies
//! on stable. A crash the fuzzer once found and somebody added to the corpus
//! fails here on every PR, which is the point of committing it.
//!
//! Also run by the Miri job, so the pure-Rust parsers get a second opinion on
//! exactly these inputs. Miri does not see foreign code and this file has none.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;

use acadsharp_rs_tests::fuzz_harness::{CappedAlloc, TARGETS};

#[global_allocator]
static ALLOC: CappedAlloc = CappedAlloc;

const EXPECTED: [&str; 5] = [
    "batch_reader",
    "canonical_parser",
    "comparator",
    "fixture_manifest",
    "parity_manifest",
];

fn corpus(target: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("fuzz/corpus")
        .join(target)
}

fn entries(target: &str) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(corpus(target))
        .map(|d| d.filter_map(|e| e.ok()).map(|e| e.path()).collect())
        .unwrap_or_default();
    v.retain(|p| p.is_file());
    v.sort();
    v
}

#[test]
fn the_five_targets_are_registered() {
    let mut names: Vec<&str> = TARGETS.iter().map(|t| t.0).collect();
    names.sort_unstable();
    assert_eq!(names, EXPECTED, "TARGETS and the issue's five targets differ");
}

#[test]
fn every_target_has_a_seed_corpus() {
    for t in EXPECTED {
        let n = entries(t).len();
        assert!(
            n >= 3,
            "fuzz/corpus/{t} has {n} entries; a target that starts from nothing finds \
             the shallow bugs only. Regenerate with `cargo run --example fuzz_seeds`."
        );
    }
}

#[test]
fn every_corpus_entry_replays_clean() {
    let mut failures = Vec::new();
    let mut replayed = 0usize;
    for (name, body) in TARGETS {
        for path in entries(name) {
            let data = std::fs::read(&path).expect("read corpus entry");
            replayed += 1;
            if catch_unwind(AssertUnwindSafe(|| body(&data))).is_err() {
                failures.push(format!("{name}: {}", path.display()));
            }
        }
    }
    assert!(replayed > 0, "no corpus entry was replayed; this test proved nothing");
    assert!(failures.is_empty(), "these corpus entries panic:\n{}", failures.join("\n"));
}
