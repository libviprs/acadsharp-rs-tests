#!/usr/bin/env python3
"""The comparisons `parity_inventory.py` has to get right.

Stdlib only:

    python3 -m unittest discover -s tools/tests

The tool's whole job is to notice a difference between upstream and the
manifest, so these drive it from both sides: a file upstream with no row, a row
with no file, and the positive control that an in-sync pair reports nothing. The
network paths are covered through a local stub rather than the real API, because
a test that needs GitHub to be up is a test that fails for reasons unrelated to
the code.
"""

from __future__ import annotations

import http.server
import io
import json
import pathlib
import sys
import threading
import unittest
import unittest.mock

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent.parent))

import parity_inventory as tool  # noqa: E402

SHA = "d7dc111023477d8a9fffc2153139459c95b4f345"


def rows(*specs) -> list[dict]:
    return [{"path": p, "status": s} for p, s in specs]


class TestCompare(unittest.TestCase):
    def test_an_in_sync_pair_reports_nothing(self):
        # The positive control. A `compare` that reported everything as
        # unclassified would pass both negative cases below.
        unclassified, stale = tool.compare(
            ["Entities/ArcTests.cs", "IO/IOTests.cs"],
            rows(("Entities/ArcTests.cs", "differential"), ("IO/IOTests.cs", "differential")),
        )
        self.assertEqual((unclassified, stale), ([], []))

    def test_a_file_upstream_with_no_row_is_unclassified(self):
        unclassified, stale = tool.compare(
            ["Entities/ArcTests.cs", "Entities/NewTests.cs"],
            rows(("Entities/ArcTests.cs", "differential")),
        )
        self.assertEqual(unclassified, ["Entities/NewTests.cs"])
        self.assertEqual(stale, [])

    def test_a_row_with_no_file_upstream_is_stale(self):
        unclassified, stale = tool.compare(
            ["Entities/ArcTests.cs"],
            rows(("Entities/ArcTests.cs", "differential"), ("Entities/GoneTests.cs", "differential")),
        )
        self.assertEqual(unclassified, [])
        self.assertEqual(stale, ["Entities/GoneTests.cs"])

    def test_a_rename_shows_as_both(self):
        # The shape a bump actually produces, and the reason both directions are
        # reported rather than just the new file: a rename that only reported
        # the addition would leave the old row behind forever.
        unclassified, stale = tool.compare(
            ["Tables/Collections/LayersTableTests.cs"],
            rows(("Tables/LayersTableTests.cs", "native_conformance")),
        )
        self.assertEqual(unclassified, ["Tables/Collections/LayersTableTests.cs"])
        self.assertEqual(stale, ["Tables/LayersTableTests.cs"])


class TestCoverageAccounting(unittest.TestCase):
    def capture(self, r: list[dict]) -> str:
        buf = io.StringIO()
        with unittest.mock.patch("sys.stdout", buf):
            tool.report_coverage(r)
        return buf.getvalue()

    def test_not_applicable_is_excluded_from_applicable(self):
        out = self.capture(rows(
            ("a.cs", "mirrored"),
            ("b.cs", "differential"),
            ("c.cs", "native_conformance"),
            ("d.cs", "not_applicable"),
            ("e.cs", "not_applicable"),
        ))
        self.assertIn("total                   5", out)
        self.assertIn("applicable              3", out)
        self.assertIn("1/3", out)

    def test_not_applicable_is_not_in_the_covered_set(self):
        # The rule stated as a rule, so relaxing it has to be deliberate.
        self.assertNotIn("not_applicable", tool.COVERED)
        self.assertIn("not_applicable", tool.STATUSES)

    def test_a_status_outside_the_four_is_refused(self):
        with self.assertRaises(SystemExit) as cm:
            self.capture(rows(("a.cs", "probably_fine")))
        self.assertIn("probably_fine", str(cm.exception))


class TestDirectoryInventory(unittest.TestCase):
    def setUp(self):
        import tempfile

        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = pathlib.Path(self.tmp.name)
        base = self.root / "src/ACadSharp.Tests"
        (base / "Entities").mkdir(parents=True)
        (base / "Entities/ArcTests.cs").write_text("// arc")
        (base / "IOTests.cs").write_text("// io")
        (base / "notes.md").write_text("not a test")

    def test_it_finds_cs_files_recursively_and_nothing_else(self):
        got = tool.from_dir(self.root, "src/ACadSharp.Tests")
        self.assertEqual(got, ["Entities/ArcTests.cs", "IOTests.cs"])

    def test_a_missing_directory_is_refused_rather_than_read_as_empty(self):
        # An empty result and a wrong path look identical otherwise, and the
        # empty one would report all 112 rows as stale.
        with self.assertRaises(SystemExit):
            tool.from_dir(self.root, "src/NoSuchProject")


class TreeHandler(http.server.BaseHTTPRequestHandler):
    TRUNCATED = False
    PATHS = [
        "src/ACadSharp.Tests/Entities/ArcTests.cs",
        "src/ACadSharp.Tests/Entities/NewTests.cs",
        "src/ACadSharp.Tests/Data/sample_AC1032_tree.json",
        "samples/sample_AC1032.dwg",
        "src/ACadSharp/Entities/Arc.cs",
    ]

    def do_GET(self):  # noqa: N802
        body = json.dumps({
            "truncated": type(self).TRUNCATED,
            "tree": [{"path": p, "type": "blob"} for p in type(self).PATHS],
        }).encode()
        self.send_response(200)
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def log_message(self, *args):
        pass


class TestApiInventory(unittest.TestCase):
    def setUp(self):
        TreeHandler.TRUNCATED = False
        self.httpd = http.server.HTTPServer(("127.0.0.1", 0), TreeHandler)
        self.thread = threading.Thread(target=self.httpd.serve_forever, daemon=True)
        self.thread.start()
        self.addCleanup(self.stop)
        self.patch = unittest.mock.patch.object(
            tool, "API", f"http://127.0.0.1:{self.httpd.server_port}/{{sha}}"
        )
        self.patch.start()
        self.addCleanup(self.patch.stop)

    def stop(self):
        self.httpd.shutdown()
        self.httpd.server_close()
        self.thread.join(timeout=5)

    def test_it_keeps_only_test_sources_and_strips_the_prefix(self):
        got = tool.from_api(SHA, "src/ACadSharp.Tests")
        self.assertEqual(got, ["Entities/ArcTests.cs", "Entities/NewTests.cs"])

    def test_a_truncated_tree_is_refused(self):
        # GitHub truncates large trees silently. Accepting one would report the
        # missing files as deleted upstream, which is a very convincing lie.
        TreeHandler.TRUNCATED = True
        with self.assertRaises(SystemExit) as cm:
            tool.from_api(SHA, "src/ACadSharp.Tests")
        self.assertIn("truncated", str(cm.exception))


class TestTheRealManifest(unittest.TestCase):
    def test_the_committed_manifest_matches_its_committed_snapshot(self):
        # Ties every check above to the files they protect. If the manifest's
        # shape changes, this fails rather than the synthetic cases quietly
        # passing against a structure nothing uses any more.
        manifest = tool.load_manifest()
        commit = manifest["upstream"]["commit"]
        upstream = tool.from_snapshot(commit)
        self.assertIsNotNone(upstream, f"no committed snapshot for {commit}")
        unclassified, stale = tool.compare(upstream, manifest["test_file"])
        self.assertEqual(unclassified, [])
        self.assertEqual(stale, [])
        self.assertEqual(len(manifest["test_file"]), 112)


if __name__ == "__main__":
    unittest.main()
