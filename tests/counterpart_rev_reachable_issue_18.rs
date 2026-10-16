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
//! Both tests go through `clone`, so they exercise the rule the action and a
//! laptop both run rather than a second copy of it in Rust. One is the live
//! check that the committed pin is on `main`, and it doubles as the positive
//! control: a `clone` that refused everything would fail it. The other hands
//! `clone` a fixture pin holding the orphaned commit and expects a refusal
//! that leaves nothing checked out.
//!
//! Both need the network. Without it they fail and say so, unless
//! `VIPRS_OFFLINE=1`, which skips them out loud.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// The acadsharp-rs commit `COUNTERPART_REV` named when `main` was rewritten
/// under it. Same tree as `ae89e80` on today's `main`, reachable from no ref.
const ORPHANED: &str = "88d34a0f7a085de92477fcebee0a98cfc7d5624a";

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
    assert!(
        out.status.success(),
        "counterpart.sh rev failed: {}",
        joined(&out)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn joined(out: &Output) -> String {
    let mut s = String::from_utf8_lossy(&out.stdout).into_owned();
    s.push_str(&String::from_utf8_lossy(&out.stderr));
    s.trim().to_string()
}

/// Runs `counterpart.sh clone <dest> [<pin-file>]` the way the clone action
/// does, against the real acadsharp-rs. No pin file means the committed one.
fn clone(dest: &Path, pin_file: Option<&Path>) -> Output {
    let mut cmd = Command::new(repo_root().join("tools/counterpart.sh"));
    cmd.arg("clone").arg(dest);
    if let Some(pin_file) = pin_file {
        cmd.arg(pin_file);
    }
    cmd.env("COUNTERPART_REPO", "libviprs/acadsharp-rs")
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .expect("run tools/counterpart.sh clone")
}

/// What `clone` left at `dest` besides `.git`: a working tree, if it checked
/// one out.
fn checked_out(dest: &Path) -> Vec<String> {
    match std::fs::read_dir(dest) {
        Ok(entries) => entries
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|name| name != ".git")
            .collect(),
        Err(_) => Vec::new(),
    }
}

fn head_of(dest: &Path) -> String {
    std::fs::read_to_string(dest.join(".git/HEAD"))
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}

#[test]
fn issue_18_clone_with_the_committed_pin_succeeds() {
    if offline() {
        eprintln!(
            "issue_18: VIPRS_OFFLINE=1, so I did NOT check that clone accepts the committed COUNTERPART_REV"
        );
        return;
    }
    // The live check that COUNTERPART_REV is on acadsharp-rs's main, once
    // `clone` asks, and until then the control that the mechanism can still say
    // yes. It uses the committed pin file, the way the clone action does.
    let dest = scratch("issue_18_clone_committed").join("acadsharp-rs");
    let out = clone(&dest, None);
    let all = joined(&out);
    assert_eq!(
        out.status.code(),
        Some(0),
        "counterpart.sh clone refused the committed COUNTERPART_REV (expected exit 0). Either the \
         pin is not on acadsharp-rs's main and has to move, or clone refuses more than it should. \
         This check needs the network; set VIPRS_OFFLINE=1 to skip it on purpose. It said:\n{all}"
    );
    let pin = counterpart_rev();
    assert_eq!(
        head_of(&dest),
        pin,
        "clone said yes but {} is not at the pin:\n{all}",
        dest.display()
    );
}

#[test]
fn issue_18_clone_refuses_a_pin_main_does_not_reach() {
    if offline() {
        eprintln!(
            "issue_18: VIPRS_OFFLINE=1, so I did NOT check that clone refuses an unreachable pin"
        );
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
    let out = clone(&dest, Some(&pin_file));
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
    let left = checked_out(&dest);
    assert!(
        left.is_empty(),
        "clone refused {ORPHANED} and left a working tree at {} anyway: {left:?}. A refusal has \
         to leave nothing checked out, otherwise the suite beside it builds against the very tree \
         it just refused.",
        dest.display()
    );
    assert_ne!(
        head_of(&dest),
        ORPHANED,
        "clone refused {ORPHANED} and left {} with HEAD at it anyway, which is what `verify` \
         would then report as the sibling",
        dest.display()
    );
}
