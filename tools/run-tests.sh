#!/usr/bin/env bash
# The gate for this suite: one script, run three ways.
#
#   tools/run-tests.sh --plan        say which three trees it would test, then stop
#   tools/run-tests.sh               run the suite in a stock rust container
#   tools/run-tests.sh --in-ci       run the cargo lines on this host (what CI calls)
#   tools/run-tests.sh -- CMD...     run CMD in that same environment instead of the lines
#   tools/run-tests.sh --verdicts-base REF
#                                    also hold the parity verdicts to REF's copy of
#                                    parity/VERDICTS.toml (a git ref of this checkout)
#
# There is a fourth form, `--in-container`, which is the second half of the
# default one: it runs inside the container, fetches the native archive into
# $CARGO_HOME and then does what `--in-ci` does. It exists so the same script
# can be pushed to the x64 docker host with `tools/nas.sh run`, which already
# provides the container and the bind-mounted trees.
#
# Why a script and not an image. libviprs-tests builds an image per push and
# keeps a step-for-step copy of its CI in a replay script held honest by a
# parser (libviprs/libviprs#683 and #691 are what that cost). Here the whole
# environment is a stock `rust` image, the trees go in as a tar stream, and the
# only thing that is not already in the image is the native archive, which is
# fetched once per sha into a named volume and verified every time
# (`NATIVE_ARTIFACT.toml` is the pin, `tools/fetch_native.py` the fetch).
#
# Every extracted file is touched. `git archive` stamps files with the commit's
# time, and cargo decides a path dependency is fresh by comparing mtimes with the
# cached build, so a tree whose commit is older than the last build in the volume
# would be reported fresh and never rebuilt: a gate that tests the previous run's
# code. The registry stays cached; the three path crates rebuild on every run.
#
# Three trees take part, laid out as siblings because the suite depends on both
# by path (`../acadsharp-rs`, `../acadrust`):
#
#   this checkout          the suite, always the one this script lives in
#   ../acadsharp-rs        or --acadsharp-rs PATH
#   ../acadrust            or --acadrust PATH
#
# A tree goes into the container as `git archive` of the commit it is at, so
# untracked files never take part and a file whose case differs on a laptop
# cannot pass here and fail on a case-sensitive runner. Each one gets a
# `.git/HEAD` holding that sha, so `tools/counterpart.sh` and the emitters can
# still say which commit they ran against. The commits are printed beside the
# pins that name them (`--plan`), and are not required to equal them: the pin
# governs CI, a local run tests whatever is on disk, which is how
# libviprs-tests runs too. A push from acadsharp-rs or acadrust is gated by
# passing that tree here, in its slot.
#
# Every REQUIRE variable is set to 1, for the reason the `Test` job in
# acadsharp-rs says at length: a lane that skips is the same colour as a lane
# that passes.
#
# Exit codes: 0 green, 1 a test failed or a tree could not be used, 2 called
# wrong.

set -euo pipefail

PROG="$(basename "$0")"
HERE="$(cd -- "$(dirname -- "$0")" && pwd)"
SUITE="$(cd -- "$HERE/.." && pwd)"
PARENT="$(cd -- "$SUITE/.." && pwd)"

IMAGE="${PARITY_IMAGE:-rust:1.99.1-bookworm}"
VOLUME="${PARITY_CARGO_VOLUME:-acadsharp-parity-cargo}"

# The variables the gate sets, in the order --plan prints them.
REQUIRE_VARS=(VIPRS_REQUIRE_COUNTERPART ACADSHARP_REQUIRE_NATIVE ACADRUST_REQUIRE PARITY_REQUIRE_CORPUS)

ACADSHARP_RS="$PARENT/acadsharp-rs"
ACADRUST="$PARENT/acadrust"
MODE=run
PLAN_ONLY=0
ARCHIVE_SHA_OVERRIDE="${ACADSHARP_ARCHIVE_SHA256:-}"
EXPECT_FROM_TREES=1
# Everything after `--` replaces the cargo lines, for running one thing in the
# same environment (the trees, the archive, the REQUIRE variables).
EXEC=()
NATIVE_TGZ="${ACADSHARP_NATIVE_TGZ:-}"
VERDICTS_BASE_REF=""
VERDICTS_BASE_FILE="${PARITY_VERDICTS_BASE:-}"

usage() {
  sed -n '2,/^set -euo pipefail$/p' "$0" | sed '$d; s/^# \{0,1\}//' >&2
}

misuse() {
  printf '%s: %s\n' "$PROG" "$1" >&2
  exit 2
}

while [ $# -gt 0 ]; do
  case "$1" in
    --plan) PLAN_ONLY=1 ;;
    --in-ci) MODE=in-ci; EXPECT_FROM_TREES=0 ;;
    --in-container) MODE=in-container ;;
    --acadsharp-rs)
      [ $# -ge 2 ] || misuse "--acadsharp-rs needs a path"
      ACADSHARP_RS="$2"; shift ;;
    --acadrust)
      [ $# -ge 2 ] || misuse "--acadrust needs a path"
      ACADRUST="$2"; shift ;;
    --archive-sha256)
      [ $# -ge 2 ] || misuse "--archive-sha256 needs a digest"
      ARCHIVE_SHA_OVERRIDE="$2"; shift ;;
    --native-tgz)
      # An archive that is not the published one (a rebuild, or another
      # arch's), by path on the machine running this script. It goes in with the
      # trees and is verified against --archive-sha256, which is then required.
      [ $# -ge 2 ] || misuse "--native-tgz needs a path"
      NATIVE_TGZ="$2"; shift ;;
    --verdicts-base)
      # The self-approval check. A change that moves a verdict and re-records it in
      # the same commit passes the committed table, so the run is held a second
      # time to the table as it was at REF (the remote's tip, for a push), and the
      # moved rows are the failure. `--verdicts-base-file` is the same with a file.
      [ $# -ge 2 ] || misuse "--verdicts-base needs a git ref"
      VERDICTS_BASE_REF="$2"; shift ;;
    --verdicts-base-file)
      [ $# -ge 2 ] || misuse "--verdicts-base-file needs a path"
      VERDICTS_BASE_FILE="$2"; shift ;;
    --dep-commit)
      [ $# -ge 2 ] || misuse "--dep-commit needs a sha"
      export ACADSHARP_VIPRS_DEP_COMMIT="$2"; shift ;;
    --) shift; EXEC=("$@"); break ;;
    -h|--help) usage; exit 0 ;;
    *) misuse "I do not know the argument '$1'" ;;
  esac
  shift
done

# --- what is where ----------------------------------------------------------

pin_of() { # <name> -> the sha the suite pins that tree at
  COUNTERPART_NAME="$1" "$HERE/counterpart.sh" rev 2>/dev/null || echo "unreadable"
}

head_of() { # <dir> -> its HEAD sha, or "none"
  # A directory that merely sits inside some other repository would answer for
  # that repository, so ask for the top of the checkout first.
  local dir="$1" top here raw
  [ -d "$dir" ] || { echo none; return; }
  # A tree this script laid down itself carries a bare sha in .git/HEAD and no
  # repository to ask.
  if [ -f "$dir/.git/HEAD" ]; then
    raw="$(tr -d '[:space:]' < "$dir/.git/HEAD")"
    if [ "${#raw}" -eq 40 ] && [ -z "${raw//[0-9a-f]/}" ]; then echo "$raw"; return; fi
  fi
  top="$(git -C "$dir" rev-parse --show-toplevel 2>/dev/null)" || { echo none; return; }
  here="$(cd -- "$dir" && pwd -P)"
  top="$(cd -- "$top" && pwd -P)"
  [ "$top" = "$here" ] || { echo none; return; }
  git -C "$dir" rev-parse HEAD 2>/dev/null || echo none
}

dirty_of() { # <dir> -> "clean", "dirty" (the archive ignores the difference)
  if [ -z "$(git -C "$1" status --porcelain 2>/dev/null)" ]; then echo clean; else echo dirty; fi
}

container_arch() {
  # The docker daemon's architecture when there is one (the client may be a
  # laptop talking to an x64 host over ssh), this host's otherwise.
  local m
  m="$(docker info --format '{{.Architecture}}' 2>/dev/null || true)"
  [ -n "$m" ] || m="$(uname -m)"
  case "$m" in
    x86_64|amd64) echo amd64 ;;
    arm64|aarch64) echo arm64 ;;
    *) echo "$m" ;;
  esac
}

plan() {
  local arch="$1"
  local s_head r_head a_head
  s_head="$(head_of "$SUITE")"
  r_head="$(head_of "$ACADSHARP_RS")"
  a_head="$(head_of "$ACADRUST")"
  printf 'tree            sha                                       state  pin                                       path\n'
  printf 'acadsharp-rs-tests  %s  %s  -                                         %s\n' "$s_head" "$(dirty_of "$SUITE")" "$SUITE"
  printf 'acadsharp-rs    %s  %s  %s  %s\n' "$r_head" "$(dirty_of "$ACADSHARP_RS")" "$(pin_of acadsharp-rs)" "$ACADSHARP_RS"
  printf 'acadrust        %s  %s  %s  %s\n' "$a_head" "$(dirty_of "$ACADRUST")" "$(pin_of acadrust)" "$ACADRUST"
  printf 'arch            %s\n' "$arch"
  printf 'image           %s\n' "$IMAGE"
  local v
  for v in "${REQUIRE_VARS[@]}"; do
    printf 'env             %s=1\n' "$v"
  done
  if [ -n "$VERDICTS_BASE_REF" ]; then
    printf 'verdicts-base   %s\n' "$VERDICTS_BASE_REF"
  elif [ -n "$VERDICTS_BASE_FILE" ]; then
    printf 'verdicts-base   %s\n' "$VERDICTS_BASE_FILE"
  fi
  if [ "$EXPECT_FROM_TREES" = 1 ]; then
    printf 'env             VIPRS_COUNTERPART_EXPECTED_SHA=%s\n' "$r_head"
    printf 'env             ACADRUST_EXPECTED_SHA=%s\n' "$a_head"
  fi
}

# --- the cargo lines --------------------------------------------------------

# The same lines the `test` and `parity` jobs run. Kept in one function so the
# default form, `--in-ci` and `--in-container` cannot disagree about them.
# `--locked` only where the lock file can honestly be held: with both siblings
# at the commits the suite pins. A crate PR's tree, or a fork commit that is not
# the pin, can change what resolves (acadsharp-rs's build script grew serde_json
# after the suite's lock was written), and the crate's own Suite job has never
# passed `--locked` for that reason. Saying so beats a refusal that blames the
# lock file for a change in a sibling.
LOCKED=()
decide_locked() {
  local name dir pinned head
  LOCKED=(--locked)
  for pair in "acadsharp-rs:$ACADSHARP_RS" "acadrust:$ACADRUST"; do
    name="${pair%%:*}"; dir="${pair#*:}"
    pinned="$(pin_of "$name")"
    head="$(head_of "$dir")"
    if [ "$pinned" != "$head" ]; then
      LOCKED=()
      echo "note: $name is at $head and the suite pins $pinned, so cargo runs without --locked" >&2
    fi
  done
}

# The same lines the `test` and `parity` jobs run. Kept in one function so the
# default form, `--in-ci` and `--in-container` cannot disagree about them.
cargo_lines() {
  if [ "${#EXEC[@]}" -gt 0 ]; then
    set -x
    "${EXEC[@]}"
    return
  fi
  decide_locked
  set -x
  # Every line runs, and the script fails if any did: a red first line must not
  # hide the parity lane behind it (a push whose only red test is early would
  # otherwise never show the moved verdict the gate exists to show).
  local rc=0
  cargo test "${LOCKED[@]}" --no-fail-fast --all-targets || rc=1
  cargo test "${LOCKED[@]}" --no-fail-fast --doc || rc=1
  # The parity lane: both emitters, the oracle, ezdxf and the verdict table.
  cargo test "${LOCKED[@]}" --no-fail-fast --features acadrust --all-targets || rc=1
  if [ -n "${PARITY_VERDICTS_BASE:-}" ]; then
    # The same test, a second time, against the base table: this is the pass a
    # moved-and-re-recorded verdict cannot get through.
    cargo test "${LOCKED[@]}" --features acadrust --test parity -- --exact the_run_agrees_with_the_committed_verdicts || rc=1
  fi
  return "$rc"
}

export_requires() {
  local v
  for v in "${REQUIRE_VARS[@]}"; do export "$v=1"; done
}

# --- the three forms --------------------------------------------------------

# The native archive. The published one comes from `NATIVE_ARTIFACT.toml` through
# `tools/fetch_native.py`, which refuses a wrong hash before it extracts a byte,
# for the target of whatever arch this runs on. `--native-tgz` supplies one that
# is not the published one (a rebuild), verified against `--archive-sha256`.
fetch_native() {
  local dir sha target tomlsha
  if [ -n "$NATIVE_TGZ" ]; then
    local archive cache top
    [ -f "$NATIVE_TGZ" ] || { echo "$PROG: no archive at $NATIVE_TGZ" >&2; exit 1; }
    [ -n "$ARCHIVE_SHA_OVERRIDE" ] || { echo "$PROG: --native-tgz needs --archive-sha256: an archive nobody hashed is not a label" >&2; exit 1; }
    sha="$ARCHIVE_SHA_OVERRIDE"; archive="$(basename "$NATIVE_TGZ")"
    cache="${CARGO_HOME:-$HOME/.cargo}/acadsharp-native/$sha"
    mkdir -p "$cache"
    cp "$NATIVE_TGZ" "$cache/$archive"
    echo "$sha  $cache/$archive" | sha256sum -c -
    rm -rf "$cache/unpacked"
    mkdir -p "$cache/unpacked"
    tar -xzf "$cache/$archive" -C "$cache/unpacked"
    top="$(ls "$cache/unpacked")"
    [ "$(printf '%s\n' "$top" | wc -l)" -eq 1 ] || { echo "$PROG: the archive has more than one top-level entry" >&2; exit 1; }
    dir="$cache/unpacked/$top"
  else
    case "$(uname -m)" in
      x86_64|amd64) target=linux-x64 ;;
      arm64|aarch64) target=linux-arm64 ;;
      *) echo "$PROG: no native archive for $(uname -m)" >&2; exit 1 ;;
    esac
    dir="$(python3 "$SUITE/tools/fetch_native.py" --target "$target")"
    sha="$(python3 - "$SUITE/NATIVE_ARTIFACT.toml" "$target" <<'PY'
import sys, tomllib
m = tomllib.load(open(sys.argv[1], "rb"))
print(next(t["sha256"] for t in m["target"] if t["name"] == sys.argv[2]))
PY
)"
    tomlsha="$sha"
    [ -z "$ARCHIVE_SHA_OVERRIDE" ] || [ "$ARCHIVE_SHA_OVERRIDE" = "$tomlsha" ] || {
      echo "$PROG: --archive-sha256 disagrees with NATIVE_ARTIFACT.toml for $target; use --native-tgz for another archive" >&2; exit 1; }
  fi
  export ACADSHARP_NATIVE_DIR="$dir"
  # What the emitters stamp into `run.json`, so two archives that claim one
  # `artifact_version` still have different labels.
  export ACADSHARP_ARCHIVE_SHA256="$sha"
  echo "native archive: $sha at $ACADSHARP_NATIVE_DIR"
}

if [ "$PLAN_ONLY" = 1 ]; then
  plan "$(container_arch)"
  exit 0
fi

case "$MODE" in

  in-ci)
    export_requires
    # A job that already ran the crate's fetch action has an archive; one that
    # did not gets the same pin, fetched and verified the same way, so the
    # native lane cannot be compiled out of a run that asked for it.
    if [ -z "${ACADSHARP_NATIVE_DIR:-}" ] || [ ! -d "$ACADSHARP_NATIVE_DIR" ]; then
      fetch_native
    fi
    cd "$SUITE"
    cargo_lines
    ;;

  in-container)
    export_requires
    # Where each sibling sits is whatever the caller laid down. `--plan` is not
    # run here: the trees carry a `.git/HEAD` and nothing else.
    if [ "$EXPECT_FROM_TREES" = 1 ]; then
      VIPRS_COUNTERPART_EXPECTED_SHA="${VIPRS_COUNTERPART_EXPECTED_SHA:-$(tr -d '[:space:]' < "$ACADSHARP_RS/.git/HEAD")}"
      ACADRUST_EXPECTED_SHA="${ACADRUST_EXPECTED_SHA:-$(tr -d '[:space:]' < "$ACADRUST/.git/HEAD")}"
      export VIPRS_COUNTERPART_EXPECTED_SHA ACADRUST_EXPECTED_SHA
    fi
    fetch_native
    cd "$SUITE"
    cargo_lines
    ;;

  run)
    command -v docker >/dev/null || { echo "$PROG: no docker on PATH" >&2; exit 1; }
    arch="$(container_arch)"
    plan "$arch"
    for t in "$SUITE" "$ACADSHARP_RS" "$ACADRUST"; do
      [ "$(head_of "$t")" != none ] || { echo "$PROG: $t is not the top of a git checkout" >&2; exit 1; }
    done
    stage="$(mktemp -d "${TMPDIR:-/tmp}/parity-gate.XXXXXX")"
    trap 'rm -rf "$stage"' EXIT
    for pair in "$SUITE:acadsharp-rs-tests" "$ACADSHARP_RS:acadsharp-rs" "$ACADRUST:acadrust"; do
      dir="${pair%:*}"; name="${pair##*:}"
      mkdir -p "$stage/$name/.git"
      git -C "$dir" archive HEAD | tar -x -C "$stage/$name"
      git -C "$dir" rev-parse HEAD > "$stage/$name/.git/HEAD"
    done
    envs=()
    if [ -n "$VERDICTS_BASE_REF" ]; then
      mkdir -p "$stage/base"
      if git -C "$SUITE" show "$VERDICTS_BASE_REF:parity/VERDICTS.toml" > "$stage/base/VERDICTS.toml" 2>/dev/null; then
        VERDICTS_BASE_FILE="$stage/base/VERDICTS.toml"
      else
        # A ref that has no table yet (the push that introduces it) has nothing
        # to be held to; say so rather than passing silently.
        echo "note: $VERDICTS_BASE_REF has no parity/VERDICTS.toml, so there is no base table to hold the verdicts to" >&2
        rm -rf "$stage/base"
      fi
    elif [ -n "$VERDICTS_BASE_FILE" ]; then
      mkdir -p "$stage/base"
      cp "$VERDICTS_BASE_FILE" "$stage/base/VERDICTS.toml"
    fi
    [ ! -f "$stage/base/VERDICTS.toml" ] || envs+=(-e "PARITY_VERDICTS_BASE=/work/base/VERDICTS.toml")
    [ -z "$ARCHIVE_SHA_OVERRIDE" ] || envs+=(-e "ACADSHARP_ARCHIVE_SHA256=$ARCHIVE_SHA_OVERRIDE")
    [ -z "${ACADSHARP_VIPRS_DEP_COMMIT:-}" ] || envs+=(-e "ACADSHARP_VIPRS_DEP_COMMIT=$ACADSHARP_VIPRS_DEP_COMMIT")
    if [ -n "$NATIVE_TGZ" ]; then
      [ -f "$NATIVE_TGZ" ] || { echo "$PROG: no archive at $NATIVE_TGZ" >&2; exit 1; }
      mkdir -p "$stage/native"
      cp "$NATIVE_TGZ" "$stage/native/$(basename "$NATIVE_TGZ")"
      envs+=(-e "ACADSHARP_NATIVE_TGZ=/work/native/$(basename "$NATIVE_TGZ")")
    fi
    inner=(/work/acadsharp-rs-tests/tools/run-tests.sh --in-container
           --acadsharp-rs /work/acadsharp-rs --acadrust /work/acadrust)
    if [ "${#EXEC[@]}" -gt 0 ]; then inner+=(-- "${EXEC[@]}"); fi
    tar -C "$stage" -cf - . | docker run --rm -i --platform "linux/$arch" \
      -v "$VOLUME:/cargo-home" \
      -e CARGO_HOME=/cargo-home -e CARGO_TARGET_DIR=/cargo-home/target \
      -e CARGO_TERM_COLOR=never -e RUSTFLAGS=-Dwarnings \
      "${envs[@]}" "$IMAGE" \
      sh -c 'mkdir -p /work && tar -x -C /work && find /work -type f -exec touch {} + && exec "$@"' sh "${inner[@]}"
    ;;
esac
