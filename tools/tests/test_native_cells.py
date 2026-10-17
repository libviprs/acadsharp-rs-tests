#!/usr/bin/env python3
"""The matrix planner and the shared-versus-static comparison.

    python3 -m unittest discover -s tools/tests -v
"""

from __future__ import annotations

import pathlib
import sys
import tempfile
import unittest

TOOLS = pathlib.Path(__file__).resolve().parent.parent
sys.path.insert(0, str(TOOLS))

import compare_cells as compare  # noqa: E402
import native_cells as cells  # noqa: E402

import tomllib  # noqa: E402


def manifest(**flags: bool) -> dict:
    rows = []
    for name in ("linux-x64", "linux-arm64", "musl-x64", "musl-arm64", "mac-arm64"):
        rows.append(
            {
                "name": name,
                "static_certified": flags.get(name.replace("-", "_"), True),
                "runner": "macos-15" if name.startswith("mac") else "ubuntu-latest",
                **({"container": "rust:1-alpine"} if name.startswith("musl") else {}),
            }
        )
    return {"target": rows}


class Planner(unittest.TestCase):
    def names(self, m: dict, **kw) -> list[str]:
        return [c["name"] for c in cells.cells(m, **kw)]

    def test_a_certified_target_gets_both_cells_and_an_uncertified_one_only_shared(self):
        names = self.names(manifest(mac_arm64=False), include_own=True)
        self.assertIn("Native (linux-arm64, static)", names)
        self.assertIn("Native (mac-arm64, shared)", names)
        self.assertNotIn("Native (mac-arm64, static)", names)

    def test_the_cells_ci_yml_runs_as_its_own_jobs_are_left_out_by_default(self):
        names = self.names(manifest())
        self.assertNotIn("Native (linux-x64, shared)", names)
        self.assertNotIn("Native (mac-arm64, shared)", names)
        # but their static sibling is not one of those jobs
        self.assertIn("Native (linux-x64, static)", names)

    def test_flipping_the_flag_removes_exactly_that_cell(self):
        with_static = set(self.names(manifest(), include_own=True))
        without = set(self.names(manifest(musl_x64=False), include_own=True))
        self.assertEqual(with_static - without, {"Native (musl-x64, static)"})

    def test_the_committed_manifest_plans_nine_cells_and_the_mac_one_is_shared_only(self):
        with open(cells.MANIFEST, "rb") as f:
            committed = tomllib.load(f)
        every = self.names(committed, include_own=True)
        self.assertEqual(len(every), 9, every)
        self.assertEqual(len(self.names(committed)), 7)
        self.assertNotIn("Native (mac-arm64, static)", every)

    def test_a_musl_cell_names_its_container(self):
        planned = {c["name"]: c for c in cells.cells(manifest(), include_own=True)}
        self.assertEqual(planned["Native (musl-arm64, static)"]["container"], "rust:1-alpine")
        self.assertEqual(planned["Native (linux-arm64, shared)"]["container"], "")


class Compare(unittest.TestCase):
    def setUp(self):
        self._tmp = tempfile.TemporaryDirectory()
        self.root = pathlib.Path(self._tmp.name)

    def tearDown(self):
        self._tmp.cleanup()

    def write(self, cell: str, name: str, data: bytes) -> None:
        d = self.root / cell
        d.mkdir(exist_ok=True)
        (d / name).write_bytes(data)

    def test_identical_cells_pass_and_say_how_many_files(self):
        for mode in ("shared", "static"):
            self.write(f"cell-linux-x64-{mode}", "a.acadsharp.jsonl", b"{}\n")
            self.write(f"cell-linux-x64-{mode}", "b.acadsharp.jsonl", b"{}\n{}\n")
        code, lines = compare.compare(self.root)
        self.assertEqual(code, 0, lines)
        self.assertIn("2 files byte-identical", " ".join(lines))

    def test_one_flipped_byte_fails_naming_the_file(self):
        self.write("cell-linux-x64-shared", "a.acadsharp.jsonl", b'{"x":1.0}\n')
        self.write("cell-linux-x64-static", "a.acadsharp.jsonl", b'{"x":1.1}\n')
        code, lines = compare.compare(self.root)
        self.assertEqual(code, 1)
        self.assertIn("a.acadsharp.jsonl", " ".join(lines))

    def test_a_missing_file_fails(self):
        self.write("cell-linux-x64-shared", "a.acadsharp.jsonl", b"{}\n")
        self.write("cell-linux-x64-shared", "b.acadsharp.jsonl", b"{}\n")
        self.write("cell-linux-x64-static", "a.acadsharp.jsonl", b"{}\n")
        code, _ = compare.compare(self.root)
        self.assertEqual(code, 1)

    def test_an_empty_input_is_refused_not_passed(self):
        code, _ = compare.compare(self.root)
        self.assertEqual(code, 1)

    def test_an_empty_cell_is_refused(self):
        (self.root / "cell-linux-x64-shared").mkdir()
        self.write("cell-linux-x64-static", "a.acadsharp.jsonl", b"{}\n")
        code, lines = compare.compare(self.root)
        self.assertEqual(code, 1, lines)

    def test_a_target_with_one_mode_only_is_not_a_pair_and_alone_is_vacuous(self):
        self.write("cell-mac-arm64-shared", "a.acadsharp.jsonl", b"{}\n")
        code, lines = compare.compare(self.root)
        self.assertEqual(code, 1, "nothing was compared, so this must not pass")
        self.write("cell-linux-x64-shared", "a.acadsharp.jsonl", b"{}\n")
        self.write("cell-linux-x64-static", "a.acadsharp.jsonl", b"{}\n")
        code, lines = compare.compare(self.root)
        self.assertEqual(code, 0, lines)

    def test_a_hyphenated_target_is_split_on_the_last_hyphen(self):
        self.write("cell-linux-arm64-shared", "a.acadsharp.jsonl", b"{}\n")
        self.write("cell-linux-arm64-static", "a.acadsharp.jsonl", b"{}\n")
        code, lines = compare.compare(self.root)
        self.assertEqual(code, 0, lines)
        self.assertIn("linux-arm64", " ".join(lines))


if __name__ == "__main__":
    unittest.main()
