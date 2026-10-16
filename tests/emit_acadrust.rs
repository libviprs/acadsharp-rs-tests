//! `emit-acadrust` and the adapter behind it (R3.2, #23).
//!
//! Runs only with the `acadrust` feature, which needs `../acadrust` on disk.
//! CI's Test job runs it with `--features acadrust`.
//!
//! Three kinds of check. A test per row of schema 1's entity mapping table,
//! opening Tier B's per-kind fixtures through `fixtures::load` and asserting the
//! record kinds and counts the row promises (the counts are the oracle's, written
//! out by hand). A few hand-built documents where no fixture covers the row.
//! And the process-level properties: the same document twice is byte-identical,
//! one thread and the default agree, the parser reads what the binary wrote, and
//! nothing of upstream acadrust's source is in this suite.
#![cfg(feature = "acadrust")]

mod support;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use acadrust::document::CadDocument;
use acadrust::entities::{
    Arc as ArcEntity, AttributeDefinition, AttributeEntity, Circle, EntityType, Face3D, Insert,
    Line, LwPolyline, MText, Solid, Text, Viewport,
};
use acadrust::tables::BlockRecord;
use acadrust::types::{DxfVersion, Handle, Vector2, Vector3};
use acadsharp_rs_tests::acadrust_emit::{extract, extract_bytes};
use acadsharp_rs_tests::canonical::{Record, Sidecar, parse_stream};
use acadsharp_rs_tests::emit::Emission;
use acadsharp_rs_tests::fixtures;

// ---- helpers --------------------------------------------------------------------

fn emit_fixture(id: &str) -> Emission {
    let bytes = fixtures::load(id).unwrap_or_else(|e| panic!("{e}"));
    extract_bytes(&bytes).unwrap_or_else(|e| panic!("{id}: {e}"))
}

/// Entity record kinds and how many of each, leaving out the framing records.
fn drawn(e: &Emission) -> BTreeMap<&'static str, usize> {
    let mut out = BTreeMap::new();
    for r in &e.records {
        if !matches!(r.kind(), "document" | "view" | "warning") {
            *out.entry(r.kind()).or_default() += 1;
        }
    }
    out
}

fn warnings(e: &Emission) -> Vec<(String, String, String)> {
    e.records
        .iter()
        .filter_map(|r| match r {
            Record::Warning(w) => Some((w.category.clone(), w.entity_type.clone(), w.detail.clone())),
            _ => None,
        })
        .collect()
}

fn has_warning(e: &Emission, category: &str, detail: &str) -> bool {
    warnings(e).iter().any(|(c, _, d)| c == category && d == detail)
}

fn want(pairs: &[(&'static str, usize)]) -> BTreeMap<&'static str, usize> {
    pairs.iter().copied().collect()
}

macro_rules! row {
    ($name:ident, $fixture:literal, [$(($kind:literal, $n:literal)),* $(,)?]) => {
        #[test]
        fn $name() {
            let e = emit_fixture($fixture);
            assert_eq!(
                drawn(&e),
                want(&[$(($kind, $n)),*]),
                "{}: the records this row promises",
                $fixture
            );
        }
    };
}

// ---- one test per row of the entity mapping table --------------------------------

row!(row_line, "g13_line", [("line", 1)]);
row!(row_point, "g13_point", [("point", 8)]);
row!(row_polyline_lw_and_2d_keep_their_bulges, "g13_polyline", [("polyline", 3)]);
row!(row_a_mirrored_bulge_is_still_one_polyline, "g13_mirrored_bulge", [("polyline", 1)]);
row!(row_circle, "g13_circle", [("circle", 1)]);
row!(row_arc, "g13_arc", [("arc", 1)]);
row!(row_ellipse, "g13_ellipse", [("ellipse", 1)]);
row!(row_a_non_uniform_scale_turns_a_circle_into_an_ellipse, "g13_nonuniform", [("ellipse", 1)]);
row!(row_spline, "g13_spline", [("spline", 1)]);
row!(row_text_and_mtext, "g13_text", [("text", 2)]);
row!(row_a_long_text, "g13_long_text", [("text", 1)]);
row!(row_solid, "g13_solid", [("polygon", 5)]);
row!(row_face3d, "g13_face3d", [("polygon", 6)]);
row!(row_insert, "g13_insert", [("circle", 1), ("line", 1)]);
row!(row_nested_inserts, "g13_deep_blocks", [("line", 1)]);
row!(row_a_block_with_a_polyline_body, "g13_slot_block", [("polyline", 3)]);
row!(row_a_uniform_scale_keeps_circles_circles, "g13_scale_1x", [("circle", 64), ("line", 64)]);
row!(
    row_a_dimension_is_its_block_including_the_inserts_inside_it,
    "g13_dimension",
    [("line", 3), ("point", 4), ("polygon", 2), ("text", 1)]
);
row!(
    row_a_shallow_dimension,
    "g13_dimension_shallow",
    [("line", 12), ("point", 16), ("polygon", 8), ("text", 4)]
);
row!(row_hatch_boundary_edges_are_analytic_records, "g13_hatch", [("arc", 2), ("line", 10), ("spline", 1)]);
row!(row_ocs_mirror, "g13_ocs_mirror", [("arc", 2), ("circle", 2), ("ellipse", 2)]);
row!(row_ocs_plane, "g13_ocs_plane", [("arc", 1), ("circle", 1), ("ellipse", 1), ("polyline", 3)]);
row!(row_ocs_rotated, "g13_ocs_rotated", [("arc", 1), ("circle", 1), ("polyline", 1)]);
row!(row_ocs_skew, "g13_ocs_skew", [("ellipse", 1), ("polyline", 1)]);

#[test]
fn row_everything_else_is_an_unsupported_entity_warning_not_silence() {
    // The refused-entity fixtures hold nothing schema 1 draws, so each stream is
    // the warning that says so.
    for id in [
        "g13_mesh",
        "g13_mline",
        "g13_leader",
        "g13_wipeout",
        "g13_tolerance",
        "g13_polyface_mesh",
        "g13_polygon_mesh",
    ] {
        let e = emit_fixture(id);
        assert!(drawn(&e).is_empty(), "{id}: drew {:?}", drawn(&e));
        assert!(
            warnings(&e)
                .iter()
                .any(|(c, t, d)| c == "unsupported_entity" && !t.is_empty() && d == "no_canonical_mapping"),
            "{id}: no unsupported_entity warning in {:?}",
            warnings(&e)
        );
    }
}

#[test]
fn row_the_framing_records_open_every_stream_in_the_schemas_order() {
    let e = emit_fixture("g13_line");
    assert!(matches!(e.records[0], Record::Document(_)));
    let mut stage = 0;
    for r in &e.records {
        let s = match r {
            Record::Document(_) => 0,
            Record::View(_) => 1,
            Record::Warning(_) => 3,
            _ => 2,
        };
        assert!(s >= stage, "{r:?} is out of order");
        stage = s;
    }
    assert_eq!(
        e.records.iter().filter(|r| matches!(r, Record::View(_))).count(),
        2,
        "model and one layout"
    );
}

#[test]
fn the_document_record_names_the_version_and_the_maintenance_version() {
    let e = emit_fixture("acadsharp-ac1032");
    let Record::Document(d) = &e.records[0] else { panic!() };
    assert_eq!((d.format.as_str(), d.acad_version.as_str()), ("dwg", "AC1032"));
    let oracle = frozen("acadsharp-ac1032");
    let Record::Document(o) = &oracle[0] else { panic!() };
    assert_eq!(d.maintenance_version, o.maintenance_version, "both read the same header byte");
}

fn frozen(id: &str) -> Vec<Record> {
    let fx = fixtures::by_id(id).unwrap();
    parse_stream(&String::from_utf8(fx.load_reference_jsonl().unwrap()).unwrap()).unwrap()
}

// ---- hand-built documents, where no fixture covers the row ---------------------------

fn doc() -> CadDocument {
    CadDocument::with_version(DxfVersion::AC1032)
}

fn block(doc: &mut CadDocument, name: &str) -> Handle {
    let mut br = BlockRecord::new(name);
    br.handle = doc.allocate_handle();
    br.block_entity_handle = doc.allocate_handle();
    br.block_end_handle = doc.allocate_handle();
    let h = br.handle;
    doc.block_records.add(br).unwrap();
    h
}

fn put(doc: &mut CadDocument, owner: Option<Handle>, mut e: EntityType) -> Handle {
    if let Some(o) = owner {
        e.common_mut().owner_handle = o;
    }
    doc.add_entity(e).unwrap()
}

fn v(x: f64, y: f64, z: f64) -> Vector3 {
    Vector3::new(x, y, z)
}

fn line_at(x: f64) -> EntityType {
    EntityType::Line(Line::from_points(v(x, 0.0, 0.0), v(x + 1.0, 0.0, 0.0)))
}

#[test]
fn an_attdef_inside_a_block_is_skipped_and_the_inserts_attribs_are_emitted() {
    let mut d = doc();
    let b = block(&mut d, "TAGGED");
    put(&mut d, Some(b), line_at(0.0));
    put(
        &mut d,
        Some(b),
        EntityType::AttributeDefinition(AttributeDefinition::new(
            "TAG".into(),
            "prompt".into(),
            "default".into(),
        )),
    );
    let mut insert = Insert::new("TAGGED", v(10.0, 0.0, 0.0));
    insert
        .attributes
        .push(AttributeEntity::new("TAG".into(), "the value".into()).with_position(v(12.0, 1.0, 0.0)));
    put(&mut d, None, EntityType::Insert(insert));
    // A top-level ATTDEF is ordinary text.
    put(
        &mut d,
        None,
        EntityType::AttributeDefinition(
            AttributeDefinition::new("TOP".into(), "p".into(), "top level".into())
                .with_position(v(1.0, 2.0, 0.0)),
        ),
    );
    let e = extract(&d);
    let texts: Vec<_> = e
        .records
        .iter()
        .filter_map(|r| match r {
            Record::Text(t) => Some(t.text.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(drawn(&e), want(&[("line", 1), ("text", 2)]));
    // The ATTRIB comes out after the block body, and the ATTDEF template does not.
    assert!(texts.contains(&"the value".to_string()), "{texts:?}");
    assert!(texts.contains(&"top level".to_string()), "{texts:?}");
    assert!(!texts.contains(&"default".to_string()), "the block's ATTDEF was drawn: {texts:?}");
    // The ATTRIB sits in the space that contains the insert, not in the block's.
    let attrib = e
        .records
        .iter()
        .find_map(|r| match r {
            Record::Text(t) if t.text == "the value" => Some(t.at),
            _ => None,
        })
        .unwrap();
    assert_eq!(attrib, [12.0, 1.0, 0.0]);
}

#[test]
fn an_minsert_past_1024_copies_warns_and_expands_once() {
    let mut d = doc();
    let b = block(&mut d, "GRID");
    put(&mut d, Some(b), line_at(0.0));
    let mut insert = Insert::new("GRID", v(0.0, 0.0, 0.0));
    insert.column_count = 33;
    insert.row_count = 33;
    insert.column_spacing = 5.0;
    insert.row_spacing = 5.0;
    put(&mut d, None, EntityType::Insert(insert));
    let e = extract(&d);
    assert_eq!(drawn(&e), want(&[("line", 1)]));
    assert!(has_warning(&e, "malformed_geometry", "array_too_large_1024"), "{:?}", warnings(&e));
}

#[test]
fn an_minsert_within_the_cap_expands_every_copy() {
    let mut d = doc();
    let b = block(&mut d, "GRID");
    put(&mut d, Some(b), line_at(0.0));
    let mut insert = Insert::new("GRID", v(0.0, 0.0, 0.0));
    insert.column_count = 3;
    insert.row_count = 2;
    insert.column_spacing = 5.0;
    insert.row_spacing = 7.0;
    put(&mut d, None, EntityType::Insert(insert));
    let e = extract(&d);
    assert_eq!(drawn(&e), want(&[("line", 6)]));
    assert!(warnings(&e).is_empty(), "{:?}", warnings(&e));
}

#[test]
fn a_block_that_inserts_itself_is_a_cycle_warning_not_a_hang() {
    let mut d = doc();
    let b = block(&mut d, "LOOP");
    put(&mut d, Some(b), line_at(0.0));
    put(&mut d, Some(b), EntityType::Insert(Insert::new("LOOP", v(0.0, 0.0, 0.0))));
    put(&mut d, None, EntityType::Insert(Insert::new("LOOP", v(0.0, 0.0, 0.0))));
    let e = extract(&d);
    assert_eq!(drawn(&e), want(&[("line", 1)]));
    assert!(has_warning(&e, "malformed_geometry", "cyclic_block_reference"), "{:?}", warnings(&e));
}

#[test]
fn nesting_deeper_than_sixteen_is_a_recursion_limit_warning() {
    let mut d = doc();
    let mut names = Vec::new();
    for i in 0..20 {
        names.push(format!("B{i}"));
    }
    for i in 0..20 {
        let b = block(&mut d, &names[i]);
        put(&mut d, Some(b), line_at(i as f64));
        if i + 1 < 20 {
            put(&mut d, Some(b), EntityType::Insert(Insert::new(names[i + 1].clone(), v(0.0, 0.0, 0.0))));
        }
    }
    put(&mut d, None, EntityType::Insert(Insert::new("B0", v(0.0, 0.0, 0.0))));
    let e = extract(&d);
    assert!(has_warning(&e, "malformed_geometry", "recursion_limit_16"), "{:?}", warnings(&e));
    assert_eq!(drawn(&e)["line"], 16, "sixteen levels are drawn and no more");
}

#[test]
fn an_insert_of_a_block_that_is_not_there_is_a_missing_reference_warning() {
    let mut d = doc();
    put(&mut d, None, EntityType::Insert(Insert::new("NOPE", v(0.0, 0.0, 0.0))));
    let e = extract(&d);
    assert!(has_warning(&e, "missing_reference", "block_record_missing"), "{:?}", warnings(&e));
}

#[test]
fn an_invisible_entity_is_absent_not_warned_about() {
    let mut d = doc();
    let mut hidden = Line::from_points(v(0.0, 0.0, 0.0), v(1.0, 0.0, 0.0));
    hidden.common.invisible = true;
    put(&mut d, None, EntityType::Line(hidden));
    put(&mut d, None, line_at(5.0));
    let e = extract(&d);
    assert_eq!(drawn(&e), want(&[("line", 1)]));
    assert!(warnings(&e).is_empty(), "{:?}", warnings(&e));
}

#[test]
fn a_viewport_is_an_unsupported_entity_warning_not_dropped() {
    let mut d = doc();
    put(&mut d, None, EntityType::Viewport(Viewport::new()));
    let e = extract(&d);
    assert!(
        warnings(&e)
            .iter()
            .any(|(c, t, d)| c == "unsupported_entity" && t == "VIEWPORT" && d == "no_canonical_mapping"),
        "{:?}",
        warnings(&e)
    );
}

#[test]
fn a_bulge_under_a_non_uniform_scale_is_carried_and_declared() {
    let mut d = doc();
    let b = block(&mut d, "ARCS");
    let mut pl = LwPolyline::new();
    pl.vertices.push(acadrust::entities::LwVertex::with_bulge(Vector2::new(0.0, 0.0), 0.5));
    pl.vertices.push(acadrust::entities::LwVertex::new(Vector2::new(4.0, 0.0)));
    put(&mut d, Some(b), EntityType::LwPolyline(pl));
    let mut insert = Insert::new("ARCS", v(0.0, 0.0, 0.0));
    insert.set_x_scale(2.0);
    put(&mut d, None, EntityType::Insert(insert));
    let e = extract(&d);
    assert_eq!(drawn(&e), want(&[("polyline", 1)]));
    let Record::Polyline(p) = e.records.iter().find(|r| matches!(r, Record::Polyline(_))).unwrap() else {
        panic!()
    };
    assert_eq!(p.bulges, [0.5, 0.0], "carried through unchanged");
    assert!(has_warning(&e, "malformed_geometry", "non_uniform_scaled_bulge"));
}

#[test]
fn polyline_bulges_are_always_present() {
    let mut d = doc();
    put(&mut d, None, EntityType::LwPolyline(LwPolyline::from_points(vec![Vector2::new(0.0, 0.0), Vector2::new(1.0, 1.0)])));
    let e = extract(&d);
    let Record::Polyline(p) = e.records.iter().find(|r| matches!(r, Record::Polyline(_))).unwrap() else {
        panic!()
    };
    assert_eq!(p.points.len(), 2);
    assert_eq!(p.bulges, [0.0, 0.0]);
}

#[test]
fn a_solid_is_a_polygon_in_perimeter_order_and_a_face3d_in_stored_order() {
    let mut d = doc();
    // SOLID stores 1, 2, 4, 3: drawn in stored order that is a bow tie.
    put(
        &mut d,
        None,
        EntityType::Solid(Solid::new(v(0.0, 0.0, 0.0), v(1.0, 0.0, 0.0), v(0.0, 1.0, 0.0), v(1.0, 1.0, 0.0))),
    );
    put(
        &mut d,
        None,
        EntityType::Face3D(Face3D::new(v(0.0, 0.0, 5.0), v(1.0, 0.0, 5.0), v(1.0, 1.0, 5.0), v(0.0, 1.0, 5.0))),
    );
    // A degenerate quad is how DXF spells a triangle.
    put(
        &mut d,
        None,
        EntityType::Face3D(Face3D::new(v(0.0, 0.0, 9.0), v(1.0, 0.0, 9.0), v(1.0, 1.0, 9.0), v(1.0, 1.0, 9.0))),
    );
    let e = extract(&d);
    let polys: Vec<_> = e
        .records
        .iter()
        .filter_map(|r| match r {
            Record::Polygon(p) => Some(p.points.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(polys.len(), 3);
    assert_eq!(polys[0], [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [1.0, 1.0, 0.0], [0.0, 1.0, 0.0]]);
    assert_eq!(polys[1], [[0.0, 0.0, 5.0], [1.0, 0.0, 5.0], [1.0, 1.0, 5.0], [0.0, 1.0, 5.0]]);
    assert_eq!(polys[2].len(), 3, "a degenerate quad is three points");
}

#[test]
fn text_carries_its_alignment_style_width_and_an_in_range_rotation() {
    let mut d = doc();
    let mut t = Text::with_value("Hello\r\nworld\rend", v(1.0, 2.0, 0.0));
    t.height = 3.0;
    t.width_factor = 0.8;
    t.style = "Standard".into();
    t.rotation = -std::f64::consts::FRAC_PI_2;
    t.horizontal_alignment = acadrust::entities::TextHorizontalAlignment::Center;
    t.vertical_alignment = acadrust::entities::TextVerticalAlignment::Middle;
    t.alignment_point = Some(v(7.0, 8.0, 0.0));
    put(&mut d, None, EntityType::Text(t));
    let e = extract(&d);
    let Record::Text(t) = e.records.iter().find(|r| matches!(r, Record::Text(_))).unwrap() else {
        panic!()
    };
    assert_eq!((t.halign.as_str(), t.valign.as_str()), ("center", "middle"));
    assert_eq!(t.at, [7.0, 8.0, 0.0], "the alignment point is the anchor for anything but left/baseline");
    assert_eq!((t.height, t.width_factor, t.style.as_str()), (3.0, 0.8, "Standard"));
    assert!((t.rotation - 3.0 * std::f64::consts::FRAC_PI_2).abs() < 1e-12, "{}", t.rotation);
    assert!((0.0..std::f64::consts::TAU).contains(&t.rotation));
    assert_eq!(t.text, "Hello\nworld\nend", "line endings are LF");
}

#[test]
fn mtext_has_a_width_factor_of_one_and_maps_its_attachment_point() {
    let mut d = doc();
    let mut m = MText::with_value("block", v(0.0, 0.0, 0.0));
    m.attachment_point = acadrust::entities::AttachmentPoint::BottomRight;
    put(&mut d, None, EntityType::MText(m));
    let e = extract(&d);
    let Record::Text(t) = e.records.iter().find(|r| matches!(r, Record::Text(_))).unwrap() else {
        panic!()
    };
    assert_eq!((t.halign.as_str(), t.valign.as_str()), ("right", "bottom"));
    assert_eq!(t.width_factor, 1.0);
}

#[test]
fn a_circle_in_a_non_uniformly_scaled_block_is_an_ellipse_not_a_lie() {
    let mut d = doc();
    let b = block(&mut d, "ROUND");
    let mut c = Circle::new();
    c.radius = 2.0;
    put(&mut d, Some(b), EntityType::Circle(c));
    let mut insert = Insert::new("ROUND", v(0.0, 0.0, 0.0));
    insert.set_x_scale(3.0);
    put(&mut d, None, EntityType::Insert(insert));
    let e = extract(&d);
    assert_eq!(drawn(&e), want(&[("ellipse", 1)]));
    let Record::Ellipse(el) = e.records.iter().find(|r| matches!(r, Record::Ellipse(_))).unwrap() else {
        panic!()
    };
    assert!((el.ratio - 2.0 / 3.0).abs() < 1e-12, "{}", el.ratio);
}

#[test]
fn an_arc_keeps_its_stored_digits_when_nothing_transforms_it() {
    let mut d = doc();
    let mut a = ArcEntity::new();
    a.center = v(1.5, 2.5, 0.0);
    a.radius = 4.125;
    a.start_angle = 0.1;
    a.end_angle = 1.3;
    put(&mut d, None, EntityType::Arc(a));
    let e = extract(&d);
    let Record::Arc(a) = e.records.iter().find(|r| matches!(r, Record::Arc(_))).unwrap() else {
        panic!()
    };
    assert_eq!((a.center, a.radius, a.start, a.end), ([1.5, 2.5, 0.0], 4.125, 0.1, 1.3));
}

#[test]
fn views_are_ordered_by_tab_order_then_name_and_named_as_the_drawing_names_them() {
    let mut d = doc();
    d.add_layout("Zeta").unwrap();
    d.add_layout("Alpha").unwrap();
    let e = extract(&d);
    let views: Vec<_> = e
        .records
        .iter()
        .filter_map(|r| match r {
            Record::View(v) => Some((v.index, v.name.clone(), v.kind.clone())),
            _ => None,
        })
        .collect();
    assert_eq!(views[0], (0, "Model".to_string(), "model".to_string()));
    assert_eq!(views.iter().map(|v| v.0).collect::<Vec<_>>(), (0..views.len() as u64).collect::<Vec<_>>());
    assert_eq!(views.len(), 4);
    assert!(views[1..].iter().all(|v| v.2 == "layout"));
}

#[test]
fn warnings_are_sorted_by_category_then_entity_type_then_detail() {
    let mut d = doc();
    put(&mut d, None, EntityType::Insert(Insert::new("NOPE", v(0.0, 0.0, 0.0))));
    put(&mut d, None, EntityType::Viewport(Viewport::new()));
    let e = extract(&d);
    let w = warnings(&e);
    let mut sorted = w.clone();
    sorted.sort();
    assert_eq!(w, sorted);
    assert!(w.len() >= 2);
}

// ---- the sidecar from this side is complete ------------------------------------------

#[test]
fn the_sidecar_has_a_line_per_record_and_names_the_insert_path_down_to_the_leaf() {
    let mut d = doc();
    let inner = block(&mut d, "INNER");
    let leaf = put(&mut d, Some(inner), line_at(0.0));
    let outer = block(&mut d, "OUTER");
    let inner_insert = put(&mut d, Some(outer), EntityType::Insert(Insert::new("INNER", v(0.0, 0.0, 0.0))));
    let outer_insert = put(&mut d, None, EntityType::Insert(Insert::new("OUTER", v(0.0, 0.0, 0.0))));
    let e = extract(&d);
    assert_eq!(e.sidecar.len(), e.records.len());
    let at = e.records.iter().position(|r| matches!(r, Record::Line(_))).unwrap();
    let entry = &e.sidecar[at];
    let hex = |h: Handle| format!("{:X}", h.value());
    assert_eq!(entry.handles, [hex(outer_insert), hex(inner_insert), hex(leaf)]);
    assert_eq!(entry.record_type.as_deref(), Some("line"));
    assert_eq!(entry.dxf_name.as_deref(), Some("LINE"));
    assert!(entry.handles.iter().all(|h| h.bytes().all(|b| b.is_ascii_digit() || (b'A'..=b'F').contains(&b))));
}

// ---- the process: determinism, threads, the parser, the oracle ----------------------------

fn emit_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_emit-acadrust"))
}

fn scratch(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("emit-acadrust-{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn run_bin(id: &str, out: &Path, threads: Option<&str>) {
    let mut cmd = Command::new(emit_bin());
    cmd.args(["--fixture", id, "--out"]).arg(out);
    match threads {
        Some(n) => cmd.env("RAYON_NUM_THREADS", n),
        None => cmd.env_remove("RAYON_NUM_THREADS"),
    };
    let result = cmd.output().expect("run emit-acadrust");
    assert!(
        result.status.success(),
        "emit-acadrust {id} failed:\n{}",
        String::from_utf8_lossy(&result.stderr)
    );
}

const SAVES: [&str; 7] = [
    "acadsharp-ac1014",
    "acadsharp-ac1015",
    "acadsharp-ac1018",
    "acadsharp-ac1021",
    "acadsharp-ac1024",
    "acadsharp-ac1027",
    "acadsharp-ac1032",
];

#[test]
fn the_seven_saves_emit_non_empty_output_the_parser_reads_and_the_sidecar_matches() {
    let out = scratch("saves");
    for id in SAVES {
        run_bin(id, &out, None);
        let jsonl = std::fs::read_to_string(out.join(format!("{id}.acadrust.jsonl"))).unwrap();
        let records = parse_stream(&jsonl).unwrap_or_else(|e| panic!("{id}: {e}"));
        let drawn: usize = records
            .iter()
            .filter(|r| !matches!(r.kind(), "document" | "view" | "warning"))
            .count();
        assert!(drawn > 100, "{id}: only {drawn} drawn records");
        let sidecar = std::fs::read_to_string(out.join(format!("{id}.acadrust.src.jsonl"))).unwrap();
        Sidecar::parse(&sidecar, &records).unwrap_or_else(|e| panic!("{id}: {e}"));
        let run: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(out.join(format!("{id}.acadrust.run.json"))).unwrap()).unwrap();
        assert_eq!(run["fixture"], id);
        assert_eq!(run["engine"], "acadrust");
        assert_eq!(run["fixture_sha256"], fixtures::by_id(id).unwrap().sha256.as_str());
        assert_eq!(run["engine_pin"], include_str!("../ACADRUST_REV").lines().find(|l| !l.starts_with('#') && !l.trim().is_empty()).unwrap().trim());
        for key in ["engine_head", "target", "rayon_num_threads"] {
            assert!(run.get(key).is_some(), "run.json has no {key}");
        }
    }
}

#[test]
fn the_same_document_emitted_twice_is_byte_identical() {
    let a = scratch("twice-a");
    let b = scratch("twice-b");
    for id in ["acadsharp-ac1032", "g13_dimension", "g13_hatch"] {
        run_bin(id, &a, None);
        run_bin(id, &b, None);
        for suffix in ["jsonl", "src.jsonl"] {
            let name = format!("{id}.acadrust.{suffix}");
            assert_eq!(
                std::fs::read(a.join(&name)).unwrap(),
                std::fs::read(b.join(&name)).unwrap(),
                "{name}"
            );
        }
    }
}

#[test]
fn one_thread_and_the_default_give_identical_output() {
    // acadrust's DWG reader goes through src/io/dwg/parallel.rs, so this is a
    // real question and not a ritual.
    let one = scratch("threads-1");
    let many = scratch("threads-default");
    for id in SAVES {
        run_bin(id, &one, Some("1"));
        run_bin(id, &many, None);
        for suffix in ["jsonl", "src.jsonl"] {
            let name = format!("{id}.acadrust.{suffix}");
            assert_eq!(
                std::fs::read(one.join(&name)).unwrap(),
                std::fs::read(many.join(&name)).unwrap(),
                "{name}: RAYON_NUM_THREADS=1 and the default disagree"
            );
        }
    }
}

#[test]
fn a_fixture_the_manifest_does_not_know_is_refused() {
    let out = scratch("unknown");
    let result = Command::new(emit_bin())
        .args(["--fixture", "no-such-fixture", "--out"])
        .arg(&out)
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("no-such-fixture"));
    assert!(std::fs::read_dir(&out).unwrap().next().is_none(), "nothing may be written");
}

/// The kinds a stream holds, counted, warnings left out.
fn counts(records: &[Record]) -> BTreeMap<String, usize> {
    let mut out = BTreeMap::new();
    for r in records {
        if r.kind() != "warning" {
            *out.entry(r.kind().to_string()).or_default() += 1;
        }
    }
    out
}

#[test]
fn nothing_under_src_is_copied_from_upstream_acadrust() {
    use sha2::{Digest, Sha256};
    fn rs_files(dir: &Path, out: &mut Vec<PathBuf>) {
        for entry in std::fs::read_dir(dir).unwrap().flatten() {
            let p = entry.path();
            if p.is_dir() {
                rs_files(&p, out);
            } else if p.extension().is_some_and(|e| e == "rs") {
                out.push(p);
            }
        }
    }
    let hash = |p: &Path| format!("{:x}", Sha256::digest(std::fs::read(p).unwrap()));
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut theirs = Vec::new();
    rs_files(&root.join("..").join("acadrust").join("src"), &mut theirs);
    let mut ours = Vec::new();
    rs_files(&root.join("src"), &mut ours);
    // Positive controls: a walker that found nothing would pass this.
    assert!(theirs.len() > 100, "found only {} files in ../acadrust/src", theirs.len());
    assert!(ours.len() >= 8, "found only {} files in src/", ours.len());
    let their_hashes: BTreeMap<String, &PathBuf> = theirs.iter().map(|p| (hash(p), p)).collect();
    for file in &ours {
        if let Some(upstream) = their_hashes.get(&hash(file)) {
            panic!("{} is byte for byte {}", file.display(), upstream.display());
        }
    }
    // And the suite depends on acadrust, it does not contain it.
    let manifest = std::fs::read_to_string(root.join("Cargo.toml")).unwrap();
    assert!(manifest.contains("path = \"../acadrust\""));
}

#[test]
fn the_flatteners_header_says_where_it_came_from_and_that_it_is_ours() {
    let text = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("src/acadrust_emit.rs")).unwrap();
    let head: String = text.lines().take(20).collect::<Vec<_>>().join("\n");
    assert!(head.contains("tests/viprs/mod.rs"), "{head}");
    assert!(head.contains("MIT"), "the moved code comes under this repository's licence: {head}");
}

#[test]
fn the_sample_ac1032_counts_agree_with_the_oracles_except_where_the_doc_lists_the_difference() {
    let ours = counts(&emit_fixture("acadsharp-ac1032").records);
    let theirs = counts(&frozen("acadsharp-ac1032"));
    let listed = emitters_doc_differences("acadsharp-ac1032");
    let mut kinds: Vec<&String> = ours.keys().chain(theirs.keys()).collect();
    kinds.sort();
    kinds.dedup();
    for kind in kinds {
        let (a, o) = (ours.get(kind).copied().unwrap_or(0), theirs.get(kind).copied().unwrap_or(0));
        match listed.get(kind.as_str()) {
            None => assert_eq!(a, o, "{kind}: acadrust {a} against the oracle's {o}, and docs/EMITTERS.md lists no difference"),
            Some(&(lo, la)) => {
                assert_eq!((o, a), (lo, la), "{kind}: docs/EMITTERS.md says oracle {lo} / acadrust {la} and the run says {o} / {a}: update the row")
            }
        }
    }
    // A row for a kind that no longer differs is stale.
    for (kind, (lo, la)) in &listed {
        assert_ne!(lo, la, "docs/EMITTERS.md lists {kind} as different and it is {lo} / {la}");
    }
}

/// Rows of `docs/EMITTERS.md`'s acadrust section for one fixture:
/// `| fixture | kind | oracle | acadrust | why |`.
fn emitters_doc_differences(fixture: &str) -> BTreeMap<String, (usize, usize)> {
    let text = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("docs/EMITTERS.md")).unwrap();
    let section = text.split("## acadrust").nth(1).expect("docs/EMITTERS.md has an acadrust section");
    let section = section.split("\n## ").next().unwrap();
    let mut out = BTreeMap::new();
    for line in section.lines().filter(|l| l.trim_start().starts_with('|')) {
        let cells: Vec<String> = line.trim().trim_matches('|').split('|').map(|c| c.trim().trim_matches('`').to_string()).collect();
        if cells.len() >= 5 && cells[0] == fixture {
            let (Ok(o), Ok(a)) = (cells[2].parse(), cells[3].parse()) else { continue };
            assert!(!cells[4].is_empty(), "{fixture}.{}: a difference with no reason", cells[1]);
            out.insert(cells[1].clone(), (o, a));
        }
    }
    out
}
