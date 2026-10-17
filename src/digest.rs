//! The per-archive, per-target digest tables under `parity/emit/`.
//!
//! The parity table (`parity/VERDICTS.toml`) is one table for every architecture,
//! because verdicts are arch-independent by contract. What is not independent is
//! the emitted JSONL itself: a float can round differently on x64 and arm64, and
//! two archives that both call themselves `3.7.1-viprs.N` are different shims.
//! So the run writes down, per archive and per target, what each fixture came to:
//! the sha256 of the JSONL and the record count by kind. The JSONL is not
//! committed, the digest is, and a run on the same archive and target must
//! reproduce it exactly.
//!
//! A table is `parity/emit/<artifact_version>+<library sha8>.<target>.tsv`:
//!
//! ```text
//! # archive 3.7.1-viprs.1 library 5c982373... target x86_64-linux
//! fixture<TAB>sha256<TAB>records<TAB>kind=count,kind=count
//! ```

use std::collections::BTreeMap;

use sha2::{Digest, Sha256};

use crate::canonical::parse_stream;

/// One fixture's line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub fixture: String,
    pub sha256: String,
    pub records: usize,
    pub kinds: BTreeMap<String, usize>,
}

/// The digest of an emitted JSONL stream.
pub fn row(fixture: &str, jsonl: &str) -> Result<Row, String> {
    let records = parse_stream(jsonl).map_err(|e| format!("{fixture}: {e}"))?;
    let mut kinds: BTreeMap<String, usize> = BTreeMap::new();
    for r in &records {
        *kinds.entry(r.kind().to_string()).or_default() += 1;
    }
    let sha = Sha256::digest(jsonl.as_bytes());
    Ok(Row {
        fixture: fixture.to_string(),
        sha256: sha.iter().map(|b| format!("{b:02x}")).collect(),
        records: records.len(),
        kinds,
    })
}

/// A table's file name for an archive on a target.
pub fn file_name(artifact_version: &str, library_sha256: &str, target: &str) -> String {
    let sha8: String = library_sha256.chars().take(8).collect();
    format!("{artifact_version}+{sha8}.{target}.tsv")
}

pub fn render(header: &str, rows: &[Row]) -> String {
    let mut out = format!("# {header}\n");
    let mut sorted: Vec<&Row> = rows.iter().collect();
    sorted.sort_by(|a, b| a.fixture.cmp(&b.fixture));
    for r in sorted {
        let kinds: Vec<String> = r.kinds.iter().map(|(k, n)| format!("{k}={n}")).collect();
        out.push_str(&format!(
            "{}\t{}\t{}\t{}\n",
            r.fixture,
            r.sha256,
            r.records,
            kinds.join(",")
        ));
    }
    out
}

pub fn parse(text: &str) -> Result<Vec<Row>, String> {
    let mut out = Vec::new();
    for (n, line) in text.lines().enumerate() {
        if line.starts_with('#') || line.trim().is_empty() {
            continue;
        }
        let cells: Vec<&str> = line.split('\t').collect();
        if cells.len() != 4 {
            return Err(format!("line {}: expected four tab-separated cells", n + 1));
        }
        let mut kinds = BTreeMap::new();
        for kv in cells[3].split(',').filter(|s| !s.is_empty()) {
            let (k, v) = kv
                .split_once('=')
                .ok_or_else(|| format!("line {}: {kv:?}", n + 1))?;
            kinds.insert(
                k.to_string(),
                v.parse().map_err(|_| format!("line {}: {kv:?}", n + 1))?,
            );
        }
        out.push(Row {
            fixture: cells[0].to_string(),
            sha256: cells[1].to_string(),
            records: cells[2]
                .parse()
                .map_err(|_| format!("line {}: records", n + 1))?,
            kinds,
        });
    }
    Ok(out)
}

/// Where `actual` departs from `committed`, as a line each.
pub fn differences(committed: &[Row], actual: &[Row]) -> Vec<String> {
    let want: BTreeMap<&str, &Row> = committed.iter().map(|r| (r.fixture.as_str(), r)).collect();
    let got: BTreeMap<&str, &Row> = actual.iter().map(|r| (r.fixture.as_str(), r)).collect();
    let mut out = Vec::new();
    for (id, g) in &got {
        match want.get(id) {
            None => out.push(format!("{id}: no committed digest")),
            Some(w) if w.sha256 != g.sha256 => out.push(format!(
                "{id}: sha256 {} against the committed {} ({} records against {}; kinds {:?} against {:?})",
                g.sha256, w.sha256, g.records, w.records, g.kinds, w.kinds
            )),
            Some(_) => {}
        }
    }
    for id in want.keys() {
        if !got.contains_key(id) {
            out.push(format!("{id}: committed and not produced"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const STREAM: &str = "{\"schema\":1,\"record\":\"document\",\"format\":\"dwg\",\"acad_version\":\"AC1032\",\"maintenance_version\":0}\n\
{\"schema\":1,\"record\":\"view\",\"index\":0,\"name\":\"Model\",\"kind\":\"model\"}\n";

    #[test]
    fn a_row_counts_kinds_and_hashes_the_bytes() {
        let r = row("f", STREAM).unwrap();
        assert_eq!(r.records, 2);
        assert_eq!(r.kinds.get("document"), Some(&1));
        assert_eq!(r.sha256.len(), 64);
        assert_ne!(
            r.sha256,
            row("f", &STREAM.replace("Model", "Modeo")).unwrap().sha256
        );
    }

    #[test]
    fn a_table_round_trips_and_names_what_moved() {
        let a = row("a", STREAM).unwrap();
        let b = row("b", STREAM).unwrap();
        let text = render("archive x", &[b.clone(), a.clone()]);
        assert!(text.starts_with("# archive x\n"));
        let back = parse(&text).unwrap();
        assert_eq!(back, vec![a.clone(), b.clone()]);
        let mut moved = b.clone();
        moved.sha256 = "0".repeat(64);
        let d = differences(&back, &[a, moved]);
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].starts_with("b: sha256 "), "{d:?}");
        assert_eq!(differences(&back, &[]).len(), 2);
    }

    #[test]
    fn the_file_name_carries_the_version_the_library_and_the_target() {
        assert_eq!(
            file_name(
                "3.7.1-viprs.1",
                "5c982373ac60d83840d476e89b7a04e6e61bf6168307786c90471096da97cc42",
                "x86_64-linux"
            ),
            "3.7.1-viprs.1+5c982373.x86_64-linux.tsv"
        );
    }
}
