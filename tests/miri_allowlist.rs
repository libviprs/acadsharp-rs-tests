//! The Miri job's list of test targets is explicit, and nothing on it may link
//! the native library or a stub of it.
//!
//! Miri cannot run foreign code. A job that quietly skipped such a test would be
//! the false-green shape this org keeps finding, so the list is a file
//! (`miri-allowlist.txt`), the CI job reads that file, and this test reads the
//! same file and refuses a target whose source reaches the native side.

use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Text that only appears in code reaching the native library, a stub of it, or
/// the lane that provides one.
const NATIVE_MARKERS: &[&str] = &[
    "acadsharp_rs::ffi",
    "Decoder",
    "Document::open",
    "ACADSHARP_NATIVE_DIR",
    "VIPRS_REQUIRE_NATIVE",
    "feature = \"native\"",
    "link-static",
    "viprs_acad_",
    "libloading",
    "extern \"C\"",
];

/// What is wrong with a source that is on the allowlist.
fn violations(name: &str, source: &str) -> Vec<String> {
    let mut out = Vec::new();
    for marker in NATIVE_MARKERS {
        if source.contains(marker) {
            out.push(format!("{name} mentions `{marker}`, which needs the native library"));
        }
    }
    // The only part of the crate Miri can run is the pure wire parser.
    for (i, _) in source.match_indices("acadsharp_rs::") {
        let rest = &source[i + "acadsharp_rs::".len()..];
        if !rest.starts_with("batch") {
            let tail: String = rest.chars().take(24).collect();
            out.push(format!("{name} reaches acadsharp_rs::{tail}..., and only `batch` is pure Rust"));
        }
    }
    out
}

fn allowlist() -> Vec<String> {
    let text = std::fs::read_to_string(root().join("miri-allowlist.txt"))
        .expect("miri-allowlist.txt is the Miri job's list of test targets");
    text.lines()
        .map(|l| l.split('#').next().unwrap_or("").trim().to_string())
        .filter(|l| !l.is_empty())
        .collect()
}

fn sources_of(target: &str) -> Vec<(String, String)> {
    let read = |p: &Path| (p.display().to_string(), std::fs::read_to_string(p).unwrap_or_default());
    if target == "lib" {
        let mut v = Vec::new();
        for e in std::fs::read_dir(root().join("src")).unwrap().flatten() {
            let p = e.path();
            if p.extension().is_some_and(|x| x == "rs") {
                v.push(read(&p));
            }
        }
        return v;
    }
    let mut v = vec![read(&root().join("tests").join(format!("{target}.rs")))];
    // Shared helpers come in through `mod support;`.
    if v[0].1.contains("mod support") {
        v.push(read(&root().join("tests/support/mod.rs")));
    }
    v
}

#[test]
fn the_allowlist_is_explicit_and_nonempty() {
    let list = allowlist();
    for need in ["fuzz_corpus_replay", "canonical_parser", "comparator"] {
        assert!(list.iter().any(|t| t == need), "{need} is missing from miri-allowlist.txt");
    }
}

#[test]
fn every_listed_target_exists() {
    for t in allowlist() {
        if t != "lib" {
            assert!(
                root().join("tests").join(format!("{t}.rs")).is_file(),
                "miri-allowlist.txt lists {t}, and tests/{t}.rs does not exist"
            );
        }
    }
}

#[test]
fn nothing_on_the_allowlist_reaches_the_native_library() {
    let mut bad = Vec::new();
    for t in allowlist() {
        for (name, src) in sources_of(&t) {
            bad.extend(violations(&name, &src));
        }
    }
    assert!(bad.is_empty(), "Miri cannot run these:\n{}", bad.join("\n"));
}

#[test]
fn the_guard_names_a_stub_linking_test() {
    // The teeth: a source that links the stub is named, and a pure one is not.
    let linked = "use acadsharp_rs::{Decoder, Document};\nfn t() { Decoder::new(); }";
    assert!(!violations("tests/stubby.rs", linked).is_empty());
    let ffi = "use acadsharp_rs::ffi::open;";
    assert!(!violations("tests/ffi.rs", ffi).is_empty());
    let pure = "use acadsharp_rs::batch::BatchReader;";
    assert!(violations("tests/pure.rs", pure).is_empty());
}

#[test]
fn the_ci_miri_job_runs_exactly_that_list_and_links_nothing() {
    let ci = std::fs::read_to_string(root().join(".github/workflows/ci.yml")).unwrap();
    let start = ci.find("\n  miri:").expect("ci.yml has no `miri` job");
    let rest = &ci[start + 1..];
    let end = rest[1..]
        .find("\n  ")
        .and_then(|_| {
            // The next job key at two-space indent.
            rest.match_indices("\n  ")
                .skip(1)
                .find(|(i, _)| rest[i + 3..].chars().next().is_some_and(|c| c != ' ' && c != '#'))
                .map(|(i, _)| i)
        })
        .unwrap_or(rest.len());
    let job = &rest[..end];
    assert!(job.contains("miri-allowlist.txt"), "the miri job does not read miri-allowlist.txt");
    assert!(job.contains("-Zmiri-strict-provenance"), "MIRIFLAGS lacks -Zmiri-strict-provenance");
    for banned in ["ACADSHARP_NATIVE_DIR", "--features native", "link-static", "--all-targets"] {
        let code: String = job.lines().filter(|l| !l.trim_start().starts_with('#')).collect();
        assert!(!code.contains(banned), "the miri job uses `{banned}`");
    }
}

#[test]
fn no_document_claims_miri_validates_the_native_side() {
    for file in ["README.md", "fuzz/README.md", ".github/workflows/ci.yml", ".github/workflows/fuzz.yml"] {
        let text = std::fs::read_to_string(root().join(file)).unwrap_or_default();
        for (n, line) in text.lines().enumerate() {
            let l = line.to_lowercase();
            if l.contains("miri") && (l.contains("nativeaot") || l.contains(" abi")) {
                let denies = ["not", "cannot", "never", "no ", "doesn't", "does n"].iter().any(|w| l.contains(w));
                assert!(denies, "{file}:{} says Miri covers the native side: {line}", n + 1);
            }
        }
    }
}
