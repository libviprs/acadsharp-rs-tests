//! `tools/run-tests.sh`, the gate (R4.2, acadsharp-rs-tests#26).
//!
//! These drive `--plan`, which resolves the three trees and exits without
//! touching Docker, so they run anywhere a shell and git are. What the script
//! does with Docker is demonstrated on the x64 host instead, and the PR says so.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn script() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tools")
        .join("run-tests.sh")
}

fn scratch(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(name);
    if dir.exists() {
        std::fs::remove_dir_all(&dir).expect("I could not clear the scratch directory");
    }
    std::fs::create_dir_all(&dir).expect("I could not make a scratch directory");
    dir
}

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["-c", "user.name=t", "-c", "user.email=t@example.invalid"])
        .args(args)
        .output()
        .expect("git runs");
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// A one-commit repository, and the sha it is at.
fn repo(path: &Path) -> String {
    std::fs::create_dir_all(path).unwrap();
    git(path, &["init", "-q"]);
    std::fs::write(path.join("f"), "x").unwrap();
    git(path, &["add", "f"]);
    git(path, &["commit", "-q", "-m", "x"]);
    git(path, &["rev-parse", "HEAD"])
}

fn plan(extra: &[&str]) -> Output {
    Command::new("bash")
        .arg(script())
        .arg("--plan")
        .args(extra)
        .env_remove("VIPRS_REQUIRE_COUNTERPART")
        .output()
        .expect("bash runs")
}

#[test]
fn plan_names_the_three_trees_the_arch_and_the_four_require_variables() {
    let dir = scratch("gate_plan");
    let crate_sha = repo(&dir.join("crate"));
    let fork_sha = repo(&dir.join("fork"));
    let out = plan(&[
        "--acadsharp-rs",
        dir.join("crate").to_str().unwrap(),
        "--acadrust",
        dir.join("fork").to_str().unwrap(),
    ]);
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    assert!(
        out.status.success(),
        "--plan must exit 0: {text}{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        text.contains(&crate_sha),
        "the acadsharp-rs tree's sha is missing:\n{text}"
    );
    assert!(
        text.contains(&fork_sha),
        "the acadrust tree's sha is missing:\n{text}"
    );
    // This checkout is a row too, and the pins sit beside the trees.
    let pin = |file: &str| {
        std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join(file))
            .unwrap()
            .lines()
            .map(str::trim)
            .find(|l| !l.is_empty() && !l.starts_with('#'))
            .unwrap()
            .to_string()
    };
    assert!(
        text.contains(&pin("COUNTERPART_REV")),
        "COUNTERPART_REV is not beside its tree:\n{text}"
    );
    assert!(
        text.contains(&pin("ACADRUST_REV")),
        "ACADRUST_REV is not beside its tree:\n{text}"
    );
    assert!(text.contains("acadsharp-rs-tests"), "{text}");
    assert!(
        text.contains("amd64") || text.contains("arm64"),
        "no arch in:\n{text}"
    );
    for var in [
        "VIPRS_REQUIRE_COUNTERPART",
        "ACADSHARP_REQUIRE_NATIVE",
        "ACADRUST_REQUIRE",
        "PARITY_REQUIRE_CORPUS",
    ] {
        assert!(
            text.contains(&format!("{var}=1")),
            "{var}=1 is missing from the plan:\n{text}"
        );
    }
}

#[test]
fn plan_does_not_need_docker() {
    // An empty PATH apart from the tools the script itself calls: no docker.
    let dir = scratch("gate_plan_nodocker");
    repo(&dir.join("crate"));
    repo(&dir.join("fork"));
    let bin = dir.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    for tool in [
        "bash", "git", "awk", "tr", "uname", "dirname", "basename", "cat", "sed", "env", "head",
        "wc",
    ] {
        if let Ok(p) = Command::new("sh")
            .args(["-c", &format!("command -v {tool}")])
            .output()
        {
            let p = String::from_utf8_lossy(&p.stdout).trim().to_string();
            if !p.is_empty() {
                let _ = std::os::unix::fs::symlink(&p, bin.join(tool));
            }
        }
    }
    let out = Command::new(bin.join("bash"))
        .arg(script())
        .args(["--plan", "--acadsharp-rs"])
        .arg(dir.join("crate"))
        .arg("--acadrust")
        .arg(dir.join("fork"))
        .env("PATH", &bin)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn a_tree_that_is_not_a_checkout_shows_as_none_and_does_not_borrow_the_parent() {
    let dir = scratch("gate_plan_notarepo");
    repo(&dir.join("crate"));
    std::fs::create_dir_all(dir.join("fork")).unwrap();
    let out = plan(&[
        "--acadsharp-rs",
        dir.join("crate").to_str().unwrap(),
        "--acadrust",
        dir.join("fork").to_str().unwrap(),
    ]);
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    let line = text.lines().find(|l| l.starts_with("acadrust ")).unwrap();
    assert!(
        line.contains(" none "),
        "an empty slot must read as none, not as a parent repository: {line}"
    );
}

#[test]
fn an_unknown_argument_is_called_wrong() {
    let out = Command::new("bash")
        .arg(script())
        .arg("--no-such-flag")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
}

#[test]
fn the_ci_form_does_not_pin_the_trees_to_their_own_heads() {
    // `--in-ci` takes the expected shas from the job, not from the trees.
    let dir = scratch("gate_plan_ci");
    repo(&dir.join("crate"));
    repo(&dir.join("fork"));
    let out = Command::new("bash")
        .arg(script())
        .args(["--plan", "--in-ci", "--acadsharp-rs"])
        .arg(dir.join("crate"))
        .arg("--acadrust")
        .arg(dir.join("fork"))
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(!text.contains("EXPECTED_SHA"), "{text}");
}

#[test]
fn plan_names_the_base_table_the_verdicts_are_held_to_a_second_time() {
    let dir = scratch("gate_plan_base");
    repo(&dir.join("crate"));
    repo(&dir.join("fork"));
    let out = plan(&[
        "--acadsharp-rs",
        dir.join("crate").to_str().unwrap(),
        "--acadrust",
        dir.join("fork").to_str().unwrap(),
        "--verdicts-base",
        "0123456789abcdef0123456789abcdef01234567",
    ]);
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        text.lines()
            .any(|l| l.starts_with("verdicts-base") && l.contains("0123456789abcdef")),
        "{text}"
    );
}
