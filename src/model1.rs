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

/// Every kind with a record, in `docs/MODEL_1.md`. One `const` per kind above
/// and one name here, so a new kind never touches the others.
pub const SCHEMAS: &[Schema] = &[LAYER, LINETYPE, TEXTSTYLE];

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
        .ok_or_else(|| format!("no frozen {engine} model-1 stream for {id}"))?;
    let path = model1_dir().join(&f.path);
    let bytes = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let got: String = {
        use sha2::{Digest, Sha256};
        Sha256::digest(&bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect()
    };
    if got != f.sha256 {
        return Err(format!(
            "{}: hashes to {got}, the manifest froze {}",
            path.display(),
            f.sha256
        ));
    }
    let text = String::from_utf8(bytes).map_err(|e| e.to_string())?;
    parse_stream(&text).map_err(|e| format!("{}: {e}", path.display()))
}

/// The committed `parity/MODEL1.toml`.
pub fn committed_rules() -> Result<Rules, String> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("parity")
        .join("MODEL1.toml");
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    Rules::parse(&text)
}

// ---- comparing ----------------------------------------------------------------------

/// What kind of difference a finding is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FindingKind {
    OnlyInA,
    OnlyInB,
    Type,
    Value,
}

/// What a finding amounts to once the rules have been applied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Disposition {
    /// Nothing explains it: debt.
    Red,
    /// A listed difference by construction.
    Known,
    /// A name one engine cannot produce, listed.
    Excluded,
}

#[derive(Debug, Clone)]
pub struct Finding {
    pub kind: FindingKind,
    pub key: String,
    pub detail: String,
    pub disposition: Disposition,
    /// The issue of the row that lists it, when one does.
    pub issue: String,
    /// For an `Excluded` finding, the engine the row excludes.
    pub excluded: Option<String>,
}

impl fmt::Display for Finding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "header {} {} [{:?}]",
            self.key, self.detail, self.disposition
        )
    }
}

/// What one comparison came to.
#[derive(Debug, Clone, Default)]
pub struct Report {
    pub findings: Vec<Finding>,
    /// How many names were on both sides and compared.
    pub compared: usize,
}

impl Report {
    pub fn red(&self) -> Vec<&Finding> {
        self.findings
            .iter()
            .filter(|f| f.disposition == Disposition::Red)
            .collect()
    }

    pub fn verdict(&self) -> Verdict {
        let red = self.red().len();
        if red > 0 {
            return Verdict::Diff(red);
        }
        let known = self
            .findings
            .iter()
            .filter(|f| f.disposition == Disposition::Known)
            .count();
        if known > 0 {
            Verdict::Known(known)
        } else {
            Verdict::Match
        }
    }

    /// The issues of the rows that listed a difference, in order, once each.
    pub fn issues(&self) -> String {
        let mut seen: Vec<&str> = Vec::new();
        for f in self
            .findings
            .iter()
            .filter(|f| f.disposition == Disposition::Known)
        {
            if !f.issue.is_empty() && !seen.contains(&f.issue.as_str()) {
                seen.push(&f.issue);
            }
        }
        seen.join(", ")
    }

    /// The first finding that counts, rendered, for the verdict file.
    pub fn first(&self) -> String {
        self.red()
            .first()
            .map(|f| f.to_string())
            .or_else(|| {
                self.findings
                    .iter()
                    .find(|f| f.disposition == Disposition::Known)
                    .map(|f| f.to_string())
            })
            .unwrap_or_default()
    }
}

fn same(a: &Value, b: &Value) -> bool {
    let t = |x: f64, y: f64| BOUNDED.within(x, y);
    match (a, b) {
        (Value::Double(x), Value::Double(y)) => t(*x, *y),
        (Value::Point2(x), Value::Point2(y)) => x.iter().zip(y).all(|(p, q)| t(*p, *q)),
        (Value::Point3(x), Value::Point3(y)) => x.iter().zip(y).all(|(p, q)| t(*p, *q)),
        _ => a == b,
    }
}

/// One side of a comparison.
pub struct Side<'a> {
    pub engine: &'a str,
    pub fixture: &'a str,
    pub records: &'a [Record],
}

/// A record as the comparator sees it: one value (a header variable) or a list
/// of named scalar leaves (a table entry, with its lists unrolled).
enum Item {
    Scalar(Value),
    Fields(Vec<(String, Value)>),
}

impl Item {
    fn show(&self) -> String {
        match self {
            Self::Scalar(v) => v.show(),
            Self::Fields(f) => f
                .iter()
                .map(|(n, v)| format!("{n}={}", v.show()))
                .collect::<Vec<_>>()
                .join(" "),
        }
    }
}

/// The key a record is compared and listed under: the DXF name for a header
/// variable, `<kind>/<name>` for anything else.
fn item_key(r: &Record) -> String {
    match r {
        Record::Header(h) => h.name.clone(),
        Record::Entry(e) => format!("{}/{}", e.kind, e.name),
    }
}

/// Unroll a value into scalar leaves: a list is its length, `name.len`, and then
/// `name[i].field` for each element.
fn leaves(prefix: &str, ty: &Ty, v: &Value, out: &mut Vec<(String, Value)>) {
    match (ty, v) {
        (Ty::List(fields), Value::List(items)) => {
            out.push((format!("{prefix}.len"), Value::Int(items.len() as i64)));
            for (i, item) in items.iter().enumerate() {
                for ((name, fty), fv) in fields.iter().zip(item) {
                    leaves(&format!("{prefix}[{i}].{name}"), fty, fv, out);
                }
            }
        }
        _ => out.push((prefix.to_string(), v.clone())),
    }
}

fn item_of(r: &Record) -> Item {
    match r {
        Record::Header(h) => Item::Scalar(h.value.clone()),
        Record::Entry(e) => {
            let mut out = Vec::new();
            if let Some(s) = schema(e.kind) {
                for ((name, ty), v) in s.fields.iter().zip(&e.values) {
                    leaves(name, ty, v, &mut out);
                }
            }
            Item::Fields(out)
        }
    }
}

/// Does `pattern` (which may hold `*`, standing for any run of characters) match
/// `text`? A layer name is data, so a rule about every layer's colour is
/// `layer/*.color`.
pub fn glob(pattern: &str, text: &str) -> bool {
    let parts: Vec<&str> = pattern.split('*').collect();
    if parts.len() == 1 {
        return pattern == text;
    }
    let mut rest = text;
    for (i, part) in parts.iter().enumerate() {
        if i == 0 {
            match rest.strip_prefix(part) {
                Some(r) => rest = r,
                None => return false,
            }
        } else if i == parts.len() - 1 {
            return rest.ends_with(part);
        } else {
            match rest.find(part) {
                Some(at) => rest = &rest[at + part.len()..],
                None => return false,
            }
        }
    }
    true
}

/// Compare two streams by key. `pair` is `oracle/acadrust` and so on, the name
/// `parity/MODEL1.toml` rows use.
pub fn compare(pair: &str, a: &Side, b: &Side, rules: &Rules) -> Report {
    let index = |s: &Side| -> BTreeMap<String, Item> {
        s.records
            .iter()
            .map(|r| (item_key(r), item_of(r)))
            .collect()
    };
    let (ia, ib) = (index(a), index(b));
    let names: BTreeSet<&String> = ia.keys().chain(ib.keys()).collect();
    let mut report = Report::default();
    let push = |report: &mut Report, kind: FindingKind, key: String, detail: String| {
        let known = rules.known_for(pair, a.fixture, &key);
        report.findings.push(Finding {
            kind,
            key,
            detail,
            disposition: if known.is_some() {
                Disposition::Known
            } else {
                Disposition::Red
            },
            issue: known.map(|k| k.issue.clone()).unwrap_or_default(),
            excluded: None,
        });
    };
    let compare_values = |x: &Value, y: &Value| -> Option<(FindingKind, String)> {
        if x.type_name() != y.type_name() {
            Some((
                FindingKind::Type,
                format!("type {} vs {}", x.type_name(), y.type_name()),
            ))
        } else if !same(x, y) {
            Some((FindingKind::Value, format!("{} vs {}", x.show(), y.show())))
        } else {
            None
        }
    };
    for name in names {
        match (ia.get(name), ib.get(name)) {
            (Some(Item::Scalar(x)), Some(Item::Scalar(y))) => {
                report.compared += 1;
                if let Some((kind, detail)) = compare_values(x, y) {
                    push(&mut report, kind, name.clone(), detail);
                }
            }
            (Some(Item::Fields(x)), Some(Item::Fields(y))) => {
                report.compared += 1;
                let theirs: BTreeMap<&str, &Value> =
                    y.iter().map(|(k, v)| (k.as_str(), v)).collect();
                for (field, vx) in x {
                    // A leaf the other side does not have is an element of a
                    // longer list, which its `.len` finding already says.
                    let Some(vy) = theirs.get(field.as_str()) else {
                        continue;
                    };
                    // A field an engine cannot produce is written as its zero
                    // value there and left out of the comparison, and the
                    // report counts it.
                    let leaf = format!("{name}.{field}");
                    let left_out = [a, b].into_iter().find_map(|side| {
                        rules
                            .excluded_for(side.engine, side.fixture, &leaf)
                            .map(|e| (side.engine, e.issue.clone()))
                    });
                    if let Some((engine, issue)) = left_out {
                        report.findings.push(Finding {
                            kind: FindingKind::Value,
                            key: leaf,
                            detail: format!("left out: {engine} cannot produce it"),
                            disposition: Disposition::Excluded,
                            issue,
                            excluded: Some(engine.to_string()),
                        });
                        continue;
                    }
                    if let Some((kind, detail)) = compare_values(vx, vy) {
                        push(&mut report, kind, format!("{name}.{field}"), detail);
                    }
                }
            }
            (Some(x), None) => {
                let excluded = rules.excluded_for(b.engine, b.fixture, name);
                report.findings.push(Finding {
                    kind: FindingKind::OnlyInA,
                    key: name.clone(),
                    detail: format!("{name} = {} on {}, not on {}", x.show(), a.engine, b.engine),
                    disposition: if excluded.is_some() {
                        Disposition::Excluded
                    } else {
                        Disposition::Red
                    },
                    issue: excluded.map(|e| e.issue.clone()).unwrap_or_default(),
                    excluded: excluded.map(|_| b.engine.to_string()),
                });
            }
            (None, Some(y)) => {
                let excluded = rules.excluded_for(a.engine, a.fixture, name);
                report.findings.push(Finding {
                    kind: FindingKind::OnlyInB,
                    key: name.clone(),
                    detail: format!("{name} = {} on {}, not on {}", y.show(), b.engine, a.engine),
                    disposition: if excluded.is_some() {
                        Disposition::Excluded
                    } else {
                        Disposition::Red
                    },
                    issue: excluded.map(|e| e.issue.clone()).unwrap_or_default(),
                    excluded: excluded.map(|_| a.engine.to_string()),
                });
            }
            _ => unreachable!("one key is one kind of record"),
        }
    }
    report
}

#[cfg(test)]
mod tests {
    use super::*;

    fn h(name: &str, v: Value) -> Record {
        Record::Header(Header {
            name: name.into(),
            value: v,
        })
    }

    #[test]
    fn a_stream_round_trips_and_is_sorted_by_name() {
        let recs = vec![
            h("$LTSCALE", Value::Double(1.0)),
            h("$EXTMIN", Value::Point3([0.0, -1.5, 2.0])),
            h("$CLAYER", Value::Str("0".into())),
            h("$ORTHOMODE", Value::Bool(false)),
            h("$OSMODE", Value::Int(-3)),
            h("$LIMMIN", Value::Point2([0.0, 0.0])),
        ];
        let text = render_stream(&recs).unwrap();
        let back = parse_stream(&text).unwrap();
        let names: Vec<&str> = back.iter().map(Record::key).collect();
        assert_eq!(
            names,
            [
                "$CLAYER",
                "$EXTMIN",
                "$LIMMIN",
                "$LTSCALE",
                "$ORTHOMODE",
                "$OSMODE"
            ]
        );
        assert_eq!(render_stream(&back).unwrap(), text);
        assert!(text.starts_with("{\"model\":1,\"record\":\"header\",\"name\":\"$CLAYER\""));
    }

    #[test]
    fn the_parser_is_strict() {
        let ok =
            "{\"model\":1,\"record\":\"header\",\"name\":\"$A\",\"type\":\"int\",\"value\":1}\n";
        assert!(parse_stream(ok).is_ok());
        // a double spelled without its point
        assert!(parse_stream(&ok.replace("int", "double")).is_err());
        // wrong type for the shape
        assert!(parse_stream(&ok.replace("\"value\":1", "\"value\":\"x\"")).is_err());
        // not model 1
        assert!(parse_stream(&ok.replace("\"model\":1", "\"model\":2")).is_err());
        // a schema 1 record
        assert!(parse_stream("{\"schema\":1,\"record\":\"line\"}\n").is_err());
        // no final newline
        assert!(parse_stream(ok.trim_end()).is_err());
        // out of order
        let two = format!("{}{}", ok.replace("$A", "$B"), ok);
        assert!(parse_stream(&two).is_err());
        // a duplicate key
        assert!(parse_stream(&format!("{ok}{ok}")).is_err());
    }

    #[test]
    fn the_type_follows_the_group_code() {
        assert_eq!(header_type("$X", &[70]).unwrap(), "int");
        assert_eq!(header_type("$X", &[40]).unwrap(), "double");
        assert_eq!(header_type("$X", &[1]).unwrap(), "string");
        assert_eq!(header_type("$X", &[290]).unwrap(), "bool");
        assert_eq!(header_type("$X", &[280]).unwrap(), "int");
        assert_eq!(header_type("$X", &[10, 20]).unwrap(), "point2");
        assert_eq!(header_type("$X", &[10, 20, 30]).unwrap(), "point3");
        assert!(header_type("$X", &[347]).is_err());
        assert!(is_handle_code(347));
    }

    fn side<'a>(engine: &'a str, records: &'a [Record]) -> Side<'a> {
        Side {
            engine,
            fixture: "f",
            records,
        }
    }

    #[test]
    fn names_compare_by_key_not_by_position() {
        let a = vec![h("$A", Value::Int(1)), h("$B", Value::Int(2))];
        let b = vec![h("$B", Value::Int(2)), h("$A", Value::Int(1))];
        let r = compare(
            "oracle/acadrust",
            &side("oracle", &a),
            &side("acadrust", &b),
            &Rules::default(),
        );
        assert_eq!(r.verdict(), Verdict::Match);
        assert_eq!(r.compared, 2);
    }

    #[test]
    fn a_double_inside_the_bound_matches_and_outside_it_is_one_diff() {
        let a = vec![h("$LTSCALE", Value::Double(1.0))];
        let near = vec![h("$LTSCALE", Value::Double(1.0 + 1e-8))];
        let far = vec![h("$LTSCALE", Value::Double(1.001))];
        let rules = Rules::default();
        let m = compare(
            "oracle/acadrust",
            &side("oracle", &a),
            &side("acadrust", &near),
            &rules,
        );
        assert_eq!(m.verdict(), Verdict::Match);
        let d = compare(
            "oracle/acadrust",
            &side("oracle", &a),
            &side("acadrust", &far),
            &rules,
        );
        assert_eq!(d.verdict(), Verdict::Diff(1));
        assert!(d.first().contains("$LTSCALE"));
    }

    #[test]
    fn a_name_on_one_side_is_red_unless_the_rules_exclude_it_for_that_engine() {
        let a = vec![h("$A", Value::Int(1)), h("$B", Value::Int(2))];
        let b = vec![h("$A", Value::Int(1))];
        let none = Rules::default();
        let r = compare(
            "oracle/acadrust",
            &side("oracle", &a),
            &side("acadrust", &b),
            &none,
        );
        assert_eq!(r.verdict(), Verdict::Diff(1));
        let rules = Rules::parse(
            "[[excluded]]\nengine = \"acadrust\"\nnames = [\"$B\"]\nreason = \"no field\"\nissue = \"x#1\"\n",
        )
        .unwrap();
        let r = compare(
            "oracle/acadrust",
            &side("oracle", &a),
            &side("acadrust", &b),
            &rules,
        );
        assert_eq!(r.verdict(), Verdict::Match);
        assert_eq!(r.findings.len(), 1);
        // and the same row does not excuse the oracle lacking a name acadrust has
        let r = compare(
            "oracle/acadrust",
            &side("oracle", &b),
            &side("acadrust", &a),
            &rules,
        );
        assert_eq!(r.verdict(), Verdict::Diff(1));
    }

    #[test]
    fn a_known_row_applies_to_its_pairs_only() {
        let a = vec![h("$TDCREATE", Value::Double(1.0))];
        let b = vec![h("$TDCREATE", Value::Double(2.0))];
        let rules = Rules::parse(
            "[[known]]\nnames = [\"$TDCREATE\"]\npairs = [\"ezdxf/oracle\"]\nreason = \"saved at another moment\"\nissue = \"x#1\"\n",
        )
        .unwrap();
        let r = compare(
            "ezdxf/oracle",
            &side("ezdxf", &a),
            &side("oracle", &b),
            &rules,
        );
        assert_eq!(r.verdict(), Verdict::Known(1));
        let r = compare(
            "oracle/acadrust",
            &side("oracle", &a),
            &side("acadrust", &b),
            &rules,
        );
        assert_eq!(r.verdict(), Verdict::Diff(1));
    }

    #[test]
    fn a_rule_without_a_reason_or_an_issue_is_refused() {
        assert!(
            Rules::parse("[[known]]\nnames = [\"$A\"]\nreason = \"\"\nissue = \"x\"\n").is_err()
        );
        assert!(Rules::parse("[[excluded]]\nengine = \"oracle\"\nnames = [\"$A\"]\nreason = \"r\"\nissue = \"\"\n").is_err());
        assert!(Rules::parse("[[excluded]]\nengine = \"nobody\"\nnames = [\"$A\"]\nreason = \"r\"\nissue = \"i\"\n").is_err());
    }

    fn layer(name: &str, color: i64) -> Record {
        Record::Entry(Entry::new(
            "layer",
            name,
            vec![
                Value::Int(0),
                Value::Int(color),
                Value::Int(-1),
                Value::Bool(true),
                Value::Str("Continuous".into()),
                Value::Int(-3),
                Value::Bool(true),
            ],
        ))
    }

    #[test]
    fn a_layer_round_trips_and_sorts_after_the_header() {
        let recs = vec![
            layer("Walls", 1),
            h("$LTSCALE", Value::Double(1.0)),
            layer("0", 7),
        ];
        let text = render_stream(&recs).unwrap();
        let back = parse_stream(&text).unwrap();
        let keys: Vec<(&str, &str)> = back.iter().map(|r| (r.kind(), r.key())).collect();
        assert_eq!(
            keys,
            [("header", "$LTSCALE"), ("layer", "0"), ("layer", "Walls")]
        );
        assert_eq!(render_stream(&back).unwrap(), text);
        assert!(text.contains(
            "{\"model\":1,\"record\":\"layer\",\"name\":\"0\",\"flags\":0,\"color\":7,\"true_color\":-1,\"on\":true,\"linetype\":\"Continuous\",\"lineweight\":-3,\"plot\":true}"
        ));
    }

    #[test]
    fn a_layer_line_with_a_field_missing_or_out_of_order_is_refused() {
        let ok = render_stream(&[layer("0", 7)]).unwrap();
        assert!(parse_stream(&ok).is_ok());
        assert!(parse_stream(&ok.replace("\"plot\":true", "\"plot\":1")).is_err());
        assert!(parse_stream(&ok.replace(",\"plot\":true", "")).is_err());
        let swapped = ok.replace("\"flags\":0,\"color\":7", "\"color\":7,\"flags\":0");
        assert!(parse_stream(&swapped).is_err());
        // a layer sorted before a header
        let two = format!("{ok}{}", render_stream(&[h("$A", Value::Int(1))]).unwrap());
        assert!(parse_stream(&two).is_err());
    }

    #[test]
    fn a_changed_layer_colour_is_one_finding_naming_the_layer_and_the_field() {
        let a = vec![layer("0", 7), layer("Walls", 1)];
        let b = vec![layer("0", 7), layer("Walls", 2)];
        let r = compare(
            "oracle/acadrust",
            &side("oracle", &a),
            &side("acadrust", &b),
            &Rules::default(),
        );
        assert_eq!(r.verdict(), Verdict::Diff(1));
        assert_eq!(r.red()[0].key, "layer/Walls.color");
        assert_eq!(r.compared, 2);
    }

    #[test]
    fn a_layer_on_one_side_is_red_and_a_rule_can_name_layers_with_a_star() {
        let a = vec![layer("0", 7), layer("Walls", 1)];
        let b = vec![layer("0", 7)];
        let r = compare(
            "oracle/acadrust",
            &side("oracle", &a),
            &side("acadrust", &b),
            &Rules::default(),
        );
        assert_eq!(r.verdict(), Verdict::Diff(1));
        assert_eq!(r.red()[0].key, "layer/Walls");
        let rules = Rules::parse(
            "[[excluded]]\nengine = \"acadrust\"\nnames = [\"layer/W*\"]\nreason = \"r\"\nissue = \"i\"\n",
        )
        .unwrap();
        let r = compare(
            "oracle/acadrust",
            &side("oracle", &a),
            &side("acadrust", &b),
            &rules,
        );
        assert_eq!(r.verdict(), Verdict::Match);
    }

    #[test]
    fn a_star_matches_any_run_and_nothing_else() {
        assert!(glob("layer/*.color", "layer/Walls.color"));
        assert!(glob("layer/*.color", "layer/.color"));
        assert!(!glob("layer/*.color", "layer/Walls.flags"));
        assert!(glob("$A", "$A"));
        assert!(!glob("$A", "$AB"));
        assert!(glob("*", "anything"));
        assert!(glob("a*b*c", "aXbYc"));
        assert!(!glob("a*b*c", "aXcYb"));
    }
}
