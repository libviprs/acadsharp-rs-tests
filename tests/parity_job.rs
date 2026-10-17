//! The `parity` job runs the gate script and nothing else (R4.2.1, #44).
//!
//! The job is wiring, and wiring that nothing checks drifts into a copy of the
//! script's cargo lines. This walks `ci.yml` as YAML, like
//! `tests/counterpart_pin.rs`, and asserts the shape that keeps it honest: both
//! siblings laid down before anything builds, the toolchain, then
//! `tools/run-tests.sh --in-ci` as the last step, with the four REQUIRE
//! variables at job level and the script's `--plan` printing the same four.

use std::path::{Path, PathBuf};
use std::process::Command;

use yaml_rust2::{Yaml, YamlLoader};

const CI: &str = include_str!("../.github/workflows/ci.yml");
const FOUR: [&str; 4] = [
    "VIPRS_REQUIRE_COUNTERPART",
    "ACADSHARP_REQUIRE_NATIVE",
    "ACADRUST_REQUIRE",
    "PARITY_REQUIRE_CORPUS",
];

fn parity_job() -> Yaml {
    let docs = YamlLoader::load_from_str(CI).expect("ci.yml parses");
    docs[0]["jobs"]["parity"].clone()
}

fn steps(job: &Yaml) -> Vec<Yaml> {
    job["steps"].as_vec().cloned().unwrap_or_default()
}

fn uses(step: &Yaml) -> &str {
    step["uses"].as_str().unwrap_or("")
}

#[test]
fn the_parity_job_exists_and_is_called_plain() {
    let job = parity_job();
    assert!(!job.is_badvalue(), "ci.yml has no `parity` job");
    let name = job["name"].as_str().unwrap_or("");
    assert_eq!(
        name, "Parity",
        "no version or sha in a required check's name"
    );
}

#[test]
fn it_lays_down_both_siblings_before_the_toolchain_and_runs_the_gate_last() {
    let steps = steps(&parity_job());
    let kinds: Vec<String> = steps
        .iter()
        .map(|s| {
            if uses(s).starts_with("actions/checkout") {
                "checkout".to_string()
            } else if uses(s) == "./.github/actions/clone-counterpart" {
                match s["with"]["pin"].as_str() {
                    Some("ACADRUST_REV") => "clone:ACADRUST_REV".to_string(),
                    None => "clone:COUNTERPART_REV".to_string(),
                    Some(other) => format!("clone:{other}"),
                }
            } else if uses(s).starts_with("dtolnay/rust-toolchain") {
                "toolchain".to_string()
            } else if uses(s).starts_with("Swatinem/rust-cache") {
                "cache".to_string()
            } else if let Some(run) = s["run"].as_str() {
                format!("run:{}", run.trim())
            } else {
                format!("other:{}", uses(s))
            }
        })
        .collect();
    assert_eq!(
        kinds,
        [
            "checkout",
            "clone:COUNTERPART_REV",
            "clone:ACADRUST_REV",
            "toolchain",
            "cache",
            "run:tools/run-tests.sh --in-ci",
        ],
        "the job is the siblings, the toolchain and the gate script, in that order"
    );
}

#[test]
fn the_four_require_variables_are_at_job_level_and_nothing_here_is_conditional() {
    let job = parity_job();
    for var in FOUR {
        let v = match &job["env"][var] {
            Yaml::Integer(i) => i.to_string(),
            Yaml::String(s) => s.clone(),
            other => panic!("{var} is {other:?} at job level"),
        };
        assert_eq!(v, "1", "{var}");
    }
    assert!(
        job["if"].is_badvalue(),
        "a conditional job can go green having tested nothing"
    );
    for s in steps(&job) {
        assert!(s["if"].is_badvalue(), "a conditional step: {s:?}");
        assert!(
            s["continue-on-error"].is_badvalue(),
            "a step that swallows failure: {s:?}"
        );
    }
}

#[test]
fn the_scripts_plan_prints_the_same_four() {
    let script: PathBuf = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tools")
        .join("run-tests.sh");
    let out = Command::new("bash")
        .arg(script)
        .arg("--plan")
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    for var in FOUR {
        assert!(
            text.contains(&format!("{var}=1")),
            "--plan does not print {var}:\n{text}"
        );
    }
}
