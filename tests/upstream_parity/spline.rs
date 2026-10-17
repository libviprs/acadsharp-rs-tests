//! Mirrors `Entities/SplineTests.cs`: degree, knot vector and control points.

use acadsharp_rs::Primitive;

use crate::common::*;

#[test]
fn a_cubic_and_a_quadratic_keep_degree_knots_and_control_points() {
    let o = open("parity_spline");
    let prims = primitives(&o.document, 0);
    assert_eq!(prims.len(), 2, "a spline stays a spline: {prims:?}");
    let [Primitive::Spline(cubic), Primitive::Spline(quad)] = &prims[..] else {
        panic!("expected two Spline primitives, got {prims:?}");
    };

    assert_eq!(cubic.degree, 3);
    assert_eq!(cubic.knots.len(), 10, "knot count");
    for (i, want) in [0.0, 0.0, 0.0, 0.0, 1.0, 2.0, 3.0, 3.0, 3.0, 3.0]
        .iter()
        .enumerate()
    {
        assert_near(&format!("cubic knot {i}"), cubic.knots[i], *want);
    }
    let cps: [[f64; 3]; 6] = [
        [0.0, 0.0, 0.0],
        [2.0, 5.0, 0.0],
        [5.0, 6.0, 0.0],
        [8.0, 2.0, 0.0],
        [11.0, 4.0, 0.0],
        [14.0, 0.0, 0.0],
    ];
    assert_eq!(cubic.controls.len(), 6);
    for (i, want) in cps.iter().enumerate() {
        assert_near3(&format!("cubic control {i}"), cubic.controls[i], *want);
    }
    assert!(!cubic.is_rational() && !cubic.is_closed() && !cubic.is_periodic());

    assert_eq!(quad.degree, 2);
    assert_eq!(quad.knots.len(), 7, "knot count");
    for (i, want) in [0.0, 0.0, 0.0, 1.0, 2.0, 2.0, 2.0].iter().enumerate() {
        assert_near(&format!("quadratic knot {i}"), quad.knots[i], *want);
    }
    let cps: [[f64; 3]; 4] = [
        [0.0, 20.0, 0.0],
        [5.0, 30.0, 1.0],
        [10.0, 20.0, 2.0],
        [15.0, 30.0, 3.0],
    ];
    assert_eq!(quad.controls.len(), 4);
    for (i, want) in cps.iter().enumerate() {
        assert_near3(&format!("quadratic control {i}"), quad.controls[i], *want);
    }
}
