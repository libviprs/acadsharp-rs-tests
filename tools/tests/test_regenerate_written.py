#!/usr/bin/env python3
"""The round-trip matrix tool (R5.3): the plan, the ids, the manifest block."""

from __future__ import annotations

import pathlib
import sys
import unittest

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent.parent))

import regenerate_reference as ref  # noqa: E402
import regenerate_written as tool  # noqa: E402


class Plan(unittest.TestCase):
    def test_the_plan_is_forty_eight_outputs_with_distinct_ids(self):
        rows = tool.outputs(tool.load_plan())
        self.assertEqual(len(rows), 22 + 26)
        self.assertEqual(len({r["id"] for r in rows}), len(rows))

    def test_an_id_names_writer_source_container_and_version(self):
        self.assertEqual(
            tool.output_id("acadsharp", "acadsharp-ac1032", "dxf-binary", "AC1015"),
            "w-acadsharp-ac1032-dxf-binary-ac1015",
        )

    def test_a_writer_that_refuses_a_version_is_not_planned_for_it(self):
        ids = {r["id"] for r in tool.outputs(tool.load_plan())}
        self.assertNotIn("w-acadsharp-ac1032-dwg-ac1012", ids)
        self.assertNotIn("w-acadrust-ac1032-dwg-ac1009", ids)


class ManifestBlock(unittest.TestCase):
    ROW = {"id": "w-x", "writer": "acadsharp", "source": "s", "container": "dwg", "version": "AC1015",
           "bytes": 3, "sha256": "ab", "generated_by": "g", "writer_version": "v"}

    def test_the_block_is_replaced_and_nothing_around_it_is_touched(self):
        text = "head\n\n" + tool.render_rows([self.ROW]) + "\ntail = 1\n"
        out = tool.rewrite_manifest(text, [dict(self.ROW, version="AC1018")])
        self.assertIn('version = "AC1018"', out)
        self.assertNotIn('version = "AC1015"', out)
        self.assertTrue(out.startswith("head"))
        self.assertTrue(out.rstrip().endswith("tail = 1"))

    def test_rewriting_twice_changes_nothing(self):
        once = tool.rewrite_manifest("head\n", [self.ROW])
        self.assertEqual(tool.rewrite_manifest(once, [self.ROW]), once)

    def test_integers_are_bare_and_strings_quoted(self):
        out = tool.render_rows([self.ROW])
        self.assertIn("bytes = 3\n", out)
        self.assertIn('sha256 = "ab"\n', out)


class TheReferenceToolLeavesWrittenRowsAlone(unittest.TestCase):
    def test_a_written_row_keeps_the_reference_keys_the_other_tool_would_drop(self):
        text = (
            '[[fixture]]\nid = "one"\nsha256 = "aaaa"\n\n'
            '[[written]]\nid = "w-one"\nsha256 = "bbbb"\nreference_jsonl = "written/w-one.zst"\n'
            'reference_jsonl_uncompressed_sha256 = "cc"\n'
        )
        values = {k: ("1" if k in ref.INT_KEYS else "x") for k in ref.REFERENCE_KEYS}
        out = ref.rewrite_manifest(text, {"one": values})
        self.assertIn('reference_jsonl = "written/w-one.zst"', out)
        self.assertIn('reference_jsonl_uncompressed_sha256 = "cc"', out)


class VerifyOnTheCommittedTree(unittest.TestCase):
    def test_verify_passes(self):
        import argparse

        self.assertEqual(tool.command_verify(argparse.Namespace()), 0)


if __name__ == "__main__":
    unittest.main()
