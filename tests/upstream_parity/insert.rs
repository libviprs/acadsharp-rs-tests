//! Mirrors `InsertTest` and `DynamicBlockTests`: an insert's point, scale and
//! rotation applied to the block's geometry, a block nested two levels deep, and a
//! dynamic block that still decodes.

use acadsharp_rs::Primitive;
use acadsharp_rs_tests::fixtures;

use crate::common::*;

#[test]
fn a_block_nested_two_levels_deep_lands_where_the_hand_computation_says() {
    // GEN_L1 holds (0,0)-(1,0); GEN_L2 holds L1 at (2,0); GEN_L3 holds L2 at
    // (0,3); the drawing inserts L3 at (100,50), scale 2, a quarter turn. A point
    // (x,0) goes (x+2,3), (2x+4,6), (-6,2x+4), (94,2x+54): (94,54) to (94,56).
    let o = open("parity_insert");
    let items = items(&o.document, 0);
    let lines: Vec<_> = items
        .iter()
        .filter_map(|i| match i {
            acadsharp_rs::Item::Primitive(Primitive::Line(l)) => Some(l),
            _ => None,
        })
        .collect();
    assert_eq!(lines.len(), 2, "{lines:?}");
    assert_near3("nested start", lines[0].start, [94.0, 54.0, 0.0]);
    assert_near3("nested end", lines[0].end, [94.0, 56.0, 0.0]);
    assert!(
        lines[0].origin.from_expanded_insert,
        "the flag that says it came from an insert"
    );
    // And the untransformed insert beside it, the identity case.
    assert_near3("plain start", lines[1].start, [200.0, 0.0, 0.0]);
    assert_near3("plain end", lines[1].end, [201.0, 0.0, 0.0]);
}

#[test]
fn a_dynamic_block_sample_decodes_to_the_end_and_yields_geometry() {
    // `dynamic-blocks/BLOCKROTATIONPARAMETER.dwg` is an AutoCAD save, so there is
    // no generator value to compare against; what is asserted is that a block
    // with dynamic-block objects around it decodes completely and still produces
    // primitives, the property DynamicBlockTests exists to keep.
    let id = "acadsharp-blockrotationparameter";
    assert!(
        fixtures::by_id(id).is_some(),
        "the corpus carries the sample"
    );
    let o = open(id);
    let n = primitives(&o.document, 0).len();
    assert!(n > 0, "a dynamic block produced no geometry");
}
