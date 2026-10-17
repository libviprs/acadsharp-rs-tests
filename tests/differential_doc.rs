//! `docs/DIFFERENTIAL.md` shows a diff line, and it has to be the line the
//! comparator prints, not one written by hand to look plausible.

mod support;

use std::fs;
use std::path::PathBuf;

fn doc() -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("docs")
        .join("DIFFERENTIAL.md");
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("I could not read {}: {e}", path.display()))
}

/// The circle in the base stream is radius 5.0; the other side says 5.005.
fn rendered() -> String {
    let a = support::base();
    let mut b = a.clone();
    let at = b
        .iter()
        .position(|l| l.contains(r#""record":"circle""#))
        .expect("the base stream has a circle");
    support::set(&mut b, at, r#""radius":5.0"#, r#""radius":5.005"#);
    let report = support::diff(&a, &b);
    let red = report.red();
    assert_eq!(red.len(), 1, "one changed radius is one finding: {red:#?}");
    red[0].render()
}

#[test]
fn the_diff_line_in_the_doc_is_the_one_the_comparator_prints() {
    let line = rendered();
    assert!(
        doc().contains(&line),
        "docs/DIFFERENTIAL.md does not contain the comparator's own line:\n{line}"
    );
}

#[test]
fn the_doc_names_the_pieces_it_depends_on_and_they_exist() {
    let text = doc();
    for path in [
        "tests/parity.rs",
        "src/acadsharp_emit.rs",
        "tests/fixture_parity.rs",
        "tests/exporter_independence.rs",
        "tools/native_cells.py",
        "tools/native_cell.sh",
        "tools/compare_cells.py",
        "NATIVE_ARTIFACT.toml",
    ] {
        assert!(text.contains(path), "the doc never mentions {path}");
        assert!(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join(path)
                .exists(),
            "{path} is named in the doc and does not exist"
        );
    }
}
