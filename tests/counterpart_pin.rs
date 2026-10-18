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

/// A throwaway git repository shaped like acadsharp-rs or acadrust after a
/// rewrite of `main`: history rewritten from the root with identical trees, the
/// old tip still fetchable by sha and reached by no branch.
struct Fixture {
    /// `file://` plus the directory that holds `<repo>.git`.
    url_base: String,
    repo: String,
    /// On main, below the tip.
    on_main: String,
    main_tip: String,
    /// Same tree as `twin_on_main`, no common ancestor with main, no ref.
    orphan: String,
    twin_on_main: String,
    /// Shares history with main, is on a branch, is not on main.
    unmerged: String,
}

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args([
            "-c",
            "user.name=fixture",
            "-c",
            "user.email=fixture@example.invalid",
        ])
        .args(args)
        .current_dir(dir)
        .env("GIT_AUTHOR_DATE", "2026-01-01T00:00:00Z")
        .env("GIT_COMMITTER_DATE", "2026-01-01T00:00:00Z")
        .output()
        .unwrap_or_else(|e| panic!("I could not run git {args:?}: {e}"));
    assert!(
        out.status.success(),
        "git {args:?} failed in {}:\n{}",
        dir.display(),
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn commit_file(dir: &Path, file: &str, body: &str) -> String {
    fs::write(dir.join(file), body).unwrap();
    git(dir, &["add", file]);
    git(dir, &["commit", "-q", "-m", &format!("add {file}")]);
    git(dir, &["rev-parse", "HEAD"])
}

fn fixture_repo(tag: &str) -> Fixture {
    let base = scratch(&format!("fx-{tag}"));
    let repo = "fx".to_string();
    let dir = base.join(format!("{repo}.git"));
    fs::create_dir_all(&dir).unwrap();
    git(&dir, &["init", "-q", "-b", "main"]);
    // A fetch by sha of something no ref reaches, which is what GitHub serves
    // until it collects the object, and a blob-less fetch.
    git(&dir, &["config", "uploadpack.allowAnySHA1InWant", "true"]);
    git(&dir, &["config", "uploadpack.allowFilter", "true"]);

    commit_file(&dir, "a", "one\n");
    commit_file(&dir, "b", "two\n");
    let old_tip = commit_file(&dir, "c", "three\n");

    // The rewrite: a new root commit with the old tip's tree, and main moves.
    let tree = git(&dir, &["rev-parse", &format!("{old_tip}^{{tree}}")]);
    let twin = git(
        &dir,
        &["commit-tree", &tree, "-m", "rewritten from the root"],
    );
    git(&dir, &["update-ref", "refs/heads/main", &twin]);
    git(&dir, &["reset", "-q", "--hard", "main"]);
    let main_tip = commit_file(&dir, "d", "four\n");

    git(&dir, &["checkout", "-q", "-b", "pr"]);
    let unmerged = commit_file(&dir, "e", "five\n");
    git(&dir, &["checkout", "-q", "main"]);

    Fixture {
        url_base: format!("file://{}", base.display()),
        repo,
        on_main: twin.clone(),
        main_tip,
        orphan: old_tip,
        twin_on_main: twin,
        unmerged,
    }
}

/// One pin's whole test matrix. The two pins are `COUNTERPART_REV` (the crate
/// under test) and `ACADRUST_REV` (the fork the second emitter is written
/// against); both go through the same script, so both run the same test bodies.
macro_rules! pin_matrix {
    (
        $module:ident,
        name: $name:literal,
        file: $file:literal,
        today: $today:literal,
        require: $require:literal,
        expected: $expected:literal $(,)?
    ) => {
        mod $module {
            use super::*;

            /// The counterpart's directory name, which is `COUNTERPART_NAME`.
            const NAME: &str = $name;
            const PIN_FILE: &str = $file;
            const TODAY: &str = $today;
            const REQUIRE: &str = $require;
            const EXPECTED: &str = $expected;

            fn run(args: &[&str], env: &[(&str, Option<&str>)]) -> Run {
                let mut env = env.to_vec();
                env.push(("COUNTERPART_NAME", Some(NAME)));
                super::run(args, &env)
            }

            /// Per-pin scratch, so the two matrices can run side by side.
            fn scratch(name: &str) -> PathBuf {
                super::scratch(&format!("{NAME}-{name}"))
            }

            fn pin_fixture(name: &str, body: &str) -> PathBuf {
                let path = scratch(name).join(PIN_FILE);
                fs::write(&path, body).expect("I could not write the fixture");
                path
            }

            fn checkout_at(name: &str, sha: &str) -> PathBuf {
                let dir = scratch(name);
                let git = dir.join(".git");
                fs::create_dir_all(&git).expect("I could not make a .git directory");
                fs::write(git.join("HEAD"), format!("{sha}\n")).expect("I could not write HEAD");
                dir
            }

            fn head_of(dest: &Path) -> String {
                fs::read_to_string(dest.join(".git/HEAD"))
                    .map(|s| s.trim().to_string())
                    .unwrap_or_default()
            }

            fn fixture_repo(tag: &str) -> Fixture {
                super::fixture_repo(&format!("{NAME}-{tag}"))
            }
    // ---------------------------------------------------------------------------
    // The pin file and the parser that reads it
    // ---------------------------------------------------------------------------

    #[test]
    fn the_pin_file_is_comments_then_exactly_one_sha() {
        let path = repo_root().join(PIN_FILE);
        let text = read(&path);

        let payload: Vec<&str> = text
            .lines()
            .filter(|line| !line.trim().is_empty() && !line.trim_start().starts_with('#'))
            .collect();
        assert_eq!(
            payload.len(),
            1,
            "{PIN_FILE} must hold exactly one non-comment line, and it holds {}: {payload:?}",
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
            sha, TODAY,
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
                "# bumped from {OTHER_SHA}, which was the old head\n#\n{TODAY}\n# a label\n"
            ),
        );

        let parsed = run(&["rev", fixture.to_str().unwrap()], &[]);
        parsed.expect_ok("a pin file with a sha quoted in a comment above the payload");
        assert_eq!(
            parsed.stdout.trim(),
            TODAY,
            "the payload line is the pin; a sha inside a comment is prose"
        );
    }

    #[test]
    fn the_org_parser_would_have_taken_the_comment_sha() {
        // The negative control for the test above. Without this, "my parser gets it
        // right" is a claim about a file that might not have been able to go wrong.
        let fixture = pin_fixture(
            "decoy-control",
            &format!("# bumped from {OTHER_SHA}, which was the old head\n{TODAY}\n"),
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
                format!("{}\n", TODAY.to_uppercase()),
                "lowercase hex",
            ),
            ("short", "88d34a0\n".to_string(), "40"),
            ("branch-name", "main\n".to_string(), "lowercase hex"),
            (
                "two-payloads",
                format!("{TODAY}\n{OTHER_SHA}\n"),
                "exactly one",
            ),
            (
                "trailing-junk",
                format!("{TODAY} # the head\n"),
                "the sha and nothing else",
            ),
        ];

        for (name, body, needle) in cases {
            let fixture = pin_fixture(&format!("refuse-{name}"), &body);
            run(&["rev", fixture.to_str().unwrap()], &[])
                .expect_refused(&format!("the {name} pin file"))
                .mentions(needle);
        }

        let missing = scratch("refuse-missing").join(PIN_FILE);
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
            run(&["merged-only", status, TODAY], &[])
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
            run(&["merged-only", status, TODAY], &[])
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
        let pin = repo_root().join(PIN_FILE);

        let own = run(
            &["expect", pin.to_str().unwrap()],
            &[(EXPECTED, None)],
        );
        own.expect_ok("asking what to expect with no override");
        assert_eq!(own.stdout.trim(), TODAY);

        let overridden = run(
            &["expect", pin.to_str().unwrap()],
            &[(EXPECTED, Some(OTHER_SHA))],
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
            &[(EXPECTED, Some("main"))],
        )
        .expect_refused("an expected-sha override that is not a sha")
        .mentions(EXPECTED);
    }

    #[test]
    fn verify_is_happy_with_a_sibling_at_the_pin() {
        let dir = checkout_at("sibling-right", TODAY);
        run(
            &["verify", dir.to_str().unwrap()],
            &[
                (REQUIRE, Some("1")),
                (EXPECTED, None),
            ],
        )
        .expect_ok("a sibling sitting at exactly the pin")
        .mentions(TODAY);
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
                    (REQUIRE, require),
                    (EXPECTED, None),
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
                (REQUIRE, Some("1")),
                (EXPECTED, Some(OTHER_SHA)),
            ],
        )
        .expect_ok("a sibling at the sha the environment names");
    }

    #[test]
    fn verify_fails_rather_than_skips_when_it_cannot_tell() {
        // Two ways to be unreadable, and the second one CI found for me on this
        // branch's first run. `CARGO_TARGET_TMPDIR` sits inside this repository on
        // a runner, and `git rev-parse HEAD` answers from anywhere inside a
        // checkout, so the empty directory below was told this suite's own branch
        // head and the pin check reported the sibling as being at a commit of the
        // wrong project. A directory that merely sits inside a repository is not a
        // checkout of it, and the script now says so.
        let bare = scratch("sibling-opaque");

        let outer = scratch("sibling-nested");
        let status = Command::new("git")
            .args(["init", "-q"])
            .arg(&outer)
            .status()
            .expect("I could not run git init");
        assert!(
            status.success(),
            "git init failed, and this case needs a real repository"
        );
        let nested = outer.join("sibling");
        fs::create_dir_all(&nested).expect("I could not make the nested directory");

        for dir in [bare, nested] {
            let shown = dir.display().to_string();
            run(
                &["verify", &shown],
                &[
                    (REQUIRE, Some("1")),
                    (EXPECTED, None),
                ],
            )
            .expect_refused("an unreadable sibling with the check demanded")
            .mentions(REQUIRE);

            let lenient = run(
                &["verify", &shown],
                &[
                    (REQUIRE, None),
                    (EXPECTED, None),
                ],
            );
            lenient.expect_ok("an unreadable sibling on a developer's machine");
            // The printed reason has to say what went unchecked and how to make it
            // fatal, otherwise it is noise somebody scrolls past.
            lenient.mentions(REQUIRE);
            assert!(
                lenient.said().contains("did not check"),
                "the reason must say what it skipped, and it said:\n{}",
                lenient.said()
            );
        }
    }

    #[test]
    fn the_sibling_beside_this_checkout_is_the_one_we_expect() {
        // The test the crate's `Suite (acadsharp-rs-tests)` job actually drives. It
        // inherits the ambient environment on purpose: CI sets both variables, a
        // laptop sets neither.
        let sibling = repo_root().join("..").join(NAME);
        let got = run(&["verify", sibling.to_str().unwrap()], &[]);
        println!("{}", got.said());
        got.expect_ok("the counterpart beside this checkout");
    }


    // ---------------------------------------------------------------------
    // Reachability, offline: a fixture repository whose main was rewritten
    // ---------------------------------------------------------------------

    #[test]
    fn verify_names_the_variable_when_the_sibling_is_missing_and_the_check_is_demanded() {
        // `ACADRUST_REQUIRE=1` (or the other pin's variable) with nothing beside
        // the suite is a failure that names the variable, not a skip.
        let gone = scratch("sibling-gone").join("not-there");
        run(
            &["verify", gone.to_str().unwrap()],
            &[(REQUIRE, Some("1")), (EXPECTED, None)],
        )
        .expect_refused("a missing sibling with the check demanded")
        .mentions(REQUIRE);
    }

    fn clone_from(fx: &Fixture, pin: &str, dest_name: &str) -> (Run, PathBuf) {
        let dir = scratch(dest_name);
        let pin_file = dir.join(PIN_FILE);
        fs::write(&pin_file, format!("# a fixture pin\n{pin}\n")).unwrap();
        let dest = dir.join("sibling");
        let got = run(
            &["clone", dest.to_str().unwrap(), pin_file.to_str().unwrap()],
            &[
                ("COUNTERPART_URL_BASE", Some(&fx.url_base)),
                ("COUNTERPART_REPO", Some(&fx.repo)),
                ("GIT_TERMINAL_PROMPT", Some("0")),
            ],
        );
        (got, dest)
    }

    fn nothing_checked_out(dest: &Path) -> bool {
        match fs::read_dir(dest) {
            Ok(entries) => entries
                .filter_map(Result::ok)
                .all(|e| e.file_name() == ".git"),
            Err(_) => true,
        }
    }

    #[test]
    fn clone_accepts_a_pin_that_main_reaches_even_when_it_is_not_the_tip() {
        let fx = fixture_repo(&format!("{NAME}-reach"));
        for (label, sha) in [("tip", &fx.main_tip), ("below the tip", &fx.on_main)] {
            let (got, dest) = clone_from(&fx, sha, &format!("clone-ok-{}", label.replace(' ', "-")));
            got.expect_ok(&format!("a pin at {label} of main"));
            assert_eq!(
                head_of(&dest),
                *sha,
                "clone said yes for the pin at {label}, but the checkout is not at it"
            );
        }
    }

    #[test]
    fn clone_refuses_a_pin_on_an_unrelated_history_and_leaves_nothing_checked_out() {
        // The rewrite case: the commit has the same tree as one on main, is
        // still fetchable by sha, and shares no ancestor with main. The compare
        // API answers 404 for this, and a check that keys on `diverged` alone
        // would let it through.
        let fx = fixture_repo(&format!("{NAME}-unrelated"));
        let (got, dest) = clone_from(&fx, &fx.orphan, "clone-orphan");
        got.expect_refused("a pin on a history main does not share")
            .mentions(&format!("::error file={PIN_FILE}::"))
            .mentions(&format!("{} is not on", fx.orphan))
            .mentions(&fx.twin_on_main);
        assert!(
            nothing_checked_out(&dest),
            "a refused clone must leave no working tree behind"
        );
        assert_ne!(head_of(&dest), fx.orphan);
    }

    #[test]
    fn clone_refuses_a_pin_on_an_unmerged_branch() {
        // Shares history with main and is not on it: the middle arm. For this
        // pin it is a refusal on every ref, because it never names an unmerged
        // commit (a paired change makes the suite PR wait instead).
        let fx = fixture_repo(&format!("{NAME}-unmerged"));
        let (got, dest) = clone_from(&fx, &fx.unmerged, "clone-unmerged");
        got.expect_refused("a pin on a branch main has not merged")
            .mentions(&format!("::error file={PIN_FILE}::"))
            .mentions(&format!("{} is not on", fx.unmerged));
        assert!(nothing_checked_out(&dest));
    }
        }
    };
}

pin_matrix!(
    acadsharp_rs,
    name: "acadsharp-rs",
    file: "COUNTERPART_REV",
    today: "f23f37bd453e02e660ef655faa381c45509d8ac1",
    require: "VIPRS_REQUIRE_COUNTERPART",
    expected: "VIPRS_COUNTERPART_EXPECTED_SHA",
);

pin_matrix!(
    acadrust,
    name: "acadrust",
    file: "ACADRUST_REV",
    today: "49786e64ad9dcac02f2d14a1d1d3e639ca13880c",
    require: "ACADRUST_REQUIRE",
    expected: "ACADRUST_EXPECTED_SHA",
);

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

    // `packages` lists everything in the graph, so an entry there survives the
    // dependency being optional-and-off or sitting under `[dev-dependencies]`,
    // and both of those still satisfy the crate job's grep while changing what
    // an ordinary build links. So ask the resolver what this package actually
    // depends on, as a normal dependency.
    let root = meta["resolve"]["root"]
        .as_str()
        .expect("metadata has no resolve root");
    let node = meta["resolve"]["nodes"]
        .as_array()
        .expect("metadata has no resolve nodes")
        .iter()
        .find(|node| node["id"].as_str() == Some(root))
        .expect("the resolve graph has no node for this package");
    let wired = node["deps"]
        .as_array()
        .expect("the root node has no deps")
        .iter()
        .find(|dep| dep["name"].as_str() == Some("acadsharp_rs"))
        .unwrap_or_else(|| {
            panic!("cargo resolved acadsharp-rs but this package does not depend on it")
        });
    assert!(
        wired["dep_kinds"]
            .as_array()
            .expect("no dep_kinds")
            .iter()
            .any(|kind| kind["kind"].is_null()),
        "acadsharp-rs has to be a normal dependency, and cargo says it is {}",
        wired["dep_kinds"]
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

    // One action serves both pins, through an input that names a pin FILE.
    assert_eq!(
        doc["inputs"]["pin"]["default"].as_str(),
        Some("COUNTERPART_REV"),
        "the action's `pin` input has to default to COUNTERPART_REV, so existing callers keep \
         meaning the crate"
    );
    let mut actions: Vec<_> = fs::read_dir(repo_root().join(".github").join("actions"))
        .expect("no .github/actions")
        .filter_map(Result::ok)
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    actions.sort();
    // `fetch-native-artifact` is the native lane's own action (a different
    // thing: it fetches an archive by hash and clones nothing). What this guards
    // is a second *clone* action, so the list is the clone action plus that one.
    assert_eq!(
        actions,
        ["clone-counterpart", "fetch-native-artifact"],
        "there is one clone action, not one per pin: a second copy is a second rule"
    );

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
fn the_script_checks_out_only_the_pinned_sha_and_fetches_main_only_to_ask() {
    let path = script_path();
    let body = read(&path);

    let fetches: Vec<&str> = body
        .lines()
        .map(str::trim)
        .filter(|line| !line.starts_with('#') && line.contains("git ") && line.contains("fetch"))
        .collect();
    // Two, and only these two (#18): the pin, fetched as a sha, and `main`,
    // fetched blob-less so the pin can be asked whether `main` reaches it. The
    // second is not a way to pick the commit: nothing is ever checked out from
    // it, which the `checkout` assertion below holds.
    assert_eq!(
        fetches.len(),
        2,
        "there should be exactly two git fetches in the pin mechanism (the sha, then main for \
         the ancestry check), and I found {}: {fetches:?}",
        fetches.len()
    );
    assert!(
        fetches[0].contains("origin \"$REV\""),
        "the first fetch must name the validated sha and nothing else, and it reads: {}",
        fetches[0]
    );
    for forbidden in ["origin/", "refs/heads", "--tags", "main"] {
        assert!(
            !fetches[0].contains(forbidden),
            "the pin fetch must not mention {forbidden:?}, and it reads: {}",
            fetches[0]
        );
    }
    assert!(
        fetches[1].contains("refs/heads/main:refs/remotes/origin/main")
            && !fetches[1].contains("--tags"),
        "the second fetch has to be main into a remote-tracking ref and nothing else, and it \
         reads: {}",
        fetches[1]
    );
    let checkouts: Vec<&str> = body
        .lines()
        .map(str::trim)
        .filter(|line| !line.starts_with('#') && line.contains("checkout -q"))
        .collect();
    assert_eq!(
        checkouts.len(),
        1,
        "exactly one checkout, and it must be of the pin: {checkouts:?}"
    );
    assert!(
        checkouts[0].ends_with("\"$REV\""),
        "the checkout must name $REV, not FETCH_HEAD (which the main fetch overwrites): {}",
        checkouts[0]
    );

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

    // Jobs that never run cargo, so have no use for either sibling: the matrix
    // planner (it prints JSON from NATIVE_ARTIFACT.toml) and the comparison of the
    // cells' output files (it diffs bytes). The list is explicit rather than
    // inferred, so a new job is covered the day it is added unless somebody
    // writes its name here and defends it, and `tests/native_artifact.rs` checks
    // that neither of these two has grown a cargo step.
    const NO_CARGO_JOBS: [&str; 2] = ["native-plan", "native-compare"];

    for (name, job) in jobs {
        let name = name.as_str().unwrap_or_default();
        if NO_CARGO_JOBS.contains(&name) {
            continue;
        }
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
        let clones: Vec<(usize, &Yaml)> = steps
            .iter()
            .enumerate()
            .filter(|(_, step)| {
                step["uses"].as_str() == Some("./.github/actions/clone-counterpart")
            })
            .collect();
        let pin_of = |step: &Yaml| {
            step["with"]["pin"]
                .as_str()
                .unwrap_or("COUNTERPART_REV")
                .to_string()
        };
        let pins: Vec<String> = clones.iter().map(|(_, step)| pin_of(step)).collect();
        assert_eq!(
            {
                let mut sorted = pins.clone();
                sorted.sort();
                sorted
            },
            ["ACADRUST_REV", "COUNTERPART_REV"],
            "job {name} has to lay down both siblings, once each, through the one action, and its \
             clone steps pin {pins:?}: cargo reads every path dependency's manifest, so without \
             ../acadrust beside it the job cannot even load this manifest"
        );
        for (position, step) in &clones {
            assert!(
                checkout < *position,
                "job {name} runs the clone action for {} before it checks the repository out, and \
                 the action reads the pin out of the checkout",
                pin_of(step)
            );
        }

        let mut env = workflow_env.clone();
        env.extend(env_map(&job["env"]));
        assert_eq!(
            env.get("ACADRUST_REQUIRE").map(String::as_str),
            Some("1"),
            "job {name} must set ACADRUST_REQUIRE=1, so a job that says it needs the fork and \
             does not get it fails naming the variable"
        );
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

// ---------------------------------------------------------------------------
// The second sibling: acadrust, declared, optional, and resolved
// ---------------------------------------------------------------------------

fn cargo_metadata(extra: &[&str]) -> serde_json::Value {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
    let out = Command::new(cargo)
        .args(["metadata", "--format-version", "1"])
        .args(extra)
        .arg("--manifest-path")
        .arg(repo_root().join("Cargo.toml"))
        .output()
        .expect("I could not run cargo metadata");
    assert!(
        out.status.success(),
        "cargo metadata failed (with the optional acadrust dependency it needs ../acadrust on \
         disk, exactly as it needs ../acadsharp-rs):\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).expect("cargo metadata did not give me JSON")
}

#[test]
fn the_manifest_declares_acadrust_as_an_optional_path_dependency_behind_a_feature() {
    let manifest = read(&repo_root().join("Cargo.toml"));
    let line = manifest
        .lines()
        .map(str::trim_start)
        .find(|l| (l.starts_with("acadrust ") || l.starts_with("acadrust=")) && l.contains("path"))
        .unwrap_or_else(|| panic!("Cargo.toml declares no acadrust dependency"));
    assert!(
        line.contains("path = \"../acadrust\"") && line.contains("optional = true"),
        "acadrust must be `{{ path = \"../acadrust\", optional = true }}` on one line, and it is: {line}"
    );
    assert!(
        manifest
            .lines()
            .any(|l| l.trim() == "acadrust = [\"dep:acadrust\"]"),
        "the `acadrust` feature has to enable exactly the optional dependency"
    );
    // The default build must not pull it in.
    assert!(
        !manifest
            .lines()
            .any(|l| l.trim_start().starts_with("default")),
        "this crate has no default features; acadrust must stay off unless asked for"
    );
}

#[test]
fn cargo_resolves_acadrust_from_the_sibling_and_a_default_build_does_not_use_it() {
    // Resolved with every feature on, because an optional dependency that is off
    // is not in the resolve graph at all, which is the second half of this test.
    let all = cargo_metadata(&["--all-features"]);
    let packages = all["packages"].as_array().expect("no packages");

    let acadrust = packages
        .iter()
        .find(|p| p["name"].as_str() == Some("acadrust"))
        .expect("cargo resolved no package called acadrust");
    assert!(
        acadrust["source"].is_null(),
        "acadrust has to come from the sibling path, and cargo says {}",
        acadrust["source"]
    );
    let resolved = fs::canonicalize(acadrust["manifest_path"].as_str().unwrap()).unwrap();
    let wanted =
        fs::canonicalize(repo_root().join("..").join("acadrust").join("Cargo.toml")).unwrap();
    assert_eq!(resolved, wanted, "acadrust must resolve to ../acadrust");

    let me = packages
        .iter()
        .find(|p| p["name"].as_str() == Some("acadsharp-rs-tests"))
        .unwrap();
    let declared = me["dependencies"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["name"].as_str() == Some("acadrust"))
        .expect("the suite does not declare acadrust");
    assert_eq!(declared["optional"], true, "acadrust must be optional");

    // With no feature on, the resolver does not wire it in: nothing from it is
    // compiled by an ordinary `cargo test`.
    let meta = cargo_metadata(&[]);
    let root = meta["resolve"]["root"].as_str().unwrap();
    let node = meta["resolve"]["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["id"].as_str() == Some(root))
        .unwrap();
    let wired: Vec<_> = node["deps"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|d| d["name"].as_str())
        .collect();
    assert!(
        !wired.contains(&"acadrust"),
        "a default build wires acadrust in: {wired:?}"
    );
    assert!(wired.contains(&"acadsharp_rs"));
}

#[test]
fn the_two_pins_are_two_files_and_neither_is_the_others() {
    let a = run(&["rev"], &[("COUNTERPART_NAME", Some("acadsharp-rs"))]);
    let b = run(&["rev"], &[("COUNTERPART_NAME", Some("acadrust"))]);
    a.expect_ok("the crate pin");
    b.expect_ok("the fork pin");
    assert_ne!(a.stdout, b.stdout, "two repositories cannot share a sha");
}

#[test]
fn the_script_refuses_a_pin_name_it_does_not_know() {
    let got = run(
        &["rev"],
        &[("COUNTERPART_NAME", Some("somebody-elses-repo"))],
    );
    assert_eq!(got.code, 2, "a wiring mistake exits 2:\n{}", got.said());
    got.mentions("COUNTERPART_NAME");
}

#[test]
fn the_suite_lock_is_committed_and_every_cargo_step_uses_it() {
    let ignore = read(&repo_root().join(".gitignore"));
    assert!(
        !ignore.lines().any(|l| l.trim() == "Cargo.lock"),
        ".gitignore still ignores Cargo.lock. The emitters are built by this suite, so this lock \
         decides what a parity run resolves."
    );
    assert!(
        repo_root().join("Cargo.lock").is_file(),
        "Cargo.lock has to exist and be committed"
    );

    let ci = read(&repo_root().join(".github").join("workflows").join("ci.yml"));
    let mut cargo_steps = 0;
    for line in ci.lines() {
        let code = line.split('#').next().unwrap_or("").trim();
        let Some(command) = code.strip_prefix("- run: cargo ") else {
            continue;
        };
        if command.starts_with("fmt") {
            continue;
        }
        cargo_steps += 1;
        assert!(
            command.contains("--locked"),
            "`cargo {command}` runs without --locked, so it could resolve something the lock does not say"
        );
    }
    assert!(
        cargo_steps >= 5,
        "I found only {cargo_steps} cargo steps in ci.yml"
    );
}

#[test]
fn the_readme_says_how_the_second_pin_lands_and_what_a_laptop_needs() {
    let body = read(&repo_root().join("README.md"));
    for needed in ["ACADRUST_REV", "../acadrust", "pre-push hook", "upstream"] {
        assert!(
            body.contains(needed),
            "the README should mention {needed:?}: how a paired acadrust and suite change lands, \
             the upstream-sync case, and that a laptop needs the fork beside the suite under the name the hook keys its slot on"
        );
    }
}

#[test]
fn clone_with_the_committed_acadrust_pin_succeeds() {
    // Live: the fork's `main` has to reach the committed ACADRUST_REV. It is
    // also the positive control for the reachability check on the second pin,
    // since a `clone` that refused everything would fail it. Strict by default;
    // `VIPRS_OFFLINE=1` skips it out loud.
    if std::env::var("VIPRS_OFFLINE").as_deref() == Ok("1") {
        eprintln!(
            "VIPRS_OFFLINE=1, so I did NOT check that clone accepts the committed ACADRUST_REV"
        );
        return;
    }
    let dest = scratch("acadrust-live-clone").join("acadrust");
    let got = run(
        &["clone", dest.to_str().unwrap()],
        &[
            ("COUNTERPART_NAME", Some("acadrust")),
            ("GIT_TERMINAL_PROMPT", Some("0")),
        ],
    );
    got.expect_ok("the committed ACADRUST_REV against libviprs/acadrust");
    let pin = run(&["rev"], &[("COUNTERPART_NAME", Some("acadrust"))]);
    assert_eq!(
        fs::read_to_string(dest.join(".git/HEAD")).unwrap().trim(),
        pin.stdout.trim()
    );
    let config = fs::read_to_string(dest.join(".git/config")).unwrap();
    assert!(
        config.contains("libviprs/acadrust.git"),
        "the clone has to come from the fork, and its remote says:\n{config}"
    );
}
