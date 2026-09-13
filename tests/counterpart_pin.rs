//! The cross-repo pin: does this checkout know which `acadsharp-rs` it is
//! testing, and can it be talked into building against a different one.
//!
//! This suite depends on the crate as `{ path = "../acadsharp-rs" }`, so
//! something has to lay that checkout down beside this one, and that something
//! decides what every green run here actually means. The org has two scars from
//! getting it wrong: `libviprs-org` resolved an unset repository variable to the
//! empty string, which `actions/checkout` reads as "the default branch", so its
//! gate compared against whatever moved that morning; and `libviprs`'
//! integration job clones a same-named counterpart branch, so a stale branch
//! that happens to share a name builds against the wrong tree (libviprs#1013).
//!
//! So the rule is a committed pin, one sha, fetched as a sha, and a refusal
//! rather than a fallback. The rule lives in `tools/counterpart.sh` and this
//! file drives that script rather than reimplementing it, because CI runs the
//! script and a test that reimplements a rule can only ever prove the copy.
//!
//! Two environment variables steer the one part of this that is a real runtime
//! question:
//!
//! - `VIPRS_REQUIRE_COUNTERPART=1` turns "I cannot tell which commit the
//!   sibling is at" from a printed reason into a failure. CI sets it.
//! - `VIPRS_COUNTERPART_EXPECTED_SHA` replaces this repo's own pin, for the
//!   crate's `Suite (acadsharp-rs-tests)` job, where the sibling is deliberately
//!   the crate PR under test rather than the commit this repo pins.
//!
//! There is deliberately no "the sibling is missing" skip. With a hard path
//! dependency and no sibling, cargo dies at manifest load, so no test binary is
//! ever built and no test gets the chance to skip or print anything. I measured
//! that in a container rather than assuming it: `cargo metadata` and
//! `cargo test` both exit 101 and only `cargo fmt` survives. A branch for a
//! state in which this file cannot run is dead code, so it is not here.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use yaml_rust2::{Yaml, YamlLoader};

/// The crate's `main` head at the time I wrote this, and what `COUNTERPART_REV`
/// should name. Written out rather than read from the pin file: reading it from
/// the file would make this test agree with any value the file happens to hold,
/// which is the shape of assertion that pins a constant to itself.
const PINNED_TODAY: &str = "88d34a0f7a085de92477fcebee0a98cfc7d5624a";

/// A sha that exists nowhere, for the shapes that must not resolve.
const OTHER_SHA: &str = "0123456789abcdef0123456789abcdef01234567";

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn script_path() -> PathBuf {
    repo_root().join("tools").join("counterpart.sh")
}

/// A scratch directory under `target/`, so nothing here writes outside the
/// build output and `cargo clean` takes it away.
fn scratch(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(name);
    if dir.exists() {
        fs::remove_dir_all(&dir).expect("I could not clear the scratch directory");
    }
    fs::create_dir_all(&dir).expect("I could not make a scratch directory");
    dir
}

struct Run {
    code: i32,
    stdout: String,
    stderr: String,
}

impl Run {
    fn said(&self) -> String {
        format!("{}{}", self.stdout, self.stderr)
    }

    fn expect_ok(&self, what: &str) -> &Self {
        assert_eq!(
            self.code,
            0,
            "{what} should have been allowed, and it exited {}:\n{}",
            self.code,
            self.said()
        );
        self
    }

    fn expect_refused(&self, what: &str) -> &Self {
        assert_eq!(
            self.code,
            1,
            "{what} should have been refused with exit 1, and it exited {}:\n{}",
            self.code,
            self.said()
        );
        self
    }

    fn mentions(&self, needle: &str) -> &Self {
        assert!(
            self.said().contains(needle),
            "I expected the message to mention {needle:?}, and it said:\n{}",
            self.said()
        );
        self
    }
}

/// Runs `tools/counterpart.sh`. `env` entries with `None` are removed from the
/// child's environment, so a matrix case cannot be quietly steered by whatever
/// the person running `cargo test` happens to have exported.
fn run(args: &[&str], env: &[(&str, Option<&str>)]) -> Run {
    let script = script_path();
    let mut cmd = Command::new(&script);
    cmd.args(args).current_dir(repo_root());
    for (key, value) in env {
        match value {
            Some(value) => cmd.env(key, value),
            None => cmd.env_remove(key),
        };
    }
    let out = cmd.output().unwrap_or_else(|err| {
        panic!(
            "I could not run {}: {err}. That script is the whole pin mechanism, \
             so until it exists nothing below can pass.",
            script.display()
        )
    });
    Run {
        code: out.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    }
}

/// Writes a pin file into a scratch directory and hands back its path.
fn pin_fixture(name: &str, body: &str) -> PathBuf {
    let path = scratch(name).join("COUNTERPART_REV");
    fs::write(&path, body).expect("I could not write the fixture");
    path
}

/// Fakes what the clone action leaves behind: a checkout whose `.git/HEAD` is a
/// detached sha. That is the exact shape `git checkout FETCH_HEAD` produces, so
/// this is the real input rather than a convenient one.
fn checkout_at(name: &str, sha: &str) -> PathBuf {
    let dir = scratch(name);
    let git = dir.join(".git");
    fs::create_dir_all(&git).expect("I could not make a .git directory");
    fs::write(git.join("HEAD"), format!("{sha}\n")).expect("I could not write HEAD");
    dir
}

fn read(path: &Path) -> String {
    fs::read_to_string(path)
        .unwrap_or_else(|err| panic!("I could not read {}: {err}", path.display()))
}

fn yaml_of(path: &Path) -> Yaml {
    let text = read(path);
    let mut docs = YamlLoader::load_from_str(&text)
        .unwrap_or_else(|err| panic!("{} is not parseable YAML: {err}", path.display()));
    assert_eq!(
        docs.len(),
        1,
        "{} should be one YAML document and it holds {}",
        path.display(),
        docs.len()
    );
    docs.remove(0)
}

/// Every `run:` block in a workflow or action, concatenated, so a forbidden
/// shape can be looked for once rather than per step.
fn all_run_blocks(steps: &[Yaml]) -> String {
    steps
        .iter()
        .filter_map(|step| step["run"].as_str())
        .collect::<Vec<_>>()
        .join("\n")
}

// ---------------------------------------------------------------------------
// The pin file and the parser that reads it
// ---------------------------------------------------------------------------

#[test]
fn the_pin_file_is_comments_then_exactly_one_sha() {
    let path = repo_root().join("COUNTERPART_REV");
    let text = read(&path);

    let payload: Vec<&str> = text
        .lines()
        .filter(|line| !line.trim().is_empty() && !line.trim_start().starts_with('#'))
        .collect();
    assert_eq!(
        payload.len(),
        1,
        "COUNTERPART_REV must hold exactly one non-comment line, and it holds {}: {payload:?}",
        payload.len()
    );
    let sha = payload[0];
    assert_eq!(
        sha,
        sha.trim(),
        "the payload line must be the sha and nothing else"
    );
    assert_eq!(
        sha.len(),
        40,
        "a commit sha is 40 characters and this is {}",
        sha.len()
    );
    assert!(
        sha.chars()
            .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c)),
        "the payload must be lowercase hex, and it is {sha:?}"
    );
    assert!(
        text.lines()
            .next()
            .is_some_and(|line| line.starts_with('#')),
        "the file should open with the comments that explain the rule"
    );

    let parsed = run(&["rev", path.to_str().unwrap()], &[]);
    parsed.expect_ok("reading the real pin file");
    assert_eq!(
        parsed.stdout.trim(),
        sha,
        "the script and this test must read the same line out of the same file"
    );
    assert_eq!(
        sha, PINNED_TODAY,
        "the pin should name the acadsharp-rs main commit this change was written against"
    );
}

#[test]
fn the_parser_reads_the_payload_line_not_a_sha_quoted_in_a_comment() {
    // The decoy is the whole point. `libviprs-tests` reads its pin with
    // `grep -Eom1 '[0-9a-f]{40}'`, which takes the first 40-hex run anywhere in
    // the file, comments included. A bump note that quotes the sha it replaced
    // is an ordinary thing to write and it silently becomes the pin.
    let fixture = pin_fixture(
        "decoy-comment",
        &format!(
            "# bumped from {OTHER_SHA}, which was the old head\n#\n{PINNED_TODAY}\n# a label\n"
        ),
    );

    let parsed = run(&["rev", fixture.to_str().unwrap()], &[]);
    parsed.expect_ok("a pin file with a sha quoted in a comment above the payload");
    assert_eq!(
        parsed.stdout.trim(),
        PINNED_TODAY,
        "the payload line is the pin; a sha inside a comment is prose"
    );
}

#[test]
fn the_org_parser_would_have_taken_the_comment_sha() {
    // The negative control for the test above. Without this, "my parser gets it
    // right" is a claim about a file that might not have been able to go wrong.
    let fixture = pin_fixture(
        "decoy-control",
        &format!("# bumped from {OTHER_SHA}, which was the old head\n{PINNED_TODAY}\n"),
    );
    let out = Command::new("grep")
        .args(["-Eom1", "[0-9a-f]{40}", fixture.to_str().unwrap()])
        .output()
        .expect("I could not run grep");
    let got = String::from_utf8_lossy(&out.stdout).trim().to_string();
    assert_eq!(
        got, OTHER_SHA,
        "the org's parser should take the comment sha on this fixture, otherwise this fixture \
         does not reproduce the defect and the test above proves nothing"
    );
}

#[test]
fn the_parser_refuses_every_shape_that_is_not_one_lowercase_sha() {
    let cases: Vec<(&str, String, &str)> = vec![
        ("empty", String::new(), "no payload line"),
        (
            "comments-only",
            "# nothing but prose\n".to_string(),
            "no payload line",
        ),
        (
            "uppercase",
            format!("{}\n", PINNED_TODAY.to_uppercase()),
            "lowercase hex",
        ),
        ("short", "88d34a0\n".to_string(), "40"),
        ("branch-name", "main\n".to_string(), "lowercase hex"),
        (
            "two-payloads",
            format!("{PINNED_TODAY}\n{OTHER_SHA}\n"),
            "exactly one",
        ),
        (
            "trailing-junk",
            format!("{PINNED_TODAY} # the head\n"),
            "the sha and nothing else",
        ),
    ];

    for (name, body, needle) in cases {
        let fixture = pin_fixture(&format!("refuse-{name}"), &body);
        run(&["rev", fixture.to_str().unwrap()], &[])
            .expect_refused(&format!("the {name} pin file"))
            .mentions(needle);
    }

    let missing = scratch("refuse-missing").join("COUNTERPART_REV");
    run(&["rev", missing.to_str().unwrap()], &[])
        .expect_refused("a pin file that is not there")
        .mentions("guess");
}

// ---------------------------------------------------------------------------
// The merged-only rule
// ---------------------------------------------------------------------------

#[test]
fn merged_only_accepts_identical_and_behind() {
    // `behind` is the one that matters. A pin naming an older commit that is
    // still an ancestor of `main` is merged, and it is what every pin becomes
    // the moment the next crate PR lands. Refusing it would make the rule
    // demand a pin bump for every unrelated crate commit.
    for status in ["identical", "behind"] {
        run(&["merged-only", status, PINNED_TODAY], &[])
            .expect_ok(&format!("a pin whose compare status is {status}"))
            .mentions(status);
    }
}

#[test]
fn merged_only_refuses_ahead_and_diverged() {
    for status in ["ahead", "diverged"] {
        run(&["merged-only", status, OTHER_SHA], &[])
            .expect_refused(&format!("a pin whose compare status is {status}"))
            .mentions("unmerged");
    }
}

#[test]
fn merged_only_refuses_a_status_it_does_not_recognise() {
    // An empty status is what a failed `gh api` call leaves behind, and a
    // renamed field is what a future API change leaves behind. Neither means
    // "merged", and reading silence as consent is the whole family of bug this
    // file exists for.
    for status in ["", "null", "Behind", "identical\nbehind", "unknown"] {
        run(&["merged-only", status, PINNED_TODAY], &[])
            .expect_refused(&format!("the compare status {status:?}"))
            .mentions("recognise");
    }
}

#[test]
fn merged_only_says_when_it_was_called_wrong() {
    // Exit 2 is "you wired this up wrong", separate from exit 1 "I refuse".
    // Collapsing them would let a typo in the workflow read as a policy
    // refusal, which is the wrong thing to go and fix.
    for args in [vec!["merged-only"], vec!["merged-only", "behind"]] {
        let got = run(&args, &[]);
        assert_eq!(
            got.code,
            2,
            "calling {args:?} is a wiring mistake and should exit 2, not {}:\n{}",
            got.code,
            got.said()
        );
    }
}

// ---------------------------------------------------------------------------
// Which sibling is beside us, and which one should be
// ---------------------------------------------------------------------------

#[test]
fn the_expected_sha_env_var_overrides_the_committed_pin() {
    let pin = repo_root().join("COUNTERPART_REV");

    let own = run(
        &["expect", pin.to_str().unwrap()],
        &[("VIPRS_COUNTERPART_EXPECTED_SHA", None)],
    );
    own.expect_ok("asking what to expect with no override");
    assert_eq!(own.stdout.trim(), PINNED_TODAY);

    let overridden = run(
        &["expect", pin.to_str().unwrap()],
        &[("VIPRS_COUNTERPART_EXPECTED_SHA", Some(OTHER_SHA))],
    );
    overridden.expect_ok("asking what to expect with the override set");
    assert_eq!(
        overridden.stdout.trim(),
        OTHER_SHA,
        "the crate's Suite job sets this to the crate PR head, and there the sibling is \
         deliberately not the commit this repo pins"
    );

    // A malformed override is refused rather than ignored. Ignoring it would
    // silently fall back to the committed pin, which is exactly the shape of
    // fallback this whole mechanism refuses to have.
    run(
        &["expect", pin.to_str().unwrap()],
        &[("VIPRS_COUNTERPART_EXPECTED_SHA", Some("main"))],
    )
    .expect_refused("a VIPRS_COUNTERPART_EXPECTED_SHA that is not a sha")
    .mentions("VIPRS_COUNTERPART_EXPECTED_SHA");
}

#[test]
fn verify_is_happy_with_a_sibling_at_the_pin() {
    let dir = checkout_at("sibling-right", PINNED_TODAY);
    run(
        &["verify", dir.to_str().unwrap()],
        &[
            ("VIPRS_REQUIRE_COUNTERPART", Some("1")),
            ("VIPRS_COUNTERPART_EXPECTED_SHA", None),
        ],
    )
    .expect_ok("a sibling sitting at exactly the pin")
    .mentions(PINNED_TODAY);
}

#[test]
fn verify_refuses_a_sibling_at_the_wrong_commit() {
    let dir = checkout_at("sibling-wrong", OTHER_SHA);
    // Wrong is wrong whether or not CI asked for the check. The env var decides
    // what happens when the answer is unknown, never what happens when the
    // answer is known and bad.
    for require in [Some("1"), None] {
        run(
            &["verify", dir.to_str().unwrap()],
            &[
                ("VIPRS_REQUIRE_COUNTERPART", require),
                ("VIPRS_COUNTERPART_EXPECTED_SHA", None),
            ],
        )
        .expect_refused("a sibling at a commit this repo does not pin")
        .mentions(OTHER_SHA);
    }
}

#[test]
fn the_expected_sha_env_var_decides_what_verify_accepts() {
    let dir = checkout_at("sibling-pr-head", OTHER_SHA);
    // This is the crate's Suite job in miniature: the sibling is the crate PR,
    // not the pin, and the job says so through the environment.
    run(
        &["verify", dir.to_str().unwrap()],
        &[
            ("VIPRS_REQUIRE_COUNTERPART", Some("1")),
            ("VIPRS_COUNTERPART_EXPECTED_SHA", Some(OTHER_SHA)),
        ],
    )
    .expect_ok("a sibling at the sha the environment names");
}

#[test]
fn verify_fails_rather_than_skips_when_it_cannot_tell() {
    // A checkout with no git metadata at all. CI never produces this (the clone
    // action leaves a real detached HEAD), so under VIPRS_REQUIRE_COUNTERPART
    // it means something went wrong upstream of here, and a check that cannot
    // tell is the same colour as one that passed.
    let dir = scratch("sibling-opaque");

    run(
        &["verify", dir.to_str().unwrap()],
        &[
            ("VIPRS_REQUIRE_COUNTERPART", Some("1")),
            ("VIPRS_COUNTERPART_EXPECTED_SHA", None),
        ],
    )
    .expect_refused("an unreadable sibling with the check demanded")
    .mentions("VIPRS_REQUIRE_COUNTERPART");

    let lenient = run(
        &["verify", dir.to_str().unwrap()],
        &[
            ("VIPRS_REQUIRE_COUNTERPART", None),
            ("VIPRS_COUNTERPART_EXPECTED_SHA", None),
        ],
    );
    lenient.expect_ok("an unreadable sibling on a developer's machine");
    // The printed reason has to say what went unchecked and how to make it
    // fatal, otherwise it is noise somebody scrolls past.
    lenient.mentions("VIPRS_REQUIRE_COUNTERPART");
    assert!(
        lenient.said().contains("did not check"),
        "the reason must say what it skipped, and it said:\n{}",
        lenient.said()
    );
}

#[test]
fn the_sibling_beside_this_checkout_is_the_one_we_expect() {
    // The test the crate's `Suite (acadsharp-rs-tests)` job actually drives. It
    // inherits the ambient environment on purpose: CI sets both variables, a
    // laptop sets neither.
    let sibling = repo_root().join("..").join("acadsharp-rs");
    let got = run(&["verify", sibling.to_str().unwrap()], &[]);
    println!("{}", got.said());
    got.expect_ok("the acadsharp-rs beside this checkout");
}

// ---------------------------------------------------------------------------
// The dependency, declared and resolved
// ---------------------------------------------------------------------------

#[test]
fn the_manifest_declares_the_path_dependency_the_crate_job_greps_for() {
    // The crate's Suite job refuses a suite commit whose Cargo.toml does not
    // match this exact regex, so a reformat that breaks their grep has to fail
    // here rather than over there, where the message would blame the pin.
    let manifest = read(&repo_root().join("Cargo.toml"));
    let matched = manifest.lines().any(|line| {
        let line = line.trim_start();
        line.starts_with("acadsharp-rs")
            && line.contains('=')
            && line.contains("path")
            && line.contains("\"../acadsharp-rs\"")
    });
    assert!(
        matched,
        "Cargo.toml must carry `acadsharp-rs = {{ path = \"../acadsharp-rs\" }}` on one line, \
         spelled the way libviprs/acadsharp-rs' Suite job greps for it"
    );
}

#[test]
fn cargo_metadata_resolves_acadsharp_rs_inside_the_sibling_checkout() {
    // Grepping the manifest proves the declaration, not the resolution. A
    // `[patch]`, a workspace member, a second source or the entry sitting under
    // `[dev-dependencies]` all satisfy the grep and change what actually gets
    // built. This asks cargo where the package came from.
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
    let out = Command::new(cargo)
        .args(["metadata", "--format-version", "1"])
        .arg("--manifest-path")
        .arg(repo_root().join("Cargo.toml"))
        .output()
        .expect("I could not run cargo metadata");
    assert!(
        out.status.success(),
        "cargo metadata failed:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let meta: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("cargo metadata did not give me JSON");

    let package = meta["packages"]
        .as_array()
        .expect("metadata has no packages array")
        .iter()
        .find(|pkg| pkg["name"].as_str() == Some("acadsharp-rs"))
        .unwrap_or_else(|| {
            panic!("cargo resolved no package called acadsharp-rs, so this suite tests nothing")
        });

    assert!(
        package["source"].is_null(),
        "acadsharp-rs must resolve from the sibling path, and cargo says it came from {}",
        package["source"]
    );

    let resolved = PathBuf::from(package["manifest_path"].as_str().expect("no manifest_path"));
    let resolved = fs::canonicalize(&resolved)
        .unwrap_or_else(|err| panic!("I could not canonicalize {}: {err}", resolved.display()));
    let wanted = fs::canonicalize(
        repo_root()
            .join("..")
            .join("acadsharp-rs")
            .join("Cargo.toml"),
    )
    .expect("I could not canonicalize the sibling manifest");
    assert_eq!(
        resolved, wanted,
        "acadsharp-rs has to resolve to the checkout beside this one, which is the one \
         COUNTERPART_REV pins and the one the crate's Suite job lays down"
    );
}

// ---------------------------------------------------------------------------
// The action and the workflow that uses it
// ---------------------------------------------------------------------------

#[test]
fn the_action_parses_and_has_no_way_to_fall_back_to_a_branch() {
    let path = repo_root()
        .join(".github")
        .join("actions")
        .join("clone-counterpart")
        .join("action.yml");
    let doc = yaml_of(&path);

    assert_eq!(
        doc["runs"]["using"].as_str(),
        Some("composite"),
        "the clone action must be a composite action"
    );
    let steps = doc["runs"]["steps"]
        .as_vec()
        .expect("the action has no steps");
    assert!(!steps.is_empty(), "the action has no steps");
    for step in steps {
        assert_eq!(
            step["shell"].as_str(),
            Some("bash"),
            "every step in this action runs bash; delegating to another action is how a \
             `ref:` and a default-branch fallback get back in"
        );
        assert!(
            step["uses"].is_badvalue(),
            "this action must not call another action, for the same reason"
        );
        assert!(
            step["continue-on-error"].is_badvalue(),
            "a clone that is allowed to fail is a clone that proves nothing"
        );
    }

    // The pin comes from the committed file. An input that names a revision
    // would let a caller pass one, which is the repository-variable failure
    // with extra steps.
    if let Some(inputs) = doc["inputs"].as_hash() {
        for key in inputs.keys() {
            let name = key.as_str().unwrap_or_default();
            assert!(
                !["ref", "rev", "branch", "sha", "revision"].contains(&name),
                "the action takes no revision input, and it declares {name:?}"
            );
        }
    }

    let body = read(&path);
    for forbidden in [
        "vars.",
        "origin/main",
        "origin main",
        "default_branch",
        "--branch",
        "github.ref",
        "|| true",
    ] {
        assert!(
            !body.contains(forbidden),
            "the action must not contain {forbidden:?}: that is a branch name, a repository \
             variable or a swallowed error, and all three are how the org built against the \
             wrong tree before"
        );
    }
    let runs = all_run_blocks(steps);
    assert!(
        runs.contains("counterpart.sh"),
        "the action should call tools/counterpart.sh rather than carry a second copy of the rule"
    );
    assert!(
        runs.contains("merged-only"),
        "the action has to apply the merged-only rule, otherwise the pin can name an unmerged commit"
    );
}

#[test]
fn the_script_fetches_exactly_one_sha_and_never_a_branch() {
    let path = script_path();
    let body = read(&path);

    let fetches: Vec<&str> = body
        .lines()
        .map(str::trim)
        .filter(|line| !line.starts_with('#') && line.contains("git ") && line.contains("fetch"))
        .collect();
    assert_eq!(
        fetches.len(),
        1,
        "there should be exactly one git fetch in the pin mechanism, and I found {}: {fetches:?}",
        fetches.len()
    );
    assert!(
        fetches[0].contains("origin \"$REV\""),
        "the fetch must name the validated sha and nothing else, and it reads: {}",
        fetches[0]
    );
    for forbidden in ["origin/", "refs/heads", "--tags", "main"] {
        assert!(
            !fetches[0].contains(forbidden),
            "the fetch must not mention {forbidden:?}, and it reads: {}",
            fetches[0]
        );
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(&path).expect("no script").permissions().mode();
        assert!(
            mode & 0o111 != 0,
            "tools/counterpart.sh has to be executable, and its mode is {mode:o}"
        );
    }
}

#[test]
fn every_ci_job_lays_the_counterpart_down_before_it_builds() {
    // Iterating the jobs rather than naming them is the point: a job added
    // later is covered by this the day it is added, and without the sibling it
    // could not compile anything anyway.
    let path = repo_root().join(".github").join("workflows").join("ci.yml");
    let doc = yaml_of(&path);

    let workflow_env = env_map(&doc["env"]);
    let jobs = doc["jobs"].as_hash().expect("ci.yml declares no jobs");
    assert!(jobs.len() >= 4, "ci.yml should still have its four jobs");

    for (name, job) in jobs {
        let name = name.as_str().unwrap_or_default();
        let steps = job["steps"]
            .as_vec()
            .unwrap_or_else(|| panic!("job {name} has no steps"));

        let checkout = steps
            .iter()
            .position(|step| {
                step["uses"]
                    .as_str()
                    .is_some_and(|u| u.starts_with("actions/checkout"))
            })
            .unwrap_or_else(|| panic!("job {name} never checks this repository out"));
        let clone = steps
            .iter()
            .position(|step| step["uses"].as_str() == Some("./.github/actions/clone-counterpart"))
            .unwrap_or_else(|| {
                panic!(
                    "job {name} never runs ./.github/actions/clone-counterpart, so it has no \
                     acadsharp-rs beside it and cannot even load this manifest"
                )
            });
        assert!(
            checkout < clone,
            "job {name} runs the clone action before it checks the repository out, and the \
             action reads COUNTERPART_REV out of the checkout"
        );

        let mut env = workflow_env.clone();
        env.extend(env_map(&job["env"]));
        assert_eq!(
            env.get("VIPRS_REQUIRE_COUNTERPART").map(String::as_str),
            Some("1"),
            "job {name} must set VIPRS_REQUIRE_COUNTERPART=1, so the pin check in this file \
             fails rather than prints when it cannot confirm the sibling"
        );
    }
}

fn env_map(node: &Yaml) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    if let Some(hash) = node.as_hash() {
        for (key, value) in hash {
            let key = key.as_str().unwrap_or_default().to_string();
            let value = value
                .as_str()
                .map(str::to_string)
                .or_else(|| value.as_i64().map(|n| n.to_string()))
                .unwrap_or_default();
            out.insert(key, value);
        }
    }
    out
}

#[test]
fn the_ci_header_no_longer_says_the_mechanism_is_missing() {
    // The header used to explain, at length, that there was deliberately no
    // counterpart mechanism and what a correct one would have to do. This
    // change is that mechanism, so a header still calling it absent would be a
    // comment contradicting the file it sits in, which is how the org ended up
    // with one wrong enumeration in eleven places.
    let body = read(&repo_root().join(".github").join("workflows").join("ci.yml"));
    for stale in [
        "deliberately NOT here yet",
        "this file stays honest until then",
        "The epic decides it",
    ] {
        assert!(
            !body.contains(stale),
            "ci.yml's header still says {stale:?}, and it is no longer true"
        );
    }
    assert!(
        body.contains("COUNTERPART_REV"),
        "ci.yml's header should name the pin it now uses"
    );
}

#[test]
fn the_readme_states_the_rule_it_used_to_call_an_open_question() {
    let body = read(&repo_root().join("README.md"));
    assert!(
        !body.contains("Open question this repo does not answer yet"),
        "the README still calls the counterpart mechanism an open question"
    );
    for needed in ["COUNTERPART_REV", "merged", "clone-counterpart"] {
        assert!(
            body.contains(needed),
            "the README should mention {needed:?} now that the rule is decided"
        );
    }
}
