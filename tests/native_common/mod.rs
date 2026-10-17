//! What the native tests share: finding a fixture, draining a stream, reading
//! this process's resident set, and the guard that says "there is no library".
//!
//! Every file that uses this is behind `feature = "native"`, and the real
//! bodies are behind `cfg(native_linked)` (set by `build.rs`). This module is
//! plain Rust over the safe API, so it compiles either way.
#![allow(dead_code)]

use std::path::PathBuf;

use acadsharp_rs::{Document, Item};

/// The path of a verified fixture. Verified first, so a test never decodes
/// bytes the manifest does not vouch for.
pub fn fixture(id: &str) -> PathBuf {
    let f = acadsharp_rs_tests::fixtures::by_id(id)
        .unwrap_or_else(|| panic!("the manifest has no fixture {id:?}"));
    f.load()
        .unwrap_or_else(|e| panic!("fixture {id} failed verification: {e}"));
    f.path()
}

/// What a full decode of one view came to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Drained {
    pub records: u64,
    pub batches: u64,
    pub complete: bool,
}

/// Every item of `view`, counted and dropped. Fails the test on any error.
pub fn drain(doc: &Document, view: u32) -> Drained {
    try_drain(doc, view).unwrap_or_else(|e| panic!("view {view} did not decode: {e}"))
}

/// As [`drain`], but a refusal (a drawing that hits a default limit, say) is a
/// value, for the callers that are choosing among fixtures.
pub fn try_drain(doc: &Document, view: u32) -> Result<Drained, acadsharp_rs::Error> {
    let mut stream = doc.decode(view)?;
    for item in &mut stream {
        let item: Item = item?;
        drop(item);
    }
    Ok(Drained {
        records: stream.records_seen(),
        batches: stream.batches_pulled(),
        complete: stream.is_complete(),
    })
}

/// Resident set size in KiB, or `None` where this platform has no cheap way to
/// ask. Linux reads `/proc/self/statm`; macOS asks `ps` about this pid, which is
/// slower than `task_info` and needs no unsafe or extra crate. Anything else is
/// a documented skip, not a zero.
pub fn rss_kib() -> Option<u64> {
    #[cfg(target_os = "linux")]
    {
        let statm = std::fs::read_to_string("/proc/self/statm").ok()?;
        let pages: u64 = statm.split_whitespace().nth(1)?.parse().ok()?;
        // The page size is 4 KiB on every target the lane covers (x86_64 and
        // aarch64 GNU/musl Linux as the runners configure them). Asked rather
        // than assumed would need libc; a wrong guess would scale every number
        // here equally and the margin is a ratio of two of them.
        Some(pages * 4)
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

/// The unset-archive guard, shared by every native test file. Returns `true`
/// when the caller should skip, panics under `VIPRS_REQUIRE_NATIVE=1`.
pub fn no_archive(test: &str) -> bool {
    if std::env::var("VIPRS_REQUIRE_NATIVE").is_ok_and(|v| v == "1") {
        panic!(
            "{test}: VIPRS_REQUIRE_NATIVE=1 but no native archive was linked. Set \
             ACADSHARP_NATIVE_DIR to an unpacked libviprs-dep archive (it is {:?} here).",
            std::env::var_os("ACADSHARP_NATIVE_DIR")
        );
    }
    eprintln!("{test}: skipped, no native archive was linked (ACADSHARP_NATIVE_DIR)");
    true
}

/// A scratch directory under the target dir that a test can write into.
pub fn scratch(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(name);
    std::fs::create_dir_all(&dir).expect("create a scratch directory");
    dir
}

/// A truncated copy of a fixture, written under the scratch directory.
pub fn truncated(id: &str, keep_fraction_num: usize, keep_fraction_den: usize) -> PathBuf {
    let bytes = acadsharp_rs_tests::fixtures::load(id).expect("fixture bytes");
    let keep = bytes.len() * keep_fraction_num / keep_fraction_den;
    let dir = scratch("native-truncated");
    let path = dir.join(format!("{id}.{keep_fraction_num}of{keep_fraction_den}.dwg"));
    std::fs::write(&path, &bytes[..keep]).expect("write the truncated copy");
    path
}
