//! Verdicts: one word and one reason for every (fixture, pair).
//!
//! `parity/VERDICTS.toml` is the committed table of them, `parity/SUMMARY.md` is
//! generated from it, and `tests/parity.rs` recomputes every verdict from a real
//! run and goes red on any row that moved in either direction. The words are
//! derived from R2.1's classes (`src/compare.rs`), so there is one rule and one
//! word for each outcome:
//!
//! | word | meaning |
//! |---|---|
//! | `MATCH` | every field inside its class, and no `KNOWN_DIVERGENCE` row applies |
//! | `KNOWN:n` | n records diverge and every one is a listed `KNOWN_DIVERGENCE` row with an issue |
//! | `DIFF:n` | n records carry an out-of-class field no row lists: debt, with the issue that will adjudicate it |
//! | `COUNT:a/b` | record counts differ after excluded kinds are dropped, beyond what content alignment can pair up |
//! | `OPEN_FAILED` | an engine exited non-zero or wrote no stream for this fixture |
//! | `EXCLUDED` | the whole pair is excluded for this fixture by design |
//! | `DUAL_ORACLE` | the sides disagree and no ground truth settles it: listed, not scored |

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::compare::{Disposition, Finding, Report};

/// The outcome for one (fixture, pair).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    Match,
    Known(usize),
    Diff(usize),
    Count(usize, usize),
    OpenFailed,
    Excluded,
    DualOracle,
}

impl fmt::Display for Verdict {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Match => f.write_str("MATCH"),
            Self::Known(n) => write!(f, "KNOWN:{n}"),
            Self::Diff(n) => write!(f, "DIFF:{n}"),
            Self::Count(a, b) => write!(f, "COUNT:{a}/{b}"),
            Self::OpenFailed => f.write_str("OPEN_FAILED"),
            Self::Excluded => f.write_str("EXCLUDED"),
            Self::DualOracle => f.write_str("DUAL_ORACLE"),
        }
    }
}

impl Verdict {
    /// Read the word back. `None` for anything this file does not define.
    pub fn parse(s: &str) -> Option<Self> {
        let num = |t: &str| t.parse::<usize>().ok();
        match s {
            "MATCH" => Some(Self::Match),
            "OPEN_FAILED" => Some(Self::OpenFailed),
            "EXCLUDED" => Some(Self::Excluded),
            "DUAL_ORACLE" => Some(Self::DualOracle),
            _ => {
                if let Some(n) = s.strip_prefix("KNOWN:") {
                    num(n).map(Self::Known)
                } else if let Some(n) = s.strip_prefix("DIFF:") {
                    num(n).map(Self::Diff)
                } else if let Some(r) = s.strip_prefix("COUNT:") {
                    let (a, b) = r.split_once('/')?;
                    Some(Self::Count(num(a)?, num(b)?))
                } else {
                    None
                }
            }
        }
    }

    /// Everything except `MATCH` is a verdict that needs a reason and an issue.
    pub fn needs_reason(&self) -> bool {
        !matches!(self, Self::Match)
    }
}

/// Distinct records among some findings: a record with three bad fields is one.
pub fn records_of<'a>(findings: impl Iterator<Item = &'a Finding>) -> usize {
    findings
        .map(|f| {
            (
                f.index_a,
                f.index_b,
                f.handles_a.clone(),
                f.handles_b.clone(),
            )
        })
        .collect::<BTreeSet<_>>()
        .len()
}

/// The verdict a comparison earns, and the first differing record as the
/// comparator renders it.
pub fn judge(report: &Report) -> (Verdict, Option<String>) {
    let open: Vec<&Finding> = report.red();
    let known: Vec<&Finding> = report
        .findings
        .iter()
        .filter(|f| matches!(f.disposition, Disposition::Known { .. }))
        .collect();
    let dual: Vec<&Finding> = report
        .findings
        .iter()
        .filter(|f| matches!(f.disposition, Disposition::DualOracle { .. }))
        .collect();
    let first = open
        .first()
        .or(known.first())
        .or(dual.first())
        .map(|f| f.render());

    if !open.is_empty() {
        // Counts after the excluded kinds are dropped from both sides. When the
        // alignment could not pair most of what is left, the honest word is the
        // two counts, not a finding per record.
        let (mut a, mut b) = report.records;
        for (x, y) in report.excluded_records.values() {
            a -= x;
            b -= y;
        }
        let unpaired = open
            .iter()
            .filter(|f| {
                matches!(
                    f.kind,
                    crate::compare::FindingKind::OnlyInA | crate::compare::FindingKind::OnlyInB
                )
            })
            .count();
        if a != b && unpaired * 2 > a.max(b) {
            return (Verdict::Count(a, b), first);
        }
        return (Verdict::Diff(records_of(open.into_iter())), first);
    }
    if !known.is_empty() {
        return (Verdict::Known(records_of(known.into_iter())), first);
    }
    if !dual.is_empty() {
        return (Verdict::DualOracle, first);
    }
    (Verdict::Match, first)
}

// ---- the committed table ------------------------------------------------------------

/// One `[[row]]` of `parity/VERDICTS.toml`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Row {
    pub fixture: String,
    pub pair: String,
    pub verdict: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub reason: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub issue: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub first: String,
}

/// A run that confirmed the table: where it was made.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Run {
    /// `x86_64` or `aarch64`.
    pub arch: String,
    /// The archive's `native_artifact_version`, with whatever else labels it.
    pub archive: String,
    pub acadsharp_rs: String,
    pub acadrust: String,
    pub date: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Table {
    #[serde(default, rename = "run", skip_serializing_if = "Vec::is_empty")]
    pub runs: Vec<Run>,
    #[serde(default, rename = "row")]
    pub rows: Vec<Row>,
}

const HEADER: &str = "# parity/VERDICTS.toml: one row per (fixture, pair).\n\
#\n\
# The words are defined in src/verdict.rs. `tests/parity.rs` recomputes every\n\
# verdict from a real run and is red on any row that moved, better or worse,\n\
# until this file changes. A row that is not MATCH has a reason and an issue:\n\
# a committed DIFF is debt, and its issue is where it gets adjudicated.\n\
#\n\
# `PARITY_REGEN=1 cargo test --features acadrust --test parity` rewrites the\n\
# verdicts and `first` from the run, keeps the reason and issue of every row\n\
# whose key it finds, and leaves the rest blank, which the test then refuses.\n\
# There is no `arch` key on a row: verdicts are arch-independent by contract,\n\
# and the [[run]] tables list the runs that confirmed that.\n\n";

impl Table {
    pub fn parse(text: &str) -> Result<Self, String> {
        toml::from_str(text).map_err(|e| e.to_string())
    }

    pub fn render(&self) -> String {
        let mut out = String::from(HEADER);
        let body = toml::to_string(self).unwrap_or_default();
        out.push_str(&body);
        out
    }

    pub fn key(row: &Row) -> (String, String) {
        (row.fixture.clone(), row.pair.clone())
    }

    pub fn by_key(&self) -> BTreeMap<(String, String), &Row> {
        self.rows.iter().map(|r| (Self::key(r), r)).collect()
    }

    /// Why this table cannot be committed as it stands, row by row.
    pub fn refusals(&self) -> Vec<String> {
        let mut out = Vec::new();
        let mut seen = BTreeSet::new();
        for r in &self.rows {
            if !seen.insert(Self::key(r)) {
                out.push(format!("{} {} appears twice", r.fixture, r.pair));
            }
            let Some(v) = Verdict::parse(&r.verdict) else {
                out.push(format!(
                    "{} {}: {:?} is not a verdict",
                    r.fixture, r.pair, r.verdict
                ));
                continue;
            };
            if v.needs_reason() && r.reason.trim().is_empty() {
                out.push(format!(
                    "{} {} is {} and has no reason",
                    r.fixture, r.pair, r.verdict
                ));
            }
            if v.needs_reason() && r.issue.trim().is_empty() {
                out.push(format!(
                    "{} {} is {} and has no issue",
                    r.fixture, r.pair, r.verdict
                ));
            }
        }
        out
    }

    /// Every way `actual` differs from `self`, as a line each: the row, both
    /// verdicts and `first`. Empty when nothing moved.
    pub fn moved(&self, actual: &BTreeMap<(String, String), (String, String)>) -> Vec<String> {
        let mine = self.by_key();
        let mut out = Vec::new();
        for (key, (verdict, first)) in actual {
            match mine.get(key) {
                None => out.push(format!(
                    "{} {}: no row is committed, and the run says {verdict}{}",
                    key.0,
                    key.1,
                    show_first(first)
                )),
                Some(r) if &r.verdict != verdict => out.push(format!(
                    "{} {}: committed {}, run says {verdict}{}",
                    key.0,
                    key.1,
                    r.verdict,
                    show_first(first)
                )),
                Some(_) => {}
            }
        }
        for (key, r) in &mine {
            if !actual.contains_key(key) {
                out.push(format!(
                    "{} {}: committed as {} and the run produced no such pair",
                    key.0, key.1, r.verdict
                ));
            }
        }
        out
    }
}

fn show_first(first: &str) -> String {
    if first.is_empty() {
        String::new()
    } else {
        format!("; first: {first}")
    }
}

// ---- SUMMARY.md ---------------------------------------------------------------------------

/// `parity/SUMMARY.md`, a pure function of the table. A test fails when the
/// committed copy is not exactly this.
pub fn summary(table: &Table) -> String {
    let mut out = String::from(
        "# Parity summary\n\n\
Generated from `parity/VERDICTS.toml` by `acadsharp_rs_tests::verdict::summary`;\n\
a test fails when this file is not exactly what the generator writes. Do not edit it.\n\n",
    );
    out.push_str("## Runs that confirmed the table\n\n");
    if table.runs.is_empty() {
        out.push_str("None recorded.\n");
    } else {
        out.push_str(
            "| arch | archive | acadsharp-rs | acadrust | date |\n|---|---|---|---|---|\n",
        );
        for r in &table.runs {
            out.push_str(&format!(
                "| {} | {} | `{}` | `{}` | {} |\n",
                r.arch, r.archive, r.acadsharp_rs, r.acadrust, r.date
            ));
        }
    }

    let mut pairs: BTreeMap<&str, BTreeMap<String, usize>> = BTreeMap::new();
    for r in &table.rows {
        let word = r
            .verdict
            .split(':')
            .next()
            .unwrap_or(&r.verdict)
            .to_string();
        *pairs
            .entry(r.pair.as_str())
            .or_default()
            .entry(word)
            .or_default() += 1;
    }
    out.push_str("\n## Tally by pair\n\n| pair | rows | MATCH | KNOWN | DIFF | COUNT | OPEN_FAILED | EXCLUDED | DUAL_ORACLE |\n|---|---|---|---|---|---|---|---|---|\n");
    for (pair, words) in &pairs {
        let n = |w: &str| words.get(w).copied().unwrap_or(0);
        out.push_str(&format!(
            "| `{pair}` | {} | {} | {} | {} | {} | {} | {} | {} |\n",
            words.values().sum::<usize>(),
            n("MATCH"),
            n("KNOWN"),
            n("DIFF"),
            n("COUNT"),
            n("OPEN_FAILED"),
            n("EXCLUDED"),
            n("DUAL_ORACLE")
        ));
    }

    out.push_str("\n## Everything that is not a MATCH\n\n");
    let mut any = false;
    for pair in pairs.keys() {
        let rows: Vec<&Row> = table
            .rows
            .iter()
            .filter(|r| r.pair == *pair && r.verdict != "MATCH")
            .collect();
        if rows.is_empty() {
            continue;
        }
        any = true;
        out.push_str(&format!(
            "### `{pair}`\n\n| fixture | verdict | issue | reason |\n|---|---|---|---|\n"
        ));
        for r in rows {
            out.push_str(&format!(
                "| `{}` | {} | {} | {} |\n",
                r.fixture,
                r.verdict,
                r.issue,
                r.reason.replace('|', "\\|").replace('\n', " ")
            ));
        }
        out.push('\n');
    }
    if !any {
        out.push_str("Every row is a MATCH.\n");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_word_round_trips() {
        for v in [
            Verdict::Match,
            Verdict::Known(2),
            Verdict::Diff(14),
            Verdict::Count(5, 6),
            Verdict::OpenFailed,
            Verdict::Excluded,
            Verdict::DualOracle,
        ] {
            assert_eq!(Verdict::parse(&v.to_string()), Some(v));
        }
        assert_eq!(Verdict::parse("KNOWN"), None);
        assert_eq!(Verdict::parse("DIFF:x"), None);
        assert_eq!(Verdict::parse("COUNT:1"), None);
        assert_eq!(Verdict::parse("match"), None);
    }

    fn row(f: &str, p: &str, v: &str, reason: &str, issue: &str) -> Row {
        Row {
            fixture: f.into(),
            pair: p.into(),
            verdict: v.into(),
            reason: reason.into(),
            issue: issue.into(),
            first: String::new(),
        }
    }

    #[test]
    fn a_row_that_is_not_a_match_needs_a_reason_and_an_issue() {
        let t = Table {
            runs: vec![],
            rows: vec![
                row("a", "oracle/acadrust", "MATCH", "", ""),
                row("b", "oracle/acadrust", "DIFF:1", "", "x#1"),
                row("c", "oracle/acadrust", "KNOWN:1", "why", ""),
                row("d", "oracle/acadrust", "NOPE", "why", "x#1"),
                row("a", "oracle/acadrust", "MATCH", "", ""),
            ],
        };
        let r = t.refusals();
        assert_eq!(r.len(), 4, "{r:#?}");
        assert!(
            r.iter()
                .any(|l| l.contains("b oracle/acadrust is DIFF:1 and has no reason"))
        );
        assert!(
            r.iter()
                .any(|l| l.contains("c oracle/acadrust is KNOWN:1 and has no issue"))
        );
        assert!(r.iter().any(|l| l.contains("not a verdict")));
        assert!(r.iter().any(|l| l.contains("appears twice")));
    }

    #[test]
    fn a_moved_verdict_names_the_row_both_verdicts_and_first() {
        let t = Table {
            runs: vec![],
            rows: vec![
                row("a", "p", "MATCH", "", ""),
                row("b", "p", "DIFF:2", "r", "i#1"),
                row("gone", "p", "MATCH", "", ""),
            ],
        };
        let mut actual = BTreeMap::new();
        actual.insert(
            ("a".to_string(), "p".to_string()),
            ("DIFF:1".to_string(), "text.x".to_string()),
        );
        actual.insert(
            ("b".to_string(), "p".to_string()),
            ("DIFF:1".to_string(), String::new()),
        );
        actual.insert(
            ("new".to_string(), "p".to_string()),
            ("MATCH".to_string(), String::new()),
        );
        let moved = t.moved(&actual);
        assert_eq!(moved.len(), 4, "{moved:#?}");
        assert!(moved[0].contains("committed MATCH, run says DIFF:1; first: text.x"));
        assert!(moved[1].contains("committed DIFF:2, run says DIFF:1"));
        assert!(
            moved
                .iter()
                .any(|l| l.contains("new p: no row is committed"))
        );
        assert!(moved.iter().any(|l| l.contains("gone p: committed as MATCH and the run produced no such pair")));
        assert!(
            Table {
                runs: vec![],
                rows: vec![row("a", "p", "MATCH", "", "")]
            }
            .moved(
                &[(
                    ("a".to_string(), "p".to_string()),
                    ("MATCH".to_string(), String::new())
                )]
                .into_iter()
                .collect()
            )
            .is_empty()
        );
    }

    #[test]
    fn the_table_renders_and_parses_back_to_itself() {
        let t = Table {
            runs: vec![Run {
                arch: "x86_64".into(),
                archive: "3.7.1-viprs.1".into(),
                acadsharp_rs: "a".repeat(40),
                acadrust: "b".repeat(40),
                date: "2026-10-16".into(),
            }],
            rows: vec![
                row("a", "p", "MATCH", "", ""),
                row("b", "p", "KNOWN:1", "why", "x#1"),
            ],
        };
        assert_eq!(Table::parse(&t.render()).unwrap(), t);
    }

    #[test]
    fn the_summary_is_a_function_of_the_table() {
        let t = Table {
            runs: vec![],
            rows: vec![
                row("a", "p", "MATCH", "", ""),
                row("b", "p", "DIFF:3", "why | with a bar", "x#1"),
            ],
        };
        let s = summary(&t);
        assert_eq!(s, summary(&t.clone()));
        assert!(s.contains("| `p` | 2 | 1 | 0 | 1 | 0 | 0 | 0 | 0 |"), "{s}");
        assert!(s.contains("why \\| with a bar"));
    }
}
