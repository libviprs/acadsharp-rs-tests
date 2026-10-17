//! Cancellation is observed between batches, and these tests hold the crate to
//! exactly that.
//!
//! The library reads a caller-owned flag before each batch and never inside a
//! native parse, so what a caller can rely on is a bound counted in batches:
//! once the flag is visible, the *next* pull answers `Cancelled`. Nothing here
//! asserts wall-clock time, because a time budget is a statement about the
//! machine. The claim that a cancel is not noticed in the middle of one native
//! batch is stated in the docs and not tested, because it cannot be: there is no
//! seam inside a native parse to raise a flag from.
//!
//! # The budget
//!
//! The pull that returns `Cancelled` is itself a pull. So if the canceller has
//! confirmed the flag is set when `batches_pulled()` was `k`, the stream may
//! show at most `k + 1` pulls when it fails: the buffered items of batch `k`
//! come out first (they were already decoded), then one pull, which is the
//! refusal. [`BATCH_BUDGET`] names that `+ 1`.
#![cfg(feature = "native")]

mod native_common;

#[cfg(not(native_linked))]
#[test]
fn the_cancellation_tests_need_an_archive() {
    native_common::no_archive("cancellation");
}

#[cfg(native_linked)]
mod linked {
    use std::sync::{Mutex, MutexGuard, mpsc};
    use std::thread;

    use super::native_common::{drain, fixture, rss_kib, try_drain};
    use acadsharp_rs::{CancelToken, Decoder, Document, Error, Limits};

    /// The leak loop below reads the process's RSS, so no other test may be
    /// allocating while it runs; every test takes this lock first.
    static SERIAL: Mutex<()> = Mutex::new(());

    fn serial() -> MutexGuard<'static, ()> {
        SERIAL
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Pulls allowed after the canceller confirmed the flag, counting the one
    /// that returns the cancellation.
    const BATCH_BUDGET: u64 = 1;

    /// The fixture with the most batches in its model view, so a cancel can
    /// land with several batches still to come. Chosen by measurement, not by
    /// name, and the tests assert it really is multi-batch.
    fn largest() -> (String, std::path::PathBuf, u64) {
        let decoder = Decoder::new().expect("handshake");
        let mut best: Option<(String, std::path::PathBuf, u64)> = None;
        for f in acadsharp_rs_tests::fixtures::all() {
            let path = f.path();
            let Ok(doc) = Document::open_path(&decoder, &path, &Limits::default()) else {
                continue;
            };
            let Ok(count) = doc.view_count() else {
                continue;
            };
            let mut batches = 0;
            let mut whole = true;
            for v in 0..count {
                match try_drain(&doc, v) {
                    Ok(d) => batches += d.batches,
                    // A stress drawing that hits a default limit is not a
                    // candidate: this test needs a decode that ends.
                    Err(_) => whole = false,
                }
            }
            if !whole {
                continue;
            }
            if best.as_ref().is_none_or(|b| batches > b.2) {
                best = Some((f.id.clone(), path, batches));
            }
        }
        best.expect("at least one fixture opens")
    }

    #[test]
    fn a_token_set_before_decode_answers_cancelled_on_the_first_item() {
        let _serial = serial();
        let decoder = Decoder::new().expect("handshake");
        let path = fixture("acadsharp-ac1032");
        let doc = Document::open_path(&decoder, &path, &Limits::default()).expect("open");
        let token = CancelToken::new();
        token.cancel();
        let mut stream = doc
            .decode_with_cancel(0, &token)
            .expect("beginning under a set flag is legal");
        match stream.next() {
            Some(Err(Error::Cancelled)) => {}
            other => panic!("the first item should be Cancelled, and it is {other:?}"),
        }
        assert_eq!(
            stream.records_seen(),
            0,
            "nothing was delivered before the cancellation"
        );
        assert!(!stream.is_complete());
    }

    #[test]
    fn the_control_an_uncancelled_token_changes_nothing() {
        let _serial = serial();
        // Without this, the test above could pass for a decode that always
        // fails under a token. The same call with a clear token completes.
        let decoder = Decoder::new().expect("handshake");
        let path = fixture("acadsharp-ac1032");
        let doc = Document::open_path(&decoder, &path, &Limits::default()).expect("open");
        let token = CancelToken::new();
        let mut stream = doc.decode_with_cancel(0, &token).expect("decode_begin");
        for item in &mut stream {
            item.expect("an uncancelled decode yields no error");
        }
        assert!(stream.is_complete());
    }

    #[test]
    fn a_cancel_from_another_thread_lands_within_the_batch_budget() {
        let _serial = serial();
        let (id, path, total_batches) = largest();
        assert!(
            total_batches >= 4,
            "the largest fixture ({id}) has {total_batches} batches, so a mid-stream cancel cannot be told \
             from the end of the stream; this test needs a bigger drawing"
        );

        let decoder = Decoder::new().expect("handshake");
        let doc = Document::open_path(&decoder, &path, &Limits::default()).expect("open");
        // The view with the most batches is the one to interrupt.
        let (view, view_batches) = (0..doc.view_count().unwrap())
            .map(|v| (v, drain(&doc, v).batches))
            .max_by_key(|&(_, b)| b)
            .unwrap();
        assert!(
            view_batches >= 3,
            "view {view} of {id} has {view_batches} batches"
        );

        let token = CancelToken::new();
        let (go_tx, go_rx) = mpsc::channel::<()>();
        let (ack_tx, ack_rx) = mpsc::channel::<()>();
        let canceller = {
            let token = token.clone();
            thread::spawn(move || {
                go_rx.recv().expect("the consumer asks for the cancel");
                token.cancel();
                ack_tx.send(()).expect("confirm the flag is set");
            })
        };

        let mut stream = doc.decode_with_cancel(view, &token).expect("decode_begin");
        let mut asked_at = None;
        let mut failed_at = None;
        while let Some(item) = stream.next() {
            match item {
                Ok(_) => {
                    if asked_at.is_none() && stream.batches_pulled() >= 2 {
                        go_tx.send(()).unwrap();
                        ack_rx.recv().expect("the canceller confirms");
                        asked_at = Some(stream.batches_pulled());
                    }
                }
                Err(Error::Cancelled) => {
                    failed_at = Some(stream.batches_pulled());
                    break;
                }
                Err(other) => panic!("expected Cancelled, got {other}"),
            }
        }
        canceller.join().expect("the canceller thread");
        let (asked_at, failed_at) = (
            asked_at.expect("the stream never reached batch 2"),
            failed_at.expect("the stream ended without ever reporting the cancellation"),
        );
        assert!(
            failed_at <= asked_at + BATCH_BUDGET,
            "the flag was confirmed at {asked_at} pulls and the cancellation arrived at {failed_at}, \
             which is more than the budget of {BATCH_BUDGET} pull(s) after it"
        );
        assert!(
            failed_at < view_batches,
            "the cancellation arrived at pull {failed_at} of a {view_batches}-batch view, i.e. at the end, \
             so it was not mid-stream"
        );
        assert!(!stream.is_complete());
        // The cancel is final: clearing nothing and asking again does not
        // resume the stream.
        assert!(stream.next().is_none(), "a failed stream is fused");
    }

    #[test]
    fn cancelling_at_a_random_batch_a_hundred_times_leaks_nothing() {
        let _serial = serial();
        let decoder = Decoder::new().expect("handshake");
        let path = fixture("acadsharp-ac1032");

        // A small deterministic generator, so a failing run can be repeated.
        let mut state = 0x9E37_79B9_7F4A_7C15_u64;
        let mut next = move || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };

        let mut baseline = None;
        let rounds = 100;
        for round in 1..=rounds {
            let doc = Document::open_path(&decoder, &path, &Limits::default()).expect("open");
            let token = CancelToken::new();
            let mut stream = doc.decode_with_cancel(0, &token).expect("decode_begin");
            let cancel_after = next() % 3;
            let mut ended_cancelled = false;
            while let Some(item) = stream.next() {
                match item {
                    Ok(_) if stream.batches_pulled() > cancel_after => token.cancel(),
                    Ok(_) => {}
                    Err(Error::Cancelled) => {
                        ended_cancelled = true;
                        break;
                    }
                    Err(other) => panic!("round {round}: {other}"),
                }
            }
            // A small drawing may finish inside the first batch, and that is a
            // legitimate outcome of a cancel that arrived too late.
            let _ = ended_cancelled;
            drop(stream);
            drop(doc);
            if round == 10 {
                baseline = rss_kib();
            }
        }
        if let (Some(before), Some(after)) = (baseline, rss_kib()) {
            eprintln!("cancel loop: RSS {before} KiB after round 10, {after} KiB after {rounds}");
            assert!(
                after <= before + 32 * 1024,
                "RSS grew from {before} KiB to {after} KiB over {rounds} cancelled decodes"
            );
        }
    }
}
