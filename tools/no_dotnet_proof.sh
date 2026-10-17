#!/usr/bin/env bash
# The "no .NET installed" proof for the native lane.
#
# The claim is that a clean Rust machine, with no .NET anywhere on it, can link
# and run the NativeAOT library. Saying it is cheap, so this runs it:
#
#   linux <archive-dir>   builds the native smoke test in a rust:bookworm
#                         container, then runs that same binary in a
#                         debian:bookworm-slim container with the suite and the
#                         archive mounted read-only and nothing else installed.
#   control               the positive control, on its own: a framework-dependent
#                         .NET hello-world has to FAIL to start in that very
#                         container. Without it, "the smoke passed with no .NET"
#                         could just mean the container had .NET all along.
#   scrubbed <archive>    macOS: run the smoke with a PATH that cannot find
#                         dotnet, and record whether the runner image had one.
#
# The control needs a published framework-dependent app. `CONTROL_HELLO_DIR`
# names one; unset, the script publishes one with the host's `dotnet`, which is
# on the GitHub ubuntu runner image. The container never sees an SDK either way.
#
# Two knobs exist for watching the checks fail, and CI sets neither:
#   PROOF_EMPTY_LIB=1   mounts an empty tmpfs over the archive's lib/ in the
#                       container, so the library cannot load. The smoke has to
#                       fail and the control has to keep passing: the two checks
#                       are independent.
#   PROOF_PLATFORM      docker --platform, default linux/amd64 on x86_64 hosts.
set -euo pipefail

mode=${1:-}
suite=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
platform=${PROOF_PLATFORM:-}
if [ -z "$platform" ]; then
  case "$(uname -m)" in
    x86_64) platform=linux/amd64 ;;
    aarch64 | arm64) platform=linux/arm64 ;;
    *) platform=linux/amd64 ;;
  esac
fi
image=debian:bookworm-slim

say() { printf 'no_dotnet_proof: %s\n' "$*" >&2; }

control() {
  local hello=${CONTROL_HELLO_DIR:-}
  local tmp=""
  if [ -z "$hello" ]; then
    command -v dotnet >/dev/null || {
      say "no CONTROL_HELLO_DIR and no host dotnet to publish one with"
      exit 2
    }
    tmp=$(mktemp -d)
    trap '[ -z "$tmp" ] || rm -rf "$tmp"' EXIT
    dotnet new console --output "$tmp/src" --name hello >/dev/null
    dotnet publish "$tmp/src" -c Release --self-contained false -o "$tmp/out" >/dev/null
    hello=$tmp/out
  fi
  [ -x "$hello/hello" ] || {
    say "$hello/hello is not an executable apphost"
    exit 2
  }
  # Must fail, and must fail because .NET is missing: the apphost's own message
  # names it. Any other failure (a missing libstdc++, say) would make this a
  # control for the wrong thing, so the message is part of the check.
  set +e
  out=$(docker run --rm --platform "$platform" -v "$hello":/hello:ro "$image" /hello/hello 2>&1)
  rc=$?
  set -e
  if [ "$rc" -eq 0 ]; then
    say "CONTROL FAILED: a framework-dependent .NET app ran in $image, so the container is not clean"
    exit 1
  fi
  if ! printf '%s' "$out" | grep -qi 'install .net\|dotnet'; then
    say "CONTROL INCONCLUSIVE: the app failed (exit $rc) but not for a missing .NET:"
    printf '%s\n' "$out" >&2
    exit 1
  fi
  say "control ok: a framework-dependent .NET app cannot start in $image (exit $rc)"
}

linux() {
  local archive=${1:?usage: no_dotnet_proof.sh linux <archive-dir>}
  archive=$(cd "$archive" && pwd)
  local parent
  parent=$(cd "$suite/.." && pwd)
  local target=$suite/target-bookworm

  # Built where the archive was: the library is a bookworm build, and a binary
  # from a newer glibc would not start in the clean container for a reason that
  # has nothing to do with .NET.
  say "building the smoke test in rust:1-bookworm"
  docker run --rm --platform "$platform" \
    -v "$parent":"$parent" -v "$archive":"$archive":ro -w "$suite" \
    -e CARGO_TARGET_DIR="$target" -e ACADSHARP_NATIVE_DIR="$archive" \
    -e ACADSHARP_REQUIRE_NATIVE=1 -e VIPRS_REQUIRE_NATIVE=1 \
    rust:1-bookworm cargo test --locked --features native --test native_smoke --no-run >&2

  local bin
  bin=$(find "$target/debug/deps" -maxdepth 1 -type f -perm -u+x -name 'native_smoke-*' ! -name '*.d' -print0 \
    | xargs -0 ls -t | head -n 1)
  [ -n "$bin" ] || {
    say "no native_smoke binary under $target"
    exit 1
  }

  control

  local mounts=(-v "$suite":"$suite":ro -v "$archive":"$archive":ro)
  if [ "${PROOF_EMPTY_LIB:-}" = 1 ]; then
    say "PROOF_EMPTY_LIB=1: $archive/lib is an empty tmpfs in the container"
    mounts+=(--mount "type=tmpfs,destination=$archive/lib")
  fi
  say "running $(basename "$bin") in $image"
  docker run --rm --platform "$platform" "${mounts[@]}" \
    -e ACADSHARP_NATIVE_DIR="$archive" -e VIPRS_REQUIRE_NATIVE=1 -w "$suite" \
    "$image" sh -c '
      if command -v dotnet >/dev/null 2>&1 || [ -e /usr/share/dotnet ] || [ -e /usr/lib/dotnet ]; then
        echo "a .NET install is present in this container" >&2
        exit 1
      fi
      exec "$0" --nocapture --test-threads=1
    ' "$bin"
  say "smoke ok with no .NET in the container"
}

scrubbed() {
  local archive=${1:?usage: no_dotnet_proof.sh scrubbed <archive-dir>}
  archive=$(cd "$archive" && pwd)
  if command -v dotnet >/dev/null 2>&1; then
    say "the runner image has dotnet at $(command -v dotnet); the smoke runs without it on PATH"
  else
    say "the runner image has no dotnet on PATH"
  fi
  cd "$suite"
  local bin
  bin=$(cargo test --locked --features native --test native_smoke --no-run --message-format=json 2>/dev/null \
    | python3 -c '
import json, sys
exe = None
for line in sys.stdin:
    try:
        m = json.loads(line)
    except ValueError:
        continue
    if m.get("reason") == "compiler-artifact" and m.get("executable") and m["target"]["name"] == "native_smoke":
        exe = m["executable"]
print(exe or "")
')
  [ -n "$bin" ] || {
    say "cargo did not report a native_smoke executable"
    exit 1
  }
  env -i HOME="$HOME" PATH=/usr/bin:/bin:/usr/sbin:/sbin \
    ACADSHARP_NATIVE_DIR="$archive" VIPRS_REQUIRE_NATIVE=1 \
    "$bin" --nocapture --test-threads=1
  say "smoke ok with a PATH that cannot find dotnet"
}

case $mode in
  linux) shift; linux "$@" ;;
  control) control ;;
  scrubbed) shift; scrubbed "$@" ;;
  *)
    say "usage: no_dotnet_proof.sh linux <archive-dir> | control | scrubbed <archive-dir>"
    exit 2
    ;;
esac
