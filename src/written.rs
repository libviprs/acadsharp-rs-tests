//! The writer matrix of the round-trip lane (R5.3, acadsharp-rs-tests#29).
//!
//! `fixtures/written_plan.toml` says which library rewrites which drawing at
//! which version in which container; this module reads it and names the outputs.
//! The files themselves are `[[written]]` rows of `fixtures/manifest.toml`, made
//! by `tools/regenerate_written.py`, so the plan, the rows and the test's
//! hand-written count are three things that have to agree.

use std::collections::BTreeMap;

use serde::Deserialize;

const PLAN: &str = include_str!("../fixtures/written_plan.toml");

/// The containers an output can be in.
pub const CONTAINERS: [&str; 3] = ["dwg", "dxf-ascii", "dxf-binary"];

#[derive(Debug, Deserialize)]
pub struct Stage {
    pub sources: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct Versions {
    #[serde(default)]
    pub dwg: Vec<String>,
    #[serde(default)]
    pub dxf: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct Writer {
    #[serde(default)]
    pub dwg: Vec<String>,
    #[serde(default)]
    pub dxf: Vec<String>,
    #[serde(default)]
    pub refuses: Versions,
}

#[derive(Debug, Deserialize)]
pub struct Plan {
    pub stage: BTreeMap<String, Stage>,
    pub writer: BTreeMap<String, Writer>,
}

/// One output the plan asks for.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Output {
    pub writer: String,
    pub source: String,
    /// `dwg`, `dxf-ascii` or `dxf-binary`.
    pub container: String,
    /// `AC1015` and so on.
    pub version: String,
}

impl Output {
    /// The `[[written]]` row id: `w-acadsharp-ac1032-dxf-binary-ac1015`.
    pub fn id(&self) -> String {
        let source = self.source.trim_start_matches("acadsharp-");
        format!(
            "w-{}-{}-{}-{}",
            self.writer,
            source,
            self.container,
            self.version.to_ascii_lowercase()
        )
    }

    /// The extension of the uncompressed file.
    pub fn extension(&self) -> &'static str {
        if self.container == "dwg" {
            "dwg"
        } else {
            "dxf"
        }
    }
}

pub fn plan() -> Plan {
    toml::from_str(PLAN).expect("fixtures/written_plan.toml must parse")
}

/// Every output of `stage` (`one` or `two`), in id order.
pub fn outputs(stage: &str) -> Vec<Output> {
    let plan = plan();
    let Some(stage) = plan.stage.get(stage) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for source in &stage.sources {
        for (name, w) in &plan.writer {
            for v in &w.dwg {
                out.push(Output {
                    writer: name.clone(),
                    source: source.clone(),
                    container: "dwg".into(),
                    version: v.clone(),
                });
            }
            for v in &w.dxf {
                for container in ["dxf-ascii", "dxf-binary"] {
                    out.push(Output {
                        writer: name.clone(),
                        source: source.clone(),
                        container: container.into(),
                        version: v.clone(),
                    });
                }
            }
        }
    }
    out.sort();
    out
}
