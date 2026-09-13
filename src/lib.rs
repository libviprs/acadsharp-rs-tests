//! Test suite and fixtures for [`acadsharp-rs`].
//!
//! The suite lives outside the crate on purpose. DWG fixtures are large and
//! numerous, and a consumer of `acadsharp-rs` should never pay for them: this
//! is the same split `libviprs` and `libviprs-tests` already use in this org.
//!
//! [`acadsharp-rs`]: https://github.com/libviprs/acadsharp-rs
#![forbid(unsafe_op_in_unsafe_fn)]

// The suite depends on `acadsharp-rs` by path so CI builds it against the
// sibling checkout `COUNTERPART_REV` pins, but nothing in here decodes a DWG in
// Rust yet, so no module has a reason to name the crate. This is that reason.
// It keeps the dependency used for `unused_crate_dependencies`, which is off
// today and would otherwise become a hard error the moment somebody switches it
// on, because CI builds with `-Dwarnings`. Delete it when the first real `use`
// lands.
use acadsharp_rs as _;

pub mod fixtures;
pub mod parity;

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
