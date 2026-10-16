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

use std::fmt;

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
        todo!("R2.1")
    }

    /// The view an entity record belongs to. `None` for `document`, `view` and
    /// `warning`.
    pub fn view(&self) -> Option<u64> {
        todo!("R2.1")
    }

    /// Every float in the record, in schema order, mutably. The perturbation
    /// sweep walks this, and so does nothing else.
    pub fn floats_mut(&mut self) -> Vec<&mut f64> {
        todo!("R2.1")
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
    LengthMismatch { points: usize, bulges: usize },
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
        todo!("R2.1")
    }
}

impl std::error::Error for ParseError {}

/// Parse a whole canonical stream.
pub fn parse_stream(text: &str) -> Result<Vec<Record>, ParseError> {
    let _ = text;
    todo!("R2.1")
}

/// Parse one line (without its newline). `line` is only used in errors.
pub fn parse_line(text: &str, line: usize) -> Result<Record, ParseError> {
    let _ = (text, line);
    todo!("R2.1")
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
}

#[derive(Debug, Clone, PartialEq)]
pub enum SidecarErrorKind {
    NotJson(String),
    NotAnObject,
    UnknownField,
    MissingField,
    WrongType(&'static str),
    /// `i` is not the gapless run from zero.
    IndexGap { expected: usize, found: u64 },
    /// Not one line per record.
    CountMismatch { lines: usize, records: usize },
    /// `t` names a different record type than the record at that index.
    TypeDisagrees { sidecar: String, record: String },
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
        todo!("R2.1")
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
        let _ = (text, records);
        todo!("R2.1")
    }

    pub fn handles(&self, index: usize) -> Option<&[String]> {
        self.entries.get(index).map(|e| e.handles.as_slice())
    }
}
