//! Upstream parity, mirrored (H4.2, #10): the Rust tests the `mirrored` rows of
//! `upstream_parity/manifest.toml` point at.
//!
//! ACadSharp's own suites say what a DWG reader has to do. These tests do not port
//! them (C# structure is not what matters); they check the observable behaviour
//! through the safe API against the real artifact. The rule for every test here:
//! it asserts a value the generator wrote (`reference/ACadSharp.Generator`), not
//! that decoding succeeded. "Opens without error" is the vacuous form; "the arc's
//! radius is 5.0 because the generator wrote 5.0" is the real one.
//!
//! Wire 2, which is what the pinned archive speaks, cannot express some things the
//! canonical schema can: text carries no alignment, style or width factor, a hatch
//! arrives as one `Polygon` per loop with no entity type, a bulge arrives as a
//! bulge and not as an arc. Where a test would have asserted one of those it says
//! so in a comment and asserts what the wire does carry; none is weakened silently.
//!
//! One binary for all of it, so the ten-odd families share one build. Behind
//! `feature = "native"`, real bodies behind `cfg(native_linked)`.
#![cfg(feature = "native")]

#[path = "../native_common/mod.rs"]
mod native_common;

#[cfg(not(native_linked))]
#[test]
fn the_parity_tests_cannot_run_without_an_archive() {
    native_common::no_archive("upstream_parity");
}

#[cfg(native_linked)]
mod arc;
#[cfg(native_linked)]
mod circle;
#[cfg(native_linked)]
mod common;
#[cfg(native_linked)]
mod dimensions;
#[cfg(native_linked)]
mod ellipse;
#[cfg(native_linked)]
mod hatch;
#[cfg(native_linked)]
mod insert;
#[cfg(native_linked)]
mod line;
#[cfg(native_linked)]
mod polyline;
#[cfg(native_linked)]
mod reader;
#[cfg(native_linked)]
mod spline;
#[cfg(native_linked)]
mod text;
#[cfg(native_linked)]
mod viewport;
