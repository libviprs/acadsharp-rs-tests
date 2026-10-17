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
KINDS = ["header", "layer"]
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
        text = encode(records + layer_lines(doc))
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
