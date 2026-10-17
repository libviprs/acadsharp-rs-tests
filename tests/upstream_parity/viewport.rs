//! Mirrors `ViewportTests`, `IO/ViewportTests.cs` and `LayoutTests`: the layouts of
//! a drawing enumerate as views, in order, with their names, and each view holds
//! its own geometry.
//!
//! The issue says paper views have kind `Paper`; the safe API calls it
//! `ViewKind::Layout`, which is what the wire's kind 1 is.

use acadsharp_rs::{Primitive, ViewKind};

use crate::common::*;

#[test]
fn a_drawing_with_two_named_layouts_enumerates_four_views_in_order() {
    let o = open("parity_viewport");
    let views = o.document.views().expect("views");
    let got: Vec<(u32, &str, ViewKind)> = views
        .iter()
        .map(|v| (v.index(), v.name(), v.kind()))
        .collect();
    // Model space first, then the layouts in tab order: the two the generator
    // added, then the default `Layout1` the writer always makes.
    assert_eq!(
        got,
        [
            (0, "Model", ViewKind::Model),
            (1, "Plan A", ViewKind::Layout),
            (2, "Plan B", ViewKind::Layout),
            (3, "Layout1", ViewKind::Layout),
        ]
    );
    assert_eq!(o.document.view_count().unwrap(), 4);
}

#[test]
fn each_view_holds_the_one_line_the_generator_put_in_it() {
    let o = open("parity_viewport");
    let want = [
        (0, [0.0, 0.0, 0.0], [10.0, 10.0, 0.0]),
        (1, [10.0, 10.0, 0.0], [200.0, 10.0, 0.0]),
        (2, [20.0, 20.0, 0.0], [30.0, 30.0, 0.0]),
    ];
    for (view, a, b) in want {
        let prims = primitives(&o.document, view);
        assert_eq!(prims.len(), 1, "view {view}: {prims:?}");
        let Primitive::Line(l) = &prims[0] else {
            panic!("view {view}: {prims:?}")
        };
        assert_near3(&format!("view {view} start"), l.start, a);
        assert_near3(&format!("view {view} end"), l.end, b);
    }
    // Layout1 was never drawn in.
    assert!(primitives(&o.document, 3).is_empty());
}

#[test]
fn extents_are_some_where_the_layout_defines_them() {
    let o = open("parity_viewport");
    let views = o.document.views().unwrap();
    // What the wire says for each view, printed so a change shows up in a log.
    for v in &views {
        eprintln!(
            "view {} {:?}: extents {:?}",
            v.index(),
            v.name(),
            v.extents()
        );
    }
    let plan_a = views[1].extents().expect("Plan A defines its extents");
    assert_near("Plan A min x", plan_a.min_x, 0.0);
    assert_near("Plan A min y", plan_a.min_y, 0.0);
    assert_near("Plan A max x", plan_a.max_x, 420.0);
    assert_near("Plan A max y", plan_a.max_y, 297.0);
}
