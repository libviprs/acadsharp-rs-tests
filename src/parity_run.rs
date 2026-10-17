//! The parity run's logic that needs no process: refuse a vacuous comparison,
//! compare one pair under its table, and turn the report into a verdict.
//!
//! `tests/parity.rs` is the part that runs the emitters and reads the frozen
//! files; everything it decides is decided here, so the end-to-end control (one
//! radius times 1.001 is `DIFF:1` through the same path) can drive it without a
//! single process.

use std::collections::BTreeMap;

use crate::compare::{ClassTable, Finding, Stream, compare};
use crate::verdict::{Verdict, judge, records_of};

/// One side of a pair: the stream, and the file it was read from. The label is
/// what makes "the same stream handed in as both sides" detectable, since two
/// engines that happen to agree byte for byte are a legitimate `MATCH` and two
/// reads of one file are not.
#[derive(Debug, Clone)]
pub struct Side {
    pub label: String,
    pub stream: Stream,
}

/// What a comparison came to.
#[derive(Debug, Clone)]
pub struct Judged {
    pub verdict: Verdict,
    /// The comparator's rendering of the first differing record.
    pub first: String,
    /// Every finding that counts against the verdict, rendered. For
    /// `PARITY_EXPLAIN=1`, which prints them; the verdict file keeps `first`.
    pub red: Vec<String>,
}

/// Compare `a` with `b` under the table named `pair` (`oracle/acadrust`).
///
/// Refuses, rather than compares: a side with no records (an empty stream
/// agrees with anything that is also empty, which is the shape of two readers
/// that both read nothing), the same file on both sides, and a pair no table
/// exists for.
pub fn judge_pair(
    fixture: &str,
    pair: &str,
    a: &Side,
    b: &Side,
    tables: &BTreeMap<String, ClassTable>,
) -> Result<Judged, String> {
    for (name, side) in [("A", a), ("B", b)] {
        if side.stream.records.is_empty() {
            return Err(format!(
                "{fixture} {pair}: side {name} ({}) has no records, so comparing it proves nothing",
                side.label
            ));
        }
    }
    if a.label == b.label {
        return Err(format!(
            "{fixture} {pair}: both sides are {}, so the comparison is vacuous",
            a.label
        ));
    }
    let table = tables.get(pair).ok_or_else(|| {
        format!("{fixture} {pair}: parity/TABLES.toml has no table for this pair")
    })?;
    let report = compare(fixture, &a.stream, &b.stream, table).map_err(|e| e.to_string())?;
    let (verdict, first) = judge(&report);
    Ok(Judged {
        verdict,
        first: first.unwrap_or_default(),
        red: report.red().iter().map(|f| f.render()).collect(),
    })
}

/// [`judge_pair`] for an engine against itself across two saves, where a
/// difference the format cannot carry is listed in an exception list.
///
/// `explained` says whether a red finding is one of those. The ones that are
/// make the verdict `KNOWN:n` (n records), and any that are not make it `DIFF:n`
/// over those records alone: an unlisted difference is not hidden by listed ones
/// beside it.
pub fn judge_self(
    fixture: &str,
    pair: &str,
    a: &Side,
    b: &Side,
    tables: &BTreeMap<String, ClassTable>,
    explained: &dyn Fn(&Finding) -> bool,
) -> Result<Judged, String> {
    let plain = judge_pair(fixture, pair, a, b, tables)?;
    let table = &tables[pair];
    let report = compare(fixture, &a.stream, &b.stream, table).map_err(|e| e.to_string())?;
    let red = report.red();
    if red.is_empty() {
        return Ok(plain);
    }
    let (known, open): (Vec<&Finding>, Vec<&Finding>) = red.into_iter().partition(|f| explained(f));
    let first = open
        .first()
        .or(known.first())
        .map(|f| f.render())
        .unwrap_or_default();
    let red_text: Vec<String> = open.iter().map(|f| f.render()).collect();
    let verdict = if !open.is_empty() {
        Verdict::Diff(records_of(open.into_iter()))
    } else {
        Verdict::Known(records_of(known.into_iter()))
    };
    Ok(Judged {
        verdict,
        first,
        red: red_text,
    })
}

/// `OPEN_FAILED`'s reason: the exit code and the first line of stderr.
pub fn open_failed_reason(code: Option<i32>, stderr: &str) -> String {
    let line = stderr.lines().find(|l| !l.trim().is_empty()).unwrap_or("");
    match code {
        Some(c) => format!("exit {c}: {line}"),
        None => format!("killed by a signal: {line}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canonical::{Circle, Document, Record, View};
    use crate::tables;

    fn stream(radius: f64) -> Stream {
        Stream::new(vec![
            Record::Document(Document {
                format: "dwg".into(),
                acad_version: "AC1032".into(),
                maintenance_version: 0,
            }),
            Record::View(View {
                index: 0,
                name: "Model".into(),
                kind: "model".into(),
            }),
            Record::Circle(Circle {
                view: 0,
                center: [1.0, 2.0, 0.0],
                radius,
                normal: [0.0, 0.0, 1.0],
            }),
        ])
    }

    fn side(label: &str, radius: f64) -> Side {
        Side {
            label: label.into(),
            stream: stream(radius),
        }
    }

    fn tables() -> BTreeMap<String, ClassTable> {
        tables::load_all().unwrap()
    }

    #[test]
    fn equal_streams_match_and_a_radius_times_1_001_is_one_diff() {
        let t = tables();
        let ok = judge_pair("f", "oracle/acadrust", &side("o", 5.0), &side("r", 5.0), &t).unwrap();
        assert_eq!(ok.verdict, Verdict::Match);
        let bad = judge_pair(
            "f",
            "oracle/acadrust",
            &side("o", 5.0 * 1.001),
            &side("r", 5.0),
            &t,
        )
        .unwrap();
        assert_eq!(bad.verdict, Verdict::Diff(1));
        assert!(bad.first.contains("circle.radius"), "{}", bad.first);
    }

    #[test]
    fn an_empty_stream_is_refused_not_compared() {
        let t = tables();
        let empty = Side {
            label: "e".into(),
            stream: Stream::new(vec![]),
        };
        let err = judge_pair("f", "oracle/acadrust", &empty, &side("r", 1.0), &t).unwrap_err();
        assert!(err.contains("has no records"), "{err}");
        let err = judge_pair("f", "oracle/acadrust", &side("o", 1.0), &empty, &t).unwrap_err();
        assert!(err.contains("has no records"), "{err}");
    }

    #[test]
    fn the_same_file_on_both_sides_is_refused_as_vacuous() {
        let t = tables();
        let err =
            judge_pair("f", "oracle/acadrust", &side("x", 1.0), &side("x", 1.0), &t).unwrap_err();
        assert!(err.contains("vacuous"), "{err}");
    }

    #[test]
    fn a_pair_with_no_table_is_refused() {
        let t = tables();
        let err = judge_pair("f", "ezdxf/ezdxf", &side("a", 1.0), &side("b", 1.0), &t).unwrap_err();
        assert!(err.contains("no table"), "{err}");
    }

    #[test]
    fn open_failed_carries_the_exit_code_and_the_first_line() {
        assert_eq!(open_failed_reason(Some(1), "\nboom\nmore"), "exit 1: boom");
        assert!(open_failed_reason(None, "").starts_with("killed"));
    }
}
