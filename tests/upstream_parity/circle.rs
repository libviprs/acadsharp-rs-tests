//! Mirrors `Entities/CircleTests.cs`.

use acadsharp_rs::Primitive;

use crate::common::*;

#[test]
fn two_circles_keep_the_centre_and_radius_the_generator_wrote() {
    let o = open("parity_circle");
    let prims = primitives(&o.document, 0);
    assert_eq!(prims.len(), 2, "no tessellation: {prims:?}");
    let [Primitive::Circle(a), Primitive::Circle(b)] = &prims[..] else {
        panic!("expected two Circle primitives, got {prims:?}");
    };
    assert_near3("circle 0 centre", a.centre, [3.0, 4.0, 0.0]);
    assert_near("circle 0 radius", a.radius, 2.5);
    assert_near3("circle 0 normal", a.normal, Z);
    assert_near3("circle 1 centre", b.centre, [-10.5, 0.25, 0.0]);
    assert_near("circle 1 radius", b.radius, 100.0);
}
