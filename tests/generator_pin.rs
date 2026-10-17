//! The DWG generator under `reference/ACadSharp.Generator`: what has to stay
//! true of it for a generated fixture to mean anything.
//!
//! These read files and nothing else. They need no .NET, no network and no
//! native library, so they run in the ordinary `cargo test`. The behaviour
//! (the bytes are the same on every write) is checked by the generator's own
//! `selftest` command, which needs the SDK, and `reference/ACadSharp.Generator/README.md`
//! says how to run it.

use std::path::PathBuf;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn read(rel: &str) -> String {
    let path = root().join(rel);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{} must exist and be readable: {e}", path.display()))
}

/// The version in the first `PackageReference Include="ACadSharp"` of a csproj.
fn acadsharp_version(csproj: &str) -> String {
    let at = csproj
        .find("Include=\"ACadSharp\"")
        .expect("a csproj that references ACadSharp");
    let rest = &csproj[at..];
    let v = rest.find("Version=\"").expect("a Version on the reference") + "Version=\"".len();
    rest[v..].split('"').next().unwrap().to_string()
}

fn locked_version(lock: &str) -> String {
    let at = lock
        .find("\"ACadSharp\"")
        .expect("ACadSharp in the lock file");
    let rest = &lock[at..];
    let r = rest.find("\"resolved\": \"").expect("a resolved version") + "\"resolved\": \"".len();
    rest[r..].split('"').next().unwrap().to_string()
}

#[test]
fn the_generator_and_the_oracle_use_one_acadsharp_version() {
    // A fixture written by one ACadSharp and read by another is not a bug in
    // either, and the differential would blame the wrong one.
    let oracle = read("reference/ACadSharp.Reference/ACadSharp.Reference.csproj");
    let generator = read("reference/ACadSharp.Generator/ACadSharp.Generator.csproj");
    assert_eq!(acadsharp_version(&generator), acadsharp_version(&oracle));

    let oracle_lock = read("reference/ACadSharp.Reference/packages.lock.json");
    let generator_lock = read("reference/ACadSharp.Generator/packages.lock.json");
    assert_eq!(
        locked_version(&generator_lock),
        locked_version(&oracle_lock)
    );
    assert_eq!(
        locked_version(&generator_lock),
        acadsharp_version(&generator),
        "the lock file resolves a version the csproj did not ask for"
    );
}

#[test]
fn the_generator_restores_from_a_lock_file_and_is_not_aot() {
    let csproj = read("reference/ACadSharp.Generator/ACadSharp.Generator.csproj");
    assert!(csproj.contains("<RestorePackagesWithLockFile>true</RestorePackagesWithLockFile>"));
    assert!(csproj.contains("<PublishAot>false</PublishAot>"));
    assert!(csproj.contains("<InvariantGlobalization>true</InvariantGlobalization>"));
    assert_eq!(
        csproj.matches("<PackageReference").count(),
        1,
        "ACadSharp is the only package the generator may have"
    );
    assert!(!csproj.contains("<ProjectReference"));
}

#[test]
fn the_generator_pins_everything_dwgwriter_stamps() {
    // DwgWriter writes the time and two GUIDs into the file. Each of these has
    // to be set to a constant or two runs of one program write two files, and a
    // manifest sha256 stops meaning anything. The behaviour is `selftest`'s;
    // this is the cheap check that nobody deleted a line while it still passes.
    let core = read("reference/ACadSharp.Generator/Core.cs");
    for needle in [
        "Header.CreateDateTime",
        "Header.UpdateDateTime",
        "Header.UniversalCreateDateTime",
        "Header.UniversalUpdateDateTime",
        "Header.TotalEditingTime",
        "Header.UserElapsedTimeSpan",
        "\"FingerPrintGuid\"",
        "\"VersionGuid\"",
        "SummaryInfo.CreatedDate",
        "SummaryInfo.ModifiedDate",
    ] {
        assert!(core.contains(needle), "Core.cs no longer pins {needle}");
    }
}

#[test]
fn the_generator_never_uses_the_clock_or_randomness() {
    let dir = root().join("reference/ACadSharp.Generator");
    let mut checked = 0;
    for entry in walk(&dir) {
        let text = std::fs::read_to_string(&entry).unwrap();
        for banned in [
            "DateTime.Now",
            "DateTime.UtcNow",
            "Guid.NewGuid",
            "new Random()",
        ] {
            assert!(
                !text.contains(banned),
                "{} uses {banned}, which makes the bytes depend on the run",
                entry.display()
            );
        }
        checked += 1;
    }
    assert!(
        checked >= 3,
        "found only {checked} generator sources; is the path right?"
    );
}

fn walk(dir: &std::path::Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for e in std::fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        let name = p.file_name().unwrap().to_string_lossy().to_string();
        if p.is_dir() {
            if name != "bin" && name != "obj" {
                out.extend(walk(&p));
            }
        } else if name.ends_with(".cs") {
            out.push(p);
        }
    }
    out
}

#[test]
fn every_drawing_file_is_in_the_catalog_and_every_catalog_source_exists() {
    // A drawing nobody listed is never written, and a catalog entry whose
    // `Source` names a file that is not there gives a manifest row a
    // provenance field that points at nothing. Two directories: `Drawings/` holds
    // the H4.4 regression classes (names `regression_*`), `Parity/` the H4.2
    // families (names `parity_*`, classes `*Fixture` so they cannot shadow the
    // ACadSharp entity of the same name).
    let catalog = read("reference/ACadSharp.Generator/Catalog.cs");
    for (sub, prefix, want) in [("Drawings", "regression_", 7), ("Parity", "parity_", 10)] {
        let dir = root().join("reference/ACadSharp.Generator").join(sub);
        let mut seen = 0;
        for entry in std::fs::read_dir(&dir).unwrap() {
            let p = entry.unwrap().path();
            let stem = p.file_stem().unwrap().to_string_lossy().to_string();
            assert!(
                catalog.contains(&format!("{stem}.Drawing")),
                "{sub}/{stem}.cs is not listed in Catalog.All"
            );
            let text = std::fs::read_to_string(&p).unwrap();
            let source = format!("reference/ACadSharp.Generator/{sub}/{stem}.cs");
            assert!(
                text.contains(&format!("\"{source}\"")),
                "{stem}.cs names a different Source than its own path"
            );
            assert!(
                text.contains(&format!("public const string Name = \"{prefix}")),
                "{stem}.cs: a drawing in {sub}/ is named `{prefix}<class>`"
            );
            seen += 1;
        }
        assert_eq!(seen, want, "the drawings in {sub}/");
    }
}
