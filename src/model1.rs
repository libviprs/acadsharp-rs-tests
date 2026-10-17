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

/// A header variable's value, in the one written form per type.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Str(String),
    Int(i64),
    Double(f64),
    Bool(bool),
    Point2([f64; 2]),
    Point3([f64; 3]),
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
        })
    }

    /// The value as it is shown in a finding.
    pub fn show(&self) -> String {
        self.render().unwrap_or_else(|e| format!("<{e}>"))
    }
}

/// One `header` record.
#[derive(Debug, Clone, PartialEq)]
pub struct Header {
    /// The DXF name, `$LTSCALE`.
    pub name: String,
    pub value: Value,
}

/// A model-1 record. Only `header` so far; every further kind is one more
/// variant and one more section of docs/MODEL_1.md.
#[derive(Debug, Clone, PartialEq)]
pub enum Record {
    Header(Header),
}

impl Record {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Header(_) => "header",
        }
    }

    /// The key a record is compared under, inside its kind.
    pub fn key(&self) -> &str {
        match self {
            Self::Header(h) => &h.name,
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
        _ => return Err(format!("{name}: no written form is defined for DXF code {code}")),
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

const KIND_ORDER: [&str; 1] = ["header"];

/// The exact text of a `.model1.jsonl`: sorted by (kind order, key), LF, a final
/// newline. Two records with one key are refused.
pub fn render_stream(records: &[Record]) -> Result<String, String> {
    let mut sorted: Vec<&Record> = records.iter().collect();
    sorted.sort_by(|a, b| {
        let ka = KIND_ORDER.iter().position(|k| *k == a.kind());
        let kb = KIND_ORDER.iter().position(|k| *k == b.kind());
        ka.cmp(&kb).then_with(|| a.key().as_bytes().cmp(b.key().as_bytes()))
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
        }
    }
    Ok(out)
}

// ---- reading ------------------------------------------------------------------------

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
        let keys: Vec<&str> = obj.keys().map(String::as_str).collect();
        // serde_json without preserve_order sorts keys; check the field set, and
        // the order against the raw text.
        let rec = obj.get("record").and_then(Json::as_str).unwrap_or("");
        let want: &[&str] = match rec {
            "header" => &["model", "name", "record", "type", "value"],
            other => return Err(format!("line {n}: {other:?} is not a model-1 record")),
        };
        if keys != want {
            return Err(format!("line {n}: fields {keys:?}, want {want:?}"));
        }
        if obj["model"] != Json::from(1) {
            return Err(format!("line {n}: model is not 1"));
        }
        let order = "{\"model\":1,\"record\":\"header\",\"name\":";
        if !l.starts_with(order) || !l.contains(",\"type\":\"") || !l.contains(",\"value\":") {
            return Err(format!("line {n}: fields are not in the order the document gives"));
        }
        let name = obj["name"].as_str().ok_or(format!("line {n}: name"))?.to_string();
        let ty = obj["type"].as_str().ok_or(format!("line {n}: type"))?;
        let val = &obj["value"];
        let f = |j: &Json| j.as_f64().ok_or(format!("line {n}: not a number"));
        let value = match ty {
            "string" => Value::Str(val.as_str().ok_or(format!("line {n}: not a string"))?.into()),
            "int" => Value::Int(val.as_i64().ok_or(format!("line {n}: not an integer"))?),
            "double" => Value::Double(f(val)?),
            "bool" => Value::Bool(val.as_bool().ok_or(format!("line {n}: not a bool"))?),
            "point2" | "point3" => {
                let a = val.as_array().ok_or(format!("line {n}: not an array"))?;
                match (ty, a.len()) {
                    ("point2", 2) => Value::Point2([f(&a[0])?, f(&a[1])?]),
                    ("point3", 3) => Value::Point3([f(&a[0])?, f(&a[1])?, f(&a[2])?]),
                    _ => return Err(format!("line {n}: {ty} with {} components", a.len())),
                }
            }
            other => return Err(format!("line {n}: type {other:?}")),
        };
        let r = Record::Header(Header { name, value });
        if let Some(prev) = out.last() {
            let ord = prev.key().as_bytes().cmp(r.key().as_bytes());
            if ord != std::cmp::Ordering::Less {
                return Err(format!("line {n}: {} is not after {}", r.key(), prev.key()));
            }
        }
        // Round trip: the line must be the one the writer would write.
        let again = render_stream(std::slice::from_ref(&r))?;
        if again.trim_end() != l {
            return Err(format!("line {n}: not in canonical form; the writer gives {}", again.trim_end()));
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
                return Err(format!("known row {:?} needs a reason and an issue", k.names));
            }
        }
        for e in &r.excluded {
            if e.reason.trim().is_empty() || e.issue.trim().is_empty() {
                return Err(format!("excluded row {} {:?} needs a reason and an issue", e.engine, e.names));
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
                && e.names.iter().any(|n| n == name)
                && (e.fixtures.is_empty() || e.fixtures.iter().any(|f| f == fixture))
        })
    }

    /// The row that lists a difference of `name` for this pair and fixture.
    pub fn known_for(&self, pair: &str, fixture: &str, name: &str) -> Option<&Known> {
        self.known.iter().find(|k| {
            k.names.iter().any(|n| n == name)
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
    Ok(toml::from_str::<FrozenManifest>(&text).map_err(|e| e.to_string())?.file)
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
        Sha256::digest(&bytes).iter().map(|b| format!("{b:02x}")).collect()
    };
    if got != f.sha256 {
        return Err(format!("{}: hashes to {got}, the manifest froze {}", path.display(), f.sha256));
    }
    let text = String::from_utf8(bytes).map_err(|e| e.to_string())?;
    parse_stream(&text).map_err(|e| format!("{}: {e}", path.display()))
}

/// The committed `parity/MODEL1.toml`.
pub fn committed_rules() -> Result<Rules, String> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("parity").join("MODEL1.toml");
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
}

impl fmt::Display for Finding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "header {} {} [{:?}]", self.key, self.detail, self.disposition)
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
        self.findings.iter().filter(|f| f.disposition == Disposition::Red).collect()
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
        for f in self.findings.iter().filter(|f| f.disposition == Disposition::Known) {
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

/// Compare two streams by key. `pair` is `oracle/acadrust` and so on, the name
/// `parity/MODEL1.toml` rows use.
pub fn compare(pair: &str, a: &Side, b: &Side, rules: &Rules) -> Report {
    let index = |s: &Side| -> BTreeMap<String, Value> {
        s.records
            .iter()
            .map(|Record::Header(h)| (h.name.clone(), h.value.clone()))
            .collect()
    };
    let (ia, ib) = (index(a), index(b));
    let names: BTreeSet<&String> = ia.keys().chain(ib.keys()).collect();
    let mut report = Report::default();
    for name in names {
        match (ia.get(name), ib.get(name)) {
            (Some(x), Some(y)) => {
                report.compared += 1;
                let known = rules.known_for(pair, a.fixture, name);
                let (kind, detail) = if x.type_name() != y.type_name() {
                    (FindingKind::Type, format!("type {} vs {}", x.type_name(), y.type_name()))
                } else if !same(x, y) {
                    (FindingKind::Value, format!("{} vs {}", x.show(), y.show()))
                } else {
                    continue;
                };
                report.findings.push(Finding {
                    kind,
                    key: name.clone(),
                    detail,
                    disposition: if known.is_some() { Disposition::Known } else { Disposition::Red },
                    issue: known.map(|k| k.issue.clone()).unwrap_or_default(),
                });
            }
            (Some(x), None) => report.findings.push(Finding {
                kind: FindingKind::OnlyInA,
                key: name.clone(),
                detail: format!("{} = {} on {}, not on {}", name, x.show(), a.engine, b.engine),
                disposition: if rules.excluded_for(b.engine, b.fixture, name).is_some() {
                    Disposition::Excluded
                } else {
                    Disposition::Red
                },
                issue: rules
                    .excluded_for(b.engine, b.fixture, name)
                    .map(|e| e.issue.clone())
                    .unwrap_or_default(),
            }),
            (None, Some(y)) => report.findings.push(Finding {
                kind: FindingKind::OnlyInB,
                key: name.clone(),
                detail: format!("{} = {} on {}, not on {}", name, y.show(), b.engine, a.engine),
                disposition: if rules.excluded_for(a.engine, a.fixture, name).is_some() {
                    Disposition::Excluded
                } else {
                    Disposition::Red
                },
                issue: rules
                    .excluded_for(a.engine, a.fixture, name)
                    .map(|e| e.issue.clone())
                    .unwrap_or_default(),
            }),
            (None, None) => unreachable!("a name comes from one of the two"),
        }
    }
    report
}

#[cfg(test)]
mod tests {
    use super::*;

    fn h(name: &str, v: Value) -> Record {
        Record::Header(Header { name: name.into(), value: v })
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
        assert_eq!(names, ["$CLAYER", "$EXTMIN", "$LIMMIN", "$LTSCALE", "$ORTHOMODE", "$OSMODE"]);
        assert_eq!(render_stream(&back).unwrap(), text);
        assert!(text.starts_with("{\"model\":1,\"record\":\"header\",\"name\":\"$CLAYER\""));
    }

    #[test]
    fn the_parser_is_strict() {
        let ok = "{\"model\":1,\"record\":\"header\",\"name\":\"$A\",\"type\":\"int\",\"value\":1}\n";
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
        let two = format!(
            "{}{}",
            ok.replace("$A", "$B"),
            ok
        );
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
        Side { engine, fixture: "f", records }
    }

    #[test]
    fn names_compare_by_key_not_by_position() {
        let a = vec![h("$A", Value::Int(1)), h("$B", Value::Int(2))];
        let b = vec![h("$B", Value::Int(2)), h("$A", Value::Int(1))];
        let r = compare("oracle/acadrust", &side("oracle", &a), &side("acadrust", &b), &Rules::default());
        assert_eq!(r.verdict(), Verdict::Match);
        assert_eq!(r.compared, 2);
    }

    #[test]
    fn a_double_inside_the_bound_matches_and_outside_it_is_one_diff() {
        let a = vec![h("$LTSCALE", Value::Double(1.0))];
        let near = vec![h("$LTSCALE", Value::Double(1.0 + 1e-8))];
        let far = vec![h("$LTSCALE", Value::Double(1.001))];
        let rules = Rules::default();
        let m = compare("oracle/acadrust", &side("oracle", &a), &side("acadrust", &near), &rules);
        assert_eq!(m.verdict(), Verdict::Match);
        let d = compare("oracle/acadrust", &side("oracle", &a), &side("acadrust", &far), &rules);
        assert_eq!(d.verdict(), Verdict::Diff(1));
        assert!(d.first().contains("$LTSCALE"));
    }

    #[test]
    fn a_name_on_one_side_is_red_unless_the_rules_exclude_it_for_that_engine() {
        let a = vec![h("$A", Value::Int(1)), h("$B", Value::Int(2))];
        let b = vec![h("$A", Value::Int(1))];
        let none = Rules::default();
        let r = compare("oracle/acadrust", &side("oracle", &a), &side("acadrust", &b), &none);
        assert_eq!(r.verdict(), Verdict::Diff(1));
        let rules = Rules::parse(
            "[[excluded]]\nengine = \"acadrust\"\nnames = [\"$B\"]\nreason = \"no field\"\nissue = \"x#1\"\n",
        )
        .unwrap();
        let r = compare("oracle/acadrust", &side("oracle", &a), &side("acadrust", &b), &rules);
        assert_eq!(r.verdict(), Verdict::Match);
        assert_eq!(r.findings.len(), 1);
        // and the same row does not excuse the oracle lacking a name acadrust has
        let r = compare("oracle/acadrust", &side("oracle", &b), &side("acadrust", &a), &rules);
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
        let r = compare("ezdxf/oracle", &side("ezdxf", &a), &side("oracle", &b), &rules);
        assert_eq!(r.verdict(), Verdict::Known(1));
        let r = compare("oracle/acadrust", &side("oracle", &a), &side("acadrust", &b), &rules);
        assert_eq!(r.verdict(), Verdict::Diff(1));
    }

    #[test]
    fn a_rule_without_a_reason_or_an_issue_is_refused() {
        assert!(Rules::parse("[[known]]\nnames = [\"$A\"]\nreason = \"\"\nissue = \"x\"\n").is_err());
        assert!(Rules::parse("[[excluded]]\nengine = \"oracle\"\nnames = [\"$A\"]\nreason = \"r\"\nissue = \"\"\n").is_err());
        assert!(Rules::parse("[[excluded]]\nengine = \"nobody\"\nnames = [\"$A\"]\nreason = \"r\"\nissue = \"i\"\n").is_err());
    }
}
