#!/usr/bin/env python3
"""What `regenerate_reference.py` has to get right without a .NET SDK present.

Stdlib only, and no toolchain:

    python3 -m unittest discover -s tools/tests

Two kinds of thing are checked here. The manifest rewriter, because it edits a
file that is the single source of truth for the whole suite and a reformatting
bug in it would turn a one-hash diff into an unreviewable one. And the stream
checks, because they are the ones that decide whether a frozen reference is
worth comparing against, and every one of them is paired with the input that
makes it fire.
"""

from __future__ import annotations

import pathlib
import sys
import unittest

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent.parent))

import regenerate_reference as tool  # noqa: E402

MANIFEST = """\
# A comment that must survive.

[upstream]
commit = "deadbeef"

[[fixture]]
id = "one"
file = "dwg/one.dwg"
acad_version = "AC1032"
bytes = 10
sha256 = "aaaa"

[[fixture]]
id = "two"
file = "dwg/two.dwg"
acad_version = "AC1027"
bytes = 20
sha256 = "bbbb"
"""


def reference_values(fixture_id: str) -> dict[str, object]:
    return {
        "reference_jsonl": f"expected/{fixture_id}.reference.jsonl.zst",
        "reference_jsonl_uncompressed_sha256": "1" * 64,
        "reference_svg": f"expected/{fixture_id}.reference.svg",
        "reference_svg_sha256": "2" * 64,
        "acadsharp_reference_version": "3.7.1",
        "acadsharp_reference_commit": "deadbeef",
        "dotnet_sdk": "10.0.401",
        "reference_schema": 1,
        "svg_schema": 1,
    }


class ManifestRewriting(unittest.TestCase):
    def test_adds_every_reference_key_after_the_input_hash(self):
        out = tool.rewrite_manifest(
            MANIFEST, {"one": reference_values("one"), "two": reference_values("two")}
        )
        for key in tool.REFERENCE_KEYS:
            self.assertEqual(
                out.count(f"{key} = "), 2, f"{key} should appear once per fixture"
            )
        # Placed after sha256 rather than appended at the end of the file, so a
        # row's keys stay with the row.
        self.assertIn('sha256 = "aaaa"\nreference_jsonl = ', out)

    def test_leaves_everything_it_does_not_own_alone(self):
        out = tool.rewrite_manifest(MANIFEST, {"one": reference_values("one")})
        self.assertIn("# A comment that must survive.", out)
        self.assertIn('commit = "deadbeef"', out)
        self.assertIn('id = "two"', out)
        self.assertIn('sha256 = "bbbb"', out)
        # A fixture with nothing to write gets nothing written.
        self.assertEqual(out.count("reference_jsonl = "), 1)

    def test_rewriting_twice_changes_nothing(self):
        values = {"one": reference_values("one"), "two": reference_values("two")}
        once = tool.rewrite_manifest(MANIFEST, values)
        twice = tool.rewrite_manifest(once, values)
        self.assertEqual(once, twice)

    def test_an_existing_value_is_replaced_and_not_duplicated(self):
        values = {"one": reference_values("one")}
        once = tool.rewrite_manifest(MANIFEST, values)
        changed = dict(values["one"])
        changed["reference_jsonl_uncompressed_sha256"] = "9" * 64
        twice = tool.rewrite_manifest(once, {"one": changed})
        self.assertEqual(twice.count("reference_jsonl_uncompressed_sha256 = "), 1)
        self.assertIn("9" * 64, twice)
        self.assertNotIn("1" * 64, twice)

    def test_integers_are_written_bare_and_strings_quoted(self):
        out = tool.rewrite_manifest(MANIFEST, {"one": reference_values("one")})
        self.assertIn("reference_schema = 1\n", out)
        self.assertIn("svg_schema = 1\n", out)
        self.assertIn('acadsharp_reference_version = "3.7.1"\n', out)

    def test_the_result_still_parses_as_toml(self):
        import tomllib

        out = tool.rewrite_manifest(
            MANIFEST, {"one": reference_values("one"), "two": reference_values("two")}
        )
        parsed = tomllib.loads(out)
        self.assertEqual(len(parsed["fixture"]), 2)
        self.assertEqual(parsed["fixture"][0]["reference_schema"], 1)
        self.assertEqual(parsed["fixture"][1]["acad_version"], "AC1027")


class Compression(unittest.TestCase):
    def test_round_trips(self):
        data = b'{"schema":1,"record":"document"}\n' * 500
        self.assertEqual(tool.decompress(tool.compress(data)), data)

    def test_is_stable_for_this_library_version(self):
        # Not a cross-implementation claim, which is exactly why the manifest
        # pins the uncompressed hash instead. This only says the tool does not
        # produce a different file every time it runs.
        data = b'{"schema":1,"record":"line"}\n' * 500
        self.assertEqual(tool.compress(data), tool.compress(data))

    def test_worker_threads_are_pinned_off(self):
        # Any value above zero lets libzstd split the input across threads, and
        # the frame then depends on how many cores the machine had.
        from compression import zstd

        self.assertEqual(tool.ZSTD_PARAMETERS[zstd.CompressionParameter.nb_workers], 0)
        self.assertEqual(tool.ZSTD_PARAMETERS[zstd.CompressionParameter.checksum_flag], 0)


class StreamChecks(unittest.TestCase):
    """Every refusal, with the stream that makes it fire."""

    FIXTURE = {"acad_version": "AC1032"}

    GOOD = (
        '{"schema":1,"record":"document","format":"dwg","acad_version":"AC1032"}\n'
        '{"schema":1,"record":"view","index":0,"name":"Model","kind":"model"}\n'
        '{"schema":1,"record":"line","view":0,"x1":0.0}\n'
        '{"schema":1,"record":"warning","category":"unknown"}\n'
    )

    def check(self, text: str) -> list[str]:
        return tool.check_stream("fx", self.FIXTURE, text.encode("utf-8"))

    def test_a_healthy_stream_has_no_problems(self):
        # The positive control. Without it every assertion below would pass on a
        # checker that returned a problem for absolutely anything.
        self.assertEqual(self.check(self.GOOD), [])

    def test_a_missing_final_newline_is_caught(self):
        problems = self.check(self.GOOD.rstrip("\n"))
        self.assertTrue(any("final newline" in p for p in problems), problems)

    def test_a_carriage_return_is_caught(self):
        problems = self.check(self.GOOD.replace("\n", "\r\n"))
        self.assertTrue(any("carriage return" in p for p in problems), problems)

    def test_a_stream_that_does_not_open_with_document_is_caught(self):
        lines = self.GOOD.splitlines(keepends=True)
        problems = self.check("".join(lines[1:] + lines[:1]))
        self.assertTrue(any("not 'document'" in p for p in problems), problems)

    def test_a_version_that_disagrees_with_the_manifest_is_caught(self):
        problems = self.check(self.GOOD.replace("AC1032", "AC1027", 1))
        self.assertTrue(any("AC1027" in p for p in problems), problems)

    def test_a_wrong_schema_version_is_caught(self):
        problems = self.check(self.GOOD.replace('"schema":1', '"schema":2'))
        self.assertTrue(any("schema" in p for p in problems), problems)

    def test_a_stream_with_no_view_is_caught(self):
        problems = self.check(
            "".join(l for l in self.GOOD.splitlines(keepends=True) if '"view"' not in l)
        )
        self.assertTrue(any("no view record" in p for p in problems), problems)

    def test_a_stream_with_no_geometry_is_caught(self):
        # The one that matters most. A differential over a stream with no
        # geometry compares two empty sets and passes.
        problems = self.check(
            "".join(l for l in self.GOOD.splitlines(keepends=True) if '"line"' not in l)
        )
        self.assertTrue(any("no primitive records" in p for p in problems), problems)

    def test_a_line_that_is_not_json_is_caught(self):
        problems = self.check(self.GOOD + "not json at all\n")
        self.assertTrue(any("is not JSON" in p for p in problems), problems)


class Reporting(unittest.TestCase):
    def test_the_census_counts_by_record_name(self):
        counts = tool.record_census(StreamChecks.GOOD.encode("utf-8"))
        self.assertEqual(counts, {"document": 1, "line": 1, "view": 1, "warning": 1})
        self.assertEqual(list(counts), sorted(counts), "the census must be ordered")

    def test_the_first_differences_name_the_line_and_both_sides(self):
        old = b"a\nb\nc\n"
        new = b"a\nB\nc\n"
        differences = tool.first_differences(old, new, 10)
        self.assertEqual(len(differences), 1)
        self.assertIn("line 2", differences[0])
        self.assertIn("- b", differences[0])
        self.assertIn("+ B", differences[0])

    def test_the_difference_limit_is_respected(self):
        old = b"\n".join(b"%d" % i for i in range(50)) + b"\n"
        new = b"\n".join(b"x%d" % i for i in range(50)) + b"\n"
        self.assertEqual(len(tool.first_differences(old, new, 3)), 3)

    def test_a_truncated_file_is_reported_rather_than_ignored(self):
        differences = tool.first_differences(b"a\nb\n", b"a\n", 10)
        self.assertTrue(any("end of file" in d for d in differences), differences)

    def test_the_summary_is_a_function_of_the_entries_alone(self):
        entries = [
            {"id": "one", "acad_version": "AC1032", "counts": {"line": 3, "view": 1}},
            {"id": "two", "acad_version": "AC1027", "counts": {"arc": 2, "view": 1}},
        ]
        first = tool.write_summary(entries)
        self.assertEqual(first, tool.write_summary(entries))
        # Columns are the union of every fixture's record names, ordered, so a
        # fixture that gains a record type does not shuffle the table.
        self.assertIn("| fixture | version | records | arc | line | view |", first)
        self.assertIn("| one | AC1032 | 4 | 0 | 3 | 1 |", first)
        self.assertIn("| two | AC1027 | 3 | 2 | 0 | 1 |", first)


class ToolInvariants(unittest.TestCase):
    def test_the_reference_keys_and_the_written_block_agree(self):
        # The two copies that would otherwise drift: the key list and the
        # renderer that writes them.
        rendered = tool.render_reference_block(reference_values("one"))
        self.assertEqual(len(rendered), len(tool.REFERENCE_KEYS))
        for line, key in zip(rendered, tool.REFERENCE_KEYS, strict=True):
            self.assertTrue(line.startswith(f"{key} = "), line)

    def test_the_pinned_image_is_not_a_floating_tag(self):
        self.assertNotIn(":latest", tool.DEFAULT_IMAGE)
        self.assertRegex(tool.DEFAULT_IMAGE, r":\d+\.\d+\.\d+")

    def test_the_repository_it_points_at_is_this_one(self):
        self.assertTrue(tool.MANIFEST.is_file(), tool.MANIFEST)
        self.assertTrue(tool.PROJECT_FILE.is_file(), tool.PROJECT_FILE)
        self.assertTrue(tool.GLOBAL_JSON.is_file(), tool.GLOBAL_JSON)

    def test_the_locked_acadsharp_version_is_readable(self):
        self.assertEqual(tool.locked_acadsharp_version(), "3.7.1")

    def test_the_pinned_sdk_is_readable(self):
        self.assertRegex(tool.pinned_sdk_version(), r"^\d+\.\d+\.\d+$")

    def test_verify_passes_on_the_committed_tree(self):
        # The end to end check, offline. If this ever needs a toolchain, the
        # verify command has stopped being the thing it is for.
        self.assertEqual(tool.command_verify(argparse_namespace()), 0)


def argparse_namespace():
    import argparse

    return argparse.Namespace()


if __name__ == "__main__":
    unittest.main()
