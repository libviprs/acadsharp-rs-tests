//! Mirrors `CommonDimensionTests` and the seven `Dimension*Tests`: each kind
//! comes out as its drawn block, and the text is the value the generator measured.
//!
//! `regression_dimension_fields` has one dimension of each kind with known
//! measurements: 10, 5, 4, 8, 90 degrees, 60 degrees and an ordinate at 120. Two
//! facts about ACadSharp 3.7.1 shape what can be asserted, and the generator's
//! comments say how (reference/ACadSharp.Generator/Drawings/DimensionFields.cs):
//! its `UpdateBlock` draws nothing for the two angular kinds, so the generator
//! authors their text; and it formats the ordinate wrongly, so that text is written
//! out. The issue asks for at least one `Line` per kind; the two angular kinds
//! therefore have only their text, and that is asserted instead of a Line that
//! cannot exist.
//!
//! The shim also skips the inserts inside a dimension's block, which is where
//! arrowheads live (acadsharp-rs-tests#55), so no arrowhead is asserted.

use acadsharp_rs::{Item, Primitive};

use crate::common::*;

const MEASURED: [&str; 7] = ["10", "5", "4", "8", "90°", "60°", "120"];
const KINDS: [&str; 7] = [
    "linear",
    "aligned",
    "radius",
    "diameter",
    "angular (two lines)",
    "angular (three points)",
    "ordinate",
];

/// The primitives of the view, split after each text: group `i` is dimension `i`.
fn groups() -> Vec<Vec<Primitive>> {
    let o = open("regression_dimension_fields");
    let mut out = vec![Vec::new()];
    for item in items(&o.document, 0) {
        if let Item::Primitive(p) = item {
            let is_text = matches!(p, Primitive::Text(_));
            out.last_mut().unwrap().push(p);
            if is_text {
                out.push(Vec::new());
            }
        }
    }
    out.pop(); // nothing follows the last text
    out
}

#[test]
fn seven_dimensions_make_seven_texts_that_equal_the_measured_values() {
    let groups = groups();
    assert_eq!(groups.len(), 7, "one text per dimension kind");
    for (i, g) in groups.iter().enumerate() {
        let Some(Primitive::Text(t)) = g.last() else {
            panic!("{}: a dimension ends in its text", KINDS[i]);
        };
        assert_eq!(t.text, MEASURED[i], "{}", KINDS[i]);
        assert!(
            t.height > 0.0,
            "{}: a dimension text has a height",
            KINDS[i]
        );
    }
}

#[test]
fn every_kind_the_generator_can_draw_also_emits_a_line() {
    let groups = groups();
    for i in [0usize, 1, 2, 3, 6] {
        let lines = groups[i]
            .iter()
            .filter(|p| matches!(p, Primitive::Line(_)))
            .count();
        assert!(lines >= 1, "{}: no Line before its text", KINDS[i]);
    }
}

#[test]
fn the_two_angular_kinds_have_only_the_text_the_generator_authored() {
    let groups = groups();
    for i in [4usize, 5] {
        assert_eq!(groups[i].len(), 1, "{}: {:?}", KINDS[i], groups[i]);
    }
}
