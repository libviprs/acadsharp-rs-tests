//! The allocation cap is live: an input that takes more than the cap aborts
//! the process. Run in a child process because abort cannot be caught.
//!
//! Not in the Miri allowlist: Miri cannot spawn a process.

use acadsharp_rs_tests::fuzz_harness::{CappedAlloc, arm};

#[global_allocator]
static ALLOC: CappedAlloc = CappedAlloc;

const PROBE: &str = "ACADSHARP_ALLOC_CAP_PROBE";

#[test]
fn an_allocation_past_the_cap_aborts_the_process() {
    if std::env::var_os(PROBE).is_some() {
        arm();
        let big = vec![1u8; 128 * 1024 * 1024];
        std::hint::black_box(&big);
        // Reaching here means the cap did nothing.
        return;
    }
    let out = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "an_allocation_past_the_cap_aborts_the_process",
            "--nocapture",
        ])
        .env(PROBE, "1")
        .output()
        .unwrap();
    assert!(
        !out.status.success(),
        "a 128 MiB allocation went through; the cap is not live"
    );
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("allocation cap exceeded"),
        "aborted for another reason: {err}"
    );
}

#[test]
fn an_allocation_under_the_cap_is_fine() {
    arm();
    let ok = vec![1u8; 1024 * 1024];
    std::hint::black_box(&ok);
}
