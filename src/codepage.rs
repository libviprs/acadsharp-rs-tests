//! The code page fixtures' plan (R5.4, acadsharp-rs-tests#30).
//!
//! `fixtures/codepage_plan.toml` lists one authored drawing per code page family;
//! this reads it and names the outputs. The files are `[[authored]]` rows of
//! `fixtures/manifest.toml`, made by `tools/regenerate_codepage.py`.

use serde::Deserialize;

const PLAN: &str = include_str!("../fixtures/codepage_plan.toml");

#[derive(Debug, Clone, Deserialize)]
pub struct Family {
    pub page: u32,
    /// The header's code page string, for example `ANSI_1250`.
    pub header: String,
    pub held: String,
    pub literal: String,
    #[serde(default)]
    pub mif: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct Unheld {
    pub text: String,
}

#[derive(Debug, Deserialize)]
pub struct Plan {
    pub versions: Vec<String>,
    pub writers: Vec<String>,
    pub containers: Vec<String>,
    pub family: Vec<Family>,
    pub unheld: Unheld,
}

pub fn plan() -> Plan {
    toml::from_str(PLAN).expect("fixtures/codepage_plan.toml must parse")
}

/// One authored string and what kind of string it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Authored {
    /// `held`, `unheld`, `literal` or `mif`.
    pub kind: &'static str,
    pub text: String,
}

impl Family {
    /// The strings of the drawing, in the order they are written.
    pub fn texts(&self, unheld: &str) -> Vec<Authored> {
        let mut v = vec![
            Authored {
                kind: "held",
                text: self.held.clone(),
            },
            Authored {
                kind: "unheld",
                text: unheld.to_string(),
            },
            Authored {
                kind: "literal",
                text: self.literal.clone(),
            },
        ];
        if let Some(m) = &self.mif {
            v.push(Authored {
                kind: "mif",
                text: m.clone(),
            });
        }
        v
    }
}

/// One output the plan asks for.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Output {
    pub writer: String,
    pub page: u32,
    pub container: String,
    pub version: String,
}

impl Output {
    /// The `[[authored]]` row id: `cp-acadsharp-1250-dwg-ac1015`.
    pub fn id(&self) -> String {
        format!(
            "cp-{}-{}-{}-{}",
            self.writer,
            self.page,
            self.container,
            self.version.to_ascii_lowercase()
        )
    }

    pub fn extension(&self) -> &'static str {
        if self.container == "dwg" {
            "dwg"
        } else {
            "dxf"
        }
    }
}

/// Every output of the plan, in id order.
pub fn outputs() -> Vec<Output> {
    let p = plan();
    let mut out = Vec::new();
    for f in &p.family {
        for w in &p.writers {
            for c in &p.containers {
                for v in &p.versions {
                    out.push(Output {
                        writer: w.clone(),
                        page: f.page,
                        container: c.clone(),
                        version: v.clone(),
                    });
                }
            }
        }
    }
    out.sort();
    out
}
