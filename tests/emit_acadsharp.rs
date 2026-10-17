//! `emit-acadsharp` and the adapter behind it (R3.1, #22).
//!
//! Three kinds of check. The mapping rows, on hand-built items, so they run with
//! no native library at all. The native lane, which decodes real drawings through
//! the safe API and is skipped with its reason printed when nothing is linked,
//! unless `ACADSHARP_REQUIRE_NATIVE=1`, which turns that skip into a failure (the
//! crate's own build script already stops the build, so this is the second belt).
//! And the process-level properties of the binary.

use std::path::{Path, PathBuf};
use std::process::Command;

use acadsharp_rs::item::{Arc, Circle, Ellipse, Line, Polyline, Spline, Text};
use acadsharp_rs::{
    Decoder, Document, Item, ItemHandle, Limits, Origin, Primitive, View, Warning, WarningCode,
};
use acadsharp_rs_tests::acadsharp_emit::{ViewWalk, extract_bytes, map_walk};
use acadsharp_rs_tests::canonical::{Record, Sidecar, parse_stream, render_sidecar, render_stream};
use acadsharp_rs_tests::emit::Emission;
use acadsharp_rs_tests::fixtures::{self, ManifestFile};

// ---- helpers ------------------------------------------------------------------

fn view(index: u32, kind: u32, name: &str) -> View {
    View::new(index, kind, [0.0, 0.0, 1.0, 1.0], 0, name)
}

fn walk(items: Vec<Item>) -> Vec<ViewWalk> {
    vec![ViewWalk {
        view: view(0, 0, "Model"),
        items,
    }]
}

fn origin(handle: u64, from_insert: bool) -> Origin {
    let mut o = Origin::default();
    o.item_handle = Some(ItemHandle::new(handle));
    o.from_expanded_insert = from_insert;
    o
}

fn entity_records(e: &Emission) -> Vec<&Record> {
    e.records
        .iter()
        .filter(|r| !matches!(r.kind(), "document" | "view" | "warning"))
        .collect()
}

fn kinds(e: &Emission) -> Vec<&'static str> {
    e.records.iter().map(Record::kind).collect()
}

const TAU: f64 = std::f64::consts::TAU;

// ---- the mapping rows, with no native library -----------------------------------

#[test]
fn the_framing_records_open_the_stream_and_the_warnings_close_it() {
    let mut w = Warning::new(WarningCode::UNSUPPORTED_ENTITY, "HELIX is not flattened");
    w.entity = Some(ItemHandle::new(0x2A));
    let mut line = Line::default();
    line.end = [1.0, 0.0, 0.0];
    let e = map_walk(
        1032,
        &walk(vec![
            Item::Warning(w),
            Item::Primitive(Primitive::Line(line)),
        ]),
    );
    assert_eq!(kinds(&e), ["document", "view", "line", "warning"]);
    match &e.records[0] {
        Record::Document(d) => {
            assert_eq!(d.format, "dwg");
            assert_eq!(d.acad_version, "AC1032");
            assert_eq!(d.maintenance_version, 0, "wire 2 does not carry it");
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn views_are_named_and_kinded_as_the_wire_gives_them() {
    let walks = vec![
        ViewWalk {
            view: view(0, 0, "Model"),
            items: vec![],
        },
        ViewWalk {
            view: view(1, 1, "Layout1"),
            items: vec![],
        },
        ViewWalk {
            view: view(2, 2, "Odd"),
            items: vec![],
        },
    ];
    let e = map_walk(1032, &walks);
    let views: Vec<(u64, String, String)> = e
        .records
        .iter()
        .filter_map(|r| match r {
            Record::View(v) => Some((v.index, v.name.clone(), v.kind.clone())),
            _ => None,
        })
        .collect();
    assert_eq!(
        views,
        [
            (0, "Model".into(), "model".into()),
            (1, "Layout1".into(), "layout".into()),
            (2, "Odd".into(), "unknown".into()),
        ]
    );
}

#[test]
fn a_polyline_is_projected_into_the_ocs_of_its_normal_and_always_has_bulges() {
    // A polyline in the XZ plane (normal +Y): world (1, 5, 2) is OCS (-1, 2, 5)
    // because the x axis of a +Y normal is -X and the y axis is +Z.
    let mut p = Polyline::default();
    p.normal = [0.0, 1.0, 0.0];
    p.vertices = vec![[1.0, 5.0, 2.0], [3.0, 5.0, 2.0]];
    let e = map_walk(1032, &walk(vec![Item::Primitive(Primitive::Polyline(p))]));
    match entity_records(&e)[0] {
        Record::Polyline(p) => {
            assert_eq!(p.points, [[-1.0, 2.0, 5.0], [-3.0, 2.0, 5.0]]);
            assert_eq!(p.bulges, [0.0, 0.0], "bulge_count 0 is every span straight");
            assert_eq!(p.normal, [0.0, 1.0, 0.0]);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_polylines_own_bulges_cross_unchanged_and_closed_is_kept() {
    let mut p = Polyline::default();
    p.closed = true;
    p.normal = [0.0, 0.0, 1.0];
    p.vertices = vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [1.0, 1.0, 0.0]];
    p.bulges = vec![0.5, 0.0, -1.0];
    let e = map_walk(1032, &walk(vec![Item::Primitive(Primitive::Polyline(p))]));
    match entity_records(&e)[0] {
        Record::Polyline(p) => {
            assert!(p.closed);
            assert_eq!(p.bulges, [0.5, 0.0, -1.0]);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_circle_centre_is_in_its_own_ocs() {
    let mut c = Circle::default();
    c.centre = [1.0, 5.0, 2.0];
    c.radius = 3.0;
    c.normal = [0.0, 1.0, 0.0];
    let e = map_walk(1032, &walk(vec![Item::Primitive(Primitive::Circle(c))]));
    match entity_records(&e)[0] {
        Record::Circle(c) => {
            assert_eq!(c.center, [-1.0, 2.0, 5.0]);
            assert_eq!(c.radius, 3.0);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn an_arc_sweeps_counter_clockwise_from_a_start_in_range() {
    let mut a = Arc::default();
    a.normal = [0.0, 0.0, 1.0];
    a.radius = 1.0;
    a.start_angle = -0.5;
    a.end_angle = 0.5;
    let e = map_walk(1032, &walk(vec![Item::Primitive(Primitive::Arc(a))]));
    match entity_records(&e)[0] {
        Record::Arc(a) => {
            assert!((0.0..TAU).contains(&a.start), "{}", a.start);
            assert!(a.end > a.start);
            assert!((a.end - a.start - 1.0).abs() < 1e-12);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn an_arc_in_range_keeps_the_digits_the_wire_gave() {
    let mut a = Arc::default();
    a.normal = [0.0, 0.0, 1.0];
    a.start_angle = 0.123_456_789_012_345_67;
    a.end_angle = std::f64::consts::FRAC_PI_2;
    let e = map_walk(1032, &walk(vec![Item::Primitive(Primitive::Arc(a))]));
    match entity_records(&e)[0] {
        Record::Arc(r) => {
            assert_eq!(r.start.to_bits(), a.start_angle.to_bits());
            assert_eq!(r.end.to_bits(), a.end_angle.to_bits());
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn an_ellipse_carries_its_major_axis_in_the_ocs_and_a_full_turn_ends_a_turn_after_its_start() {
    let mut el = Ellipse::default();
    el.normal = [0.0, 0.0, 1.0];
    el.major_axis = [4.0, 0.0, 0.0];
    el.ratio = 0.25;
    el.start_param = 0.0;
    el.end_param = TAU;
    let e = map_walk(1032, &walk(vec![Item::Primitive(Primitive::Ellipse(el))]));
    match entity_records(&e)[0] {
        Record::Ellipse(r) => {
            assert_eq!(r.major, [4.0, 0.0, 0.0]);
            assert_eq!(r.start, 0.0);
            assert_eq!(r.end, TAU);
            assert_eq!(r.ratio, 0.25);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_spline_has_weights_only_when_rational_and_no_fit_points() {
    let mut s = Spline::default();
    s.degree = 3;
    s.flags = Spline::CLOSED | Spline::PERIODIC;
    s.knots = vec![0.0, 0.0, 1.0, 1.0];
    s.controls = vec![[0.0; 3], [1.0; 3]];
    s.weights = vec![1.0, 1.0];
    let e = map_walk(
        1032,
        &walk(vec![Item::Primitive(Primitive::Spline(s.clone()))]),
    );
    match entity_records(&e)[0] {
        Record::Spline(r) => {
            assert!(r.closed && r.periodic);
            assert!(r.weights.is_empty(), "not rational, so no weights");
            assert!(r.fit_points.is_empty());
            assert_eq!(r.degree, 3);
        }
        other => panic!("{other:?}"),
    }
    s.flags |= Spline::RATIONAL;
    let e = map_walk(1032, &walk(vec![Item::Primitive(Primitive::Spline(s))]));
    match entity_records(&e)[0] {
        Record::Spline(r) => assert_eq!(r.weights, [1.0, 1.0]),
        other => panic!("{other:?}"),
    }
}

#[test]
fn text_is_normalised_in_rotation_and_line_endings_and_defaults_what_the_wire_lacks() {
    let mut t = Text::default();
    t.position = [1.0, 2.0, 3.0];
    t.height = 2.5;
    t.rotation = -1.0;
    t.text = "a\r\nb\rc".to_string();
    let e = map_walk(1032, &walk(vec![Item::Primitive(Primitive::Text(t))]));
    match entity_records(&e)[0] {
        Record::Text(r) => {
            assert_eq!(r.at, [1.0, 2.0, 3.0]);
            assert_eq!(r.rotation, TAU - 1.0);
            assert_eq!(r.text, "a\nb\nc");
            assert_eq!((r.halign.as_str(), r.valign.as_str()), ("left", "baseline"));
            assert_eq!(r.width_factor, 1.0);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_polygon_is_a_polygon_and_its_bulges_have_nowhere_to_go() {
    let mut p = Polyline::default();
    p.closed = true;
    p.vertices = vec![[0.0; 3], [1.0, 0.0, 0.0], [1.0, 1.0, 0.0]];
    p.bulges = vec![0.0, 0.5, 0.0];
    let e = map_walk(1032, &walk(vec![Item::Primitive(Primitive::Polygon(p))]));
    match entity_records(&e)[0] {
        Record::Polygon(r) => assert_eq!(r.points.len(), 3),
        other => panic!("{other:?}"),
    }
}

#[test]
fn the_sidecar_has_a_line_per_record_names_the_leaf_and_marks_expanded_inserts() {
    let mut a = Line::default();
    a.origin = origin(0x1F, false);
    let mut b = Line::default();
    b.origin = origin(0xAB, true);
    let e = map_walk(
        1032,
        &walk(vec![
            Item::Primitive(Primitive::Line(a)),
            Item::Primitive(Primitive::Line(b)),
        ]),
    );
    assert_eq!(e.sidecar.len(), e.records.len());
    let lines: Vec<_> = e.sidecar.iter().filter(|s| s.index >= 2).collect();
    assert_eq!(lines[0].handles, ["1F"]);
    assert_eq!(lines[0].from_insert, Some(false));
    assert_eq!(lines[1].handles, ["AB"]);
    assert_eq!(lines[1].from_insert, Some(true));
    assert!(
        e.sidecar.iter().all(|s| s.record_type.is_none()),
        "no entity type on the wire, so no t"
    );
    // And the parser reads what the renderer wrote.
    let text = render_stream(&e.records).unwrap();
    let records = parse_stream(&text).unwrap();
    Sidecar::parse(&render_sidecar(&e.sidecar), &records).unwrap();
}

#[test]
fn warnings_map_to_categories_and_sort_by_category_entity_type_and_detail() {
    let w = |code: WarningCode, msg: &str| Item::Warning(Warning::new(code, msg));
    let e = map_walk(
        1032,
        &walk(vec![
            w(
                WarningCode::UNSUPPORTED_ENTITY,
                "WIPEOUT could not be lowered",
            ),
            w(
                WarningCode::UNRESOLVED_BLOCK,
                "block NAME|1883 is not there",
            ),
            w(WarningCode::NON_UNIFORM_BLOCK_SCALE, "scale"),
            w(WarningCode::NON_FINITE_GEOMETRY, "nan"),
            w(WarningCode::READER_NOTIFICATION, "note"),
            w(WarningCode::UNSUPPORTED_ENTITY, "ACAD_TABLE nope"),
        ]),
    );
    let got: Vec<(String, String, String)> = e
        .records
        .iter()
        .filter_map(|r| match r {
            Record::Warning(w) => {
                Some((w.category.clone(), w.entity_type.clone(), w.detail.clone()))
            }
            _ => None,
        })
        .collect();
    let mut sorted = got.clone();
    sorted.sort();
    assert_eq!(got, sorted, "warnings are sorted");
    assert_eq!(got.len(), 6, "duplicates and all, nothing dropped");
    let cats: Vec<&str> = got.iter().map(|g| g.0.as_str()).collect();
    for want in [
        "unsupported_entity",
        "missing_reference",
        "malformed_geometry",
        "invalid_entity",
        "reader_notification",
    ] {
        assert!(cats.contains(&want), "{want} missing from {cats:?}");
    }
    assert!(
        got.iter().any(|g| g.1 == "WIPEOUT") && got.iter().any(|g| g.1 == "ACAD_TABLE"),
        "the kind is read off the message's first token: {got:?}"
    );
    assert!(
        got.iter().all(|g| !g.2.contains("1883")),
        "a detail never carries a handle: {got:?}"
    );
}

#[test]
fn the_forward_probe_is_skipped_and_any_other_unknown_record_is_a_warning() {
    let probe = Item::Unknown {
        record_type: acadsharp_rs::record_type::FORWARD_PROBE_MIN,
        payload: vec![0; 8],
    };
    let other = Item::Unknown {
        record_type: 40,
        payload: vec![],
    };
    let e = map_walk(1032, &walk(vec![probe]));
    assert_eq!(
        kinds(&e),
        ["document", "view"],
        "the library's own probe says nothing about the drawing"
    );
    let e = map_walk(1032, &walk(vec![other]));
    assert_eq!(
        kinds(&e),
        ["document", "view", "warning"],
        "an unknown record is never dropped silently"
    );
}

#[test]
fn the_same_walk_twice_is_the_same_emission() {
    let mut l = Line::default();
    l.end = [1.0, 2.0, 3.0];
    let w = walk(vec![Item::Primitive(Primitive::Line(l))]);
    assert_eq!(map_walk(1032, &w), map_walk(1032, &w));
}

// ---- the native lane ---------------------------------------------------------------

/// A decoder, or `None` with the reason printed. Under
/// `ACADSHARP_REQUIRE_NATIVE=1` there is no `None`: a skipped lane is the same
/// colour as a passing one.
fn decoder() -> Option<Decoder> {
    match Decoder::new() {
        Ok(d) => Some(d),
        Err(e) if e.is_unlinked() => {
            assert!(
                std::env::var("ACADSHARP_REQUIRE_NATIVE").as_deref() != Ok("1"),
                "ACADSHARP_REQUIRE_NATIVE=1 and nothing is linked"
            );
            eprintln!("skipped: no native library is linked (set ACADSHARP_NATIVE_DIR)");
            None
        }
        Err(e) => panic!("the handshake failed: {e}"),
    }
}

#[test]
fn sample_ac1032_through_the_safe_api_has_four_views_in_tab_order_and_163_model_entities() {
    let Some(decoder) = decoder() else { return };
    let bytes = fixtures::load("acadsharp-ac1032").unwrap();
    let doc = Document::open_bytes(&decoder, &bytes, &Limits::new()).unwrap();
    let views = doc.views().unwrap();
    let names: Vec<&str> = views.iter().map(View::name).collect();
    assert_eq!(names, ["Model", "Layout1", "Layout2", "MyLayout"]);
    assert_eq!(
        doc.view(0).unwrap().entity_count(),
        163,
        "the model-space count AutoCAD's own census holds"
    );
}

fn emit(id: &str) -> Result<Emission, String> {
    let bytes = fixtures::load(id).unwrap();
    extract_bytes(&bytes).map_err(|e| e.to_string())
}

/// Drawings whose every entity is one the wire refuses, so the stream is
/// warnings and no geometry. Named, because a stream with no geometry in it is
/// otherwise the shape of a reader that read nothing. These are the published
/// archive's refusals (`viprs.1` predates the lowering of SOLID and 3DFACE and
/// the POINT record); the rebuilt archive draws some of them.
const WARNING_ONLY_ON_VIPRS_1: &[&str] =
    &["g13_face3d", "g13_point", "g13_solid", "g13_unsupported"];

/// Tier C rows the reader refuses by one of its own bounds, by design of the
/// fixture: opened, counted, and refused with the reader's message.
const REFUSED_BY_A_BOUND: &[&str] = &["g13_dimension_deep"];

#[test]
fn every_drawing_row_yields_a_complete_stream_with_geometry_or_a_named_refusal() {
    if decoder().is_none() {
        return;
    }
    let mut bad = Vec::new();
    for f in fixtures::all() {
        let e = match emit(&f.id) {
            Ok(e) => e,
            Err(m) if REFUSED_BY_A_BOUND.contains(&f.id.as_str()) => {
                assert!(m.contains("bound"), "{}: {m}", f.id);
                continue;
            }
            Err(m) => {
                bad.push(format!("{}: {m}", f.id));
                continue;
            }
        };
        let geometry = entity_records(&e).len();
        let warned = e.records.iter().any(|r| r.kind() == "warning");
        match (f.tier.as_deref(), f.no_primitives.is_some(), geometry) {
            (Some("C"), _, _) | (_, true, _) => {}
            (_, false, 0) if warned && WARNING_ONLY_ON_VIPRS_1.contains(&f.id.as_str()) => {}
            (_, false, 0) => bad.push(format!(
                "{}: no geometry record and not a listed refusal",
                f.id
            )),
            _ => {}
        }
    }
    assert!(bad.is_empty(), "{bad:#?}");
}

#[test]
fn the_ac1009_stub_is_a_named_refusal() {
    let Some(decoder) = decoder() else { return };
    let r = fixtures::refusals()
        .iter()
        .find(|r| r.id == "g13_ac1009")
        .expect("the AC1009 stub is in the manifest as a refusal");
    let bytes = r.load().unwrap();
    let err = Document::open_bytes(&decoder, &bytes, &Limits::new())
        .and_then(|d| d.decode(0).map(|_| ()))
        .expect_err("AC1009 is refused");
    assert!(
        matches!(err, acadsharp_rs::Error::UnsupportedFormat { .. }),
        "{err:?}"
    );
}

#[test]
fn the_seven_saves_agree_on_the_view_names() {
    if decoder().is_none() {
        return;
    }
    let names = |id: &str| -> Vec<String> {
        emit(id)
            .unwrap()
            .records
            .iter()
            .filter_map(|r| match r {
                Record::View(v) => Some(v.name.clone()),
                _ => None,
            })
            .collect()
    };
    let want = names("acadsharp-ac1032");
    for id in [
        "acadsharp-ac1018",
        "acadsharp-ac1021",
        "acadsharp-ac1024",
        "acadsharp-ac1027",
    ] {
        assert_eq!(names(id), want, "{id}");
    }
}

#[test]
fn what_the_adapter_writes_the_parser_reads_and_the_sidecar_matches() {
    if decoder().is_none() {
        return;
    }
    for id in [
        "acadsharp-ac1032",
        "g13_text",
        "g13_ocs_plane",
        "g13_insert",
    ] {
        let e = emit(id).unwrap();
        let text = render_stream(&e.records).unwrap();
        let back = parse_stream(&text).unwrap();
        assert_eq!(back.len(), e.records.len(), "{id}");
        let side = render_sidecar(&e.sidecar);
        Sidecar::parse(&side, &back).unwrap_or_else(|err| panic!("{id}: {err:?}"));
        assert_eq!(side.lines().count(), back.len(), "{id}");
    }
}

// ---- the binary ------------------------------------------------------------------------

fn emit_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_emit-acadsharp"))
}

fn scratch(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(name);
    if dir.exists() {
        std::fs::remove_dir_all(&dir).unwrap();
    }
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn run_bin(id: &str, out: &Path) -> std::process::Output {
    Command::new(emit_bin())
        .args(["--fixture", id, "--out"])
        .arg(out)
        .output()
        .unwrap()
}

#[test]
fn the_same_fixture_emitted_twice_is_byte_identical_and_run_json_has_nothing_that_moves() {
    if decoder().is_none() {
        return;
    }
    let (a, b) = (scratch("acadsharp_twice_a"), scratch("acadsharp_twice_b"));
    for dir in [&a, &b] {
        let out = run_bin("acadsharp-ac1032", dir);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    for name in [
        "acadsharp-ac1032.acadsharp.jsonl",
        "acadsharp-ac1032.acadsharp.src.jsonl",
        "acadsharp-ac1032.acadsharp.run.json",
    ] {
        assert_eq!(
            std::fs::read(a.join(name)).unwrap(),
            std::fs::read(b.join(name)).unwrap(),
            "{name}"
        );
    }
    let run: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(a.join("acadsharp-ac1032.acadsharp.run.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(run["engine"], "acadsharp");
    assert_eq!(run["fixture"], "acadsharp-ac1032");
    assert!(run["native_artifact_version"].is_string(), "{run}");
    assert!(run["native_library_sha256"].is_string(), "{run}");
    let text = run.to_string();
    for forbidden in ["/tmp", "/work", "/Users", "/home"] {
        assert!(!text.contains(forbidden), "run.json carries a path: {text}");
    }
}

#[test]
fn a_fixture_the_manifest_does_not_know_is_refused() {
    let out = run_bin("no-such-fixture", &scratch("acadsharp_unknown"));
    assert_eq!(out.status.code(), Some(2));
}

#[test]
fn a_binary_built_without_an_archive_exits_non_zero_and_says_unlinked() {
    // The real property: no archive means a refusal at run time, never an empty
    // success. It needs a second build of this package with the archive left out,
    // in a target directory of its own so the outer build is not disturbed.
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
    let target = scratch("acadsharp_unlinked_target");
    let out_dir = scratch("acadsharp_unlinked_out");
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
    let built = Command::new(cargo)
        .args([
            "build",
            "--offline",
            "--bin",
            "emit-acadsharp",
            "--manifest-path",
        ])
        .arg(&manifest)
        .arg("--target-dir")
        .arg(&target)
        .env_remove("ACADSHARP_NATIVE_DIR")
        .env_remove("ACADSHARP_REQUIRE_NATIVE")
        .output()
        .unwrap();
    assert!(
        built.status.success(),
        "{}",
        String::from_utf8_lossy(&built.stderr)
    );
    let out = Command::new(target.join("debug").join("emit-acadsharp"))
        .args(["--fixture", "g13_line", "--out"])
        .arg(&out_dir)
        .env_remove("ACADSHARP_NATIVE_DIR")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("Unlinked"), "{err}");
    assert!(
        std::fs::read_dir(&out_dir).unwrap().next().is_none(),
        "a refusal writes nothing"
    );
}
