//! The pair tables as data: `parity/TABLES.toml` read into R2.1's [`ClassTable`]s.
//!
//! The comparator's tables are data on purpose (`docs/TOLERANCES.md`), and what a
//! pair excludes or lists as a known divergence is that pair's rows, "added with
//! the run that found them". They live in one committed TOML file rather than in
//! Rust so a reviewer reads the rows and their reasons without reading code, and
//! so a row cannot be added without an issue: [`ClassTable::validate`] refuses
//! one, and so does the loader.
//!
//! ```toml
//! [[warnings]]            # one per pair, explicit, never a default
//! pair = "oracle/acadrust"
//! compare = false
//! reason = "..."
//! issue = "..."
//!
//! [[exclude]]             # a record kind, or one field of one
//! pairs = ["oracle/acadsharp", "acadsharp/acadrust"]
//! record = "text"
//! field = "style"         # omit for the whole kind
//! reason = "..."
//! issue = "..."
//!
//! [[known]]               # a listed divergence that must keep diverging exactly so
//! pair = "oracle/acadrust"
//! fixture = "acadsharp-ac1018"
//! record_type = "text"
//! field = "text"
//! oracle_index = 123      # or: handles = ["1F", "2A"]
//! a = "..."               # the A side's value as recorded
//! b = "..."
//! reason = "..."
//! issue = "..."
//! ```
//!
//! Pairs are written `a/b` in the order the verdict file uses.

use std::collections::BTreeMap;

use serde::Deserialize;

use crate::compare::{
    ClassTable, Engine, Exclusion, KnownDivergence, Pair, RecordKey, Scope, Val, WarningRow,
};

const TABLES: &str = include_str!("../parity/TABLES.toml");

/// `oracle/acadsharp` and friends: ordered, as `parity/VERDICTS.toml` spells them.
pub fn parse_pair(s: &str) -> Result<Pair, String> {
    let engine = |e: &str| match e {
        "oracle" => Ok(Engine::Oracle),
        "acadsharp" => Ok(Engine::Acadsharp),
        "acadrust" => Ok(Engine::Acadrust),
        "ezdxf" => Ok(Engine::Ezdxf),
        other => Err(format!("{other:?} is not an engine")),
    };
    let (a, b) = s
        .split_once('/')
        .ok_or_else(|| format!("{s:?} is not a pair written a/b"))?;
    // `self/<engine>` is an engine against itself, across the seven saves.
    if a == "self" {
        let e = engine(b)?;
        return Ok(Pair::new(e, e));
    }
    Ok(Pair::new(engine(a)?, engine(b)?))
}

/// The spelling [`parse_pair`] reads.
pub fn pair_name(p: Pair) -> String {
    if p.a == p.b {
        return format!("self/{}", p.a.name());
    }
    format!("{}/{}", p.a.name(), p.b.name())
}

#[derive(Debug, Deserialize)]
struct WarningsRow {
    pair: String,
    compare: bool,
    #[serde(default)]
    reason: String,
    #[serde(default)]
    issue: String,
}

#[derive(Debug, Deserialize)]
struct ExcludeRow {
    pairs: Vec<String>,
    record: String,
    #[serde(default)]
    field: Option<String>,
    #[serde(default)]
    reason: String,
    #[serde(default)]
    issue: String,
}

#[derive(Debug, Deserialize)]
struct KnownRow {
    pair: String,
    fixture: String,
    record_type: String,
    field: String,
    #[serde(default)]
    oracle_index: Option<usize>,
    #[serde(default)]
    handles: Option<Vec<String>>,
    a: toml::Value,
    b: toml::Value,
    #[serde(default)]
    reason: String,
    #[serde(default)]
    issue: String,
}

#[derive(Debug, Default, Deserialize)]
struct File {
    #[serde(default)]
    warnings: Vec<WarningsRow>,
    #[serde(default)]
    exclude: Vec<ExcludeRow>,
    #[serde(default)]
    known: Vec<KnownRow>,
}

fn val(v: &toml::Value) -> Result<Val, String> {
    Ok(match v {
        toml::Value::String(s) => Val::Str(s.clone()),
        toml::Value::Integer(i) => {
            Val::Int(u64::try_from(*i).map_err(|_| format!("{i} is negative"))?)
        }
        toml::Value::Boolean(b) => Val::Bool(*b),
        toml::Value::Float(f) => Val::Float(*f),
        other => return Err(format!("{other} is not a string, integer, bool or float")),
    })
}

/// Every pair's table, built from `parity/TABLES.toml`.
pub fn load_all() -> Result<BTreeMap<String, ClassTable>, String> {
    load(TABLES)
}

/// The tables in `text`. Separate from [`load_all`] so a test can feed it a bad
/// file.
pub fn load(text: &str) -> Result<BTreeMap<String, ClassTable>, String> {
    let file: File = toml::from_str(text).map_err(|e| format!("parity/TABLES.toml: {e}"))?;
    let mut tables: BTreeMap<String, ClassTable> = BTreeMap::new();
    for w in &file.warnings {
        let pair = parse_pair(&w.pair)?;
        let row = if w.compare {
            WarningRow::Compare
        } else {
            WarningRow::Excluded {
                reason: w.reason.clone(),
                issue: w.issue.clone(),
            }
        };
        if tables
            .insert(w.pair.clone(), ClassTable::for_pair(pair, row))
            .is_some()
        {
            return Err(format!("the warnings row for {} appears twice", w.pair));
        }
    }
    for x in &file.exclude {
        let scope = match &x.field {
            Some(field) => Scope::Field {
                record: x.record.clone(),
                field: field.clone(),
            },
            None => Scope::Record(x.record.clone()),
        };
        for p in &x.pairs {
            let t = tables
                .get_mut(p)
                .ok_or_else(|| format!("an exclude row names {p}, which has no warnings row"))?;
            t.excluded.push(Exclusion {
                scope: scope.clone(),
                reason: x.reason.clone(),
                issue: x.issue.clone(),
            });
        }
    }
    for k in &file.known {
        let t = tables
            .get_mut(&k.pair)
            .ok_or_else(|| format!("a known row names {}, which has no warnings row", k.pair))?;
        let record = match (&k.oracle_index, &k.handles) {
            (Some(i), None) => RecordKey::OracleIndex(*i),
            (None, Some(h)) => RecordKey::Handles(h.clone()),
            _ => {
                return Err(format!(
                    "the known row {} {}.{} needs exactly one of oracle_index and handles",
                    k.fixture, k.record_type, k.field
                ));
            }
        };
        t.known.push(KnownDivergence {
            fixture: k.fixture.clone(),
            record,
            record_type: k.record_type.clone(),
            field: k.field.clone(),
            a: val(&k.a)?,
            b: val(&k.b)?,
            reason: k.reason.clone(),
            issue: k.issue.clone(),
        });
    }
    for t in tables.values() {
        t.validate()
            .map_err(|e| format!("{}: {e}", pair_name(t.pair)))?;
    }
    Ok(tables)
}

#[cfg(test)]
mod tests {
    use super::*;

    const W: &str = "[[warnings]]\npair = \"oracle/acadrust\"\ncompare = false\nreason = \"r\"\nissue = \"i#1\"\n";

    #[test]
    fn a_pair_is_a_slash_b_and_nothing_else() {
        assert_eq!(
            parse_pair("oracle/acadsharp").unwrap(),
            Pair::new(Engine::Oracle, Engine::Acadsharp)
        );
        assert_eq!(
            pair_name(parse_pair("acadsharp/acadrust").unwrap()),
            "acadsharp/acadrust"
        );
        assert!(parse_pair("oracle-acadsharp").is_err());
        let me = parse_pair("self/acadrust").unwrap();
        assert_eq!(me, Pair::new(Engine::Acadrust, Engine::Acadrust));
        assert_eq!(pair_name(me), "self/acadrust");
        assert!(parse_pair("oracle/nobody").is_err());
    }

    #[test]
    fn the_committed_tables_load_and_validate() {
        load_all().unwrap();
    }

    #[test]
    fn an_exclusion_without_an_issue_is_refused_at_load() {
        let text = format!(
            "{W}\n[[exclude]]\npairs=[\"oracle/acadrust\"]\nrecord=\"point\"\nreason=\"r\"\n"
        );
        let err = load(&text).unwrap_err();
        assert!(err.contains("no issue"), "{err}");
    }

    #[test]
    fn a_row_for_a_pair_with_no_warnings_row_is_refused() {
        let text = "[[exclude]]\npairs=[\"oracle/acadrust\"]\nrecord=\"point\"\nreason=\"r\"\nissue=\"i#1\"\n";
        assert!(load(text).unwrap_err().contains("no warnings row"));
    }

    #[test]
    fn a_known_row_needs_exactly_one_way_to_find_its_record() {
        let text = format!(
            "{W}\n[[known]]\npair=\"oracle/acadrust\"\nfixture=\"f\"\nrecord_type=\"text\"\nfield=\"text\"\na=\"x\"\nb=\"y\"\nreason=\"r\"\nissue=\"i#1\"\n"
        );
        assert!(load(&text).unwrap_err().contains("exactly one"));
        let ok = format!("{text}oracle_index = 4\n");
        let tables = load(&ok).unwrap();
        assert_eq!(tables["oracle/acadrust"].known.len(), 1);
        assert_eq!(tables["oracle/acadrust"].known[0].a, Val::Str("x".into()));
    }
}
