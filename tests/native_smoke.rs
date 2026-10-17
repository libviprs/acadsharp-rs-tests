//! The native lane's smoke test: the real library, through the raw `ffi`.
//!
//! Everything before this ran against stubs. This is the first place the crate
//! meets the artifact `libviprs-dep` publishes, and it is deliberately small
//! and low level: it calls the C ABI directly, so a failure here is about the
//! library and the vendored header, not about any layer of the safe API.
//!
//! What it claims, and nothing past it: the handshake succeeds, the capability
//! range and version string say what the archive's `LINKINFO.json` says, a real
//! drawing opens and decodes one batch that the wire parser accepts, a six byte
//! `AC1009` file is refused with `UNSUPPORTED_FORMAT`, and every handle that was
//! opened was closed. It does not say the decode is *right*: that is the
//! differential's job (H4.1).
//!
//! # When there is no library
//!
//! `build.rs` sets `cfg(native_linked)` only when `acadsharp-rs`'s build script
//! resolved an archive. Without it this file compiles to a test that skips, and
//! under `VIPRS_REQUIRE_NATIVE=1` (which every native CI job sets) to a test that
//! panics naming `ACADSHARP_NATIVE_DIR`. A lane that was supposed to link the
//! library and quietly did not is the false green this guard exists to stop.
#![cfg(feature = "native")]

/// The unset case. Runs only when nothing was linked.
#[cfg(not(native_linked))]
#[test]
fn the_native_smoke_cannot_run_without_an_archive() {
    if std::env::var("VIPRS_REQUIRE_NATIVE").is_ok_and(|v| v == "1") {
        panic!(
            "VIPRS_REQUIRE_NATIVE=1 but no native archive was linked. Set ACADSHARP_NATIVE_DIR to \
             an unpacked libviprs-dep archive (tools/fetch_native.py prints one); it is {:?} here.",
            std::env::var_os("ACADSHARP_NATIVE_DIR")
        );
    }
    eprintln!(
        "skipped: ACADSHARP_NATIVE_DIR did not resolve to an archive, so nothing was linked \
         (set VIPRS_REQUIRE_NATIVE=1 to make this a failure)"
    );
}

#[cfg(native_linked)]
mod linked {
    use std::path::PathBuf;
    use std::ptr;

    use acadsharp_rs::batch::{BatchReader, Record};
    use acadsharp_rs::{
        Decoder, EXPECTED_ABI_FINGERPRINT, EXPECTED_ABI_VERSION, EXPECTED_WIRE_VERSION, ffi,
    };

    /// A document handle that closes itself, so "every handle is closed" does
    /// not depend on every assertion above it having passed.
    struct Doc(*mut ffi::viprs_acad_handle);

    impl Drop for Doc {
        fn drop(&mut self) {
            // SAFETY: the pointer came from a successful open call and is
            // closed exactly once, here. Close on a handle with open decode
            // handles is documented to release those too.
            unsafe { ffi::viprs_acad_close(self.0) };
            CLOSED.with(|c| c.set(c.get() + 1));
        }
    }

    struct Decode(*mut ffi::viprs_acad_decode_handle);

    impl Drop for Decode {
        fn drop(&mut self) {
            // SAFETY: from a successful `decode_begin`, closed once, and before
            // the document it belongs to (declaration order below).
            unsafe { ffi::viprs_acad_decode_close(self.0) };
            CLOSED.with(|c| c.set(c.get() + 1));
        }
    }

    thread_local! {
        static OPENED: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
        static CLOSED: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
    }

    fn open_path(path: &str) -> (u32, Option<Doc>) {
        let mut handle: *mut ffi::viprs_acad_handle = ptr::null_mut();
        // SAFETY: `path` is live UTF-8 for the call and `handle` is a live
        // out-pointer. A null `limits` is the documented request for defaults.
        let code = unsafe {
            ffi::viprs_acad_open_path_utf8(
                path.as_ptr(),
                path.len() as u64,
                ptr::null(),
                &mut handle,
            )
        };
        if code == ffi::VIPRS_ACAD_OK {
            assert!(!handle.is_null(), "OK with a null handle");
            OPENED.with(|c| c.set(c.get() + 1));
            (code, Some(Doc(handle)))
        } else {
            (code, None)
        }
    }

    fn archive_linkinfo() -> serde_json::Value {
        let dir = std::env::var_os("ACADSHARP_NATIVE_DIR")
            .map(PathBuf::from)
            .expect("ACADSHARP_NATIVE_DIR is unset, yet the library linked");
        let path = dir.join("metadata").join("LINKINFO.json");
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
        serde_json::from_str(&text)
            .unwrap_or_else(|e| panic!("{} is not JSON: {e}", path.display()))
    }

    #[test]
    fn the_real_library_links_handshakes_decodes_one_batch_and_closes_everything() {
        // 1. The handshake, through the raw calls and then through `Decoder`,
        //    which is what a consumer calls.
        // SAFETY: both calls take no arguments, return a plain integer and are
        // documented never to fail; the symbols are linked under `native_linked`.
        let (abi, fingerprint) = unsafe {
            (
                ffi::viprs_acad_abi_version(),
                ffi::viprs_acad_abi_fingerprint(),
            )
        };
        assert_eq!(abi, EXPECTED_ABI_VERSION, "the library's ABI version");
        assert_eq!(
            fingerprint, EXPECTED_ABI_FINGERPRINT,
            "the library's header fingerprint"
        );
        let decoder = Decoder::new().expect("the handshake refused the archive");

        // 2. Capabilities: the supported range, and the version string against
        //    the archive's own manifest.
        let mut caps = ffi::viprs_acad_capabilities_v1::default();
        let mut required = 0u64;
        // SAFETY: a live, correctly sized out-struct; a null buffer with a
        // capacity of zero is the documented way to ask for the length.
        let code = unsafe {
            ffi::viprs_acad_get_capabilities_v1(&mut caps, ptr::null_mut(), 0, &mut required)
        };
        assert_eq!(code, ffi::VIPRS_ACAD_OK, "the sizing call");
        assert_eq!(caps.dwg_version_min, 1014, "oldest readable drawing");
        assert_eq!(caps.dwg_version_max, 1032, "newest readable drawing");
        assert_eq!(caps.wire_version, EXPECTED_WIRE_VERSION, "wire version");
        let mut version = vec![0u8; required as usize];
        // SAFETY: `version` is `required` bytes, which is the capacity passed.
        let code = unsafe {
            ffi::viprs_acad_get_capabilities_v1(
                &mut caps,
                version.as_mut_ptr(),
                version.len() as u64,
                &mut required,
            )
        };
        assert_eq!(code, ffi::VIPRS_ACAD_OK, "the filling call");
        let version = String::from_utf8(version).expect("the version string is UTF-8");
        let linkinfo = archive_linkinfo();
        assert_eq!(
            linkinfo["acadsharp_version"].as_str(),
            Some(version.as_str()),
            "the library reports ACadSharp {version:?} and the archive's LINKINFO.json says otherwise"
        );
        assert_eq!(
            decoder.capabilities().acadsharp_version(),
            version,
            "safe API agrees with ffi"
        );

        // 3. A real drawing opens, has a view, and decodes one batch that the
        //    wire parser accepts with a DocumentBegin first.
        let fixture = acadsharp_rs_tests::fixtures::by_id("acadsharp-ac1032")
            .expect("the manifest has acadsharp-ac1032");
        fixture
            .load()
            .expect("the fixture is verified before it is opened");
        let path = fixture.path();
        let (code, doc) = open_path(path.to_str().expect("a UTF-8 fixture path"));
        assert_eq!(code, ffi::VIPRS_ACAD_OK, "opening {}", path.display());
        let doc = doc.expect("open succeeded");

        let mut views = 0u32;
        // SAFETY: a live document handle and a live out-pointer.
        let code = unsafe { ffi::viprs_acad_view_count(doc.0, &mut views) };
        assert_eq!(code, ffi::VIPRS_ACAD_OK, "view_count");
        assert!(
            views >= 1,
            "a drawing has at least its model view, and this reports {views}"
        );

        let mut raw: *mut ffi::viprs_acad_decode_handle = ptr::null_mut();
        // SAFETY: a live document, view 0 exists, a null cancel flag is allowed.
        let code = unsafe { ffi::viprs_acad_decode_begin(doc.0, 0, ptr::null(), &mut raw) };
        assert_eq!(code, ffi::VIPRS_ACAD_OK, "decode_begin on view 0");
        OPENED.with(|c| c.set(c.get() + 1));
        let decode = Decode(raw);

        let mut buf = vec![0u8; 1 << 20];
        let (written, _done) = loop {
            let mut written = 0u64;
            let mut done = 0u8;
            // SAFETY: a live decode handle and a live buffer of the length passed.
            let code = unsafe {
                ffi::viprs_acad_decode_next_batch(
                    decode.0,
                    buf.as_mut_ptr(),
                    buf.len() as u64,
                    &mut written,
                    &mut done,
                )
            };
            if code == ffi::VIPRS_ACAD_BUFFER_TOO_SMALL {
                // A batch never spans two calls; `written` is the size needed.
                buf.resize(written as usize, 0);
                continue;
            }
            assert_eq!(code, ffi::VIPRS_ACAD_OK, "decode_next_batch");
            break (written as usize, done);
        };
        assert!(written > 0, "the first batch has bytes in it");
        let reader = BatchReader::new(&buf[..written]).expect("the batch header parses");
        let first = reader
            .records()
            .next()
            .expect("the first batch has a record")
            .expect("the first record parses");
        assert!(
            matches!(first, Record::DocumentBegin(_)),
            "the first record of a stream is DocumentBegin, and this is {first:?}"
        );

        // 4. A six byte AC1009 file is refused for its format and nothing else.
        let dir = tempdir("native-smoke");
        let old = dir.join("old.dwg");
        std::fs::write(&old, b"AC1009").expect("write the crafted file");
        let (code, handle) = open_path(old.to_str().expect("a UTF-8 temp path"));
        assert_eq!(
            code,
            ffi::VIPRS_ACAD_UNSUPPORTED_FORMAT,
            "a six byte AC1009 file"
        );
        assert!(handle.is_none());
        std::fs::remove_dir_all(&dir).ok();

        // 5. Every handle is closed. The decode handle goes first, then the
        //    document, then the counters are read.
        drop(decode);
        drop(doc);
        let (opened, closed) = (OPENED.with(|c| c.get()), CLOSED.with(|c| c.get()));
        assert_eq!(opened, 2, "one document and one decode were opened");
        assert_eq!(opened, closed, "every handle that was opened was closed");
    }

    fn tempdir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("{tag}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("create a temp directory");
        dir
    }
}
