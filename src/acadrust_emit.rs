//! acadrust to canonical schema 1.
//!
//! The adapter behind `emit-acadrust`. acadrust does not flatten; this turns
//! its document into the records `docs/CANONICAL_SCHEMA.md` defines, with the
//! mapping, the conventions and the refusals that document spells out.
//!
//! The render half of the fork's conformance harness (`tests/viprs/mod.rs`,
//! which we wrote) comes from there, retargeted from libviprs-dep's wire 2
//! mapping to schema 1's. It is our code and carries no MPL notice; nothing of
//! upstream acadrust's source is in this file.

use acadrust::document::CadDocument;

use crate::emit::Emission;

/// Read a DWG with acadrust and emit it.
pub fn extract_bytes(bytes: &[u8]) -> Result<Emission, String> {
    let _ = bytes;
    todo!("R3.2")
}

/// Emit an already-read document.
pub fn extract(doc: &CadDocument) -> Emission {
    let _ = doc;
    todo!("R3.2")
}
