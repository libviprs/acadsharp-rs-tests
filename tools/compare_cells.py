#!/usr/bin/env python3
"""Hold each target's shared and static cells to byte-identical output.

    python3 tools/compare_cells.py <dir>

`<dir>` holds one directory per cell, named `cell-<target>-<mode>`, each with the
canonical JSONL that cell's `fixture_parity` run wrote. For every target that has
both a `shared` and a `static` directory the file sets must be the same and every
file byte-for-byte equal. A static link and a shared link of one archive are the
same code, so a difference is a finding about the link and not about the data,
and it is never absorbed by a tolerance.

A target with only one of the two is fine (mac-arm64 has no static cell, because
its archive is not certified). A target with neither is not in the directory at
all, and an empty directory is refused: a comparison of nothing with nothing is
the vacuous pass this script exists to avoid.

Exit codes: 0 equal, 1 a difference or an empty input, 2 called wrong.
"""

from __future__ import annotations

import pathlib
import sys


def cell_dirs(root: pathlib.Path) -> dict[str, dict[str, pathlib.Path]]:
    targets: dict[str, dict[str, pathlib.Path]] = {}
    for d in sorted(root.iterdir()):
        if not d.is_dir() or not d.name.startswith("cell-"):
            continue
        body = d.name[len("cell-"):]
        target, _, mode = body.rpartition("-")
        if mode not in ("shared", "static") or not target:
            continue
        targets.setdefault(target, {})[mode] = d
    return targets


def compare(root: pathlib.Path) -> tuple[int, list[str]]:
    lines: list[str] = []
    targets = cell_dirs(root)
    if not targets:
        return 1, [f"no cell-<target>-<mode> directories under {root}, so there is nothing to compare"]
    pairs = 0
    bad = 0
    for target, modes in sorted(targets.items()):
        for mode, d in sorted(modes.items()):
            if not any(d.glob("*.jsonl")):
                lines.append(f"{target}/{mode}: {d.name} holds no .jsonl, an empty cell proves nothing")
                bad += 1
        if "shared" not in modes or "static" not in modes:
            lines.append(f"{target}: only {sorted(modes)} ran, nothing to compare it with")
            continue
        shared = {p.name: p for p in modes["shared"].glob("*.jsonl")}
        static = {p.name: p for p in modes["static"].glob("*.jsonl")}
        if set(shared) != set(static):
            only_a = sorted(set(shared) - set(static))[:3]
            only_b = sorted(set(static) - set(shared))[:3]
            lines.append(f"{target}: the cells wrote different files (shared only {only_a}, static only {only_b})")
            bad += 1
            continue
        differing = [n for n in sorted(shared) if shared[n].read_bytes() != static[n].read_bytes()]
        pairs += 1
        if differing:
            lines.append(f"{target}: {len(differing)} of {len(shared)} files differ between shared and static, first {differing[0]}")
            bad += 1
        else:
            lines.append(f"{target}: {len(shared)} files byte-identical between shared and static")
    if pairs == 0 and bad == 0:
        lines.append("no target had both a shared and a static cell, so no pair was compared")
        bad += 1
    return (1 if bad else 0), lines


def main(argv: list[str]) -> int:
    if len(argv) != 1:
        print("usage: compare_cells.py <dir>", file=sys.stderr)
        return 2
    root = pathlib.Path(argv[0])
    if not root.is_dir():
        print(f"compare_cells: {root} is not a directory", file=sys.stderr)
        return 2
    code, lines = compare(root)
    for line in lines:
        print(f"compare_cells: {line}", file=sys.stderr if code else sys.stdout)
    return code


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
