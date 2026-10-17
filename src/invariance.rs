//! The cross-version invariance exception list (R3.4, acadsharp-rs-tests#24).
//!
//! The seven saves are one drawing saved seven times, so each reader's output
//! has to be the same across them except where the format cannot carry it. The
//! check itself is R4.1's, "each engine against itself across the seven saves",
//! through the comparator. What lives here is the list of what is allowed to
//! differ, so the check has something to be held to: for the oracle's frozen
//! outputs, in `parity/oracle_invariance.toml`.
//!
//! Two kinds of row, and the difference is the point.
//!
//! * `[[exception]]`: the saves differ here because the format cannot carry the
//!   same thing (a pre-2007 text keeps its MIF escape, R14 upper-cases table
//!   names). A reason and an issue, and where AutoCAD's own DXF of the same save
//!   holds the value, the input that proves it.
//! * `[[failure]]`: the saves differ here and the format could carry it, so one
//!   of them is a reader bug. It is listed so it is pinned, not so it is
//!   excused: it stays a failure upstream, and the row goes the day it is fixed.
//!
//! Records are named by the baseline's record index, which is the one
//! coordinate that cannot shift. A row with no reason or no issue is refused
//! before anything is compared.

use std::collections::BTreeSet;

use crate::compare::{Finding, Val};

/// Which kind of row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// The format cannot carry it.
    Exception,
    /// A reader bug, pinned.
    Failure,
}

/// One allowed (or pinned) difference.
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub kind: Kind,
    /// The save being compared with the baseline, a `[[fixture]]` id.
    pub fixture: String,
    /// The record's index in the baseline's stream.
    pub record: usize,
    pub record_type: String,
    pub field: String,
    /// The baseline's value.
    pub a: Val,
    /// This save's value.
    pub b: Val,
    pub reason: String,
    pub issue: String,
    /// A `[[oracle_input]]` whose reading by ezdxf holds the value this save
    /// holds (`b`), when AutoCAD's own DXF of the save settles it.
    pub ground_truth: Option<String>,
}

/// What loading the list can refuse.
#[derive(Debug, Clone, PartialEq)]
pub enum ListError {
    NotToml(String),
    Missing(String),
    /// A row with no reason.
    NoReason(String),
    /// A row with no issue, or one that is not `owner/repo#N`.
    NoIssue(String),
}

impl std::fmt::Display for ListError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotToml(e) => write!(f, "not TOML: {e}"),
            Self::Missing(e) => write!(f, "{e}"),
            Self::NoReason(r) => write!(f, "{r} has no reason"),
            Self::NoIssue(r) => write!(f, "{r} has no issue (an `owner/repo#N`)"),
        }
    }
}

impl std::error::Error for ListError {}

/// The parsed list.
#[derive(Debug, Clone, PartialEq)]
pub struct ExceptionList {
    pub baseline: String,
    pub rows: Vec<Row>,
}

fn is_issue(s: &str) -> bool {
    let Some((repo, n)) = s.split_once('#') else {
        return false;
    };
    repo.split_once('/').is_some_and(|(o, r)| !o.is_empty() && !r.is_empty())
        && !n.is_empty()
        && n.bytes().all(|b| b.is_ascii_digit())
}

fn val(v: &toml::Value, what: &str) -> Result<Val, ListError> {
    match v {
        toml::Value::String(s) => Ok(Val::Str(s.clone())),
        toml::Value::Integer(i) if *i >= 0 => Ok(Val::Int(*i as u64)),
        toml::Value::Boolean(b) => Ok(Val::Bool(*b)),
        toml::Value::Float(x) => Ok(Val::Float(*x)),
        _ => Err(ListError::Missing(format!("{what} is not a string, count, bool or float"))),
    }
}

impl ExceptionList {
    /// The committed list for the oracle.
    pub fn oracle() -> Result<Self, ListError> {
        Self::parse(include_str!("../parity/oracle_invariance.toml"))
    }

    pub fn parse(text: &str) -> Result<Self, ListError> {
        let root: toml::Table = text.parse().map_err(|e: toml::de::Error| ListError::NotToml(e.to_string()))?;
        let baseline = root
            .get("baseline")
            .and_then(toml::Value::as_str)
            .ok_or_else(|| ListError::Missing("no `baseline`".into()))?
            .to_string();
        let mut rows = Vec::new();
        for (key, kind) in [("exception", Kind::Exception), ("failure", Kind::Failure)] {
            let Some(list) = root.get(key).and_then(toml::Value::as_array) else {
                continue;
            };
            for (n, row) in list.iter().enumerate() {
                let t = row
                    .as_table()
                    .ok_or_else(|| ListError::Missing(format!("{key} #{n} is not a table")))?;
                let s = |k: &str| {
                    t.get(k)
                        .and_then(toml::Value::as_str)
                        .map(str::to_string)
                        .ok_or_else(|| ListError::Missing(format!("{key} #{n} has no `{k}`")))
                };
                let label = format!(
                    "{key} #{n} ({} record {} {})",
                    s("fixture").unwrap_or_default(),
                    t.get("record").map_or("?".to_string(), ToString::to_string),
                    s("field").unwrap_or_default()
                );
                let reason = s("reason").unwrap_or_default();
                if reason.trim().is_empty() {
                    return Err(ListError::NoReason(label));
                }
                let issue = s("issue").unwrap_or_default();
                if !is_issue(&issue) {
                    return Err(ListError::NoIssue(label));
                }
                rows.push(Row {
                    kind,
                    fixture: s("fixture")?,
                    record: t
                        .get("record")
                        .and_then(toml::Value::as_integer)
                        .and_then(|i| usize::try_from(i).ok())
                        .ok_or_else(|| ListError::Missing(format!("{label} has no `record`")))?,
                    record_type: s("record_type")?,
                    field: s("field")?,
                    a: val(t.get("a").ok_or_else(|| ListError::Missing(format!("{label} has no `a`")))?, "a")?,
                    b: val(t.get("b").ok_or_else(|| ListError::Missing(format!("{label} has no `b`")))?, "b")?,
                    reason,
                    issue,
                    ground_truth: t.get("ground_truth").and_then(toml::Value::as_str).map(str::to_string),
                });
            }
        }
        Ok(Self { baseline, rows })
    }

    /// The row that explains this finding, if there is one.
    pub fn row_for(&self, fixture: &str, f: &Finding) -> Option<&Row> {
        let record = f.index_a?;
        self.rows.iter().find(|r| {
            r.fixture == fixture
                && r.record == record
                && r.record_type == f.record_type
                && r.field == f.field
                && f.a.as_ref() == Some(&r.a)
                && f.b.as_ref() == Some(&r.b)
        })
    }

    /// The saves the list speaks about.
    pub fn fixtures(&self) -> BTreeSet<&str> {
        self.rows.iter().map(|r| r.fixture.as_str()).collect()
    }
}
