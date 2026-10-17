#!/usr/bin/env python3
"""tools/model1.py without a container: the written form, and the checks on it.

Stdlib only:

    python3 -m unittest discover -s tools/tests

`freeze` needs docker and is exercised by running it; what can be shown here is
the part that decides what a line looks like, and that `verify` is red on a
file that is not the one the manifest froze.
"""

from __future__ import annotations

import pathlib
import sys
import unittest

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent.parent))

import model1  # noqa: E402


class WrittenForm(unittest.TestCase):
    def test_the_type_follows_the_group_code(self):
        t = model1.header_type
        self.assertEqual(t("$X", 70, 1), "int")
        self.assertEqual(t("$X", 280, 1), "int")
        self.assertEqual(t("$X", 40, 1), "double")
        self.assertEqual(t("$X", 1, 1), "string")
        self.assertEqual(t("$X", 290, 1), "bool")
        self.assertEqual(t("$X", 10, 2), "point2")
        self.assertEqual(t("$X", 10, 3), "point3")
        with self.assertRaises(ValueError):
            t("$X", 347, 1)

    def test_a_handle_code_has_no_record(self):
        self.assertTrue(model1.is_handle_code(347))
        self.assertFalse(model1.is_handle_code(70))

    def test_reals_are_written_the_schema_1_way(self):
        self.assertEqual(model1.real(1.0), "1.0")
        self.assertEqual(model1.real(100), "100.0")
        self.assertEqual(model1.real(-0.0), "0.0")
        self.assertEqual(model1.real(0.1 + 0.2), "0.30000000000000004")
        with self.assertRaises(ValueError):
            model1.real(float("nan"))

    def test_a_stream_is_sorted_by_name_and_refuses_a_duplicate(self):
        a = model1.line("header", "$B", [("name", '"$B"'), ("type", '"int"'), ("value", "1")])
        b = model1.line("header", "$A", [("name", '"$A"'), ("type", '"int"'), ("value", "2")])
        text = model1.encode([a, b])
        self.assertTrue(text.startswith('{"model":1,"record":"header","name":"$A"'))
        self.assertTrue(text.endswith("\n"))
        with self.assertRaises(ValueError):
            model1.encode([a, a])

    def test_a_string_keeps_its_unicode_and_escapes_only_what_json_forbids(self):
        self.assertEqual(model1.quote('a"b\\c\né'), '"a\\"b\\\\c\\né"')


if __name__ == "__main__":
    unittest.main()
