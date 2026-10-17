//! acadrust to canonical schema 1.
//!
//! The adapter behind `emit-acadrust`. acadrust does not flatten; this turns
//! its document into the records `docs/CANONICAL_SCHEMA.md` defines, with the
//! mapping, the conventions and the refusals that document spells out.
//!
//! The render half of the fork's conformance harness (`tests/viprs/mod.rs`,
//! which we wrote: nine commits, all ours) comes from there, retargeted from
//! libviprs-dep's wire 2 mapping to schema 1's: `Ocs` and the placement
//! transform are the same arbitrary-axis and insert algebra, and the schema 1
//! mapping (curves kept as curves, hatch boundaries as analytic edges, text
//! with its alignment and style, dimension blocks with the inserts inside them)
//! is written against `reference/ACadSharp.Reference/Cad/`, the oracle that
//! defines it. It is our code and carries no MPL notice; this file is under this
//! repository's MIT licence, and nothing of upstream acadrust's source is in it.
//! The fork's `Record::parse`, `Verdict` and `dump_fixture` (the `.txt`
//! comparison side) stay there until R4.3.
//!
//! acadrust's own `EntityType::apply_transform` exists and is not used: the
//! placement here was written to match the reference's basis, and
//! libviprs/acadrust#14 is a note on exactly that difference.

use std::io::Cursor;

use acadrust::document::CadDocument;
use acadrust::entities::{
    AttachmentPoint, AttributeEntity, BoundaryEdge, EntityType, HorizontalAlignment, Insert,
    TextHorizontalAlignment, TextVerticalAlignment, VerticalAlignment,
};
use acadrust::io::dwg::DwgReader;
use acadrust::notification::NotificationType;
use acadrust::objects::ObjectType;
use acadrust::types::Vector3;

use crate::canonical::{
    Arc, Circle, Document, Ellipse, Line, Point, Polygon, Polyline, Record, SidecarEntry, Spline,
    Text, View, Warning,
};
use crate::emit::Emission;

/// How deep nesting may go before it is refused. Sixteen because real drawings
/// nest a title block inside a sheet inside a border and stop, and anything past
/// that is far more likely to be a cycle than a drawing somebody made.
const MAX_DEPTH: usize = 16;

/// Ceiling on the copies one MInsert array may expand to. A corrupt row or
/// column count is a two-byte field that can ask for billions of copies, and
/// expanding it is indistinguishable from a hang.
const MAX_ARRAY_COPIES: u64 = 1024;

const TAU: f64 = std::f64::consts::TAU;

/// Read a DWG with acadrust and emit it.
pub fn extract_bytes(bytes: &[u8]) -> Result<Emission, String> {
    let doc = DwgReader::from_stream(Cursor::new(bytes.to_vec()))
        .read()
        .map_err(|e| format!("acadrust could not read this file: {e}"))?;
    Ok(extract(&doc))
}

/// Read a DXF, ASCII or binary, with acadrust and emit it. acadrust tells the
/// two encodings (and the pre-AC1012 single-byte group codes) apart on its own.
pub fn extract_dxf_bytes(bytes: &[u8]) -> Result<Emission, String> {
    let doc = acadrust::io::dxf::DxfReader::from_reader(Cursor::new(bytes.to_vec()))
        .and_then(|r| r.read())
        .map_err(|e| format!("acadrust could not read this DXF: {e}"))?;
    Ok(extract_as(&doc, "dxf"))
}

/// Emit an already-read document.
pub fn extract(doc: &CadDocument) -> Emission {
    extract_as(doc, "dwg")
}

/// [`extract`] for a document that came out of `format` (`dwg` or `dxf`).
pub fn extract_as(doc: &CadDocument, format: &str) -> Emission {
    let mut x = Extractor {
        doc,
        warnings: Vec::new(),
        at: Vec::new(),
        active: Vec::new(),
        sink: Vec::new(),
    };
    x.reader_notifications();
    let mut views: Vec<(i16, String, acadrust::types::Handle)> = doc
        .objects
        .values()
        .filter_map(|o| match o {
            ObjectType::Layout(l) => Some((l.tab_order, l.name.clone(), l.block_record)),
            _ => None,
        })
        .collect();
    views.sort_by(|a, b| {
        a.0.cmp(&b.0)
            .then_with(|| a.1.as_bytes().cmp(b.1.as_bytes()))
    });

    let mut head: Vec<Item> = vec![Item::framing(Record::Document(Document {
        format: format.to_string(),
        acad_version: doc.version.to_string(),
        maintenance_version: u64::from(doc.maintenance_version),
    }))];

    for (index, (_, name, block_handle)) in views.iter().enumerate() {
        let block_name = doc
            .block_records
            .iter()
            .find(|b| b.handle == *block_handle)
            .map(|b| b.name.clone());
        let kind = match &block_name {
            Some(n) if n.eq_ignore_ascii_case("*Model_Space") => "model",
            Some(n) if n.to_ascii_uppercase().starts_with("*PAPER_SPACE") => "layout",
            _ => "unknown",
        };
        head.push(Item::framing(Record::View(View {
            index: index as u64,
            name: name.clone(),
            kind: kind.to_string(),
        })));
        let Some(block_name) = block_name else {
            x.warn("missing_reference", "Layout", "layout_without_block_record");
            continue;
        };
        let entities: Vec<&EntityType> = doc.entities_in_block(&block_name).collect();
        let mut path = Vec::new();
        for e in entities {
            x.extract(e, index as u64, &Xf::identity(), 0, false, &mut path);
        }
    }

    x.warnings
        .sort_by(|a, b| (&a.0, &a.1, &a.2).cmp(&(&b.0, &b.1, &b.2)));
    let mut items = head;
    items.append(&mut x.sink);
    for (category, entity_type, detail, at) in x.warnings {
        items.push(Item {
            record: Record::Warning(Warning {
                category,
                entity_type,
                detail,
            }),
            path: at,
            dxf: None,
        });
    }

    let mut out = Emission::default();
    for (index, item) in items.into_iter().enumerate() {
        out.sidecar.push(SidecarEntry {
            index,
            handles: if item.path.is_empty() {
                vec!["0".to_string()]
            } else {
                item.path.iter().map(|h| format!("{h:X}")).collect()
            },
            record_type: Some(item.record.kind().to_string()),
            from_insert: None,
            dxf_name: item.dxf,
        });
        out.records.push(item.record);
    }
    out
}

/// One record, and where it came from.
struct Item {
    record: Record,
    /// Insert handles down to the entity's own, outermost first.
    path: Vec<u64>,
    dxf: Option<String>,
}

impl Item {
    fn framing(record: Record) -> Self {
        Self {
            record,
            path: Vec::new(),
            dxf: None,
        }
    }
}

// ---- transform algebra -------------------------------------------------------------

/// An affine map from an entity's own coordinates into world coordinates.
///
/// `similarity` is tracked structurally from the scale factors that went into
/// the chain, never sniffed back out of the matrix: comparing computed entries
/// for equality would classify an ordinary 30 degree rotation as non-uniform on
/// float noise alone, and every rotated block in the corpus would turn into an
/// ellipse.
#[derive(Clone, Copy)]
struct Xf {
    m: [[f64; 3]; 3],
    t: [f64; 3],
    similarity: bool,
}

type V3 = [f64; 3];

fn v3(v: &Vector3) -> V3 {
    [v.x, v.y, v.z]
}

fn cross(a: V3, b: V3) -> V3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn dot(a: V3, b: V3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn length(a: V3) -> f64 {
    dot(a, a).sqrt()
}

fn normalize(a: V3) -> V3 {
    let l = length(a);
    if l == 0.0 {
        [0.0; 3]
    } else {
        [a[0] / l, a[1] / l, a[2] / l]
    }
}

fn scale(a: V3, k: f64) -> V3 {
    [a[0] * k, a[1] * k, a[2] * k]
}

impl Xf {
    fn identity() -> Self {
        Self {
            m: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            t: [0.0; 3],
            similarity: true,
        }
    }

    fn apply(&self, p: V3) -> V3 {
        [
            self.m[0][0] * p[0] + self.m[0][1] * p[1] + self.m[0][2] * p[2] + self.t[0],
            self.m[1][0] * p[0] + self.m[1][1] * p[1] + self.m[1][2] * p[2] + self.t[1],
            self.m[2][0] * p[0] + self.m[2][1] * p[1] + self.m[2][2] * p[2] + self.t[2],
        ]
    }

    fn vector(&self, v: V3) -> V3 {
        [
            self.m[0][0] * v[0] + self.m[0][1] * v[1] + self.m[0][2] * v[2],
            self.m[1][0] * v[0] + self.m[1][1] * v[1] + self.m[1][2] * v[2],
            self.m[2][0] * v[0] + self.m[2][1] * v[1] + self.m[2][2] * v[2],
        ]
    }

    /// `outer` after `inner`.
    fn compose(outer: &Xf, inner: &Xf) -> Xf {
        let mut m = [[0.0; 3]; 3];
        for (r, row) in m.iter_mut().enumerate() {
            for (c, cell) in row.iter_mut().enumerate() {
                *cell = (0..3).map(|k| outer.m[r][k] * inner.m[k][c]).sum();
            }
        }
        let mut t = [0.0; 3];
        for (r, cell) in t.iter_mut().enumerate() {
            *cell = (0..3).map(|k| outer.m[r][k] * inner.t[k]).sum::<f64>() + outer.t[r];
        }
        Xf {
            m,
            t,
            similarity: outer.similarity && inner.similarity,
        }
    }

    fn translation(t: V3) -> Xf {
        Xf {
            t,
            ..Xf::identity()
        }
    }

    fn scaling(sx: f64, sy: f64, sz: f64) -> Xf {
        Xf {
            m: [[sx, 0.0, 0.0], [0.0, sy, 0.0], [0.0, 0.0, sz]],
            t: [0.0; 3],
            similarity: sx.abs() == sy.abs() && sy.abs() == sz.abs(),
        }
    }

    fn rotation_z(radians: f64) -> Xf {
        let (s, c) = radians.sin_cos();
        Xf {
            m: [[c, -s, 0.0], [s, c, 0.0], [0.0, 0.0, 1.0]],
            t: [0.0; 3],
            similarity: true,
        }
    }

    /// The object coordinate system a DXF normal implies, as a map from OCS
    /// into world space. DXF's arbitrary axis algorithm: the 1/64 test is from
    /// the specification and is not a tolerance to tune. The rule itself is
    /// `ocs::axes`, which `emit-acadsharp` projects through as well.
    fn arbitrary_axis(normal: V3) -> Xf {
        let (ax, ay, n) = crate::ocs::axes(normal);
        Xf {
            m: [
                [ax[0], ay[0], n[0]],
                [ax[1], ay[1], n[1]],
                [ax[2], ay[2], n[2]],
            ],
            t: [0.0; 3],
            similarity: true,
        }
    }

    fn axes(&self) -> (V3, V3) {
        (
            [self.m[0][0], self.m[1][0], self.m[2][0]],
            [self.m[0][1], self.m[1][1], self.m[2][1]],
        )
    }
}

/// The map from a block's own coordinates into the space containing the
/// insert: base point, then scale, then the array offset, then rotation about
/// the insert's own Z, then the insertion point, then the insert's OCS. Written
/// in the order DXF defines.
fn insert_transform(insert: &Insert, base: V3, column: u32, row: u32) -> Xf {
    let mut t = Xf::translation([-base[0], -base[1], -base[2]]);
    t = Xf::compose(
        &Xf::scaling(insert.x_scale(), insert.y_scale(), insert.z_scale()),
        &t,
    );
    if column != 0 || row != 0 {
        t = Xf::compose(
            &Xf::translation([
                f64::from(column) * insert.column_spacing,
                f64::from(row) * insert.row_spacing,
                0.0,
            ]),
            &t,
        );
    }
    t = Xf::compose(&Xf::rotation_z(insert.rotation), &t);
    t = Xf::compose(&Xf::translation(v3(&insert.insert_point)), &t);
    Xf::compose(&Xf::arbitrary_axis(v3(&insert.normal)), &t)
}

/// What a TABLE contributes to the block it caches. A TABLE is an INSERT over
/// there (`TableEntity` derives from `Insert`), so its drawn cell borders and
/// cell text come out of the anonymous block with a placement an INSERT gets
/// for free and acadrust's separate entity has to be given. Rotation is the
/// angle the horizontal direction makes in the plane the normal names; there is
/// no scale.
fn table_transform(t: &acadrust::entities::Table) -> Xf {
    let o = Xf::arbitrary_axis(v3(&t.normal));
    let (ax, ay) = o.axes();
    let h = v3(&t.horizontal_direction);
    let (dx, dy) = (dot(h, ax), dot(h, ay));
    let len = (dx * dx + dy * dy).sqrt();
    let (c, s) = if len < 1e-12 {
        (1.0, 0.0)
    } else {
        (dx / len, dy / len)
    };
    let local = Xf {
        m: [[c, -s, 0.0], [s, c, 0.0], [0.0, 0.0, 1.0]],
        t: v3(&t.insertion_point),
        similarity: true,
    };
    Xf::compose(&o, &local)
}

// ---- curves --------------------------------------------------------------------------

/// A curve after mapping, in the OCS of its own normal.
struct Mapped {
    circular: bool,
    center: V3,
    radius: f64,
    major: V3,
    ratio: f64,
    angle_offset: f64,
    normal: V3,
    degenerate: bool,
}

/// Carries an analytic curve through a transform without flattening it: every
/// affine map takes conjugate semi-diameters to conjugate semi-diameters, so
/// transforming `u` and `v` and rebuilding the curve is exact. On a similarity
/// the result is the same kind of curve with an angle offset; otherwise a two by
/// two singular value decomposition recovers the principal axes of the image
/// ellipse.
fn map_curve(center: V3, u: V3, v: V3, to_world: &Xf) -> Mapped {
    let world_center = to_world.apply(center);
    let wu = to_world.vector(u);
    let wv = to_world.vector(v);
    let normal = normalize(cross(wu, wv));
    if normal == [0.0; 3] {
        return Mapped {
            circular: false,
            center: world_center,
            radius: 0.0,
            major: [0.0; 3],
            ratio: 1.0,
            angle_offset: 0.0,
            normal: [0.0, 0.0, 1.0],
            degenerate: true,
        };
    }
    let (ax, ay) = Xf::arbitrary_axis(normal).axes();
    let c = [
        dot(world_center, ax),
        dot(world_center, ay),
        dot(world_center, normal),
    ];
    let (ux, uy, vx, vy) = (dot(wu, ax), dot(wu, ay), dot(wv, ax), dot(wv, ay));

    if to_world.similarity {
        let radius = (ux * ux + uy * uy).sqrt();
        let minor = (vx * vx + vy * vy).sqrt();
        return Mapped {
            circular: radius == minor,
            center: c,
            radius,
            major: [ux, uy, 0.0],
            ratio: if radius == 0.0 { 1.0 } else { minor / radius },
            angle_offset: uy.atan2(ux),
            normal,
            degenerate: radius == 0.0,
        };
    }

    // [u v] = R(phi) * diag(s1, s2) * R(theta)^T, the closed-form two by two
    // SVD. R(phi)'s columns are the principal directions, the singular values
    // are the semi-axis lengths, and subtracting theta from the source
    // parameter re-parametrises the curve onto them.
    let e = (ux + vy) / 2.0;
    let f = (ux - vy) / 2.0;
    let g = (uy + vx) / 2.0;
    let h = (uy - vx) / 2.0;
    let q = (e * e + h * h).sqrt();
    let r = (f * f + g * g).sqrt();
    let sigma1 = q + r;
    let sigma2 = q - r;
    let a1 = h.atan2(e);
    let a2 = g.atan2(f);
    let phi = (a1 + a2) / 2.0;
    let theta = (a1 - a2) / 2.0;
    if sigma1 == 0.0 {
        return Mapped {
            circular: false,
            center: c,
            radius: 0.0,
            major: [0.0; 3],
            ratio: 1.0,
            angle_offset: 0.0,
            normal,
            degenerate: true,
        };
    }
    let major = [sigma1 * phi.cos(), sigma1 * phi.sin(), 0.0];
    let ratio = sigma2.abs() / sigma1;
    if ratio == 0.0 {
        return Mapped {
            circular: false,
            center: c,
            radius: 0.0,
            major,
            ratio: 0.0,
            angle_offset: 0.0,
            normal,
            degenerate: true,
        };
    }
    Mapped {
        circular: false,
        center: c,
        radius: sigma1,
        major,
        ratio,
        angle_offset: -theta,
        normal,
        degenerate: false,
    }
}

use crate::ocs::{normalize_angle, sweep};

// ---- the extractor ------------------------------------------------------------------

struct Extractor<'a> {
    doc: &'a CadDocument,
    /// Category, entity type, detail, and the handle path of the entity being
    /// read when it was raised (empty for a finding about the document).
    warnings: Vec<(String, String, String, Vec<u64>)>,
    /// The handle path of the entity `extract` is inside, for `warn`.
    at: Vec<u64>,
    /// Block names on the path being expanded. Keyed on the block itself and not
    /// on a handle: a document built in memory has not been assigned handles
    /// yet, so every block would answer zero and the first nested block would
    /// read as a cycle.
    active: Vec<String>,
    sink: Vec<Item>,
}

impl Extractor<'_> {
    fn warn(&mut self, category: &str, entity_type: &str, detail: &str) {
        self.warnings.push((
            category.to_string(),
            entity_type.to_string(),
            detail.to_string(),
            self.at.clone(),
        ));
    }

    /// What the reader itself reported, classified by severity. acadrust's
    /// messages are not ACadSharp's, so the text is not parsed: the detail is a
    /// stable discriminator and never carries an exception message or a handle.
    fn reader_notifications(&mut self) {
        for n in self.doc.notifications.iter() {
            let font = {
                let m = n.message.to_ascii_lowercase();
                m.contains("font") || m.contains(".shx") || m.contains(".ttf")
            };
            let (category, detail) = if font {
                ("missing_font", "reader_font_notification")
            } else {
                match n.notification_type {
                    NotificationType::NotImplemented => {
                        ("unsupported_entity", "reader_not_implemented")
                    }
                    NotificationType::NotSupported => {
                        ("unsupported_entity", "reader_not_supported")
                    }
                    NotificationType::Error => ("invalid_entity", "reader_error"),
                    NotificationType::Warning => ("reader_notification", "reader_warning"),
                }
            };
            self.warn(category, "", detail);
        }
    }

    fn push(&mut self, record: Record, path: &[u64], handle: u64, dxf: &str) {
        let mut full = path.to_vec();
        full.push(handle);
        self.sink.push(Item {
            record,
            path: full,
            dxf: Some(dxf.to_string()),
        });
    }

    fn extract(
        &mut self,
        entity: &EntityType,
        view: u64,
        xf: &Xf,
        depth: usize,
        inside_block: bool,
        path: &mut Vec<u64>,
    ) {
        // A warning names the entity it was raised inside, so a diff and the
        // census check can say which one. Restored on the way out, because the
        // extraction recurses.
        let mut here = path.clone();
        here.push(entity.common().handle.value());
        let saved = std::mem::replace(&mut self.at, here);
        self.extract_entity(entity, view, xf, depth, inside_block, path);
        self.at = saved;
    }

    fn extract_entity(
        &mut self,
        entity: &EntityType,
        view: u64,
        xf: &Xf,
        depth: usize,
        inside_block: bool,
        path: &mut Vec<u64>,
    ) {
        // An entity the drawing marks invisible is not drawn, and a reference
        // that emitted it would disagree with every viewer. Absence is the
        // correct representation, which is why this one is not a warning.
        if entity.common().invisible {
            return;
        }
        let handle = entity.common().handle.value();
        let dxf = entity.as_entity().entity_type();
        match entity {
            // Structural markers: a SEQEND closes a vertex or attribute run and
            // a BLOCK/ENDBLK bracket a definition. None is drawable, so they
            // are the one deliberate silent skip.
            EntityType::Seqend(_) | EntityType::Block(_) | EntityType::BlockEnd(_) => {}
            EntityType::Line(l) => {
                let r = Record::Line(Line {
                    view,
                    a: xf.apply(v3(&l.start)),
                    b: xf.apply(v3(&l.end)),
                });
                self.push(r, path, handle, dxf);
            }
            EntityType::Point(p) => {
                let r = Record::Point(Point {
                    view,
                    at: xf.apply(v3(&p.location)),
                });
                self.push(r, path, handle, dxf);
            }
            EntityType::Arc(a) => {
                let to_world = Xf::compose(xf, &Xf::arbitrary_axis(v3(&a.normal)));
                let r = a.radius;
                let m = map_curve(v3(&a.center), [r, 0.0, 0.0], [0.0, r, 0.0], &to_world);
                if m.degenerate {
                    self.warn("malformed_geometry", "Arc", "degenerate_after_transform");
                    return;
                }
                let (s, e) = sweep(a.start_angle, a.end_angle, m.angle_offset);
                let rec = if m.circular {
                    Record::Arc(Arc {
                        view,
                        center: m.center,
                        radius: m.radius,
                        start: s,
                        end: e,
                        normal: m.normal,
                    })
                } else {
                    Record::Ellipse(Ellipse {
                        view,
                        center: m.center,
                        major: m.major,
                        ratio: m.ratio,
                        start: s,
                        end: e,
                        normal: m.normal,
                    })
                };
                self.push(rec, path, handle, dxf);
            }
            EntityType::Circle(c) => {
                let to_world = Xf::compose(xf, &Xf::arbitrary_axis(v3(&c.normal)));
                let r = c.radius;
                let m = map_curve(v3(&c.center), [r, 0.0, 0.0], [0.0, r, 0.0], &to_world);
                if m.degenerate {
                    self.warn("malformed_geometry", "Circle", "degenerate_after_transform");
                    return;
                }
                // A non-uniformly scaled circle is an ellipse. Saying so is the
                // honest record; keeping a "circle" with one of the two radii
                // would be a lie the differential would then have to agree with.
                let rec = if m.circular {
                    Record::Circle(Circle {
                        view,
                        center: m.center,
                        radius: m.radius,
                        normal: m.normal,
                    })
                } else {
                    Record::Ellipse(Ellipse {
                        view,
                        center: m.center,
                        major: m.major,
                        ratio: m.ratio,
                        start: 0.0,
                        end: TAU,
                        normal: m.normal,
                    })
                };
                self.push(rec, path, handle, dxf);
            }
            EntityType::Ellipse(el) => {
                // An ELLIPSE stores its centre and major axis in world space
                // already, so the only map that applies is the block chain.
                let major = v3(&el.major_axis);
                let normal = v3(&el.normal);
                let minor = scale(
                    normalize(cross(normal, major)),
                    length(major) * el.minor_axis_ratio,
                );
                let m = map_curve(v3(&el.center), major, minor, xf);
                if m.degenerate {
                    self.warn(
                        "malformed_geometry",
                        "Ellipse",
                        "degenerate_after_transform",
                    );
                    return;
                }
                let (mut s, mut e) = sweep(el.start_parameter, el.end_parameter, m.angle_offset);
                // A full ellipse is stored as 0 to 2*pi and `sweep` would fold
                // that to a zero-length arc. Nothing else produces an exactly
                // empty sweep.
                if el.start_parameter == el.end_parameter
                    || (el.end_parameter - el.start_parameter).abs() >= TAU
                {
                    s = normalize_angle(el.start_parameter + m.angle_offset);
                    e = s + TAU;
                }
                // On a similarity the ratio is invariant: use the number the
                // drawing stores rather than one from two square roots.
                let ratio = if xf.similarity {
                    el.minor_axis_ratio
                } else {
                    m.ratio
                };
                let rec = Record::Ellipse(Ellipse {
                    view,
                    center: m.center,
                    major: m.major,
                    ratio,
                    start: s,
                    end: e,
                    normal: m.normal,
                });
                self.push(rec, path, handle, dxf);
            }
            EntityType::Spline(s) => self.spline(s, view, xf, path, handle, dxf),
            // A helix is a spline in ACadSharp, which is what the oracle maps.
            EntityType::Helix(h) => self.spline(&h.spline, view, xf, path, handle, dxf),
            EntityType::LwPolyline(pl) => {
                let pts: Vec<V3> = pl
                    .vertices
                    .iter()
                    .map(|v| [v.location.x, v.location.y, pl.elevation])
                    .collect();
                let bulges: Vec<f64> = pl.vertices.iter().map(|v| v.bulge).collect();
                self.polyline(
                    view,
                    xf,
                    v3(&pl.normal),
                    pts,
                    bulges,
                    pl.is_closed,
                    path,
                    handle,
                    dxf,
                );
            }
            EntityType::Polyline2D(p2) => {
                let pts: Vec<V3> = p2.vertices.iter().map(|v| v3(&v.location)).collect();
                let bulges: Vec<f64> = p2.vertices.iter().map(|v| v.bulge).collect();
                self.polyline(
                    view,
                    xf,
                    v3(&p2.normal),
                    pts,
                    bulges,
                    p2.flags.is_closed(),
                    path,
                    handle,
                    dxf,
                );
            }
            EntityType::Polyline3D(p3) => {
                // A 3D polyline's vertices are already world space and it has no
                // bulges, so it gets the +Z normal and a row of zeroes rather
                // than a plane it does not have.
                let pts: Vec<V3> = p3.vertices.iter().map(|v| v3(&v.position)).collect();
                let zeros = vec![0.0; pts.len()];
                self.polyline(
                    view,
                    xf,
                    [0.0, 0.0, 1.0],
                    pts,
                    zeros,
                    p3.flags.closed,
                    path,
                    handle,
                    dxf,
                );
            }
            EntityType::Polyline(p3) => {
                let pts: Vec<V3> = p3.vertices.iter().map(|v| v3(&v.location)).collect();
                let zeros = vec![0.0; pts.len()];
                self.polyline(
                    view,
                    xf,
                    [0.0, 0.0, 1.0],
                    pts,
                    zeros,
                    p3.flags.is_closed(),
                    path,
                    handle,
                    dxf,
                );
            }
            EntityType::Solid(s) => {
                // DXF stores a SOLID's corners 1, 2, 4, 3. Drawn in stored order
                // that is a bow tie, so they are reordered, and the record says
                // its points are in perimeter order.
                let to_world = Xf::compose(xf, &Xf::arbitrary_axis(v3(&s.normal)));
                let corners = [
                    v3(&s.first_corner),
                    v3(&s.second_corner),
                    v3(&s.fourth_corner),
                    v3(&s.third_corner),
                ];
                self.polygon(view, &to_world, &corners, path, handle, dxf);
            }
            EntityType::Face3D(f) => {
                let corners = [
                    v3(&f.first_corner),
                    v3(&f.second_corner),
                    v3(&f.third_corner),
                    v3(&f.fourth_corner),
                ];
                self.polygon(view, xf, &corners, path, handle, dxf);
            }
            EntityType::AttributeDefinition(a) => {
                // An ATTDEF inside a block definition is the template for an
                // attribute, not something a viewer draws when the block is
                // inserted: the insert's own ATTRIBs are drawn instead. At the
                // top of a view it is ordinary text.
                if !inside_block {
                    let use_alignment = a.horizontal_alignment != HorizontalAlignment::Left
                        || a.vertical_alignment != VerticalAlignment::Baseline;
                    let anchor = if use_alignment {
                        a.alignment_point
                    } else {
                        a.insertion_point
                    };
                    let t = TextIn {
                        normal: v3(&a.normal),
                        anchor: v3(&anchor),
                        height: a.height,
                        rotation: a.rotation,
                        width_factor: a.width_factor,
                        horizontal: horizontal_name(a.horizontal_alignment),
                        vertical: vertical_name(a.vertical_alignment),
                        style: &a.text_style,
                        value: &a.default_value,
                    };
                    self.text(view, xf, &t, path, handle, dxf);
                }
            }
            EntityType::AttributeEntity(a) => self.attribute(a, view, xf, path, handle, dxf),
            EntityType::Text(t) => {
                let use_alignment = t.horizontal_alignment != TextHorizontalAlignment::Left
                    || t.vertical_alignment != TextVerticalAlignment::Baseline;
                let anchor = if use_alignment {
                    t.alignment_point.unwrap_or(t.insertion_point)
                } else {
                    t.insertion_point
                };
                let h = match t.horizontal_alignment {
                    TextHorizontalAlignment::Left => "left",
                    TextHorizontalAlignment::Center => "center",
                    TextHorizontalAlignment::Right => "right",
                    TextHorizontalAlignment::Aligned => "aligned",
                    TextHorizontalAlignment::Middle => "middle",
                    TextHorizontalAlignment::Fit => "fit",
                };
                let v = match t.vertical_alignment {
                    TextVerticalAlignment::Baseline => "baseline",
                    TextVerticalAlignment::Bottom => "bottom",
                    TextVerticalAlignment::Middle => "middle",
                    TextVerticalAlignment::Top => "top",
                };
                let ti = TextIn {
                    normal: v3(&t.normal),
                    anchor: v3(&anchor),
                    height: t.height,
                    rotation: t.rotation,
                    width_factor: t.width_factor,
                    horizontal: h,
                    vertical: v,
                    style: &t.style,
                    value: &t.value,
                };
                self.text(view, xf, &ti, path, handle, dxf);
            }
            EntityType::MText(m) => {
                let (h, v) = match m.attachment_point {
                    AttachmentPoint::TopLeft => ("left", "top"),
                    AttachmentPoint::TopCenter => ("center", "top"),
                    AttachmentPoint::TopRight => ("right", "top"),
                    AttachmentPoint::MiddleLeft => ("left", "middle"),
                    AttachmentPoint::MiddleCenter => ("center", "middle"),
                    AttachmentPoint::MiddleRight => ("right", "middle"),
                    AttachmentPoint::BottomLeft => ("left", "bottom"),
                    AttachmentPoint::BottomCenter => ("center", "bottom"),
                    AttachmentPoint::BottomRight => ("right", "bottom"),
                };
                // MTEXT has no width factor of its own. One keeps the field set
                // of a text record the same whichever entity produced it.
                let ti = TextIn {
                    normal: v3(&m.normal),
                    anchor: v3(&m.insertion_point),
                    height: m.height,
                    rotation: m.rotation,
                    width_factor: 1.0,
                    horizontal: h,
                    vertical: v,
                    style: &m.style,
                    value: &m.value,
                };
                self.text(view, xf, &ti, path, handle, dxf);
            }
            EntityType::Hatch(h) => self.hatch(h, view, xf, path, handle, dxf),
            EntityType::Insert(i) => self.insert(i, view, xf, depth, path),
            EntityType::Dimension(d) => {
                // A dimension's drawn form is the anonymous block the writer
                // generated for it: the extension lines, the arrowheads and the
                // measurement text. Reconstructing it from the definition points
                // would be a second dimension engine.
                let name = d.base().block_name.clone();
                if self.doc.block_records.get(&name).is_none() {
                    self.warn("missing_reference", dxf, "dimension_block_missing");
                    return;
                }
                if !self.enter(&name, depth) {
                    return;
                }
                path.push(handle);
                let members: Vec<&EntityType> = self.doc.entities_in_block(&name).collect();
                for m in members {
                    self.extract(m, view, xf, depth + 1, true, path);
                }
                path.pop();
                self.leave(&name);
            }
            EntityType::Table(t) => {
                // A TABLE caches what it draws in an anonymous block, expanded
                // because `TableEntity` derives from `Insert` over there. The DWG
                // names the cache block by handle and the entity's own
                // `block_name` comes through empty, so the name may have to come
                // out of the block table.
                let name = if t.block_name.is_empty() {
                    t.block_record_handle.and_then(|h| {
                        self.doc
                            .block_records
                            .iter()
                            .find(|b| b.handle == h)
                            .map(|b| b.name.clone())
                    })
                } else {
                    Some(t.block_name.clone())
                };
                // No cache block is a table nothing has drawn yet.
                if let Some(name) = name {
                    if !self.enter(&name, depth) {
                        return;
                    }
                    let inner = Xf::compose(xf, &table_transform(t));
                    path.push(handle);
                    let members: Vec<&EntityType> = self.doc.entities_in_block(&name).collect();
                    for m in members {
                        self.extract(m, view, &inner, depth + 1, true, path);
                    }
                    path.pop();
                    self.leave(&name);
                }
            }
            EntityType::Unknown(u) => {
                self.warn("unsupported_entity", &u.dxf_name, "unknown_entity");
            }
            other => {
                let name = other.as_entity().entity_type();
                self.warn("unsupported_entity", name, "no_canonical_mapping");
            }
        }
    }

    /// Whether a block may be entered, recording the reason when it may not.
    fn enter(&mut self, name: &str, depth: usize) -> bool {
        if depth >= MAX_DEPTH {
            self.warn(
                "malformed_geometry",
                "Insert",
                &format!("recursion_limit_{MAX_DEPTH}"),
            );
            return false;
        }
        if self.active.iter().any(|a| a == name) {
            // Self-reference and longer cycles look the same from here: the
            // block is already on the path being expanded.
            self.warn("malformed_geometry", "Insert", "cyclic_block_reference");
            return false;
        }
        self.active.push(name.to_string());
        true
    }

    fn leave(&mut self, name: &str) {
        if let Some(at) = self.active.iter().rposition(|a| a == name) {
            self.active.remove(at);
        }
    }

    fn insert(&mut self, insert: &Insert, view: u64, xf: &Xf, depth: usize, path: &mut Vec<u64>) {
        let handle = insert.common.handle.value();
        let Some(block) = self.doc.block_records.get(&insert.block_name) else {
            self.warn("missing_reference", "Insert", "block_record_missing");
            return;
        };
        let base = v3(&block.base_point);
        let name = insert.block_name.clone();
        if self.enter(&name, depth) {
            let columns = u64::from(insert.column_count.max(1));
            let rows = u64::from(insert.row_count.max(1));
            let cells: Vec<(u32, u32)> = if columns * rows > MAX_ARRAY_COPIES {
                self.warn(
                    "malformed_geometry",
                    "Insert",
                    &format!("array_too_large_{MAX_ARRAY_COPIES}"),
                );
                vec![(0, 0)]
            } else {
                (0..rows)
                    .flat_map(|r| (0..columns).map(move |c| (c as u32, r as u32)))
                    .collect()
            };
            path.push(handle);
            for (column, row) in cells {
                let child = Xf::compose(xf, &insert_transform(insert, base, column, row));
                let members: Vec<&EntityType> = self.doc.entities_in_block(&name).collect();
                for m in members {
                    self.extract(m, view, &child, depth + 1, true, path);
                }
            }
            path.pop();
            self.leave(&name);
        }
        // The insert's own ATTRIBs are positioned in the space that contains the
        // insert, not in the block's, so they take the parent transform.
        for a in &insert.attributes {
            path.push(handle);
            self.extract_attribute(a, view, xf, path);
            path.pop();
        }
    }

    fn extract_attribute(&mut self, a: &AttributeEntity, view: u64, xf: &Xf, path: &[u64]) {
        if a.common.invisible {
            return;
        }
        let handle = a.common.handle.value();
        self.attribute(a, view, xf, path, handle, "ATTRIB");
    }

    fn attribute(
        &mut self,
        a: &AttributeEntity,
        view: u64,
        xf: &Xf,
        path: &[u64],
        handle: u64,
        dxf: &str,
    ) {
        let use_alignment = a.horizontal_alignment != HorizontalAlignment::Left
            || a.vertical_alignment != VerticalAlignment::Baseline;
        let anchor = if use_alignment {
            a.alignment_point
        } else {
            a.insertion_point
        };
        let t = TextIn {
            normal: v3(&a.normal),
            anchor: v3(&anchor),
            height: a.height,
            rotation: a.rotation,
            width_factor: a.width_factor,
            horizontal: horizontal_name(a.horizontal_alignment),
            vertical: vertical_name(a.vertical_alignment),
            style: &a.text_style,
            value: &a.value,
        };
        self.text(view, xf, &t, path, handle, dxf);
    }

    fn spline(
        &mut self,
        s: &acadrust::entities::Spline,
        view: u64,
        xf: &Xf,
        path: &[u64],
        handle: u64,
        dxf: &str,
    ) {
        // Affine maps commute with the rational B-spline basis, so control and
        // fit points move and degree, knots and weights do not.
        let control: Vec<V3> = s.control_points.iter().map(|p| xf.apply(v3(p))).collect();
        let fit: Vec<V3> = s.fit_points.iter().map(|p| xf.apply(v3(p))).collect();
        if control.is_empty() && fit.is_empty() {
            self.warn("invalid_entity", "Spline", "no_control_or_fit_points");
            return;
        }
        let rec = Record::Spline(Spline {
            view,
            degree: s.degree.max(0) as u64,
            closed: s.flags.closed,
            periodic: s.flags.periodic,
            control_points: control,
            knots: s.knots.clone(),
            weights: s.weights.clone(),
            fit_points: fit,
        });
        self.push(rec, path, handle, dxf);
    }

    #[allow(clippy::too_many_arguments)]
    fn polyline(
        &mut self,
        view: u64,
        xf: &Xf,
        normal: V3,
        local_points: Vec<V3>,
        mut bulges: Vec<f64>,
        closed: bool,
        path: &[u64],
        handle: u64,
        dxf: &str,
    ) {
        let to_world = Xf::compose(xf, &Xf::arbitrary_axis(normal));
        // The plane the bulges live in, after the map. Taking the new normal
        // from the images of the local axes rather than from the old normal
        // keeps the sense of rotation consistent, so a mirrored block does not
        // need its bulge signs flipped by hand.
        let u = to_world.vector([1.0, 0.0, 0.0]);
        let v = to_world.vector([0.0, 1.0, 0.0]);
        let mut world_normal = normalize(cross(u, v));
        if world_normal == [0.0; 3] {
            world_normal = [0.0, 0.0, 1.0];
        }
        let (ax, ay) = Xf::arbitrary_axis(world_normal).axes();
        if local_points.is_empty() {
            self.warn("invalid_entity", "Polyline", "no_vertices");
            return;
        }
        let points: Vec<V3> = local_points
            .iter()
            .map(|p| {
                let w = to_world.apply(*p);
                [dot(w, ax), dot(w, ay), dot(w, world_normal)]
            })
            .collect();
        bulges.resize(points.len(), 0.0);
        if !xf.similarity && bulges.iter().any(|b| *b != 0.0) {
            // A bulge is a circular arc by definition. Under a non-uniform scale
            // its image is elliptical and schema 1 has no way to say so inside a
            // polyline, so the bulge is carried through unchanged and the
            // approximation is declared rather than hidden.
            self.warn("malformed_geometry", "Polyline", "non_uniform_scaled_bulge");
        }
        let rec = Record::Polyline(Polyline {
            view,
            closed,
            normal: world_normal,
            points,
            bulges,
        });
        self.push(rec, path, handle, dxf);
    }

    fn polygon(
        &mut self,
        view: u64,
        to_world: &Xf,
        corners: &[V3; 4],
        path: &[u64],
        handle: u64,
        dxf: &str,
    ) {
        let mut points: Vec<V3> = Vec::with_capacity(4);
        for c in corners {
            let w = to_world.apply(*c);
            // A degenerate quad is how DXF spells a triangle: the last two
            // corners coincide. Emitting the duplicate would give the polygon a
            // zero-length edge.
            if points.last() == Some(&w) {
                continue;
            }
            points.push(w);
        }
        if points.len() >= 3 {
            self.push(Record::Polygon(Polygon { view, points }), path, handle, dxf);
        }
    }

    fn text(&mut self, view: u64, xf: &Xf, t: &TextIn<'_>, path: &[u64], handle: u64, dxf: &str) {
        let to_world = Xf::compose(xf, &Xf::arbitrary_axis(t.normal));
        // The baseline direction and the up direction, carried through the map
        // the way a curve's conjugate diameters are. Their images give the
        // rotation, the height and the width factor all at once.
        let (s, c) = t.rotation.sin_cos();
        let baseline = to_world.vector([c, s, 0.0]);
        let up = to_world.vector([-s, c, 0.0]);
        let world_normal = normalize(cross(baseline, up));
        if world_normal == [0.0; 3] {
            self.warn("malformed_geometry", "Text", "degenerate_after_transform");
            return;
        }
        let (ax, ay) = Xf::arbitrary_axis(world_normal).axes();
        let world = to_world.apply(t.anchor);
        let position = [dot(world, ax), dot(world, ay), dot(world, world_normal)];
        let baseline_len = length(baseline);
        let up_len = length(up);
        let rotation = normalize_angle(dot(baseline, ay).atan2(dot(baseline, ax)));
        let mut height = t.height;
        let mut width_factor = t.width_factor;
        if !xf.similarity {
            height = t.height * up_len;
            width_factor = if up_len == 0.0 {
                t.width_factor
            } else {
                t.width_factor * baseline_len / up_len
            };
        } else if baseline_len != 1.0 {
            height = t.height * baseline_len;
        }
        let rec = Record::Text(Text {
            view,
            at: position,
            height,
            rotation,
            width_factor,
            halign: t.horizontal.to_string(),
            valign: t.vertical.to_string(),
            style: t.style.to_string(),
            text: t.value.replace("\r\n", "\n").replace('\r', "\n"),
        });
        self.push(rec, path, handle, dxf);
    }

    /// A hatch's boundary as one analytic record per edge. Schema 1 draws the
    /// boundary and not the fill or the pattern lines; that is a stated limit of
    /// the reference and not a decoding failure, so it is not a warning.
    fn hatch(
        &mut self,
        h: &acadrust::entities::Hatch,
        view: u64,
        xf: &Xf,
        path: &[u64],
        handle: u64,
        dxf: &str,
    ) {
        let to_world = Xf::compose(xf, &Xf::arbitrary_axis(v3(&h.normal)));
        let z = h.elevation;
        if h.paths.is_empty() {
            self.warn("invalid_entity", "Hatch", "no_boundary_paths");
            return;
        }
        for bp in &h.paths {
            for edge in &bp.edges {
                self.hatch_edge(edge, view, &to_world, z, path, handle, dxf);
            }
            if bp.edges.is_empty() {
                self.warn("invalid_entity", "Hatch", "boundary_path_without_edges");
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn hatch_edge(
        &mut self,
        edge: &BoundaryEdge,
        view: u64,
        to_world: &Xf,
        z: f64,
        path: &[u64],
        handle: u64,
        dxf: &str,
    ) {
        match edge {
            BoundaryEdge::Line(l) => {
                let rec = Record::Line(Line {
                    view,
                    a: to_world.apply([l.start.x, l.start.y, z]),
                    b: to_world.apply([l.end.x, l.end.y, z]),
                });
                self.push(rec, path, handle, dxf);
            }
            BoundaryEdge::CircularArc(a) => {
                let r = a.radius;
                let m = map_curve(
                    [a.center.x, a.center.y, z],
                    [r, 0.0, 0.0],
                    [0.0, r, 0.0],
                    to_world,
                );
                if m.degenerate {
                    self.warn(
                        "malformed_geometry",
                        "HatchArc",
                        "degenerate_after_transform",
                    );
                    return;
                }
                // A clockwise boundary arc is stored with its angles the other
                // way round; the canonical form always sweeps counter-clockwise.
                let (sa, ea) = if a.counter_clockwise {
                    (a.start_angle, a.end_angle)
                } else {
                    (a.end_angle, a.start_angle)
                };
                let (s, e) = sweep(sa, ea, m.angle_offset);
                let rec = if m.circular {
                    Record::Arc(Arc {
                        view,
                        center: m.center,
                        radius: m.radius,
                        start: s,
                        end: e,
                        normal: m.normal,
                    })
                } else {
                    Record::Ellipse(Ellipse {
                        view,
                        center: m.center,
                        major: m.major,
                        ratio: m.ratio,
                        start: s,
                        end: e,
                        normal: m.normal,
                    })
                };
                self.push(rec, path, handle, dxf);
            }
            BoundaryEdge::EllipticArc(el) => {
                let major = [el.major_axis_endpoint.x, el.major_axis_endpoint.y, 0.0];
                let minor = scale(
                    normalize(cross([0.0, 0.0, 1.0], major)),
                    length(major) * el.minor_axis_ratio,
                );
                let m = map_curve([el.center.x, el.center.y, z], major, minor, to_world);
                if m.degenerate {
                    self.warn(
                        "malformed_geometry",
                        "HatchEllipse",
                        "degenerate_after_transform",
                    );
                    return;
                }
                let (sa, ea) = if el.counter_clockwise {
                    (el.start_angle, el.end_angle)
                } else {
                    (el.end_angle, el.start_angle)
                };
                let (s, e) = sweep(sa, ea, m.angle_offset);
                let rec = Record::Ellipse(Ellipse {
                    view,
                    center: m.center,
                    major: m.major,
                    ratio: if to_world.similarity {
                        el.minor_axis_ratio
                    } else {
                        m.ratio
                    },
                    start: s,
                    end: e,
                    normal: m.normal,
                });
                self.push(rec, path, handle, dxf);
            }
            BoundaryEdge::Polyline(p) => {
                // The vertices are (x, y, bulge) in the OCS plane at the hatch's
                // elevation.
                let pts: Vec<V3> = p.vertices.iter().map(|v| [v.x, v.y, z]).collect();
                let bulges: Vec<f64> = p.vertices.iter().map(|v| v.z).collect();
                self.polyline(
                    view,
                    to_world,
                    [0.0, 0.0, 1.0],
                    pts,
                    bulges,
                    p.is_closed,
                    path,
                    handle,
                    dxf,
                );
            }
            BoundaryEdge::Spline(s) => {
                let control: Vec<V3> = s
                    .control_points
                    .iter()
                    .map(|p| to_world.apply([p.x, p.y, z]))
                    .collect();
                if control.is_empty() {
                    self.warn("invalid_entity", "HatchSpline", "no_control_points");
                    return;
                }
                let rec = Record::Spline(Spline {
                    view,
                    degree: s.degree.max(0) as u64,
                    closed: false,
                    periodic: s.periodic,
                    control_points: control,
                    knots: s.knots.clone(),
                    // A rational boundary spline stores each weight in its
                    // control point's Z (acadrust's own writer says so).
                    weights: if s.rational {
                        s.control_points.iter().map(|p| p.z).collect()
                    } else {
                        Vec::new()
                    },
                    fit_points: s
                        .fit_points
                        .iter()
                        .map(|p| to_world.apply([p.x, p.y, z]))
                        .collect(),
                });
                self.push(rec, path, handle, dxf);
            }
        }
    }
}

/// What a text record is built from, whichever entity it came from.
struct TextIn<'a> {
    normal: V3,
    anchor: V3,
    height: f64,
    rotation: f64,
    width_factor: f64,
    horizontal: &'a str,
    vertical: &'a str,
    style: &'a str,
    value: &'a str,
}

fn horizontal_name(a: HorizontalAlignment) -> &'static str {
    match a {
        HorizontalAlignment::Left => "left",
        HorizontalAlignment::Center => "center",
        HorizontalAlignment::Right => "right",
        HorizontalAlignment::Aligned => "aligned",
        HorizontalAlignment::Middle => "middle",
        HorizontalAlignment::Fit => "fit",
    }
}

fn vertical_name(a: VerticalAlignment) -> &'static str {
    match a {
        VerticalAlignment::Baseline => "baseline",
        VerticalAlignment::Bottom => "bottom",
        VerticalAlignment::Middle => "middle",
        VerticalAlignment::Top => "top",
    }
}
