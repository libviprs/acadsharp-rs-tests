#!/usr/bin/env python3
"""The parts of `tools/ground_truth/extract_ezdxf.py` that need no ezdxf.

Stdlib only, so the `Test` job's `python3 -m unittest discover -s tools/tests`
runs them. The extraction itself, which does need ezdxf, is tested in
`tools/ground_truth/tests/`, inside the pinned image.
"""

from __future__ import annotations

import math
import pathlib
import re
import sys
import unittest

ROOT = pathlib.Path(__file__).resolve().parent.parent.parent
sys.path.insert(0, str(ROOT / "tools" / "ground_truth"))

import extract_ezdxf as tool  # noqa: E402


class TestNumbers(unittest.TestCase):
    def test_a_whole_number_is_visibly_a_double(self):
        self.assertEqual(tool.real(100.0), "100.0")
        self.assertEqual(tool.real(1.0), "1.0")

    def test_negative_zero_is_zero(self):
        self.assertEqual(tool.real(-0.0), "0.0")

    def test_the_shortest_round_trip_digits_are_kept(self):
        for v in (0.1, 1.494404150136851, 5.684341886080802e-16, 1.5707963267948966):
            self.assertEqual(float(tool.real(v)), v)

    def test_an_exponent_has_no_plus_and_no_leading_zero(self):
        self.assertEqual(tool.real(1e-5), "1e-5")
        self.assertEqual(tool.real(1.5e16), "1.5e16")

    def test_non_finite_values_are_refused(self):
        for v in (math.nan, math.inf, -math.inf):
            with self.assertRaises(ValueError):
                tool.real(v)


class TestStrings(unittest.TestCase):
    def test_only_what_json_forbids_is_escaped(self):
        self.assertEqual(tool.quote('a"b\\c'), '"a\\"b\\\\c"')
        self.assertEqual(tool.quote("\n\t\x01"), '"\\n\\t\\u0001"')
        self.assertEqual(tool.quote("Ø45,6 é"), '"Ø45,6 é"')

    def test_line_endings_become_lf(self):
        self.assertEqual(tool.normalize_newlines("a\r\nb\rc\n"), "a\nb\nc\n")


class TestCaretNotation(unittest.TestCase):
    """AutoCAD's own DXF writes a line feed in an MTEXT as `^J`."""

    def test_caret_j_is_a_line_feed(self):
        self.assertEqual(tool.decode_caret("a^Jb"), "a\nb")

    def test_caret_space_is_a_caret(self):
        self.assertEqual(tool.decode_caret("2^ 3"), "2^3")

    def test_text_without_a_caret_is_untouched(self):
        self.assertEqual(tool.decode_caret("94\\U+00B0"), "94\\U+00B0")

    def test_a_trailing_caret_is_kept(self):
        self.assertEqual(tool.decode_caret("a^"), "a^")


class TestRender(unittest.TestCase):
    """The writer's output against the exact text `src/canonical.rs` writes."""

    def test_a_line(self):
        rec = {"record": "line", "view": 0, "a": (0.0, 0.0, 0.0), "b": (100.0, 0.0, 0.0)}
        self.assertEqual(
            tool.render(rec),
            '{"schema":1,"record":"line","view":0,"x1":0.0,"y1":0.0,"z1":0.0,"x2":100.0,"y2":0.0,"z2":0.0}',
        )

    def test_a_circle(self):
        rec = {"record": "circle", "view": 0, "center": (10.0, 20.0, 0.0), "radius": 5.0, "normal": (0.0, 0.0, 1.0)}
        self.assertEqual(
            tool.render(rec),
            '{"schema":1,"record":"circle","view":0,"cx":10.0,"cy":20.0,"cz":0.0,"radius":5.0,"nx":0.0,"ny":0.0,"nz":1.0}',
        )

    def test_a_polyline_carries_a_bulge_per_point(self):
        rec = {"record": "polyline", "view": 0, "closed": False, "normal": (0.0, 0.0, 1.0),
               "points": [(0.0, 0.0, 0.0), (1.0, 0.0, 0.0)], "bulges": [0.0, 0.5]}
        self.assertEqual(
            tool.render(rec),
            '{"schema":1,"record":"polyline","view":0,"closed":false,"nx":0.0,"ny":0.0,"nz":1.0,'
            '"points":[[0.0,0.0,0.0],[1.0,0.0,0.0]],"bulges":[0.0,0.5]}',
        )

    def test_a_warning_and_the_sidecar_line(self):
        rec = {"record": "warning", "category": "unsupported_entity", "entity_type": "MLINE", "detail": "no_canonical_mapping"}
        self.assertEqual(
            tool.render(rec),
            '{"schema":1,"record":"warning","category":"unsupported_entity","entity_type":"MLINE","detail":"no_canonical_mapping"}',
        )
        self.assertEqual(
            tool.render_sidecar_line(3, ["1A", "2B"], "line", "LINE"),
            '{"i":3,"h":["1A","2B"],"t":"line","d":"LINE"}',
        )


class TestGeometry(unittest.TestCase):
    def test_the_plus_z_normal_is_world_space(self):
        ax, ay, n = tool.arbitrary_axis((0.0, 0.0, 1.0))
        self.assertEqual((ax, ay, n), ((1.0, 0.0, 0.0), (0.0, 1.0, 0.0), (0.0, 0.0, 1.0)))

    def test_a_tilted_normal_gives_an_orthonormal_frame(self):
        ax, ay, n = tool.arbitrary_axis((1.0, 2.0, 3.0))
        for a, b in ((ax, ay), (ay, n), (ax, n)):
            self.assertAlmostEqual(tool.dot(a, b), 0.0, places=12)
        for v in (ax, ay, n):
            self.assertAlmostEqual(tool.length(v), 1.0, places=12)

    def test_an_untouched_circle_keeps_the_radius_the_file_stores(self):
        m = tool.map_curve((1.0, 2.0, 0.0), (7.25, 0.0, 0.0), (0.0, 7.25, 0.0), tool.Xf())
        self.assertTrue(m["circular"])
        self.assertEqual(m["radius"], 7.25)
        self.assertEqual(m["center"], (1.0, 2.0, 0.0))

    def test_a_non_uniform_scale_turns_a_circle_into_an_ellipse(self):
        xf = tool.Xf.scaling(2.0, 1.0, 1.0)
        m = tool.map_curve((0.0, 0.0, 0.0), (1.0, 0.0, 0.0), (0.0, 1.0, 0.0), xf)
        self.assertFalse(m["circular"])
        self.assertAlmostEqual(m["ratio"], 0.5, places=12)
        self.assertAlmostEqual(tool.length(m["major"]), 2.0, places=12)

    def test_a_30_degree_rotation_is_still_a_similarity(self):
        xf = tool.Xf.compose(tool.Xf.rotation_z(math.radians(30)), tool.Xf.scaling(3.0, 3.0, 3.0))
        self.assertTrue(xf.similarity)
        m = tool.map_curve((0.0, 0.0, 0.0), (1.0, 0.0, 0.0), (0.0, 1.0, 0.0), xf)
        self.assertTrue(m["circular"])

    def test_a_collapsed_curve_is_none(self):
        xf = tool.Xf.scaling(1.0, 0.0, 1.0)
        self.assertIsNone(tool.map_curve((0.0, 0.0, 0.0), (1.0, 0.0, 0.0), (0.0, 1.0, 0.0), xf))

    def test_sweep_always_ends_after_it_starts(self):
        s, e = tool.sweep(1.0, 0.5, 0.0)
        self.assertLess(s, e)
        self.assertGreaterEqual(s, 0.0)
        self.assertLess(s, 2 * math.pi)


class TestRequirements(unittest.TestCase):
    def test_every_requirement_is_pinned_by_hash(self):
        text = (ROOT / "tools" / "ground_truth" / "requirements.txt").read_text()
        body = "\n".join(l for l in text.splitlines() if not l.lstrip().startswith("#"))
        entries = re.split(r"\n(?=\S)", body.strip())
        self.assertGreaterEqual(len(entries), 5)
        for entry in entries:
            self.assertRegex(entry, r"^[A-Za-z0-9_.-]+==[0-9][^\s]*")
            self.assertIn("--hash=sha256:", entry, entry)

    def test_ezdxf_is_the_pinned_version(self):
        text = (ROOT / "tools" / "ground_truth" / "requirements.txt").read_text()
        self.assertIn(f"ezdxf=={tool.EZDXF_VERSION}", text)


class TestManifestRewrite(unittest.TestCase):
    ROW = {k: "x" for k in tool.MANIFEST_KEYS}
    ROW.update(id="r1", records=3, schema=1)

    def test_a_table_after_the_generated_block_survives_a_freeze(self):
        text = "[[fixture]]\nid = 'a'\n\n" + tool.render_rows([self.ROW]) + "\n[[later]]\nid = 'z'\n"
        out = tool.rewrite_manifest(text, [dict(self.ROW, id="r2")])
        self.assertIn("[[later]]", out)
        self.assertIn('id = "r2"', out)
        self.assertNotIn('id = "r1"', out)

    def test_a_manifest_with_no_block_gets_one_at_the_end(self):
        out = tool.rewrite_manifest("[[fixture]]\nid = 'a'\n", [self.ROW])
        self.assertTrue(out.index("[[fixture]]") < out.index("[[ground_truth]]"))


class TestVerify(unittest.TestCase):
    def test_the_committed_ground_truths_are_the_ones_the_manifest_froze(self):
        self.assertEqual(tool.command_verify(None), 0)

    def test_every_input_in_the_manifest_has_a_row(self):
        manifest = tool.load_manifest()
        have = {r["input"] for r in manifest["ground_truth"]}
        want = {r["id"] for r in manifest["oracle_input"]}
        self.assertEqual(have, want)


if __name__ == "__main__":
    unittest.main()
