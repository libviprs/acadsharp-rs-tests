//! Mirrors `Entities/EllipseTests.cs`: major axis and ratio, not a polyline.

use std::f64::consts::TAU;

use acadsharp_rs::Primitive;

use crate::common::*;

#[test]
fn a_full_ellipse_and_an_elliptical_arc_keep_axis_ratio_and_parameters() {
    let o = open("parity_ellipse");
    let prims = primitives(&o.document, 0);
    assert_eq!(prims.len(), 2, "no tessellation: {prims:?}");
    let [Primitive::Ellipse(full), Primitive::Ellipse(arc)] = &prims[..] else {
        panic!("expected two Ellipse primitives, got {prims:?}");
    };
    assert_near3("full centre", full.centre, [1.0, 2.0, 0.0]);
    assert_near3("full major axis", full.major_axis, [4.0, 0.0, 0.0]);
    assert_near("full ratio", full.ratio, 0.25);
    assert_near("full start", full.start_param, 0.0);
    assert_near("full end", full.end_param, TAU);

    // The arc's major axis points along +Y. The oracle and acadrust both report
    // its start and end shifted by the axis angle (2.0708 and 4.0708); the safe
    // API returns the 0.5 and 2.5 the generator wrote (libviprs acadsharp-rs-tests#89
    // holds that disagreement). This asserts the generator's values.
    assert_near3("arc centre", arc.centre, [-5.0, -5.0, 0.0]);
    assert_near3("arc major axis", arc.major_axis, [0.0, 3.0, 0.0]);
    assert_near("arc ratio", arc.ratio, 0.5);
    assert_near("arc start", arc.start_param, 0.5);
    assert_near("arc end", arc.end_param, 2.5);
}
