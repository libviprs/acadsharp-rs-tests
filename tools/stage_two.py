#!/usr/bin/env python3
"""Stage two of the round-trip matrix: the nightly (R5.3.1, acadsharp-rs-tests#41).

Stage one is on the PR gate and its outputs are rows of `fixtures/manifest.toml`.
Stage two is the same plan over the six other saves, `sample_base.dwg` and the 41
Tier B synthetics (`fixtures/written_plan.toml`, `[stage.two]`), about 2,500
outputs and 9,216 cells, and none of it is stored in the repository. This makes
everything it needs in a scratch directory on the docker host, then runs the one
ignored test that judges it.

    python3 tools/stage_two.py --scratch DIR                  # every phase
    python3 tools/stage_two.py --scratch DIR --phase rust     # one phase
    python3 tools/stage_two.py --scratch DIR --regen          # also rewrite VERDICTS_STAGE2.toml

Phases, in order:

    rust     acadrust writes its outputs (src/bin/rewrite-acadrust.rs, twice each)
    dotnet   ACadSharp writes its outputs through the oracle (twice each, and a pair
             that differs fails the run), then the oracle reads every output
    index    DIR/index.toml: one row per planned output, with its hash
    ezdxf    ezdxf reads every DXF output
    test     `tools/run-tests.sh --stage-two DIR` in the gate's container

Where it runs. GitHub Actions is off on these repos, so "nightly" means this, on the
x64 docker host. Docker resolves a `-v` path on the host, so this has to run
somewhere the paths mean the same thing on both sides: a container that has the
docker CLI and the host's socket, with the tree mounted path-identical (what
`tools/nas-gate.sh` documents), or the host itself. The suite and its two sibling
trees (`../acadsharp-rs`, `../acadrust`) have to be laid out beside each other, as
`tools/run-tests.sh` expects, and DIR has to sit under the same mount.

Every phase prints its wall time and appends it to DIR/timing.txt, which is what the
PR that commits a table quotes.

Stdlib only; needs Python 3.14 because it reuses `regenerate_written`'s plan reader.
"""

from __future__ import annotations

import argparse
import hashlib
import pathlib
import shutil
import subprocess
import sys
import time
import tomllib

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))

import regenerate_written as rw  # noqa: E402

ROOT = rw.ROOT
PARENT = ROOT.parent
SDK_IMAGE = rw.ref.DEFAULT_IMAGE
RUST_IMAGE = "rust:1.99.1-bookworm"
EZDXF_IMAGE = "python:3.14-slim@sha256:a2b82f3c48559aa0a8446d9af49826b6e2b2016f4cd2afabfe6013ec53729170"
STAGE = "two"

# One line per command, run by `xargs -P`; each writes its own status files so a
# failure of one output is that output's row and never a stopped run.
BATCH = r"""#!/bin/sh
# usage: batch.sh <kind> <id> <src> <container> <version> <ext>
D=/repo/reference/ACadSharp.Reference/bin/Release/net10.0/ACadSharp.Reference.dll
kind=$1; id=$2; src=$3; c=$4; v=$5; ext=$6
case $kind in
  rewrite)
    for n in 1 2; do
      dotnet $D "$src" --rewrite-to /out/gen$n/$id.$ext --as $c --version $v >/dev/null 2>/out/status/$id.w$n.err
      echo $? > /out/status/$id.w$n.rc
      [ "$(cat /out/status/$id.w$n.rc)" = 0 ] || exit 0
    done
    if cmp -s /out/gen1/$id.$ext /out/gen2/$id.$ext; then
      mv /out/gen1/$id.$ext /out/files/$id.$ext; echo same > /out/status/$id.det
    else
      echo DIFFER > /out/status/$id.det
    fi
    rm -f /out/gen1/$id.$ext /out/gen2/$id.$ext ;;
  read)
    dotnet $D /out/files/$id.$ext --jsonl /out/read/$id.reference.jsonl >/dev/null 2>/out/status/$id.r.err
    echo $? > /out/status/$id.r.rc ;;
esac
"""


def sh(command: list[str], *, check: bool = True, **kwargs) -> subprocess.CompletedProcess:
    print("$ " + " ".join(command), flush=True)
    return subprocess.run(command, check=check, **kwargs)


def docker(image: str, mounts: list[tuple[str, str]], env: dict[str, str], workdir: str,
           command: list[str]) -> list[str]:
    cmd = ["docker", "run", "--rm", "--platform", "linux/amd64"]
    for host, inside in mounts:
        cmd += ["-v", f"{host}:{inside}"]
    for key, value in env.items():
        cmd += ["-e", f"{key}={value}"]
    return cmd + ["-w", workdir, image, *command]


class Timer:
    def __init__(self, scratch: pathlib.Path) -> None:
        self.path = scratch / "timing.txt"

    def run(self, name: str, fn) -> None:
        started = time.monotonic()
        fn()
        took = time.monotonic() - started
        line = f"{name}: {took:.0f} s"
        print(line, flush=True)
        with self.path.open("a", encoding="utf-8") as handle:
            handle.write(line + "\n")


def first_line(path: pathlib.Path) -> str:
    try:
        for line in path.read_text(encoding="utf-8", errors="replace").splitlines():
            if line.strip():
                return line.strip()
    except OSError:
        pass
    return ""


def phase_rust(scratch: pathlib.Path, volume: str, only: list[str]) -> None:
    out = scratch / "acadrust"
    out.mkdir(parents=True, exist_ok=True)
    extra = "".join(f" --only {o}" for o in only)
    sh(docker(
        RUST_IMAGE, [(str(PARENT), str(PARENT)), (volume, "/cargo-home")],
        {"CARGO_HOME": "/cargo-home", "CARGO_TARGET_DIR": "/cargo-home/target"}, str(ROOT),
        ["sh", "-c",
         f"cargo run -q --features acadrust --bin rewrite-acadrust -- --stage {STAGE} "
         f"--out {out}{extra} > {scratch}/acadrust.log 2>&1; echo rc=$? >> {scratch}/acadrust.log"],
    ))
    tail = first_line(scratch / "acadrust.log")
    print(f"  acadrust.log starts with: {tail}")


def dotnet(scratch: pathlib.Path, jobs: int, command: str) -> None:
    sh(docker(
        SDK_IMAGE, [(str(ROOT), "/repo"), (str(scratch), "/out")],
        {"HOME": "/tmp", "DOTNET_CLI_HOME": "/tmp", "DOTNET_CLI_TELEMETRY_OPTOUT": "1",
         "DOTNET_NOLOGO": "1", "NUGET_PACKAGES": "/tmp/nuget"}, "/repo", ["sh", "-c", command]))


def phase_dotnet(scratch: pathlib.Path, jobs: int, plan: list[dict], sources: dict[str, dict]) -> None:
    for sub in ("gen1", "gen2", "files", "read", "status", "work"):
        (scratch / sub).mkdir(parents=True, exist_ok=True)
    (scratch / "work" / "batch.sh").write_text(BATCH, encoding="utf-8")
    dotnet(scratch, jobs,
           "dotnet restore reference/ACadSharp.Reference/ACadSharp.Reference.csproj --locked-mode -v q "
           "&& dotnet build reference/ACadSharp.Reference/ACadSharp.Reference.csproj -c Release "
           "--no-restore -v q")

    # ACadSharp's rewrites, twice each.
    lines = []
    for o in plan:
        if o["writer"] != "acadsharp":
            continue
        src = f"/repo/fixtures/{sources[o['source']]['file']}"
        lines.append(f"rewrite {o['id']} {src} {rw.ORACLE_AS[o['container']]} {o['version']} {o['ext']}")
    (scratch / "work" / "rewrite.txt").write_text("\n".join(lines) + "\n", encoding="utf-8")
    dotnet(scratch, jobs, f"xargs -P {jobs} -L 1 sh /out/work/batch.sh < /out/work/rewrite.txt")

    # acadrust's, which the first phase wrote.
    for o in plan:
        if o["writer"] == "acadrust":
            produced = scratch / "acadrust" / f"{o['id']}.{o['ext']}"
            if produced.is_file():
                shutil.copy(produced, scratch / "files" / produced.name)

    # The oracle reads every output there is.
    reads = []
    for o in plan:
        if (scratch / "files" / f"{o['id']}.{o['ext']}").is_file():
            reads.append(f"read {o['id']} - - - {o['ext']}")
    (scratch / "work" / "read.txt").write_text("\n".join(reads) + "\n", encoding="utf-8")
    dotnet(scratch, jobs, f"xargs -P {jobs} -L 1 sh /out/work/batch.sh < /out/work/read.txt")


def phase_index(scratch: pathlib.Path, plan: list[dict]) -> None:
    rows = ["# Written by tools/stage_two.py. One row per planned output.", ""]
    differ = []
    for o in plan:
        rid, ext = o["id"], o["ext"]
        row = [f'id = "{rid}"', f'writer = "{o["writer"]}"', f'source = "{o["source"]}"',
               f'container = "{o["container"]}"', f'version = "{o["version"]}"', f'ext = "{ext}"']
        status = scratch / "status"
        file = scratch / "files" / f"{rid}.{ext}"
        failure = None
        if o["writer"] == "acadrust":
            err = scratch / "acadrust" / f"{rid}.error.txt"
            if err.is_file():
                failure = first_line(err)
        else:
            if (status / f"{rid}.det").is_file() and (status / f"{rid}.det").read_text().strip() == "DIFFER":
                differ.append(rid)
            for n in ("w1", "w2"):
                rc = status / f"{rid}.{n}.rc"
                if rc.is_file() and rc.read_text().strip() != "0":
                    failure = first_line(status / f"{rid}.{n}.err") or f"exit {rc.read_text().strip()}"
                    break
        if failure is not None or not file.is_file():
            failure = failure or "no file was produced"
            row.append(f'write_failed = {toml_str(failure)}')
        else:
            data = file.read_bytes()
            row += [f'sha256 = "{hashlib.sha256(data).hexdigest()}"', f"bytes = {len(data)}"]
            reading = scratch / "read" / f"{rid}.reference.jsonl"
            rc = status / f"{rid}.r.rc"
            if not reading.is_file() or (rc.is_file() and rc.read_text().strip() != "0"):
                row.append(f'no_reference = {toml_str(first_line(status / (rid + ".r.err")) or "no reading")}')
        rows += ["[[output]]", *row, ""]
    (scratch / "index.toml").write_text("\n".join(rows), encoding="utf-8")
    if differ:
        raise SystemExit("two writes of one output differ for: " + ", ".join(differ[:20]) +
                         (" ..." if len(differ) > 20 else ""))
    print(f"  index.toml: {len(plan)} outputs")


def toml_str(s: str) -> str:
    return '"' + s.replace("\\", "\\\\").replace('"', '\\"') + '"'


def phase_ezdxf(scratch: pathlib.Path) -> None:
    out = scratch / "ezdxf"
    out.mkdir(parents=True, exist_ok=True)
    sh(docker(
        EZDXF_IMAGE, [(str(ROOT), "/repo"), (str(scratch), "/out")], {"PYTHONHASHSEED": "0"}, "/repo",
        ["sh", "-c",
         "pip install -q --no-cache-dir --require-hashes -r tools/ground_truth/requirements.txt "
         "&& python tools/ground_truth/extract_ezdxf.py extract-dir --index /out/index.toml "
         "--files /out/files --out /out/ezdxf"]))


def phase_test(scratch: pathlib.Path, volume: str, regen: bool) -> None:
    env = {"CARGO_HOME": "/cargo-home", "CARGO_TARGET_DIR": "/cargo-home/target",
           "CARGO_TERM_COLOR": "never", "RUSTFLAGS": "-Dwarnings"}
    if regen:
        env["PARITY_REGEN"] = "1"
    sh(docker(
        RUST_IMAGE, [(str(PARENT), str(PARENT)), (volume, "/cargo-home")], env, str(ROOT),
        ["sh", "-c",
         f"{ROOT}/tools/run-tests.sh --in-ci --acadsharp-rs {PARENT}/acadsharp-rs "
         f"--acadrust {PARENT}/acadrust --stage-two {scratch}"]))


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--scratch", required=True, help="the scratch directory (under the same mount)")
    ap.add_argument("--jobs", type=int, default=4, help="oracle processes at a time (default 4)")
    ap.add_argument("--phase", action="append", choices=("rust", "dotnet", "index", "ezdxf", "test"))
    ap.add_argument("--only", action="append", default=[], help="an output id, for rust (debugging)")
    ap.add_argument("--cargo-volume", default="acadsharp-stage-two-cargo")
    ap.add_argument("--regen", action="store_true", help="rewrite parity/VERDICTS_STAGE2.toml")
    args = ap.parse_args()

    scratch = pathlib.Path(args.scratch).resolve()
    scratch.mkdir(parents=True, exist_ok=True)
    manifest = rw.ref.load_manifest()
    sources = {f["id"]: f for f in manifest["fixture"]}
    plan = rw.outputs(rw.load_plan(), STAGE)
    phases = args.phase or ["rust", "dotnet", "index", "ezdxf", "test"]
    timer = Timer(scratch)
    print(f"stage {STAGE}: {len(plan)} outputs from {len({o['source'] for o in plan})} sources, "
          f"scratch {scratch}")
    if "rust" in phases:
        timer.run("rust", lambda: phase_rust(scratch, args.cargo_volume, args.only))
    if "dotnet" in phases:
        timer.run("dotnet", lambda: phase_dotnet(scratch, args.jobs, plan, sources))
    if "index" in phases:
        timer.run("index", lambda: phase_index(scratch, plan))
    if "ezdxf" in phases:
        timer.run("ezdxf", lambda: phase_ezdxf(scratch))
    if "test" in phases:
        timer.run("test", lambda: phase_test(scratch, args.cargo_volume, args.regen))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
