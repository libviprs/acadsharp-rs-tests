//! `NATIVE_ARTIFACT.toml` and the wiring that consumes it, checked with no
//! network and no archive.
//!
//! The manifest names a libviprs-dep release and a sha256 per asset. What the
//! fetch tool does with them is `tools/tests/test_fetch_native.py`; this file
//! holds the shape rules (so a typo cannot reach CI as a download that 404s or
//! a hash that can never match) and the facts about `ci.yml` that make the
//! native lane a lane: both jobs exist under exactly these names, both fetch
//! through the action, both demand the library, and both run the feature.

use std::fs;
use std::path::PathBuf;

use yaml_rust2::{Yaml, YamlLoader};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn read(relative: &str) -> String {
    let path = root().join(relative);
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("I could not read {}: {e}", path.display()))
}

fn manifest() -> toml::Table {
    read("NATIVE_ARTIFACT.toml")
        .parse()
        .expect("NATIVE_ARTIFACT.toml is not TOML")
}

fn yaml(relative: &str) -> Yaml {
    YamlLoader::load_from_str(&read(relative))
        .unwrap_or_else(|e| panic!("{relative} is not YAML: {e}"))
        .remove(0)
}

/// libviprs-dep's rule: `acadsharp-<platform>-<cpu>.tgz`.
fn follows_the_naming_rule(asset: &str) -> bool {
    let Some(stem) = asset
        .strip_prefix("acadsharp-")
        .and_then(|s| s.strip_suffix(".tgz"))
    else {
        return false;
    };
    let Some((platform, cpu)) = stem.split_once('-') else {
        return false;
    };
    matches!(platform, "linux" | "musl" | "mac") && matches!(cpu, "x64" | "arm64")
}

#[test]
fn the_release_tag_is_a_libviprs_dep_acadsharp_tag() {
    let m = manifest();
    let tag = m["release_tag"].as_str().expect("release_tag is a string");
    assert!(tag.starts_with("acadsharp-"), "release_tag {tag:?}");
    assert!(
        !tag.contains('/') && !tag.contains(".."),
        "release_tag {tag:?} is not a plain tag"
    );
}

#[test]
fn five_targets_each_with_a_64_hex_hash_and_a_conforming_asset_name() {
    let m = manifest();
    let targets = m["target"].as_array().expect("[[target]] rows");
    let names: Vec<&str> = targets
        .iter()
        .map(|t| t["name"].as_str().expect("name"))
        .collect();
    assert_eq!(
        names,
        [
            "linux-x64",
            "linux-arm64",
            "musl-x64",
            "musl-arm64",
            "mac-arm64"
        ],
        "the five targets the lane covers, in this order"
    );

    let mut hashes = Vec::new();
    for t in targets {
        let name = t["name"].as_str().unwrap();
        let asset = t["asset"].as_str().expect("asset");
        let sha = t["sha256"].as_str().expect("sha256");
        assert!(
            follows_the_naming_rule(asset),
            "{name}: {asset:?} does not match acadsharp-<platform>-<cpu>.tgz"
        );
        assert_eq!(
            asset,
            format!("acadsharp-{name}.tgz"),
            "{name}: the asset must be named for its target"
        );
        assert!(
            sha.len() == 64 && sha.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')),
            "{name}: sha256 {sha:?} is not 64 lowercase hex characters"
        );
        hashes.push(sha);
    }
    hashes.sort_unstable();
    hashes.dedup();
    assert_eq!(
        hashes.len(),
        5,
        "two targets share a hash, which no two archives do"
    );
}

#[test]
fn the_naming_rule_check_refuses_what_it_should() {
    // The control for the test above: a predicate that accepts everything would
    // make that test pass for any manifest.
    for good in [
        "acadsharp-linux-x64.tgz",
        "acadsharp-mac-arm64.tgz",
        "acadsharp-musl-arm64.tgz",
    ] {
        assert!(follows_the_naming_rule(good), "{good}");
    }
    for bad in [
        "acadsharp-linux.tgz",
        "acadsharp-windows-x64.tgz",
        "acadsharp-linux-x64.tar.gz",
        "libacadsharp-linux-x64.tgz",
        "acadsharp-linux-x86.tgz",
        "",
    ] {
        assert!(!follows_the_naming_rule(bad), "{bad:?}");
    }
}

#[test]
fn the_native_feature_exists_and_is_off_by_default() {
    let cargo: toml::Table = read("Cargo.toml").parse().unwrap();
    let features = cargo["features"].as_table().expect("[features]");
    assert!(features.contains_key("native"), "no `native` feature");
    let default = features.get("default").and_then(|d| d.as_array());
    assert!(
        default.is_none_or(|d| d.iter().all(|f| f.as_str() != Some("native"))),
        "`native` must not be a default feature: a plain cargo test has no archive"
    );
}

const NATIVE_JOBS: [(&str, &str, &str); 2] = [
    (
        "native-linux-x64",
        "Native (linux-x64, shared)",
        "linux-x64",
    ),
    (
        "native-mac-arm64",
        "Native (mac-arm64, shared)",
        "mac-arm64",
    ),
];

#[test]
fn both_native_jobs_exist_under_their_required_names_and_demand_the_library() {
    let ci = yaml(".github/workflows/ci.yml");
    for (key, name, target) in NATIVE_JOBS {
        let job = &ci["jobs"][key];
        assert_eq!(job["name"].as_str(), Some(name), "job {key}'s name");

        let env = &job["env"];
        for var in ["VIPRS_REQUIRE_NATIVE", "ACADSHARP_REQUIRE_NATIVE"] {
            // YAML reads the bare 1 as an integer; accept either spelling.
            let set = env[var].as_i64() == Some(1) || env[var].as_str() == Some("1");
            assert!(
                set,
                "{key} must set {var}=1, or a missing archive is a skip"
            );
        }

        let steps = job["steps"].as_vec().expect("steps");
        let fetch = steps
            .iter()
            .position(|s| s["uses"].as_str() == Some("./.github/actions/fetch-native-artifact"))
            .unwrap_or_else(|| panic!("{key} never fetches the archive through the action"));
        assert_eq!(
            steps[fetch]["with"]["target"].as_str(),
            Some(target),
            "{key} fetches the wrong target"
        );
        let test = steps
            .iter()
            .position(|s| {
                s["run"]
                    .as_str()
                    .is_some_and(|r| r.starts_with("cargo test") && r.contains("--features native"))
            })
            .unwrap_or_else(|| panic!("{key} never runs cargo test with --features native"));
        assert!(
            fetch < test,
            "{key} runs the tests before it has an archive"
        );
        let exported = steps.iter().position(|s| {
            s["run"]
                .as_str()
                .is_some_and(|r| r.contains("ACADSHARP_NATIVE_DIR=") && r.contains("GITHUB_ENV"))
        });
        assert!(
            exported.is_some_and(|e| fetch < e && e < test),
            "{key} must export ACADSHARP_NATIVE_DIR between the fetch and the tests"
        );
        assert!(
            steps.iter().any(|s| s["run"]
                .as_str()
                .is_some_and(|r| r.contains("tools/no_dotnet_proof.sh"))),
            "{key} has no no-.NET proof step"
        );
    }
    assert_eq!(
        ci["jobs"]["native-linux-x64"]["runs-on"].as_str(),
        Some("ubuntu-latest")
    );
    assert_eq!(
        ci["jobs"]["native-mac-arm64"]["runs-on"].as_str(),
        Some("macos-15")
    );
}

#[test]
fn the_linux_job_proves_it_in_a_clean_container_and_runs_the_control() {
    let script = read("tools/no_dotnet_proof.sh");
    assert!(
        script.contains("debian:bookworm-slim"),
        "the clean container"
    );
    assert!(
        script.contains("framework-dependent"),
        "the positive control that a framework-dependent .NET app fails to start"
    );
    // The control has to run before the smoke, or a clean container is assumed.
    let control = script.find("\n  control\n").expect("linux() calls control");
    let smoke = script
        .find("running $(basename")
        .expect("linux() runs the smoke");
    assert!(control < smoke, "the control must precede the smoke");
}

#[test]
fn the_fetch_action_passes_the_target_as_data_and_has_no_way_to_pick_a_release() {
    let action = read(".github/actions/fetch-native-artifact/action.yml");
    let doc = yaml(".github/actions/fetch-native-artifact/action.yml");
    let inputs = doc["inputs"].as_hash().expect("inputs");
    let names: Vec<&str> = inputs.keys().filter_map(Yaml::as_str).collect();
    assert_eq!(names, ["target"], "the only input is the target");
    let run = doc["runs"]["steps"][0]["run"].as_str().expect("run block");
    assert!(
        !run.contains("${{"),
        "the input must reach the script through env, not by interpolation: {run}"
    );
    assert!(action.contains("tools/fetch_native.py"));
}

#[test]
fn the_readme_documents_the_variable_the_cache_and_the_local_run() {
    let readme = read("README.md");
    for needle in [
        "ACADSHARP_NATIVE_DIR",
        "ACADSHARP_NATIVE_CACHE",
        "fetch_native.py",
        "--features native",
    ] {
        assert!(readme.contains(needle), "README.md never mentions {needle}");
    }
}

#[test]
fn the_leak_cell_exists_runs_on_nightly_and_compares_two_loop_lengths() {
    let ci = yaml(".github/workflows/ci.yml");
    let job = &ci["jobs"]["native-linux-x64-lsan"];
    assert_eq!(job["name"].as_str(), Some("Native (linux-x64, lsan)"));
    let steps = job["steps"].as_vec().expect("steps");
    assert!(
        steps
            .iter()
            .any(|s| s["uses"].as_str() == Some("dtolnay/rust-toolchain@nightly")),
        "-Zsanitizer needs nightly"
    );
    assert!(
        steps.iter().any(|s| s["run"]
            .as_str()
            .is_some_and(|r| r.contains("tools/lsan_cell.sh"))),
        "the cell never runs the script"
    );
    let script = read("tools/lsan_cell.sh");
    // A clean report is not the pass condition (the runtime keeps a constant
    // startup baseline); growth with the loop length is the failure.
    assert!(script.contains("-Zsanitizer=leak"));
    assert!(script.contains("GREW with the loop length"));
    assert!(
        script.contains("LSAN_LONG_ITERATIONS") && script.contains("LSAN_SHORT_ITERATIONS"),
        "two loop lengths"
    );
    // And it says what LSan cannot see, so nobody reads a green cell as more.
    assert!(
        script.contains("GC heap"),
        "the script must say what LSan does not see"
    );
}

// ---- the lane matrix ---------------------------------------------------------

#[test]
fn the_matrix_is_planned_from_the_manifest_and_every_cell_runs_the_cell_script() {
    let ci = yaml(".github/workflows/ci.yml");

    let plan = &ci["jobs"]["native-plan"];
    let plan_runs: String = plan["steps"]
        .as_vec()
        .expect("steps")
        .iter()
        .filter_map(|s| s["run"].as_str())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        plan_runs.contains("tools/native_cells.py"),
        "the plan must come from the manifest, with no download"
    );
    assert!(
        !plan_runs.contains("cargo"),
        "the planner runs no cargo (it is exempt from the sibling clones for that reason)"
    );

    let matrix = &ci["jobs"]["native-matrix"];
    assert_eq!(matrix["name"].as_str(), Some("${{ matrix.name }}"));
    assert_eq!(matrix["needs"].as_str(), Some("native-plan"));
    assert_eq!(
        matrix["strategy"]["fail-fast"].as_bool(),
        Some(false),
        "one broken cell must not hide the others"
    );
    assert!(
        matrix["strategy"]["matrix"]
            .as_str()
            .is_some_and(|m| m.contains("fromJSON(needs.native-plan.outputs.matrix)")),
        "the matrix must be the planner's output"
    );
    let runs: String = matrix["steps"]
        .as_vec()
        .expect("steps")
        .iter()
        .filter_map(|s| s["run"].as_str())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        runs.contains("tools/native_cell.sh"),
        "cells run through the one script"
    );
    // The matrix values reach the script through env, never interpolated into it.
    assert!(
        !runs.contains("${{"),
        "a matrix value was interpolated into a shell line: {runs}"
    );
}

#[test]
fn the_comparison_job_needs_every_cell_and_compares_bytes() {
    let ci = yaml(".github/workflows/ci.yml");
    let job = &ci["jobs"]["native-compare"];
    assert_eq!(job["name"].as_str(), Some("Native (shared versus static)"));
    let needs: Vec<&str> = job["needs"]
        .as_vec()
        .expect("needs is a list")
        .iter()
        .filter_map(Yaml::as_str)
        .collect();
    for n in ["native-linux-x64", "native-mac-arm64", "native-matrix"] {
        assert!(needs.contains(&n), "native-compare does not wait for {n}");
    }
    let steps = job["steps"].as_vec().expect("steps");
    let runs: String = steps
        .iter()
        .filter_map(|s| s["run"].as_str())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(runs.contains("tools/compare_cells.py"));
    assert!(
        !runs.contains("cargo"),
        "a job that runs no cargo is only exempt from the sibling clones while it stays one"
    );
    // The two H3.2 jobs hand their output over under the names the comparison
    // reads.
    for (key, artifact) in [
        ("native-linux-x64", "cell-linux-x64-shared"),
        ("native-mac-arm64", "cell-mac-arm64-shared"),
    ] {
        let upload = ci["jobs"][key]["steps"]
            .as_vec()
            .expect("steps")
            .iter()
            .any(|s| {
                s["uses"]
                    .as_str()
                    .is_some_and(|u| u.starts_with("actions/upload-artifact"))
                    && s["with"]["name"].as_str() == Some(artifact)
            });
        assert!(upload, "{key} never uploads {artifact}");
    }
}

#[test]
fn the_cell_script_turns_static_into_the_crates_feature_and_musl_into_a_container_run() {
    let script = read("tools/native_cell.sh");
    assert!(
        script.contains("acadsharp-rs/link-static"),
        "the static cell must ask the crate for a static link"
    );
    assert!(
        script.contains("VIPRS_CELL_MODE"),
        "the cell must tell fixture_parity which link it intends"
    );
    assert!(
        script.contains("-crt-static"),
        "a musl cell cannot load a shared library with a static libc"
    );
    assert!(script.contains("docker run"), "musl cells run in an image");
}

#[test]
fn the_lane_test_holds_a_cell_to_its_mode_and_writes_the_output_the_comparison_reads() {
    let test = read("tests/fixture_parity.rs");
    for needle in [
        "VIPRS_CELL_MODE",
        "VIPRS_CELL_OUT",
        "SUITE_NATIVE_LINK_KIND",
        ".acadsharp.jsonl",
    ] {
        assert!(
            test.contains(needle),
            "tests/fixture_parity.rs never mentions {needle}"
        );
    }
    let build = read("build.rs");
    assert!(
        build.contains("SUITE_NATIVE_LINK_KIND"),
        "build.rs must hand the link kind to the lane test"
    );
}
