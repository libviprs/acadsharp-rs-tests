//! Test suite and fixtures for [`acadsharp-rs`].
//!
//! The suite lives outside the crate on purpose. DWG fixtures are large and
//! numerous, and a consumer of `acadsharp-rs` should never pay for them: this
//! is the same split `libviprs` and `libviprs-tests` already use in this org.
//!
//! [`acadsharp-rs`]: https://github.com/libviprs/acadsharp-rs
#![forbid(unsafe_op_in_unsafe_fn)]

#[cfg(feature = "acadrust")]
pub mod acadrust_emit;
#[cfg(feature = "acadrust")]
pub mod acadrust_model1;
pub mod acadsharp_emit;
pub mod canonical;
pub mod census;
pub mod codepage;
pub mod compare;
pub mod digest;
pub mod emit;
pub mod fixtures;
pub mod fuzz_harness;
pub mod invariance;
pub mod model1;
pub mod ocs;
pub mod parity;
pub mod parity_run;
pub mod tables;
pub mod verdict;
pub mod written;

#[cfg(test)]
mod tests {
    /// This crate must never reach crates.io. It carries fixtures, it pins a
    /// counterpart, and it is useless outside this repository, so `publish`
    /// stays false and something other than habit has to say so.
    #[test]
    fn the_manifest_refuses_publication() {
        let manifest = include_str!("../Cargo.toml");
        assert!(
            manifest
                .lines()
                .any(|line| line.split('#').next().unwrap_or("").trim() == "publish = false"),
            "Cargo.toml must declare `publish = false`; a fixtures crate has no business on crates.io"
        );
    }
}
