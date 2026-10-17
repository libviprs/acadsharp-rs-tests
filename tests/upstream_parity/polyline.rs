//! Mirrors `LwPolylineTest`, `Polyline2DTests`, `Polyline3DTests`,
//! `CommonPolylineTests` and `CommonVertexTests`: the closed flag, the vertices,
//! and where the bulges go.
//!
//! The issue asks for "bulge segments arrive as arcs, not as chords". Wire 2
//! retired the polyline-to-arc decomposition (libviprs-dep#64): a bulge crosses
//! the wire as a bulge on its `Polyline`, which is a lossless form and leaves
//! tessellation to the consumer. So the assertions are that the bulge values are
//! the generator's and that neither an `Arc` nor a `Line` appears in their place.

use acadsharp_rs::Primitive;

use crate::common::*;

fn polylines() -> Vec<acadsharp_rs::item::Polyline> {
    let o = open("parity_polyline");
    let prims = primitives(&o.document, 0);
    assert!(
        prims.iter().all(|p| matches!(p, Primitive::Polyline(_))),
        "a bulge must not become an Arc or a Line: {prims:?}"
    );
    prims
        .into_iter()
        .map(|p| match p {
            Primitive::Polyline(p) => p,
            _ => unreachable!(),
        })
        .collect()
}

#[test]
fn four_polylines_arrive_as_four_polylines() {
    assert_eq!(polylines().len(), 4);
}

#[test]
fn a_closed_lightweight_polyline_keeps_its_flag_vertices_and_bulges() {
    let p = &polylines()[0];
    assert!(p.closed, "the generator closed it");
    assert_near3("normal", p.normal, Z);
    let want = [
        [0.0, 0.0, 0.0],
        [10.0, 0.0, 0.0],
        [10.0, 10.0, 0.0],
        [0.0, 10.0, 0.0],
    ];
    assert_eq!(p.vertices.len(), 4);
    for (i, w) in want.iter().enumerate() {
        assert_near3(&format!("vertex {i}"), p.vertices[i], *w);
    }
    for (i, w) in [0.0, 0.5, -0.25, 0.0].iter().enumerate() {
        assert_near(&format!("bulge {i}"), bulge(&p.bulges, i), *w);
    }
}

#[test]
fn an_open_lightweight_polyline_is_open_and_straight() {
    let p = &polylines()[1];
    assert!(!p.closed);
    assert_eq!(p.vertices.len(), 3);
    assert_near3("vertex 1", p.vertices[1], [25.0, 5.0, 0.0]);
    assert!((0..3).all(|i| bulge(&p.bulges, i) == 0.0), "{:?}", p.bulges);
}

#[test]
fn a_2d_polyline_keeps_the_bulge_on_its_second_vertex() {
    let p = &polylines()[2];
    assert!(!p.closed);
    assert_eq!(p.vertices.len(), 3);
    assert_near3("vertex 0", p.vertices[0], [40.0, 0.0, 0.0]);
    assert_near3("vertex 1", p.vertices[1], [50.0, 5.0, 0.0]);
    assert_near3("vertex 2", p.vertices[2], [60.0, 0.0, 0.0]);
    assert_near("bulge 0", bulge(&p.bulges, 0), 0.0);
    assert_near("bulge 1", bulge(&p.bulges, 1), 0.3);
    assert_near("bulge 2", bulge(&p.bulges, 2), 0.0);
}

#[test]
fn a_3d_polyline_keeps_every_z() {
    let p = &polylines()[3];
    assert!(!p.closed);
    let want = [[70.0, 0.0, 0.0], [80.0, 5.0, 5.0], [90.0, 0.0, 10.0]];
    assert_eq!(p.vertices.len(), 3);
    for (i, w) in want.iter().enumerate() {
        assert_near3(&format!("vertex {i}"), p.vertices[i], *w);
    }
    assert!(
        (0..3).all(|i| bulge(&p.bulges, i) == 0.0),
        "a 3D polyline has no bulges"
    );
}
