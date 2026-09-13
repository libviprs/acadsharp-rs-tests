#!/usr/bin/env bash
# The cross-repo pin, in one file, because CI and `cargo test` have to agree
# about it.
#
# This suite depends on `acadsharp-rs` as `{ path = "../acadsharp-rs" }`, so
# something has to lay that checkout down beside this one, and that something
# decides what a green run here means. The org has paid twice for getting it
# wrong. `libviprs-org` resolved an unset repository variable to the empty
# string, which `actions/checkout` reads as "the default branch", so its gate
# compared against whatever moved that morning. `libviprs`' integration job
# clones a same-named counterpart branch, so a stale branch that happens to
# share a name builds against the wrong tree (libviprs#1013).
#
# So: one committed sha, fetched as a sha, verified after checkout, and refused
# unless it is on the crate's `main`. No branch name, no repository variable, no
# fallback, and nothing swallowed with `|| true`.
#
# `.github/actions/clone-counterpart` calls this and `tests/counterpart_pin.rs`
# drives it. That is deliberate: a rule written once in shell and again in Rust
# is two rules, and the test can then only prove the copy.
#
# Exit codes are split three ways, because they mean different things to whoever
# reads the red:
#   0  allowed
#   1  refused, or the environment could not answer (a policy outcome)
#   2  called wrong (a wiring mistake, go and fix the caller)

set -euo pipefail

PROG="$(basename "$0")"
SCRIPT_DIR="$(cd -- "$(dirname -- "$0")" && pwd)"
REPO_ROOT="$(cd -- "$SCRIPT_DIR/.." && pwd)"
DEFAULT_REV_FILE="$REPO_ROOT/COUNTERPART_REV"
COUNTERPART_REPO="${COUNTERPART_REPO:-libviprs/acadsharp-rs}"

# Set by parse_rev, expected_sha and resolve_head. Bash functions return an exit
# status, not a value, and a command substitution would swallow the annotations
# below into the caller's variable.
REV=""
EXPECTED=""
EXPECTED_FROM=""
HEAD_SHA=""
HEAD_REASON=""

usage() {
  cat <<'EOF'
usage: counterpart.sh <command> [args]

  rev [rev-file]                    print the pinned acadsharp-rs sha
  expect [rev-file]                 print the sha the sibling must be at
  head <checkout-dir>               print the sha a sibling checkout is at
  verify <checkout-dir> [rev-file]  check the sibling is the commit we expect
  merged-only <status> <sha>        apply the merged-only rule to a compare status
  clone <dest-dir> [rev-file]       fetch acadsharp-rs at exactly the pin

exit codes: 0 allowed, 1 refused, 2 called wrong.
EOF
}

# `::error file=...::` is a GitHub Actions annotation, and the runner reads it
# off stdout. It is also perfectly readable prose on a laptop, so there is one
# message rather than two.
refuse() {
  printf '::error file=COUNTERPART_REV::%s\n' "$1"
  exit 1
}

# Same exit code, different subject: this one is not the pin file's fault, so it
# does not point a reviewer at that file.
fail() {
  printf '::error::%s\n' "$1"
  exit 1
}

misuse() {
  printf '%s: %s\n' "$PROG" "$1" >&2
  usage >&2
  exit 2
}

validate_sha() {
  local value="$1" where="$2"
  case "$value" in
    "")
      refuse "$where is empty, and I will not guess a branch to put in its place"
      ;;
    *[!0-9a-f]*)
      refuse "$where is '$value', which is not 40 lowercase hex characters. A branch name is never allowed here: guessing a same-named counterpart branch is how libviprs' integration job can build against the wrong tree (libviprs#1013)."
      ;;
  esac
  if [ "${#value}" -ne 40 ]; then
    refuse "$where is '$value', which is ${#value} characters, and a commit sha is 40"
  fi
}

# The pin is the first line that is neither blank nor a comment, and it is the
# whole line. `libviprs-tests` reads its own pin with
# `grep -Eom1 '[0-9a-f]{40}'`, which takes the first 40-hex run anywhere in the
# file: write "bumped from <sha>" in the comment block above the payload and
# that comment silently becomes the pin. There is a fixture for exactly that in
# tests/counterpart_pin.rs, with a control proving the old expression gets it
# wrong.
parse_rev() {
  local file="$1"
  if [ ! -f "$file" ]; then
    refuse "there is no pin file at $file, so there is nothing to pin and I will not guess a branch"
  fi

  local count
  count="$(awk 'NF == 0 { next } $1 ~ /^#/ { next } { n += 1 } END { print n + 0 }' "$file")"
  if [ "$count" -eq 0 ]; then
    refuse "$file has no payload line: every line in it is blank or a comment, so there is no sha in it"
  fi
  if [ "$count" -ne 1 ]; then
    refuse "$file has $count non-comment lines and a pin is exactly one sha, so I cannot tell which of them you meant"
  fi

  local fields
  fields="$(awk 'NF == 0 { next } $1 ~ /^#/ { next } { print NF; exit }' "$file")"
  if [ "$fields" -ne 1 ]; then
    refuse "the payload line of $file must be the sha and nothing else, and it has $fields fields. Put the label on its own comment line."
  fi

  local line
  line="$(awk 'NF == 0 { next } $1 ~ /^#/ { next } { print $1; exit }' "$file")"
  validate_sha "$line" "the payload line of $file"
  REV="$line"
}

# What the sibling has to be. Normally the committed pin; the crate's
# `Suite (acadsharp-rs-tests)` job overrides it, because there the sibling is
# deliberately the crate PR under test rather than the commit this repo pins.
expected_sha() {
  local file="$1"
  if [ -n "${VIPRS_COUNTERPART_EXPECTED_SHA:-}" ]; then
    # Refused rather than ignored. Ignoring a malformed override would fall
    # back to the committed pin, and a silent fallback is the entire family of
    # bug this file exists to prevent.
    validate_sha "$VIPRS_COUNTERPART_EXPECTED_SHA" "VIPRS_COUNTERPART_EXPECTED_SHA"
    EXPECTED="$VIPRS_COUNTERPART_EXPECTED_SHA"
    EXPECTED_FROM="the sha VIPRS_COUNTERPART_EXPECTED_SHA names"
  else
    parse_rev "$file"
    EXPECTED="$REV"
    EXPECTED_FROM="the sha COUNTERPART_REV pins"
  fi
}

# Which commit a checkout is actually at. `clone` below leaves a detached
# `FETCH_HEAD`, so `.git/HEAD` holds the sha directly and no subprocess is
# needed; the `git` call is for an ordinary clone on somebody's laptop. A linked
# worktree, whose `.git` is a file pointing at a gitdir somewhere else, answers
# neither way, and that is a reason rather than an answer.
resolve_head() {
  local dir="$1"
  HEAD_SHA=""
  HEAD_REASON=""

  if [ ! -d "$dir" ]; then
    HEAD_REASON="there is no checkout at $dir"
    return 1
  fi

  if [ -f "$dir/.git/HEAD" ]; then
    local raw
    raw="$(tr -d '[:space:]' < "$dir/.git/HEAD")"
    if [ "${#raw}" -eq 40 ]; then
      case "$raw" in
        *[!0-9a-f]*) : ;;
        *)
          HEAD_SHA="$raw"
          return 0
          ;;
      esac
    fi
  fi

  if command -v git > /dev/null 2>&1; then
    local out
    if out="$(git -C "$dir" rev-parse HEAD 2>&1)"; then
      if [ "${#out}" -eq 40 ]; then
        case "$out" in
          *[!0-9a-f]*) : ;;
          *)
            HEAD_SHA="$out"
            return 0
            ;;
        esac
      fi
      HEAD_REASON="git in $dir answered '$out', which is not a commit sha"
      return 1
    fi
    HEAD_REASON="git could not read HEAD in $dir ($out)"
    return 1
  fi

  HEAD_REASON="$dir/.git/HEAD does not hold a detached sha and there is no git on PATH to ask"
  return 1
}

cmd_rev() {
  parse_rev "$1"
  printf '%s\n' "$REV"
}

cmd_expect() {
  expected_sha "$1"
  printf '%s\n' "$EXPECTED"
}

cmd_head() {
  if resolve_head "$1"; then
    printf '%s\n' "$HEAD_SHA"
    return 0
  fi
  fail "$HEAD_REASON"
}

cmd_verify() {
  local dir="$1" file="$2"
  expected_sha "$file"

  if resolve_head "$dir"; then
    if [ "$HEAD_SHA" = "$EXPECTED" ]; then
      printf 'The acadsharp-rs beside this suite is %s, which is %s.\n' "$HEAD_SHA" "$EXPECTED_FROM"
      return 0
    fi
    refuse "the acadsharp-rs beside this suite is at $HEAD_SHA and I expected $EXPECTED ($EXPECTED_FROM). Running the suite against a different crate than the one it pins is exactly the outcome this mechanism exists to prevent, so this is a failure and not a warning."
  fi

  # A wrong sibling is wrong either way; this branch is only about not knowing.
  if [ "${VIPRS_REQUIRE_COUNTERPART:-}" = "1" ]; then
    fail "VIPRS_REQUIRE_COUNTERPART=1 and I could not tell which acadsharp-rs is beside this suite: $HEAD_REASON. The clone action always leaves a real detached HEAD, so in CI this means something went wrong before this point. It fails rather than skips because a check that cannot tell is the same colour as one that passed."
  fi

  # There is deliberately no "the sibling is missing" branch anywhere in here.
  # With a hard path dependency and no sibling, cargo dies at manifest load and
  # no test binary is ever built, so nothing would be left to print it.
  printf 'counterpart_pin: I did not check which acadsharp-rs is beside this suite, because %s. Set VIPRS_REQUIRE_COUNTERPART=1 to make that a failure, which is what CI does.\n' "$HEAD_REASON"
}

# The merged-only rule. `identical` and `behind` both mean the pin is on the
# crate's `main`: `behind` is what every pin becomes the moment the next crate
# PR lands, and refusing it would demand a pin bump for every unrelated crate
# commit. `ahead` and `diverged` mean it is not on `main` at all.
cmd_merged_only() {
  local status="$1" rev="$2"
  case "$status" in
    identical | behind)
      printf 'acadsharp-rs %s is on main (compare says %s), so the pin is merged.\n' "$rev" "$status"
      ;;
    ahead | diverged)
      refuse "refusing to pin an unmerged acadsharp-rs commit: compare/main...$rev says '$status', so $rev is not on that repository's main. A pin has to name something that will still be there tomorrow, and a PR head can be force-pushed or closed."
      ;;
    *)
      refuse "I do not recognise the compare status '$status' for main...$rev, so I am refusing rather than reading it as merged. An empty value here means the compare call itself failed, and a value you do not expect means the API changed shape; neither of those is evidence that the pin is on main."
      ;;
  esac
}

cmd_clone() {
  local dest="$1" file="$2"
  parse_rev "$file"

  if [ -d "$dest" ]; then
    local existing
    existing="$(find "$dest" -mindepth 1 -maxdepth 1 -print -quit 2> /dev/null || true)"
    if [ -n "$existing" ]; then
      refuse "$dest already exists and is not empty, so I am not fetching into it. On a runner that means a previous step already put something there; on a laptop it is somebody's own checkout, and overwriting it is how you end up testing a tree nobody asked for."
    fi
  fi

  mkdir -p "$dest"
  git -C "$dest" init -q
  git -C "$dest" remote add origin "https://github.com/${COUNTERPART_REPO}.git"

  # One sha, fetched as a sha. There is no refspec naming a branch anywhere in
  # this file, so there is nothing for a fallback to fall back to.
  if ! git -C "$dest" fetch --quiet --depth 1 origin "$REV"; then
    refuse "could not fetch $COUNTERPART_REPO at $REV. Either that sha does not exist there, or no ref reaches it (a fetch by sha needs the commit to be reachable from a ref). There is no fallback branch here on purpose."
  fi

  git -C "$dest" checkout -q FETCH_HEAD

  # Verified, not announced. The org's action ends by echoing whatever
  # `rev-parse` returns, which reports a mismatch in the same voice as a match.
  if ! resolve_head "$dest"; then
    fail "I fetched $REV into $dest and then could not read its HEAD back: $HEAD_REASON"
  fi
  if [ "$HEAD_SHA" != "$REV" ]; then
    refuse "I asked for $REV and $dest is at $HEAD_SHA"
  fi
  printf 'Checked out %s at %s, which is the sha COUNTERPART_REV pins.\n' "$COUNTERPART_REPO" "$HEAD_SHA"
}

main() {
  local cmd="${1:-}"
  if [ $# -gt 0 ]; then
    shift
  fi
  case "$cmd" in
    rev)
      [ $# -le 1 ] || misuse "rev takes an optional pin file and nothing else"
      cmd_rev "${1:-$DEFAULT_REV_FILE}"
      ;;
    expect)
      [ $# -le 1 ] || misuse "expect takes an optional pin file and nothing else"
      cmd_expect "${1:-$DEFAULT_REV_FILE}"
      ;;
    head)
      [ $# -eq 1 ] || misuse "head takes exactly one checkout directory"
      cmd_head "$1"
      ;;
    verify)
      if [ $# -lt 1 ] || [ $# -gt 2 ]; then
        misuse "verify takes a checkout directory and an optional pin file"
      fi
      cmd_verify "$1" "${2:-$DEFAULT_REV_FILE}"
      ;;
    merged-only)
      [ $# -eq 2 ] || misuse "merged-only takes a compare status and the sha it is about"
      cmd_merged_only "$1" "$2"
      ;;
    clone)
      if [ $# -lt 1 ] || [ $# -gt 2 ]; then
        misuse "clone takes a destination directory and an optional pin file"
      fi
      cmd_clone "$1" "${2:-$DEFAULT_REV_FILE}"
      ;;
    -h | --help | help)
      usage
      ;;
    "")
      usage >&2
      exit 2
      ;;
    *)
      misuse "I do not know the command '$cmd'"
      ;;
  esac
}

main "$@"
