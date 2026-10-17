//! H4.4: the claim test for each regression class, over the committed expected
//! output alone.
//!
//! A fixture that stops exercising its class (because the generator changed, or
//! ACadSharp did) would otherwise keep passing every other test, since the
//! differential only compares two engines with each other. Each claim here is a
//! property of the *reference* stream: it needs no native library and no .NET,
//! so it runs in the ordinary `cargo test`.
//!
//! Every claim is a function from a record list to `Result`, and each has two
//! tests: the claim over the real stream, and the same claim over the stream with
//! one thing broken, which has to fail. A claim that cannot fail is a claim
//! about nothing. Where the issue text and schema 1 disagree (schema 1 keeps a
//! polyline's bulges and draws a hatch as one record per boundary edge) the
//! claim is stated in schema 1's terms and the doc on the claim says so.
#![allow(clippy::type_complexity)]

use acadsharp_rs_tests::canonical::{self, Record, Text, Vec3};
use acadsharp_rs_tests::fixtures;

fn stream(id: &str) -> Vec<Record> {
    let fx =
        fixtures::by_id(id).unwrap_or_else(|| panic!("{id} has no row in fixtures/manifest.toml"));
    let bytes = fx
        .load_reference_jsonl()
        .unwrap_or_else(|e| panic!("{id}: reference output: {e}"));
    canonical::parse_stream(std::str::from_utf8(&bytes).expect("UTF-8"))
        .unwrap_or_else(|e| panic!("{id}: {e}"))
}

fn is_warning(r: &Record) -> bool {
    matches!(r, Record::Warning(_))
}

fn lines(recs: &[Record]) -> Vec<&canonical::Line> {
    recs.iter()
        .filter_map(|r| {
            if let Record::Line(l) = r {
                Some(l)
            } else {
                None
            }
        })
        .collect()
}

fn polylines(recs: &[Record]) -> Vec<&canonical::Polyline> {
    recs.iter()
        .filter_map(|r| {
            if let Record::Polyline(p) = r {
                Some(p)
            } else {
                None
            }
        })
        .collect()
}

fn texts(recs: &[Record]) -> Vec<&Text> {
    recs.iter()
        .filter_map(|r| {
            if let Record::Text(t) = r {
                Some(t)
            } else {
                None
            }
        })
        .collect()
}

fn count(recs: &[Record], kind: &str) -> usize {
    recs.iter().filter(|r| r.kind() == kind).count()
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-9
}

fn ensure(cond: bool, what: impl FnOnce() -> String) -> Result<(), String> {
    if cond { Ok(()) } else { Err(what()) }
}

// ------------------------------------------------------------------ bulge ---

/// Three bulge segments of known sagitta.
///
/// The issue asks for "exactly three `arc` records and zero straight `line`
/// records". Schema 1 does not decompose a polyline: it keeps the bulges
/// (`docs/CANONICAL_SCHEMA.md`, entity mapping), so the same claim in its terms
/// is one `polyline` whose bulges are the three the generator wrote, each
/// `2 * sagitta / chord`, and no `line` and no `arc` anywhere. A bulge of 0 would
/// be a straight segment, which is the failure this guards against.
fn claim_bulge(recs: &[Record]) -> Result<(), String> {
    ensure(count(recs, "line") == 0, || {
        "a straight line record is present".into()
    })?;
    ensure(count(recs, "arc") == 0, || {
        "an arc record is present, and schema 1 keeps bulges".into()
    })?;
    let pls = polylines(recs);
    ensure(pls.len() == 1, || {
        format!("{} polyline records, expected 1", pls.len())
    })?;
    let p = pls[0];
    ensure(p.points.len() == 4 && p.bulges.len() == 4, || {
        format!(
            "{} points and {} bulges, expected 4 and 4",
            p.points.len(),
            p.bulges.len()
        )
    })?;
    // chord 10 each; sagitta 2, 5 (a semicircle) and -1.25 (bulging the other way).
    let want: [(f64, f64); 3] = [(10.0, 2.0), (10.0, 5.0), (10.0, -1.25)];
    for (i, (chord, sagitta)) in want.into_iter().enumerate() {
        let got_chord =
            (p.points[i + 1][0] - p.points[i][0]).hypot(p.points[i + 1][1] - p.points[i][1]);
        ensure(near(got_chord, chord), || {
            format!("segment {i} has chord {got_chord}, expected {chord}")
        })?;
        let bulge = 2.0 * sagitta / chord;
        ensure(near(p.bulges[i], bulge), || {
            format!("segment {i} has bulge {}, expected {bulge}", p.bulges[i])
        })?;
    }
    ensure(p.bulges[3] == 0.0, || {
        "the last vertex has a segment after it".into()
    })?;
    ensure(p.bulges.iter().filter(|b| **b != 0.0).count() == 3, || {
        "not exactly three non-zero bulges".into()
    })
}

#[test]
fn bulge_claim() {
    claim_bulge(&stream("regression_bulge")).unwrap();
}

#[test]
fn bulge_claim_fails_on_zero_bulges() {
    let mut recs = stream("regression_bulge");
    for r in &mut recs {
        if let Record::Polyline(p) = r {
            p.bulges.iter_mut().for_each(|b| *b = 0.0);
        }
    }
    let err = claim_bulge(&recs).unwrap_err();
    assert!(err.contains("segment 0 has bulge"), "{err}");
}

#[test]
fn bulge_claim_fails_when_the_polyline_was_decomposed_to_lines() {
    let mut recs = stream("regression_bulge");
    let at = recs.iter().position(|r| r.kind() == "polyline").unwrap();
    recs[at] = Record::Line(canonical::Line {
        view: 0,
        a: [0.0; 3],
        b: [10.0, 0.0, 0.0],
    });
    assert!(claim_bulge(&recs).unwrap_err().contains("line record"));
}

// ------------------------------------------------------- insert_transform ---

/// The line GEN_INNER holds, (0,0)-(1,0), seen through GEN_INNER at (5,2) inside
/// GEN_OUTER, which is inserted at (100,50) with scale (2,3) and a 30 degree
/// rotation. Computed here by hand, in the order the schema says: scale, rotate,
/// translate.
fn claim_insert_transform(recs: &[Record]) -> Result<(), String> {
    let ls = lines(recs);
    ensure(ls.len() == 1, || {
        format!("{} line records, expected 1", ls.len())
    })?;
    let (s, c) = (std::f64::consts::PI / 6.0).sin_cos();
    let world = |x: f64, y: f64| -> Vec3 {
        let (sx, sy) = ((x + 5.0) * 2.0, (y + 2.0) * 3.0);
        [sx * c - sy * s + 100.0, sx * s + sy * c + 50.0, 0.0]
    };
    let (a, b) = (world(0.0, 0.0), world(1.0, 0.0));
    let l = ls[0];
    for (name, got, want) in [("start", l.a, a), ("end", l.b, b)] {
        for axis in 0..3 {
            ensure(near(got[axis], want[axis]), || {
                format!("{name}[{axis}] is {}, expected {}", got[axis], want[axis])
            })?;
        }
    }
    // The transform is not the identity: a line that never moved passes the
    // check above only if the hand computation was wrong too.
    ensure(!near(l.a[0], 5.0), || {
        "the line was not transformed at all".into()
    })
}

#[test]
fn insert_transform_claim() {
    claim_insert_transform(&stream("regression_insert_transform")).unwrap();
}

#[test]
fn insert_transform_claim_fails_when_rotation_is_dropped() {
    let mut recs = stream("regression_insert_transform");
    for r in &mut recs {
        if let Record::Line(l) = r {
            // scale and translate only: what a reader that forgot the rotation draws
            l.a = [100.0 + 10.0, 50.0 + 6.0, 0.0];
            l.b = [100.0 + 12.0, 50.0 + 6.0, 0.0];
        }
    }
    assert!(
        claim_insert_transform(&recs)
            .unwrap_err()
            .contains("start[0]")
    );
}

// ------------------------------------------------------ text_zero_height ---

/// What `TextZeroHeight.LongString` writes.
fn long_string() -> String {
    (0..6000usize)
        .map(|i| char::from(b'A' + ((i * 7 + i / 26) % 26) as u8))
        .collect()
}

/// A TEXT and an MTEXT with height 0 and an MTEXT with a 6000-character string.
///
/// The issue asks for a text record per entity with the zero height preserved.
/// Measured against ACadSharp 3.7.1 that cannot hold: the writer writes a zero
/// height and the reader throws on reading it back, so neither zero-height entity
/// survives. What the reference pins instead is that they are lost *loudly*: two
/// `invalid_entity` / `reader_error` warnings and no text record with height 0.
/// The long string beside them is the part that has to survive intact.
fn claim_text_zero_height(recs: &[Record]) -> Result<(), String> {
    let ts = texts(recs);
    ensure(ts.len() == 1, || {
        format!("{} text records, expected the one long MTEXT", ts.len())
    })?;
    let t = ts[0];
    ensure(near(t.height, 2.5), || {
        format!("the long text has height {}", t.height)
    })?;
    let want = long_string();
    ensure(t.text.chars().count() == 6000, || {
        format!(
            "the long string has {} characters, expected 6000",
            t.text.chars().count()
        )
    })?;
    ensure(t.text == want, || {
        "the long string came back different".into()
    })?;
    let lost = recs
        .iter()
        .filter(|r| matches!(r, Record::Warning(w) if w.category == "invalid_entity" && w.detail == "reader_error"))
        .count();
    ensure(lost == 2, || {
        format!("{lost} reader_error warnings, expected 2 (one per zero-height entity)")
    })
}

#[test]
fn text_zero_height_claim() {
    claim_text_zero_height(&stream("regression_text_zero_height")).unwrap();
}

#[test]
fn text_zero_height_claim_fails_when_the_string_is_truncated() {
    let mut recs = stream("regression_text_zero_height");
    for r in &mut recs {
        if let Record::Text(t) = r {
            t.text.truncate(5999);
        }
    }
    assert!(claim_text_zero_height(&recs).unwrap_err().contains("5999"));
}

#[test]
fn text_zero_height_claim_fails_when_a_loss_is_silent() {
    let mut recs = stream("regression_text_zero_height");
    let at = recs
        .iter()
        .position(|r| matches!(r, Record::Warning(w) if w.detail == "reader_error"))
        .unwrap();
    recs.remove(at);
    assert!(
        claim_text_zero_height(&recs)
            .unwrap_err()
            .contains("reader_error")
    );
}

// ------------------------------------------------------- dimension_fields ---

/// One dimension of each of the seven kinds, so seven drawn measurement texts,
/// in the order the generator wrote the dimensions. Four are what ACadSharp's own
/// `UpdateBlock` formats (10, 5, 4, 8); the other three are written into the
/// block by the generator (see `DimensionFields.cs` for why), so the claim is the
/// count and the content, and the count is what breaks if a kind is missing.
fn claim_dimension_fields(recs: &[Record]) -> Result<(), String> {
    let got: Vec<&str> = texts(recs).iter().map(|t| t.text.as_str()).collect();
    ensure(got.len() == 7, || {
        format!("{} text records, expected 7: {got:?}", got.len())
    })?;
    ensure(got == ["10", "5", "4", "8", "90°", "60°", "120"], || {
        format!("measured texts are {got:?}")
    })?;
    ensure(
        !recs
            .iter()
            .any(|r| matches!(r, Record::Warning(w) if w.detail == "dimension_block_missing")),
        || "a dimension's block is missing from the output".into(),
    )
}

#[test]
fn dimension_fields_claim() {
    claim_dimension_fields(&stream("regression_dimension_fields")).unwrap();
}

#[test]
fn dimension_fields_claim_fails_with_six_kinds() {
    let mut recs = stream("regression_dimension_fields");
    let at = recs.iter().rposition(|r| r.kind() == "text").unwrap();
    recs.remove(at);
    assert!(
        claim_dimension_fields(&recs)
            .unwrap_err()
            .contains("expected 7")
    );
}

// ------------------------------------------------------------ hatch_loops ---

/// An outer loop and two islands. Schema 1 draws a hatch as one record per
/// boundary edge, not as one polygon with rings, and the generator writes each
/// loop as one closed polyline path, so the loops are exactly three closed
/// `polyline` records and the two islands lie inside the first and apart from
/// each other. Loops written as separate line edges would arrive as twelve `line`
/// records and the ring structure would be invisible, which is why none are
/// allowed here.
fn claim_hatch_loops(recs: &[Record]) -> Result<(), String> {
    ensure(count(recs, "line") == 0, || {
        "boundary edges arrived as lines".into()
    })?;
    let rings = polylines(recs);
    ensure(rings.len() == 3, || {
        format!("{} rings, expected 3", rings.len())
    })?;
    for (i, r) in rings.iter().enumerate() {
        ensure(r.closed, || format!("ring {i} is open"))?;
        ensure(r.points.len() == 4, || {
            format!("ring {i} has {} vertices", r.points.len())
        })?;
    }
    let bbox = |p: &canonical::Polyline| {
        let xs = p.points.iter().map(|q| q[0]);
        let ys = p.points.iter().map(|q| q[1]);
        (
            xs.clone().fold(f64::MAX, f64::min),
            ys.clone().fold(f64::MAX, f64::min),
            xs.fold(f64::MIN, f64::max),
            ys.fold(f64::MIN, f64::max),
        )
    };
    let (outer, a, b) = (bbox(rings[0]), bbox(rings[1]), bbox(rings[2]));
    let inside =
        |i: (f64, f64, f64, f64)| i.0 > outer.0 && i.1 > outer.1 && i.2 < outer.2 && i.3 < outer.3;
    ensure(inside(a) && inside(b), || {
        "an island is not strictly inside the outer loop".into()
    })?;
    ensure(a.2 < b.0 || b.2 < a.0 || a.3 < b.1 || b.3 < a.1, || {
        "the islands overlap".into()
    })
}

#[test]
fn hatch_loops_claim() {
    claim_hatch_loops(&stream("regression_hatch_loops")).unwrap();
}

#[test]
fn hatch_loops_claim_fails_with_one_island() {
    let mut recs = stream("regression_hatch_loops");
    let at = recs.iter().rposition(|r| r.kind() == "polyline").unwrap();
    recs.remove(at);
    assert!(claim_hatch_loops(&recs).unwrap_err().contains("expected 3"));
}

#[test]
fn hatch_loops_claim_fails_when_an_island_leaves_the_outer_loop() {
    let mut recs = stream("regression_hatch_loops");
    let at = recs.iter().rposition(|r| r.kind() == "polyline").unwrap();
    if let Record::Polyline(p) = &mut recs[at] {
        p.points.iter_mut().for_each(|q| q[0] += 100.0);
    }
    assert!(claim_hatch_loops(&recs).unwrap_err().contains("inside"));
}

// ----------------------------------------------------- unsupported_entity ---

/// A MESH between a line and a circle: both known records are there, in order,
/// and the mesh is accounted for by exactly one `unsupported_entity` warning that
/// names it, on top of the warnings every drawing in this generator carries.
fn claim_unsupported_entity(recs: &[Record], baseline_warnings: usize) -> Result<(), String> {
    let kinds: Vec<&str> = recs
        .iter()
        .filter(|r| !is_warning(r))
        .map(|r| r.kind())
        .collect();
    ensure(
        kinds == ["document", "view", "view", "line", "circle"],
        || format!("records are {kinds:?}"),
    )?;
    let named: Vec<&canonical::Warning> = recs
        .iter()
        .filter_map(|r| {
            if let Record::Warning(w) = r {
                Some(w)
            } else {
                None
            }
        })
        .filter(|w| w.category == "unsupported_entity" && w.entity_type == "Mesh")
        .collect();
    ensure(named.len() == 1, || {
        format!("{} warnings name the Mesh, expected 1", named.len())
    })?;
    let all = count(recs, "warning");
    ensure(all == baseline_warnings + 1, || {
        format!("{all} warnings, expected the baseline {baseline_warnings} plus the mesh")
    })
}

fn baseline_warnings() -> usize {
    // regression_bulge carries one entity the canonical stream supports and
    // nothing unsupported, so its warnings are what every generated drawing has.
    count(&stream("regression_bulge"), "warning")
}

#[test]
fn unsupported_entity_claim() {
    claim_unsupported_entity(
        &stream("regression_unsupported_entity"),
        baseline_warnings(),
    )
    .unwrap();
}

#[test]
fn unsupported_entity_claim_fails_when_the_entity_vanishes_silently() {
    let mut recs = stream("regression_unsupported_entity");
    let at = recs
        .iter()
        .position(|r| matches!(r, Record::Warning(w) if w.entity_type == "Mesh"))
        .unwrap();
    recs.remove(at);
    assert!(
        claim_unsupported_entity(&recs, baseline_warnings())
            .unwrap_err()
            .contains("name the Mesh")
    );
}

// ------------------------------------------------------ large_entity_count ---

/// Fifty thousand lines, line i from (i mod 250, i div 250) one unit to the right.
/// The streaming half of this class (peak memory during a native decode) needs the
/// native lane and is not in this file.
fn claim_large_entity_count(recs: &[Record]) -> Result<(), String> {
    let ls = lines(recs);
    ensure(ls.len() == 50_000, || {
        format!("{} line records, expected 50000", ls.len())
    })?;
    for (i, l) in ls.iter().enumerate() {
        let (x, y) = ((i % 250) as f64, (i / 250) as f64);
        ensure(l.a == [x, y, 0.0] && l.b == [x + 1.0, y, 0.0], || {
            format!(
                "line {i} is {:?} to {:?}, expected ({x}, {y}) to ({}, {y})",
                l.a,
                l.b,
                x + 1.0
            )
        })?;
    }
    Ok(())
}

#[test]
fn large_entity_count_claim() {
    claim_large_entity_count(&stream("regression_large_entity_count")).unwrap();
}

#[test]
fn large_entity_count_claim_fails_on_a_dropped_line() {
    let mut recs = stream("regression_large_entity_count");
    let at = recs.iter().rposition(|r| r.kind() == "line").unwrap();
    recs.remove(at);
    assert!(
        claim_large_entity_count(&recs)
            .unwrap_err()
            .contains("49999")
    );
}
