//! Issue #18: `COUNTERPART_REV` has to name a commit acadsharp-rs's `main`
//! reaches, and `tools/counterpart.sh clone` has to refuse one it does not.
//!
//! acadsharp-rs's `main` was rewritten with byte-identical trees after the pin
//! was written. The pinned commit is still fetchable by sha, because GitHub
//! keeps unreferenced objects until it collects them, so `clone` succeeds and
//! checks out the right tree, and the only thing that would have refused it is
//! the compare step in the clone action, which needs Actions. The script's own
//! error text says a fetch by sha needs a reachable commit; that is not how
//! GitHub behaves, so the fetch is not the reachability check it looks like.
//!
//! Both tests need the network. Without it they fail and say so, unless
//! `VIPRS_OFFLINE=1`, which skips them out loud.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// The acadsharp-rs commit `COUNTERPART_REV` named when `main` was rewritten
/// under it. Same tree as `ae89e80` on today's `main`, reachable from no ref.
const ORPHANED: &str = "88d34a0f7a085de92477fcebee0a98cfc7d5624a";

fn crate_url() -> String {
    std::env::var("VIPRS_COUNTERPART_URL")
        .unwrap_or_else(|_| "https://github.com/libviprs/acadsharp-rs.git".to_string())
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn offline() -> bool {
    std::env::var("VIPRS_OFFLINE").as_deref() == Ok("1")
}

fn scratch(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create scratch dir");
    dir
}

/// The pin, read the way `counterpart.sh rev` reads it, by asking the script.
fn counterpart_rev() -> String {
    let out = Command::new(repo_root().join("tools/counterpart.sh"))
        .arg("rev")
        .output()
        .expect("run tools/counterpart.sh rev");
    assert!(out.status.success(), "counterpart.sh rev failed: {}", joined(&out));
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn joined(out: &Output) -> String {
    let mut s = String::from_utf8_lossy(&out.stdout).into_owned();
    s.push_str(&String::from_utf8_lossy(&out.stderr));
    s.trim().to_string()
}

fn git(dir: &Path, args: &[&str]) -> Output {
    Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .unwrap_or_else(|e| panic!("could not run git {args:?}: {e}"))
}

#[test]
fn issue_18_counterpart_rev_is_on_acadsharp_rs_main() {
    if offline() {
        eprintln!("issue_18: VIPRS_OFFLINE=1, so I did NOT check that COUNTERPART_REV is on acadsharp-rs's main");
        return;
    }
    let pin = counterpart_rev();
    let dir = scratch("issue_18_crate_main");
    let repo = dir.join("acadsharp-rs");
    let out = Command::new("git")
        .args([
            "clone",
            "--quiet",
            "--filter=blob:none",
            "--no-checkout",
            "--single-branch",
            "--branch",
            "main",
        ])
        .arg(crate_url())
        .arg(&repo)
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .expect("run git clone");
    assert!(
        out.status.success(),
        "could not clone {}'s main: {}\nThis check needs the network. Set VIPRS_OFFLINE=1 to skip it on purpose.",
        crate_url(),
        joined(&out)
    );
    let fetched = git(&repo, &["fetch", "--quiet", "origin", &pin]);
    assert!(
        fetched.status.success(),
        "COUNTERPART_REV {pin} cannot be fetched from {} at all: {}",
        crate_url(),
        joined(&fetched)
    );
    if git(&repo, &["merge-base", "--is-ancestor", &pin, "refs/remotes/origin/main"])
        .status
        .success()
    {
        return;
    }
    let main = joined(&git(&repo, &["rev-parse", "refs/remotes/origin/main"]));
    let tree = joined(&git(&repo, &["rev-parse", &format!("{pin}^{{tree}}")]));
    let log = joined(&git(&repo, &["log", "--format=%H %T", "refs/remotes/origin/main"]));
    let twin = log
        .lines()
        .find_map(|l| {
            let (c, t) = l.split_once(' ')?;
            (t == tree).then(|| c.to_string())
        })
        .unwrap_or_else(|| "no commit on main".to_string());
    panic!(
        "COUNTERPART_REV {pin} is not on acadsharp-rs's main (main is at {main}). \
         GitHub still serves it by sha, so `counterpart.sh clone` works, but no branch reaches it. \
         The commit on main with the same tree ({tree}) is: {twin}."
    );
}

#[test]
fn issue_18_clone_refuses_a_pin_main_does_not_reach() {
    if offline() {
        eprintln!("issue_18: VIPRS_OFFLINE=1, so I did NOT check that clone refuses an unreachable pin");
        return;
    }
    // A pin file holding the commit the rewrite orphaned, so this keeps
    // testing the mechanism after COUNTERPART_REV itself moves.
    let dir = scratch("issue_18_clone");
    let pin_file = dir.join("COUNTERPART_REV");
    std::fs::write(
        &pin_file,
        format!("# the acadsharp-rs commit the rewrite of main orphaned\n{ORPHANED}\n"),
    )
    .unwrap();
    let dest = dir.join("acadsharp-rs");
    let out = Command::new(repo_root().join("tools/counterpart.sh"))
        .arg("clone")
        .arg(&dest)
        .arg(&pin_file)
        .env("COUNTERPART_REPO", "libviprs/acadsharp-rs")
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .expect("run tools/counterpart.sh clone");
    let all = joined(&out);
    assert_eq!(
        out.status.code(),
        Some(1),
        "counterpart.sh clone laid down {ORPHANED}, which acadsharp-rs's main does not reach \
         (expected exit 1, a refusal). A fetch by sha succeeds for an unreferenced commit until \
         GitHub collects it, so the fetch is not a reachability check. It said:\n{all}"
    );
    assert!(
        all.contains("::error file=COUNTERPART_REV::"),
        "the refusal does not point at COUNTERPART_REV:\n{all}"
    );
}
