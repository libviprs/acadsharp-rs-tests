//! A strict reader for canonical schema 1 (`docs/CANONICAL_SCHEMA.md`).
//!
//! This is the one place the format is read. It parses text into typed
//! [`Record`]s and refuses everything the schema forbids, each with its own
//! [`ParseErrorKind`] that names the line and, where there is one, the field:
//! an unknown record type, an unknown or missing field, a field of the wrong
//! JSON type (`100` where `100.0` belongs, or the reverse), a carriage return
//! anywhere, `-0.0`, a number that is not finite, a `schema` other than 1, and
//! a `points` and `bulges` of unequal length.
//!
//! It checks shape and nothing else. Geometric invariants (`start` in
//! `[0, 2*pi)`, a unit normal) are the oracle's own tests, so a test can feed
//! this parser any finite perturbation of a real stream.
//!
//! It also never normalises. Whatever the schema forbids is refused here, so an
//! emitter that writes it goes red instead of being tidied on the way in.
//!
//! The sidecar (`<id>.<engine>.src.jsonl`) is read here too, by
//! [`Sidecar::parse`].

use std::collections::BTreeSet;
use std::fmt;

use serde_json::{Map, Number, Value};

/// The only schema this reader knows.
pub const SCHEMA: u64 = 1;

/// A point or vector.
pub type Vec3 = [f64; 3];

#[derive(Debug, Clone, PartialEq)]
pub struct Document {
    pub format: String,
    pub acad_version: String,
    pub maintenance_version: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct View {
    pub index: u64,
    pub name: String,
    pub kind: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Line {
    pub view: u64,
    pub a: Vec3,
    pub b: Vec3,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Polyline {
    pub view: u64,
    pub closed: bool,
    pub normal: Vec3,
    pub points: Vec<Vec3>,
    pub bulges: Vec<f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Circle {
    pub view: u64,
    pub center: Vec3,
    pub radius: f64,
    pub normal: Vec3,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Arc {
    pub view: u64,
    pub center: Vec3,
    pub radius: f64,
    pub start: f64,
    pub end: f64,
    pub normal: Vec3,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Ellipse {
    pub view: u64,
    pub center: Vec3,
    pub major: Vec3,
    pub ratio: f64,
    pub start: f64,
    pub end: f64,
    pub normal: Vec3,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Spline {
    pub view: u64,
    pub degree: u64,
    pub closed: bool,
    pub periodic: bool,
    pub control_points: Vec<Vec3>,
    pub knots: Vec<f64>,
    pub weights: Vec<f64>,
    pub fit_points: Vec<Vec3>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Polygon {
    pub view: u64,
    pub points: Vec<Vec3>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Point {
    pub view: u64,
    pub at: Vec3,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Text {
    pub view: u64,
    pub at: Vec3,
    pub height: f64,
    pub rotation: f64,
    pub width_factor: f64,
    pub halign: String,
    pub valign: String,
    pub style: String,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Warning {
    pub category: String,
    pub entity_type: String,
    pub detail: String,
}

/// One line of a canonical stream.
#[derive(Debug, Clone, PartialEq)]
pub enum Record {
    Document(Document),
    View(View),
    Line(Line),
    Polyline(Polyline),
    Circle(Circle),
    Arc(Arc),
    Ellipse(Ellipse),
    Spline(Spline),
    Polygon(Polygon),
    Point(Point),
    Text(Text),
    Warning(Warning),
}

impl Record {
    /// The schema's name for this record: the value of its `record` field.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Document(_) => "document",
            Self::View(_) => "view",
            Self::Line(_) => "line",
            Self::Polyline(_) => "polyline",
            Self::Circle(_) => "circle",
            Self::Arc(_) => "arc",
            Self::Ellipse(_) => "ellipse",
            Self::Spline(_) => "spline",
            Self::Polygon(_) => "polygon",
            Self::Point(_) => "point",
            Self::Text(_) => "text",
            Self::Warning(_) => "warning",
        }
    }

    /// The view an entity record belongs to. `None` for `document`, `view` and
    /// `warning`.
    pub fn view(&self) -> Option<u64> {
        match self {
            Self::Document(_) | Self::View(_) | Self::Warning(_) => None,
            Self::Line(r) => Some(r.view),
            Self::Polyline(r) => Some(r.view),
            Self::Circle(r) => Some(r.view),
            Self::Arc(r) => Some(r.view),
            Self::Ellipse(r) => Some(r.view),
            Self::Spline(r) => Some(r.view),
            Self::Polygon(r) => Some(r.view),
            Self::Point(r) => Some(r.view),
            Self::Text(r) => Some(r.view),
        }
    }

    /// Every float in the record, in schema order, mutably. The perturbation
    /// sweep walks this, and so does nothing else.
    pub fn floats_mut(&mut self) -> Vec<&mut f64> {
        fn v3<'a>(out: &mut Vec<&'a mut f64>, v: &'a mut Vec3) {
            out.extend(v.iter_mut());
        }
        let mut out: Vec<&mut f64> = Vec::new();
        match self {
            Self::Document(_) | Self::View(_) | Self::Warning(_) => {}
            Self::Line(r) => {
                v3(&mut out, &mut r.a);
                v3(&mut out, &mut r.b);
            }
            Self::Polyline(r) => {
                v3(&mut out, &mut r.normal);
                r.points.iter_mut().for_each(|p| v3(&mut out, p));
                out.extend(r.bulges.iter_mut());
            }
            Self::Circle(r) => {
                v3(&mut out, &mut r.center);
                out.push(&mut r.radius);
                v3(&mut out, &mut r.normal);
            }
            Self::Arc(r) => {
                v3(&mut out, &mut r.center);
                out.push(&mut r.radius);
                out.push(&mut r.start);
                out.push(&mut r.end);
                v3(&mut out, &mut r.normal);
            }
            Self::Ellipse(r) => {
                v3(&mut out, &mut r.center);
                v3(&mut out, &mut r.major);
                out.push(&mut r.ratio);
                out.push(&mut r.start);
                out.push(&mut r.end);
                v3(&mut out, &mut r.normal);
            }
            Self::Spline(r) => {
                r.control_points.iter_mut().for_each(|p| v3(&mut out, p));
                out.extend(r.knots.iter_mut());
                out.extend(r.weights.iter_mut());
                r.fit_points.iter_mut().for_each(|p| v3(&mut out, p));
            }
            Self::Polygon(r) => r.points.iter_mut().for_each(|p| v3(&mut out, p)),
            Self::Point(r) => v3(&mut out, &mut r.at),
            Self::Text(r) => {
                v3(&mut out, &mut r.at);
                out.push(&mut r.height);
                out.push(&mut r.rotation);
                out.push(&mut r.width_factor);
            }
        }
        out
    }
}

/// Why a line was refused. One variant per refusal, so a test can pin which.
#[derive(Debug, Clone, PartialEq)]
pub enum ParseErrorKind {
    /// Not JSON at all. Carries the underlying message.
    NotJson(String),
    NotAnObject,
    /// A blank line. The format has none.
    EmptyLine,
    MissingFinalNewline,
    CarriageReturn,
    /// `NaN`, an infinity, or a number too large for a double (`1e999`).
    NonFinite(String),
    /// `-0.0`. The schema normalises it away, so an emitter must have.
    NegativeZero,
    /// `schema` present but not 1. Carries what was found.
    SchemaVersion(String),
    UnknownRecord(String),
    UnknownField,
    MissingField,
    /// The JSON type is wrong for the field.
    WrongType {
        expected: &'static str,
        found: String,
    },
    /// `points` and `bulges` of a polyline differ in length.
    LengthMismatch {
        points: usize,
        bulges: usize,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParseError {
    /// 1-based.
    pub line: usize,
    pub field: Option<String>,
    pub kind: ParseErrorKind,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}", self.line)?;
        if let Some(field) = &self.field {
            write!(f, ", field `{field}`")?;
        }
        write!(f, ": ")?;
        match &self.kind {
            ParseErrorKind::NotJson(m) => write!(f, "not JSON ({m})"),
            ParseErrorKind::NotAnObject => write!(f, "a record is a JSON object"),
            ParseErrorKind::EmptyLine => write!(f, "blank line; the format has none"),
            ParseErrorKind::MissingFinalNewline => write!(f, "the stream has no final newline"),
            ParseErrorKind::CarriageReturn => {
                write!(f, "carriage return; the format is LF only, in text as well")
            }
            ParseErrorKind::NonFinite(m) => write!(f, "{m} is not a finite double"),
            ParseErrorKind::NegativeZero => {
                write!(f, "-0.0; the schema normalises negative zero away")
            }
            ParseErrorKind::SchemaVersion(v) => {
                write!(f, "schema {v}; this reader knows schema {SCHEMA} only")
            }
            ParseErrorKind::UnknownRecord(r) => write!(f, "unknown record type `{r}`"),
            ParseErrorKind::UnknownField => write!(f, "unknown field"),
            ParseErrorKind::MissingField => write!(f, "missing field"),
            ParseErrorKind::WrongType { expected, found } => {
                write!(f, "expected {expected}, found {found}")
            }
            ParseErrorKind::LengthMismatch { points, bulges } => write!(
                f,
                "{points} points but {bulges} bulges; the schema says one bulge per vertex"
            ),
        }
    }
}

impl std::error::Error for ParseError {}

/// Parse a whole canonical stream.
pub fn parse_stream(text: &str) -> Result<Vec<Record>, ParseError> {
    if text.is_empty() {
        return Ok(Vec::new());
    }
    let mut records = Vec::new();
    let mut rest = text;
    let mut number = 0usize;
    while !rest.is_empty() {
        number += 1;
        let Some(end) = rest.find('\n') else {
            // A CR on the unterminated tail is the more useful thing to say.
            if rest.contains('\r') {
                return Err(ParseError {
                    line: number,
                    field: None,
                    kind: ParseErrorKind::CarriageReturn,
                });
            }
            return Err(ParseError {
                line: number,
                field: None,
                kind: ParseErrorKind::MissingFinalNewline,
            });
        };
        records.push(parse_line(&rest[..end], number)?);
        rest = &rest[end + 1..];
    }
    Ok(records)
}

fn err(line: usize, field: Option<&str>, kind: ParseErrorKind) -> ParseError {
    ParseError {
        line,
        field: field.map(str::to_string),
        kind,
    }
}

/// Parse one line (without its newline). `line` is only used in errors.
pub fn parse_line(text: &str, line: usize) -> Result<Record, ParseError> {
    if text.contains('\r') {
        return Err(err(line, None, ParseErrorKind::CarriageReturn));
    }
    if text.is_empty() {
        return Err(err(line, None, ParseErrorKind::EmptyLine));
    }
    let value: Value = match serde_json::from_str(text) {
        Ok(v) => v,
        Err(e) => return Err(json_error(text, line, &e)),
    };
    let Value::Object(map) = value else {
        return Err(err(line, None, ParseErrorKind::NotAnObject));
    };
    let mut o = Obj::new(&map, line);

    match map.get("schema") {
        None => return Err(err(line, Some("schema"), ParseErrorKind::MissingField)),
        Some(Value::Number(n)) if n.as_u64() == Some(SCHEMA) => {}
        Some(other) => {
            return Err(err(
                line,
                Some("schema"),
                ParseErrorKind::SchemaVersion(other.to_string()),
            ));
        }
    }
    o.seen.insert("schema");
    let name = match map.get("record") {
        None => return Err(err(line, Some("record"), ParseErrorKind::MissingField)),
        Some(Value::String(s)) => s.as_str(),
        Some(other) => return Err(o.wrong("record", "string", other)),
    };
    o.seen.insert("record");

    let record = match name {
        "document" => Record::Document(Document {
            format: o.string("format")?,
            acad_version: o.string("acad_version")?,
            maintenance_version: o.uint("maintenance_version")?,
        }),
        "view" => Record::View(View {
            index: o.uint("index")?,
            name: o.string("name")?,
            kind: o.string("kind")?,
        }),
        "line" => Record::Line(Line {
            view: o.uint("view")?,
            a: [o.float("x1")?, o.float("y1")?, o.float("z1")?],
            b: [o.float("x2")?, o.float("y2")?, o.float("z2")?],
        }),
        "polyline" => {
            let view = o.uint("view")?;
            let closed = o.boolean("closed")?;
            let normal = o.triple("nx", "ny", "nz")?;
            let points = o.points("points")?;
            let bulges = o.floats("bulges")?;
            if points.len() != bulges.len() {
                return Err(err(
                    line,
                    Some("bulges"),
                    ParseErrorKind::LengthMismatch {
                        points: points.len(),
                        bulges: bulges.len(),
                    },
                ));
            }
            Record::Polyline(Polyline {
                view,
                closed,
                normal,
                points,
                bulges,
            })
        }
        "circle" => Record::Circle(Circle {
            view: o.uint("view")?,
            center: o.triple("cx", "cy", "cz")?,
            radius: o.float("radius")?,
            normal: o.triple("nx", "ny", "nz")?,
        }),
        "arc" => Record::Arc(Arc {
            view: o.uint("view")?,
            center: o.triple("cx", "cy", "cz")?,
            radius: o.float("radius")?,
            start: o.float("start")?,
            end: o.float("end")?,
            normal: o.triple("nx", "ny", "nz")?,
        }),
        "ellipse" => Record::Ellipse(Ellipse {
            view: o.uint("view")?,
            center: o.triple("cx", "cy", "cz")?,
            major: o.triple("mx", "my", "mz")?,
            ratio: o.float("ratio")?,
            start: o.float("start")?,
            end: o.float("end")?,
            normal: o.triple("nx", "ny", "nz")?,
        }),
        "spline" => Record::Spline(Spline {
            view: o.uint("view")?,
            degree: o.uint("degree")?,
            closed: o.boolean("closed")?,
            periodic: o.boolean("periodic")?,
            control_points: o.points("control_points")?,
            knots: o.floats("knots")?,
            weights: o.floats("weights")?,
            fit_points: o.points("fit_points")?,
        }),
        "polygon" => Record::Polygon(Polygon {
            view: o.uint("view")?,
            points: o.points("points")?,
        }),
        "point" => Record::Point(Point {
            view: o.uint("view")?,
            at: o.triple("x", "y", "z")?,
        }),
        "text" => Record::Text(Text {
            view: o.uint("view")?,
            at: o.triple("x", "y", "z")?,
            height: o.float("height")?,
            rotation: o.float("rotation")?,
            width_factor: o.float("width_factor")?,
            halign: o.string("halign")?,
            valign: o.string("valign")?,
            style: o.string("style")?,
            text: o.string("text")?,
        }),
        "warning" => Record::Warning(Warning {
            category: o.string("category")?,
            entity_type: o.string("entity_type")?,
            detail: o.string("detail")?,
        }),
        other => {
            return Err(err(
                line,
                Some("record"),
                ParseErrorKind::UnknownRecord(other.to_string()),
            ));
        }
    };

    // Anything the record did not ask for is a field the schema does not have.
    // Map iteration is sorted, so which one is named is stable.
    for key in map.keys() {
        if !o.seen.contains(key.as_str()) {
            return Err(err(line, Some(key), ParseErrorKind::UnknownField));
        }
    }
    Ok(record)
}

/// Turn serde_json's failure into ours. The two numbers it refuses are the two
/// the schema forbids, and they get a kind of their own with the nearest field
/// name, which is the one thing serde_json cannot tell us.
fn json_error(text: &str, line: usize, e: &serde_json::Error) -> ParseError {
    let message = e.to_string();
    let col = e.column().min(text.len());
    if message.contains("number out of range") {
        let field = field_before(text, col);
        return ParseError {
            line,
            field,
            kind: ParseErrorKind::NonFinite("a number outside the range of a double".to_string()),
        };
    }
    if let Some((token, at)) = bare_nonfinite(text) {
        return ParseError {
            line,
            field: field_before(text, at),
            kind: ParseErrorKind::NonFinite(token.to_string()),
        };
    }
    ParseError {
        line,
        field: None,
        kind: ParseErrorKind::NotJson(message),
    }
}

/// The key of the last `"key":` that starts before byte `at`.
fn field_before(text: &str, at: usize) -> Option<String> {
    let head = text.get(..at.min(text.len()))?;
    let colon = head.rfind("\":")?;
    let open = head[..colon].rfind('"')?;
    Some(head[open + 1..colon].to_string())
}

/// `NaN`, `Infinity` or `-Infinity` outside a string, and where it starts.
fn bare_nonfinite(text: &str) -> Option<(&'static str, usize)> {
    let bytes = text.as_bytes();
    let mut in_string = false;
    let mut escaped = false;
    for (i, &b) in bytes.iter().enumerate() {
        if in_string {
            if escaped {
                escaped = false;
            } else if b == b'\\' {
                escaped = true;
            } else if b == b'"' {
                in_string = false;
            }
            continue;
        }
        match b {
            b'"' => in_string = true,
            b'N' if text[i..].starts_with("NaN") => return Some(("NaN", i)),
            b'I' if text[i..].starts_with("Infinity") => {
                let minus = i > 0 && bytes[i - 1] == b'-';
                return Some((if minus { "-Infinity" } else { "Infinity" }, i));
            }
            _ => {}
        }
    }
    None
}

struct Obj<'a> {
    map: &'a Map<String, Value>,
    line: usize,
    seen: BTreeSet<&'a str>,
}

fn describe(v: &Value) -> String {
    match v {
        Value::Null => "null".to_string(),
        Value::Bool(b) => format!("boolean {b}"),
        Value::Number(n) if n.is_f64() => format!("double {n}"),
        Value::Number(n) => format!("integer {n}"),
        Value::String(_) => "a string".to_string(),
        Value::Array(_) => "an array".to_string(),
        Value::Object(_) => "an object".to_string(),
    }
}

impl<'a> Obj<'a> {
    fn new(map: &'a Map<String, Value>, line: usize) -> Self {
        Self {
            map,
            line,
            seen: BTreeSet::new(),
        }
    }

    fn wrong(&self, field: &str, expected: &'static str, found: &Value) -> ParseError {
        err(
            self.line,
            Some(field),
            ParseErrorKind::WrongType {
                expected,
                found: describe(found),
            },
        )
    }

    fn get(&mut self, key: &'static str) -> Result<&'a Value, ParseError> {
        self.seen.insert(key);
        self.map
            .get(key)
            .ok_or_else(|| err(self.line, Some(key), ParseErrorKind::MissingField))
    }

    fn string(&mut self, key: &'static str) -> Result<String, ParseError> {
        let v = self.get(key)?;
        self.string_value(key, v)
    }

    fn string_value(&self, field: &str, v: &Value) -> Result<String, ParseError> {
        match v {
            Value::String(s) => {
                if s.contains('\r') {
                    return Err(err(self.line, Some(field), ParseErrorKind::CarriageReturn));
                }
                Ok(s.clone())
            }
            other => Err(self.wrong(field, "a string", other)),
        }
    }

    fn uint(&mut self, key: &'static str) -> Result<u64, ParseError> {
        let v = self.get(key)?;
        match v {
            Value::Number(n) if n.is_u64() => Ok(n.as_u64().unwrap_or(0)),
            other => Err(self.wrong(key, "an integer", other)),
        }
    }

    fn boolean(&mut self, key: &'static str) -> Result<bool, ParseError> {
        let v = self.get(key)?;
        match v {
            Value::Bool(b) => Ok(*b),
            other => Err(self.wrong(key, "a boolean", other)),
        }
    }

    fn double(&self, field: &str, v: &Value) -> Result<f64, ParseError> {
        match v {
            Value::Number(n) => self.number(field, n),
            other => Err(self.wrong(field, "a double", other)),
        }
    }

    fn number(&self, field: &str, n: &Number) -> Result<f64, ParseError> {
        if !n.is_f64() {
            return Err(err(
                self.line,
                Some(field),
                ParseErrorKind::WrongType {
                    expected: "a double (written with a fractional part or an exponent)",
                    found: format!("integer {n}"),
                },
            ));
        }
        let x = n.as_f64().unwrap_or(f64::NAN);
        if !x.is_finite() {
            return Err(err(
                self.line,
                Some(field),
                ParseErrorKind::NonFinite(n.to_string()),
            ));
        }
        if x == 0.0 && x.is_sign_negative() {
            return Err(err(self.line, Some(field), ParseErrorKind::NegativeZero));
        }
        Ok(x)
    }

    fn float(&mut self, key: &'static str) -> Result<f64, ParseError> {
        let v = self.get(key)?;
        self.double(key, v)
    }

    fn triple(
        &mut self,
        x: &'static str,
        y: &'static str,
        z: &'static str,
    ) -> Result<Vec3, ParseError> {
        Ok([self.float(x)?, self.float(y)?, self.float(z)?])
    }

    fn array(&mut self, key: &'static str) -> Result<&'a Vec<Value>, ParseError> {
        let v = self.get(key)?;
        match v {
            Value::Array(a) => Ok(a),
            other => Err(self.wrong(key, "an array", other)),
        }
    }

    fn floats(&mut self, key: &'static str) -> Result<Vec<f64>, ParseError> {
        let items = self.array(key)?;
        items
            .iter()
            .enumerate()
            .map(|(i, v)| self.double(&format!("{key}[{i}]"), v))
            .collect()
    }

    fn points(&mut self, key: &'static str) -> Result<Vec<Vec3>, ParseError> {
        let items = self.array(key)?;
        items
            .iter()
            .enumerate()
            .map(|(i, v)| {
                let Value::Array(p) = v else {
                    return Err(self.wrong(&format!("{key}[{i}]"), "an [x, y, z] array", v));
                };
                if p.len() != 3 {
                    return Err(err(
                        self.line,
                        Some(&format!("{key}[{i}]")),
                        ParseErrorKind::WrongType {
                            expected: "an [x, y, z] array",
                            found: format!("an array of {}", p.len()),
                        },
                    ));
                }
                let mut out = [0.0; 3];
                for (j, c) in p.iter().enumerate() {
                    out[j] = self.double(&format!("{key}[{i}][{j}]"), c)?;
                }
                Ok(out)
            })
            .collect()
    }
}

/// One line of a sidecar.
#[derive(Debug, Clone, PartialEq)]
pub struct SidecarEntry {
    pub index: usize,
    /// Handle path, outermost insert first, the entity last. Uppercase hex.
    pub handles: Vec<String>,
    /// The record type, when the engine writes it.
    pub record_type: Option<String>,
    /// acadsharp-rs's partial sidecars mark an entity that came from an insert.
    pub from_insert: Option<bool>,
    /// The leaf entity's DXF name (`LWPOLYLINE`), when the engine writes it
    /// (`d`). `t` is the schema 1 record type and must agree with the record;
    /// this is the other thing a diff wants to say about the entity.
    pub dxf_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SidecarErrorKind {
    NotJson(String),
    NotAnObject,
    UnknownField,
    MissingField,
    WrongType(&'static str),
    /// `i` is not the gapless run from zero.
    IndexGap {
        expected: usize,
        found: u64,
    },
    /// Not one line per record.
    CountMismatch {
        lines: usize,
        records: usize,
    },
    /// `t` names a different record type than the record at that index.
    TypeDisagrees {
        sidecar: String,
        record: String,
    },
    /// Not uppercase hex without a prefix.
    BadHandle(String),
    EmptyHandlePath,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SidecarError {
    /// 1-based; 0 when the error is about the file as a whole.
    pub line: usize,
    pub kind: SidecarErrorKind,
}

impl fmt::Display for SidecarError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.line > 0 {
            write!(f, "sidecar line {}: ", self.line)?;
        } else {
            write!(f, "sidecar: ")?;
        }
        match &self.kind {
            SidecarErrorKind::NotJson(m) => write!(f, "not JSON ({m})"),
            SidecarErrorKind::NotAnObject => write!(f, "a line is a JSON object"),
            SidecarErrorKind::UnknownField => write!(f, "unknown field"),
            SidecarErrorKind::MissingField => write!(f, "missing field"),
            SidecarErrorKind::WrongType(t) => write!(f, "wrong type, expected {t}"),
            SidecarErrorKind::IndexGap { expected, found } => write!(
                f,
                "`i` is {found} where the gapless run from zero says {expected}"
            ),
            SidecarErrorKind::CountMismatch { lines, records } => write!(
                f,
                "{lines} lines for {records} records; there is one line per record"
            ),
            SidecarErrorKind::TypeDisagrees { sidecar, record } => write!(
                f,
                "`t` says `{sidecar}` but the record at that index is a `{record}`"
            ),
            SidecarErrorKind::BadHandle(h) => {
                write!(f, "handle {h:?} is not uppercase hex without a prefix")
            }
            SidecarErrorKind::EmptyHandlePath => write!(f, "`h` is empty"),
        }
    }
}

impl std::error::Error for SidecarError {}

/// Names entities in a diff. Not part of the contract.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Sidecar {
    pub entries: Vec<SidecarEntry>,
}

impl Sidecar {
    /// Parse a sidecar and check it against the records it describes.
    pub fn parse(text: &str, records: &[Record]) -> Result<Self, SidecarError> {
        let fail = |line: usize, kind: SidecarErrorKind| SidecarError { line, kind };
        let mut entries = Vec::new();
        for (n, raw) in text.lines().enumerate() {
            let line = n + 1;
            let value: Value = serde_json::from_str(raw)
                .map_err(|e| fail(line, SidecarErrorKind::NotJson(e.to_string())))?;
            let Value::Object(map) = value else {
                return Err(fail(line, SidecarErrorKind::NotAnObject));
            };
            if map
                .keys()
                .any(|k| !matches!(k.as_str(), "i" | "h" | "t" | "x" | "d"))
            {
                return Err(fail(line, SidecarErrorKind::UnknownField));
            }
            let i = match map.get("i") {
                None => return Err(fail(line, SidecarErrorKind::MissingField)),
                Some(Value::Number(n)) if n.is_u64() => n.as_u64().unwrap_or(0),
                Some(_) => return Err(fail(line, SidecarErrorKind::WrongType("an integer"))),
            };
            if i != n as u64 {
                return Err(fail(
                    line,
                    SidecarErrorKind::IndexGap {
                        expected: n,
                        found: i,
                    },
                ));
            }
            let handles = match map.get("h") {
                None => return Err(fail(line, SidecarErrorKind::MissingField)),
                Some(Value::Array(a)) => {
                    let mut out = Vec::new();
                    for h in a {
                        let Value::String(h) = h else {
                            return Err(fail(line, SidecarErrorKind::WrongType("a string")));
                        };
                        if h.is_empty()
                            || !h
                                .bytes()
                                .all(|b| b.is_ascii_digit() || (b'A'..=b'F').contains(&b))
                        {
                            return Err(fail(line, SidecarErrorKind::BadHandle(h.clone())));
                        }
                        out.push(h.clone());
                    }
                    if out.is_empty() {
                        return Err(fail(line, SidecarErrorKind::EmptyHandlePath));
                    }
                    out
                }
                Some(_) => return Err(fail(line, SidecarErrorKind::WrongType("an array"))),
            };
            let record_type = match map.get("t") {
                None => None,
                Some(Value::String(t)) => Some(t.clone()),
                Some(_) => return Err(fail(line, SidecarErrorKind::WrongType("a string"))),
            };
            let from_insert = match map.get("x") {
                None => None,
                Some(Value::Bool(b)) => Some(*b),
                Some(_) => return Err(fail(line, SidecarErrorKind::WrongType("a boolean"))),
            };
            if let (Some(t), Some(record)) = (&record_type, records.get(n))
                && t != record.kind()
            {
                return Err(fail(
                    line,
                    SidecarErrorKind::TypeDisagrees {
                        sidecar: t.clone(),
                        record: record.kind().to_string(),
                    },
                ));
            }
            let dxf_name = match map.get("d") {
                None => None,
                Some(Value::String(d)) => Some(d.clone()),
                Some(_) => return Err(fail(line, SidecarErrorKind::WrongType("a string"))),
            };
            entries.push(SidecarEntry {
                index: n,
                handles,
                record_type,
                from_insert,
                dxf_name,
            });
        }
        if entries.len() != records.len() {
            return Err(fail(
                0,
                SidecarErrorKind::CountMismatch {
                    lines: entries.len(),
                    records: records.len(),
                },
            ));
        }
        Ok(Self { entries })
    }

    pub fn handles(&self, index: usize) -> Option<&[String]> {
        self.entries.get(index).map(|e| e.handles.as_slice())
    }
}

// ---- the writer -----------------------------------------------------------------

/// Why a record could not be written.
#[derive(Debug, Clone, PartialEq)]
pub enum WriteError {
    /// NaN or an infinity. A canonical record cannot carry one, and a NaN
    /// coordinate is a real finding about the reader rather than a formatting
    /// problem, so the write fails and names the value.
    NonFinite { field: &'static str, value: f64 },
}

impl fmt::Display for WriteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NonFinite { field, value } => write!(
                f,
                "refusing to encode the non-finite value {value:?} in `{field}`; a canonical \
                 record cannot carry NaN or an infinity"
            ),
        }
    }
}

impl std::error::Error for WriteError {}

/// A double the way the schema spells one: shortest round-trip, a `.0` when the
/// shortest form has no `.` or exponent, and negative zero written as zero.
pub fn format_real(value: f64) -> Result<String, WriteError> {
    real("value", value)
}

fn real(field: &'static str, value: f64) -> Result<String, WriteError> {
    if !value.is_finite() {
        return Err(WriteError::NonFinite { field, value });
    }
    // -0.0 and 0.0 are the same number and print differently. Two engines that
    // both round-trip IEEE-754 correctly can disagree about which one the same
    // geometry produces, so the format only ever has one of them.
    if value == 0.0 {
        return Ok("0.0".to_string());
    }
    // Rust's Debug for f64 is the shortest string that reads back as the same
    // double. It already spells an integral value `100.0`; a form past 1e16
    // comes out with an exponent, which is a visible double as well.
    let text = format!("{value:?}");
    if text.contains(['.', 'e', 'E']) {
        Ok(text)
    } else {
        Ok(format!("{text}.0"))
    }
}

/// A JSON string the way schema 1 spells one: only `"`, `\` and the C0
/// controls are escaped, and everything else, non-ASCII included, is UTF-8.
/// A default JSON encoder would escape the non-ASCII, which is why the schema
/// hand-writes the policy instead of inheriting one that can change.
pub fn quote(value: &str) -> String {
    use fmt::Write as _;
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for c in value.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

struct JsonLine {
    out: String,
}

impl JsonLine {
    fn new(record: &str) -> Self {
        Self {
            out: format!("{{\"schema\":{SCHEMA},\"record\":\"{record}\""),
        }
    }

    fn key(&mut self, name: &str) {
        self.out.push_str(",\"");
        self.out.push_str(name);
        self.out.push_str("\":");
    }

    fn uint(&mut self, name: &str, v: u64) {
        self.key(name);
        self.out.push_str(&v.to_string());
    }

    fn boolean(&mut self, name: &str, v: bool) {
        self.key(name);
        self.out.push_str(if v { "true" } else { "false" });
    }

    fn string(&mut self, name: &str, v: &str) {
        self.key(name);
        self.out.push_str(&quote(v));
    }

    fn float(&mut self, name: &'static str, v: f64) -> Result<(), WriteError> {
        self.key(name);
        self.out.push_str(&real(name, v)?);
        Ok(())
    }

    fn triple(&mut self, names: [&'static str; 3], v: &Vec3) -> Result<(), WriteError> {
        for (n, x) in names.into_iter().zip(v) {
            self.float(n, *x)?;
        }
        Ok(())
    }

    fn points(&mut self, name: &'static str, points: &[Vec3]) -> Result<(), WriteError> {
        self.key(name);
        self.out.push('[');
        for (i, p) in points.iter().enumerate() {
            if i > 0 {
                self.out.push(',');
            }
            self.out.push('[');
            for (j, x) in p.iter().enumerate() {
                if j > 0 {
                    self.out.push(',');
                }
                self.out.push_str(&real(name, *x)?);
            }
            self.out.push(']');
        }
        self.out.push(']');
        Ok(())
    }

    fn floats(&mut self, name: &'static str, values: &[f64]) -> Result<(), WriteError> {
        self.key(name);
        self.out.push('[');
        for (i, x) in values.iter().enumerate() {
            if i > 0 {
                self.out.push(',');
            }
            self.out.push_str(&real(name, *x)?);
        }
        self.out.push(']');
        Ok(())
    }

    fn done(mut self) -> String {
        self.out.push('}');
        self.out
    }
}

/// One record as one line, without its newline, fields in the schema's order.
pub fn render_record(record: &Record) -> Result<String, WriteError> {
    let mut l = JsonLine::new(record.kind());
    match record {
        Record::Document(d) => {
            l.string("format", &d.format);
            l.string("acad_version", &d.acad_version);
            l.uint("maintenance_version", d.maintenance_version);
        }
        Record::View(v) => {
            l.uint("index", v.index);
            l.string("name", &v.name);
            l.string("kind", &v.kind);
        }
        Record::Line(r) => {
            l.uint("view", r.view);
            l.triple(["x1", "y1", "z1"], &r.a)?;
            l.triple(["x2", "y2", "z2"], &r.b)?;
        }
        Record::Polyline(r) => {
            l.uint("view", r.view);
            l.boolean("closed", r.closed);
            l.triple(["nx", "ny", "nz"], &r.normal)?;
            l.points("points", &r.points)?;
            l.floats("bulges", &r.bulges)?;
        }
        Record::Circle(r) => {
            l.uint("view", r.view);
            l.triple(["cx", "cy", "cz"], &r.center)?;
            l.float("radius", r.radius)?;
            l.triple(["nx", "ny", "nz"], &r.normal)?;
        }
        Record::Arc(r) => {
            l.uint("view", r.view);
            l.triple(["cx", "cy", "cz"], &r.center)?;
            l.float("radius", r.radius)?;
            l.float("start", r.start)?;
            l.float("end", r.end)?;
            l.triple(["nx", "ny", "nz"], &r.normal)?;
        }
        Record::Ellipse(r) => {
            l.uint("view", r.view);
            l.triple(["cx", "cy", "cz"], &r.center)?;
            l.triple(["mx", "my", "mz"], &r.major)?;
            l.float("ratio", r.ratio)?;
            l.float("start", r.start)?;
            l.float("end", r.end)?;
            l.triple(["nx", "ny", "nz"], &r.normal)?;
        }
        Record::Spline(r) => {
            l.uint("view", r.view);
            l.uint("degree", r.degree);
            l.boolean("closed", r.closed);
            l.boolean("periodic", r.periodic);
            l.points("control_points", &r.control_points)?;
            l.floats("knots", &r.knots)?;
            l.floats("weights", &r.weights)?;
            l.points("fit_points", &r.fit_points)?;
        }
        Record::Polygon(r) => {
            l.uint("view", r.view);
            l.points("points", &r.points)?;
        }
        Record::Point(r) => {
            l.uint("view", r.view);
            l.triple(["x", "y", "z"], &r.at)?;
        }
        Record::Text(r) => {
            l.uint("view", r.view);
            l.triple(["x", "y", "z"], &r.at)?;
            l.float("height", r.height)?;
            l.float("rotation", r.rotation)?;
            l.float("width_factor", r.width_factor)?;
            l.string("halign", &r.halign);
            l.string("valign", &r.valign);
            l.string("style", &r.style);
            l.string("text", &r.text);
        }
        Record::Warning(r) => {
            l.string("category", &r.category);
            l.string("entity_type", &r.entity_type);
            l.string("detail", &r.detail);
        }
    }
    Ok(l.done())
}

/// A whole stream: one record per line, LF, and a final newline.
pub fn render_stream(records: &[Record]) -> Result<String, WriteError> {
    let mut out = String::new();
    for record in records {
        out.push_str(&render_record(record)?);
        out.push('\n');
    }
    Ok(out)
}

/// A sidecar, one line per entry: `{"i":..,"h":[..],"t":".."}`, with `x` and
/// `d` when the engine has them.
pub fn render_sidecar(entries: &[SidecarEntry]) -> String {
    let mut out = String::new();
    for e in entries {
        out.push_str(&format!("{{\"i\":{},\"h\":[", e.index));
        for (i, h) in e.handles.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            out.push_str(&quote(h));
        }
        out.push(']');
        if let Some(t) = &e.record_type {
            out.push_str(&format!(",\"t\":{}", quote(t)));
        }
        if let Some(x) = e.from_insert {
            out.push_str(&format!(",\"x\":{x}"));
        }
        if let Some(d) = &e.dxf_name {
            out.push_str(&format!(",\"d\":{}", quote(d)));
        }
        out.push_str("}\n");
    }
    out
}
