//! Provenance of the generated drawings: every `source = "generated"` row says
//! what wrote it, and says it truthfully.

use std::path::PathBuf;

use acadsharp_rs_tests::fixtures;
use sha2::{Digest, Sha256};

const GENERATOR: &str = "reference/ACadSharp.Generator";

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn generated() -> Vec<&'static fixtures::DwgFixture> {
    fixtures::all()
        .iter()
        .filter(|f| f.source.as_deref() == Some("generated"))
        .collect()
}

/// The names the generator's `Drawings/*.cs` declare, read from the source.
fn catalog_names() -> Vec<String> {
    let dir = root().join(GENERATOR).join("Drawings");
    let mut names = Vec::new();
    for e in std::fs::read_dir(dir).unwrap() {
        let text = std::fs::read_to_string(e.unwrap().path()).unwrap();
        let at = text.find("public const string Name = \"").unwrap() + "public const string Name = \"".len();
        names.push(text[at..].split('"').next().unwrap().to_string());
    }
    names.sort();
    names
}

#[test]
fn every_catalog_drawing_has_a_row_and_every_generated_row_has_a_drawing() {
    let mut rows: Vec<String> = generated().iter().map(|f| f.id.clone()).collect();
    rows.sort();
    assert_eq!(rows, catalog_names());
}

#[test]
fn a_generated_row_names_its_generator_and_that_generator_source_is_the_one_hashed() {
    assert!(!generated().is_empty(), "no generated rows at all");
    for f in generated() {
        assert_eq!(f.generated_by.as_deref(), Some(GENERATOR), "{}", f.id);
        let src = f.generator_source.as_deref().unwrap_or_else(|| panic!("{}: no generator_source", f.id));
        assert!(src.starts_with(&format!("{GENERATOR}/Drawings/")), "{}: {src}", f.id);
        let bytes = std::fs::read(root().join(src)).unwrap_or_else(|e| panic!("{}: {src}: {e}", f.id));
        let got = format!("{:x}", Sha256::digest(&bytes));
        assert_eq!(
            Some(got.as_str()),
            f.generator_source_sha256.as_deref(),
            "{}: {src} changed since the row was written; regenerate the drawing and the row",
            f.id
        );
        assert_eq!(f.source_path, src, "{}", f.id);
    }
}

#[test]
fn generated_drawings_were_written_by_the_acadsharp_the_oracle_reads_with() {
    let lock = std::fs::read_to_string(root().join(GENERATOR).join("packages.lock.json")).unwrap();
    let at = lock.find("\"resolved\": \"").unwrap() + "\"resolved\": \"".len();
    let locked = lock[at..].split('"').next().unwrap();
    for f in generated() {
        assert_eq!(f.acadsharp_generator_version.as_deref(), Some(locked), "{}", f.id);
        assert_eq!(f.acadsharp_reference_version.as_deref(), Some(locked), "{}", f.id);
        assert_eq!(f.tier.as_deref(), Some("B"), "{}: a generated drawing is regression grade", f.id);
        assert!(f.file.starts_with("generated/dwg/"), "{}: {}", f.id, f.file);
        assert!(f.file.ends_with(&format!("{}.dwg", f.id)), "{}: {}", f.id, f.file);
        assert_eq!(f.acad_version, "AC1032", "{}: the generator's default target", f.id);
    }
}
