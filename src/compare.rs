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
        let _ = (a, b);
        todo!("R2.1")
    }

    pub fn within(&self, a: f64, b: f64) -> bool {
        let _ = (a, b);
        todo!("R2.1")
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
        let _ = (pair, warnings);
        todo!("R2.1")
    }

    pub fn validate(&self) -> Result<(), TableError> {
        todo!("R2.1")
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
        todo!("R2.1")
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

/// Compare two streams of one fixture under one pair's table.
pub fn compare(
    fixture: &str,
    a: &Stream,
    b: &Stream,
    table: &ClassTable,
) -> Result<Report, TableError> {
    let _ = (fixture, a, b, table);
    todo!("R2.1")
}
