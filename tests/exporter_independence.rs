//! The native side's exporter shares nothing with the .NET one.
//!
//! The differential is only worth running if the two sides cannot share a bug.
//! `reference/ACadSharp.Reference` (C#, JIT) and `src/acadsharp_emit.rs` plus
//! `src/ocs.rs` (Rust, over the safe API) are written separately on purpose, and
//! the only thing they have in common is `docs/CANONICAL_SCHEMA.md`.
//!
//! This test looks for the cheapest way that stops being true: a C# identifier
//! of any distinctive size turning up in the Rust exporter, or the other way
//! round (they are the same check, since it is a set intersection). Words the
//! schema document itself uses are allowed, because a field called
//! `maintenance_version` is named by the contract and not by either side's
//! code. A control feeds the check two files that really do share an
//! identifier, so a scanner that found nothing in everything cannot pass.
//!
//! What this cannot see is a shared idea rather than a shared word. It is a
//! tripwire for copy and paste, not a proof of independence; the proof is that
//! the two disagree where the wire cannot say what the oracle says (`parity/`).

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// The Rust files that make up the native-side exporter.
const RUST_EXPORTER: [&str; 2] = ["src/acadsharp_emit.rs", "src/ocs.rs"];

/// Identifiers shorter than this, once case and underscores are folded away,
/// are ordinary vocabulary (`index`, `points`, `warning`), not an author's choice.
const MIN_LEN: usize = 10;

/// Words made of a single run of one case are ordinary words; an identifier
/// worth comparing has a boundary in it (`FooBar`, `foo_bar`, `fooBar`).
fn has_boundary(token: &str) -> bool {
    token.contains('_') || token.chars().skip(1).any(|c| c.is_ascii_uppercase())
}

fn identifiers(text: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut current = String::new();
    let flush = |current: &mut String, out: &mut BTreeSet<String>| {
        if !current.is_empty() {
            if has_boundary(current) {
                let folded: String = current
                    .chars()
                    .filter(|c| *c != '_')
                    .map(|c| c.to_ascii_lowercase())
                    .collect();
                if folded.len() >= MIN_LEN {
                    out.insert(folded);
                }
            }
            current.clear();
        }
    };
    for c in text.chars() {
        if c.is_ascii_alphanumeric() || c == '_' {
            current.push(c);
        } else {
            flush(&mut current, &mut out);
        }
    }
    flush(&mut current, &mut out);
    out
}

fn walk(dir: &Path, ext: &str, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let p = e.path();
        let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if p.is_dir() {
            if !matches!(name, "bin" | "obj") {
                walk(&p, ext, out);
            }
        } else if p.extension().and_then(|e| e.to_str()) == Some(ext) {
            out.push(p);
        }
    }
}

/// Strips comments, so a sentence that happens to quote the other side's name
/// does not count as code that uses it.
fn code_only(text: &str, line_comment: &str) -> String {
    text.lines()
        .map(|l| l.split(line_comment).next().unwrap_or(""))
        .collect::<Vec<_>>()
        .join("\n")
}

fn csharp_identifiers() -> BTreeSet<String> {
    let mut files = Vec::new();
    walk(
        &root().join("reference").join("ACadSharp.Reference"),
        "cs",
        &mut files,
    );
    assert!(
        files.len() >= 10,
        "found only {} C# files under reference/ACadSharp.Reference",
        files.len()
    );
    files
        .iter()
        .flat_map(|p| identifiers(&code_only(&fs::read_to_string(p).expect("read C#"), "//")))
        .collect()
}

fn rust_identifiers() -> BTreeSet<String> {
    RUST_EXPORTER
        .iter()
        .flat_map(|rel| {
            let text = fs::read_to_string(root().join(rel))
                .unwrap_or_else(|e| panic!("I could not read {rel}: {e}"));
            identifiers(&code_only(&text, "//"))
        })
        .collect()
}

/// Every identifier the schema document uses, folded the same way.
fn schema_vocabulary() -> BTreeSet<String> {
    let text = fs::read_to_string(root().join("docs").join("CANONICAL_SCHEMA.md")).expect("schema");
    identifiers(&text)
}

/// Words both sides use that are vocabulary, not code, each with the reason.
///
/// * `startangle` and `isperiodic` are ACadSharp's own member names
///   (`Arc.StartAngle`, `Spline.IsPeriodic`). The oracle reads them from
///   ACadSharp and the crate's safe API mirrors them (`start_angle`,
///   `is_periodic`), so both sides name the upstream library's fields.
/// * `documentrecord`, `viewrecord` and `warningrecord` are the three schema
///   record kinds that carry no geometry, named after the schema's `document`,
///   `view` and `warning` records by each side's own type or function.
///
/// The list is exact on purpose: a sixth shared word is a new finding, not an
/// addition to be waved through.
const VOCABULARY: [&str; 5] = [
    "documentrecord",
    "isperiodic",
    "startangle",
    "viewrecord",
    "warningrecord",
];

#[test]
fn the_two_exporters_share_no_identifier_the_schema_does_not_name() {
    let (cs, rs, schema) = (
        csharp_identifiers(),
        rust_identifiers(),
        schema_vocabulary(),
    );
    assert!(
        cs.len() >= 100,
        "the C# scan found only {} identifiers",
        cs.len()
    );
    assert!(
        rs.len() >= 30,
        "the Rust scan found only {} identifiers",
        rs.len()
    );
    assert!(
        schema.len() >= 20,
        "the schema scan found only {} identifiers",
        schema.len()
    );

    let shared: Vec<&String> = cs
        .intersection(&rs)
        .filter(|w| !schema.contains(*w) && !VOCABULARY.contains(&w.as_str()))
        .collect();
    assert!(
        shared.is_empty(),
        "the .NET oracle and the Rust exporter both use {shared:?}. Either the word is part of the \
         contract (then docs/CANONICAL_SCHEMA.md should say it) or one side copied the other."
    );
}

#[test]
fn control_the_scanner_finds_a_shared_identifier_when_there_is_one() {
    let a = identifiers("class BlockResolver { void ResolveNestedInsert() {} }");
    let b = identifiers("fn resolve_nested_insert() {} // and BlockResolver is a type");
    let b_code = identifiers(&code_only(
        "fn resolve_nested_insert() {} // BlockResolver",
        "//",
    ));
    let found: Vec<_> = a.intersection(&b_code).collect();
    assert_eq!(
        found,
        [&"resolvenestedinsert".to_string()],
        "folding case and underscores"
    );
    // And a comment alone does not count as use.
    assert!(b.contains("blockresolver") && !b_code.contains("blockresolver"));
}
