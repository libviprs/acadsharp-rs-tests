#!/usr/bin/env python3
"""The corpus tables, per-source pins and the acadrust view (R2.2, #20).

Stdlib only:

    python3 -m unittest discover -s tools/tests

Every case builds a tiny corpus in a temporary directory and drives the same
functions `verify` and `view` call, so a check is shown to fail on a state built
to fail it. A verifier only ever run against the real, healthy corpus cannot be
shown to work.
"""

from __future__ import annotations

import copy
import hashlib
import pathlib
import sys
import tempfile
import unittest

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent.parent))

import fetch_acadsharp_fixtures as tool  # noqa: E402

ACS = "a" * 40
DEP = "b" * 40
ACS_PREFIX = "https://raw.githubusercontent.com/DomCR/ACadSharp/"
DEP_PREFIX = "https://raw.githubusercontent.com/libviprs/libviprs-dep/"


def sha(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def dwg(version: str, tag: str) -> bytes:
    return version.encode() + (tag.encode() * 2048)[:4096]


def row(file: str, data: bytes, source: str, **extra) -> dict:
    prefix, commit = (ACS_PREFIX, ACS) if source == "acadsharp" else (DEP_PREFIX, DEP)
    out = {
        "file": file,
        "source": source,
        "source_path": file,
        "source_url": f"{prefix}{commit}/{file}",
        "license": "MIT",
        "license_evidence": f"{prefix}{commit}/LICENSE",
        "bytes": len(data),
        "sha256": sha(data),
    }
    out.update(extra)
    return out


class Corpus:
    """A small but complete corpus: every table has a row, and the files exist."""

    def __init__(self, root: pathlib.Path):
        self.root = root
        self.files: dict[str, bytes] = {}
        a1032 = dwg("AC1032", "a")
        b_arc = dwg("AC1032", "b")
        c_big = dwg("AC1032", "c")
        real = dwg("AC1018", "r")
        nan = dwg("AC1032", "n")
        r1009 = dwg("AC1009", "o")
        self.manifest = {
            "upstream": {"commit": ACS},
            "sources": {
                "acadsharp": {"commit": ACS, "url_prefix": ACS_PREFIX},
                "libviprs_dep": {"commit": DEP, "url_prefix": DEP_PREFIX},
            },
            "fixture": [
                {"id": "acadsharp-ac1018", "acad_version": "AC1018", "tier": "A",
                 **row("dwg/sample_AC1018.dwg", real, "acadsharp")},
                {"id": "acadsharp-sample-base", "acad_version": "AC1032", "tier": "A",
                 **row("dwg/sample_base.dwg", a1032, "acadsharp")},
                {"id": "g13_arc", "acad_version": "AC1032", "tier": "B",
                 **row("libviprs-dep/dwg/g13_arc.dwg", b_arc, "libviprs_dep")},
                {"id": "g13_many_inserts", "acad_version": "AC1032", "tier": "C",
                 **row("libviprs-dep/dwg/g13_many_inserts.dwg", c_big, "libviprs_dep")},
            ],
            "refusal": [
                {"id": "g13_ac1009", "acad_version": "AC1009", "oracle_exit": "UnsupportedVersion",
                 "oracle_exit_code": 4, **row("libviprs-dep/dwg/g13_ac1009.dwg", r1009, "libviprs_dep")},
                {"id": "g13_nan_bulge", "acad_version": "AC1032", "oracle_exit": "CanonicalSerializationFailure",
                 "oracle_exit_code": 6, **row("libviprs-dep/dwg/g13_nan_bulge.dwg", nan, "libviprs_dep")},
            ],
            "recording": [
                {"id": "rec-g13_arc", "drawing": "g13_arc",
                 **row("libviprs-dep/recordings/g13_arc.txt", b"arc 1.000000\n", "libviprs_dep")},
                {"id": "rec-real_AC1018", "drawing": "acadsharp-ac1018",
                 **row("libviprs-dep/recordings/real_AC1018.txt", b"real 2.000000\n", "libviprs_dep")},
                {"id": "rec-g13_nan_bulge", "drawing": "g13_nan_bulge",
                 **row("libviprs-dep/recordings/g13_nan_bulge.txt", b"nan 0\n", "libviprs_dep")},
            ],
            "oracle_input": [
                {"id": "dxf-sample-ac1018-ascii", "for_fixture": "acadsharp-ac1018",
                 **row("dxf/sample_AC1018_ascii.dxf", b"  0\nSECTION\n", "acadsharp")},
            ],
            "census": [
                {"id": "census-sample-base-tree", "for_fixtures": ["acadsharp-sample-base"],
                 **row("census/sample_base_tree.json", b"{}\n", "acadsharp")},
            ],
        }
        for table in ("fixture", "refusal", "recording", "oracle_input", "census"):
            for r in self.manifest[table]:
                self.files[r["file"]] = self.bytes_of(r)
        self.write()

    def bytes_of(self, r: dict) -> bytes:
        """Reconstruct the bytes a row was built from (they are in the row's hash)."""
        for data in self._all_data():
            if sha(data) == r["sha256"]:
                return data
        raise AssertionError(r["file"])

    def _all_data(self):
        yield dwg("AC1032", "a")
        yield dwg("AC1032", "b")
        yield dwg("AC1032", "c")
        yield dwg("AC1018", "r")
        yield dwg("AC1032", "n")
        yield dwg("AC1009", "o")
        yield b"arc 1.000000\n"
        yield b"real 2.000000\n"
        yield b"nan 0\n"
        yield b"  0\nSECTION\n"
        yield b"{}\n"

    def write(self):
        for rel, data in self.files.items():
            p = self.root / "fixtures" / rel
            p.parent.mkdir(parents=True, exist_ok=True)
            p.write_bytes(data)


class WithCorpus(unittest.TestCase):
    def setUp(self):
        self._tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self._tmp.cleanup)
        self.root = pathlib.Path(self._tmp.name)
        self.corpus = Corpus(self.root)
        self.manifest = copy.deepcopy(self.corpus.manifest)

    def verify(self) -> list[str]:
        return tool.verify_tree(self.manifest, self.root)


class VerifyCoversEveryTable(WithCorpus):
    def test_a_healthy_corpus_verifies_and_the_walk_saw_every_row(self):
        self.assertEqual(self.verify(), [])
        # The positive control: a verifier that walked no rows is green too.
        self.assertEqual(len(tool.rows(self.manifest)), 4 + 2 + 3 + 1 + 1)

    def test_deleting_any_one_file_is_red_in_every_table(self):
        for table in ("fixture", "refusal", "recording", "oracle_input", "census"):
            with self.subTest(table=table):
                self.setUp()
                victim = self.manifest[table][0]["file"]
                (self.root / "fixtures" / victim).unlink()
                problems = self.verify()
                self.assertTrue(any(victim in p for p in problems), problems)

    def test_a_changed_byte_is_red_in_every_table(self):
        for table in ("fixture", "refusal", "recording", "oracle_input", "census"):
            with self.subTest(table=table):
                self.setUp()
                victim = self.manifest[table][0]["file"]
                path = self.root / "fixtures" / victim
                data = bytearray(path.read_bytes())
                data[-1] ^= 0xFF
                path.write_bytes(bytes(data))
                problems = self.verify()
                self.assertTrue(any(victim in p for p in problems), problems)

    def test_an_unlisted_file_under_any_directory_is_red(self):
        for rel in ("dwg/extra.dwg", "dxf/extra.dxf", "census/extra.json",
                    "libviprs-dep/recordings/extra.txt", "libviprs-dep/dwg/extra.dwg",
                    "somewhere/else/extra.bin"):
            with self.subTest(rel=rel):
                self.setUp()
                p = self.root / "fixtures" / rel
                p.parent.mkdir(parents=True, exist_ok=True)
                p.write_bytes(b"nobody vetted this")
                problems = self.verify()
                self.assertTrue(any(rel in p and "manifest" in p for p in problems), problems)

    def test_the_documents_and_the_expected_directory_are_not_orphans(self):
        (self.root / "fixtures" / "README.md").write_text("docs")
        (self.root / "fixtures" / "LICENSE-ACadSharp.txt").write_text("MIT")
        (self.root / "fixtures" / "manifest.toml").write_text("")
        (self.root / "fixtures" / "expected").mkdir()
        (self.root / "fixtures" / "expected" / "SUMMARY.md").write_text("x")
        self.assertEqual(self.verify(), [])

    def test_the_same_bytes_under_two_rows_is_red(self):
        dup = copy.deepcopy(self.manifest["recording"][0])
        dup["id"] = "rec-dup"
        dup["file"] = "libviprs-dep/recordings/dup.txt"
        self.manifest["recording"].append(dup)
        (self.root / "fixtures" / dup["file"]).write_bytes(b"arc 1.000000\n")
        problems = self.verify()
        self.assertTrue(any("same sha256" in p for p in problems), problems)

    def test_a_drawing_signature_that_disagrees_with_the_row_is_red(self):
        self.manifest["fixture"][1]["acad_version"] = "AC1027"
        self.assertTrue(any("first six bytes" in p for p in self.verify()))


class CrossReferences(WithCorpus):
    def test_a_recording_must_name_a_fixture_or_a_refusal_that_exists(self):
        self.manifest["recording"][0]["drawing"] = "no-such-drawing"
        problems = self.verify()
        self.assertTrue(any("no-such-drawing" in p for p in problems), problems)

    def test_a_dxf_twin_and_a_census_must_name_a_fixture_that_exists(self):
        self.manifest["oracle_input"][0]["for_fixture"] = "ghost"
        self.manifest["census"][0]["for_fixtures"] = ["ghost2"]
        problems = " ".join(self.verify())
        self.assertIn("ghost", problems)
        self.assertIn("ghost2", problems)

    def test_every_fixture_has_exactly_one_known_tier(self):
        self.manifest["fixture"][0]["tier"] = "D"
        del self.manifest["fixture"][1]["tier"]
        problems = " ".join(self.verify())
        self.assertIn("tier", problems)
        self.assertIn("acadsharp-ac1018", problems)
        self.assertIn("acadsharp-sample-base", problems)

    def test_a_refusal_must_say_which_exit_code_it_expects(self):
        del self.manifest["refusal"][0]["oracle_exit_code"]
        self.assertTrue(any("oracle_exit_code" in p for p in self.verify()))

    def test_the_two_shared_drawings_are_one_row_each(self):
        # A libviprs-dep row for bytes the corpus already holds is a second copy.
        dup = copy.deepcopy(self.manifest["fixture"][0])
        dup["id"] = "real_AC1018"
        dup["file"] = "libviprs-dep/dwg/real_AC1018.dwg"
        dup["source"] = "libviprs_dep"
        self.manifest["fixture"].append(dup)
        (self.root / "fixtures" / dup["file"]).write_bytes(
            (self.root / "fixtures" / "dwg/sample_AC1018.dwg").read_bytes()
        )
        self.assertTrue(any("same sha256" in p for p in self.verify()))


class PerSourcePins(WithCorpus):
    def pins(self):
        return tool.check_pin(self.manifest)

    def test_a_healthy_manifest_is_pinned(self):
        self.assertEqual(self.pins(), [])

    def test_a_row_cannot_borrow_another_sources_host(self):
        r = self.manifest["fixture"][2]  # libviprs_dep
        r["source_url"] = f"{ACS_PREFIX}{ACS}/x.dwg"
        problems = self.pins()
        self.assertTrue(any(r["id"] in p and "allowlisted prefix" in p for p in problems), problems)

    def test_a_row_cannot_borrow_another_sources_commit(self):
        r = self.manifest["fixture"][2]
        r["source_url"] = f"{DEP_PREFIX}{ACS}/x.dwg"
        self.assertTrue(any(r["id"] in p and "commit" in p for p in self.pins()), self.pins())

    def test_every_table_is_checked_not_only_fixtures(self):
        for table in ("refusal", "recording", "oracle_input", "census"):
            with self.subTest(table=table):
                self.setUp()
                r = self.manifest[table][0]
                r["source_url"] = "https://example.com/" + r["source_path"]
                self.assertTrue(any(r["id"] in p for p in self.pins()), self.pins())

    def test_an_unknown_source_is_refused(self):
        self.manifest["fixture"][0]["source"] = "somewhere_else"
        self.assertTrue(any("somewhere_else" in p for p in self.pins()))

    def test_a_row_with_no_source_is_refused(self):
        del self.manifest["fixture"][0]["source"]
        self.assertTrue(any("no source" in p for p in self.pins()))

    def test_a_source_commit_must_be_a_full_sha(self):
        self.manifest["sources"]["libviprs_dep"]["commit"] = "bc351a9"
        self.assertTrue(any("libviprs_dep" in p and "40" in p for p in self.pins()))

    def test_a_floating_ref_is_refused(self):
        r = self.manifest["recording"][0]
        r["source_url"] = f"{DEP_PREFIX}{DEP}/x".replace(DEP, "main")
        self.assertTrue(any(r["id"] in p for p in self.pins()), self.pins())


class TheAcadrustView(WithCorpus):
    def view(self, source="libviprs_dep") -> tuple[list[str], pathlib.Path]:
        dest = self.root / "view"
        return tool.build_view(self.manifest, self.root, dest, source), dest

    def test_the_layout_is_fixtures_stem_dwg_beside_expectations_stem_txt(self):
        problems, dest = self.view()
        self.assertEqual(problems, [])
        fixtures = sorted(p.name for p in (dest / "fixtures").iterdir())
        expectations = sorted(p.name for p in (dest / "expectations").iterdir())
        self.assertEqual(
            fixtures,
            ["g13_ac1009.dwg", "g13_arc.dwg", "g13_many_inserts.dwg", "g13_nan_bulge.dwg", "real_AC1018.dwg"],
        )
        self.assertEqual(expectations, ["g13_arc.txt", "g13_nan_bulge.txt", "real_AC1018.txt"])

    def test_a_recording_of_a_shared_drawing_uses_the_recordings_own_spelling(self):
        _, dest = self.view()
        link = dest / "fixtures" / "real_AC1018.dwg"
        self.assertTrue(link.is_symlink())
        self.assertEqual(
            link.resolve(), (self.root / "fixtures" / "dwg" / "sample_AC1018.dwg").resolve()
        )
        self.assertEqual(
            (dest / "expectations" / "real_AC1018.txt").resolve(),
            (self.root / "fixtures" / "libviprs-dep/recordings/real_AC1018.txt").resolve(),
        )

    def test_nothing_is_copied_twice(self):
        _, dest = self.view()
        for sub in ("fixtures", "expectations"):
            for p in (dest / sub).iterdir():
                self.assertTrue(p.is_symlink(), p)

    def test_the_other_source_gives_the_other_tier(self):
        problems, dest = self.view("acadsharp")
        self.assertEqual(problems, [])
        self.assertEqual(
            sorted(p.name for p in (dest / "fixtures").iterdir()),
            ["sample_AC1018.dwg", "sample_base.dwg"],
        )

    def test_a_view_refuses_a_destination_that_is_not_empty(self):
        dest = self.root / "view"
        dest.mkdir()
        (dest / "something").write_text("x")
        problems = tool.build_view(self.manifest, self.root, dest, "libviprs_dep")
        self.assertTrue(any("not empty" in p for p in problems), problems)

    def test_an_unknown_source_is_refused(self):
        problems, _ = self.view("nobody")
        self.assertTrue(any("nobody" in p for p in problems), problems)

    def test_a_view_of_a_corpus_that_fails_verification_is_refused(self):
        (self.root / "fixtures" / "libviprs-dep/dwg/g13_arc.dwg").unlink()
        problems, dest = self.view()
        self.assertTrue(problems)
        self.assertFalse(dest.exists() and any(dest.iterdir()))


if __name__ == "__main__":
    unittest.main()
