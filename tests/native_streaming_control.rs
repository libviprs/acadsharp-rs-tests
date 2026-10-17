//! The control for `native_streaming.rs`: a consumer that keeps every record of
//! the same drawing grows the resident set by far more than four batches,
//! measured the same way. If it did not, the streaming test could pass on a
//! decoder that buffered everything. Its own binary, so it runs in a fresh
//! process whose allocator has not already been warmed by the real measurement.
#![cfg(feature = "native")]

mod native_rss;

#[cfg(not(native_linked))]
#[test]
fn the_streaming_control_cannot_run_without_an_archive() {
    if std::env::var("VIPRS_REQUIRE_NATIVE").is_ok_and(|v| v == "1") {
        panic!(
            "VIPRS_REQUIRE_NATIVE=1 but no native archive was linked (ACADSHARP_NATIVE_DIR is {:?})",
            std::env::var_os("ACADSHARP_NATIVE_DIR")
        );
    }
    eprintln!("skipped: no native archive linked");
}

#[cfg(native_linked)]
#[test]
fn a_consumer_that_keeps_every_record_blows_the_streaming_budget() {
    use native_rss::{LINES, drain, instrument};

    if !instrument() {
        return;
    }
    let d = drain(true);
    eprintln!("{}", d.report());
    assert!(d.kept >= LINES);
    let growth = d.peak_after_warmup() - d.settled();
    assert!(
        growth > d.budget_kib(),
        "keeping {} records grew RSS by only {growth} KiB after warm-up against a budget of {} KiB, \
         so the instrument cannot tell buffering from streaming",
        d.kept,
        d.budget_kib()
    );
}
