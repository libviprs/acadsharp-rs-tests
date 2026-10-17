//! Does the native library give back what the crate asks it to, over and over.
//!
//! "No leaks" is a claim until something counts, so this opens, enumerates,
//! decodes and drops `sample_AC1032.dwg` a thousand times and compares the
//! resident set size after the loop with the size after the tenth iteration. A
//! handle that is never closed, a batch buffer that is never freed, a document
//! that stays alive behind a dropped `Document`: any of them grows the number
//! by roughly one iteration's worth per iteration, and a thousand iterations
//! turn that into something a fixed margin can see.
//!
//! Four loops, because they leave through different doors:
//!
//! * a full decode, which ends at `DocumentEnd`;
//! * a decode abandoned after its first batch, which is the drop path that must
//!   close the decode handle by itself, since nothing reached the end;
//! * the same abandoned decode on one document that stays open for the whole
//!   loop, so the stream's own `Drop` is the only thing that could free each
//!   decode (closing a document releases every decode handle still open on it,
//!   which hides a missing stream close from the loops that drop the document
//!   each time);
//! * an `open` that fails on a truncated copy of the same drawing, every time,
//!   which is the error path: whatever the library allocated before it noticed
//!   must not outlive the failed call.
//!
//! # The margin
//!
//! [`MARGIN_KIB`] is the growth allowed between iteration 10 and the end of a
//! loop. It is not zero because a managed heap is allowed to settle: the GC may
//! take its time returning pages, and the allocator keeps arenas. It is small
//! against the cost of a real leak, which is measured in the comment on the
//! constant. A failure prints both numbers.
//!
//! # What this does not see
//!
//! A decode handle that is never closed. Measured with `viprs_acad_decode_close`
//! removed from the crate's `Drop` on a scratch copy: none of the four loops
//! moved past the noise, because one decode handle is small and the library
//! frees it with its document anyway. What these loops do see is a document that
//! is never closed (RSS grew by about 2.9 GiB over a thousand iterations) and,
//! under LeakSanitizer, an allocation leaked on the Rust side. The crate's own
//! stub-driven tests count the closes; this file does not claim to.
//!
//! RSS is the whole process, so it includes the .NET GC heap, which a leak
//! sanitizer does not. The LSan cell in CI sees the opposite: native `malloc`
//! and Rust allocations, and not the managed heap. Neither of the two says
//! anything about the other, and neither is a proof of the ABI.
#![cfg(feature = "native")]

mod native_common;

#[cfg(not(native_linked))]
#[test]
fn the_resource_loops_need_an_archive() {
    native_common::no_archive("resource_semantics");
}

#[cfg(native_linked)]
mod linked {
    use std::sync::{Mutex, MutexGuard};

    use super::native_common::{drain, fixture, rss_kib, truncated};
    use acadsharp_rs::{Decoder, Document, Error, Limits};

    /// RSS belongs to the whole process, so two loops running side by side
    /// would each see the other's allocations. Every test here takes this lock
    /// first, which makes "the process grew during my loop" mean what it says.
    static SERIAL: Mutex<()> = Mutex::new(());

    fn serial() -> MutexGuard<'static, ()> {
        SERIAL
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Iterations per loop. `VIPRS_LOOP_ITERATIONS` shrinks it for the
    /// sanitizer cell, where every allocation costs more; the RSS assertion is
    /// only made when at least this many ran.
    const DEFAULT_ITERATIONS: usize = 1000;
    const BASELINE_AT: usize = 10;

    /// Growth allowed between iteration `BASELINE_AT` and the end, in KiB.
    ///
    /// Measured on native x86_64 Linux against `acadsharp-3.7.1-viprs.1`: the
    /// clean loops moved by 0 to about 3 MiB over a thousand iterations (the
    /// failed-open loop is the noisiest). With `viprs_acad_close` removed from
    /// the document's `Drop` on a scratch copy of the crate, the same loops grew
    /// by about 2.9 GiB. The noisiest clean loop (a document kept open) moved by
    /// about 9.5 MiB, so 24 MiB leaves room for a noisier runner and still sits
    /// two orders of magnitude below a document that is never closed.
    const MARGIN_KIB: u64 = 24 * 1024;

    fn iterations() -> usize {
        std::env::var("VIPRS_LOOP_ITERATIONS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(DEFAULT_ITERATIONS)
    }

    /// Runs `body` `iterations()` times and holds RSS after the loop against
    /// RSS after the tenth iteration.
    fn bounded(name: &str, mut body: impl FnMut()) {
        let n = iterations();
        let mut baseline = None;
        for i in 1..=n {
            body();
            if i == BASELINE_AT {
                baseline = rss_kib();
            }
        }
        let (Some(before), Some(after)) = (baseline, rss_kib()) else {
            eprintln!(
                "{name}: no RSS reading on this platform, so the loop ran and asserted nothing about memory"
            );
            return;
        };
        eprintln!(
            "{name}: RSS after iteration {BASELINE_AT} = {before} KiB, after {n} = {after} KiB (margin {MARGIN_KIB} KiB)"
        );
        if n <= BASELINE_AT * 10 {
            eprintln!(
                "{name}: {n} iterations is too few to hold a margin against, so no assertion"
            );
            return;
        }
        assert!(
            after <= before + MARGIN_KIB,
            "{name}: RSS grew from {before} KiB (iteration {BASELINE_AT}) to {after} KiB (iteration {n}), \
             which is {} KiB past the {MARGIN_KIB} KiB margin",
            after - before - MARGIN_KIB
        );
    }

    #[test]
    fn a_thousand_full_decodes_do_not_grow_the_process() {
        let _serial = serial();
        let decoder = Decoder::new().expect("handshake");
        let path = fixture("acadsharp-ac1032");
        bounded("full decode", || {
            let doc = Document::open_path(&decoder, &path, &Limits::default()).expect("open");
            let views = doc.views().expect("views");
            assert!(!views.is_empty());
            let mut records = 0;
            for v in &views {
                let d = drain(&doc, v.index());
                assert!(d.complete, "view {} did not complete", v.index());
                records += d.records;
            }
            assert!(records > 0);
        });
    }

    #[test]
    fn a_thousand_decodes_abandoned_after_the_first_batch_do_not_grow_the_process() {
        let _serial = serial();
        let decoder = Decoder::new().expect("handshake");
        let path = fixture("acadsharp-ac1032");
        bounded("abandoned decode", || {
            let doc = Document::open_path(&decoder, &path, &Limits::default()).expect("open");
            let mut stream = doc.decode(0).expect("decode_begin");
            let first = stream.next().expect("a first item").expect("it decodes");
            drop(first);
            assert!(!stream.is_complete(), "one item is not the whole stream");
            drop(stream);
        });
    }

    #[test]
    fn a_thousand_abandoned_decodes_on_one_open_document_do_not_grow_the_process() {
        // The loops above drop the document every time, and closing a document
        // releases every decode handle still open on it, so on their own they
        // cannot tell whether a dropped *stream* closes its own handle. This one
        // keeps the document open for the whole loop, which leaves the stream's
        // own `Drop` as the only thing that can free each decode.
        let _serial = serial();
        let decoder = Decoder::new().expect("handshake");
        let path = fixture("acadsharp-ac1032");
        let doc = Document::open_path(&decoder, &path, &Limits::default()).expect("open");
        bounded("abandoned decode, document kept open", || {
            let mut stream = doc.decode(0).expect("decode_begin");
            let first = stream.next().expect("a first item").expect("it decodes");
            drop(first);
            assert!(!stream.is_complete());
        });
    }

    #[test]
    fn a_thousand_failed_opens_do_not_grow_the_process() {
        let _serial = serial();
        let decoder = Decoder::new().expect("handshake");
        let path = truncated("acadsharp-ac1032", 1, 2);
        // Control: the loop below is only about the error path if the open
        // really fails, and fails the same way every time.
        let Err(first) = Document::open_path(&decoder, &path, &Limits::default()) else {
            panic!("a half of a DWG must not open");
        };
        assert!(
            !matches!(first, Error::Unlinked),
            "the open failed because nothing is linked, which is not the path under test: {first}"
        );
        bounded("failed open", || {
            let Err(err) = Document::open_path(&decoder, &path, &Limits::default()) else {
                panic!("still refused");
            };
            assert_eq!(
                err.to_string(),
                first.to_string(),
                "the refusal changed between iterations"
            );
        });
    }
}
