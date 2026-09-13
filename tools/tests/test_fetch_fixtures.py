#!/usr/bin/env python3
"""The refusals `fetch_acadsharp_fixtures.py` has to make.

Stdlib only, so CI runs it with the `python3` that is already there:

    python3 -m unittest discover -s tools/tests

Every hostile case is driven through a real local HTTP server rather than a
mocked `urlopen`, because the interesting behaviour lives in how `urllib`
follows redirects and how much of a response body gets read. A mock would be
asserting against my own idea of what `urllib` does.
"""

from __future__ import annotations

import http.server
import pathlib
import sys
import threading
import unittest

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent.parent))

import fetch_acadsharp_fixtures as tool  # noqa: E402

# A DWG the manifest could plausibly describe: right signature, over the
# minimum size, and hashed below so the "good" case is genuinely consistent.
GOOD = b"AC1018" + b"\x00" * 4096


def sha(data: bytes) -> str:
    import hashlib

    return hashlib.sha256(data).hexdigest()


class Handler(http.server.BaseHTTPRequestHandler):
    """Serves the four shapes the tool has to survive."""

    def do_GET(self):  # noqa: N802  (the stdlib spells it this way)
        if self.path == "/good":
            body = GOOD
        elif self.path == "/big":
            body = b"AC1018" + b"\x00" * 200_000
        elif self.path == "/wrong-bytes":
            body = b"AC1018" + b"\xff" * 4096
        elif self.path == "/not-a-dwg":
            body = b"<!DOCTYPE html><title>404</title>" + b" " * 5000
        elif self.path == "/offsite":
            # `localhost` and `127.0.0.1` both resolve here, and they are
            # different hostnames, which is exactly what the allowlist compares.
            self.send_response(302)
            self.send_header("Location", f"http://localhost:{self.server.server_port}/good")
            self.end_headers()
            return
        else:
            self.send_error(404)
            return
        self.send_response(200)
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def log_message(self, *args):
        pass  # the test output is the interesting thing, not the access log


class StubServer:
    def __enter__(self):
        self.httpd = http.server.HTTPServer(("127.0.0.1", 0), Handler)
        self.port = self.httpd.server_port
        self.thread = threading.Thread(target=self.httpd.serve_forever, daemon=True)
        self.thread.start()
        return self

    def url(self, path: str) -> str:
        return f"http://127.0.0.1:{self.port}{path}"

    def __exit__(self, *exc):
        self.httpd.shutdown()
        self.httpd.server_close()
        self.thread.join(timeout=5)


def fixture(url: str, *, data: bytes = GOOD, version: str = "AC1018") -> dict:
    return {
        "id": "stub-fixture",
        "file": "dwg/stub.dwg",
        "acad_version": version,
        "source_url": url,
        "bytes": len(data),
        "sha256": sha(data),
    }


class TestDownloadRefusals(unittest.TestCase):
    def test_a_redirect_to_another_host_is_refused(self):
        with StubServer() as s:
            with self.assertRaises(RuntimeError) as cm:
                tool.download(s.url("/offsite"), allowed_host="127.0.0.1", max_bytes=1 << 20)
        self.assertIn("unexpected host", str(cm.exception))
        self.assertIn("localhost", str(cm.exception))

    def test_a_body_over_the_cap_is_refused(self):
        with StubServer() as s:
            with self.assertRaises(RuntimeError) as cm:
                tool.download(s.url("/big"), allowed_host="127.0.0.1", max_bytes=8192)
        self.assertIn("cap", str(cm.exception))

    def test_a_body_under_the_cap_comes_back_whole(self):
        # The positive control. Without it, a `download` that raised on
        # everything would pass both refusal tests above.
        with StubServer() as s:
            got = tool.download(s.url("/good"), allowed_host="127.0.0.1", max_bytes=1 << 20)
        self.assertEqual(got, GOOD)


class TestFetchWritesOnlyWhatVerifies(unittest.TestCase):
    """A bad download must never land on disk, whatever is wrong with it."""

    def setUp(self):
        import tempfile

        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.dest = pathlib.Path(self.tmp.name) / "dwg" / "stub.dwg"

    def fetch(self, path: str, fx: dict) -> list[str]:
        with StubServer() as s:
            fx = dict(fx, source_url=s.url(path))
            return tool.fetch_one(fx, self.dest, allowed_host="127.0.0.1", max_bytes=1 << 20)

    def test_bytes_that_fail_the_hash_are_not_written(self):
        problems = self.fetch("/wrong-bytes", fixture(""))
        self.assertTrue(problems)
        self.assertIn("sha256", " ".join(problems))
        self.assertFalse(self.dest.exists(), "a fixture that failed verification was written")

    def test_something_that_is_not_a_dwg_is_not_written(self):
        problems = self.fetch("/not-a-dwg", fixture(""))
        self.assertTrue(problems)
        self.assertIn("not a DWG", " ".join(problems))
        self.assertFalse(self.dest.exists())

    def test_an_off_host_redirect_is_not_written(self):
        problems = self.fetch("/offsite", fixture(""))
        self.assertTrue(problems)
        self.assertIn("download failed", " ".join(problems))
        self.assertFalse(self.dest.exists())

    def test_a_fixture_that_matches_is_written(self):
        problems = self.fetch("/good", fixture(""))
        self.assertEqual(problems, [])
        self.assertEqual(self.dest.read_bytes(), GOOD)


class TestVerifyBytes(unittest.TestCase):
    def test_the_matching_bytes_produce_no_problems(self):
        self.assertEqual(tool.verify_bytes(fixture(""), GOOD), [])

    def test_the_wrong_generation_is_caught(self):
        fx = fixture("", version="AC1032")
        self.assertIn("not a DWG", " ".join(tool.verify_bytes(fx, GOOD)))

    def test_a_size_that_disagrees_with_the_manifest_is_caught(self):
        fx = dict(fixture(""), bytes=999_999)
        self.assertIn("manifest says", " ".join(tool.verify_bytes(fx, GOOD)))

    def test_a_flipped_byte_is_caught(self):
        damaged = bytearray(GOOD)
        damaged[100] ^= 0xFF
        self.assertIn("sha256", " ".join(tool.verify_bytes(fixture(""), bytes(damaged))))

    def test_the_tool_never_offers_to_update_the_manifest(self):
        # The refusal is load-bearing, so it is asserted rather than left to
        # whoever next edits the message.
        damaged = bytearray(GOOD)
        damaged[100] ^= 0xFF
        message = " ".join(tool.verify_bytes(fixture(""), bytes(damaged)))
        self.assertIn("will not update the manifest", message)


class TestPinRefusals(unittest.TestCase):
    """A manifest that has stopped being pinned is refused before any fetch."""

    SHA = "d7dc111023477d8a9fffc2153139459c95b4f345"

    def manifest(self, *, commit: str | None = None, url: str | None = None) -> dict:
        commit = self.SHA if commit is None else commit
        url = url if url is not None else (
            f"https://raw.githubusercontent.com/DomCR/ACadSharp/{self.SHA}/samples/sample_AC1018.dwg"
        )
        return {"upstream": {"commit": commit}, "fixture": [dict(fixture(url), id="ac1018")]}

    def test_a_well_formed_manifest_passes(self):
        self.assertEqual(tool.check_pin(self.manifest()), [])

    def test_a_branch_name_is_not_a_pin(self):
        self.assertIn("not a 40-character", " ".join(tool.check_pin(self.manifest(commit="master"))))

    def test_an_abbreviated_sha_is_not_a_pin(self):
        self.assertIn("not a 40-character", " ".join(tool.check_pin(self.manifest(commit="d7dc111"))))

    def test_a_floating_source_url_is_refused(self):
        url = "https://raw.githubusercontent.com/DomCR/ACadSharp/master/samples/sample_AC1018.dwg"
        problems = " ".join(tool.check_pin(self.manifest(url=url)))
        self.assertIn("floating ref", problems)

    def test_a_url_on_another_host_is_refused(self):
        url = f"https://example.invalid/DomCR/ACadSharp/{self.SHA}/samples/sample_AC1018.dwg"
        self.assertIn("allowlisted prefix", " ".join(tool.check_pin(self.manifest(url=url))))

    def test_a_url_pinned_to_a_different_commit_is_refused(self):
        other = "0" * 40
        url = f"https://raw.githubusercontent.com/DomCR/ACadSharp/{other}/samples/sample_AC1018.dwg"
        self.assertIn("does not carry upstream.commit", " ".join(tool.check_pin(self.manifest(url=url))))


class TestTheRealManifest(unittest.TestCase):
    def test_the_committed_manifest_is_pinned(self):
        # Ties the refusals above to the file they are meant to protect. If the
        # manifest's shape ever changes, this fails rather than the bad-case
        # tests quietly passing against a structure nothing uses any more.
        manifest = tool.load_manifest()
        self.assertEqual(tool.check_pin(manifest), [])
        self.assertTrue(manifest["fixture"], "the manifest lists no fixtures")


if __name__ == "__main__":
    unittest.main()
