//! The perturbation sweep, over real frozen output (R2.1, #19).
//!
//! Every float scalar in a frozen stream, perturbed by twice its effective
//! bound, one at a time, has to produce exactly one mismatch and leave the
//! alignment exactly where the unperturbed run had it. A comparator that
//! compares zero fields passes the identical-input case and fails here, which is
//! why this exists: the matrix in `comparator.rs` is hand-written and small,
//! this is the whole corpus.
//!
//! The scalar count is written out by hand for one fixture, counted from the
//! frozen stream with a script that shares no code with this one, so a sweep
//! over zero fields cannot pass by agreeing with itself.

mod support;

use acadsharp_rs_tests::canonical::{Record, parse_stream};
use acadsharp_rs_tests::compare::{
    BOUNDED_TIGHT, Class, FindingKind, Stream, compare,
};
use acadsharp_rs_tests::fixtures;
use support::tight;

/// Float scalars in `acadsharp-ac1032`'s frozen stream: every JSON number
/// written with a fractional part, in every record, including each coordinate
/// inside `points`, `bulges`, `control_points`, `knots` and `weights`. Counted
/// outside Rust.
const AC1032_FLOATS: usize = 2155;

fn frozen(id: &str) -> Vec<Record> {
    let fixture = fixtures::by_id(id).unwrap_or_else(|| panic!("no fixture {id}"));
    let text = String::from_utf8(fixture.load_reference_jsonl().expect("frozen output")).unwrap();
    parse_stream(&text).unwrap_or_else(|e| panic!("{id}: {e}"))
}

fn float_count(records: &[Record]) -> usize {
    let mut copy = records.to_vec();
    copy.iter_mut().map(|r| r.floats_mut().len()).sum()
}

/// Perturb the `k`-th float of record `r` by `factor` times its effective bound.
fn perturbed(records: &[Record], r: usize, k: usize, factor: f64) -> Vec<Record> {
    let mut out = records.to_vec();
    let slot = out[r].floats_mut().swap_remove(k);
    let bound = BOUNDED_TIGHT.bound(*slot, *slot);
    *slot += factor * bound;
    out
}

/// Every (record, float) address, so the sweep visits each exactly once.
fn addresses(records: &[Record]) -> Vec<(usize, usize)> {
    let mut copy = records.to_vec();
    copy.iter_mut()
        .enumerate()
        .flat_map(|(r, rec)| (0..rec.floats_mut().len()).map(move |k| (r, k)))
        .collect()
}

fn sweep(id: &str) -> usize {
    let base = frozen(id);
    let table = tight();
    let a = Stream::new(base.clone());
    let unperturbed = compare(id, &a, &a, &table).expect("table");
    assert!(unperturbed.findings.is_empty(), "{id}: identical streams differ");
    assert_eq!(unperturbed.aligned.len(), base.len(), "{id}: identical streams must align fully");
    assert_eq!(
        unperturbed.floats_compared,
        float_count(&base),
        "{id}: the comparator did not look at every float"
    );

    let addresses = addresses(&base);
    for &(r, k) in &addresses {
        let b = Stream::new(perturbed(&base, r, k, 2.0));
        let report = compare(id, &a, &b, &table).expect("table");
        let kinds: Vec<_> = report.findings.iter().map(|f| f.render()).collect();
        assert_eq!(
            report.findings.len(),
            1,
            "{id}: perturbing float {k} of record {r} by twice its bound gave {} findings: {kinds:#?}",
            report.findings.len()
        );
        let f = &report.findings[0];
        assert_eq!(f.kind, FindingKind::FieldMismatch, "{id}: record {r} float {k}");
        assert_eq!(f.index_a, Some(r), "{id}: record {r} float {k}");
        assert!(
            matches!(f.class, Class::BoundedTight | Class::Angle),
            "{id}: record {r} float {k} judged by {:?}",
            f.class
        );
        assert_eq!(
            report.aligned, unperturbed.aligned,
            "{id}: perturbing float {k} of record {r} moved the alignment"
        );
    }
    addresses.len()
}

#[test]
fn the_sweep_over_ac1032_visits_exactly_the_floats_counted_by_hand() {
    assert_eq!(float_count(&frozen("acadsharp-ac1032")), AC1032_FLOATS);
    assert_eq!(sweep("acadsharp-ac1032"), AC1032_FLOATS);
}

#[test]
fn every_frozen_stream_survives_the_sweep() {
    let mut total = 0;
    for fixture in fixtures::all() {
        if fixture.id == "acadsharp-ac1032" {
            continue;
        }
        total += sweep(&fixture.id);
    }
    // Six more saves of the same drawing, so about the same again each.
    assert!(total > 6 * 2000, "the sweep visited only {total} floats");
}

#[test]
fn half_the_bound_is_never_a_finding() {
    // The other side of the sweep: the same perturbations, inside tolerance,
    // must all pass. Without this a comparator that flagged everything would
    // satisfy the sweep above.
    let base = frozen("acadsharp-ac1032");
    let table = tight();
    let a = Stream::new(base.clone());
    for (r, k) in addresses(&base) {
        let b = Stream::new(perturbed(&base, r, k, 0.5));
        let report = compare("acadsharp-ac1032", &a, &b, &table).expect("table");
        assert!(
            report.findings.is_empty(),
            "half a bound on float {k} of record {r} was flagged: {:?}",
            report.findings.iter().map(|f| f.render()).collect::<Vec<_>>()
        );
    }
}
