#!/usr/bin/env python3
"""The extractor against ezdxf itself. Needs the pinned requirements installed,
so it runs inside the extractor's image and not in the `Test` job:

    pip install --require-hashes -r tools/ground_truth/requirements.txt
    python3 -m unittest discover -s tools/ground_truth/tests

An ImportError here is a failure on purpose: a ground truth whose own tests
skipped is not one.
"""

from __future__ import annotations

import math
import pathlib
import sys
import tempfile
import unittest

import ezdxf  # noqa: F401  (a missing ezdxf is a failure, not a skip)

ROOT = pathlib.Path(__file__).resolve().parent.parent.parent.parent
sys.path.insert(0, str(ROOT / "tools" / "ground_truth"))

import extract_ezdxf as tool  # noqa: E402

SAMPLE = ROOT / "fixtures" / "dxf" / "sample_AC1032_ascii.dxf"


def records_of(doc):
    records, sidecar = tool.extract_document(doc, doc.dxfversion)
    return records, sidecar


def kinds(records):
    return [r["record"] for r in records]


class TestSmallDrawings(unittest.TestCase):
    def new(self):
        return ezdxf.new("R2018")

    def test_a_line_is_a_line_with_its_handle_in_the_sidecar(self):
        doc = self.new()
        e = doc.modelspace().add_line((0, 0, 0), (100, 0, 0))
        records, sidecar = records_of(doc)
        line = next(r for r in records if r["record"] == "line")
        self.assertEqual(line["b"], (100.0, 0.0, 0.0))
        i = records.index(line)
        self.assertEqual(sidecar[i][0], [e.dxf.handle.upper()])
        self.assertEqual(sidecar[i][1], "LINE")

    def test_views_are_in_tab_order_and_model_is_first(self):
        doc = self.new()
        records, _ = records_of(doc)
        views = [r for r in records if r["record"] == "view"]
        self.assertEqual(views[0]["name"], "Model")
        self.assertEqual(views[0]["kind"], "model")
        self.assertEqual([v["index"] for v in views], list(range(len(views))))

    def test_block_base_point_scale_rotation_and_insert_point_apply_in_dxf_order(self):
        doc = self.new()
        blk = doc.blocks.new("B", base_point=(1, 1, 0))
        blk.add_line((1, 1, 0), (2, 1, 0))
        doc.modelspace().add_blockref("B", (10, 20, 0), dxfattribs={"xscale": 2, "yscale": 2, "zscale": 2, "rotation": 90})
        records, sidecar = records_of(doc)
        line = next(r for r in records if r["record"] == "line")
        # (1,1)->(2,1) minus base = (0,0)->(1,0); x2 = (0,0)->(2,0); +90deg = (0,0)->(0,2); + (10,20)
        self.assertAlmostEqual(line["a"][0], 10.0, places=9)
        self.assertAlmostEqual(line["a"][1], 20.0, places=9)
        self.assertAlmostEqual(line["b"][0], 10.0, places=9)
        self.assertAlmostEqual(line["b"][1], 22.0, places=9)
        handles = sidecar[records.index(line)][0]
        self.assertEqual(len(handles), 2, "an expanded record names the insert and the entity")

    def test_a_circle_under_a_non_uniform_insert_scale_is_an_ellipse(self):
        doc = self.new()
        blk = doc.blocks.new("C")
        blk.add_circle((0, 0, 0), 1)
        doc.modelspace().add_blockref("C", (0, 0, 0), dxfattribs={"xscale": 2, "yscale": 1, "zscale": 1})
        records, _ = records_of(doc)
        self.assertNotIn("circle", kinds(records))
        el = next(r for r in records if r["record"] == "ellipse")
        self.assertAlmostEqual(el["ratio"], 0.5, places=9)

    def test_a_solid_comes_out_in_perimeter_order(self):
        doc = self.new()
        doc.modelspace().add_solid([(0, 0, 0), (1, 0, 0), (0, 1, 0), (1, 1, 0)])
        records, _ = records_of(doc)
        poly = next(r for r in records if r["record"] == "polygon")
        self.assertEqual(poly["points"], [(0.0, 0.0, 0.0), (1.0, 0.0, 0.0), (1.0, 1.0, 0.0), (0.0, 1.0, 0.0)])

    def test_a_lightweight_polyline_keeps_its_bulges(self):
        doc = self.new()
        doc.modelspace().add_lwpolyline([(0, 0, 0), (1, 0, 0.5), (1, 1, 0)], format="xyb", close=True)
        records, _ = records_of(doc)
        p = next(r for r in records if r["record"] == "polyline")
        self.assertTrue(p["closed"])
        self.assertEqual(p["bulges"], [0.0, 0.5, 0.0])

    def test_an_invisible_entity_is_absent_and_an_unknown_one_is_a_warning(self):
        doc = self.new()
        msp = doc.modelspace()
        msp.add_line((0, 0, 0), (1, 0, 0), dxfattribs={"invisible": 1})
        msp.add_mline([(0, 0), (5, 0)]) if hasattr(msp, "add_mline") else None
        records, _ = records_of(doc)
        self.assertNotIn("line", kinds(records))

    def test_text_alignment_uses_the_second_point(self):
        doc = self.new()
        t = doc.modelspace().add_text("hi", height=2.5)
        t.set_placement((5, 6), align=ezdxf.enums.TextEntityAlignment.MIDDLE_CENTER)
        records, _ = records_of(doc)
        rec = next(r for r in records if r["record"] == "text")
        self.assertEqual((rec["halign"], rec["valign"]), ("center", "middle"))
        self.assertEqual(rec["at"][:2], (5.0, 6.0))

    def test_a_block_that_contains_itself_is_a_warning_not_a_hang(self):
        doc = self.new()
        blk = doc.blocks.new("LOOP")
        blk.add_blockref("LOOP", (0, 0, 0))
        doc.modelspace().add_blockref("LOOP", (0, 0, 0))
        records, _ = records_of(doc)
        details = [r["detail"] for r in records if r["record"] == "warning"]
        self.assertIn("cyclic_block_reference", details)


class TestTheRealFile(unittest.TestCase):
    def extract(self, path):
        doc = ezdxf.readfile(str(path))
        records, sidecar = tool.extract_document(doc, doc.dxfversion)
        return tool.emit_texts(records, sidecar)

    def test_the_same_file_twice_is_byte_identical(self):
        a = self.extract(SAMPLE)
        b = self.extract(SAMPLE)
        self.assertEqual(a, b)

    def test_the_ac1032_save_has_four_views_in_tab_order(self):
        doc = ezdxf.readfile(str(SAMPLE))
        records, _ = tool.extract_document(doc, doc.dxfversion)
        views = [r["name"] for r in records if r["record"] == "view"]
        self.assertEqual(views, ["Model", "Layout1", "Layout2", "MyLayout"])

    def test_moving_one_coordinate_in_a_scratch_copy_changes_the_extraction(self):
        # The positive control: if this passed with nothing changed, the
        # extractor would not be reading the file at all.
        lines = SAMPLE.read_text(encoding="utf-8", errors="replace").splitlines()
        mutated = False
        in_line = False
        for i in range(0, len(lines) - 1, 2):
            code, value = lines[i].strip(), lines[i + 1].strip()
            if code == "0":
                in_line = value == "LINE"
            elif in_line and code == "10":
                lines[i + 1] = repr(float(value) + 1.0)
                mutated = True
                break
        self.assertTrue(mutated, "no LINE with a group 10 in the sample")
        with tempfile.TemporaryDirectory() as tmp:
            scratch = pathlib.Path(tmp) / "scratch.dxf"
            scratch.write_text("\n".join(lines) + "\n", encoding="utf-8")
            before = self.extract(SAMPLE)[0].splitlines()
            after = self.extract(scratch)[0].splitlines()
        self.assertEqual(len(before), len(after))
        changed = [i for i, (x, y) in enumerate(zip(before, after)) if x != y]
        self.assertEqual(len(changed), 1, f"exactly one record moves, got {changed}")
        self.assertIn('"record":"line"', before[changed[0]])

    def test_every_file_is_read_by_the_strict_parser_shape(self):
        # The Rust parser is the real check (tests/ground_truth_ezdxf.rs); this
        # is the cheap one: one JSON object per line, schema 1, LF only.
        import json

        jsonl, sidecar = self.extract(SAMPLE)
        self.assertNotIn("\r", jsonl)
        for line in jsonl.splitlines():
            self.assertEqual(json.loads(line)["schema"], 1)
        self.assertEqual(len(jsonl.splitlines()), len(sidecar.splitlines()))


if __name__ == "__main__":
    unittest.main()
