//! Cross-version invariance of the oracle's frozen outputs (R3.4).
use std::collections::BTreeSet;

use acadsharp_rs_tests::canonical::parse_stream;
use acadsharp_rs_tests::compare::{ClassTable, Engine, Exclusion, Pair, Scope, Stream, WarningRow, compare};
use acadsharp_rs_tests::fixtures;

fn oracle(id: &str) -> Stream {
    let bytes = fixtures::by_id(id).unwrap().load_reference_jsonl().unwrap();
    Stream::new(parse_stream(&String::from_utf8(bytes).unwrap()).unwrap())
}

#[test]
#[ignore = "report"]
fn report() {
    let mut table = ClassTable::for_pair(
        Pair::new(Engine::Oracle, Engine::Oracle),
        WarningRow::Excluded { reason: "r".into(), issue: "i".into() },
    );
    for f in ["acad_version", "maintenance_version"] {
        table.excluded.push(Exclusion {
            scope: Scope::Field { record: "document".into(), field: f.into() },
            reason: "r".into(),
            issue: "i".into(),
        });
    }
    let base = oracle("acadsharp-ac1032");
    let non_warning = base.records.iter().filter(|r| r.kind() != "warning").count();
    println!("ac1032 non-warning records: {non_warning}");
    for v in ["1014", "1015", "1018", "1021", "1024", "1027"] {
        let other = oracle(&format!("acadsharp-ac{v}"));
        let n = other.records.iter().filter(|r| r.kind() != "warning").count();
        let report = compare("x", &base, &other, &table).unwrap();
        let idx: BTreeSet<usize> = report.red().iter().filter_map(|f| f.index_a.or(f.index_b)).collect();
        println!("ac{v}: {n} non-warning records, {} findings over {} records", report.red().len(), idx.len());
        for f in report.red() {
            println!("   {}", f.render());
        }
    }
}
