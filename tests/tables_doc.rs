//! `docs/TOLERANCES.md`'s pair rows and `parity/TABLES.toml` agree (R4.1, #25).
//!
//! The rows are data, so the document cannot be allowed to describe a table that
//! is not the committed one: every issue a row of the file cites has to appear in
//! the document's "Pair rows" section, and every issue the section cites has to be
//! one the file (or a verdict row) uses.

use std::collections::BTreeSet;

const DOC: &str = include_str!("../docs/TOLERANCES.md");
const TABLES: &str = include_str!("../parity/TABLES.toml");

fn section() -> &'static str {
    let rest = DOC
        .split("### Pair rows")
        .nth(1)
        .expect("TOLERANCES.md has a Pair rows section");
    rest.split("\n## ").next().unwrap()
}

fn issues(text: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for word in text.split(|c: char| !(c.is_ascii_alphanumeric() || "/#-_.".contains(c))) {
        if let Some((repo, n)) = word.rsplit_once('#')
            && repo.starts_with("libviprs/")
            && !n.is_empty()
            && n.chars().all(|c| c.is_ascii_digit())
        {
            out.insert(word.to_string());
        }
    }
    out
}

#[test]
fn every_issue_in_the_tables_file_is_in_the_document() {
    let doc = issues(section());
    for i in issues(TABLES) {
        assert!(
            doc.contains(&i),
            "parity/TABLES.toml cites {i} and docs/TOLERANCES.md's Pair rows does not"
        );
    }
}

#[test]
fn every_issue_in_the_document_is_in_the_tables_file() {
    let file = issues(TABLES);
    for i in issues(section()) {
        assert!(
            file.contains(&i),
            "docs/TOLERANCES.md's Pair rows cites {i} and parity/TABLES.toml does not"
        );
    }
}
