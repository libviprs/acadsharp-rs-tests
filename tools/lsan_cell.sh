#!/usr/bin/env bash
# The leak cell: tests/resource_semantics.rs under LeakSanitizer, against the
# shared library, on nightly.
#
# What LSan can and cannot see, said once here because it is easy to claim more:
# it sees Rust allocations and the native library's `malloc`. It does not see
# .NET's own GC heap, which is mapped memory and not `malloc`. So a clean report
# means "the crate and the library's C allocations are balanced", not "the
# NativeAOT runtime frees everything", and the RSS loops in the same test file
# are the other half.
#
# The NativeAOT runtime allocates a handful of objects at startup and keeps them
# until the process exits, which LSan reports as leaks. That baseline is a
# constant: it does not depend on how many times the loops ran. So this script
# does not treat "LSan reported something" as the failure. It runs the same
# binary at two loop lengths and fails when the leaked totals GROW with the
# number of iterations, which is what a handle that is never closed looks like.
#
#   tools/lsan_cell.sh [target-triple]
#
# Needs: a nightly toolchain with the target installed, ACADSHARP_NATIVE_DIR
# pointing at an unpacked archive, python3 (to read cargo's JSON).
set -euo pipefail

target=${1:-$(rustc -vV | sed -n 's/^host: //p')}
short=${LSAN_SHORT_ITERATIONS:-20}
long=${LSAN_LONG_ITERATIONS:-300}

export RUSTFLAGS="-Zsanitizer=leak ${RUSTFLAGS:-}"
export VIPRS_REQUIRE_NATIVE=1 ACADSHARP_REQUIRE_NATIVE=1

bin=$(cargo +nightly test --locked --target "$target" --features native --test resource_semantics \
  --no-run --message-format=json 2>/dev/null | python3 -c '
import json, sys
exe = None
for line in sys.stdin:
    try:
        m = json.loads(line)
    except ValueError:
        continue
    if m.get("reason") == "compiler-artifact" and m.get("executable") and m["target"]["name"] == "resource_semantics":
        exe = m["executable"]
print(exe or "")
')
[ -n "$bin" ] || {
  echo "lsan_cell: cargo reported no resource_semantics executable" >&2
  exit 1
}

# exitcode=0 so that a baseline report is not a failure by itself; the numbers
# are what is compared. A crash or a failed assertion still fails: the shell
# sees a non-zero status that is not LSan's.
summary() {
  local n=$1 out
  out=$(LSAN_OPTIONS=exitcode=0 VIPRS_LOOP_ITERATIONS=$n "$bin" --test-threads=1 2>&1) || {
    printf '%s\n' "$out" >&2
    echo "lsan_cell: the test binary failed at $n iterations" >&2
    exit 1
  }
  local line
  line=$(printf '%s\n' "$out" | grep -E '^SUMMARY: LeakSanitizer' || true)
  if [ -z "$line" ]; then
    echo "0 0"
  else
    # "SUMMARY: LeakSanitizer: 82572 byte(s) leaked in 5 allocation(s)."
    printf '%s\n' "$line" | sed -E 's/.*: ([0-9]+) byte\(s\) leaked in ([0-9]+) allocation.*/\1 \2/'
  fi
}

read -r short_bytes short_allocs < <(summary "$short")
read -r long_bytes long_allocs < <(summary "$long")
echo "lsan_cell: $short iterations leaked $short_bytes bytes in $short_allocs allocations (the runtime's baseline)"
echo "lsan_cell: $long iterations leaked $long_bytes bytes in $long_allocs allocations"

if [ "$long_bytes" -gt "$short_bytes" ] || [ "$long_allocs" -gt "$short_allocs" ]; then
  echo "lsan_cell: the leaked total GREW with the loop length, so something is not being freed per iteration" >&2
  exit 1
fi
echo "lsan_cell: no growth between $short and $long iterations"
