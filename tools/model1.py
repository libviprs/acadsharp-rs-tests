#!/usr/bin/env python3
"""The frozen ground truths for the `model-1` stream (docs/MODEL_1.md, R5.5).

Two producers sit behind this file, kept apart on purpose, and neither of them
runs on the machine that runs this script: both run in pinned containers.

    oracle   ACadSharp (JIT .NET) over the seven AutoCAD saves and over the six
             DXF twins, through `ACadSharp.Reference --model1`.
    ezdxf    ezdxf over the six ASCII DXF twins AutoCAD wrote beside them.

    tools/model1.py verify                 offline, stdlib only: every committed file is
                                           the one fixtures/model1/MANIFEST.toml froze
    tools/model1.py freeze                 run both producers, report, write nothing
    tools/model1.py freeze --accept        write fixtures/model1/ and its manifest

`freeze` never touches a committed file without `--accept`, for the reason
`tools/regenerate_reference.py` gives: a moved ground truth has to arrive as a
reviewed diff. It drives `docker`, so on this suite's machines it runs where the
docker host is (tools/nas.sh), and the inputs go in and the outputs come out as
tar streams so nothing depends on a bind mount resolving on the right host.

`extract-ezdxf` is the half that needs ezdxf. It runs inside the pinned image
and is not meant to be called by hand.

What is ezdxf's and what is mine. The DXF reading, and the group code each
header variable is stored under, are ezdxf's. The written form of a value (one
per type, chosen by the group code) is the document's, and is written here from
docs/MODEL_1.md, not copied from either reader.
"""

from __future__ import annotations

import argparse
import hashlib
import io
import json
import math
import os
import pathlib
import subprocess
import sys
import tarfile
import tomllib

ROOT = pathlib.Path(__file__).resolve().parent.parent
FIXTURES = ROOT / "fixtures"
MANIFEST = FIXTURES / "manifest.toml"
OUT_DIR = FIXTURES / "model1"
OUT_MANIFEST = OUT_DIR / "MANIFEST.toml"
REQUIREMENTS = ROOT / "tools" / "ground_truth" / "requirements.txt"
GLOBAL_JSON = ROOT / "reference" / "global.json"
LOCK_FILE = ROOT / "reference" / "ACadSharp.Reference" / "packages.lock.json"

EZDXF_IMAGE = "python:3.14-slim@sha256:a2b82f3c48559aa0a8446d9af49826b6e2b2016f4cd2afabfe6013ec53729170"
EZDXF_VERSION = "1.4.4"
DOTNET_IMAGE = "mcr.microsoft.com/dotnet/sdk:10.0.401-noble"

# The seven AutoCAD saves, and the six ASCII DXF twins beside them. The binary
# twins hold the same header and are not compared: they are R5.1's.
ORACLE_FIXTURES = [
    "acadsharp-ac1014",
    "acadsharp-ac1015",
    "acadsharp-ac1018",
    "acadsharp-ac1021",
    "acadsharp-ac1024",
    "acadsharp-ac1027",
    "acadsharp-ac1032",
]
EZDXF_INPUTS = [
    "dxf-sample-ac1015-ascii",
    "dxf-sample-ac1018-ascii",
    "dxf-sample-ac1021-ascii",
    "dxf-sample-ac1024-ascii",
    "dxf-sample-ac1027-ascii",
    "dxf-sample-ac1032-ascii",
]

ORACLE_INPUTS = ORACLE_FIXTURES + EZDXF_INPUTS
KINDS = ["header", "layer", "linetype", "textstyle", "dimstyle", "block_record", "layout", "dictionary", "xdata", "xrecord"]
NL = b"\n"


# ------------------------------------------------------------ number format ---


def real(value: float) -> str:
    """A double the way schema 1 spells one: shortest round trip, `.0`, no -0."""
    if not math.isfinite(value):
        raise ValueError(f"refusing to encode the non-finite value {value!r}")
    if value == 0.0:
        return "0.0"
    text = repr(float(value))
    if "e" in text:
        mantissa, exponent = text.split("e")
        sign = "-" if exponent.startswith("-") else ""
        exponent = exponent.lstrip("+-").lstrip("0") or "0"
        return f"{mantissa}e{sign}{exponent}"
    return text if "." in text else text + ".0"


def quote(value: str) -> str:
    out = ['"']
    for c in value:
        if c == '"':
            out.append('\\"')
        elif c == "\\":
            out.append("\\\\")
        elif c == "\b":
            out.append("\\b")
        elif c == "\f":
            out.append("\\f")
        elif c == "\n":
            out.append("\\n")
        elif c == "\r":
            out.append("\\r")
        elif c == "\t":
            out.append("\\t")
        elif ord(c) < 0x20:
            out.append(f"\\u{ord(c):04x}")
        else:
            out.append(c)
    out.append('"')
    return "".join(out)


def normalize_newlines(text: str) -> str:
    return text.replace("\r\n", "\n").replace("\r", "\n")


def line(kind: str, key: str, fields: list[tuple[str, str]]) -> tuple[str, str, str]:
    body = ",".join(f"{quote(n)}:{v}" for n, v in fields)
    return kind, key, f'{{"model":1,"record":{quote(kind)},{body}}}'


def encode(records: list[tuple[str, str, str]]) -> str:
    """The exact text of a .model1.jsonl: sorted by (kind order, key), LF, final newline."""
    ordered = sorted(records, key=lambda r: (KINDS.index(r[0]), r[1].encode("utf-8")))
    seen = set()
    for kind, key, _ in ordered:
        if (kind, key) in seen:
            raise ValueError(f"two {kind} records are keyed {key}")
        seen.add((kind, key))
    return "".join(text + "\n" for _, _, text in ordered)


# --------------------------------------------------------------- ezdxf side ---


def header_type(name: str, code: int, components: int) -> str:
    """The written form of a header variable, chosen by its DXF group code.

    The same table as `Model1Extractor.HeaderType` in the oracle and
    `model1::header_type` in the Rust adapters, written a third time on purpose.
    """
    if components == 2:
        return "point2"
    if components == 3:
        return "point3"
    if 0 <= code <= 9 or 100 <= code <= 102:
        return "string"
    if 40 <= code <= 59 or 140 <= code <= 149:
        return "double"
    if 60 <= code <= 79 or 90 <= code <= 99 or 160 <= code <= 179 or 270 <= code <= 289 or 370 <= code <= 389:
        return "int"
    if 290 <= code <= 299:
        return "bool"
    raise ValueError(f"{name}: no written form is defined for DXF code {code}")


def is_handle_code(code: int) -> bool:
    return 320 <= code <= 369 or 390 <= code <= 399 or 480 <= code <= 481


# ezdxf stamps these with the time of the read (it keeps its own creation date
# current), so the value is the moment the extraction ran and a frozen file
# would move on every freeze. Left out and listed with the others.
UNSTABLE = {"$TDCREATE"}


def header_lines(doc) -> tuple[list[tuple[str, str, str]], list[str]]:
    """The header records, and the names of the variables with no written form.

    A variable stored under a handle code (a material, a visual style) is the
    handle of an object, and handles stay out of the compared stream, so it has
    no record; one ezdxf stamps with the time of the read (UNSTABLE) cannot be
    frozen. Neither is dropped quietly: the names come back as the second value
    and are committed beside the stream.
    """
    out = []
    skipped = []
    for name, var in doc.header.hdrvars.items():
        if is_handle_code(int(var.code)) or name in UNSTABLE:
            skipped.append(name)
            continue
        value = var.value
        if isinstance(value, (tuple, list)) or hasattr(value, "x"):
            parts = [float(p) for p in (value if isinstance(value, (tuple, list)) else tuple(value))]
            kind = header_type(name, int(var.code), len(parts))
            text = "[" + ",".join(real(p) for p in parts) + "]"
        else:
            kind = header_type(name, int(var.code), 1)
            if kind == "string":
                text = quote(normalize_newlines(str(value)))
            elif kind == "int":
                text = str(int(value))
            elif kind == "double":
                text = real(float(value))
            else:
                text = "true" if int(value) != 0 else "false"
        out.append(line("header", name, [("name", quote(name)), ("type", quote(kind)), ("value", text)]))
    return out, sorted(skipped)


def layer_lines(doc) -> list[tuple[str, str, str]]:
    """One `layer` record per LAYER entry, keyed by name (docs/MODEL_1.md).

    Fields by DXF code. A colour is the colour index (62, its sign is `on`), or
    -1 when the layer holds a true colour (420), which has no index. Optional
    codes a DXF of an older version does not write read as the default the
    document gives.
    """
    out = []
    for layer in doc.layers:
        d = layer.dxf
        name = d.name
        has_true = d.hasattr("true_color")
        raw_color = int(d.get("color", 7))
        color = -1 if has_true else abs(raw_color)
        true_color = int(d.true_color) & 0xFFFFFF if has_true else -1
        fields = [
            ("name", quote(name)),
            ("flags", str(int(d.get("flags", 0)) & 7)),
            ("color", str(color)),
            ("true_color", str(true_color)),
            ("on", "true" if raw_color >= 0 else "false"),
            ("linetype", quote(str(d.get("linetype", "Continuous")))),
            ("lineweight", str(int(d.get("lineweight", -3)))),
            ("plot", "true" if int(d.get("plot", 1)) != 0 else "false"),
        ]
        out.append(line("layer", name, fields))
    return out


def linetype_lines(doc) -> list[tuple[str, str, str]]:
    """One `linetype` record per LTYPE entry, keyed by name (docs/MODEL_1.md).

    The pattern is ezdxf's tag list (72, 73, 40 and then, per segment, 49 with
    the tags that follow it), read the way the DXF group codes say: a code a
    segment does not write is the default the document gives. The style is the
    text style's name, resolved from the handle in 340.
    """
    out = []
    for lt in doc.linetypes:
        name = lt.dxf.name
        alignment, length, segs, cur = 65, 0.0, [], None
        for t in lt.pattern_tags.tags:
            c, v = t.code, t.value
            if c == 72:
                alignment = int(v)
            elif c == 40:
                length = float(v)
            elif c == 49:
                cur = {"length": float(v), "flags": 0, "shape": 0, "scale": 1.0, "rot": 0.0,
                       "ox": 0.0, "oy": 0.0, "text": "", "style": ""}
                segs.append(cur)
            elif cur is not None:
                if c == 74:
                    cur["flags"] = int(v)
                elif c == 75:
                    cur["shape"] = int(v)
                elif c == 340:
                    target = doc.entitydb.get(v)
                    cur["style"] = target.dxf.name if target is not None and target.dxf.hasattr("name") else ""
                elif c == 46:
                    cur["scale"] = float(v)
                elif c == 50:
                    cur["rot"] = math.radians(float(v))
                elif c == 44:
                    cur["ox"] = float(v)
                elif c == 45:
                    cur["oy"] = float(v)
                elif c == 9:
                    cur["text"] = normalize_newlines(str(v))
        items = ",".join(
            "{" + ",".join([
                f'"length":{real(g["length"])}', f'"flags":{g["flags"]}', f'"shape_number":{g["shape"]}',
                f'"scale":{real(g["scale"])}', f'"rotation":{real(g["rot"])}',
                f'"offset":[{real(g["ox"])},{real(g["oy"])}]', f'"text":{quote(g["text"])}',
                f'"style":{quote(g["style"])}',
            ]) + "}"
            for g in segs
        )
        fields = [
            ("name", quote(name)),
            ("description", quote(normalize_newlines(str(lt.dxf.get("description", ""))))),
            ("alignment", str(alignment)),
            ("pattern_length", real(length)),
            ("segments", "[" + items + "]"),
        ]
        out.append(line("linetype", name, fields))
    return out


def annotative(entry) -> bool:
    """Whether the entry carries the AcadAnnotative xdata with its flag set: the
    first 1070 inside the AnnotativeData group is 1."""
    if not entry.has_xdata("AcadAnnotative"):
        return False
    for code, value in entry.get_xdata("AcadAnnotative"):
        if code == 1070:
            return int(value) == 1
    return False


def textstyle_lines(doc) -> list[tuple[str, str, str]]:
    """One `textstyle` record per STYLE entry, keyed by name (docs/MODEL_1.md).

    Fields by DXF code. The stored last height is the field, whatever a reader
    would substitute for a zero. The oblique angle is converted from the DXF's
    degrees to radians.
    """
    out = []
    for st in doc.styles:
        d = st.dxf
        name = d.name
        if not name:  # an entry with no name cannot be keyed: it has no record
            continue
        flags = int(d.get("flags", 0))
        gen = int(d.get("generation_flags", 0))
        fields = [
            ("name", quote(name)),
            ("font_file", quote(str(d.get("font", "")))),
            ("big_font_file", quote(str(d.get("bigfont", "")))),
            ("height", real(float(d.get("height", 0.0)))),
            ("width_factor", real(float(d.get("width", 1.0)))),
            ("last_height", real(float(d.get("last_height", 0.0)))),
            ("oblique_angle", real(math.radians(float(d.get("oblique", 0.0))))),
            ("backward", "true" if gen & 2 else "false"),
            ("upside_down", "true" if gen & 4 else "false"),
            ("is_shape", "true" if flags & 1 else "false"),
            ("is_vertical", "true" if flags & 4 else "false"),
            ("xref_dependent", "true" if flags & 16 else "false"),
            ("annotative", "true" if annotative(st) else "false"),
        ]
        out.append(line("textstyle", name, fields))
    return out


# The `$DIM*` fields of a `dimstyle` record, in the order docs/MODEL_1.md gives
# them, with the type each is written as.
DIMENSION_FIELDS = [
    ("DIMPOST", "s"),
    ("DIMAPOST", "s"),
    ("DIMSCALE", "d"),
    ("DIMASZ", "d"),
    ("DIMEXO", "d"),
    ("DIMDLI", "d"),
    ("DIMEXE", "d"),
    ("DIMRND", "d"),
    ("DIMDLE", "d"),
    ("DIMTP", "d"),
    ("DIMTM", "d"),
    ("DIMFXL", "d"),
    ("DIMJOGANG", "d"),
    ("DIMTFILL", "i"),
    ("DIMTOL", "i"),
    ("DIMLIM", "i"),
    ("DIMTIH", "i"),
    ("DIMTOH", "i"),
    ("DIMSE1", "i"),
    ("DIMSE2", "i"),
    ("DIMTAD", "i"),
    ("DIMZIN", "i"),
    ("DIMAZIN", "i"),
    ("DIMARCSYM", "i"),
    ("DIMTXT", "d"),
    ("DIMCEN", "d"),
    ("DIMTSZ", "d"),
    ("DIMALTF", "d"),
    ("DIMLFAC", "d"),
    ("DIMTVP", "d"),
    ("DIMTFAC", "d"),
    ("DIMGAP", "d"),
    ("DIMALTRND", "d"),
    ("DIMALT", "i"),
    ("DIMALTD", "i"),
    ("DIMTOFL", "i"),
    ("DIMSAH", "i"),
    ("DIMTIX", "i"),
    ("DIMSOXD", "i"),
    ("DIMCLRD", "i"),
    ("DIMCLRE", "i"),
    ("DIMCLRT", "i"),
    ("DIMADEC", "i"),
    ("DIMUNIT", "i"),
    ("DIMDEC", "i"),
    ("DIMTDEC", "i"),
    ("DIMALTU", "i"),
    ("DIMALTTD", "i"),
    ("DIMAUNIT", "i"),
    ("DIMFRAC", "i"),
    ("DIMLUNIT", "i"),
    ("DIMDSEP", "i"),
    ("DIMTMOVE", "i"),
    ("DIMJUST", "i"),
    ("DIMSD1", "i"),
    ("DIMSD2", "i"),
    ("DIMTOLJ", "i"),
    ("DIMTZIN", "i"),
    ("DIMALTZ", "i"),
    ("DIMALTTZ", "i"),
    ("DIMFIT", "i"),
    ("DIMUPT", "i"),
    ("DIMATFIT", "i"),
    ("DIMFXLON", "i"),
    ("DIMTXTDIRECTION", "i"),
    ("DIMTXSTY", "s"),
    ("DIMLDRBLK", "s"),
    ("DIMBLK", "s"),
    ("DIMBLK1", "s"),
    ("DIMBLK2", "s"),
    ("DIMLTYPE", "s"),
    ("DIMLTEX1", "s"),
    ("DIMLTEX2", "s"),
    ("DIMLWD", "i"),
    ("DIMLWE", "i"),
]


def dimstyle_lines(doc) -> list[tuple[str, str, str]]:
    """One `dimstyle` record per DIMSTYLE entry, keyed by name (docs/MODEL_1.md).

    A DXF leaves a code out when it holds the default, and the default is not a
    value the file gave, so a field the entry does not write is `null`. ezdxf
    already resolves a handle-valued field (text style, arrow blocks, linetypes)
    to its name, and the angle is converted from the DXF's degrees to radians.
    """
    out = []
    for ds in doc.dimstyles:
        d = ds.dxf
        name = d.name
        if not name:
            continue
        fields = [("name", quote(name))]
        for field, kind in DIMENSION_FIELDS:
            attr = field.lower()
            if not d.hasattr(attr):
                text = "null"
            else:
                v = d.get(attr)
                if kind == "s":
                    text = quote(normalize_newlines(str(v)))
                elif kind == "i":
                    text = str(int(v))
                else:
                    f = float(v)
                    text = real(math.radians(f) if attr == "dimjogang" else f)
            fields.append(("$" + field, text))
        fields.append(("annotative", "true" if annotative(ds) else "false"))
        out.append(line("dimstyle", name, fields))
    return out


def table_entries(doc):
    """The table entries by the kind the owner paths of docs/MODEL_1.md use."""
    for kind, table in (
        ("layer", doc.layers),
        ("linetype", doc.linetypes),
        ("textstyle", doc.styles),
        ("dimstyle", doc.dimstyles),
        ("block_record", doc.block_records),
        ("appid", doc.appids),
        ("view", doc.views),
        ("vport", doc.viewports),
        ("ucs", doc.ucs),
    ):
        for entry in table:
            yield kind, entry.dxf.name, entry


def value_form(code: int) -> str:
    """The written form of a value, decided by its group code: one table for
    xdata and for XRECORDs, the same as `model1::value_form` and
    `Model1Extractor.ValueForm`, written a third time on purpose.
    """
    if (
        0 <= code <= 4 or 6 <= code <= 9 or 100 <= code <= 102 or 300 <= code <= 309
        or 410 <= code <= 419 or 430 <= code <= 439 or 470 <= code <= 479 or code == 999
        or code in (1000, 1002, 1003)
    ):
        return "text"
    if code in (5, 105, 1005) or 320 <= code <= 369 or 390 <= code <= 399 or 480 <= code <= 481:
        return "handle"
    if 310 <= code <= 319 or code == 1004:
        return "binary"
    if 10 <= code <= 37 or 1010 <= code <= 1013:
        return "point"
    if 38 <= code <= 59 or 110 <= code <= 149 or 210 <= code <= 239 or 460 <= code <= 469 or 1040 <= code <= 1042:
        return "real"
    if (
        60 <= code <= 99 or 150 <= code <= 179 or 270 <= code <= 299 or 370 <= code <= 389
        or 400 <= code <= 409 or 420 <= code <= 429 or 440 <= code <= 459 or code in (1070, 1071)
    ):
        return "whole"
    raise ValueError(f"no written form is defined for group code {code}")


def coded_value(code: int, raw) -> str:
    """One `{"code":..,"value":..}` pair."""
    form = value_form(code)
    if form == "text":
        value = quote(normalize_newlines(str(raw)))
    elif form == "handle":
        value = quote(format(int(str(raw), 16), "X"))
    elif form == "binary":
        data = raw if isinstance(raw, (bytes, bytearray)) else bytes.fromhex(str(raw))
        value = quote(bytes(data).hex().upper())
    elif form == "point":
        parts = [float(p) for p in (raw if isinstance(raw, (tuple, list)) else tuple(raw))]
        parts += [0.0] * (3 - len(parts))
        value = "[" + ",".join(real(p) for p in parts) + "]"
    elif form == "real":
        value = real(float(raw))
    else:
        value = str(int(raw))
    return '{"code":' + str(code) + ',"value":' + value + "}"


def dictionary_tree(doc):
    """The `dictionary` and `xrecord` lines, and the xdata owner of every
    dictionary and XRECORD reached (`dictionary/<path>`, `xrecord/<path>`), by
    handle.
    """
    lines, owners = _walk_dictionaries(doc)
    return lines, owners


def dictionary_lines(doc) -> list[tuple[str, str, str]]:
    return dictionary_tree(doc)[0]


def _walk_dictionaries(doc):
    """One `dictionary` record per DICTIONARY (and DICTIONARYWDFLT) reachable from
    the root, keyed by the path of entry keys, and per extension dictionary of a
    table entry or a layout (docs/MODEL_1.md). Those of entities are keyed by
    handle only and are not written.
    """
    out = []
    owners = {}
    seen = set()

    def walk(d, path: str) -> None:
        if id(d) in seen:
            return
        seen.add(id(d))
        owners[d.dxf.handle] = f"dictionary/{path}"
        # An entry whose handle names no object is a dangling reference.
        items = sorted(((k, e) for k, e in d.items() if e is not None), key=lambda kv: kv[0].encode("utf-8"))
        entries = ",".join(
            "{" + f'"key":{quote(k)},"class":{quote(e.dxftype())}' + "}" for k, e in items
        )
        out.append(
            line(
                "dictionary",
                path,
                [
                    ("name", quote(path)),
                    ("hard_owner", "true" if int(d.dxf.get("hard_owned", 0)) else "false"),
                    ("cloning", str(int(d.dxf.get("cloning", 1)))),
                    ("entries", f"[{entries}]"),
                    ("entry_count", str(len(items))),
                ],
            )
        )
        for k, e in items:
            child = k if not path else f"{path}/{k}"
            if e.dxftype() in ("DICTIONARY", "ACDBDICTIONARYWDFLT"):
                walk(e, child)
            elif e.dxftype() == "XRECORD" and e.dxf.handle not in owners:
                owners[e.dxf.handle] = f"xrecord/{child}"
                # The handle-valued entries go last, each group in stored order:
                # ACadSharp lists them last and a DWG keeps them in a stream of
                # their own, so their place among the others is not compared.
                tags = list(e.tags)
                tags = [t for t in tags if value_form(int(t.code)) != "handle"] + [
                    t for t in tags if value_form(int(t.code)) == "handle"
                ]
                values = ",".join(coded_value(int(t.code), t.value) for t in tags)
                out.append(
                    line(
                        "xrecord",
                        child,
                        [
                            ("name", quote(child)),
                            ("cloning", str(int(e.dxf.get("cloning", 0)))),
                            ("entries", f"[{values}]"),
                        ],
                    )
                )

    walk(doc.rootdict, "")
    for kind, name, entry in table_entries(doc):
        if entry.has_extension_dict:
            walk(entry.get_extension_dict().dictionary, f"table/{kind}/{name}")
    for layout in doc.layouts:
        dxf_layout = layout.dxf_layout
        if dxf_layout.has_extension_dict:
            walk(dxf_layout.get_extension_dict().dictionary, f"layout/{layout.name}")
    return out, owners


def xdata_lines(doc, owners) -> list[tuple[str, str, str]]:
    """One `xdata` record per (owner, application): table entries, layouts,
    dictionaries and XRECORDs by their path, and entities by handle.
    """
    from ezdxf.entities import DXFGraphic

    out = []

    def add(entity, owner: str) -> None:
        xd = entity.xdata
        if xd is None:
            return
        for appid, tags in xd.data.items():
            values = ",".join(coded_value(int(t.code), t.value) for t in tags if int(t.code) != 1001)
            key = f"{owner}|{appid}"
            out.append(
                line(
                    "xdata",
                    key,
                    [
                        ("name", quote(key)),
                        ("owner", quote(owner)),
                        ("appid", quote(appid)),
                        ("values", f"[{values}]"),
                    ],
                )
            )

    for kind, name, entry in table_entries(doc):
        add(entry, f"table/{kind}/{name}")
    for layout in doc.layouts:
        add(layout.dxf_layout, f"layout/{layout.name}")
    for handle, owner in owners.items():
        add(doc.entitydb[handle], owner)
    for entity in doc.entitydb.values():
        if isinstance(entity, DXFGraphic):
            add(entity, f"entity/{entity.dxf.handle.upper()}")
    return out


def point3(v) -> str:
    return "[" + ",".join(real(float(c)) for c in (v[0], v[1], v[2])) + "]"


def block_record_lines(doc) -> list[tuple[str, str, str]]:
    """One `block_record` record per BLOCK_RECORD entry, keyed by name (docs/MODEL_1.md).

    The flags, base point, path and description are on the BLOCK entity, the
    rest on the record. `entity_count` is the entities the block owns, the BLOCK
    and ENDBLK pair left out. A code the file does not write is the default the
    document gives, and the layout is the name of the LAYOUT the record's 340
    points at (empty for a block that has none). ezdxf keeps no preview bitmap,
    so `preview_bytes` is 0 here and the rules list the difference.
    """
    out = []
    for r in doc.block_records:
        d = r.dxf
        name = d.name
        b = r.block.dxf if r.block is not None else None
        layout = ""
        handle = d.get("layout", "0")
        if handle not in ("", "0"):
            target = doc.entitydb.get(handle)
            if target is not None and target.dxf.hasattr("name"):
                layout = target.dxf.name
        base = b.get("base_point", (0.0, 0.0, 0.0)) if b is not None else (0.0, 0.0, 0.0)
        fields = [
            ("name", quote(name)),
            ("flags", str(int(b.get("flags", 0)) if b is not None else 0)),
            ("base_point", point3(base)),
            ("xref_path", quote(normalize_newlines(str(b.get("xref_path", ""))) if b is not None else "")),
            ("description", quote(normalize_newlines(str(b.get("description", ""))) if b is not None else "")),
            ("units", str(int(d.get("units", 0)))),
            ("explodable", "true" if int(d.get("explode", 1)) != 0 else "false"),
            ("scale_uniformly", "true" if int(d.get("scale", 0)) != 0 else "false"),
            ("entity_count", str(len(r.entity_space))),
            ("layout", quote(layout)),
            ("preview_bytes", "0"),
        ]
        out.append(line("block_record", name, fields))
    return out


def point2(v) -> str:
    return "[" + ",".join(real(float(c)) for c in (v[0], v[1])) + "]"


def layout_lines(doc) -> list[tuple[str, str, str]]:
    """One `layout` record per LAYOUT object, keyed by name (docs/MODEL_1.md).

    The layout's own fields and the plot settings it embeds, by DXF code. A
    code the file does not write reads as the default ezdxf's DXF attribute
    table gives. The block record is the name of the record the 330 points at.
    """
    out = []
    for layout in doc.layouts:
        d = layout.dxf_layout.dxf
        name = d.name
        block = ""
        target = doc.entitydb.get(d.get("block_record_handle", "0"))
        if target is not None and target.dxf.hasattr("name"):
            block = target.dxf.name

        def text(attr: str) -> str:
            return quote(normalize_newlines(str(d.get(attr, ""))))

        def num(attr: str) -> str:
            # an attribute the DXF version does not define has no default: zero
            v = d.get(attr)
            return real(float(0 if v is None else v))

        def whole(attr: str) -> str:
            v = d.get(attr)
            return str(int(0 if v is None else v))

        fields = [
            ("name", quote(name)),
            ("tab_order", whole("taborder")),
            ("flags", whole("layout_flags")),
            ("min_limits", point2(d.get("limmin"))),
            ("max_limits", point2(d.get("limmax"))),
            ("insertion_base", point3(d.get("insert_base"))),
            ("min_extents", point3(d.get("extmin"))),
            ("max_extents", point3(d.get("extmax"))),
            ("elevation", num("elevation")),
            ("ucs_origin", point3(d.get("ucs_origin"))),
            ("ucs_x_axis", point3(d.get("ucs_xaxis"))),
            ("ucs_y_axis", point3(d.get("ucs_yaxis"))),
            ("ucs_ortho_type", whole("ucs_type")),
            ("block_record", quote(block)),
            ("page_name", text("page_setup_name")),
            ("printer_name", text("plot_configuration_file")),
            ("paper_size", text("paper_size")),
            ("plot_view_name", text("plot_view_name")),
            ("style_sheet", text("current_style_sheet")),
            ("margin_left", num("left_margin")),
            ("margin_bottom", num("bottom_margin")),
            ("margin_right", num("right_margin")),
            ("margin_top", num("top_margin")),
            ("paper_width", num("paper_width")),
            ("paper_height", num("paper_height")),
            ("plot_origin", point2((d.get("plot_origin_x_offset"), d.get("plot_origin_y_offset")))),
            ("plot_window_min", point2((d.get("plot_window_x1"), d.get("plot_window_y1")))),
            ("plot_window_max", point2((d.get("plot_window_x2"), d.get("plot_window_y2")))),
            ("scale_numerator", num("scale_numerator")),
            ("scale_denominator", num("scale_denominator")),
            ("plot_flags", whole("plot_layout_flags")),
            ("paper_units", whole("plot_paper_units")),
            ("rotation", whole("plot_rotation")),
            ("plot_type", whole("plot_type")),
            ("scale_type", whole("standard_scale_type")),
            ("shade_plot_mode", whole("shade_plot_mode")),
            ("shade_plot_resolution", whole("shade_plot_resolution_level")),
            ("shade_plot_dpi", whole("shade_plot_custom_dpi")),
            ("standard_scale", num("unit_factor")),
            ("paper_image_origin", point2((d.get("paper_image_origin_x"), d.get("paper_image_origin_y")))),
        ]
        out.append(line("layout", name, fields))
    return out


def command_extract_ezdxf(args) -> int:
    import ezdxf  # only here: `freeze` and `verify` must run without it

    manifest = tomllib.loads(pathlib.Path(args.manifest).read_text(encoding="utf-8"))
    rows = {r["id"]: r for r in manifest["oracle_input"]}
    out = pathlib.Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    for rid in args.only:
        row = rows[rid]
        path = pathlib.Path(args.dxf_root) / row["file"]
        if sha256(path.read_bytes()) != row["sha256"]:
            print(f"{rid}: {path} is not the file the manifest froze", file=sys.stderr)
            return 1
        doc = ezdxf.readfile(str(path))
        records, skipped = header_lines(doc)
        tree, owners = dictionary_tree(doc)
        text = encode(
            records + layer_lines(doc) + linetype_lines(doc) + textstyle_lines(doc) + dimstyle_lines(doc) + tree + xdata_lines(doc, owners)
            + block_record_lines(doc)
            + layout_lines(doc)
        )
        (out / f"{rid}.ezdxf.model1.jsonl").write_text(text, encoding="utf-8", newline="\n")
        (out / f"{rid}.ezdxf.model1.skipped").write_text("".join(n + "\n" for n in skipped), encoding="utf-8", newline="\n")
        print(f"{rid}: {len(text.splitlines())} records")
    return 0


# ---------------------------------------------------------------- the host ---


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def load_manifest() -> dict:
    return tomllib.loads(MANIFEST.read_text(encoding="utf-8"))


def tar_of(files: dict[str, pathlib.Path]) -> bytes:
    buf = io.BytesIO()
    with tarfile.open(fileobj=buf, mode="w") as tar:
        for arc, path in files.items():
            info = tar.gettarinfo(str(path), arc)
            info.uid = info.gid = 0
            info.uname = info.gname = ""
            with open(path, "rb") as fh:
                tar.addfile(info, fh)
    return buf.getvalue()


def untar(data: bytes) -> dict[str, bytes]:
    files = {}
    with tarfile.open(fileobj=io.BytesIO(data), mode="r") as tar:
        for member in tar.getmembers():
            if member.isfile():
                files[pathlib.PurePosixPath(member.name).name] = tar.extractfile(member).read()
    return files


def docker_run(image: str, script: str, stdin: bytes, env: list[str] | None = None) -> bytes:
    cmd = ["docker", "run", "--rm", "-i", "--platform", "linux/amd64"]
    for e in env or []:
        cmd += ["-e", e]
    cmd += [image, "sh", "-c", script]
    print("$ docker run ... " + image)
    proc = subprocess.run(cmd, input=stdin, stdout=subprocess.PIPE, check=False)
    if proc.returncode != 0:
        sys.exit(f"{image}: exit code {proc.returncode}")
    return proc.stdout


def run_oracle(manifest: dict) -> dict[str, bytes]:
    """The oracle's model-1 stream per fixture, from the pinned SDK image."""
    files: dict[str, pathlib.Path] = {"fixtures/manifest.toml": MANIFEST, "reference/global.json": GLOBAL_JSON}
    for path in sorted((ROOT / "reference").rglob("*")):
        if path.is_file() and not any(p in ("bin", "obj") for p in path.parts):
            files[path.relative_to(ROOT).as_posix()] = path
    rows = {f["id"]: f for f in manifest["fixture"]}
    rows.update({r["id"]: r for r in manifest["oracle_input"]})
    for fid in ORACLE_INPUTS:
        files[f"fixtures/{rows[fid]['file']}"] = FIXTURES / rows[fid]["file"]
    runs = "".join(
        f"dotnet /tmp/oracle/ACadSharp.Reference.dll fixtures/{rows[fid]['file']} "
        f"--jsonl /tmp/discard.jsonl --model1 /out/{fid}.oracle.model1.jsonl >&2 && "
        for fid in ORACLE_INPUTS
    )
    script = (
        "set -e; mkdir -p /w /out && tar -x -C /w && cd /w && "
        "export HOME=/tmp DOTNET_CLI_HOME=/tmp DOTNET_NOLOGO=1 DOTNET_CLI_TELEMETRY_OPTOUT=1 && "
        "dotnet restore reference/ACadSharp.Reference/ACadSharp.Reference.csproj --locked-mode -v q >&2 && "
        "dotnet build reference/ACadSharp.Reference/ACadSharp.Reference.csproj -c Release --no-restore -v q -o /tmp/oracle >&2 && "
        + runs
        + "tar -c -C /out ."
    )
    return untar(docker_run(DOTNET_IMAGE, script, tar_of(files)))


def run_ezdxf(manifest: dict) -> dict[str, bytes]:
    rows = {r["id"]: r for r in manifest["oracle_input"]}
    files: dict[str, pathlib.Path] = {
        "fixtures/manifest.toml": MANIFEST,
        "tools/model1.py": pathlib.Path(__file__).resolve(),
        "tools/ground_truth/requirements.txt": REQUIREMENTS,
    }
    for rid in EZDXF_INPUTS:
        files[f"fixtures/{rows[rid]['file']}"] = FIXTURES / rows[rid]["file"]
    only = "".join(f" --only {r}" for r in EZDXF_INPUTS)
    script = (
        "set -e; mkdir -p /w && tar -x -C /w && cd /w && "
        "pip install -q --no-cache-dir --require-hashes -r tools/ground_truth/requirements.txt >&2 && "
        "python tools/model1.py extract-ezdxf --manifest fixtures/manifest.toml "
        f"--dxf-root fixtures --out /out{only} >&2 && tar -c -C /out ."
    )
    return untar(docker_run(EZDXF_IMAGE, script, tar_of(files), ["PYTHONHASHSEED=0"]))


def requirements_sha256() -> str:
    return sha256(REQUIREMENTS.read_bytes())


def locked_acadsharp() -> str:
    lock = json.loads(LOCK_FILE.read_text(encoding="utf-8"))
    for framework in lock.get("dependencies", {}).values():
        entry = framework.get("ACadSharp")
        if entry and "resolved" in entry:
            return entry["resolved"]
    sys.exit("packages.lock.json does not lock ACadSharp")


def sdk_version() -> str:
    return json.loads(GLOBAL_JSON.read_text(encoding="utf-8"))["sdk"]["version"]


def render_manifest(rows: list[dict]) -> str:
    out = [
        "# fixtures/model1/MANIFEST.toml: the frozen ground truths for the model-1 stream.",
        "#",
        "# Written by `tools/model1.py freeze --accept`; checked by `tools/model1.py verify`",
        "# and by tests/model1_fixtures.rs. Do not edit: the next accepted freeze overwrites it.",
        "# The sha256 is of the file as committed (they are small, so they are not compressed).",
        "",
    ]
    for r in rows:
        out.append("[[file]]")
        for key, value in r.items():
            out.append(f"{key} = {value}" if isinstance(value, int) else f'{key} = "{value}"')
        out.append("")
    return "\n".join(out)


def stage(manifest: dict) -> tuple[list[dict], dict[str, bytes]]:
    oracle = run_oracle(manifest)
    ezdxf_files = run_ezdxf(manifest)
    acadsharp = locked_acadsharp()
    sdk = sdk_version()
    rows: list[dict] = []
    store: dict[str, bytes] = {}
    for fid in ORACLE_INPUTS:
        name = f"{fid}.oracle.model1.jsonl"
        data = oracle[name]
        rows.append({
            "id": fid, "engine": "oracle", "path": f"oracle/{name}", "sha256": sha256(data),
            "records": data.count(NL), "acadsharp_version": acadsharp, "dotnet_sdk": sdk,
            "image": DOTNET_IMAGE,
        })
        store[f"oracle/{name}"] = data
    req = requirements_sha256()
    for rid in EZDXF_INPUTS:
        name = f"{rid}.ezdxf.model1.jsonl"
        data = ezdxf_files[name]
        rows.append({
            "id": rid, "engine": "ezdxf", "path": f"ezdxf/{name}", "sha256": sha256(data),
            "records": data.count(NL), "ezdxf_version": EZDXF_VERSION, "requirements_sha256": req,
            "image": EZDXF_IMAGE,
        })
        store[f"ezdxf/{name}"] = data
        skipped = ezdxf_files[f"{rid}.ezdxf.model1.skipped"]
        rows[-1]["skipped_handle_variables"] = skipped.decode("utf-8").replace("\n", " ").strip()
        store[f"ezdxf/{rid}.ezdxf.model1.skipped"] = skipped
    return rows, store


def command_freeze(args) -> int:
    manifest = load_manifest()
    rows, store = stage(manifest)
    moved = []
    for rel, data in store.items():
        path = OUT_DIR / rel
        if not path.is_file():
            moved.append(f"{rel}: new ({data.count(NL)} records)")
        elif path.read_bytes() != data:
            old = path.read_text(encoding="utf-8").splitlines()
            new = data.decode("utf-8").splitlines()
            gone = [x for x in old if x not in new]
            came = [x for x in new if x not in old]
            moved.append(f"{rel}: moved, {len(gone)} lines gone, {len(came)} new")
            for x in gone[:5]:
                moved.append(f"    - {x}")
            for x in came[:5]:
                moved.append(f"    + {x}")
    for m in moved:
        print(m)
    new_manifest = render_manifest(rows)
    if OUT_MANIFEST.is_file() and OUT_MANIFEST.read_text(encoding="utf-8") != new_manifest:
        moved.append("MANIFEST.toml: moved")
        print("MANIFEST.toml: moved")
    if not args.accept:
        print("nothing written (pass --accept to write)" if moved else "no differences")
        return 1 if moved else 0
    for rel, data in store.items():
        path = OUT_DIR / rel
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(data)
    OUT_MANIFEST.write_text(new_manifest, encoding="utf-8", newline="\n")
    print(f"wrote {len(store)} files and the manifest")
    return 0


def command_verify(_args) -> int:
    if not OUT_MANIFEST.is_file():
        print(f"{OUT_MANIFEST} is missing")
        return 1
    rows = tomllib.loads(OUT_MANIFEST.read_text(encoding="utf-8")).get("file", [])
    problems = []
    listed = {(r["engine"], r["id"]) for r in rows}
    want = {("oracle", f) for f in ORACLE_INPUTS} | {("ezdxf", r) for r in EZDXF_INPUTS}
    for missing in sorted(want - listed):
        problems.append(f"{missing}: no row")
    for extra in sorted(listed - want):
        problems.append(f"{extra}: a row for something this tool does not freeze")
    req = requirements_sha256()
    for r in rows:
        path = OUT_DIR / r["path"]
        if not path.is_file():
            problems.append(f"{r['path']}: missing")
            continue
        data = path.read_bytes()
        if sha256(data) != r["sha256"]:
            problems.append(f"{r['path']}: hashes to {sha256(data)}, the manifest froze {r['sha256']}")
        if data.count(NL) != r["records"]:
            problems.append(f"{r['path']}: {data.count(NL)} records, the manifest says {r['records']}")
        if r["engine"] == "ezdxf" and r["requirements_sha256"] != req:
            problems.append(f"{r['path']}: made with other requirements than tools/ground_truth/requirements.txt")
        try:
            text = data.decode("utf-8")
            if not text.endswith("\n") or "\r" in text:
                problems.append(f"{r['path']}: not LF-terminated")
            parsed = [json.loads(x) for x in text.splitlines()]
            keys = [(KINDS.index(p["record"]), p["name"].encode("utf-8")) for p in parsed]
            if keys != sorted(keys) or len(set(keys)) != len(keys):
                problems.append(f"{r['path']}: records are not sorted and unique")
            if any(p.get("model") != 1 for p in parsed):
                problems.append(f"{r['path']}: a record without model 1")
        except (ValueError, KeyError) as e:
            problems.append(f"{r['path']}: {e}")
    for p in problems:
        print(p)
    if problems:
        return 1
    print(f"model-1 ground truths: {len(rows)} files, every one is the file the manifest froze")
    return 0


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    sub = ap.add_subparsers(dest="cmd", required=True)
    p = sub.add_parser("extract-ezdxf")
    p.add_argument("--manifest", default=str(MANIFEST))
    p.add_argument("--dxf-root", default=str(FIXTURES))
    p.add_argument("--out", required=True)
    p.add_argument("--only", action="append", required=True)
    p.set_defaults(fn=command_extract_ezdxf)
    p = sub.add_parser("freeze")
    p.add_argument("--accept", action="store_true")
    p.set_defaults(fn=command_freeze)
    p = sub.add_parser("verify")
    p.set_defaults(fn=command_verify)
    args = ap.parse_args(argv)
    return args.fn(args)


if __name__ == "__main__":
    sys.exit(main())
