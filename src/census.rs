//! AutoCAD's object census as a ground truth (R3.4, acadsharp-rs-tests#24).
//!
//! STUB: the red tests in `tests/census.rs` come first.

use crate::canonical::{Record, Sidecar};

/// One entity of the census' model space.
#[derive(Debug, Clone, PartialEq)]
pub struct CensusEntity {
    pub handle: u64,
    /// The AutoCAD class without its namespace, for example `DBText`.
    pub class: String,
    pub invisible: bool,
}

/// What AutoCAD recorded for one save.
#[derive(Debug, Clone, PartialEq)]
pub struct Census {
    pub model_space: Vec<CensusEntity>,
}

impl Census {
    pub fn parse(_json: &str) -> Result<Self, String> {
        unimplemented!()
    }
    /// The census that belongs to this DWG fixture.
    pub fn for_fixture(_id: &str) -> Result<Self, String> {
        unimplemented!()
    }
}

/// The DXF names an AutoCAD class is written under.
pub fn dxf_names(_class: &str) -> Option<&'static [&'static str]> {
    unimplemented!()
}

#[derive(Debug, Clone, PartialEq)]
pub enum Finding {
    /// A visible census entity no record names.
    Missing { handle: u64, class: String },
    /// A handle the reader reports that the census does not have.
    Unexpected { handle: u64 },
    /// The reader names the entity as a different kind.
    WrongClass { handle: u64, census: String, reader: String },
    /// The census has a class the name map does not know.
    UnmappedClass { class: String },
    /// Nothing to check: no sidecar entries in the model space.
    Vacuous,
}

impl std::fmt::Display for Finding {
    fn fmt(&self, _f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        unimplemented!()
    }
}

/// The full check, for a reader whose sidecar names the top-level entity and
/// its DXF name (acadrust, ezdxf).
pub fn check_full(_census: &Census, _records: &[Record], _sidecar: &Sidecar) -> Vec<Finding> {
    unimplemented!()
}

/// The weaker check for a sidecar with no entity type and no path
/// (acadsharp-rs): handles on records whose `x` flag is clear.
pub fn check_weak(_census: &Census, _records: &[Record], _sidecar: &Sidecar) -> Vec<Finding> {
    unimplemented!()
}
