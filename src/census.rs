//! AutoCAD's object census as a ground truth (R3.4, acadsharp-rs-tests#24).
//!
//! Upstream records one census per save, `sample_AC10xx_tree.json`, written by
//! AutoCAD's `DOCUMENTTREE` straight after the `SAVEAS` that produced the DWG.
//! It dumps the live session database, so the seven are equal by construction
//! and say nothing about the save format. What they can do is settle a count:
//! the entities in model space, by handle and by class, are the ones AutoCAD
//! says are there, whatever a reader found.
//!
//! Two checks, because the two kinds of sidecar say different things.
//!
//! * [`check_full`] is for a sidecar that names the top-level entity and its DXF
//!   name (`acadrust`, `ezdxf`). The set of outermost handles over the model
//!   space is the census' set, and a record that stands for the entity itself
//!   carries the DXF name the census' class maps to.
//! * [`check_weak`] is for `acadsharp-rs`, whose sidecar has one handle (the
//!   leaf), no entity type and an `x` flag for "this came out of an insert". The
//!   handles on records whose flag is clear must be in the census, and must cover
//!   every census entity that is not a block reference or a dimension.
//!
//! An entity the drawing marks invisible is allowed to be absent, because the
//! schema skips it by design; the census records the flag, so the rule needs no
//! list of exceptions. Anything else is a finding, and a finding is red.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use serde_json::Value;

use crate::canonical::{Record, Sidecar};
use crate::fixtures::{self, ManifestFile};

/// One entity of the census' model space.
#[derive(Debug, Clone, PartialEq)]
pub struct CensusEntity {
    pub handle: u64,
    /// The AutoCAD class without its namespace, for example `DBText`.
    pub class: String,
    pub invisible: bool,
}

/// What AutoCAD recorded for one save.
#[derive(Debug, Clone, PartialEq)]
pub struct Census {
    pub model_space: Vec<CensusEntity>,
    /// Every entity handle in every block record, so a warning that cannot say
    /// which view it came from is only unexpected when AutoCAD never had it.
    pub all_entity_handles: BTreeSet<u64>,
}

fn class_of(name: &str) -> String {
    name.rsplit('.').next().unwrap_or(name).to_string()
}

impl Census {
    /// Read the model space out of a census file.
    pub fn parse(json: &str) -> Result<Self, String> {
        let root: Value = serde_json::from_str(json).map_err(|e| format!("not JSON: {e}"))?;
        let entries = root
            .pointer("/BlocksTable/Entries")
            .and_then(Value::as_array)
            .ok_or("no BlocksTable.Entries")?;
        let mut model = None;
        let mut all = BTreeSet::new();
        for entry in entries {
            let name = entry.get("Name").and_then(Value::as_str).unwrap_or("");
            let Some(entities) = entry.get("Entities").and_then(Value::as_array) else {
                continue;
            };
            let mut rows = Vec::new();
            for e in entities {
                let handle = e
                    .get("Handle")
                    .and_then(Value::as_u64)
                    .ok_or("an entity with no Handle")?;
                let class = class_of(
                    e.get("ACadName")
                        .and_then(Value::as_str)
                        .ok_or("an entity with no ACadName")?,
                );
                let invisible = e
                    .get("IsInvisible")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                all.insert(handle);
                rows.push(CensusEntity {
                    handle,
                    class,
                    invisible,
                });
            }
            if name.eq_ignore_ascii_case("*Model_Space") {
                model = Some(rows);
            }
        }
        Ok(Self {
            model_space: model.ok_or("no *Model_Space block record")?,
            all_entity_handles: all,
        })
    }

    /// The census that belongs to this DWG fixture, verified against the
    /// manifest on the way in.
    pub fn for_fixture(id: &str) -> Result<Self, String> {
        let row = fixtures::censuses()
            .iter()
            .find(|c| c.for_fixtures.iter().any(|f| f == id))
            .ok_or_else(|| format!("no census row for {id}"))?;
        let bytes = row.load().map_err(|e| e.to_string())?;
        Self::parse(&String::from_utf8(bytes).map_err(|e| e.to_string())?)
    }

    fn visible(&self) -> impl Iterator<Item = &CensusEntity> {
        self.model_space.iter().filter(|e| !e.invisible)
    }

    fn model_handles(&self) -> BTreeSet<u64> {
        self.model_space.iter().map(|e| e.handle).collect()
    }
}

/// AutoCAD's class names onto the DXF names a reader's sidecar carries. One
/// table, so the mapping is a thing a reviewer can read.
const CLASS_MAP: &[(&str, &[&str])] = &[
    ("Line", &["LINE"]),
    ("DBPoint", &["POINT"]),
    ("DBText", &["TEXT"]),
    ("MText", &["MTEXT"]),
    ("AttributeDefinition", &["ATTDEF"]),
    ("AttributeReference", &["ATTRIB"]),
    ("Circle", &["CIRCLE"]),
    ("Arc", &["ARC"]),
    ("Ellipse", &["ELLIPSE"]),
    ("Spline", &["SPLINE"]),
    ("Polyline", &["LWPOLYLINE", "POLYLINE"]),
    ("Polyline2d", &["POLYLINE"]),
    ("Polyline3d", &["POLYLINE"]),
    ("PolyFaceMesh", &["POLYLINE"]),
    ("PolygonMesh", &["POLYLINE"]),
    ("Solid", &["SOLID"]),
    ("Trace", &["TRACE"]),
    ("Face", &["3DFACE"]),
    ("Hatch", &["HATCH"]),
    ("BlockReference", &["INSERT"]),
    ("Table", &["ACAD_TABLE", "INSERT"]),
    ("RotatedDimension", &["DIMENSION"]),
    ("AlignedDimension", &["DIMENSION"]),
    ("RadialDimension", &["DIMENSION"]),
    ("DiametricDimension", &["DIMENSION"]),
    ("OrdinateDimension", &["DIMENSION"]),
    ("Point3AngularDimension", &["DIMENSION"]),
    ("LineAngularDimension2", &["DIMENSION"]),
    ("ArcDimension", &["ARC_DIMENSION"]),
    ("Viewport", &["VIEWPORT"]),
    ("MLeader", &["MULTILEADER"]),
    ("Leader", &["LEADER"]),
    ("Mline", &["MLINE"]),
    ("FeatureControlFrame", &["TOLERANCE"]),
    ("SubDMesh", &["MESH"]),
    ("Solid3d", &["3DSOLID"]),
    ("Body", &["BODY"]),
    ("Region", &["REGION"]),
    ("Xline", &["XLINE"]),
    ("Ray", &["RAY"]),
    ("Wipeout", &["WIPEOUT"]),
    ("Shape", &["SHAPE"]),
    ("RasterImage", &["IMAGE"]),
    ("PdfReference", &["PDFUNDERLAY"]),
    ("DgnReference", &["DGNUNDERLAY"]),
    ("DwfReference", &["DWFUNDERLAY"]),
    ("Section", &["SECTION"]),
    ("Light", &["LIGHT"]),
    ("Camera", &["CAMERA"]),
    ("Ole2Frame", &["OLE2FRAME"]),
    ("OleFrame", &["OLEFRAME"]),
    ("ProxyEntity", &["ACAD_PROXY_ENTITY"]),
];

/// The DXF names an AutoCAD class is written under, if the table knows it.
pub fn dxf_names(class: &str) -> Option<&'static [&'static str]> {
    CLASS_MAP.iter().find(|(c, _)| *c == class).map(|(_, n)| *n)
}

/// What a check found. Each one names the handle, in hex the way a sidecar
/// spells it.
#[derive(Debug, Clone, PartialEq)]
pub enum Finding {
    /// A visible census entity no record names.
    Missing { handle: u64, class: String },
    /// A handle the reader reports that the census does not have.
    Unexpected { handle: u64 },
    /// The reader names the entity as a different kind.
    WrongClass {
        handle: u64,
        census: String,
        reader: String,
    },
    /// The census has a class the name map does not know.
    UnmappedClass { class: String },
    /// Nothing to check: the reader wrote no model-space entity at all.
    Vacuous,
}

impl Finding {
    /// The handle this is about, when it is about one.
    pub fn handle(&self) -> Option<u64> {
        match self {
            Self::Missing { handle, .. }
            | Self::Unexpected { handle }
            | Self::WrongClass { handle, .. } => Some(*handle),
            _ => None,
        }
    }

    /// `missing`, `unexpected`, `wrong_class`, `unmapped_class` or `vacuous`,
    /// the words `parity/census_exceptions.toml` uses.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Missing { .. } => "missing",
            Self::Unexpected { .. } => "unexpected",
            Self::WrongClass { .. } => "wrong_class",
            Self::UnmappedClass { .. } => "unmapped_class",
            Self::Vacuous => "vacuous",
        }
    }
}

impl fmt::Display for Finding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Missing { handle, class } => write!(
                f,
                "handle {handle:X} ({class}) is in AutoCAD's model space and the reader never reported it"
            ),
            Self::Unexpected { handle } => write!(
                f,
                "handle {handle:X} is reported by the reader and is not in AutoCAD's model space"
            ),
            Self::WrongClass {
                handle,
                census,
                reader,
            } => write!(
                f,
                "handle {handle:X} is a {census} to AutoCAD and a {reader} to the reader"
            ),
            Self::UnmappedClass { class } => write!(
                f,
                "the census has a {class} and the class name map does not know it"
            ),
            Self::Vacuous => write!(
                f,
                "the reader wrote no model-space entity, so there is nothing to check"
            ),
        }
    }
}

fn parse_handle(h: &str) -> Option<u64> {
    u64::from_str_radix(h, 16).ok()
}

/// The index of the model view, which is the first `view` record of kind
/// `model`.
fn model_view(records: &[Record]) -> Option<u64> {
    records.iter().find_map(|r| match r {
        Record::View(v) if v.kind == "model" => Some(v.index),
        _ => None,
    })
}

fn is_reference_or_dimension(class: &str) -> bool {
    dxf_names(class).is_some_and(|n| n.contains(&"INSERT") || n.contains(&"DIMENSION"))
}

fn unmapped(census: &Census) -> Vec<Finding> {
    let classes: BTreeSet<&str> = census
        .model_space
        .iter()
        .map(|e| e.class.as_str())
        .filter(|c| dxf_names(c).is_none())
        .collect();
    classes
        .into_iter()
        .map(|c| Finding::UnmappedClass {
            class: c.to_string(),
        })
        .collect()
}

/// The full check, for a reader whose sidecar names the top-level entity and
/// its DXF name (acadrust, ezdxf).
pub fn check_full(census: &Census, records: &[Record], sidecar: &Sidecar) -> Vec<Finding> {
    let mut findings = unmapped(census);
    let model = model_view(records);

    // Outermost handle of every model-space record, and the classes the records
    // that stand for the entity itself claim.
    let mut seen: BTreeSet<u64> = BTreeSet::new();
    let mut reader_class: BTreeMap<u64, String> = BTreeMap::new();
    // Warnings cannot say which view they came from, so they only ever cover.
    let mut warned: BTreeSet<u64> = BTreeSet::new();
    for (record, entry) in records.iter().zip(&sidecar.entries) {
        let Some(h0) = entry.handles.first().and_then(|h| parse_handle(h)) else {
            continue;
        };
        if h0 == 0 {
            continue;
        }
        match record {
            Record::Warning(_) => {
                warned.insert(h0);
            }
            r if r.view().is_some() && r.view() == model => {
                seen.insert(h0);
                if entry.handles.len() == 1
                    && let Some(d) = &entry.dxf_name
                {
                    reader_class.entry(h0).or_insert_with(|| d.clone());
                }
            }
            _ => {}
        }
    }

    if seen.is_empty() && warned.is_empty() {
        findings.push(Finding::Vacuous);
        return findings;
    }

    let model_handles = census.model_handles();
    for e in census.visible() {
        if !seen.contains(&e.handle) && !warned.contains(&e.handle) {
            findings.push(Finding::Missing {
                handle: e.handle,
                class: e.class.clone(),
            });
        }
    }
    for h in &seen {
        if !model_handles.contains(h) {
            findings.push(Finding::Unexpected { handle: *h });
        }
    }
    for h in &warned {
        if !census.all_entity_handles.contains(h) {
            findings.push(Finding::Unexpected { handle: *h });
        }
    }
    for e in &census.model_space {
        if let (Some(reader), Some(names)) = (reader_class.get(&e.handle), dxf_names(&e.class))
            && !names.contains(&reader.as_str())
        {
            findings.push(Finding::WrongClass {
                handle: e.handle,
                census: e.class.clone(),
                reader: reader.clone(),
            });
        }
    }
    findings
}

/// The weaker check for a sidecar with no entity type and no path
/// (acadsharp-rs): the handles on records whose `x` flag is clear.
pub fn check_weak(census: &Census, records: &[Record], sidecar: &Sidecar) -> Vec<Finding> {
    let mut findings = unmapped(census);
    let model = model_view(records);
    let mut top: BTreeSet<u64> = BTreeSet::new();
    for (record, entry) in records.iter().zip(&sidecar.entries) {
        if record.view().is_none() || record.view() != model || entry.from_insert != Some(false) {
            continue;
        }
        if let Some(h) = entry.handles.last().and_then(|h| parse_handle(h))
            && h != 0
        {
            top.insert(h);
        }
    }
    if top.is_empty() {
        findings.push(Finding::Vacuous);
        return findings;
    }
    let model_handles = census.model_handles();
    for h in &top {
        if !model_handles.contains(h) {
            findings.push(Finding::Unexpected { handle: *h });
        }
    }
    for e in census.visible() {
        if !is_reference_or_dimension(&e.class) && !top.contains(&e.handle) {
            findings.push(Finding::Missing {
                handle: e.handle,
                class: e.class.clone(),
            });
        }
    }
    findings
}
