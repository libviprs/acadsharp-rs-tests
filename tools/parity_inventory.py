#!/usr/bin/env python3
"""Diff ACadSharp's test files against upstream_parity/manifest.toml.

ACadSharp's own tests are the closest thing to a specification of what a DWG
reader does, so the manifest classifies every one of them. This tool is what
stops that classification drifting: a bump that adds, renames or deletes a test
file fails here until somebody decides what the change means.

    python3 tools/parity_inventory.py                      # offline, the committed snapshot
    python3 tools/parity_inventory.py --upstream-dir ../ACadSharp
    python3 tools/parity_inventory.py --upstream-commit <sha>   # a proposed bump, over the API
    python3 tools/parity_inventory.py --write-snapshot          # refresh the snapshot for a bump

Offline by default. The snapshot is named after the commit it describes, so a
bump cannot silently compare a new manifest against the old revision's file
list, which would pass and mean nothing.

Exits non-zero on any file upstream with no row, or any row with no file
upstream. It never edits the manifest: deciding what a new test file means is
the one thing here that needs a person.
"""

from __future__ import annotations

import argparse
import json
import os
import pathlib
import sys
import tomllib
import urllib.error
import urllib.request

ROOT = pathlib.Path(__file__).resolve().parent.parent
MANIFEST = ROOT / "upstream_parity" / "manifest.toml"
PARITY_DIR = ROOT / "upstream_parity"

API = "https://api.github.com/repos/DomCR/ACadSharp/git/trees/{sha}?recursive=1"

# Coverage counts these. `not_applicable` is deliberately absent: writer suites
# and DXF suites are not gaps, and counting them would drag the number down
# every time upstream adds a writer test while saying nothing about this crate.
COVERED = ("mirrored", "differential", "native_conformance")
STATUSES = COVERED + ("not_applicable",)


def load_manifest() -> dict:
    with MANIFEST.open("rb") as f:
        return tomllib.load(f)


def snapshot_path(commit: str) -> pathlib.Path:
    return PARITY_DIR / f"inventory.{commit}.txt"


def from_snapshot(commit: str) -> list[str] | None:
    path = snapshot_path(commit)
    if not path.exists():
        return None
    return sorted(l.strip() for l in path.read_text().splitlines() if l.strip())


def from_dir(upstream_dir: pathlib.Path, test_root: str) -> list[str]:
    base = upstream_dir / test_root
    if not base.is_dir():
        raise SystemExit(f"error: {base} is not a directory, so there is nothing to inventory")
    return sorted(str(p.relative_to(base)).replace("\\", "/") for p in base.rglob("*.cs"))


def from_api(commit: str, test_root: str) -> list[str]:
    req = urllib.request.Request(
        API.format(sha=commit),
        headers={
            "User-Agent": "acadsharp-rs-tests-parity",
            "Accept": "application/vnd.github+json",
        },
    )
    # Read from the environment if it happens to be set, purely for the rate
    # limit. Never written anywhere, never logged.
    token = os.environ.get("GH_TOKEN") or os.environ.get("GITHUB_TOKEN")
    if token:
        req.add_header("Authorization", f"Bearer {token}")
    try:
        with urllib.request.urlopen(req, timeout=60) as resp:
            tree = json.loads(resp.read())
    except urllib.error.HTTPError as exc:
        raise SystemExit(f"error: GitHub returned {exc.code} for commit {commit}: {exc.reason}")
    except urllib.error.URLError as exc:
        raise SystemExit(f"error: cannot reach GitHub: {exc.reason}")

    if tree.get("truncated"):
        raise SystemExit(
            "error: GitHub truncated the tree listing, so this inventory would be incomplete "
            "and a missing file would read as a deleted one. Use --upstream-dir against a "
            "local checkout instead."
        )
    prefix = test_root.rstrip("/") + "/"
    return sorted(
        e["path"][len(prefix):]
        for e in tree.get("tree", [])
        if e.get("type") == "blob" and e["path"].startswith(prefix) and e["path"].endswith(".cs")
    )


def compare(upstream: list[str], rows: list[dict]) -> tuple[list[str], list[str]]:
    classified = {r["path"] for r in rows}
    up = set(upstream)
    return sorted(up - classified), sorted(classified - up)


def report_coverage(rows: list[dict]) -> None:
    counts = {s: 0 for s in STATUSES}
    unknown = []
    for r in rows:
        if r["status"] in counts:
            counts[r["status"]] += 1
        else:
            unknown.append(f"{r['path']}: {r['status']}")
    if unknown:
        raise SystemExit("error: rows with a status that is not one of the four: " + ", ".join(unknown))

    total = sum(counts.values())
    applicable = total - counts["not_applicable"]
    print()
    for s in STATUSES:
        print(f"  {s:<20} {counts[s]:>4}")
    print(f"  {'-' * 20} {'-' * 4}")
    print(f"  {'total':<20} {total:>4}")
    print(f"  {'applicable':<20} {applicable:>4}   (total minus not_applicable)")
    if applicable:
        pct = 100.0 * counts["mirrored"] / applicable
        print(f"\n  mirrored in Rust: {counts['mirrored']}/{applicable} ({pct:.1f}% of applicable)")
        print("  The rest is real coverage through the oracle comparison, not a gap;")
        print("  H4.2 promotes differential rows to mirrored as it writes them.")


def main() -> int:
    ap = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    ap.add_argument("--upstream-dir", type=pathlib.Path,
                    help="inventory a local ACadSharp checkout instead of the snapshot or the API")
    ap.add_argument("--upstream-commit",
                    help="inventory this commit over the API instead of the manifest's pinned one")
    ap.add_argument("--write-snapshot", action="store_true",
                    help="write the inventory to upstream_parity/inventory.<sha>.txt")
    args = ap.parse_args()

    manifest = load_manifest()
    pinned = manifest["upstream"]["commit"]
    test_root = manifest["upstream"]["test_root"]
    rows = manifest.get("test_file", [])
    commit = args.upstream_commit or pinned

    if args.upstream_dir:
        upstream = from_dir(args.upstream_dir, test_root)
        source = f"{args.upstream_dir}/{test_root}"
    elif args.upstream_commit:
        # An explicit commit always goes to the API. Reading a snapshot here
        # would compare a proposed bump against a file list that predates it.
        upstream = from_api(commit, test_root)
        source = f"GitHub @ {commit[:7]}"
    else:
        upstream = from_snapshot(commit)
        if upstream is None:
            upstream = from_api(commit, test_root)
            source = f"GitHub @ {commit[:7]} (no snapshot committed)"
        else:
            source = f"snapshot @ {commit[:7]}"

    print(f"upstream: {len(upstream)} test files from {source}")
    print(f"manifest: {len(rows)} rows, pinned to {pinned[:7]}")

    if args.write_snapshot:
        path = snapshot_path(commit)
        path.write_text("\n".join(upstream) + "\n")
        print(f"wrote {path.relative_to(ROOT)}")

    unclassified, stale = compare(upstream, rows)

    if not upstream:
        print("\nerror: the inventory is empty, so every comparison below would pass over nothing",
              file=sys.stderr)
        return 1

    rc = 0
    if unclassified:
        print(f"\nerror: upstream carries {len(unclassified)} test file(s) the manifest says "
              "nothing about:", file=sys.stderr)
        for p in unclassified:
            print(f"  + {p}", file=sys.stderr)
        print("  Each needs a status and a reason. That decision is the point of the manifest, "
              "so this tool will not make it.", file=sys.stderr)
        rc = 1
    if stale:
        print(f"\nerror: the manifest classifies {len(stale)} file(s) upstream no longer has:",
              file=sys.stderr)
        for p in stale:
            print(f"  - {p}", file=sys.stderr)
        print("  They were renamed or deleted upstream; the rows have to follow.", file=sys.stderr)
        rc = 1

    if rc == 0:
        print("\nin sync: every upstream test file has a row, every row has a file")
        report_coverage(rows)
    return rc


if __name__ == "__main__":
    raise SystemExit(main())
