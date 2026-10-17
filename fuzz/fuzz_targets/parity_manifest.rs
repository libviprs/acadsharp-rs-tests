#![no_main]
//! Fuzz target `parity_manifest`. The body lives in `acadsharp_rs_tests::fuzz_harness`, so
//! the corpus replay and the Miri job run exactly what runs here.

use acadsharp_rs_tests::fuzz_harness::{self, CappedAlloc};
use libfuzzer_sys::fuzz_target;

// Aborts an input that takes more than 64 MiB.
#[global_allocator]
static ALLOC: CappedAlloc = CappedAlloc;

fuzz_target!(|data: &[u8]| fuzz_harness::parity_manifest(data));
