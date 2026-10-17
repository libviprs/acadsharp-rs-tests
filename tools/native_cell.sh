#!/usr/bin/env bash
# Run one cell of the native matrix: one target, one link mode.
#
#   tools/native_cell.sh <target> <shared|static> <archive-dir> <out-dir> [<container-image>]
#
# What a cell runs: the real-library smoke, the cell test (`fixture_parity`,
# which holds the cell to the link mode it claims, checks the ACadSharp version
# lockstep and writes the canonical JSONL for every drawing into <out-dir>) and
# the native emitter's own tests. `tools/compare_cells.py` then holds a target's
# two output directories byte-identical.
#
# `static` adds `--features acadsharp-rs/link-static`, which the crate refuses
# unless the archive's manifest says `static_certified: true`; the matrix only
# plans a static cell where it does, and a static cell that linked shared would
# fail `fixture_parity`'s own check.
#
# A container image means the target is a musl build, so the cell runs inside
# that image with `docker run`, the suite, its siblings and the archive mounted
# at the same paths. It is not a job container: GitHub's JavaScript actions do
# not run in an Alpine job container, and nothing in this script needs them.
# musl links its own libc statically by default, which cannot load a shared
# library, so those cells build with `-C target-feature=-crt-static`.
set -euo pipefail

target=${1:?usage: native_cell.sh <target> <shared|static> <archive-dir> <out-dir> [<container-image>]}
mode=${2:?}
archive=${3:?}
out=${4:?}
image=${5:-}

case $mode in
  shared) features=native ;;
  static) features=native,acadsharp-rs/link-static ;;
  *)
    echo "native_cell: mode must be shared or static, not '$mode'" >&2
    exit 2
    ;;
esac

suite=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
archive=$(cd "$archive" && pwd)
mkdir -p "$out"
out=$(cd "$out" && pwd)
cargo_args=(test --locked --features "$features" --test native_smoke --test fixture_parity --test emit_acadsharp)

echo "native_cell: $target, $mode link, archive $archive" >&2

if [ -z "$image" ]; then
  VIPRS_CELL_MODE=$mode VIPRS_CELL_OUT=$out ACADSHARP_NATIVE_DIR=$archive \
    VIPRS_REQUIRE_NATIVE=1 ACADSHARP_REQUIRE_NATIVE=1 \
    cargo "${cargo_args[@]}"
else
  parent=$(cd "$suite/.." && pwd)
  case $(uname -m) in
    x86_64) platform=linux/amd64 ;;
    aarch64 | arm64) platform=linux/arm64 ;;
    *) platform=linux/amd64 ;;
  esac
  docker run --rm --platform "$platform" \
    -v "$parent":"$parent" -v "$archive":"$archive":ro -v "$out":"$out" -w "$suite" \
    -e VIPRS_CELL_MODE="$mode" -e VIPRS_CELL_OUT="$out" -e ACADSHARP_NATIVE_DIR="$archive" \
    -e VIPRS_REQUIRE_NATIVE=1 -e ACADSHARP_REQUIRE_NATIVE=1 \
    -e CARGO_TARGET_DIR="$suite/target-$target-$mode" \
    -e RUSTFLAGS="${RUSTFLAGS:-} -C target-feature=-crt-static" \
    "$image" cargo "${cargo_args[@]}"
fi
