//! What the ABI lets two threads do, and what the crate refuses to let one do.
//!
//! `ABI.md` ("Threading") says one decode handle is single threaded and calls on
//! it must not overlap, and that two handles may be used from two threads,
//! including two decode handles on one document. The crate turns the first half
//! into the type system: `Decoder`, `Document` and `PrimitiveStream` implement
//! neither `Send` nor `Sync`, and the compile-fail cases that prove it live in
//! the crate (`tests/compile_fail/`, run by `trybuild`). This file adds the
//! other side, which the ABI allows and which therefore has to be shown to work:
//!
//! * two threads, each with its **own** `Decoder` and its own document, decoding
//!   two different fixtures to completion at the same time, a hundred times,
//!   with every output compared against the single-threaded output;
//! * the statement that the crate stays `!Send`/`!Sync`, asserted again here
//!   from the outside so a future `unsafe impl Send` in the crate fails this
//!   suite as well as the crate's own.
//!
//! Each thread builds its own `Decoder` because the type cannot cross threads.
//! That is the claim being tested, not a workaround: one decoder, one thread.
//! Sharing a handle across threads is not a test that fails at runtime here, it
//! is a program that does not compile, and the crate's compile-fail cases are
//! the evidence.
#![cfg(feature = "native")]

mod native_common;

use std::marker::PhantomData;

/// `true` when `T` is `Send`. Specialisation-free: an ambiguity trick over a
/// reference to the type, so the answer is a value a test can assert on.
macro_rules! is_send {
    ($t:ty) => {{
        struct Probe<T>(PhantomData<T>);
        #[allow(dead_code)]
        trait No {
            const IS: bool = false;
        }
        impl<T> No for Probe<T> {}
        #[allow(dead_code)]
        impl<T: Send> Probe<T> {
            const IS: bool = true;
        }
        <Probe<$t>>::IS
    }};
}

macro_rules! is_sync {
    ($t:ty) => {{
        struct Probe<T>(PhantomData<T>);
        #[allow(dead_code)]
        trait No {
            const IS: bool = false;
        }
        impl<T> No for Probe<T> {}
        #[allow(dead_code)]
        impl<T: Sync> Probe<T> {
            const IS: bool = true;
        }
        <Probe<$t>>::IS
    }};
}

#[test]
fn the_handles_are_neither_send_nor_sync() {
    use acadsharp_rs::{Decoder, Document, PrimitiveStream};
    // The control first: the probe says yes for something that is Send + Sync.
    // A probe that always answered `false` would make everything below pass.
    assert!(
        is_send!(String) && is_sync!(String),
        "the probe cannot see Send/Sync at all"
    );
    assert!(
        !is_send!(std::rc::Rc<u8>),
        "the probe cannot see a !Send type"
    );
    assert!(
        !is_sync!(std::cell::Cell<u8>),
        "the probe cannot see a !Sync type"
    );

    assert!(!is_send!(Decoder) && !is_sync!(Decoder), "Decoder");
    assert!(!is_send!(Document) && !is_sync!(Document), "Document");
    assert!(
        !is_send!(PrimitiveStream<'static>) && !is_sync!(PrimitiveStream<'static>),
        "PrimitiveStream"
    );
    // The token, by contrast, is the one thing that has to cross threads.
    assert!(
        is_send!(acadsharp_rs::CancelToken) && is_sync!(acadsharp_rs::CancelToken),
        "CancelToken"
    );
}

#[cfg(not(native_linked))]
#[test]
fn the_concurrent_decode_needs_an_archive() {
    native_common::no_archive("threading");
}

#[cfg(native_linked)]
mod linked {
    use std::path::PathBuf;
    use std::thread;

    use super::native_common::fixture;
    use acadsharp_rs::{Decoder, Document, Item, Limits};

    /// One stable line per item, enough to tell two decodes apart. `Debug` of
    /// the typed item is the contract surface here: it prints every field.
    fn render(path: &PathBuf) -> Vec<String> {
        let decoder = Decoder::new().expect("handshake");
        let doc = Document::open_path(&decoder, path, &Limits::default()).expect("open");
        let mut out = Vec::new();
        for v in 0..doc.view_count().expect("view_count") {
            let mut stream = doc.decode(v).expect("decode_begin");
            for item in &mut stream {
                let item: Item = item.expect("a record decodes");
                out.push(format!("{v} {item:?}"));
            }
            assert!(stream.is_complete());
        }
        out
    }

    #[test]
    fn two_threads_with_two_decoders_decode_two_fixtures_exactly_as_one_thread_does() {
        let a = fixture("acadsharp-ac1032");
        let b = fixture("acadsharp-ac1024");
        assert_ne!(a, b);
        let want_a = render(&a);
        let want_b = render(&b);
        assert!(!want_a.is_empty() && !want_b.is_empty());
        assert_ne!(
            want_a, want_b,
            "the two fixtures must decode differently or the comparison is vacuous"
        );

        for round in 0..100 {
            let (pa, pb) = (a.clone(), b.clone());
            let ta = thread::spawn(move || render(&pa));
            let tb = thread::spawn(move || render(&pb));
            let got_a = ta.join().expect("thread A did not panic");
            let got_b = tb.join().expect("thread B did not panic");
            assert!(
                got_a == want_a,
                "round {round}: thread A's output differs from the single-threaded decode"
            );
            assert!(
                got_b == want_b,
                "round {round}: thread B's output differs from the single-threaded decode"
            );
        }
    }

    #[test]
    fn two_threads_decoding_the_same_fixture_each_get_the_same_answer() {
        // Same file, two documents, two decoders: independent handles on the
        // same input, which is the other shape the ABI allows.
        let path = fixture("acadsharp-ac1032");
        let want = render(&path);
        for round in 0..20 {
            let handles: Vec<_> = (0..2)
                .map(|_| {
                    let p = path.clone();
                    thread::spawn(move || render(&p))
                })
                .collect();
            for h in handles {
                assert!(
                    h.join().expect("no panic") == want,
                    "round {round}: a thread disagreed"
                );
            }
        }
    }
}
