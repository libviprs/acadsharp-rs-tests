//! Mirrors `Entities/LineTests.cs`: both endpoints, all three coordinates.

use acadsharp_rs::Primitive;

use crate::common::*;

#[test]
fn a_3d_line_and_a_long_line_keep_their_endpoints() {
    let o = open("parity_line");
    let prims = primitives(&o.document, 0);
    assert_eq!(prims.len(), 2, "{prims:?}");
    let [Primitive::Line(a), Primitive::Line(b)] = &prims[..] else {
        panic!("expected two Line primitives, got {prims:?}");
    };
    // Non-zero z on both ends: a reader that flattens to 2D fails here.
    assert_near3("line 0 start", a.start, [1.0, 2.0, 3.0]);
    assert_near3("line 0 end", a.end, [4.0, 5.0, 6.0]);
    assert_near3("line 1 start", b.start, [0.0, 0.0, 0.0]);
    assert_near3("line 1 end", b.end, [100.0, 0.0, 0.0]);
}
