//! The comparator: two canonical streams in, findings out.
//!
//! A class per row, never a number per test. [`ClassTable`] says, for one
//! [`Pair`] of engines, how every kind of field is judged; the table is data, so
//! `docs/TOLERANCES.md` can be held to the same numbers by a test.
//!
//! Two rules shape everything here.
//!
//! * It compares parsed values and never normalises. Whatever the schema forbids
//!   is [`canonical`](crate::canonical)'s refusal, so an emitter that writes it
//!   goes red instead of being tidied.
//! * It never re-derives geometry. An arc spelled with the opposite normal and
//!   swapped angles is the same curve, and it is still a finding here, because
//!   deciding equivalent forms is the schema's job and a comparator that grew
//!   that logic would be a third reader.
//!
//! Records are aligned on content before they are compared, with or without a
//! sidecar, so one dropped record costs one finding and not the rest of the
//! stream.

use std::collections::BTreeMap;
use std::fmt;

use crate::canonical::{Record, Sidecar};

/// `abs <= 1e-9 or rel <= 1e-12`: H1.4's opening values, for the pair that is
/// one reader behind two flatteners.
pub const BOUNDED_TIGHT: Tolerance = Tolerance {
    abs: 1e-9,
    rel: 1e-12,
};

/// `rel <= 1e-6` with a `1e-9` floor: the decision plan's number, for any pair
/// with an unrelated reader in it.
pub const BOUNDED: Tolerance = Tolerance {
    abs: 1e-9,
    rel: 1e-6,
};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tolerance {
    pub abs: f64,
    pub rel: f64,
}

impl Tolerance {
    /// `max(abs, rel * max(|a|, |b|))`. Symmetric, no division, and two zeros
    /// are equal.
    pub fn bound(&self, a: f64, b: f64) -> f64 {
        self.abs.max(self.rel * a.abs().max(b.abs()))
    }

    pub fn within(&self, a: f64, b: f64) -> bool {
        (a - b).abs() <= self.bound(a, b)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Engine {
    Oracle,
    Acadsharp,
    Acadrust,
    Ezdxf,
}

impl Engine {
    pub fn name(self) -> &'static str {
        match self {
            Self::Oracle => "oracle",
            Self::Acadsharp => "acadsharp",
            Self::Acadrust => "acadrust",
            Self::Ezdxf => "ezdxf",
        }
    }
}

/// Which two streams are being compared. `a == b` is an engine against itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Pair {
    pub a: Engine,
    pub b: Engine,
}

impl Pair {
    pub const fn new(a: Engine, b: Engine) -> Self {
        Self { a, b }
    }

    pub fn label(&self) -> String {
        format!("{}-{}", self.a.name(), self.b.name())
    }
}

impl fmt::Display for Pair {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.label())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Class {
    Exact,
    BoundedTight,
    Bounded,
    Angle,
    KnownDivergence,
    Excluded,
    DualOracle,
}

impl Class {
    /// The name `docs/TOLERANCES.md` and the verdict file use.
    pub fn name(self) -> &'static str {
        match self {
            Self::Exact => "EXACT",
            Self::BoundedTight => "BOUNDED_TIGHT",
            Self::Bounded => "BOUNDED",
            Self::Angle => "ANGLE",
            Self::KnownDivergence => "KNOWN_DIVERGENCE",
            Self::Excluded => "EXCLUDED",
            Self::DualOracle => "DUAL_ORACLE",
        }
    }
}

/// What kind of number a float is, because the class is chosen per kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FieldKind {
    Coordinate,
    Direction,
    Scalar,
    /// `text.rotation`, and the `start` of an `arc` and an `ellipse`.
    Angle,
    /// The `end` of an `arc` and an `ellipse`: never wrapped.
    AngleEnd,
}

impl FieldKind {
    pub fn name(self) -> &'static str {
        match self {
            Self::Coordinate => "coordinate",
            Self::Direction => "direction",
            Self::Scalar => "scalar",
            Self::Angle => "angle",
            Self::AngleEnd => "angle_end",
        }
    }
}

/// A value as a finding reports it.
#[derive(Debug, Clone, PartialEq)]
pub enum Val {
    Str(String),
    Int(u64),
    Bool(bool),
    Float(f64),
}

impl fmt::Display for Val {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Str(s) => write!(f, "{s:?}"),
            Self::Int(i) => write!(f, "{i}"),
            Self::Bool(b) => write!(f, "{b}"),
            Self::Float(x) => write!(f, "{x:?}"),
        }
    }
}

/// A class table row for something one side cannot produce by design.
#[derive(Debug, Clone, PartialEq)]
pub struct Exclusion {
    pub scope: Scope,
    pub reason: String,
    pub issue: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Scope {
    /// Every record of this kind, on both sides.
    Record(String),
    /// One field of one record kind, for example `("text", "style")`. An array
    /// field excludes its length and every element.
    Field { record: String, field: String },
}

/// How a known-divergent record is found again.
#[derive(Debug, Clone, PartialEq)]
pub enum RecordKey {
    /// The oracle side's record index. The oracle's index is the one coordinate
    /// that cannot shift, so this is the preferred key when the oracle is one
    /// side of the pair.
    OracleIndex(usize),
    /// A handle path from a sidecar, on either side.
    Handles(Vec<String>),
}

/// A listed (fixture, pair, record, field) that must still differ by exactly
/// the values written here.
#[derive(Debug, Clone, PartialEq)]
pub struct KnownDivergence {
    pub fixture: String,
    pub record: RecordKey,
    pub record_type: String,
    pub field: String,
    pub a: Val,
    pub b: Val,
    pub reason: String,
    pub issue: String,
}

/// Records of one kind that the A side has and the B side does not, listed by
/// their index in A's stream. For a defect that loses whole records (a writer
/// that leaves an entity out), where a field-level [`KnownDivergence`] has
/// nothing to hold on to. Only for pairs whose A side is the oracle, because the
/// index is the oracle's. Every listed index must still be missing, as that kind,
/// or the row is red: a writer that starts keeping the record makes the row stale.
#[derive(Debug, Clone, PartialEq)]
pub struct KnownMissing {
    pub fixture: String,
    pub record_type: String,
    pub indices: Vec<usize>,
    pub reason: String,
    pub issue: String,
}

/// Records one side draws and the other does not, where the reason is known and
/// settled: the whole record is absent from one stream, so there is no field to
/// list a `KnownDivergence` against. The row says how many, and the count has to
/// be exact: one more is a new difference, one fewer a row that has gone stale.
#[derive(Debug, Clone, PartialEq)]
pub struct KnownRecords {
    pub fixture: String,
    pub record_type: String,
    /// Whether the records are in side B and missing from side A (otherwise the
    /// other way round).
    pub only_in_b: bool,
    pub count: usize,
    pub reason: String,
    pub issue: String,
}

/// A disagreement no ground truth settles: listed, not scored.
#[derive(Debug, Clone, PartialEq)]
pub struct DualOracle {
    pub fixture: String,
    pub record_type: String,
    pub field: String,
    pub reason: String,
    pub issue: String,
}

/// What a pair's table says about `warning` records. There is no default,
/// because `entity_type` is an ACadSharp type name and `reader_notification` is
/// a claim about one adapter: whether to compare them is a decision per pair.
#[derive(Debug, Clone, PartialEq)]
pub enum WarningRow {
    Compare,
    Excluded { reason: String, issue: String },
}

#[derive(Debug, Clone, PartialEq)]
pub struct ClassTable {
    pub pair: Pair,
    pub class: Class,
    pub tolerance: Tolerance,
    pub warnings: WarningRow,
    pub excluded: Vec<Exclusion>,
    pub known: Vec<KnownDivergence>,
    pub known_missing: Vec<KnownMissing>,
    pub known_records: Vec<KnownRecords>,
    pub dual_oracle: Vec<DualOracle>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TableError {
    /// An `EXCLUDED`, `KNOWN_DIVERGENCE` or `DUAL_ORACLE` row without a reason.
    NoReason(String),
    /// The same, without an issue.
    NoIssue(String),
}

impl fmt::Display for TableError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoReason(row) => write!(f, "{row} has no reason"),
            Self::NoIssue(row) => write!(f, "{row} has no issue"),
        }
    }
}

impl std::error::Error for TableError {}

impl ClassTable {
    /// The table a pair starts from: `BOUNDED_TIGHT` for oracle against
    /// acadsharp-rs (or either against itself), `BOUNDED` for anything with
    /// acadrust or ezdxf in it.
    pub fn for_pair(pair: Pair, warnings: WarningRow) -> Self {
        let one_reader = |e: Engine| matches!(e, Engine::Oracle | Engine::Acadsharp);
        let (class, tolerance) = if one_reader(pair.a) && one_reader(pair.b) {
            (Class::BoundedTight, BOUNDED_TIGHT)
        } else {
            (Class::Bounded, BOUNDED)
        };
        Self {
            pair,
            class,
            tolerance,
            warnings,
            excluded: Vec::new(),
            known: Vec::new(),
            known_missing: Vec::new(),
            known_records: Vec::new(),
            dual_oracle: Vec::new(),
        }
    }

    /// Every `EXCLUDED`, `KNOWN_DIVERGENCE` and `DUAL_ORACLE` row has to say why
    /// and name an issue. A row without either is an unexplained hole in the
    /// comparison, and it is refused before anything is compared.
    pub fn validate(&self) -> Result<(), TableError> {
        fn need(row: &str, reason: &str, issue: &str) -> Result<(), TableError> {
            if reason.trim().is_empty() {
                return Err(TableError::NoReason(row.to_string()));
            }
            if issue.trim().is_empty() {
                return Err(TableError::NoIssue(row.to_string()));
            }
            Ok(())
        }
        if let WarningRow::Excluded { reason, issue } = &self.warnings {
            need("the EXCLUDED warning row", reason, issue)?;
        }
        for x in &self.excluded {
            need(&format!("EXCLUDED row {:?}", x.scope), &x.reason, &x.issue)?;
        }
        for k in &self.known {
            need(
                &format!(
                    "KNOWN_DIVERGENCE row {}.{} of {}",
                    k.record_type, k.field, k.fixture
                ),
                &k.reason,
                &k.issue,
            )?;
        }
        for k in &self.known_missing {
            need(
                &format!("KNOWN_MISSING row {} of {}", k.record_type, k.fixture),
                &k.reason,
                &k.issue,
            )?;
        }
        for k in &self.known_records {
            need(
                &format!(
                    "KNOWN_DIVERGENCE row for {} {} records of {}",
                    k.count, k.record_type, k.fixture
                ),
                &k.reason,
                &k.issue,
            )?;
        }
        for d in &self.dual_oracle {
            need(
                &format!(
                    "DUAL_ORACLE row {}.{} of {}",
                    d.record_type, d.field, d.fixture
                ),
                &d.reason,
                &d.issue,
            )?;
        }
        Ok(())
    }

    fn record_excluded(&self, kind: &str) -> bool {
        (kind == "warning" && matches!(self.warnings, WarningRow::Excluded { .. }))
            || self
                .excluded
                .iter()
                .any(|x| matches!(&x.scope, Scope::Record(r) if r == kind))
    }

    fn field_excluded(&self, kind: &str, name: &Name) -> bool {
        self.excluded.iter().any(|x| {
            matches!(&x.scope, Scope::Field { record, field } if record == kind && name.is_field(field))
        })
    }
}

/// One side of a comparison: the records, and a sidecar if the engine wrote one.
#[derive(Debug, Clone, Default)]
pub struct Stream {
    pub records: Vec<Record>,
    pub sidecar: Option<Sidecar>,
}

impl Stream {
    pub fn new(records: Vec<Record>) -> Self {
        Self {
            records,
            sidecar: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum FindingKind {
    /// Aligned, and one field differs.
    FieldMismatch,
    /// Aligned by position, and the two records are different kinds.
    RecordTypeMismatch,
    /// Present in A, absent from B.
    OnlyInA,
    /// Present in B, absent from A.
    OnlyInB,
    /// The same record, in a different place.
    Order,
    /// A `KNOWN_DIVERGENCE` row whose record no longer differs at all.
    StaleKnownRow,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Disposition {
    /// Counts against the verdict.
    Open,
    /// A listed `KNOWN_DIVERGENCE` that still diverges exactly as listed.
    Known { issue: String },
    /// A listed `DUAL_ORACLE`: not scored.
    DualOracle { issue: String },
    /// A `KNOWN_DIVERGENCE` row that no longer diverges: tighten the row.
    KnownVanished,
    /// A `KNOWN_DIVERGENCE` row whose values moved: re-argue it.
    KnownMoved,
}

/// One difference. Never a bare "mismatch": every column below is the answer to
/// a question somebody will ask.
#[derive(Debug, Clone, PartialEq)]
pub struct Finding {
    pub fixture: String,
    pub pair: Pair,
    pub kind: FindingKind,
    pub view: Option<u64>,
    pub index_a: Option<usize>,
    pub index_b: Option<usize>,
    pub handles_a: Option<Vec<String>>,
    pub handles_b: Option<Vec<String>>,
    pub record_type: String,
    pub field: String,
    pub a: Option<Val>,
    pub b: Option<Val>,
    pub abs_error: Option<f64>,
    pub rel_error: Option<f64>,
    pub class: Class,
    pub disposition: Disposition,
}

impl Finding {
    /// Whether this counts against the verdict.
    pub fn is_red(&self) -> bool {
        matches!(
            self.disposition,
            Disposition::Open | Disposition::KnownVanished | Disposition::KnownMoved
        )
    }

    /// One line. R4.1 writes `VERDICTS.toml` from the struct, not from this.
    pub fn render(&self) -> String {
        use std::fmt::Write;
        let mut out = format!("{} {}:", self.fixture, self.pair);
        if let Some(v) = self.view {
            let _ = write!(out, " view {v}");
        }
        match (self.index_a, self.index_b) {
            (Some(a), Some(b)) => {
                let _ = write!(out, " a#{a} b#{b}");
            }
            (Some(a), None) => {
                let _ = write!(out, " a#{a}");
            }
            (None, Some(b)) => {
                let _ = write!(out, " b#{b}");
            }
            (None, None) => {}
        }
        for (side, handles) in [("A", &self.handles_a), ("B", &self.handles_b)] {
            if let Some(h) = handles {
                let _ = write!(out, " {side} handles {}", h.join("/"));
            }
        }
        let shown = |v: &Option<Val>| v.as_ref().map_or("-".to_string(), ToString::to_string);
        match &self.kind {
            FindingKind::FieldMismatch => {
                let _ = write!(
                    out,
                    " {}.{}: {} vs {}",
                    self.record_type,
                    self.field,
                    shown(&self.a),
                    shown(&self.b)
                );
            }
            FindingKind::RecordTypeMismatch => {
                let _ = write!(
                    out,
                    " record kind differs: {} vs {}",
                    shown(&self.a),
                    shown(&self.b)
                );
            }
            FindingKind::OnlyInA => {
                let _ = write!(out, " {} is in A and not in B", self.record_type);
            }
            FindingKind::OnlyInB => {
                let _ = write!(out, " {} is in B and not in A", self.record_type);
            }
            FindingKind::Order => {
                let _ = write!(out, " {} is in a different place", self.record_type);
            }
            FindingKind::StaleKnownRow => {
                let _ = write!(out, " {}.{}", self.record_type, self.field);
            }
        }
        if let (Some(abs), Some(rel)) = (self.abs_error, self.rel_error) {
            let _ = write!(out, " (abs {abs:e}, rel {rel:e})");
        }
        let _ = write!(out, " [{}]", self.class.name());
        match &self.disposition {
            Disposition::Open => {}
            Disposition::Known { issue } => {
                let _ = write!(out, " known, {issue}");
            }
            Disposition::DualOracle { issue } => {
                let _ = write!(out, " not scored, {issue}");
            }
            Disposition::KnownVanished => {
                let _ = write!(
                    out,
                    " the listed divergence is gone: tighten the row (A {} vs B {} was listed)",
                    shown(&self.a),
                    shown(&self.b)
                );
            }
            Disposition::KnownMoved => {
                let _ = write!(out, " the divergence moved: re-argue the row");
            }
        }
        out
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct MaxError {
    pub abs: f64,
    pub rel: f64,
}

#[derive(Debug, Clone, Default)]
pub struct Report {
    pub fixture: String,
    pub findings: Vec<Finding>,
    /// Records on each side, before exclusions.
    pub records: (usize, usize),
    /// Aligned `(index in A, index in B)`, in order.
    pub aligned: Vec<(usize, usize)>,
    /// How many scalars were compared field by field.
    pub scalars_compared: usize,
    /// How many floats of those were judged by a tolerance.
    pub floats_compared: usize,
    /// The largest error seen per field kind, over every float compared whether
    /// it passed or not: a bound sitting a thousand times above anything the
    /// corpus produced is visible in every run.
    pub max_errors: BTreeMap<FieldKind, MaxError>,
    /// Records dropped from the comparison by an `EXCLUDED` row, per kind, as
    /// (in A, in B).
    pub excluded_records: BTreeMap<String, (usize, usize)>,
    /// Scalars skipped by an `EXCLUDED` field row.
    pub excluded_fields: usize,
}

impl Report {
    /// The findings that count against the verdict.
    pub fn red(&self) -> Vec<&Finding> {
        self.findings.iter().filter(|f| f.is_red()).collect()
    }

    pub fn is_match(&self) -> bool {
        self.findings.iter().all(|f| !f.is_red())
    }
}

// ---- scalars ------------------------------------------------------------------

/// A field's name, kept unrendered so flattening a record allocates no strings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Name {
    S(&'static str),
    /// The length of an array field: `points.len`.
    Len(&'static str),
    /// An element of an array of numbers: `knots[3]`.
    E(&'static str, usize),
    /// A coordinate of an array of points: `points[2].y`.
    P(&'static str, usize, u8),
}

impl Name {
    fn base(&self) -> &'static str {
        match self {
            Self::S(n) | Self::Len(n) | Self::E(n, _) | Self::P(n, _, _) => n,
        }
    }

    /// Whether an `EXCLUDED` row naming `field` covers this scalar. An array
    /// field covers its length and every element.
    fn is_field(&self, field: &str) -> bool {
        self.base() == field
    }
}

impl fmt::Display for Name {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::S(n) => write!(f, "{n}"),
            Self::Len(n) => write!(f, "{n}.len"),
            Self::E(n, i) => write!(f, "{n}[{i}]"),
            Self::P(n, i, axis) => write!(f, "{n}[{i}].{}", ['x', 'y', 'z'][*axis as usize % 3]),
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum SVal<'a> {
    Str(&'a str),
    Int(u64),
    Bool(bool),
    Float(f64),
}

impl SVal<'_> {
    fn owned(&self) -> Val {
        match *self {
            Self::Str(s) => Val::Str(s.to_string()),
            Self::Int(i) => Val::Int(i),
            Self::Bool(b) => Val::Bool(b),
            Self::Float(x) => Val::Float(x),
        }
    }
}

struct Scalar<'a> {
    name: Name,
    val: SVal<'a>,
    /// Set for floats only.
    kind: Option<FieldKind>,
}

fn st<'a>(v: &mut Vec<Scalar<'a>>, n: &'static str, x: &'a str) {
    v.push(Scalar {
        name: Name::S(n),
        val: SVal::Str(x),
        kind: None,
    });
}
fn un(v: &mut Vec<Scalar<'_>>, n: &'static str, x: u64) {
    v.push(Scalar {
        name: Name::S(n),
        val: SVal::Int(x),
        kind: None,
    });
}
fn bo(v: &mut Vec<Scalar<'_>>, n: &'static str, x: bool) {
    v.push(Scalar {
        name: Name::S(n),
        val: SVal::Bool(x),
        kind: None,
    });
}
fn fl(v: &mut Vec<Scalar<'_>>, n: &'static str, x: f64, k: FieldKind) {
    v.push(Scalar {
        name: Name::S(n),
        val: SVal::Float(x),
        kind: Some(k),
    });
}
fn tri(v: &mut Vec<Scalar<'_>>, names: [&'static str; 3], x: &[f64; 3], k: FieldKind) {
    for (n, x) in names.into_iter().zip(x) {
        fl(v, n, *x, k);
    }
}
fn pts(v: &mut Vec<Scalar<'_>>, n: &'static str, points: &[[f64; 3]]) {
    v.push(Scalar {
        name: Name::Len(n),
        val: SVal::Int(points.len() as u64),
        kind: None,
    });
    for (i, p) in points.iter().enumerate() {
        for (axis, x) in p.iter().enumerate() {
            v.push(Scalar {
                name: Name::P(n, i, axis as u8),
                val: SVal::Float(*x),
                kind: Some(FieldKind::Coordinate),
            });
        }
    }
}
fn flts(v: &mut Vec<Scalar<'_>>, n: &'static str, xs: &[f64]) {
    v.push(Scalar {
        name: Name::Len(n),
        val: SVal::Int(xs.len() as u64),
        kind: None,
    });
    for (i, x) in xs.iter().enumerate() {
        v.push(Scalar {
            name: Name::E(n, i),
            val: SVal::Float(*x),
            kind: Some(FieldKind::Scalar),
        });
    }
}

const XYZ: [&str; 3] = ["x", "y", "z"];
const CXYZ: [&str; 3] = ["cx", "cy", "cz"];
const NXYZ: [&str; 3] = ["nx", "ny", "nz"];
const MXYZ: [&str; 3] = ["mx", "my", "mz"];

/// Every scalar of a record, in schema order.
fn flatten(r: &Record) -> Vec<Scalar<'_>> {
    use FieldKind::{Angle, AngleEnd, Coordinate, Direction, Scalar as Sc};
    let mut v = Vec::with_capacity(24);
    match r {
        Record::Document(d) => {
            st(&mut v, "format", &d.format);
            st(&mut v, "acad_version", &d.acad_version);
            un(&mut v, "maintenance_version", d.maintenance_version);
        }
        Record::View(w) => {
            un(&mut v, "index", w.index);
            st(&mut v, "name", &w.name);
            st(&mut v, "kind", &w.kind);
        }
        Record::Line(l) => {
            un(&mut v, "view", l.view);
            tri(&mut v, ["x1", "y1", "z1"], &l.a, Coordinate);
            tri(&mut v, ["x2", "y2", "z2"], &l.b, Coordinate);
        }
        Record::Polyline(p) => {
            un(&mut v, "view", p.view);
            bo(&mut v, "closed", p.closed);
            tri(&mut v, NXYZ, &p.normal, Direction);
            pts(&mut v, "points", &p.points);
            flts(&mut v, "bulges", &p.bulges);
        }
        Record::Circle(c) => {
            un(&mut v, "view", c.view);
            tri(&mut v, CXYZ, &c.center, Coordinate);
            fl(&mut v, "radius", c.radius, Sc);
            tri(&mut v, NXYZ, &c.normal, Direction);
        }
        Record::Arc(a) => {
            un(&mut v, "view", a.view);
            tri(&mut v, CXYZ, &a.center, Coordinate);
            fl(&mut v, "radius", a.radius, Sc);
            fl(&mut v, "start", a.start, Angle);
            fl(&mut v, "end", a.end, AngleEnd);
            tri(&mut v, NXYZ, &a.normal, Direction);
        }
        Record::Ellipse(e) => {
            un(&mut v, "view", e.view);
            tri(&mut v, CXYZ, &e.center, Coordinate);
            tri(&mut v, MXYZ, &e.major, Direction);
            fl(&mut v, "ratio", e.ratio, Sc);
            fl(&mut v, "start", e.start, Angle);
            fl(&mut v, "end", e.end, AngleEnd);
            tri(&mut v, NXYZ, &e.normal, Direction);
        }
        Record::Spline(s) => {
            un(&mut v, "view", s.view);
            un(&mut v, "degree", s.degree);
            bo(&mut v, "closed", s.closed);
            bo(&mut v, "periodic", s.periodic);
            pts(&mut v, "control_points", &s.control_points);
            flts(&mut v, "knots", &s.knots);
            flts(&mut v, "weights", &s.weights);
            pts(&mut v, "fit_points", &s.fit_points);
        }
        Record::Polygon(p) => {
            un(&mut v, "view", p.view);
            pts(&mut v, "points", &p.points);
        }
        Record::Point(p) => {
            un(&mut v, "view", p.view);
            tri(&mut v, XYZ, &p.at, Coordinate);
        }
        Record::Text(t) => {
            un(&mut v, "view", t.view);
            tri(&mut v, XYZ, &t.at, Coordinate);
            fl(&mut v, "height", t.height, Sc);
            fl(&mut v, "rotation", t.rotation, Angle);
            fl(&mut v, "width_factor", t.width_factor, Sc);
            st(&mut v, "halign", &t.halign);
            st(&mut v, "valign", &t.valign);
            st(&mut v, "style", &t.style);
            st(&mut v, "text", &t.text);
        }
        Record::Warning(w) => {
            st(&mut v, "category", &w.category);
            st(&mut v, "entity_type", &w.entity_type);
            st(&mut v, "detail", &w.detail);
        }
    }
    v
}

// ---- alignment ----------------------------------------------------------------

/// What a record is, cheaply: enough to line two streams up, and nothing a
/// tolerance question could change. The coordinates are rounded to `1e-3`, and
/// two keys agree when they are within one step of each other, so a value that
/// sits on a rounding boundary still lines up with its perturbed twin.
#[derive(Debug, Clone, PartialEq)]
struct Key {
    kind: &'static str,
    view: Option<u64>,
    ident: Vec<String>,
    at: Option<[i64; 3]>,
}

fn quantise(p: &[f64; 3]) -> [i64; 3] {
    [0, 1, 2].map(|i| (p[i] * 1000.0).round() as i64)
}

fn key_of(r: &Record) -> Key {
    let (ident, at): (Vec<String>, Option<[i64; 3]>) = match r {
        Record::Document(_) => (vec![], None),
        Record::View(w) => (vec![w.index.to_string()], None),
        Record::Warning(w) => (
            vec![w.category.clone(), w.entity_type.clone(), w.detail.clone()],
            None,
        ),
        Record::Line(l) => (vec![], Some(quantise(&l.a))),
        Record::Polyline(p) => (vec![], p.points.first().map(quantise)),
        Record::Circle(c) => (vec![], Some(quantise(&c.center))),
        Record::Arc(a) => (vec![], Some(quantise(&a.center))),
        Record::Ellipse(e) => (vec![], Some(quantise(&e.center))),
        Record::Spline(s) => (
            vec![],
            s.control_points
                .first()
                .or(s.fit_points.first())
                .map(quantise),
        ),
        Record::Polygon(p) => (vec![], p.points.first().map(quantise)),
        Record::Point(p) => (vec![], Some(quantise(&p.at))),
        Record::Text(t) => (vec![], Some(quantise(&t.at))),
    };
    Key {
        kind: r.kind(),
        view: r.view(),
        ident,
        at,
    }
}

fn compat(a: &Key, b: &Key) -> bool {
    a.kind == b.kind
        && a.view == b.view
        && a.ident == b.ident
        && match (&a.at, &b.at) {
            (Some(x), Some(y)) => x.iter().zip(y).all(|(p, q)| p.abs_diff(*q) <= 1),
            _ => true,
        }
}

/// Longest common subsequence under `compat`, banded so a 30,000-record stream
/// with one dropped record stays linear: the matching prefix and suffix are
/// peeled off first, and the DP only ever sees what is left.
fn lcs(ka: &[&Key], kb: &[&Key]) -> Vec<(usize, usize)> {
    let (n, m) = (ka.len(), kb.len());
    let mut pairs = Vec::new();
    let mut lo = 0;
    while lo < n && lo < m && compat(ka[lo], kb[lo]) {
        pairs.push((lo, lo));
        lo += 1;
    }
    let (mut ea, mut eb) = (n, m);
    let mut tail = Vec::new();
    while ea > lo && eb > lo && compat(ka[ea - 1], kb[eb - 1]) {
        ea -= 1;
        eb -= 1;
        tail.push((ea, eb));
    }
    pairs.extend(
        middle(&ka[lo..ea], &kb[lo..eb])
            .into_iter()
            .map(|(i, j)| (i + lo, j + lo)),
    );
    tail.reverse();
    pairs.extend(tail);
    pairs
}

fn middle(ka: &[&Key], kb: &[&Key]) -> Vec<(usize, usize)> {
    let (n, m) = (ka.len(), kb.len());
    if n == 0 || m == 0 {
        return Vec::new();
    }
    let width = if n * m <= 4_000_000 {
        m
    } else {
        n.abs_diff(m) + 256
    };
    let centre = |i: usize| i * m / n;
    let lo = |i: usize| centre(i).saturating_sub(width);
    let hi = |i: usize| (centre(i) + width).min(m);
    let mut rows: Vec<Vec<u32>> = Vec::with_capacity(n + 1);
    let get = |rows: &Vec<Vec<u32>>, i: usize, j: usize| -> u32 {
        if i >= rows.len() || j < lo(i) || j > hi(i) {
            0
        } else {
            rows[i][j - lo(i)]
        }
    };
    rows.push(vec![0; hi(0) - lo(0) + 1]);
    for i in 1..=n {
        let mut row = vec![0u32; hi(i) - lo(i) + 1];
        for j in lo(i)..=hi(i) {
            if j == 0 {
                continue;
            }
            let diag = get(&rows, i - 1, j - 1);
            let up = get(&rows, i - 1, j);
            let left = if j > lo(i) { row[j - 1 - lo(i)] } else { 0 };
            row[j - lo(i)] = if compat(ka[i - 1], kb[j - 1]) {
                diag + 1
            } else {
                up.max(left)
            };
        }
        rows.push(row);
    }
    let (mut i, mut j) = (n, m);
    let mut out = Vec::new();
    while i > 0 && j > 0 {
        let here = get(&rows, i, j);
        if compat(ka[i - 1], kb[j - 1]) && here == get(&rows, i - 1, j - 1) + 1 {
            out.push((i - 1, j - 1));
            i -= 1;
            j -= 1;
        } else if get(&rows, i - 1, j) >= get(&rows, i, j - 1) {
            i -= 1;
        } else {
            j -= 1;
        }
    }
    out.reverse();
    out
}

// ---- comparing ----------------------------------------------------------------

/// `(-pi, pi]`.
fn wrap(d: f64) -> f64 {
    let tau = std::f64::consts::TAU;
    let mut d = d % tau;
    if d > std::f64::consts::PI {
        d -= tau;
    } else if d <= -std::f64::consts::PI {
        d += tau;
    }
    d
}

fn rel_error(abs: f64, a: f64, b: f64) -> f64 {
    let scale = a.abs().max(b.abs());
    if scale == 0.0 { 0.0 } else { abs / scale }
}

struct Run<'t> {
    fixture: &'t str,
    table: &'t ClassTable,
    a: &'t Stream,
    b: &'t Stream,
    report: Report,
}

impl Run<'_> {
    fn finding(
        &self,
        kind: FindingKind,
        index_a: Option<usize>,
        index_b: Option<usize>,
        record_type: &str,
        field: String,
    ) -> Finding {
        let rec = index_a
            .and_then(|i| self.a.records.get(i))
            .or_else(|| index_b.and_then(|i| self.b.records.get(i)));
        let handles = |s: &Stream, i: Option<usize>| {
            let (sc, i) = (s.sidecar.as_ref()?, i?);
            sc.handles(i).map(<[String]>::to_vec)
        };
        Finding {
            fixture: self.fixture.to_string(),
            pair: self.table.pair,
            kind,
            view: rec.and_then(Record::view),
            index_a,
            index_b,
            handles_a: handles(self.a, index_a),
            handles_b: handles(self.b, index_b),
            record_type: record_type.to_string(),
            field,
            a: None,
            b: None,
            abs_error: None,
            rel_error: None,
            class: Class::Exact,
            disposition: Disposition::Open,
        }
    }

    fn only(&mut self, in_a: bool, i: usize) {
        let rec = if in_a {
            &self.a.records[i]
        } else {
            &self.b.records[i]
        };
        let (kind, ia, ib) = if in_a {
            (FindingKind::OnlyInA, Some(i), None)
        } else {
            (FindingKind::OnlyInB, None, Some(i))
        };
        let f = self.finding(kind, ia, ib, rec.kind(), String::new());
        self.report.findings.push(f);
    }

    fn fields(&mut self, ia: usize, ib: usize) {
        let (ra, rb) = (&self.a.records[ia], &self.b.records[ib]);
        let kind = ra.kind();
        let (sa, sb) = (flatten(ra), flatten(rb));
        let mut by_name: Option<Vec<(String, usize)>> = None;
        let mut shift = 0.0;
        for (n, x) in sa.iter().enumerate() {
            if self.table.field_excluded(kind, &x.name) {
                self.report.excluded_fields += 1;
                continue;
            }
            let y = match sb.get(n) {
                Some(y) if y.name == x.name => y,
                _ => {
                    // The two records differ in an array's length, and the
                    // length is its own finding. Elements past the shorter
                    // array have nothing to compare against.
                    let index = by_name.get_or_insert_with(|| {
                        sb.iter()
                            .enumerate()
                            .map(|(i, s)| (s.name.to_string(), i))
                            .collect()
                    });
                    let wanted = x.name.to_string();
                    match index.iter().find(|(name, _)| *name == wanted) {
                        Some((_, i)) => &sb[*i],
                        None => continue,
                    }
                }
            };
            self.report.scalars_compared += 1;
            match (x.val, y.val) {
                (SVal::Float(p), SVal::Float(q)) => {
                    let fk = x.kind.unwrap_or(FieldKind::Scalar);
                    let tol = self.table.tolerance;
                    let (abs, ok, class) = match fk {
                        FieldKind::Angle => {
                            let d = wrap(p - q);
                            if x.name == Name::S("start") {
                                shift = ((p - q) / std::f64::consts::TAU).round()
                                    * std::f64::consts::TAU;
                            }
                            (d.abs(), d.abs() <= tol.bound(p, q), Class::Angle)
                        }
                        FieldKind::AngleEnd => {
                            // Not wrapped: a full turn is a different arc. The
                            // only allowance is the whole turn `start` already
                            // wrapped by, since the schema spells `end` as
                            // start + sweep.
                            let q = q + shift;
                            let d = (p - q).abs();
                            (d, d <= tol.bound(p, q), Class::Angle)
                        }
                        _ => ((p - q).abs(), tol.within(p, q), self.table.class),
                    };
                    let rel = rel_error(abs, p, q);
                    self.report.floats_compared += 1;
                    let max = self.report.max_errors.entry(fk).or_default();
                    max.abs = max.abs.max(abs);
                    max.rel = max.rel.max(rel);
                    if !ok {
                        let mut f = self.finding(
                            FindingKind::FieldMismatch,
                            Some(ia),
                            Some(ib),
                            kind,
                            x.name.to_string(),
                        );
                        f.a = Some(x.val.owned());
                        f.b = Some(y.val.owned());
                        f.abs_error = Some(abs);
                        f.rel_error = Some(rel);
                        f.class = class;
                        self.report.findings.push(f);
                    }
                }
                (va, vb) => {
                    let (oa, ob) = (va.owned(), vb.owned());
                    if oa != ob {
                        let mut f = self.finding(
                            FindingKind::FieldMismatch,
                            Some(ia),
                            Some(ib),
                            kind,
                            x.name.to_string(),
                        );
                        f.a = Some(oa);
                        f.b = Some(ob);
                        self.report.findings.push(f);
                    }
                }
            }
        }
        self.report.aligned.push((ia, ib));
    }

    /// `KNOWN_DIVERGENCE` and `DUAL_ORACLE` rows decide what the open findings
    /// mean. A known row that still holds exactly is not red; one that no longer
    /// diverges, or diverges differently, is red and says what to do about it.
    fn reconcile(&mut self) {
        let pair = self.table.pair;
        let oracle_side = if pair.a == Engine::Oracle {
            Some(true)
        } else if pair.b == Engine::Oracle {
            Some(false)
        } else {
            None
        };
        for row in self
            .table
            .known
            .iter()
            .filter(|k| k.fixture == self.fixture)
        {
            let hit = self.report.findings.iter().position(|f| {
                f.kind == FindingKind::FieldMismatch
                    && f.record_type == row.record_type
                    && f.field == row.field
                    && match &row.record {
                        RecordKey::OracleIndex(i) => match oracle_side {
                            Some(true) => f.index_a == Some(*i),
                            Some(false) => f.index_b == Some(*i),
                            None => false,
                        },
                        RecordKey::Handles(path) => {
                            f.handles_a.as_ref() == Some(path) || f.handles_b.as_ref() == Some(path)
                        }
                    }
            });
            match hit {
                Some(at) => {
                    let f = &mut self.report.findings[at];
                    f.class = Class::KnownDivergence;
                    f.disposition = if f.a.as_ref() == Some(&row.a) && f.b.as_ref() == Some(&row.b)
                    {
                        Disposition::Known {
                            issue: row.issue.clone(),
                        }
                    } else {
                        Disposition::KnownMoved
                    };
                }
                None => {
                    let (ia, ib) = match (&row.record, oracle_side) {
                        (RecordKey::OracleIndex(i), Some(true)) => (Some(*i), None),
                        (RecordKey::OracleIndex(i), Some(false)) => (None, Some(*i)),
                        _ => (None, None),
                    };
                    let mut f = self.finding(
                        FindingKind::StaleKnownRow,
                        ia,
                        ib,
                        &row.record_type,
                        row.field.clone(),
                    );
                    f.a = Some(row.a.clone());
                    f.b = Some(row.b.clone());
                    f.class = Class::KnownDivergence;
                    f.disposition = Disposition::KnownVanished;
                    self.report.findings.push(f);
                }
            }
        }
        for row in self
            .table
            .known_missing
            .iter()
            .filter(|k| k.fixture == self.fixture)
        {
            for &idx in &row.indices {
                let hit = self.report.findings.iter().position(|f| {
                    f.kind == FindingKind::OnlyInA
                        && f.disposition == Disposition::Open
                        && f.record_type == row.record_type
                        && f.index_a == Some(idx)
                });
                match hit {
                    Some(at) => {
                        let f = &mut self.report.findings[at];
                        f.class = Class::KnownDivergence;
                        f.disposition = Disposition::Known {
                            issue: row.issue.clone(),
                        };
                    }
                    None => {
                        let mut f = self.finding(
                            FindingKind::StaleKnownRow,
                            Some(idx),
                            None,
                            &row.record_type,
                            "(missing)".to_string(),
                        );
                        f.class = Class::KnownDivergence;
                        f.disposition = Disposition::KnownVanished;
                        self.report.findings.push(f);
                    }
                }
            }
        }
        for row in self
            .table
            .known_records
            .iter()
            .filter(|k| k.fixture == self.fixture)
        {
            let want = if row.only_in_b {
                FindingKind::OnlyInB
            } else {
                FindingKind::OnlyInA
            };
            let hits: Vec<usize> = self
                .report
                .findings
                .iter()
                .enumerate()
                .filter(|(_, f)| {
                    f.kind == want
                        && f.disposition == Disposition::Open
                        && f.record_type == row.record_type
                })
                .map(|(at, _)| at)
                .collect();
            let exact = hits.len() == row.count;
            for &at in &hits {
                let f = &mut self.report.findings[at];
                f.class = Class::KnownDivergence;
                f.disposition = if exact {
                    Disposition::Known {
                        issue: row.issue.clone(),
                    }
                } else {
                    Disposition::KnownMoved
                };
            }
            if hits.is_empty() {
                let mut f = self.finding(
                    FindingKind::StaleKnownRow,
                    None,
                    None,
                    &row.record_type,
                    String::new(),
                );
                f.class = Class::KnownDivergence;
                f.disposition = Disposition::KnownVanished;
                self.report.findings.push(f);
            }
        }
        for row in self
            .table
            .dual_oracle
            .iter()
            .filter(|d| d.fixture == self.fixture)
        {
            for f in self.report.findings.iter_mut() {
                if f.disposition == Disposition::Open
                    && f.kind == FindingKind::FieldMismatch
                    && f.record_type == row.record_type
                    && f.field == row.field
                {
                    f.class = Class::DualOracle;
                    f.disposition = Disposition::DualOracle {
                        issue: row.issue.clone(),
                    };
                }
            }
        }
    }
}

/// Compare two streams of one fixture under one pair's table.
pub fn compare(
    fixture: &str,
    a: &Stream,
    b: &Stream,
    table: &ClassTable,
) -> Result<Report, TableError> {
    table.validate()?;
    let mut run = Run {
        fixture,
        table,
        a,
        b,
        report: Report {
            fixture: fixture.to_string(),
            records: (a.records.len(), b.records.len()),
            ..Report::default()
        },
    };
    for kind in [
        FieldKind::Coordinate,
        FieldKind::Direction,
        FieldKind::Scalar,
        FieldKind::Angle,
        FieldKind::AngleEnd,
    ] {
        run.report.max_errors.insert(kind, MaxError::default());
    }

    // Excluded record kinds leave the comparison on both sides, and are counted.
    let kept = |s: &Stream, side: usize, run: &mut Run<'_>| -> Vec<usize> {
        let mut out = Vec::new();
        for (i, r) in s.records.iter().enumerate() {
            if table.record_excluded(r.kind()) {
                let e = run
                    .report
                    .excluded_records
                    .entry(r.kind().to_string())
                    .or_default();
                if side == 0 { e.0 += 1 } else { e.1 += 1 }
            } else {
                out.push(i);
            }
        }
        out
    };
    let ia = kept(a, 0, &mut run);
    let ib = kept(b, 1, &mut run);
    let keys_a: Vec<Key> = ia.iter().map(|&i| key_of(&a.records[i])).collect();
    let keys_b: Vec<Key> = ib.iter().map(|&i| key_of(&b.records[i])).collect();
    let ref_a: Vec<&Key> = keys_a.iter().collect();
    let ref_b: Vec<&Key> = keys_b.iter().collect();

    let matched = lcs(&ref_a, &ref_b);
    let mut in_a = vec![true; ia.len()];
    let mut in_b = vec![true; ib.len()];
    for &(i, j) in &matched {
        in_a[i] = false;
        in_b[j] = false;
    }
    let only_a: Vec<usize> = (0..ia.len()).filter(|&i| in_a[i]).collect();
    let only_b: Vec<usize> = (0..ib.len()).filter(|&j| in_b[j]).collect();

    // The same record in another place: pair it up before anything is called
    // missing. Looked up by quantised position, so it stays linear.
    type Cell = (&'static str, Option<u64>, Vec<String>, Option<[i64; 3]>);
    let mut pool: std::collections::HashMap<Cell, Vec<usize>> = std::collections::HashMap::new();
    for &j in &only_b {
        let k = &keys_b[j];
        pool.entry((k.kind, k.view, k.ident.clone(), k.at))
            .or_default()
            .push(j);
    }
    let mut moved: Vec<(usize, usize)> = Vec::new();
    let mut moved_a = std::collections::HashSet::new();
    let mut moved_b = std::collections::HashSet::new();
    for &i in &only_a {
        let k = &keys_a[i];
        let offsets: Vec<[i64; 3]> = if k.at.is_some() {
            (-1..=1)
                .flat_map(|x| (-1..=1).flat_map(move |y| (-1..=1).map(move |z| [x, y, z])))
                .collect()
        } else {
            vec![[0, 0, 0]]
        };
        'search: for o in offsets {
            // Saturating: a quantised coordinate can sit at the end of the i64
            // range (a 1e300 coordinate casts there), and `+ 1` on it is an
            // overflow panic in a debug build. Found by the comparator fuzz
            // target.
            let at = k.at.map(|p| {
                [
                    p[0].saturating_add(o[0]),
                    p[1].saturating_add(o[1]),
                    p[2].saturating_add(o[2]),
                ]
            });
            if let Some(list) = pool.get_mut(&(k.kind, k.view, k.ident.clone(), at))
                && let Some(pos) = list.iter().position(|j| !moved_b.contains(j))
            {
                let j = list[pos];
                moved_b.insert(j);
                moved_a.insert(i);
                moved.push((i, j));
                break 'search;
            }
        }
    }

    // What is left sits in a gap between two anchors, and the two sides of one
    // gap are paired in order: a replaced record is one finding, not a delete
    // and an insert.
    let gap_a = |i: usize| matched.partition_point(|&(m, _)| m < i);
    let gap_b = |j: usize| matched.partition_point(|&(_, m)| m < j);
    let mut gaps: BTreeMap<usize, (Vec<usize>, Vec<usize>)> = BTreeMap::new();
    for &i in only_a.iter().filter(|i| !moved_a.contains(*i)) {
        gaps.entry(gap_a(i)).or_default().0.push(i);
    }
    for &j in only_b.iter().filter(|j| !moved_b.contains(*j)) {
        gaps.entry(gap_b(j)).or_default().1.push(j);
    }

    for &(i, j) in &matched {
        run.fields(ia[i], ib[j]);
    }
    for &(i, j) in &moved {
        let (oi, oj) = (ia[i], ib[j]);
        let mut f = run.finding(
            FindingKind::Order,
            Some(oi),
            Some(oj),
            a.records[oi].kind(),
            String::new(),
        );
        f.class = Class::Exact;
        run.report.findings.push(f);
        run.fields(oi, oj);
    }
    for (_, (da, db)) in gaps {
        for (n, &i) in da.iter().enumerate() {
            match db.get(n) {
                Some(&j) => {
                    let (oi, oj) = (ia[i], ib[j]);
                    let (ka, kb) = (a.records[oi].kind(), b.records[oj].kind());
                    if ka == kb {
                        run.fields(oi, oj);
                    } else {
                        let mut f = run.finding(
                            FindingKind::RecordTypeMismatch,
                            Some(oi),
                            Some(oj),
                            ka,
                            "record".to_string(),
                        );
                        f.a = Some(Val::Str(ka.to_string()));
                        f.b = Some(Val::Str(kb.to_string()));
                        run.report.findings.push(f);
                    }
                }
                None => run.only(true, ia[i]),
            }
        }
        for &j in db.iter().skip(da.len()) {
            run.only(false, ib[j]);
        }
    }

    run.report.aligned.sort_unstable();
    run.report
        .findings
        .sort_by_key(|f| (f.index_a.or(f.index_b).unwrap_or(usize::MAX), f.index_b));
    run.reconcile();
    Ok(run.report)
}
