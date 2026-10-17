//! Mirrors `Entities/ArcTests.cs`: centre, radius and both angles survive, and an
//! arc stays an arc.

use acadsharp_rs::Primitive;

use crate::common::*;

#[test]
fn two_arcs_keep_the_centre_radius_and_angles_the_generator_wrote() {
    let o = open("parity_arc");
    let prims = primitives(&o.document, 0);
    // No tessellation: two arcs written, two arcs read, and nothing else.
    assert_eq!(prims.len(), 2, "{prims:?}");
    let [Primitive::Arc(a), Primitive::Arc(b)] = &prims[..] else {
        panic!("expected two Arc primitives, got {prims:?}");
    };
    assert_near3("arc 0 centre", a.centre, [12.5, -7.25, 0.0]);
    assert_near("arc 0 radius", a.radius, 5.0);
    assert_near("arc 0 start", a.start_angle, 0.5);
    assert_near("arc 0 end", a.end_angle, 2.0);
    assert_near3("arc 0 normal", a.normal, Z);
    assert_near3("arc 1 centre", b.centre, [-3.0, 2.0, 0.0]);
    assert_near("arc 1 radius", b.radius, 0.75);
    assert_near("arc 1 start", b.start_angle, 3.5);
    assert_near("arc 1 end", b.end_angle, 5.75);
}

#[test]
fn a_swapped_field_would_fail_because_every_number_differs() {
    // The control: the two arcs share no value, so a reader that mixed up the
    // centre of one with the other's, or radius with an angle, cannot pass above.
    let o = open("parity_arc");
    let prims = primitives(&o.document, 0);
    let (Primitive::Arc(a), Primitive::Arc(b)) = (&prims[0], &prims[1]) else {
        panic!("two arcs");
    };
    assert!(!near(a.radius, b.radius) && !near(a.start_angle, b.start_angle));
    assert!(!near3(a.centre, b.centre));
}
