#!/usr/bin/env python3
"""Regenerate and verify the .NET reference artefacts.

`fixtures/manifest.toml` is the source of truth for both sides: the input hashes
it already carried, and the reference hashes this tool writes into it. The
artefacts themselves live in `fixtures/expected/`.

    python3 tools/regenerate_reference.py verify        # offline, no .NET
    python3 tools/regenerate_reference.py regenerate    # runs the oracle, reports, writes nothing
    python3 tools/regenerate_reference.py regenerate --accept   # writes

`regenerate` never touches a checked-in file without `--accept`. It stages into a
temporary directory, compares, prints what moved, and exits non-zero if anything
did. That is the whole shape of the thing: an ACadSharp upgrade has to arrive as
a reviewed diff with a person's name on it, not as a tool that noticed a
difference and helpfully resolved it. CI runs `verify`, and a test asserts no
workflow anywhere in this repository passes `--accept`.

The sha256 contract is on the UNCOMPRESSED canonical JSONL, not on the `.zst`.
Zstandard frames are reproducible for one library version and one set of frame
parameters, and this tool pins both, but they are not a cross-implementation
guarantee and the brief (section 16) says to put the contract on the bytes that
are. The `.zst` is storage; `reference_jsonl_uncompressed_sha256` is the claim.

Needs Python 3.14 or newer for `compression.zstd`. That is a pin, not an
accident: falling back to a second zstd implementation would mean two code paths
producing two different files.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import pathlib
import re
import shutil
import subprocess
import sys
import tempfile
import tomllib

if sys.version_info < (3, 14):  # pragma: no cover - the message is the point
    sys.exit(
        "tools/regenerate_reference.py needs Python 3.14 or newer for "
        "compression.zstd; this is Python "
        f"{sys.version_info.major}.{sys.version_info.minor}"
    )

from compression import zstd  # noqa: E402  (after the version guard on purpose)

ROOT = pathlib.Path(__file__).resolve().parent.parent
MANIFEST = ROOT / "fixtures" / "manifest.toml"
FIXTURE_DIR = ROOT / "fixtures"
EXPECTED_DIR = FIXTURE_DIR / "expected"
SUMMARY = EXPECTED_DIR / "SUMMARY.md"
PROJECT = ROOT / "reference" / "ACadSharp.Reference"
PROJECT_FILE = PROJECT / "ACadSharp.Reference.csproj"
LOCK_FILE = PROJECT / "packages.lock.json"
GLOBAL_JSON = ROOT / "reference" / "global.json"

# The container the whole toolchain runs in when there is no local dotnet. An
# exact digest-free tag, but never `latest`: "the SDK CI happened to have" is
# not a pin, and the SDK version is stamped into every manifest row.
DEFAULT_IMAGE = "mcr.microsoft.com/dotnet/sdk:10.0.401-noble"

# Deterministic Zstandard settings, written out rather than left to defaults.
# nb_workers is the one that matters most: any value above zero lets libzstd
# split the input across threads and the frame then depends on how many threads
# the machine had.
ZSTD_LEVEL = 19
ZSTD_PARAMETERS = {
    zstd.CompressionParameter.compression_level: ZSTD_LEVEL,
    zstd.CompressionParameter.content_size_flag: 1,
    zstd.CompressionParameter.checksum_flag: 0,
    zstd.CompressionParameter.dict_id_flag: 0,
    zstd.CompressionParameter.nb_workers: 0,
}

REFERENCE_SCHEMA = 1
SVG_SCHEMA = 1

JSONL_SUFFIX = ".reference.jsonl"
ZST_SUFFIX = ".reference.jsonl.zst"
SVG_SUFFIX = ".reference.svg"

# The keys this tool owns in every [[fixture]] row, in the order it writes them.
# Ints are written bare, everything else quoted.
REFERENCE_KEYS = (
    "reference_jsonl",
    "reference_jsonl_uncompressed_sha256",
    "reference_svg",
    "reference_svg_sha256",
    "acadsharp_reference_version",
    "acadsharp_reference_commit",
    "dotnet_sdk",
    "reference_schema",
    "svg_schema",
)
INT_KEYS = frozenset({"reference_schema", "svg_schema"})

# Record names that count as drawable. A reference holding zero of them cannot
# fail a differential, so it is refused rather than accepted as "the drawing was
# empty".
PRIMITIVE_RECORDS = frozenset(
    {"line", "polyline", "arc", "circle", "ellipse", "spline", "polygon", "text", "point"}
)


# ---------------------------------------------------------------- manifest ---


def load_manifest() -> dict:
    with MANIFEST.open("rb") as handle:
        return tomllib.load(handle)


def sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def compress(data: bytes) -> bytes:
    """The one compression path. See ZSTD_PARAMETERS for why each flag is set."""
    compressor = zstd.ZstdCompressor(options=ZSTD_PARAMETERS)
    return compressor.compress(data, mode=zstd.ZstdCompressor.FLUSH_FRAME)


def decompress(data: bytes) -> bytes:
    return zstd.decompress(data)


def locked_acadsharp_version() -> str | None:
    """The ACadSharp version `packages.lock.json` actually locks.

    Read from the lock rather than from the csproj, because the lock is what
    `dotnet restore --locked-mode` enforces. A csproj that asked for 3.7.2
    against a lock that pins 3.7.1 fails the restore, so the lock is the one
    that cannot be wrong about what ran.
    """
    if not LOCK_FILE.is_file():
        return None
    lock = json.loads(LOCK_FILE.read_text(encoding="utf-8"))
    for framework in lock.get("dependencies", {}).values():
        entry = framework.get("ACadSharp")
        if entry and "resolved" in entry:
            return entry["resolved"]
    return None


def pinned_sdk_version() -> str | None:
    if not GLOBAL_JSON.is_file():
        return None
    return json.loads(GLOBAL_JSON.read_text(encoding="utf-8")).get("sdk", {}).get("version")


def render_reference_block(values: dict[str, object]) -> list[str]:
    lines = []
    for key in REFERENCE_KEYS:
        value = values[key]
        if key in INT_KEYS:
            lines.append(f"{key} = {int(value)}")
        else:
            lines.append(f'{key} = "{value}"')
    return lines


def rewrite_manifest(text: str, per_fixture: dict[str, dict[str, object]]) -> str:
    """Set the reference keys on every [[fixture]] row, leaving the rest alone.

    Surgical line editing rather than a TOML round trip. A round trip would
    reformat the whole file, drop every comment in it, and turn a one-hash diff
    into an unreviewable one. The keys this tool owns are removed and rewritten
    as a block immediately after the row's `sha256`, so the output is stable no
    matter what order they were in before.
    """
    lines = text.splitlines()
    out: list[str] = []
    index = 0
    current_id: str | None = None
    pending: list[str] | None = None

    def flush_pending() -> None:
        nonlocal pending
        if pending:
            out.extend(pending)
            pending = None

    while index < len(lines):
        line = lines[index]
        stripped = line.strip()

        if stripped.startswith("["):
            flush_pending()
            current_id = None
            out.append(line)
            index += 1
            continue

        key_match = re.match(r'^\s*([A-Za-z0-9_]+)\s*=\s*(.*)$', line)
        if key_match and current_id is None and key_match.group(1) == "id":
            current_id = key_match.group(2).strip().strip('"')
            out.append(line)
            index += 1
            continue

        if key_match and key_match.group(1) in REFERENCE_KEYS:
            # Dropped here and re-emitted after sha256, so a row that already
            # had them does not accumulate a second copy.
            index += 1
            continue

        out.append(line)
        if key_match and key_match.group(1) == "sha256" and current_id in per_fixture:
            pending = render_reference_block(per_fixture[current_id])
            flush_pending()
        index += 1

    flush_pending()
    return "\n".join(out) + "\n"


# ------------------------------------------------------------- the oracle ---


def choose_runner(requested: str, image: str) -> list[str]:
    """How to invoke dotnet, said out loud.

    `auto` is not a silent fallback: it prints which it picked. A run that
    silently used a different toolchain than the one the manifest records is
    exactly the failure this whole file exists to prevent.
    """
    local = shutil.which("dotnet")
    if requested == "dotnet" or (requested == "auto" and local):
        if not local:
            raise SystemExit("--runner dotnet was asked for but there is no dotnet on PATH")
        print(f"runner: local dotnet at {local}")
        return [local]

    if not shutil.which("docker"):
        raise SystemExit(
            "no dotnet on PATH and no docker either. Install the pinned SDK "
            f"({pinned_sdk_version()}) or make docker available."
        )
    print(f"runner: docker, image {image}")
    uid = os.getuid()
    gid = os.getgid()
    home = pathlib.Path(tempfile.gettempdir()) / "acadsharp-reference-dotnet-home"
    home.mkdir(parents=True, exist_ok=True)
    return [
        "docker", "run", "--rm",
        "-u", f"{uid}:{gid}",
        "-e", "HOME=/dotnet-home",
        "-e", "DOTNET_CLI_HOME=/dotnet-home",
        "-e", "DOTNET_CLI_TELEMETRY_OPTOUT=1",
        "-e", "DOTNET_NOLOGO=1",
        "-e", "NUGET_PACKAGES=/dotnet-home/.nuget/packages",
        "-v", f"{home}:/dotnet-home",
        "-v", f"{ROOT}:/repo",
        "-v", f"{STAGING_MOUNT[0]}:/staging",
        "-w", "/repo",
        image,
        "dotnet",
    ]


# Set by regenerate() before choose_runner is called, so the container can see
# the staging directory without it having to live inside the repository.
STAGING_MOUNT: list[str] = [""]


def run(command: list[str], *, what: str) -> None:
    print(f"$ {' '.join(command)}")
    result = subprocess.run(command, cwd=ROOT, check=False)
    if result.returncode != 0:
        raise SystemExit(f"{what} failed with exit code {result.returncode}")


def verify_inputs(manifest: dict) -> list[str]:
    """Every DWG is the file the manifest froze, before anything is generated."""
    problems = []
    for fixture in manifest["fixture"]:
        path = FIXTURE_DIR / fixture["file"]
        if not path.is_file():
            problems.append(f"{fixture['id']}: {path} is missing")
            continue
        data = path.read_bytes()
        if len(data) != fixture["bytes"]:
            problems.append(
                f"{fixture['id']}: {len(data)} bytes on disk, manifest says {fixture['bytes']}"
            )
        got = sha256_bytes(data)
        if got != fixture["sha256"]:
            problems.append(
                f"{fixture['id']}: input sha256 is {got}, manifest froze {fixture['sha256']}. "
                "Regenerating against bytes the manifest does not describe would freeze a "
                "reference for a file nobody vetted."
            )
    return problems


# -------------------------------------------------------------- reporting ---


def record_census(jsonl: bytes) -> dict[str, int]:
    counts: dict[str, int] = {}
    for line in jsonl.decode("utf-8").splitlines():
        if not line:
            continue
        name = json.loads(line)["record"]
        counts[name] = counts.get(name, 0) + 1
    return dict(sorted(counts.items()))


def first_differences(old: bytes, new: bytes, limit: int) -> list[str]:
    old_lines = old.decode("utf-8").splitlines()
    new_lines = new.decode("utf-8").splitlines()
    differences = []
    for index in range(max(len(old_lines), len(new_lines))):
        before = old_lines[index] if index < len(old_lines) else "<end of file>"
        after = new_lines[index] if index < len(new_lines) else "<end of file>"
        if before != after:
            differences.append(f"    line {index + 1}:\n      - {before}\n      + {after}")
            if len(differences) >= limit:
                break
    return differences


def report_difference(fixture_id: str, old: dict, new: dict, diff_lines: int) -> None:
    print(f"  {fixture_id}: CHANGED")
    print(f"    jsonl sha256 {old.get('jsonl_sha', '<none>')} -> {new['jsonl_sha']}")
    print(f"    svg   sha256 {old.get('svg_sha', '<none>')} -> {new['svg_sha']}")
    old_counts = old.get("counts", {})
    new_counts = new["counts"]
    print(
        f"    records {sum(old_counts.values()) or '<none>'} -> {sum(new_counts.values())}"
    )
    for name in sorted(set(old_counts) | set(new_counts)):
        before = old_counts.get(name, 0)
        after = new_counts.get(name, 0)
        if before != after:
            print(f"      {name}: {before} -> {after}")
    if old.get("jsonl") is not None:
        for line in first_differences(old["jsonl"], new["jsonl"], diff_lines):
            print(line)


def write_summary(entries: list[dict]) -> str:
    """The committed census, so a reviewer sees what moved without decompressing."""
    lines = [
        "# Reference output summary",
        "",
        "Written by `tools/regenerate_reference.py --accept`. Do not edit by hand:",
        "the next accepted regeneration overwrites it, and a hand edit would be a",
        "claim about the artefacts that nothing checks.",
        "",
        "Counts are canonical records, not ACadSharp entities. One block reference",
        "expands into the geometry a viewer draws, so these numbers are deliberately",
        "not the entity counts a DWG inspector would show.",
        "",
    ]
    names: list[str] = []
    for entry in entries:
        for name in entry["counts"]:
            if name not in names:
                names.append(name)
    names.sort()

    header = "| fixture | version | records | " + " | ".join(names) + " |"
    lines.append(header)
    lines.append("|" + "---|" * (3 + len(names)))
    for entry in entries:
        counts = entry["counts"]
        row = [entry["id"], entry["acad_version"], str(sum(counts.values()))]
        row += [str(counts.get(name, 0)) for name in names]
        lines.append("| " + " | ".join(row) + " |")
    lines.append("")
    return "\n".join(lines)


# -------------------------------------------------------------- commands ----


def command_regenerate(args: argparse.Namespace) -> int:
    manifest = load_manifest()
    problems = verify_inputs(manifest)
    if problems:
        for problem in problems:
            print(f"error: {problem}", file=sys.stderr)
        return 1

    acadsharp = locked_acadsharp_version()
    if acadsharp is None:
        print(f"error: {LOCK_FILE} does not lock ACadSharp", file=sys.stderr)
        return 1
    sdk = pinned_sdk_version()
    if sdk is None:
        print(f"error: {GLOBAL_JSON} does not pin an SDK version", file=sys.stderr)
        return 1
    print(f"ACadSharp {acadsharp} (from packages.lock.json), .NET SDK {sdk} (from global.json)")

    with tempfile.TemporaryDirectory(prefix="acadsharp-reference-") as staging_name:
        staging = pathlib.Path(staging_name)
        STAGING_MOUNT[0] = str(staging)
        dotnet = choose_runner(args.runner, args.image)
        in_container = dotnet[0] == "docker"
        staging_arg = "/staging" if in_container else str(staging)

        run(dotnet + ["restore", "reference/ACadSharp.Reference/ACadSharp.Reference.csproj",
                      "--locked-mode", "-v", "q"],
            what="dotnet restore --locked-mode")
        run(dotnet + ["build", "reference/ACadSharp.Reference/ACadSharp.Reference.csproj",
                      "-c", "Release", "--no-restore", "-v", "q"],
            what="dotnet build")
        run(dotnet + ["run", "--no-build", "--no-restore", "-c", "Release",
                      "--project", "reference/ACadSharp.Reference", "--",
                      "--manifest", "fixtures/manifest.toml", "--all",
                      "--output", staging_arg],
            what="the reference oracle")

        entries = []
        changed = []
        per_fixture: dict[str, dict[str, object]] = {}

        for fixture in manifest["fixture"]:
            fixture_id = fixture["id"]
            jsonl_path = staging / (fixture_id + JSONL_SUFFIX)
            svg_path = staging / (fixture_id + SVG_SUFFIX)
            if not jsonl_path.is_file() or not svg_path.is_file():
                print(f"error: {fixture_id}: the oracle produced no artefacts", file=sys.stderr)
                return 1

            jsonl = jsonl_path.read_bytes()
            svg = svg_path.read_bytes()
            counts = record_census(jsonl)
            primitives = sum(counts.get(name, 0) for name in PRIMITIVE_RECORDS)
            if primitives == 0:
                print(
                    f"error: {fixture_id}: the reference holds no primitive records. "
                    "A differential over an empty stream cannot fail, so this is refused "
                    "rather than frozen.",
                    file=sys.stderr,
                )
                return 1

            new = {
                "jsonl": jsonl,
                "jsonl_sha": sha256_bytes(jsonl),
                "svg": svg,
                "svg_sha": sha256_bytes(svg),
                "counts": counts,
            }
            entries.append({
                "id": fixture_id,
                "acad_version": fixture["acad_version"],
                "counts": counts,
            })
            per_fixture[fixture_id] = {
                "reference_jsonl": f"expected/{fixture_id}{ZST_SUFFIX}",
                "reference_jsonl_uncompressed_sha256": new["jsonl_sha"],
                "reference_svg": f"expected/{fixture_id}{SVG_SUFFIX}",
                "reference_svg_sha256": new["svg_sha"],
                "acadsharp_reference_version": acadsharp,
                "acadsharp_reference_commit": manifest["upstream"]["commit"],
                "dotnet_sdk": sdk,
                "reference_schema": REFERENCE_SCHEMA,
                "svg_schema": SVG_SCHEMA,
            }

            old = read_committed(fixture_id)
            if old.get("jsonl_sha") == new["jsonl_sha"] and old.get("svg_sha") == new["svg_sha"]:
                print(f"  {fixture_id}: unchanged ({sum(counts.values())} records)")
                continue
            changed.append(fixture_id)
            report_difference(fixture_id, old, new, args.diff_lines)

        summary = write_summary(entries)
        if SUMMARY.is_file() and SUMMARY.read_text(encoding="utf-8") != summary:
            if not changed:
                changed.append("SUMMARY.md")
                print("  SUMMARY.md: CHANGED (the artefacts did not)")
        elif not SUMMARY.is_file():
            changed.append("SUMMARY.md")

        if not changed:
            print("no differences; nothing to accept")
            return 0

        if not args.accept:
            print()
            print(
                f"{len(changed)} difference(s) and --accept was not given, so nothing was "
                "written. Read the diff above, decide whether it is the change you meant to "
                "make, then re-run with --accept."
            )
            return 1

        EXPECTED_DIR.mkdir(parents=True, exist_ok=True)
        for fixture in manifest["fixture"]:
            fixture_id = fixture["id"]
            jsonl = (staging / (fixture_id + JSONL_SUFFIX)).read_bytes()
            svg = (staging / (fixture_id + SVG_SUFFIX)).read_bytes()
            (EXPECTED_DIR / (fixture_id + ZST_SUFFIX)).write_bytes(compress(jsonl))
            (EXPECTED_DIR / (fixture_id + SVG_SUFFIX)).write_bytes(svg)
        SUMMARY.write_text(summary, encoding="utf-8")
        MANIFEST.write_text(
            rewrite_manifest(MANIFEST.read_text(encoding="utf-8"), per_fixture),
            encoding="utf-8",
        )
        print()
        print(f"accepted: wrote {len(manifest['fixture'])} reference pairs, SUMMARY.md and the manifest rows")
        return 0


def read_committed(fixture_id: str) -> dict:
    zst = EXPECTED_DIR / (fixture_id + ZST_SUFFIX)
    svg = EXPECTED_DIR / (fixture_id + SVG_SUFFIX)
    result: dict = {}
    if zst.is_file():
        try:
            jsonl = decompress(zst.read_bytes())
        except zstd.ZstdError:
            return result
        result["jsonl"] = jsonl
        result["jsonl_sha"] = sha256_bytes(jsonl)
        result["counts"] = record_census(jsonl)
    if svg.is_file():
        result["svg_sha"] = sha256_bytes(svg.read_bytes())
    return result


def command_verify(args: argparse.Namespace) -> int:
    del args
    manifest = load_manifest()
    problems = verify_inputs(manifest)

    acadsharp = locked_acadsharp_version()
    sdk = pinned_sdk_version()
    upstream_commit = manifest["upstream"]["commit"]

    checked = 0
    entries = []
    for fixture in manifest["fixture"]:
        fixture_id = fixture["id"]
        missing = [key for key in REFERENCE_KEYS if key not in fixture]
        if len(missing) == len(REFERENCE_KEYS):
            problems.append(
                f"{fixture_id}: carries no reference fields at all. Every fixture in the "
                "corpus needs a frozen reference, or the differential silently covers fewer "
                "versions than the corpus claims."
            )
            continue
        if missing:
            problems.append(f"{fixture_id}: missing reference fields {missing}")
            continue

        checked += 1
        zst = FIXTURE_DIR / fixture["reference_jsonl"]
        svg = FIXTURE_DIR / fixture["reference_svg"]

        if not zst.is_file():
            problems.append(f"{fixture_id}: {zst} is missing")
            continue
        if not svg.is_file():
            problems.append(f"{fixture_id}: {svg} is missing")
            continue

        try:
            jsonl = decompress(zst.read_bytes())
        except zstd.ZstdError as error:
            problems.append(f"{fixture_id}: {zst} does not decompress: {error}")
            continue

        got = sha256_bytes(jsonl)
        if got != fixture["reference_jsonl_uncompressed_sha256"]:
            problems.append(
                f"{fixture_id}: uncompressed JSONL sha256 is {got}, manifest froze "
                f"{fixture['reference_jsonl_uncompressed_sha256']}"
            )
        got_svg = sha256_bytes(svg.read_bytes())
        if got_svg != fixture["reference_svg_sha256"]:
            problems.append(
                f"{fixture_id}: SVG sha256 is {got_svg}, manifest froze "
                f"{fixture['reference_svg_sha256']}"
            )

        problems.extend(check_stream(fixture_id, fixture, jsonl))
        entries.append({
            "id": fixture_id,
            "acad_version": fixture["acad_version"],
            "counts": record_census(jsonl),
        })

        if acadsharp and fixture["acadsharp_reference_version"] != acadsharp:
            problems.append(
                f"{fixture_id}: says ACadSharp {fixture['acadsharp_reference_version']} but "
                f"packages.lock.json locks {acadsharp}. The reference was produced by a "
                "different package than the one a restore would install today."
            )
        if sdk and fixture["dotnet_sdk"] != sdk:
            problems.append(
                f"{fixture_id}: says SDK {fixture['dotnet_sdk']} but global.json pins {sdk}"
            )
        if fixture["acadsharp_reference_commit"] != upstream_commit:
            problems.append(
                f"{fixture_id}: acadsharp_reference_commit {fixture['acadsharp_reference_commit']} "
                f"is not upstream.commit {upstream_commit}"
            )
        if fixture["reference_schema"] != REFERENCE_SCHEMA:
            problems.append(
                f"{fixture_id}: reference_schema {fixture['reference_schema']} is not "
                f"{REFERENCE_SCHEMA}, which is what this tool understands"
            )
        if fixture["svg_schema"] != SVG_SCHEMA:
            problems.append(f"{fixture_id}: svg_schema {fixture['svg_schema']} is not {SVG_SCHEMA}")

    if checked == 0:
        problems.append(
            "no fixture carried reference fields, so every per-fixture check above ran over "
            "nothing. That is a broken verify, not a clean corpus."
        )

    if entries:
        expected_summary = write_summary(entries)
        if not SUMMARY.is_file():
            problems.append(f"{SUMMARY} is missing")
        elif SUMMARY.read_text(encoding="utf-8") != expected_summary:
            problems.append(
                f"{SUMMARY} does not match the committed artefacts. Re-run "
                "`regenerate --accept`; it writes this file."
            )

    for problem in problems:
        print(f"error: {problem}", file=sys.stderr)
    if problems:
        print(f"{len(problems)} problem(s)", file=sys.stderr)
        return 1

    print(f"verified {checked} reference pair(s) against fixtures/manifest.toml")
    return 0


def check_stream(fixture_id: str, fixture: dict, jsonl: bytes) -> list[str]:
    """The properties a reference has to have to be worth comparing against."""
    problems = []
    text = jsonl.decode("utf-8")
    if not text.endswith("\n"):
        problems.append(f"{fixture_id}: canonical JSONL has no final newline")
    if "\r" in text:
        problems.append(f"{fixture_id}: canonical JSONL carries a carriage return")

    lines = text.splitlines()
    if not lines:
        problems.append(f"{fixture_id}: canonical JSONL is empty")
        return problems

    records = []
    for number, line in enumerate(lines, start=1):
        try:
            records.append(json.loads(line))
        except json.JSONDecodeError as error:
            problems.append(f"{fixture_id}: line {number} is not JSON: {error}")
            return problems

    first = records[0]
    if first.get("record") != "document":
        problems.append(f"{fixture_id}: the first record is {first.get('record')!r}, not 'document'")
    elif first.get("acad_version") != fixture["acad_version"]:
        problems.append(
            f"{fixture_id}: the document record says {first.get('acad_version')!r} but the "
            f"manifest says {fixture['acad_version']!r}"
        )
    if any(record.get("schema") != REFERENCE_SCHEMA for record in records):
        problems.append(f"{fixture_id}: not every record carries schema {REFERENCE_SCHEMA}")
    if not any(record.get("record") == "view" for record in records):
        problems.append(f"{fixture_id}: no view record")
    if not any(record.get("record") in PRIMITIVE_RECORDS for record in records):
        problems.append(
            f"{fixture_id}: no primitive records. A differential over a stream with no "
            "geometry compares two empty sets and passes."
        )
    return problems


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    sub = parser.add_subparsers(dest="command")

    verify = sub.add_parser("verify", help="check the committed artefacts; never runs .NET")
    verify.set_defaults(handler=command_verify)

    regenerate = sub.add_parser("regenerate", help="run the oracle and compare")
    regenerate.add_argument(
        "--accept",
        action="store_true",
        help="write the regenerated artefacts and manifest rows. Without this nothing is written.",
    )
    regenerate.add_argument(
        "--runner",
        choices=("auto", "dotnet", "docker"),
        default="auto",
        help="how to invoke the SDK. auto prefers a local dotnet and says which it chose.",
    )
    regenerate.add_argument("--image", default=DEFAULT_IMAGE, help="SDK container image")
    regenerate.add_argument(
        "--diff-lines", type=int, default=10, help="how many differing JSONL lines to print"
    )
    regenerate.set_defaults(handler=command_regenerate)

    args = parser.parse_args(argv)
    if not getattr(args, "handler", None):
        args = parser.parse_args(["verify"])
    return args.handler(args)


if __name__ == "__main__":
    raise SystemExit(main())
