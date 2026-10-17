#!/usr/bin/env python3
"""The refusals `fetch_native.py` has to make, against a real local server.

    python3 -m unittest discover -s tools/tests -v

Same approach as `test_fetch_fixtures.py`: a stub HTTP server and no mocked
`urlopen`, because what matters is that a wrong hash leaves nothing extracted.
"""

from __future__ import annotations

import hashlib
import http.server
import io
import pathlib
import subprocess
import sys
import tarfile
import tempfile
import threading
import unittest

TOOLS = pathlib.Path(__file__).resolve().parent.parent
TOOL = TOOLS / "fetch_native.py"
sys.path.insert(0, str(TOOLS))

import fetch_native as tool  # noqa: E402


def make_archive(top: str = "acadsharp-linux-x64", linkinfo: bool = True) -> bytes:
    buf = io.BytesIO()
    with tarfile.open(fileobj=buf, mode="w:gz") as tar:
        files = {f"{top}/lib/libacadsharp_native.so": b"\x7fELF-not-really"}
        if linkinfo:
            files[f"{top}/metadata/LINKINFO.json"] = b'{"schema_version": 1}'
        for name, data in files.items():
            info = tarfile.TarInfo(name)
            info.size = len(data)
            tar.addfile(info, io.BytesIO(data))
    return buf.getvalue()


def make_evil_archive() -> bytes:
    buf = io.BytesIO()
    with tarfile.open(fileobj=buf, mode="w:gz") as tar:
        info = tarfile.TarInfo("../escaped.txt")
        info.size = 1
        tar.addfile(info, io.BytesIO(b"x"))
    return buf.getvalue()


GOOD = make_archive()
BAD_BODY = make_archive() + b"tampered"
NO_LINKINFO = make_archive(linkinfo=False)
EVIL = make_evil_archive()


def sha(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


class Handler(http.server.BaseHTTPRequestHandler):
    def do_GET(self):  # noqa: N802
        bodies = {
            "/acadsharp-test.1/acadsharp-linux-x64.tgz": GOOD,
            "/acadsharp-test.1/acadsharp-musl-x64.tgz": BAD_BODY,
            "/acadsharp-test.1/acadsharp-linux-arm64.tgz": NO_LINKINFO,
            "/acadsharp-test.1/acadsharp-mac-arm64.tgz": EVIL,
        }
        self.server.hits.append(self.path)
        body = bodies.get(self.path)
        if body is None:
            self.send_error(404)
            return
        self.send_response(200)
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def log_message(self, *_args):
        pass


def manifest_text(musl_hash: str | None = None) -> str:
    return f"""
release_tag = "acadsharp-test.1"

[[target]]
name = "linux-x64"
asset = "acadsharp-linux-x64.tgz"
sha256 = "{sha(GOOD)}"

[[target]]
name = "musl-x64"
asset = "acadsharp-musl-x64.tgz"
sha256 = "{musl_hash or sha(make_archive('acadsharp-musl-x64'))}"

[[target]]
name = "linux-arm64"
asset = "acadsharp-linux-arm64.tgz"
sha256 = "{sha(NO_LINKINFO)}"

[[target]]
name = "mac-arm64"
asset = "acadsharp-mac-arm64.tgz"
sha256 = "{sha(EVIL)}"
"""


class FetchNative(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        cls.server.hits = []
        cls.thread = threading.Thread(target=cls.server.serve_forever, daemon=True)
        cls.thread.start()
        cls.base = f"http://127.0.0.1:{cls.server.server_port}"

    @classmethod
    def tearDownClass(cls):
        cls.server.shutdown()
        cls.server.server_close()

    def setUp(self):
        self._tmp = tempfile.TemporaryDirectory()
        self.tmp = pathlib.Path(self._tmp.name)
        self.cache = self.tmp / "cache"
        self.manifest = self.tmp / "NATIVE_ARTIFACT.toml"
        self.manifest.write_text(manifest_text())
        self.server.hits.clear()

    def tearDown(self):
        self._tmp.cleanup()

    def run_tool(self, target: str):
        return subprocess.run(
            [sys.executable, str(TOOL), "--target", target, "--manifest", str(self.manifest),
             "--cache", str(self.cache), "--base-url", self.base],
            capture_output=True, text=True,
        )

    def extracted_anything(self) -> bool:
        return any(p.is_dir() for p in self.cache.rglob("*") if p.name == "metadata")

    def test_a_good_archive_is_extracted_and_its_directory_printed(self):
        r = self.run_tool("linux-x64")
        self.assertEqual(r.returncode, 0, r.stderr)
        out = pathlib.Path(r.stdout.strip())
        self.assertEqual(len(r.stdout.strip().splitlines()), 1, "stdout carries one line and nothing else")
        self.assertTrue((out / "metadata" / "LINKINFO.json").is_file())
        self.assertTrue((out / "lib" / "libacadsharp_native.so").is_file())

    def test_a_second_run_uses_the_cache_and_does_not_download(self):
        self.assertEqual(self.run_tool("linux-x64").returncode, 0)
        hits = len(self.server.hits)
        again = self.run_tool("linux-x64")
        self.assertEqual(again.returncode, 0, again.stderr)
        self.assertEqual(len(self.server.hits), hits)

    def test_a_wrong_hash_deletes_the_download_and_extracts_nothing(self):
        # The control for this test is `test_a_good_archive...`: the server,
        # the tool and the cache are the same, and only the hash differs.
        self.manifest.write_text(manifest_text(musl_hash="0" * 64))
        r = self.run_tool("musl-x64")
        self.assertEqual(r.returncode, 1, r.stderr)
        self.assertEqual(r.stdout, "")
        self.assertIn("sha256 mismatch", r.stderr)
        leftovers = [p.name for p in self.cache.rglob("*") if p.is_file()]
        self.assertEqual(leftovers, [], "the downloaded bytes must be deleted")
        self.assertFalse(self.extracted_anything())

    def test_a_cached_file_that_no_longer_matches_is_not_trusted(self):
        self.assertEqual(self.run_tool("linux-x64").returncode, 0)
        archive = next(self.cache.rglob("acadsharp-linux-x64.tgz"))
        archive.write_bytes(GOOD + b"\x00")
        r = self.run_tool("linux-x64")
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertEqual(sha(archive.read_bytes()), sha(GOOD), "the bad cached copy was replaced")

    def test_an_archive_without_linkinfo_is_refused(self):
        r = self.run_tool("linux-arm64")
        self.assertEqual(r.returncode, 1, r.stderr)
        self.assertIn("LINKINFO", r.stderr)
        self.assertFalse(self.extracted_anything())

    def test_an_archive_that_tries_to_leave_the_directory_is_refused(self):
        r = self.run_tool("mac-arm64")
        self.assertEqual(r.returncode, 1, r.stderr)
        self.assertFalse((self.tmp / "escaped.txt").exists())
        self.assertFalse((self.cache / "escaped.txt").exists())

    def test_an_unknown_target_is_refused_before_any_request(self):
        r = self.run_tool("plan9-mips")
        self.assertEqual(r.returncode, 1)
        self.assertEqual(self.server.hits, [])

    def test_a_missing_download_is_refused(self):
        self.manifest.write_text(manifest_text().replace("acadsharp-test.1", "acadsharp-test.404"))
        r = self.run_tool("linux-x64")
        self.assertEqual(r.returncode, 1)
        self.assertEqual(r.stdout, "")


class Manifest(unittest.TestCase):
    def test_the_committed_manifest_passes_the_shape_rules(self):
        targets = tool.check_manifest(tool.load_manifest(tool.MANIFEST))
        self.assertEqual(
            sorted(targets), ["linux-arm64", "linux-x64", "mac-arm64", "musl-arm64", "musl-x64"]
        )

    def test_a_short_hash_is_refused(self):
        with self.assertRaises(tool.Refused):
            tool.check_manifest({
                "release_tag": "acadsharp-x.1",
                "target": [{"name": "linux-x64", "asset": "acadsharp-linux-x64.tgz", "sha256": "ab" * 31}],
            })

    def test_an_asset_that_breaks_the_naming_rule_is_refused(self):
        with self.assertRaises(tool.Refused):
            tool.check_manifest({
                "release_tag": "acadsharp-x.1",
                "target": [{"name": "linux-x64", "asset": "libacadsharp.tar.gz", "sha256": "a" * 64}],
            })


if __name__ == "__main__":
    unittest.main()
