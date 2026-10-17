//! Mirrors `HatchTests`: a solid hatch's boundary comes out with the boundary's
//! vertex count.
//!
//! Wire 2 has no hatch record: a boundary loop arrives as one `Polygon` and the
//! wire carries no entity type, so a hatch loop and a SOLID look the same
//! (acadsharp-rs-tests#59). The assertions are therefore on the polygons and their
//! vertex counts, which is what the issue asks for, and say nothing about the fill.

use acadsharp_rs::Primitive;

use crate::common::*;

#[test]
fn a_solid_hatch_becomes_polygons_with_the_boundarys_vertex_counts() {
    let o = open("parity_hatch");
    let prims = primitives(&o.document, 0);
    let polys: Vec<_> = prims
        .iter()
        .map(|p| match p {
            Primitive::Polygon(p) => p,
            other => panic!("a hatch boundary must be a Polygon, got {other:?}"),
        })
        .collect();
    // One hatch with a five-vertex loop, one with a four-vertex outer loop and a
    // three-vertex island: three loops, three polygons.
    let counts: Vec<usize> = polys.iter().map(|p| p.vertices.len()).collect();
    assert_eq!(counts, [5, 4, 3]);
    assert_near3("pentagon vertex 3", polys[0].vertices[3], [5.0, 9.0, 0.0]);
    assert_near3("outer vertex 2", polys[1].vertices[2], [50.0, 20.0, 0.0]);
    assert_near3("island vertex 2", polys[2].vertices[2], [40.0, 15.0, 0.0]);
    assert!(polys.iter().all(|p| p.closed), "a boundary loop is closed");
}
