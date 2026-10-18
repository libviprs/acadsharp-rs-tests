#!/usr/bin/env python3
"""Give every row of parity/VERDICTS_STAGE2.toml that is not a MATCH a reason and an issue.

`PARITY_REGEN=1 tools/run-tests.sh --stage-two DIR` rewrites the verdicts and leaves
the reason and issue of every row that moved blank, which the test then refuses. The
stage one rows were written by hand from what the comparator printed; stage two has
about 1,500, so the reasons are made from the same printout (`PARITY_EXPLAIN=1`) by
the rules below. Every rule names the issue that adjudicates what it describes. A
finding no rule claims is reported and gets issue #41, so nothing is silently
attributed.

    python3 tools/stage_two_reasons.py VERDICTS_STAGE2.toml explain.log > out.toml
"""
from __future__ import annotations

import re
import sys
from collections import Counter, defaultdict

R = "libviprs/acadsharp-rs-tests#"
DEP = "libviprs/libviprs-dep#"


def issue(*nums) -> list[str]:
    return [n if "/" in n else R + n.lstrip("#") for n in nums]


# (tag) -> (clause, issues)
CLAUSES = {
    "sharp_ac1014": (
        "ACadSharp's AC1014 DWG is damaged: the readers that open it find fewer records than the source has", issue("107")),
    "sharp_ac1015_tail": (
        "ACadSharp does not read the model space of its own AC1015 DWG past the first lost record, and acadrust does", issue("108")),
    "sharp_spline": ("the spline is missing from the DWG ACadSharp writes below AC1027", issue("108")),
    "sharp_table": ("ACadSharp's writer emits no ACAD_TABLE entity, so the table geometry is missing", issue("106")),
    "diameter": ("the diameter callout ∅45,6 reads back as 'Ø45,6', the character the code page of that version best-fits it to",
                 [DEP + "111", R + "30"]),
    "caret": ("the MTEXT line feed is the caret form ^J in the binary DXF, which acadrust returns literally", issue("109")),
    "stale": ("a divergence the reader carries on the source is gone (the multiline attribute's embedded MTEXT)", issue("114")),
    "style": ("AC1014 stores table names upper-cased, so a text style reads back as Standard", issue("24")),
    "shim_extra": ("the extra records are the ones the shim already emits for the source (R4.1)", issue("30")),
    "shim_pair": ("the shim's reading of this file does not line up with the oracle's once records are lost", issue("108")),
    "hatch": ("a hatch boundary spline reads back with z 1.0 where the source reading has 0.0, and ezdxf reads a boundary arc half a turn from the source reading",
              issue("159")),
    "rust_dwg": ("acadrust's DWG writer loses most of the drawing below AC1021", issue("112")),
    "r12": ("an AC1009 (R12) file has no place for this record, this text alignment or this text formatting, and the lowering is not on R3.4's list yet", issue("24")),
    "rust_text_p": ("the MTEXT paragraph break reads back as \\P, where the source reading has a line feed", issue("24")),
    "rust_moji": ("non-ASCII text is written as UTF-8 into a file whose code page is not, so it reads back as mojibake", [DEP + "111"]),
    "rust_rot": ("four MTEXTs read back with a rotation the source reading does not have", issue("113")),
    "rust_ocs": ("a lightweight polyline in an OCS plane reads back with another normal and elevation", issue("160")),
    "rust_table": ("the oracle's DXF reading of the ACAD_TABLE at handle 528 has other line ends, text positions and widths than every other reading, as it does on AutoCAD's own DXF twin of this save", issue("94")),
    "overflow": ("ezdxf refuses the DXF ACadSharp writes for a mesh with faces that point nowhere", issue("148")),
    "codepage": ("the code page text of g11_codepage reads back as replacement characters", issue("30")),
    "other": ("a difference no rule has attributed yet", issue("41")),
}


def classify(fx: str, pair: str, body: str) -> str:
    rust = fx.startswith("w-acadrust")
    dwg = "-dwg-" in fx
    ver = fx.rsplit("-", 1)[1]
    ins_a = " is in A and not in B" in body
    ins_b = " is in B and not in A" in body
    kind = ""
    m = re.search(r" ([a-z_]+) is in [AB] and not in [AB]", body)
    if m:
        kind = m.group(1)
    fld = re.search(r" ([a-z_]+\.[A-Za-z_.0-9\[\]]+): ", body)
    field = fld.group(1) if fld else ""
    if "KNOWN_DIVERGENCE] the listed divergence is gone" in body or "is in a different place" in body or "my multi line text for the attrrib" in body:
        return "stale"
    if "ÄÖÜ" in body and not rust:
        return "codepage"
    if rust:
        if dwg:
            return "rust_dwg"
        if ver == "ac1009" and (ins_a or ins_b or field in ("text.halign", "text.valign")):
            return "r12"
        if field == "text.text":
            if "âˆ" in body or "Ã" in body:
                return "rust_moji"
            if ver == "ac1009":
                return "r12"
            if "\\\\P" in body:
                return "rust_text_p"
            return "other"
        if field == "text.rotation":
            return "rust_rot"
        if field.startswith("polyline."):
            return "rust_ocs"
        if re.search(r"handles 528/", body) or field in ("line.x1", "line.x2", "text.width_factor", "text.x") or field.startswith("polygon.points"):
            return "rust_table"
        if field.startswith("spline.") or field.startswith("arc."):
            return "hatch"
        return "other"
    # ACadSharp
    if dwg:
        if ver == "ac1014":
            return "sharp_ac1014"
        if ins_b:
            return "shim_extra"
        if kind == "spline" and ins_a:
            return "sharp_spline"
        if ins_a and pair.endswith("acadsharp"):
            return "shim_pair"
        if ins_a and ver == "ac1015":
            return "sharp_ac1015_tail"
        if field == "text.style":
            return "style"
        if field == "text.text":
            return "diameter" if "∅" in body else "other"
        if ins_a:
            return "sharp_table"
        return "other"
    if field == "text.text":
        if "^J" in body:
            return "caret"
        return "diameter" if "∅" in body else "other"
    if field.startswith("spline.") or field.startswith("arc."):
        return "hatch"
    if ins_a:
        return "sharp_table"
    return "other"


def parse_explain(path):
    cells: dict[tuple[str, str], list[str]] = defaultdict(list)
    for line in open(path, encoding="utf-8", errors="replace"):
        if not line.startswith("EXPLAIN "):
            continue
        parts = line.rstrip("\n").split(" ", 3)
        if len(parts) < 4:
            continue
        cells[(parts[2], parts[1])].append(parts[3])
    return cells


def split_rows(text: str):
    head, *rows = re.split(r"(?m)^\[\[row\]\]\n", text)
    return head, rows


def field(row: str, key: str):
    m = re.search(rf"(?m)^{key} = (.*)$", row)
    return m.group(1) if m else None


def toml_str(s: str) -> str:
    return '"' + s.replace("\\", "\\\\").replace('"', '\\"') + '"'


# OPEN_FAILED reasons the run already wrote, and the issue each belongs to.
def failed_issue(fx: str, pair: str, reason: str) -> list[str]:
    if "two dictionary records are keyed" in reason:
        return issue("139", "129")
    if "does not read that drawing format" in reason:
        return issue("28")
    if "carriage return" in reason:
        return issue("112")
    if "ezdxf would not read it" in reason:
        if "OverflowError" in reason:
            return issue("148")
        return issue("110")
    if "-dwg-ac1014" in fx and fx.startswith("w-acadsharp"):
        return issue("107")
    if "DxfException" in reason:
        return issue("110")
    return issue("41")


def main() -> int:
    table_path, explain_path = sys.argv[1:3]
    text = open(table_path, encoding="utf-8").read()
    cells = parse_explain(explain_path)
    head, rows = split_rows(text)
    out = [head.rstrip("\n") + "\n"]
    unattributed = Counter()
    failed_by_cell: dict[tuple[str, str], list[str]] = {}
    for row in rows:
        fx = field(row, "fixture").strip('"')
        pair = field(row, "pair").strip('"')
        verdict = field(row, "verdict").strip('"')
        if verdict == "OPEN_FAILED" and not pair.endswith(":version"):
            failed_by_cell[(fx, pair)] = failed_issue(fx, pair, field(row, "reason") or "")
    for row in rows:
        fx = field(row, "fixture").strip('"')
        pair = field(row, "pair").strip('"')
        verdict = field(row, "verdict").strip('"')
        block = "[[row]]\nfixture = " + toml_str(fx) + "\npair = " + toml_str(pair) + "\nverdict = " + toml_str(verdict) + "\n"
        if verdict == "MATCH":
            out.append(block)
            continue
        first = field(row, "first")
        if pair.endswith(":version") and verdict.startswith("DIFF") and fx.endswith("-ac1009"):
            reason = ("acadrust wrote an AC1012 file when asked for AC1009 (its ASCII outputs for the two "
                      "versions are byte-identical), and the oracle reads it as AC1012")
            block += f"reason = {toml_str(reason)}\nissue = {toml_str(', '.join(issue('111')))}\n"
            if first:
                block += f"first = {first}\n"
            out.append(block)
            continue
        if verdict == "OPEN_FAILED":
            reason = (field(row, "reason") or '""')
            if pair.endswith(":version"):
                oracle = failed_by_cell.get((fx, pair.split(":")[0] + ":oracle"))
                iss = oracle or issue("41")
                block += f"reason = {reason}\nissue = {toml_str(', '.join(iss))}\n"
            else:
                iss = failed_by_cell[(fx, pair)]
                block += f"reason = {reason}\nissue = {toml_str(', '.join(iss))}\n"
            out.append(block)
            continue
        counts: Counter[str] = Counter()
        counts_n: Counter[str] = Counter()
        for body in cells.get((fx, pair), []):
            tag = classify(fx, pair, body)
            counts[tag] += 1
            if tag == "other":
                unattributed[(pair, body[:110])] += 1
        clauses = []
        issues: list[str] = []
        for tag, n in sorted(counts.items(), key=lambda kv: -kv[1]):
            text_, iss = CLAUSES[tag]
            clauses.append(f"{text_} ({n} finding{'s' if n != 1 else ''})")
            for i in iss:
                if i not in issues:
                    issues.append(i)
        if verdict.startswith("KNOWN"):
            reason = "every difference of the cell is listed as known (parity/TABLES.toml)"
            iss = issue("106")
            if "-dwg-" in fx:
                iss += issue("108")
            if "dxf-binary" in fx and pair.endswith("acadrust"):
                iss += issue("109")
        else:
            reason = "; ".join(clauses) if clauses else "differences the comparator lists in the run"
            iss = issues or issue("41")
        block += f"reason = {toml_str(reason)}\nissue = {toml_str(', '.join(iss))}\n"
        if first:
            block += f"first = {first}\n"
        out.append(block)
    sys.stdout.write("\n".join(out))
    if unattributed:
        print("UNATTRIBUTED", file=sys.stderr)
        for (p, b), n in unattributed.most_common(40):
            print(n, p, b, file=sys.stderr)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
