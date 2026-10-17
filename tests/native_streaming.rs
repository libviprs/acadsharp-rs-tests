//! H4.4: the streaming claim, on a corpus file rather than a synthetic stream.
//!
//! `regression_large_entity_count` is fifty thousand lines. Decoding it through
//! the safe API has to hold one native batch at a time, so once the first
//! batches are in, the process's resident set stays within four batches for the
//! rest of the decode. The issue states it against the RSS right after open;
//! measured, the first decode steps the resident set up by about 10.7 MiB
//! whatever the stream length (the runtime paging in what a first decode
//! touches), then it is flat, so the assertion is on growth after
//! `WARMUP_BATCHES` (see `native_rss`), and the step is printed every run. A "batch" here is the largest one the
//! stream actually pulled (`PrimitiveStream::buffer_len` after each pull), not
//! `max_batch_bytes`, which is the 64 MiB ceiling the buffer may grow to and
//! would make the budget meaningless. `native_streaming_control.rs` is the
//! other half: the same measurement on a consumer that keeps everything, which
//! has to blow the same budget.
//!
//! What this does not claim: it samples between batches, so growth that
//! happens and is released inside one batch is invisible, and four batches is
//! the allowance for allocator slack, not a measurement of the minimum. RSS
//! comes from `/proc/self/statm` on Linux and `ps` on macOS; elsewhere the test
//! skips unless `VIPRS_REQUIRE_NATIVE=1`.
#![cfg(feature = "native")]

mod native_rss;

#[cfg(not(native_linked))]
#[test]
fn the_streaming_claim_cannot_run_without_an_archive() {
    if std::env::var("VIPRS_REQUIRE_NATIVE").is_ok_and(|v| v == "1") {
        panic!(
            "VIPRS_REQUIRE_NATIVE=1 but no native archive was linked. Set ACADSHARP_NATIVE_DIR to \
             an unpacked libviprs-dep archive (tools/fetch_native.py prints one); it is {:?} here.",
            std::env::var_os("ACADSHARP_NATIVE_DIR")
        );
    }
    eprintln!(
        "skipped: no native archive linked (set VIPRS_REQUIRE_NATIVE=1 to make this a failure)"
    );
}

#[cfg(native_linked)]
#[test]
fn after_warm_up_decoding_fifty_thousand_lines_grows_by_less_than_four_batches() {
    use native_rss::{drain, instrument};

    if !instrument() {
        return;
    }
    let d = drain(false);
    eprintln!("{}", d.report());
    eprintln!("series {:?}", d.series);
    let growth = d.peak_after_warmup() - d.settled();
    assert!(
        growth <= d.budget_kib(),
        "RSS kept growing after the warm-up batches: {}",
        d.report()
    );
}
