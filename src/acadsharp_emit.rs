//! acadsharp-rs to canonical schema 1.
//!
//! The adapter behind `emit-acadsharp`. It walks the safe API
//! (`Decoder -> Document::open_bytes -> views() -> decode(i) -> PrimitiveStream`)
//! and turns wire 2's items into the records `docs/CANONICAL_SCHEMA.md` defines.
//!
//! Wire 2 and schema 1 are two flatteners over the same reader, not two spellings
//! of one. This file is the wire 2 adapter and nothing else: it converts what the
//! wire carries (a world-space polyline with a normal beside it, a text with a
//! position and a height) into the schema's frames and spellings, and it does not
//! invent what the wire lacks. A `text` record's `halign`, `valign`, `style` and
//! `width_factor` are not on the wire, so they are written as the schema's
//! defaults and `docs/EMITTERS.md` lists them as excluded for this engine rather
//! than pretending they were read. The same goes for every other gap in that
//! document: nothing here is a silent absence.
//!
//! The mapping is split from the decoding. [`map_walk`] is a pure function from
//! what a walk saw to an [`Emission`], so every row of the mapping is testable on
//! hand-built items with no native library in sight; [`extract_bytes`] is the
//! only function that needs one, and it answers [`EmitError::Unlinked`] when
//! there is none.

use acadsharp_rs::{
    Decoder, Document, DocumentBegin, Error, Item, Limits, Primitive, View, ViewKind, Warning,
    WarningCode,
};

use crate::canonical::{
    Arc, Circle, Document as DocumentRecord, Ellipse, Line, Polygon, Polyline, Record,
    SidecarEntry, Spline, Text, View as ViewRecord, Warning as WarningRecord,
};
use crate::emit::Emission;
use crate::ocs::{normalize_angle, point_to_ocs, sweep};

/// Why a fixture could not be emitted.
#[derive(Debug)]
pub enum EmitError {
    /// No native library is linked, so nothing can be decoded. A failure, never
    /// an empty success: a run that decoded nothing must not look like one.
    Unlinked,
    /// The reader refused the file or the stream was cut short.
    Reader(String),
}

impl std::fmt::Display for EmitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unlinked => f.write_str(
                "acadsharp-rs has no native library to decode with (Error::Unlinked): set \
                 ACADSHARP_NATIVE_DIR to an unpacked libviprs-dep archive and rebuild",
            ),
            Self::Reader(m) => f.write_str(m),
        }
    }
}

impl std::error::Error for EmitError {}

/// One view, as a walk saw it: the view record and every item of its stream.
#[derive(Debug, Clone)]
pub struct ViewWalk {
    pub view: View,
    pub items: Vec<Item>,
}

/// Read a DWG through the safe API and emit it.
///
/// Every view's stream must report [`acadsharp_rs::PrimitiveStream::is_complete`]:
/// a decode that stopped early yields an error and then nothing, and nothing on
/// its own looks exactly like an ending.
pub fn extract_bytes(bytes: &[u8]) -> Result<Emission, EmitError> {
    let decoder = match Decoder::new() {
        Ok(d) => d,
        Err(e) if e.is_unlinked() => return Err(EmitError::Unlinked),
        Err(e) => return Err(EmitError::Reader(format!("the handshake failed: {e}"))),
    };
    let reader = |what: &str, e: Error| EmitError::Reader(format!("{what}: {e}"));
    let document = Document::open_bytes(&decoder, bytes, &Limits::new())
        .map_err(|e| reader("acadsharp-rs could not open this file", e))?;
    let views = document
        .views()
        .map_err(|e| reader("could not list the views", e))?;
    let mut begin: Option<DocumentBegin> = None;
    let mut walks = Vec::with_capacity(views.len());
    for view in views {
        let mut stream = document
            .decode(view.index())
            .map_err(|e| reader(&format!("view {} would not decode", view.index()), e))?;
        let mut items = Vec::new();
        for item in &mut stream {
            let item = item.map_err(|e| reader(&format!("view {}", view.index()), e))?;
            if let Item::DocumentBegin(d) = &item {
                begin.get_or_insert(*d);
            }
            items.push(item);
        }
        if !stream.is_complete() {
            return Err(EmitError::Reader(format!(
                "view {} ({}) ended before its own totals said it should",
                view.index(),
                view.name()
            )));
        }
        walks.push(ViewWalk { view, items });
    }
    let version = begin.map(|d| d.drawing_version).unwrap_or(0);
    Ok(map_walk(version, &walks))
}

/// The leaf handle the sidecar names, uppercase hex like the other engine's.
fn handle_of(origin: acadsharp_rs::Origin) -> String {
    origin
        .item_handle
        .map_or_else(|| "0".to_string(), |h| format!("{:X}", h.get()))
}

/// A record and where the wire said it came from.
struct Entry {
    record: Record,
    handle: String,
    from_insert: Option<bool>,
}

impl Entry {
    fn framing(record: Record) -> Self {
        Self {
            record,
            handle: "0".to_string(),
            from_insert: None,
        }
    }
}

/// `1032` is `AC1032`; an unknown version is the empty string, which the
/// parser refuses loudly rather than a made-up one passing.
fn version_name(code: u32) -> String {
    if code == 0 {
        String::new()
    } else {
        format!("AC{code}")
    }
}

/// Schema 1 for what a walk saw. Pure.
pub fn map_walk(drawing_version: u32, walks: &[ViewWalk]) -> Emission {
    let mut head = vec![Entry::framing(Record::Document(DocumentRecord {
        format: "dwg".to_string(),
        acad_version: version_name(drawing_version),
        // Not on wire 2: `DocumentBegin` carries `view_count` and
        // `drawing_version` only. Zero here, and the field is excluded for
        // every pair this engine is in (docs/EMITTERS.md).
        maintenance_version: 0,
    }))];
    let mut body: Vec<Entry> = Vec::new();
    let mut warnings: Vec<(WarningRecord, String)> = Vec::new();

    // Views are already in tab order on the wire, so the index is the walk's.
    let mut ordered: Vec<&ViewWalk> = walks.iter().collect();
    ordered.sort_by_key(|w| w.view.index());
    for (index, walk) in ordered.iter().enumerate() {
        let index = index as u64;
        head.push(Entry::framing(Record::View(ViewRecord {
            index,
            name: walk.view.name().to_string(),
            kind: match walk.view.kind() {
                ViewKind::Model => "model",
                ViewKind::Layout => "layout",
                _ => "unknown",
            }
            .to_string(),
        })));
        for item in &walk.items {
            match item {
                Item::Primitive(p) => body.push(primitive(index, p)),
                Item::Warning(w) => warnings.push((warning(w), warn_handle(w))),
                // The library's own forward-compatibility probe, emitted on
                // purpose in every stream to check that a consumer skips by
                // length. It says nothing about the drawing.
                Item::Unknown { record_type, .. }
                    if *record_type >= acadsharp_rs::record_type::FORWARD_PROBE_MIN => {}
                Item::Unknown { record_type, .. } => warnings.push((
                    WarningRecord {
                        category: "unknown".to_string(),
                        entity_type: String::new(),
                        detail: format!("wire_record_{record_type}"),
                    },
                    "0".to_string(),
                )),
                // Framing the schema writes itself.
                _ => {}
            }
        }
    }

    // Sorted by (category, entity_type, detail), ordinally, duplicates kept.
    warnings.sort_by(|a, b| {
        (&a.0.category, &a.0.entity_type, &a.0.detail).cmp(&(
            &b.0.category,
            &b.0.entity_type,
            &b.0.detail,
        ))
    });

    let mut out = Emission::default();
    let push = |e: Entry, out: &mut Emission| {
        let index = out.records.len();
        out.sidecar.push(SidecarEntry {
            index,
            handles: vec![e.handle],
            // The wire carries no entity type, so `t` is absent on purpose and
            // the sidecar says it is partial by saying nothing here.
            record_type: None,
            from_insert: e.from_insert,
            dxf_name: None,
        });
        out.records.push(e.record);
    };
    for e in head {
        push(e, &mut out);
    }
    for e in body {
        push(e, &mut out);
    }
    for (w, handle) in warnings {
        push(
            Entry {
                record: Record::Warning(w),
                handle,
                from_insert: None,
            },
            &mut out,
        );
    }
    out
}

fn warn_handle(w: &Warning) -> String {
    w.entity
        .map_or_else(|| "0".to_string(), |h| format!("{:X}", h.get()))
}

/// Wire warning codes onto schema 1's categories. Best effort, and the pair
/// tables treat the warning stream as excluded: wire 2 names kinds by DXF name
/// and the oracle by ACadSharp class, and the counts differ anyway.
fn warning(w: &Warning) -> WarningRecord {
    let category = match w.code {
        WarningCode::UNSUPPORTED_ENTITY => "unsupported_entity",
        WarningCode::READER_NOTIFICATION => "reader_notification",
        WarningCode::UNRESOLVED_BLOCK => "missing_reference",
        WarningCode::NON_UNIFORM_BLOCK_SCALE => "malformed_geometry",
        WarningCode::NON_FINITE_GEOMETRY => "invalid_entity",
        c if c.get() == 109 => "unsupported_entity",
        _ => "unknown",
    };
    // The kind is the message's first token on the codes that name one.
    let entity_type = match w.code {
        WarningCode::UNSUPPORTED_ENTITY => w
            .message
            .split(|c: char| c.is_whitespace() || c == ':' || c == ',')
            .next()
            .unwrap_or_default()
            .to_string(),
        _ => String::new(),
    };
    WarningRecord {
        category: category.to_string(),
        entity_type,
        // A stable discriminator and never the message: a message can carry a
        // handle, which means nothing across two files.
        detail: format!("wire_{}", w.code.get()),
    }
}

fn primitive(view: u64, p: &Primitive) -> Entry {
    let origin = p.origin();
    let record = match p {
        Primitive::Line(l) => Record::Line(Line {
            view,
            a: l.start,
            b: l.end,
        }),
        Primitive::Polyline(pl) => Record::Polyline(polyline(view, pl)),
        // A hatch loop or a SOLID. The wire carries no entity type, so the two
        // look the same here, and a hatch loop's bulges have nowhere to go in
        // a polygon record (docs/EMITTERS.md, hatch boundaries).
        Primitive::Polygon(pl) => Record::Polygon(Polygon {
            view,
            points: pl.vertices.clone(),
        }),
        Primitive::Circle(c) => Record::Circle(Circle {
            view,
            center: point_to_ocs(c.normal, c.centre),
            radius: c.radius,
            normal: c.normal,
        }),
        Primitive::Arc(a) => {
            let (start, end) = sweep(a.start_angle, a.end_angle, 0.0);
            Record::Arc(Arc {
                view,
                center: point_to_ocs(a.normal, a.centre),
                radius: a.radius,
                start,
                end,
                normal: a.normal,
            })
        }
        Primitive::Ellipse(e) => {
            // A full ellipse is `start .. start + 2*pi` with start in range, which
            // is what `sweep` gives back for any pair a full turn apart.
            let (s, en) = sweep(e.start_param, e.end_param, 0.0);
            Record::Ellipse(Ellipse {
                view,
                center: point_to_ocs(e.normal, e.centre),
                major: point_to_ocs(e.normal, e.major_axis),
                ratio: e.ratio,
                start: s,
                end: en,
                normal: e.normal,
            })
        }
        Primitive::Spline(s) => Record::Spline(Spline {
            view,
            degree: u64::from(s.degree),
            closed: s.is_closed(),
            periodic: s.is_periodic(),
            control_points: s.controls.clone(),
            knots: s.knots.clone(),
            // A non-rational curve has no weights. The wire already sends
            // none for one, and a rational flag with no weights is the
            // reader's to explain, not this adapter's.
            weights: if s.is_rational() {
                s.weights.clone()
            } else {
                Vec::new()
            },
            // Not on wire 2 (docs/EMITTERS.md, spline.fit_points).
            fit_points: Vec::new(),
        }),
        Primitive::Text(t) => Record::Text(Text {
            view,
            at: t.position,
            height: t.height,
            rotation: normalize_angle(t.rotation),
            // Not on the wire: the schema's defaults, and the fields are
            // excluded for this engine rather than compared.
            width_factor: 1.0,
            halign: "left".to_string(),
            valign: "baseline".to_string(),
            style: "Standard".to_string(),
            text: t.text.replace("\r\n", "\n").replace('\r', "\n"),
        }),
        // A primitive a later crate version adds. Never dropped.
        _ => {
            return Entry {
                record: Record::Warning(WarningRecord {
                    category: "unknown".to_string(),
                    entity_type: String::new(),
                    detail: "wire_primitive_not_mapped".to_string(),
                }),
                handle: handle_of(origin),
                from_insert: Some(origin.from_expanded_insert),
            };
        }
    };
    Entry {
        record,
        handle: handle_of(origin),
        from_insert: Some(origin.from_expanded_insert),
    }
}

fn polyline(view: u64, p: &acadsharp_rs::Polyline) -> Polyline {
    let n = p.normal;
    let points: Vec<[f64; 3]> = p.vertices.iter().map(|v| point_to_ocs(n, *v)).collect();
    // Always one bulge per vertex, whatever the wire sent: `bulge_count` of 0
    // means every span is straight.
    let bulges = if p.bulges.len() == points.len() {
        p.bulges.clone()
    } else {
        vec![0.0; points.len()]
    };
    Polyline {
        view,
        closed: p.closed,
        normal: n,
        points,
        bulges,
    }
}
