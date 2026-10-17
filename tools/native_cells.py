#!/usr/bin/env python3
"""Plan the native lane's matrix from NATIVE_ARTIFACT.toml, with no download.

    python3 tools/native_cells.py          # the cells the matrix job runs, as JSON
    python3 tools/native_cells.py --all    # every cell, the two H3.2 jobs included

Every target gets a `shared` cell. A target also gets a `static` cell exactly
when its row says `static_certified = true`, and a target that says `false` gets
no static cell at all, not a skipped one: a skipped cell is a green check that
tested nothing. The flag is the producer's measurement (it linked the static
archive into a probe program and ran it on that target), copied into the
manifest and checked against the archive's own `LINKINFO.json` by
`tools/fetch_native.py` on every fetch, so planning from the manifest cannot
drift from the archives.

The two cells `ci.yml` already runs as their own jobs (linux-x64 shared and
mac-arm64 shared, which carry the no-.NET proof) are left out of the default
output so no cell runs twice under one name. Output is the shape a workflow's
`fromJSON` wants: `{"include": [{target, mode, runner, container, name}, ...]}`.
"""

from __future__ import annotations

import argparse
import json
import pathlib
import sys
import tomllib

ROOT = pathlib.Path(__file__).resolve().parent.parent
MANIFEST = ROOT / "NATIVE_ARTIFACT.toml"

# Run by their own jobs in ci.yml, with the proofs that only they carry.
OWN_JOBS = {("linux-x64", "shared"), ("mac-arm64", "shared")}


def cells(manifest: dict, include_own: bool = False) -> list[dict]:
    out = []
    for row in manifest["target"]:
        modes = ["shared"] + (["static"] if row["static_certified"] else [])
        for mode in modes:
            if not include_own and (row["name"], mode) in OWN_JOBS:
                continue
            out.append(
                {
                    "target": row["name"],
                    "mode": mode,
                    "runner": row["runner"],
                    "container": row.get("container", ""),
                    "name": f"Native ({row['name']}, {mode})",
                    "artifact": f"cell-{row['name']}-{mode}",
                }
            )
    return out


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--manifest", default=str(MANIFEST))
    parser.add_argument("--all", action="store_true", help="include the cells ci.yml runs as their own jobs")
    args = parser.parse_args(argv)
    with open(args.manifest, "rb") as f:
        manifest = tomllib.load(f)
    print(json.dumps({"include": cells(manifest, include_own=args.all)}, indent=None))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
