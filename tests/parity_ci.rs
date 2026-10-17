//! `ci.yml` has a cell that runs the parity lane (R4.1, #25).
//!
//! The parity test is behind the `acadrust` feature, so a CI that never turns the
//! feature on compiles it out and goes green having compared nothing, which is
//! the colour of a pass. This walks the workflow the way `tests/counterpart_pin.rs`
//! does, as YAML, and asks for a job that enables the feature (directly, or
//! through the gate script that does) and sets `PARITY_REQUIRE_CORPUS: 1` at job
//! level, where the `env:` lesson the crate's `Test` job comment spells out
//! says it has to be.

use yaml_rust2::{Yaml, YamlLoader};

const CI: &str = include_str!("../.github/workflows/ci.yml");

fn jobs() -> Vec<(String, Yaml)> {
    let docs = YamlLoader::load_from_str(CI).expect("ci.yml parses");
    let jobs = docs[0]["jobs"].as_hash().expect("ci.yml has jobs");
    jobs.iter()
        .map(|(k, v)| (k.as_str().unwrap().to_string(), v.clone()))
        .collect()
}

fn runs(job: &Yaml) -> Vec<String> {
    job["steps"]
        .as_vec()
        .into_iter()
        .flatten()
        .filter_map(|s| s["run"].as_str().map(str::to_string))
        .collect()
}

fn env_value(job: &Yaml, key: &str) -> Option<String> {
    match &job["env"][key] {
        Yaml::Integer(i) => Some(i.to_string()),
        Yaml::String(s) => Some(s.clone()),
        _ => None,
    }
}

#[test]
fn some_job_enables_the_acadrust_feature_and_requires_the_corpus() {
    let cells: Vec<String> = jobs()
        .into_iter()
        .filter(|(_, job)| {
            env_value(job, "PARITY_REQUIRE_CORPUS").as_deref() == Some("1")
                && runs(job).iter().any(|r| {
                    r.contains("--features acadrust") || r.contains("tools/run-tests.sh --in-ci")
                })
        })
        .map(|(name, _)| name)
        .collect();
    assert!(
        !cells.is_empty(),
        "no job in ci.yml both enables the acadrust feature (or runs tools/run-tests.sh --in-ci) \
         and sets PARITY_REQUIRE_CORPUS: 1 at job level, so tests/parity.rs can be compiled out everywhere"
    );
}

#[test]
fn the_job_that_runs_it_also_requires_the_native_archive() {
    for (name, job) in jobs() {
        if env_value(&job, "PARITY_REQUIRE_CORPUS").as_deref() == Some("1") {
            assert_eq!(
                env_value(&job, "ACADSHARP_REQUIRE_NATIVE").as_deref(),
                Some("1"),
                "job {name} requires the corpus but not the native archive, so emit-acadsharp could answer Unlinked and be skipped"
            );
        }
    }
}
