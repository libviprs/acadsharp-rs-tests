//! The `model-1` stream (docs/MODEL_1.md): header variables, and the tables and
//! objects that follow them, in a second canonical stream beside schema 1.
//!
//! Three things live here: the records with a strict parser and writer, the
//! comparator, and the rules file (`parity/MODEL1.toml`) that says which
//! differences are by construction and which names an engine cannot produce.
//! The comparator keys a record by what it *is* (a header variable by its DXF
//! `$NAME`), never by position, so a reader that lists variables in another
//! order, or a file that lacks half of them, compares by name.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use serde::{Deserialize, Serialize};
use serde_json::Value as Json;

use crate::canonical::{format_real, quote};
use crate::compare::BOUNDED;
use crate::verdict::Verdict;

/// A value in the one written form per type.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Str(String),
    Int(i64),
    Double(f64),
    Bool(bool),
    Point2([f64; 2]),
    Point3([f64; 3]),
    /// A list of records, each one a value per field of the element schema.
    List(Vec<Vec<Value>>),
    /// The file does not store this field, written `null`. Only a reader of a
    /// DXF can say so: AutoCAD leaves a code out when it holds the default, and
    /// the default is not a value the file gave. Never in a header record.
    Absent,
}

impl Value {
    /// The `type` field a value is written under.
    pub fn type_name(&self) -> &'static str {
        match self {
            Self::Str(_) => "string",
            Self::Int(_) => "int",
            Self::Double(_) => "double",
            Self::Bool(_) => "bool",
            Self::Point2(_) => "point2",
            Self::Point3(_) => "point3",
            Self::List(_) => "list",
            Self::Absent => "null",
        }
    }

    fn render(&self) -> Result<String, String> {
        let r = |v: f64| format_real(v).map_err(|e| e.to_string());
        Ok(match self {
            Self::Str(s) => quote(s),
            Self::Int(i) => i.to_string(),
            Self::Double(d) => r(*d)?,
            Self::Bool(b) => b.to_string(),
            Self::Point2([x, y]) => format!("[{},{}]", r(*x)?, r(*y)?),
            Self::Point3([x, y, z]) => format!("[{},{},{}]", r(*x)?, r(*y)?, r(*z)?),
            Self::List(_) => return Err("a list is written through its schema".into()),
            Self::Absent => "null".to_string(),
        })
    }

    /// The value as it is shown in a finding.
    pub fn show(&self) -> String {
        match self {
            Self::List(l) => format!("[{} elements]", l.len()),
            other => other.render().unwrap_or_else(|e| format!("<{e}>")),
        }
    }
}

/// The type of one field of a table kind.
#[derive(Debug, Clone, Copy)]
pub enum Ty {
    Str,
    Int,
    Double,
    Bool,
    Point2,
    Point3,
    /// A list whose elements are records of these fields.
    List(&'static [(&'static str, Ty)]),
}

/// The fields of one kind of table entry, after its `name`. The order is the
/// order of the written line.
pub struct Schema {
    pub kind: &'static str,
    pub fields: &'static [(&'static str, Ty)],
}

const LAYER: Schema = Schema {
    kind: "layer",
    fields: &[
        ("flags", Ty::Int),
        ("color", Ty::Int),
        ("true_color", Ty::Int),
        ("on", Ty::Bool),
        ("linetype", Ty::Str),
        ("lineweight", Ty::Int),
        ("plot", Ty::Bool),
    ],
};

const LINETYPE: Schema = Schema {
    kind: "linetype",
    fields: &[
        ("description", Ty::Str),
        ("alignment", Ty::Int),
        ("pattern_length", Ty::Double),
        ("segments", Ty::List(SEGMENT)),
    ],
};

const TEXTSTYLE: Schema = Schema {
    kind: "textstyle",
    fields: &[
        ("font_file", Ty::Str),
        ("big_font_file", Ty::Str),
        ("height", Ty::Double),
        ("width_factor", Ty::Double),
        ("last_height", Ty::Double),
        ("oblique_angle", Ty::Double),
        ("backward", Ty::Bool),
        ("upside_down", Ty::Bool),
        ("is_shape", Ty::Bool),
        ("is_vertical", Ty::Bool),
        ("xref_dependent", Ty::Bool),
        ("annotative", Ty::Bool),
    ],
};

const DIMSTYLE: Schema = Schema {
    kind: "dimstyle",
    fields: &[
        ("$DIMPOST", Ty::Str),
        ("$DIMAPOST", Ty::Str),
        ("$DIMSCALE", Ty::Double),
        ("$DIMASZ", Ty::Double),
        ("$DIMEXO", Ty::Double),
        ("$DIMDLI", Ty::Double),
        ("$DIMEXE", Ty::Double),
        ("$DIMRND", Ty::Double),
        ("$DIMDLE", Ty::Double),
        ("$DIMTP", Ty::Double),
        ("$DIMTM", Ty::Double),
        ("$DIMFXL", Ty::Double),
        ("$DIMJOGANG", Ty::Double),
        ("$DIMTFILL", Ty::Int),
        ("$DIMTOL", Ty::Int),
        ("$DIMLIM", Ty::Int),
        ("$DIMTIH", Ty::Int),
        ("$DIMTOH", Ty::Int),
        ("$DIMSE1", Ty::Int),
        ("$DIMSE2", Ty::Int),
        ("$DIMTAD", Ty::Int),
        ("$DIMZIN", Ty::Int),
        ("$DIMAZIN", Ty::Int),
        ("$DIMARCSYM", Ty::Int),
        ("$DIMTXT", Ty::Double),
        ("$DIMCEN", Ty::Double),
        ("$DIMTSZ", Ty::Double),
        ("$DIMALTF", Ty::Double),
        ("$DIMLFAC", Ty::Double),
        ("$DIMTVP", Ty::Double),
        ("$DIMTFAC", Ty::Double),
        ("$DIMGAP", Ty::Double),
        ("$DIMALTRND", Ty::Double),
        ("$DIMALT", Ty::Int),
        ("$DIMALTD", Ty::Int),
        ("$DIMTOFL", Ty::Int),
        ("$DIMSAH", Ty::Int),
        ("$DIMTIX", Ty::Int),
        ("$DIMSOXD", Ty::Int),
        ("$DIMCLRD", Ty::Int),
        ("$DIMCLRE", Ty::Int),
        ("$DIMCLRT", Ty::Int),
        ("$DIMADEC", Ty::Int),
        ("$DIMUNIT", Ty::Int),
        ("$DIMDEC", Ty::Int),
        ("$DIMTDEC", Ty::Int),
        ("$DIMALTU", Ty::Int),
        ("$DIMALTTD", Ty::Int),
        ("$DIMAUNIT", Ty::Int),
        ("$DIMFRAC", Ty::Int),
        ("$DIMLUNIT", Ty::Int),
        ("$DIMDSEP", Ty::Int),
        ("$DIMTMOVE", Ty::Int),
        ("$DIMJUST", Ty::Int),
        ("$DIMSD1", Ty::Int),
        ("$DIMSD2", Ty::Int),
        ("$DIMTOLJ", Ty::Int),
        ("$DIMTZIN", Ty::Int),
        ("$DIMALTZ", Ty::Int),
        ("$DIMALTTZ", Ty::Int),
        ("$DIMFIT", Ty::Int),
        ("$DIMUPT", Ty::Int),
        ("$DIMATFIT", Ty::Int),
        ("$DIMFXLON", Ty::Int),
        ("$DIMTXTDIRECTION", Ty::Int),
        ("$DIMTXSTY", Ty::Str),
        ("$DIMLDRBLK", Ty::Str),
        ("$DIMBLK", Ty::Str),
        ("$DIMBLK1", Ty::Str),
        ("$DIMBLK2", Ty::Str),
        ("$DIMLTYPE", Ty::Str),
        ("$DIMLTEX1", Ty::Str),
        ("$DIMLTEX2", Ty::Str),
        ("$DIMLWD", Ty::Int),
        ("$DIMLWE", Ty::Int),
        ("annotative", Ty::Bool),
    ],
};

/// Every kind with a record, in `docs/MODEL_1.md`. One `const` per kind above
/// and one name here, so a new kind never touches the others.
pub const SCHEMAS: &[Schema] = &[LAYER, LINETYPE, TEXTSTYLE, DIMSTYLE];

/// One segment of a linetype's pattern, in file order.
const SEGMENT: &[(&str, Ty)] = &[
    ("length", Ty::Double),
    ("flags", Ty::Int),
    ("shape_number", Ty::Int),
    ("scale", Ty::Double),
    ("rotation", Ty::Double),
    ("offset", Ty::Point2),
    ("text", Ty::Str),
    ("style", Ty::Str),
];

pub fn schema(kind: &str) -> Option<&'static Schema> {
    SCHEMAS.iter().find(|s| s.kind == kind)
}

/// One header variable.
#[derive(Debug, Clone, PartialEq)]
pub struct Header {
    /// The DXF name, `$LTSCALE`.
    pub name: String,
    pub value: Value,
}

/// One table entry of any kind that has a [`Schema`]: its kind, its key, and a
/// value per field in the schema's order.
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub kind: &'static str,
    pub name: String,
    pub values: Vec<Value>,
}

impl Entry {
    pub fn new(kind: &'static str, name: impl Into<String>, values: Vec<Value>) -> Self {
        Self {
            kind,
            name: name.into(),
            values,
        }
    }

    /// The value of a field by name.
    pub fn get(&self, field: &str) -> Option<&Value> {
        let s = schema(self.kind)?;
        let i = s.fields.iter().position(|(n, _)| *n == field)?;
        self.values.get(i)
    }

    /// The value of a field by name, replaced. For controls that change one
    /// value in a scratch copy.
    pub fn set(&mut self, field: &str, value: Value) -> bool {
        let Some(s) = schema(self.kind) else {
            return false;
        };
        let Some(i) = s.fields.iter().position(|(n, _)| *n == field) else {
            return false;
        };
        self.values[i] = value;
        true
    }
}

/// A model-1 record. Every further kind is one more [`Schema`] and one more
/// section of docs/MODEL_1.md.
#[derive(Debug, Clone, PartialEq)]
pub enum Record {
    Header(Header),
    Entry(Entry),
}

impl Record {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Header(_) => "header",
            Self::Entry(e) => e.kind,
        }
    }

    /// The key a record is compared under, inside its kind.
    pub fn key(&self) -> &str {
        match self {
            Self::Header(h) => &h.name,
            Self::Entry(e) => &e.name,
        }
    }
}

/// The written form of a header variable, decided by the DXF group code(s) it
/// is stored under. The same table is in the oracle (`Model1Extractor.HeaderType`)
/// and in `tools/model1.py`, written three times on purpose.
pub fn header_type(name: &str, codes: &[u16]) -> Result<&'static str, String> {
    match codes.len() {
        2 => return Ok("point2"),
        3 => return Ok("point3"),
        1 => {}
        n => return Err(format!("{name}: {n} codes")),
    }
    let code = codes[0];
    Ok(match code {
        0..=9 | 100..=102 => "string",
        40..=59 | 140..=149 => "double",
        60..=79 | 90..=99 | 160..=179 | 270..=289 | 370..=389 => "int",
        290..=299 => "bool",
        _ => {
            return Err(format!(
                "{name}: no written form is defined for DXF code {code}"
            ));
        }
    })
}

/// Is this a group code that holds the handle of an object? A variable stored
/// under one has no record: handles stay out of the compared stream.
pub fn is_handle_code(code: u16) -> bool {
    matches!(code, 320..=369 | 390..=399 | 480..=481)
}

/// Where an emitter writes its model-1 stream for `fixture`.
pub fn output_path(out: &std::path::Path, fixture: &str, engine: &str) -> std::path::PathBuf {
    out.join(format!("{fixture}.{engine}.model1.jsonl"))
}

// ---- writing ------------------------------------------------------------------------

/// The kinds in stream order, every one `docs/MODEL_1.md` plans. A kind with no
/// [`Schema`] yet is refused by the parser and the writer.
pub const KIND_ORDER: [&str; 10] = [
    "header",
    "layer",
    "linetype",
    "textstyle",
    "dimstyle",
    "block_record",
    "layout",
    "dictionary",
    "xdata",
    "xrecord",
];

fn kind_index(kind: &str) -> Option<usize> {
    KIND_ORDER.iter().position(|k| *k == kind)
}

fn render_typed(v: &Value, ty: &Ty) -> Result<String, String> {
    match (ty, v) {
        (Ty::List(_), Value::Absent) => Err("a list is always stored".into()),
        (_, Value::Absent) => Ok("null".to_string()),
        (Ty::Str, Value::Str(_))
        | (Ty::Int, Value::Int(_))
        | (Ty::Double, Value::Double(_))
        | (Ty::Bool, Value::Bool(_))
        | (Ty::Point2, Value::Point2(_))
        | (Ty::Point3, Value::Point3(_)) => v.render(),
        (Ty::List(fields), Value::List(items)) => {
            let mut parts = Vec::new();
            for item in items {
                parts.push(render_fields(fields, item)?);
            }
            Ok(format!("[{}]", parts.join(",")))
        }
        _ => Err(format!("a {} where the schema says {ty:?}", v.type_name())),
    }
}

fn render_fields(fields: &[(&'static str, Ty)], values: &[Value]) -> Result<String, String> {
    if fields.len() != values.len() {
        return Err(format!(
            "{} values for {} fields",
            values.len(),
            fields.len()
        ));
    }
    let mut parts = Vec::new();
    for ((name, ty), v) in fields.iter().zip(values) {
        parts.push(format!("\"{name}\":{}", render_typed(v, ty)?));
    }
    Ok(format!("{{{}}}", parts.join(",")))
}

/// The exact text of a `.model1.jsonl`: sorted by (kind order, key), LF, a final
/// newline. Two records with one key are refused.
pub fn render_stream(records: &[Record]) -> Result<String, String> {
    let mut sorted: Vec<&Record> = records.iter().collect();
    sorted.sort_by(|a, b| {
        kind_index(a.kind())
            .cmp(&kind_index(b.kind()))
            .then_with(|| a.key().as_bytes().cmp(b.key().as_bytes()))
    });
    let mut out = String::new();
    let mut prev: Option<(&str, &str)> = None;
    for r in sorted {
        if prev == Some((r.kind(), r.key())) {
            return Err(format!("two {} records are keyed {}", r.kind(), r.key()));
        }
        prev = Some((r.kind(), r.key()));
        match r {
            Record::Header(h) => {
                out.push_str(&format!(
                    "{{\"model\":1,\"record\":\"header\",\"name\":{},\"type\":\"{}\",\"value\":{}}}\n",
                    quote(&h.name),
                    h.value.type_name(),
                    h.value.render()?
                ));
            }
            Record::Entry(e) => {
                let s = schema(e.kind).ok_or(format!("{} has no schema", e.kind))?;
                let body = render_fields(s.fields, &e.values)?;
                // `{"a":1}` becomes `,"a":1}` so the fields follow the name.
                out.push_str(&format!(
                    "{{\"model\":1,\"record\":\"{}\",\"name\":{},{}\n",
                    e.kind,
                    quote(&e.name),
                    &body[1..]
                ));
            }
        }
    }
    Ok(out)
}

// ---- reading ------------------------------------------------------------------------

fn parse_typed(j: &Json, ty: &Ty, what: &str) -> Result<Value, String> {
    let num = |j: &Json| j.as_f64().ok_or(format!("{what}: not a number"));
    if j.is_null() && !matches!(ty, Ty::List(_)) {
        return Ok(Value::Absent);
    }
    Ok(match ty {
        Ty::Str => Value::Str(j.as_str().ok_or(format!("{what}: not a string"))?.into()),
        Ty::Int => Value::Int(j.as_i64().ok_or(format!("{what}: not an integer"))?),
        Ty::Double => Value::Double(num(j)?),
        Ty::Bool => Value::Bool(j.as_bool().ok_or(format!("{what}: not a bool"))?),
        Ty::Point2 | Ty::Point3 => {
            let a = j.as_array().ok_or(format!("{what}: not an array"))?;
            match (ty, a.len()) {
                (Ty::Point2, 2) => Value::Point2([num(&a[0])?, num(&a[1])?]),
                (Ty::Point3, 3) => Value::Point3([num(&a[0])?, num(&a[1])?, num(&a[2])?]),
                _ => return Err(format!("{what}: {} components", a.len())),
            }
        }
        Ty::List(fields) => {
            let a = j.as_array().ok_or(format!("{what}: not an array"))?;
            let mut items = Vec::new();
            for (i, item) in a.iter().enumerate() {
                items.push(parse_fields(item, fields, &format!("{what}[{i}]"))?);
            }
            Value::List(items)
        }
    })
}

fn parse_fields(j: &Json, fields: &[(&'static str, Ty)], what: &str) -> Result<Vec<Value>, String> {
    let obj = j.as_object().ok_or(format!("{what}: not an object"))?;
    let mut have: Vec<&str> = obj.keys().map(String::as_str).collect();
    let mut want: Vec<&str> = fields.iter().map(|(n, _)| *n).collect();
    have.sort_unstable();
    want.sort_unstable();
    if have != want {
        return Err(format!("{what}: fields {have:?}, want {want:?}"));
    }
    fields
        .iter()
        .map(|(n, ty)| parse_typed(&obj[*n], ty, &format!("{what}.{n}")))
        .collect()
}

/// Parse a stream strictly: every line is `{"model":1,"record":..}` with the
/// fields the document gives, in that order, a `type` that agrees with the shape
/// of its `value`, and the lines are in stream order with no key twice.
pub fn parse_stream(text: &str) -> Result<Vec<Record>, String> {
    if !text.is_empty() && !text.ends_with('\n') {
        return Err("no final newline".into());
    }
    if text.contains('\r') {
        return Err("a carriage return".into());
    }
    let mut out: Vec<Record> = Vec::new();
    for (i, l) in text.lines().enumerate() {
        let n = i + 1;
        let v: Json = serde_json::from_str(l).map_err(|e| format!("line {n}: {e}"))?;
        let obj = v.as_object().ok_or(format!("line {n}: not an object"))?;
        if obj.get("model").and_then(Json::as_i64) != Some(1) {
            return Err(format!("line {n}: model is not 1"));
        }
        let rec = obj.get("record").and_then(Json::as_str).unwrap_or("");
        let name = obj
            .get("name")
            .and_then(Json::as_str)
            .ok_or(format!("line {n}: name"))?
            .to_string();
        let r = if rec == "header" {
            let mut have: Vec<&str> = obj.keys().map(String::as_str).collect();
            have.sort_unstable();
            if have != ["model", "name", "record", "type", "value"] {
                return Err(format!("line {n}: fields {have:?}"));
            }
            let ty = match obj["type"].as_str().ok_or(format!("line {n}: type"))? {
                "string" => Ty::Str,
                "int" => Ty::Int,
                "double" => Ty::Double,
                "bool" => Ty::Bool,
                "point2" => Ty::Point2,
                "point3" => Ty::Point3,
                other => return Err(format!("line {n}: type {other:?}")),
            };
            Record::Header(Header {
                name,
                value: parse_typed(&obj["value"], &ty, &format!("line {n}"))?,
            })
        } else {
            let s = schema(rec).ok_or(format!("line {n}: {rec:?} is not a model-1 record"))?;
            let mut have: Vec<&str> = obj.keys().map(String::as_str).collect();
            let mut want: Vec<&str> = s.fields.iter().map(|(n, _)| *n).collect();
            want.extend(["model", "name", "record"]);
            have.sort_unstable();
            want.sort_unstable();
            if have != want {
                return Err(format!("line {n}: fields {have:?}, want {want:?}"));
            }
            let values = s
                .fields
                .iter()
                .map(|(f, ty)| parse_typed(&obj[*f], ty, &format!("line {n}.{f}")))
                .collect::<Result<Vec<_>, _>>()?;
            Record::Entry(Entry {
                kind: s.kind,
                name,
                values,
            })
        };
        if let Some(prev) = out.last() {
            let after = kind_index(r.kind())
                .cmp(&kind_index(prev.kind()))
                .then_with(|| r.key().as_bytes().cmp(prev.key().as_bytes()));
            if after != std::cmp::Ordering::Greater {
                return Err(format!("line {n}: {} is not after {}", r.key(), prev.key()));
            }
        }
        // Round trip: the line must be the one the writer would write, which
        // checks the order of the fields and the spelling of every number.
        let again = render_stream(std::slice::from_ref(&r))?;
        if again.trim_end() != l {
            return Err(format!(
                "line {n}: not in canonical form; the writer gives {}",
                again.trim_end()
            ));
        }
        out.push(r);
    }
    Ok(out)
}

// ---- the rules ----------------------------------------------------------------------

/// `parity/MODEL1.toml`.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Rules {
    #[serde(default, rename = "known")]
    pub known: Vec<Known>,
    #[serde(default, rename = "excluded")]
    pub excluded: Vec<Excluded>,
}

/// Names whose values differ by construction between two engines or two files.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Known {
    pub names: Vec<String>,
    /// Pairs the row applies to; empty means every pair.
    #[serde(default)]
    pub pairs: Vec<String>,
    /// Fixture ids the row applies to (the first side's: a save for
    /// `oracle/acadrust`, a twin for the ezdxf pairs); empty means every fixture.
    #[serde(default)]
    pub fixtures: Vec<String>,
    pub reason: String,
    pub issue: String,
}

/// Names an engine cannot produce for some files.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Excluded {
    /// `oracle`, `acadrust` or `ezdxf`.
    pub engine: String,
    pub names: Vec<String>,
    /// Fixture ids the row applies to; empty means every fixture.
    #[serde(default)]
    pub fixtures: Vec<String>,
    pub reason: String,
    pub issue: String,
}

impl Rules {
    pub fn parse(text: &str) -> Result<Self, String> {
        let r: Rules = toml::from_str(text).map_err(|e| e.to_string())?;
        for k in &r.known {
            if k.reason.trim().is_empty() || k.issue.trim().is_empty() {
                return Err(format!(
                    "known row {:?} needs a reason and an issue",
                    k.names
                ));
            }
        }
        for e in &r.excluded {
            if e.reason.trim().is_empty() || e.issue.trim().is_empty() {
                return Err(format!(
                    "excluded row {} {:?} needs a reason and an issue",
                    e.engine, e.names
                ));
            }
            if !["oracle", "acadrust", "ezdxf"].contains(&e.engine.as_str()) {
                return Err(format!("excluded row names the engine {:?}", e.engine));
            }
        }
        Ok(r)
    }

    fn excluded_for(&self, engine: &str, fixture: &str, name: &str) -> Option<&Excluded> {
        self.excluded.iter().find(|e| {
            e.engine == engine
                && e.names.iter().any(|n| glob(n, name))
                && (e.fixtures.is_empty() || e.fixtures.iter().any(|f| f == fixture))
        })
    }

    /// The row that lists a difference of `name` for this pair and fixture.
    pub fn known_for(&self, pair: &str, fixture: &str, name: &str) -> Option<&Known> {
        self.known.iter().find(|k| {
            k.names.iter().any(|n| glob(n, name))
                && (k.pairs.is_empty() || k.pairs.iter().any(|p| p == pair))
                && (k.fixtures.is_empty() || k.fixtures.iter().any(|f| f == fixture))
        })
    }
}

// ---- the frozen ground truths -------------------------------------------------------

/// One `[[file]]` of `fixtures/model1/MANIFEST.toml`.
#[derive(Debug, Clone, Deserialize)]
pub struct FrozenFile {
    pub id: String,
    pub engine: String,
    pub path: String,
    pub sha256: String,
    pub records: usize,
}

#[derive(Debug, Deserialize)]
struct FrozenManifest {
    #[serde(default)]
    file: Vec<FrozenFile>,
}

fn model1_dir() -> std::path::PathBuf {
    crate::fixtures::fixture_root().join("model1")
}

/// Every file `tools/model1.py freeze` committed.
pub fn frozen_files() -> Result<Vec<FrozenFile>, String> {
    let path = model1_dir().join("MANIFEST.toml");
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(toml::from_str::<FrozenManifest>(&text)
        .map_err(|e| e.to_string())?
        .file)
}

/// The frozen stream for `engine` (`oracle` or `ezdxf`) over `id`, after the
/// bytes have been checked against the hash the manifest froze.
pub fn load_frozen(engine: &str, id: &str) -> Result<Vec<Record>, String> {
    let files = frozen_files()?;
    let f = files
        .iter()
        .find(|f| f.engine == engine && f.id == id)
       >­ën®x›Ú×ÛÜ—Ù[ÙJ›Ü›X]J››Èœ›Ş™[ˆÙ[™Ú[™_H[Ù[LHİ™X[H›ÜˆÚYHŠJOÎÂˆ]]H[Ù[WÙ\Š
Kš›Ú[Š	™‹œ]
NÂˆ]]\ÈHİ™œÎœ™XY
	œ]
K›X\Ù\œŠ_›Ü›X]JßNˆÙ_H‹]™\Ü^J
JJOÎÂˆ]Ûİˆİš[™ÈHÂˆ\ÙHÚLÑYÙ\İÚLMŸNÂˆÚLM™YÙ\İ
	˜]\ÊBˆš]\Š
Bˆ›X\
Ÿ›Ü›X]JØŒHŠJBˆ˜ÛÛXİ

BˆNÂˆYˆÛİOH‹œÚLMˆÂˆ™]\›ˆ\œŠ›Ü›X]JˆßNˆ\Ú\ÈÈÙÛİKHX[šY™\İœ›Ş™HßH‹ˆ]™\Ü^J
Kˆ‹œÚLM‚ˆ
JNÂˆBˆ]^Hİš[™Î™œ›ÛWİ]
]\ÊK›X\Ù\œŠ_K×Üİš[™Ê
JOÎÂˆ\œÙWÜİ™X[J	^
K›X\Ù\œŠ_›Ü›X]JßNˆÙ_H‹]™\Ü^J
JJBŸB‚‹ËËÈHÛÛ[Z]Y\š]KÓSÑSKÛ[‚œXˆ›ˆÛÛ[Z]YÜ[\Ê
HOˆ™\İ[[\Ëİš[™ÏˆÂˆ]]Hİœ]”]›™]Ê[ˆJĞT‘Ó×ÓPS’Q‘TÕÑTˆŠJBˆš›Ú[Šœ\š]HŠBˆš›Ú[Š“SÑSKÛ[ŠNÂˆ]^Hİ™œÎœ™XYİ×Üİš[™Ê	œ]
K›X\Ù\œŠ_›Ü›X]JßNˆÙ_H‹]™\Ü^J
JJOÎÂˆ[\Îœ\œÙJ	^
BŸB‚‹ËÈKKKHÛÛ\\š[™ÈKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKB‚‹ËËÈÚ]Ú[™ÙˆY™™\™[˜ÙHHš[™[™È\Ë‚ˆÖÙ\š]™JXYËÛÛ™K\X[\K\JWBœXˆ[[Hš[™[™ÒÚ[™ÂˆÛ›R[KˆÛ›R[‹ˆ\Kˆ˜[YKŸB‚‹ËËÈÚ]Hš[™[™È[[İ[ÈÈÛ˜ÙHH[\È]™H™Y[ˆ\YY‚ˆÖÙ\š]™JXYËÛÛ™K\X[\K\JWBœXˆ[[H\ÜÜÚ][ÛˆÂˆËËÈ›İ[™È^Z[œÈ]ˆX‚ˆ™YˆËËÈH\İYY™™\™[˜ÙHHÛÛœİXİ[Û‹‚ˆÛ›İÛ‹ˆËËÈH˜[YHÛ™H[™Ú[™HØ[››İ›ÙXÙK\İY‚ˆ^ÛYYŸB‚ˆÖÙ\š]™JXYËÛÛ™JWBœXˆİXİš[™[™ÈÂˆXˆÚ[™ˆš[™[™ÒÚ[™ˆXˆÙ^Nˆİš[™ËˆXˆ]Z[ˆİš[™ËˆXˆ\ÜÜÚ][Ûˆ\ÜÜÚ][Û‹ˆËËÈH\ÜİYHÙˆH›İÈ]\İÈ]Ú[ˆÛ™HÙ\Ë‚ˆXˆ\ÜİYNˆİš[™ËˆËËÈ›Üˆ[ˆ^ÛYYš[™[™ËH[™Ú[™HH›İÈ^ÛY\Ë‚ˆXˆ^ÛYYˆÜ[Ûİš[™Ï‹ŸB‚š[\›]‘\Ü^H›Üˆš[™[™ÈÂˆ›ˆ›]
	œÙ[‹ˆ	›]]›]‘›Ü›X]\	×ÏŠHOˆ›]”™\İ[ÂˆÜš]HJˆ‹ˆšXY\ˆßHßHŞÎßWH‹ˆÙ[‹šÙ^KÙ[‹™]Z[Ù[‹™\ÜÜÚ][Û‚ˆ
BˆBŸB‚‹ËËÈÚ]Û™HÛÛ\\š\ÛÛˆØ[YHË‚ˆÖÙ\š]™JXYËÛÛ™KY˜][
WBœXˆİXİ™\ÜÂˆXˆš[™[™ÜÎˆ™XÏš[™[™Ï‹ˆËËÈİÈX[H˜[Y\ÈÙ\™HÛˆ›İÚY\È[™ÛÛ\\™Y‚ˆXˆÛÛ\\™Yˆ\Ú^™KŸB‚š[\™\ÜÂˆXˆ›ˆ™Y
	œÙ[ŠHOˆ™XÏ	‘š[™[™ÏˆÂˆÙ[‹™š[™[™ÜÂˆš]\Š
Bˆ™š[\ŠŸ‹™\ÜÜÚ][ÛˆOH\ÜÜÚ][Û”™Y
Bˆ˜ÛÛXİ

BˆB‚ˆXˆ›ˆ™\™Xİ
	œÙ[ŠHOˆ™\™XİÂˆ]™YHÙ[‹œ™Y

K›[Š
NÂˆYˆ™YˆÂˆ™]\›ˆ™\™Xİ‘Y™Š™Y
NÂˆBˆ]Û›İÛˆHÙ[‚ˆ™š[™[™ÜÂˆš]\Š
Bˆ™š[\ŠŸ‹™\ÜÜÚ][ÛˆOH\ÜÜÚ][Û’Û›İÛŠBˆ˜Ûİ[

NÂˆYˆÛ›İÛˆˆÂˆ™\™Xİ’Û›İÛŠÛ›İÛŠBˆH[ÙHÂˆ™\™Xİ“X]ÚˆBˆB‚ˆËËÈH\ÜİY\ÈÙˆH›İÜÈ]\İYHY™™\™[˜ÙK[ˆÜ™\‹Û˜ÙHXXÚ‚ˆXˆ›ˆ\ÜİY\Ê	œÙ[ŠHOˆİš[™ÈÂˆ]]]ÙY[ˆ™XÏ	œİˆH™XÎ›™]Ê
NÂˆ›Üˆˆ[ˆÙ[‚ˆ™š[™[™ÜÂˆš]\Š
Bˆ™š[\ŠŸ‹™\ÜÜÚ][ÛˆOH\ÜÜÚ][Û’Û›İÛŠBˆÂˆYˆY‹š\ÜİYKš\×Ù[\J
H	‰ˆ\ÙY[‹˜ÛÛZ[œÊ	™‹š\ÜİYK˜\×ÜİŠ
JHÂˆÙY[‹œ\Ú
	™‹š\ÜİYJNÂˆBˆBˆÙY[‹š›Ú[Š‹ŠBˆB‚ˆËËÈHš\œİš[™[™È]Ûİ[Ë™[™\™Y›ÜˆH™\™Xİš[K‚ˆXˆ›ˆš\œİ
	œÙ[ŠHOˆİš[™ÈÂˆÙ[‹œ™Y

Bˆ™š\œİ

Bˆ›X\
Ÿ‹×Üİš[™Ê
JBˆ›Ü—Ù[ÙJÂˆÙ[‹™š[™[™ÜÂˆš]\Š
Bˆ™š[™
Ÿ‹™\ÜÜÚ][ÛˆOH\ÜÜÚ][Û’Û›İÛŠBˆ›X\
Ÿ‹×Üİš[™Ê
JBˆJBˆ[Ü˜\ÛÜ—ÙY˜][

BˆBŸB‚™›ˆØ[YJNˆ	•˜[YKˆ	•˜[YJHOˆ›ÛÛÂˆ]HˆNˆ“ÕS‘QÚ][ŠJNÂˆX]Ú
KŠHÂˆ
˜[YN‘İX›J
K˜[YN‘İX›JJJHOˆ


JKˆ
˜[YN”Ú[Š
K˜[YN”Ú[ŠJJHOˆš]\Š
Kš\
JK˜[

J_

œ
œJJKˆ
˜[YN”Ú[Ê
K˜[YN”Ú[ÊJJHOˆš]\Š
Kš\
JK˜[

J_

œ
œJJKˆÈOˆHOH‹ˆBŸB‚‹ËËÈÛ™HÚYHÙˆHÛÛ\\š\ÛÛ‹‚œXˆİXİÚYO	ØOˆÂˆXˆ[™Ú[™Nˆ	‰ØHİ‹ˆXˆš^\™Nˆ	‰ØHİ‹ˆXˆ™XÛÜ™Îˆ	‰ØHÔ™XÛÜ™KŸB‚‹ËËÈH™XÛÜ™\ÈHÛÛ\\˜]ÜˆÙY\È]ˆÛ™H˜[YH
HXY\ˆ˜\šXX›JHÜˆH\İ‹ËËÈÙˆ˜[YYØØ[\ˆX]™\È
HX›H[KÚ]]È\İÈ[œ›ÛY
K‚™[[H][HÂˆØØ[\Š˜[YJKˆšY[Ê™XÏ
İš[™Ë˜[YJOŠKŸB‚š[\][HÂˆ›ˆÚİÊ	œÙ[ŠHOˆİš[™ÈÂˆX]ÚÙ[ˆÂˆÙ[”ØØ[\ŠŠHOˆ‹œÚİÊ
KˆÙ[‘šY[ÊŠHOˆ‚ˆš]\Š
Bˆ›X\

‹Š_›Ü›X]JÛŸO^ßH‹‹œÚİÊ
JJBˆ˜ÛÛXİ™XÏÏŠ
Bˆš›Ú[ŠˆŠKˆBˆBŸB‚‹ËËÈHÙ^HH™XÛÜ™\ÈÛÛ\\™Y[™\İY[™\ˆHˆ˜[YH›ÜˆHXY\‚‹ËËÈ˜\šXX›KÚ[™‹Ï˜[YO˜›Üˆ[][™È[ÙK‚™›ˆ][WÚÙ^Jˆ	”™XÛÜ™
HOˆİš[™ÈÂˆX]ÚˆÂˆ™XÛÜ™’XY\Š
HOˆ›˜[YK˜ÛÛ™J
Kˆ™XÛÜ™‘[JJHOˆ›Ü›X]JßKŞßH‹KšÚ[™K›˜[YJKˆBŸB‚‹ËËÈ[œ›ÛH˜[YH[ÈØØ[\ˆX]™\ÎˆH\İ\È]È[™İ˜[YK›[˜[™[‚‹ËËÈ˜[YVÚWK™šY[›ÜˆXXÚ[[Y[‚™›ˆX]™\Ê™Yš^ˆ	œİ‹Nˆ	•Kˆ	•˜[YKİ]ˆ	›]]™XÏ
İš[™Ë˜[YJOŠHÂˆX]Ú
KŠHÂˆ
N“\İ
šY[ÊK˜[YN“\İ
][\ÊJHOˆÂˆİ]œ\Ú

›Ü›X]JÜ™Yš^K›[ˆŠK˜[YN’[
][\Ë›[Š
H\ÈM
JJNÂˆ›Üˆ
K][JH[ˆ][\Ëš]\Š
K™[[Y\˜]J
HÂˆ›Üˆ

˜[YKJKŠH[ˆšY[Ëš]\Š
Kš\
][JHÂˆX]™\Ê	™›Ü›X]JÜ™Yš^VŞÚ_WKÛ˜[Y_HŠKK‹İ]
NÂˆBˆBˆBˆÈOˆİ]œ\Ú

™Yš^×Üİš[™Ê
K‹˜ÛÛ™J
JJKˆBŸB‚™›ˆ][WÛÙŠˆ	”™XÛÜ™
HOˆ][HÂˆX]ÚˆÂˆ™XÛÜ™’XY\Š
HOˆ][N”ØØ[\Š˜[YK˜ÛÛ™J
JKˆ™XÛÜ™‘[JJHOˆÂˆ]]]İ]H™XÎ›™]Ê
NÂˆYˆ]ÛÛYJÊHHØÚ[XJKšÚ[™
HÂˆ›Üˆ

˜[YKJKŠH[ˆË™šY[Ëš]\Š
Kš\
	™K˜[Y\ÊHÂˆX]™\Ê˜[YKK‹	›]]İ]
NÂˆBˆBˆ][N‘šY[Êİ]
BˆBˆBŸB‚‹ËËÈÙ\È]\›˜
ÚXÚX^HÛ
˜İ[™[™È›Üˆ[H[ˆÙˆÚ\˜Xİ\œÊHX]Ú‹ËËÈ^ÈH^Y\ˆ˜[YH\È]KÛÈH[HX›İ]]™\H^Y\‰ÜÈÛÛİ\ˆ\Â‹ËËÈ^Y\‹Ê‹˜ÛÛÜ˜‚œXˆ›ˆÛØŠ]\›ˆ	œİ‹^ˆ	œİŠHOˆ›ÛÛÂˆ]\Îˆ™XÏ	œİˆH]\›‹œÜ]
	Ê‰ÊK˜ÛÛXİ

NÂˆYˆ\Ë›[Š
HOHHÂˆ™]\›ˆ]\›ˆOH^ÂˆBˆ]]]™\İH^Âˆ›Üˆ
K\
H[ˆ\Ëš]\Š
K™[[Y\˜]J
HÂˆYˆHOHÂˆX]Ú™\İœİš\Ü™Yš^
\
HÂˆÛÛYJŠHOˆ™\İH‹ˆ›Û™HOˆ™]\›ˆ˜[ÙKˆBˆH[ÙHYˆHOH\Ë›[Š
HHHÂˆ™]\›ˆ™\İ™[™×İÚ]
\
NÂˆH[ÙHÂˆX]Ú™\İ™š[™
\
HÂˆÛÛYJ]
HOˆ™\İH	œ™\İØ]
È\›[Š
K‹—Kˆ›Û™HOˆ™]\›ˆ˜[ÙKˆBˆBˆBˆYBŸB‚‹ËËÈÛÛ\\™HÛÈİ™X[\ÈHÙ^KˆZ\˜\ÈÜ˜XÛKØXØY\İ[™ÛÈÛ‹H˜[YB‹ËËÈ\š]KÓSÑSKÛ[›İÜÈ\ÙK‚œXˆ›ˆÛÛ\\™JZ\ˆ	œİ‹Nˆ	”ÚYKˆ	”ÚYK[\Îˆ	”[\ÊHOˆ™\ÜÂˆ][™^HÎˆ	”ÚY_Oˆ•™YSX\İš[™Ë][OˆÂˆËœ™XÛÜ™Âˆš]\Š
Bˆ›X\
Ÿ
][WÚÙ^JŠK][WÛÙŠŠJJBˆ˜ÛÛXİ

BˆNÂˆ]
XKXŠHH
[™^
JK[™^
ŠJNÂˆ]˜[Y\Îˆ•™YTÙ]	”İš[™ÏˆHXKšÙ^\Ê
K˜ÚZ[ŠX‹šÙ^\Ê
JK˜ÛÛXİ

NÂˆ]]]™\ÜH™\Ü™Y˜][

NÂˆ]\ÚH™\Üˆ	›]]™\ÜÚ[™ˆš[™[™ÒÚ[™Ù^Nˆİš[™Ë]Z[ˆİš[™ßÂˆ]Û›İÛˆH[\ËšÛ›İÛ—Ù›ÜŠZ\‹K™š^\™K	šÙ^JNÂˆ™\Ü™š[™[™ÜËœ\Ú
š[™[™ÈÂˆÚ[™ˆÙ^Kˆ]Z[ˆ\ÜÜÚ][ÛˆYˆÛ›İÛ‹š\×ÜÛÛYJ
HÂˆ\ÜÜÚ][Û’Û›İÛ‚ˆH[ÙHÂˆ\ÜÜÚ][Û”™YˆKˆ\ÜİYNˆÛ›İÛ‹›X\
ßËš\ÜİYK˜ÛÛ™J
JK[Ü˜\ÛÜ—ÙY˜][

Kˆ^ÛYYˆ›Û™KˆJNÂˆNÂˆ]ÛÛ\\™Wİ˜[Y\ÈHˆ	•˜[YKNˆ	•˜[Y_OˆÜ[Û
š[™[™ÒÚ[™İš[™ÊOˆÂˆYˆ\WÛ˜[YJ
HOHK\WÛ˜[YJ
HÂˆÛÛYJ
ˆš[™[™ÒÚ[™•\Kˆ›Ü›X]J\HßHœÈßH‹\WÛ˜[YJ
KK\WÛ˜[YJ
JKˆ
JBˆH[ÙHYˆ\Ø[YJJHÂˆÛÛYJ
š[™[™ÒÚ[™•˜[YK›Ü›X]JßHœÈßH‹œÚİÊ
KKœÚİÊ
JJJBˆH[ÙHÂˆ›Û™BˆBˆNÂˆ›Üˆ˜[YH[ˆ˜[Y\ÈÂˆX]Ú
XK™Ù]
˜[YJKX‹™Ù]
˜[YJJHÂˆ
ÛÛYJ][N”ØØ[\Š
JKÛÛYJ][N”ØØ[\ŠJJJHOˆÂˆ™\Ü˜ÛÛ\\™Y
ÏHNÂˆYˆ]ÛÛYJ
Ú[™]Z[
JHHÛÛ\\™Wİ˜[Y\ÊJHÂˆ\Ú
	›]]™\ÜÚ[™˜[YK˜ÛÛ™J
K]Z[
NÂˆBˆBˆ
ÛÛYJ][N‘šY[Ê
JKÛÛYJ][N‘šY[ÊJJJHOˆÂˆ™\Ü˜ÛÛ\\™Y
ÏHNÂˆ]Z\œÎˆ•™YSX\	œİ‹	•˜[YOˆBˆKš]\Š
K›X\

ËŠ_
Ë˜\×ÜİŠ
KŠJK˜ÛÛXİ

NÂˆ›Üˆ
šY[
H[ˆÂˆËÈHXYˆHİ\ˆÚYHÙ\È›İ]™H\È[ˆ[[Y[ÙˆBˆËÈÛ™Ù\ˆ\İÚXÚ]È›[˜š[™[™È[™XYHØ^\Ë‚ˆ]ÛÛYJJHHZ\œË™Ù]
šY[˜\×ÜİŠ
JH[ÙHÂˆÛÛ[YNÂˆNÂˆËÈHšY[Hš[HÙ\È›İİÜ™H\È›İH˜[YHÈÛÛ\\™K‚ˆYˆX]Ú\ÈJ˜[YNXœÙ[
HX]Ú\ÈJK˜[YNXœÙ[
HÂˆ][™Ú[™HHYˆX]Ú\ÈJ˜[YNXœÙ[
HÂˆK™[™Ú[™BˆH[ÙHÂˆ‹™[™Ú[™BˆNÂˆ™\Ü™š[™[™ÜËœ\Ú
š[™[™ÈÂˆÚ[™ˆš[™[™ÒÚ[™•˜[YKˆÙ^Nˆ›Ü›X]JÛ˜[Y_KÙšY[HŠKˆ]Z[ˆ›Ü›X]J›Yİ]ˆHš[HÙ[™Ú[™_H™XYÙ\È›İİÜ™H]ŠKˆ\ÜÜÚ][Ûˆ\ÜÜÚ][Û‘^ÛYYˆ\ÜİYNˆİš[™Î›™]Ê
Kˆ^ÛYYˆÛÛYJ[™Ú[™K×Üİš[™Ê
JKˆJNÂˆÛÛ[YNÂˆBˆËÈHšY[[ˆ[™Ú[™HØ[››İ›ÙXÙH\ÈÜš][ˆ\È]È™\›ÂˆËÈ˜[YH\™H[™Yİ]ÙˆHÛÛ\\š\ÛÛ‹[™BˆËÈ™\ÜÛİ[È]‚ˆ]XYˆH›Ü›X]JÛ˜[Y_KÙšY[HŠNÂˆ]YÛİ]HØK—Kš[×Ú]\Š
K™š[™ÛX\
ÚY_Âˆ[\Âˆ™^ÛYYÙ›ÜŠÚYK™[™Ú[™KÚYK™š^\™K	›XYŠBˆ›X\
_
ÚYK™[™Ú[™KKš\ÜİYK˜ÛÛ™J
JJBˆJNÂˆYˆ]ÛÛYJ
[™Ú[™K\ÜİYJJHHYÛİ]Âˆ™\Ü™š[™[™ÜËœ\Ú
š[™[™ÈÂˆÚ[™ˆš[™[™ÒÚ[™•˜[YKˆÙ^NˆXY‹ˆ]Z[ˆ›Ü›X]J›Yİ]ˆÙ[™Ú[™_HØ[››İ›ÙXÙH]ŠKˆ\ÜÜÚ][Ûˆ\ÜÜÚ][Û‘^ÛYYˆ\ÜİYKˆ^ÛYYˆÛÛYJ[™Ú[™K×Üİš[™Ê
JKˆJNÂˆÛÛ[YNÂˆBˆYˆ]ÛÛYJ
Ú[™]Z[
JHHÛÛ\\™Wİ˜[Y\ÊJHÂˆ\Ú
	›]]™\ÜÚ[™›Ü›X]JÛ˜[Y_KÙšY[HŠK]Z[
NÂˆBˆBˆBˆ
ÛÛYJ
K›Û™JHOˆÂˆ]^ÛYYH[\Ë™^ÛYYÙ›ÜŠ‹™[™Ú[™K‹™š^\™K˜[YJNÂˆ™\Ü™š[™[™ÜËœ\Ú
š[™[™ÈÂˆÚ[™ˆš[™[™ÒÚ[™“Û›R[KˆÙ^Nˆ˜[YK˜ÛÛ™J
Kˆ]Z[ˆ›Ü›X]JÛ˜[Y_HHßHÛˆßK›İÛˆßH‹œÚİÊ
KK™[™Ú[™K‹™[™Ú[™JKˆ\ÜÜÚ][ÛˆYˆ^ÛYYš\×ÜÛÛYJ
HÂˆ\ÜÜÚ][Û‘^ÛYYˆH[ÙHÂˆ\ÜÜÚ][Û”™YˆKˆ\ÜİYNˆ^ÛYY›X\
_Kš\ÜİYK˜ÛÛ™J
JK[Ü˜\ÛÜ—ÙY˜][

Kˆ^ÛYYˆ^ÛYY›X\
ß‹™[™Ú[™K×Üİš[™Ê
JKˆJNÂˆBˆ
›Û™KÛÛYJJJHOˆÂˆ]^ÛYYH[\Ë™^ÛYYÙ›ÜŠK™[™Ú[™KK™š^\™K˜[YJNÂˆ™\Ü™š[™[™ÜËœ\Ú
š[™[™ÈÂˆÚ[™ˆš[™[™ÒÚ[™“Û›R[‹ˆÙ^Nˆ˜[YK˜ÛÛ™J
Kˆ]Z[ˆ›Ü›X]JÛ˜[Y_HHßHÛˆßK›İÛˆßH‹KœÚİÊ
K‹™[™Ú[™KK™[™Ú[™JKˆ\ÜÜÚ][ÛˆYˆ^ÛYYš\×ÜÛÛYJ
HÂˆ\ÜÜÚ][Û‘^ÛYYˆH[ÙHÂˆ\ÜÜÚ][Û”™YˆKˆ\ÜİYNˆ^ÛYY›X\
_Kš\ÜİYK˜ÛÛ™J
JK[Ü˜\ÛÜ—ÙY˜][

Kˆ^ÛYYˆ^ÛYY›X\
ßK™[™Ú[™K×Üİš[™Ê
JKˆJNÂˆBˆÈOˆ[œ™XXÚX›HJ›Û™HÙ^H\ÈÛ™HÚ[™Ùˆ™XÛÜ™ŠKˆBˆBˆ™\ÜŸB‚ˆÖØÙ™Ê\İ
WB›[Ù\İÈÂˆ\ÙHİ\\ŠÂ‚ˆ›ˆ
˜[YNˆ	œİ‹ˆ˜[YJHOˆ™XÛÜ™Âˆ™XÛÜ™’XY\ŠXY\ˆÂˆ˜[YNˆ˜[YKš[Ê
Kˆ˜[YNˆ‹ˆJBˆB‚ˆÖİ\İBˆ›ˆWÜİ™X[WÜ›İ[™İš\×Ø[™Ú\×ÜÛÜYØWÛ˜[YJ
HÂˆ]™XÜÈH™XÈVÂˆ
‰ĞĞSH‹˜[YN‘İX›JKŒ
JKˆ
‰VRSˆ‹˜[YN”Ú[ÊÌŒLKK‹ŒJJKˆ
‰ÓVQTˆ‹˜[YN”İŠŒ‹š[Ê
JJKˆ
‰Ô•ÓSÑH‹˜[YN›ÛÛ
˜[ÙJJKˆ
‰ÔÓSÑH‹˜[YN’[
LÊJKˆ
‰SSRSˆ‹˜[YN”Ú[ŠÌŒŒJJKˆNÂˆ]^H™[™\—Üİ™X[J	œ™XÜÊK[Ü˜\

NÂˆ]˜XÚÈH\œÙWÜİ™X[J	^
K[Ü˜\

NÂˆ]˜[Y\Îˆ™XÏ	œİˆH˜XÚËš]\Š
K›X\
™XÛÜ™šÙ^JK˜ÛÛXİ

NÂˆ\ÜÙ\Ù\HJˆ˜[Y\ËˆÂˆ‰ÓVQTˆ‹ˆ‰VRSˆ‹ˆ‰SSRSˆ‹ˆ‰ĞĞSH‹ˆ‰Ô•ÓSÑH‹ˆ‰ÔÓSÑH‚ˆBˆ
NÂˆ\ÜÙ\Ù\HJ™[™\—Üİ™X[J	˜˜XÚÊK[Ü˜\

K^
NÂˆ\ÜÙ\J^œİ\×İÚ]
×›[Ù[ŒKœ™XÛÜ™—šXY\—‹›˜[YW—‰ÓVQT—ˆŠJNÂˆB‚ˆÖİ\İBˆ›ˆWÜ\œÙ\—Ú\×ÜİšXİ

HÂˆ]ÚÈBˆ×›[Ù[ŒKœ™XÛÜ™—šXY\—‹›˜[YW—‰W‹\W—š[‹˜[YWŒ_WˆÂˆ\ÜÙ\J\œÙWÜİ™X[JÚÊKš\×ÛÚÊ
JNÂˆËÈHİX›HÜ[YÚ]İ]]ÈÚ[ˆ\ÜÙ\J\œÙWÜİ™X[J	›ÚËœ™\XÙJš[‹™İX›HŠJKš\×Ù\œŠ
JNÂˆËÈÜ›Û™È\H›ÜˆHÚ\Bˆ\ÜÙ\J\œÙWÜİ™X[J	›ÚËœ™\XÙJ—˜[YWŒH‹—˜[YW—ˆŠJKš\×Ù\œŠ
JNÂˆËÈ›İ[Ù[Bˆ\ÜÙ\J\œÙWÜİ™X[J	›ÚËœ™\XÙJ—›[Ù[ŒH‹—›[Ù[ŒˆŠJKš\×Ù\œŠ
JNÂˆËÈHØÚ[XHH™XÛÜ™ˆ\ÜÙ\J\œÙWÜİ™X[J×œØÚ[XWŒKœ™XÛÜ™—›[™WŸWˆŠKš\×Ù\œŠ
JNÂˆËÈ›Èš[˜[™]Û[™Bˆ\ÜÙ\J\œÙWÜİ™X[JÚËš[WÙ[™

JKš\×Ù\œŠ
JNÂˆËÈİ]ÙˆÜ™\‚ˆ]ÛÈH›Ü›X]Jß^ßH‹ÚËœ™\XÙJ‰H‹‰ˆŠKÚÊNÂˆ\ÜÙ\J\œÙWÜİ™X[J	ÛÊKš\×Ù\œŠ
JNÂˆËÈH\XØ]HÙ^Bˆ\ÜÙ\J\œÙWÜİ™X[J	™›Ü›X]JÛÚß^ÛÚßHŠJKš\×Ù\œŠ
JNÂˆB‚ˆÖİ\İBˆ›ˆWİ\WÙ›ÛİÜ×İWÙÜ›İ\ØÛÙJ
HÂˆ\ÜÙ\Ù\HJXY\—İ\J‰‹	–ÍÌJK[Ü˜\

Kš[ŠNÂˆ\ÜÙ\Ù\HJXY\—İ\J‰‹	–ÍJK[Ü˜\

K™İX›HŠNÂˆ\ÜÙ\Ù\HJXY\—İ\J‰‹	–ÌWJK[Ü˜\

Kœİš[™ÈŠNÂˆ\ÜÙ\Ù\HJXY\—İ\J‰‹	–ÌLJK[Ü˜\

K˜›ÛÛŠNÂˆ\ÜÙ\Ù\HJXY\—İ\J‰‹	–ÌJK[Ü˜\

Kš[ŠNÂˆ\ÜÙ\Ù\HJXY\—İ\J‰‹	–ÌLŒJK[Ü˜\

KœÚ[ˆŠNÂˆ\ÜÙ\Ù\HJXY\—İ\J‰‹	–ÌLŒÌJK[Ü˜\

KœÚ[ÈŠNÂˆ\ÜÙ\JXY\—İ\J‰‹	–ÌÍ×JKš\×Ù\œŠ
JNÂˆ\ÜÙ\J\×Ú[™WØÛÙJÍÊJNÂˆB‚ˆ›ˆÚYO	ØOŠ[™Ú[™Nˆ	‰ØHİ‹™XÛÜ™Îˆ	‰ØHÔ™XÛÜ™JHOˆÚYO	ØOˆÂˆÚYHÂˆ[™Ú[™Kˆš^\™Nˆ™ˆ‹ˆ™XÛÜ™ËˆBˆB‚ˆÖİ\İBˆ›ˆ˜[Y\×ØÛÛ\\™WØWÚÙ^WÛ›İØWÜÜÚ][ÛŠ
HÂˆ]HH™XÈVÚ
‰H‹˜[YN’[
JJK
‰ˆ‹˜[YN’[
ŠJWNÂˆ]ˆH™XÈVÚ
‰ˆ‹˜[YN’[
ŠJK
‰H‹˜[YN’[
JJWNÂˆ]ˆHÛÛ\\™Jˆ›Ü˜XÛKØXØY\İ‹ˆ	œÚYJ›Ü˜XÛH‹	˜JKˆ	œÚYJ˜XØY\İ‹	˜ŠKˆ	”[\Î™Y˜][

Kˆ
NÂˆ\ÜÙ\Ù\HJ‹™\™Xİ

K™\™Xİ“X]Ú
NÂˆ\ÜÙ\Ù\HJ‹˜ÛÛ\\™YŠNÂˆB‚ˆÖİ\İBˆ›ˆWÙİX›WÚ[œÚYWİWØ›İ[™ÛX]Ú\×Ø[™Ûİ]ÚYWÚ]Ú\×ÛÛ™WÙY™Š
HÂˆ]HH™XÈVÚ
‰ĞĞSH‹˜[YN‘İX›JKŒ
JWNÂˆ]™X\ˆH™XÈVÚ
‰ĞĞSH‹˜[YN‘İX›JKŒ
ÈYKN
JWNÂˆ]˜\ˆH™XÈVÚ
‰ĞĞSH‹˜[YN‘İX›JKŒJJWNÂˆ][\ÈH[\Î™Y˜][

NÂˆ]HHÛÛ\\™Jˆ›Ü˜XÛKØXØY\İ‹ˆ	œÚYJ›Ü˜XÛH‹	˜JKˆ	œÚYJ˜XØY\İ‹	›™X\ŠKˆ	œ[\Ëˆ
NÂˆ\ÜÙ\Ù\HJK™\™Xİ

K™\™Xİ“X]Ú
NÂˆ]HÛÛ\\™Jˆ›Ü˜XÛKØXØY\İ‹ˆ	œÚYJ›Ü˜XÛH‹	˜JKˆ	œÚYJ˜XØY\İ‹	™˜\ŠKˆ	œ[\Ëˆ
NÂˆ\ÜÙ\Ù\HJ™\™Xİ

K™\™Xİ‘Y™ŠJJNÂˆ\ÜÙ\J™š\œİ

K˜ÛÛZ[œÊ‰ĞĞSHŠJNÂˆB‚ˆÖİ\İBˆ›ˆWÛ˜[YWÛÛ—ÛÛ™WÜÚYWÚ\×Ü™Yİ[›\Ü×İWÜ[\×Ù^ÛYWÚ]Ù›Ü—İ]Ù[™Ú[™J
HÂˆ]HH™XÈVÚ
‰H‹˜[YN’[
JJK
‰ˆ‹˜[YN’[
ŠJWNÂˆ]ˆH™XÈVÚ
‰H‹˜[YN’[
JJWNÂˆ]›Û™HH[\Î™Y˜][

NÂˆ]ˆHÛÛ\\™Jˆ›Ü˜XÛKØXØY\İ‹ˆ	œÚYJ›Ü˜XÛH‹	˜JKˆ	œÚYJ˜XØY\İ‹	˜ŠKˆ	››Û™Kˆ
NÂˆ\ÜÙ\Ù\HJ‹™\™Xİ

K™\™Xİ‘Y™ŠJJNÂˆ][\ÈH[\Îœ\œÙJˆ–ÖÙ^ÛYYWW™[™Ú[™HH˜XØY\İ—›˜[Y\ÈH×‰——Wœ™X\ÛÛˆH››ÈšY[—š\ÜİYHHÌW—ˆ‹ˆ
Bˆ[Ü˜\

NÂˆ]ˆHÛÛ\\™Jˆ›Ü˜XÛKØXØY\İ‹ˆ	œÚYJ›Ü˜XÛH‹	˜JKˆ	œÚYJ˜XØY\İ‹	˜ŠKˆ	œ[\Ëˆ
NÂˆ\ÜÙ\Ù\HJ‹™\™Xİ

K™\™Xİ“X]Ú
NÂˆ\ÜÙ\Ù\HJ‹™š[™[™ÜË›[Š
KJNÂˆËÈ[™HØ[YH›İÈÙ\È›İ^İ\ÙHHÜ˜XÛHXÚÚ[™ÈH˜[YHXØY\İ\Âˆ]ˆHÛÛ\\™Jˆ›Ü˜XÛKØXØY\İ‹ˆ	œÚYJ›Ü˜XÛH‹	˜ŠKˆ	œÚYJ˜XØY\İ‹	˜JKˆ	œ[\Ëˆ
NÂˆ\ÜÙ\Ù\HJ‹™\™Xİ

K™\™Xİ‘Y™ŠJJNÂˆB‚ˆÖİ\İBˆ›ˆWÚÛ›İÛ—Ü›İ×Ø\Y\×İ×Ú]×ÜZ\œ×ÛÛ›J
HÂˆ]HH™XÈVÚ
‰Ô‘PUH‹˜[YN‘İX›JKŒ
JWNÂˆ]ˆH™XÈVÚ
‰Ô‘PUH‹˜[YN‘İX›J‹Œ
JWNÂˆ][\ÈH[\Îœ\œÙJˆ–ÖÚÛ›İÛ—WW›˜[Y\ÈH×‰Ô‘PUW—WœZ\œÈH×™^™‹ÛÜ˜XÛW—Wœ™X\ÛÛˆHœØ]™Y][›İ\ˆ[ÛY[—š\ÜİYHHÌW—ˆ‹ˆ
Bˆ[Ü˜\

NÂˆ]ˆHÛÛ\\™Jˆ™^™‹ÛÜ˜XÛH‹ˆ	œÚYJ™^™ˆ‹	˜JKˆ	œÚYJ›Ü˜XÛH‹	˜ŠKˆ	œ[\Ëˆ
NÂˆ\ÜÙ\Ù\HJ‹™\™Xİ

K™\™Xİ’Û›İÛŠJJNÂˆ]ˆHÛÛ\\™Jˆ›Ü˜XÛKØXØY\İ‹ˆ	œÚYJ›Ü˜XÛH‹	˜JKˆ	œÚYJ˜XØY\İ‹	˜ŠKˆ	œ[\Ëˆ
NÂˆ\ÜÙ\Ù\HJ‹™\™Xİ

K™\™Xİ‘Y™ŠJJNÂˆB‚ˆÖİ\İBˆ›ˆWÜ[WİÚ]İ]ØWÜ™X\ÛÛ—ÛÜ—Ø[—Ú\ÜİYWÚ\×Ü™Y\ÙY

HÂˆ\ÜÙ\Jˆ[\Îœ\œÙJ–ÖÚÛ›İÛ—WW›˜[Y\ÈH×‰W—Wœ™X\ÛÛˆH——š\ÜİYHH—ˆŠKš\×Ù\œŠ
Bˆ
NÂˆ\ÜÙ\J[\Îœ\œÙJ–ÖÙ^ÛYYWW™[™Ú[™HH›Ü˜XÛW—›˜[Y\ÈH×‰W—Wœ™X\ÛÛˆHœ——š\ÜİYHH——ˆŠKš\×Ù\œŠ
JNÂˆ\ÜÙ\J[\Îœ\œÙJ–ÖÙ^ÛYYWW™[™Ú[™HH››Ø›ÙW—›˜[Y\ÈH×‰W—Wœ™X\ÛÛˆHœ——š\ÜİYHHšW—ˆŠKš\×Ù\œŠ
JNÂˆB‚ˆ›ˆ^Y\Š˜[YNˆ	œİ‹ÛÛÜˆM
HOˆ™XÛÜ™Âˆ™XÛÜ™‘[J[N›™]Êˆ›^Y\ˆ‹ˆ˜[YKˆ™XÈVÂˆ˜[YN’[

Kˆ˜[YN’[
ÛÛÜŠKˆ˜[YN’[
LJKˆ˜[YN›ÛÛ
YJKˆ˜[YN”İŠÛÛ[[İ\È‹š[Ê
JKˆ˜[YN’[
LÊKˆ˜[YN›ÛÛ
YJKˆKˆ
JBˆB‚ˆÖİ\İBˆ›ˆWÛ^Y\—Ü›İ[™İš\×Ø[™ÜÛÜ×ØY\—İWÚXY\Š
HÂˆ]™XÜÈH™XÈVÂˆ^Y\Š•Ø[È‹JKˆ
‰ĞĞSH‹˜[YN‘İX›JKŒ
JKˆ^Y\ŠŒ‹ÊKˆNÂˆ]^H™[™\—Üİ™X[J	œ™XÜÊK[Ü˜\

NÂˆ]˜XÚÈH\œÙWÜİ™X[J	^
K[Ü˜\

NÂˆ]Ù^\Îˆ™XÏ
	œİ‹	œİŠOˆH˜XÚËš]\Š
K›X\
Ÿ
‹šÚ[™

K‹šÙ^J
JJK˜ÛÛXİ

NÂˆ\ÜÙ\Ù\HJˆÙ^\ËˆÊšXY\ˆ‹‰ĞĞSHŠK
›^Y\ˆ‹ŒŠK
›^Y\ˆ‹•Ø[ÈŠWBˆ
NÂˆ\ÜÙ\Ù\HJ™[™\—Üİ™X[J	˜˜XÚÊK[Ü˜\

K^
NÂˆ\ÜÙ\J^˜ÛÛZ[œÊˆ×›[Ù[ŒKœ™XÛÜ™—›^Y\—‹›˜[YW—Œ‹™›YÜ×Œ˜ÛÛÜ—ËYWØÛÛÜ—‹LK›Û—YK›[™]\W—ÛÛ[[İ\×‹›[™]ÙZYÚ‹LËœİY_H‚ˆ
JNÂˆB‚ˆÖİ\İBˆ›ˆWÛ^Y\—Û[™WİÚ]ØWÙšY[ÛZ\ÜÚ[™×ÛÜ—Ûİ]ÛÙ—ÛÜ™\—Ú\×Ü™Y\ÙY

HÂˆ]ÚÈH™[™\—Üİ™X[J	–Û^Y\ŠŒ‹ÊWJK[Ü˜\

NÂˆ\ÜÙ\J\œÙWÜİ™X[J	›ÚÊKš\×ÛÚÊ
JNÂˆ\ÜÙ\J\œÙWÜİ™X[J	›ÚËœ™\XÙJ—œİYH‹—œİŒHŠJKš\×Ù\œŠ
JNÂˆ\ÜÙ\J\œÙWÜİ™X[J	›ÚËœ™\XÙJ‹œİYH‹ˆŠJKš\×Ù\œŠ
JNÂˆ]İØ\YHÚËœ™\XÙJ—™›YÜ×Œ˜ÛÛÜ—È‹—˜ÛÛÜ—Ë™›YÜ×ŒŠNÂˆ\ÜÙ\J\œÙWÜİ™X[J	œİØ\Y
Kš\×Ù\œŠ
JNÂˆËÈH^Y\ˆÛÜY™Y›Ü™HHXY\‚ˆ]ÛÈH›Ü›X]JÛÚß^ßH‹™[™\—Üİ™X[J	–Ú
‰H‹˜[YN’[
JJWJK[Ü˜\

JNÂˆ\ÜÙ\J\œÙWÜİ™X[J	ÛÊKš\×Ù\œŠ
JNÂˆB‚ˆÖİ\İBˆ›ˆWØÚ[™ÙYÛ^Y\—ØÛÛİ\—Ú\×ÛÛ™WÙš[™[™×Û˜[Z[™×İWÛ^Y\—Ø[™İWÙšY[

HÂˆ]HH™XÈVÛ^Y\ŠŒ‹ÊK^Y\Š•Ø[È‹JWNÂˆ]ˆH™XÈVÛ^Y\ŠŒ‹ÊK^Y\Š•Ø[È‹ŠWNÂˆ]ˆHÛÛ\\™Jˆ›Ü˜XÛKØXØY\İ‹ˆ	œÚYJ›Ü˜XÛH‹	˜JKˆ	œÚYJ˜XØY\İ‹	˜ŠKˆ	”[\Î™Y˜][

Kˆ
NÂˆ\ÜÙ\Ù\HJ‹™\™Xİ

K™\™Xİ‘Y™ŠJJNÂˆ\ÜÙ\Ù\HJ‹œ™Y

VÌKšÙ^K›^Y\‹ÕØ[Ë˜ÛÛÜˆŠNÂˆ\ÜÙ\Ù\HJ‹˜ÛÛ\\™YŠNÂˆB‚ˆÖİ\İBˆ›ˆWÛ^Y\—ÛÛ—ÛÛ™WÜÚYWÚ\×Ü™YØ[™ØWÜ[WØØ[—Û˜[YWÛ^Y\œ×İÚ]ØWÜİ\Š
HÂˆ]HH™XÈVÛ^Y\ŠŒ‹ÊK^Y\Š•Ø[È‹JWNÂˆ]ˆH™XÈVÛ^Y\ŠŒ‹ÊWNÂˆ]ˆHÛÛ\\™Jˆ›Ü˜XÛKØXØY\İ‹ˆ	œÚYJ›Ü˜XÛH‹	˜JKˆ	œÚYJ˜XØY\İ‹	˜ŠKˆ	”[\Î™Y˜][

Kˆ
NÂˆ\ÜÙ\Ù\HJ‹™\™Xİ

K™\™Xİ‘Y™ŠJJNÂˆ\ÜÙ\Ù\HJ‹œ™Y

VÌKšÙ^K›^Y\‹ÕØ[ÈŠNÂˆ][\ÈH[\Îœ\œÙJˆ–ÖÙ^ÛYYWW™[™Ú[™HH˜XØY\İ—›˜[Y\ÈH×›^Y\‹ÕÊ——Wœ™X\ÛÛˆHœ——š\ÜİYHHšW—ˆ‹ˆ
Bˆ[Ü˜\

NÂˆ]ˆHÛÛ\\™Jˆ›Ü˜XÛKØXØY\İ‹ˆ	œÚYJ›Ü˜XÛH‹	˜JKˆ	œÚYJ˜XØY\İ‹	˜ŠKˆ	œ[\Ëˆ
NÂˆ\ÜÙ\Ù\HJ‹™\™Xİ

K™\™Xİ“X]Ú
NÂˆB‚ˆÖİ\İBˆ›ˆWÜİ\—ÛX]Ú\×Ø[WÜ[—Ø[™Û›İ[™×Ù[ÙJ
HÂˆ\ÜÙ\JÛØŠ›^Y\‹Ê‹˜ÛÛÜˆ‹›^Y\‹ÕØ[Ë˜ÛÛÜˆŠJNÂˆ\ÜÙ\JÛØŠ›^Y\‹Ê‹˜ÛÛÜˆ‹›^Y\‹Ë˜ÛÛÜˆŠJNÂˆ\ÜÙ\JYÛØŠ›^Y\‹Ê‹˜ÛÛÜˆ‹›^Y\‹ÕØ[Ë™›YÜÈŠJNÂˆ\ÜÙ\JÛØŠ‰H‹‰HŠJNÂˆ\ÜÙ\JYÛØŠ‰H‹‰PˆŠJNÂˆ\ÜÙ\JÛØŠŠˆ‹˜[][™ÈŠJNÂˆ\ÜÙ\JÛØŠ˜J˜Š˜È‹˜V–XÈŠJNÂˆ\ÜÙ\JYÛØŠ˜J˜Š˜È‹˜VÖXˆŠJNÂˆ