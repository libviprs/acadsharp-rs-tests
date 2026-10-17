//! H4.4: the streaming claim, on a corpus file rather than a synthetic stream.
//!
//! `regression_large_entity_count` is fifty thousand lines. Decoding it through
//! the safe API has to hold one native batch at a time, so the process's
//! resident set while the decode runs stays within four batch sizes of what it
//! was right after the document was opened. A decoder that collected the whole
//! stream would grow by megabytes, which the control below does on purpose to
//! show the instrument can see it.
//!
//! What this does not claim: it says nothing about the .NET heap beyond what
//! shows up in this process's resident set (the library is in-process, so its
//! allocations do), and it samples between batches, so growth that happens and
//! is released inside one batch is invisible. Four batches is the allowance for
//! allocator slack, not a measurement of the minimum.
//!
//! RSS comes from `/proc/self/statm` on Linux and `ps` on macOS. Elsewhere the
//! test skips, and says so, unless `VIPRS_REQUIRE_NATIVE=1`, which makes a
//! missing instrument a failure.
#![cfg(feature = "native")]

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
    eprintln!("skipped: no native archive linked (set VIPRS_REQUIRE_NATIVE=1 to make this a failure)");
}

#[cfg(native_linked)]
mod linked {
    use acadsharp_rs::{Decoder, Document, Item, Limits, Primitive};
    use acadsharp_rs_tests::fixtures;

    const ID: &str = "regression_large_entity_count";
    const LINES: usize = 50_000;
    const COLUMNS: usize = 250;

    /// Resident set size in KiB, or `None` where there is no instrument.
    fn rss_kib() -> Option<u64> {
        #[cfg(target_os = "linux")]
        {
            let statm = std::fs::read_to_string("/proc/self/statm").ok()?;
            let pages: u64 = statm.split_whitespace().nth(1)?.parse().ok()?;
            Some(pages * 4096 / 1024)
        }
        #[cfg(target_os = "macos")]
        {
            let out = std::process::Command::new("ps")
                .args(["-o", "rss=", "-p", &std::process::id().to_string()])
                .output()
                .ok()?;
            String::from_utf8(out.stdout).ok()?.trim().parse().ok()
        }
        #[cfg(not(any(target_os = "linux", target_os = "macos")))]
        {
            None
        }
    }

    fn instrument() -> bool {
        if rss_kib().is_some() {
            return true;
        }
        assert!(
            !std::env::var("VIPRS_REQUIRE_NATIVE").is_ok_and(|v| v == "1"),
            "VIPRS_REQUIRE_NATIVE=1 but this platform has no RSS instrument"
        );
        eprintln!("skipped: no RSS instrument on this platform");
        false
    }

    fn line_at(i: usize) -> ([f64; 3], [f64; 3]) {
        let (x, y) = ((i % COLUMNS) as f64, (i / COLUMNS) as f64);
        ([x, y, 0.0], [x + 1.0, y, 0.0])
    }

    #[test]
    fn decoding_fifty_thousand_lines_stays_within_four_batches_of_the_post_open_rss() {
        if !instrument() {
            return;
        }
        let fx = fixtures::by_id(ID).expect("the fixture row");
        // Verified bytes are not needed here (the path goes to the library), but
        // a drawing that is not the one the row froze would make this meaningless.
        fx.load().expect("the committed drawing matches its row");

        let decoder = Decoder::new().expect("handshake");
        let doc = Document::open_path(&decoder, fx.path(), &Limits::default()).expect("open");
        let after_open = rss_kib().unwrap();

        let mut stream = doc.decode(0).expect("decode");
        let budget_kib = (4 * stream.max_batch_bytes() as u64).div_ceil(1024);
        let mut peak = after_open;
        let mut lines = 0usize;
        let mut last_batch = 0;
        let mut samples = 0u32;
        for item in &mut stream {
            if let Item::Primitive(Primitive::Line(l)) = item.expect("a record") {
                let (a, b) = line_at(lines);
                assert_eq!((l.start, l.end), (a, b), "line {lines}");
                lines += 1;
            }
            if stream.batches_pulled() != last_batch {
                last_batch = stream.batches_pulled();
                peak = peak.max(rss_kib().unwrap());
                samples += 1;
            }
        }
        assert!(stream.is_complete(), "the decode did not reach DocumentEnd");
        assert_eq!(lines, LINES, "every line arrived");
        assert!(
            samples >= 8,
            "only {samples} batches were sampled; with fewer the claim is about nothing"
        );

        let growth = peak - after_open;
        eprintln!(
            "rss after open {after_open} KiB, peak between batches {peak} KiB, growth {growth} KiB \
             over {samples} batches, budget {budget_kib} KiB (4 x {} byte batches)",
            stream.max_batch_bytes()
        );
        assert!(
            growth <= budget_kib,
            "RSS grew by {growth} KiB while decoding (after open {after_open} KiB, peak {peak} KiB); \
             four batches of {} bytes allow {budget_kib} KiB",
            stream.max_batch_bytes()
        );

        // Same process, same test, and after the real measurement: two tests
        // would run on parallel threads and measure each other, and a control
        // run first would leave the allocator holding pages the real run then
        // reuses for free.
        drop(stream);
        drop(doc);
        a_consumer_that_keeps_every_record_blows_the_same_budget();
    }

    /// The control: a consumer that keeps every item grows by far more than the
    /// budget, on the same drawing, measured the same way. If this did not, the
    /// test above could pass on a decoder that buffered everything.
    fn a_consumer_that_keeps_every_record_blows_the_same_budget() {
        let fx = fixtures::by_id(ID).expect("the fixture row");
        let decoder = Decoder::new().expect("handshake");
        let doc = Document::open_path(&decoder, fx.path(), &Limits::default()).expect("open");
        let after_open = rss_kib().unwrap();
        let mut stream = doc.decode(0).expect("decode");
        let budget_kib = (4 * stream.max_batch_bytes() as u64).div_ceil(1024);
        let kept: Vec<Item> = (&mut stream).map(|i| i.expect("a record")).collect();
        let after = rss_kib().unwrap();
        assert!(kept.len() >= LINES);
        assert!(
            after - after_open > budget_kib,
            "keeping {} records grew RSS by only {} KiB against a budget of {budget_kib} KiB, \
             so the instrument cannot tell buffering from streaming",
            kept.len(),
            after.saturating_sub(after_open)
        );
    }
}
