//! `docs/TOLERANCES.md` and `src/compare.rs` agree about every bound (R2.1, #19).
//!
//! A bound only moves with fixture evidence, so it must not be possible to
//! widen one in code and leave the document claiming the old number, or the
//! other way round. The document's tables are read here the way a person reads
//! them, and every number is compared with the constant the comparator uses.

use acadsharp_rs_tests::compare::{
    BOUNDED, BOUNDED_TIGHT, Class, ClassTable, Engine, Pair, Tolerance, WarningRow,
};

const DOC: &str = include_str!("../docs/TOLERANCES.md");

/// The cells of every table row whose first cell is `needle`, backticks removed.
fn rows<'a>(needle: &str) -> Vec<Vec<String>> {
    DOC.lines()
        .filter(|l| l.trim_start().starts_with('|'))
        .map(|l| {
            l.trim()
                .trim_matches('|')
                .split('|')
                .map(|c| c.trim().trim_matches('`').to_string())
                .collect::<Vec<_>>()
        })
        .filter(|cells| cells.first().map(String::as_str) == Some(needle))
        .collect()
}

fn tolerance_row(class: &str) -> Tolerance {
    let found = rows(class);
    assert_eq!(found.len(), 1, "docs/TOLERANCES.md needs exactly one `{class}` bound row, found {}", found.len());
    let cells = &found[0];
    let parse = |i: usize| -> f64 {
        cells[i]
            .parse()
            .unwrap_or_else(|_| panic!("`{class}` row cell {i} is {:?}, not a number", cells[i]))
    };
    Tolerance { abs: parse(1), rel: parse(2) }
}

#[test]
fn the_documented_bounded_tight_bound_is_the_one_in_code() {
    assert_eq!(tolerance_row("BOUNDED_TIGHT"), BOUNDED_TIGHT);
}

#[test]
fn the_documented_bounded_bound_is_the_one_in_code() {
    assert_eq!(tolerance_row("BOUNDED"), BOUNDED);
}

#[test]
fn the_documented_class_per_pair_is_the_one_in_code() {
    let engines = [Engine::Oracle, Engine::Acadsharp, Engine::Acadrust, Engine::Ezdxf];
    let mut checked = 0;
    for a in engines {
        for b in engines {
            let pair = Pair::new(a, b);
            if a > b {
                continue; // a pair is unordered in the document
            }
            let found = rows(&pair.label());
            assert_eq!(found.len(), 1, "docs/TOLERANCES.md needs exactly one row for the pair `{pair}`");
            let table = ClassTable::for_pair(pair, WarningRow::Compare);
            let want = match table.class {
                Class::BoundedTight => "BOUNDED_TIGHT",
                Class::Bounded => "BOUNDED",
                other => panic!("{pair} starts from {other:?}"),
            };
            assert_eq!(found[0][1], want, "the document gives `{pair}` the wrong class");
            checked += 1;
        }
    }
    assert_eq!(checked, 10, "four engines make ten unordered pairs");
}

#[test]
fn the_document_states_the_rules_it_is_held_to() {
    for needle in [
        "provisional",
        "only moves with fixture evidence",
        "max(abs, rel * max(|a|, |b|))",
        "EXCLUDED",
        "KNOWN_DIVERGENCE",
        "DUAL_ORACLE",
        "ANGLE",
    ] {
        assert!(DOC.contains(needle), "docs/TOLERANCES.md does not say {needle:?}");
    }
}
