//! The committed DWG corpus is what every differential test compares against,
//! so its bytes have to be exactly what the manifest says before any of those
//! comparisons mean anything.
//!
//! These run offline under an ordinary `cargo test`: no network, no .NET, no
//! ACadSharp. The committed corpus is authoritative, and a fetch that
//! disagrees with it is a decision for a human rather than something CI
//! resolves on its own.
//!
//! Each check below is paired with the state that makes it fail, because a
//! fixture guard that cannot go red is worse than none: it would let an empty
//! `fixtures/dwg/` read as a healthy corpus.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use acadsharp_rs_tests::fixtures;
use sha2::{Digest, Sha256};

/// Every DWG generation ACadSharp's `DwgReader` documents as readable.
///
/// Spelled out rather than derived from the manifest on purpose. Deriving it
/// would make the check vacuous: deleting a fixture would delete the
/// expectation with it and the suite would stay green on a shrunken corpus,
/// which is exactly the accident this exists to catch.
const EXPECTED_VERSIONS: &[&str] = &[
    "AC1014", "AC1015", "AC1018", "AC1021", "AC1024", "AC1027", "AC1032",
];

/// Smallest plausible DWG. The real samples are around a megabyte, so this
/// only has to be large enough to reject an error page or a truncated write.
const MIN_PLAUSIBLE_BYTES: u64 = 1024;

#[test]
fn the_corpus_covers_exactly_the_supported_versions() {
    let got: BTreeSet<&str> = fixtures::all()
        .iter()
        .map(|f| f.acad_version.as_str())
        .collect();
    let want: BTreeSet<&str> = EXPECTED_VERSIONS.iter().copied().collect();

    let missing: Vec<_> = want.difference(&got).collect();
    let extra: Vec<_> = got.difference(&want).collect();

    assert!(
        missing.is_empty(),
        "the corpus is missing {missing:?}. A deleted fixture silently reduces \
         differential coverage, which is the whole reason this list is written \
         out rather than derived."
    );
    assert!(
        extra.is_empty(),
        "the corpus carries {extra:?}, which is not in the supported set. Add it \
         to EXPECTED_VERSIONS with a reason, or remove the fixture."
    );
}

#[test]
fn the_upstream_pin_is_an_exact_commit() {
    let up = fixtures::upstream();
    assert_eq!(
        up.commit.len(),
        40,
        "upstream.commit is {:?}, which is not a full sha. An abbreviated or \
         symbolic ref is not a pin.",
        up.commit
    );
    assert!(
        up.commit
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()),
        "upstream.commit {:?} is not lowercase hex",
        up.commit
    );
    for fx in fixtures::all() {
        assert!(
            fx.source_url.contains(&up.commit),
            "{}: source_url does not carry the pinned commit, so it does not \
             describe where these bytes came from: {}",
            fx.id,
            fx.source_url
        );
        for floating in ["/master/", "/main/", "/HEAD/"] {
            assert!(
                !fx.source_url.contains(floating),
                "{}: source_url uses the floating ref {floating}",
                fx.id
            );
        }
    }
}

#[test]
fn every_fixture_is_the_file_the_manifest_describes() {
    // The positive control. An empty corpus would make every per-fixture
    // assertion below pass over nothing, so the count is checked first.
    assert_eq!(
        fixtures::all().len(),
        EXPECTED_VERSIONS.len(),
        "expected {} fixtures, found {}",
        EXPECTED_VERSIONS.len(),
        fixtures::all().len()
    );

    for fx in fixtures::all() {
        let path = fx.path();
        // The one place `read_unverified` is legitimate: this test has to do
        // the comparison itself so a failure can print both hashes.
        let bytes = fx.read_unverified().unwrap_or_else(|e| {
            panic!(
                "{}: cannot read {}: {e}. Run `python3 tools/fetch_acadsharp_fixtures.py fetch` \
                 to restore it from the pinned commit.",
                fx.id,
                path.display()
            )
        });

        assert_eq!(
            bytes.len() as u64,
            fx.bytes,
            "{}: {} bytes on disk, manifest says {}",
            fx.id,
            bytes.len(),
            fx.bytes
        );
        assert!(
            bytes.len() as u64 >= MIN_PLAUSIBLE_BYTES,
            "{}: {} bytes is too small to be a real DWG",
            fx.id,
            bytes.len()
        );

        // The six-byte signature catches an HTML error page, a DXF renamed to
        // .dwg, and a file that is simply the wrong generation.
        let magic = &bytes[..6];
        assert_eq!(
            magic,
            fx.acad_version.as_bytes(),
            "{}: first six bytes are {:?}, expected {:?}",
            fx.id,
            String::from_utf8_lossy(magic),
            fx.acad_version
        );

        let got = format!("{:x}", Sha256::digest(&bytes));
        assert_eq!(
            got, fx.sha256,
            "{}: sha256 mismatch. These are not the bytes the manifest froze.",
            fx.id
        );
    }
}

#[test]
fn no_two_fixtures_are_the_same_file() {
    let mut by_hash: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for fx in fixtures::all() {
        by_hash.entry(&fx.sha256).or_default().push(&fx.id);
    }
    let dupes: Vec<_> = by_hash.values().filter(|ids| ids.len() > 1).collect();
    assert!(
        dupes.is_empty(),
        "these fixtures share a sha256, so at least one is not the version it \
         claims to be: {dupes:?}. If a duplicate is ever deliberate, say so here \
         with a reason rather than relaxing the check."
    );
}

#[test]
fn the_upstream_licence_travels_with_the_fixtures() {
    let up = fixtures::upstream();
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures")
        .join(&up.license_file);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{} must be readable: {e}", path.display()));
    assert!(
        text.contains("MIT License"),
        "{} does not look like the MIT licence the manifest claims",
        path.display()
    );
    assert_eq!(up.license, "MIT");
}

#[test]
fn nothing_sits_in_the_corpus_without_a_manifest_entry() {
    let root = fixtures::fixture_root();
    let claimed: BTreeSet<PathBuf> = fixtures::all().iter().map(|f| f.path()).collect();

    let mut orphans = Vec::new();
    let mut found = 0usize;
    for path in dwg_files_under(&root) {
        found += 1;
        if !claimed.contains(&path) {
            orphans.push(
                path.strip_prefix(&root)
                    .unwrap_or(&path)
                    .display()
                    .to_string(),
            );
        }
    }

    // Positive control. Without it a typo in the walker that returned nothing
    // would make the orphan check pass by never looking at anything.
    assert_eq!(
        found,
        fixtures::all().len(),
        "walked {} DWG files under {} but the manifest lists {}",
        found,
        root.display(),
        fixtures::all().len()
    );
    assert!(
        orphans.is_empty(),
        "these DWG files are in the tree but not in fixtures/manifest.toml: {orphans:?}. \
         A fixture nobody added to the manifest is a fixture nobody vetted: no provenance, \
         no licence, no hash. Add an entry or delete the file."
    );
}

#[test]
fn every_fixture_carries_its_own_licence_evidence() {
    let up = fixtures::upstream();
    for fx in fixtures::all() {
        assert!(
            !fx.license.trim().is_empty(),
            "{}: no licence recorded, so there is nothing saying these bytes may be \
             redistributed",
            fx.id
        );
        assert!(
            fx.license_evidence.starts_with("https://"),
            "{}: license_evidence is {:?}, which is not a URL anyone can check",
            fx.id,
            fx.license_evidence
        );
        // Pinned for the same reason the fixture URLs are: a licence read off
        // a branch is a licence that can change after the fact.
        assert!(
            fx.license_evidence.contains(&up.commit),
            "{}: license_evidence is not pinned to the upstream commit: {}",
            fx.id,
            fx.license_evidence
        );
    }
}

#[test]
fn load_refuses_bytes_that_are_not_the_fixture() {
    let fx = fixtures::by_version("AC1018").expect("AC1018 is in the corpus");

    // Proves the verifier is doing something. Without a case like this, `load`
    // could return early on every input and every other test here would still
    // be green, because they all read the real, correct files.
    let mut damaged = fx.load().expect("the committed fixture is intact");
    damaged[fx.bytes as usize / 2] ^= 0xff;
    let err = fx
        .verify(&damaged)
        .expect_err("a flipped byte must not verify");
    let msg = err.to_string();
    assert!(
        msg.contains(&fx.id),
        "the error should name the fixture: {msg}"
    );
    assert!(
        msg.contains("sha256"),
        "the error should say what disagreed: {msg}"
    );

    let truncated = &fx.load().expect("intact")[..512];
    assert!(
        fx.verify(truncated).is_err(),
        "a truncated fixture must not verify"
    );

    let err = fixtures::load("no-such-fixture").expect_err("unknown ids must not resolve");
    assert!(
        err.to_string().contains("no-such-fixture"),
        "the error should name the id it was given: {err}"
    );
}

#[test]
fn no_test_reads_the_corpus_behind_the_loaders_back() {
    // `fixtures::load` verifies before returning. That guarantee is worth
    // exactly as much as the number of callers who use it, so the two ways
    // around it are checked here rather than left to review.
    const BYPASSES: &[(&str, &str)] = &[
        ("fixtures/dwg", "a hardcoded path into the corpus"),
        ("include_bytes!", "a fixture embedded at compile time"),
        ("read_unverified", "the documented escape hatch"),
    ];
    // `fixtures.rs` defines them and this file has to use one to compare
    // hashes. Everything else goes through `load`.
    const ALLOWED: &[&str] = &["src/fixtures.rs", "tests/fixture_integrity.rs"];

    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut offences = Vec::new();
    let mut scanned = 0usize;
    for dir in ["src", "tests"] {
        for path in rust_files_under(&root.join(dir)) {
            let rel = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .display()
                .to_string();
            let rel = rel.replace('\\', "/");
            if ALLOWED.contains(&rel.as_str()) {
                continue;
            }
            scanned += 1;
            let text = std::fs::read_to_string(&path).expect("source file readable");
            for (needle, what) in BYPASSES {
                if text.contains(needle) {
                    offences.push(format!("{rel}: {what} ({needle})"));
                }
            }
        }
    }

    assert!(
        offences.is_empty(),
        "these read the corpus without verifying it: {offences:?}. Use \
         `fixtures::load(id)`, which checks size, signature and sha256 first. If a case \
         genuinely needs raw bytes, add the file to ALLOWED here with a reason.\n\
         (scanned {scanned} files)"
    );
}

/// Every `.dwg` under `dir`, recursively.
fn dwg_files_under(dir: &std::path::Path) -> Vec<PathBuf> {
    files_under(dir, &|p| {
        p.extension().is_some_and(|e| e.eq_ignore_ascii_case("dwg"))
    })
}

/// Every `.rs` under `dir`, recursively.
fn rust_files_under(dir: &std::path::Path) -> Vec<PathBuf> {
    files_under(dir, &|p| p.extension().is_some_and(|e| e == "rs"))
}

fn files_under(dir: &std::path::Path, keep: &dyn Fn(&std::path::Path) -> bool) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            out.extend(files_under(&path, keep));
        } else if keep(&path) {
            out.push(path);
        }
    }
    out.sort();
    out
}
