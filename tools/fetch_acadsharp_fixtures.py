#!/usr/bin/env python3
"""Fetch and verify the committed DWG fixture corpus.

`fixtures/manifest.toml` is the source of truth. This tool reads it and never
writes it. That is the whole point: if an upstream file's bytes change, the
verify run goes red and a human decides what that means. A tool that
"helpfully" refreshed the hash would turn a supply-chain event into a silent
diff, which is the one outcome a fixture corpus exists to prevent.

    python3 tools/fetch_acadsharp_fixtures.py verify   # offline, default
    python3 tools/fetch_acadsharp_fixtures.py fetch    # network, then verify
    python3 tools/fetch_acadsharp_fixtures.py view <dir> --source libviprs_dep

`verify` covers every row of every table, not only the drawings: `[[fixture]]`
(a DWG the oracle reads, with a `tier`), `[[refusal]]` (a DWG the oracle is
expected to refuse), `[[recording]]` (libviprs-dep's `.txt` recordings),
`[[oracle_input]]` (AutoCAD's DXF twins) and `[[census]]` (its recorded object
census). Every row names its `source`, and each source has its own pinned
commit and host prefix, so a row cannot borrow another source's pin.

`view` lays down the tree acadrust's conformance harness reads as
`ACADSHARP_CORPUS`: `fixtures/<stem>.dwg` beside `expectations/<stem>.txt`,
all symlinks into this corpus, nothing copied twice.

Exits non-zero on any discrepancy.
"""

from __future__ import annotations

import argparse
import hashlib
import pathlib
import sys
import tomllib
import urllib.error
import urllib.parse
import urllib.request

ROOT = pathlib.Path(__file__).resolve().parent.parent
MANIFEST = ROOT / "fixtures" / "manifest.toml"

# A fixture is a DWG from one host, pinned to one commit. Anything else is a
# bug in the manifest rather than something to download.
ALLOWED_HOST = "raw.githubusercontent.com"
ALLOWED_URL_ROOT = "https://raw.githubusercontent.com/"
ALLOWED_URL_PREFIX = "https://raw.githubusercontent.com/DomCR/ACadSharp/"

# The largest sample upstream is ~1.3 MB. 16 MiB is generous and still refuses
# a redirect to something enormous.
MAX_BYTES = 16 * 1024 * 1024


def load_manifest() -> dict:
    with MANIFEST.open("rb") as f:
        return tomllib.load(f)


TABLES = ("fixture", "refusal", "recording", "oracle_input", "census")

# The tables whose files are DWG drawings, and so carry a six-byte signature.
DRAWING_TABLES = frozenset({"fixture", "refusal"})

TIERS = frozenset({"A", "B", "C"})

FLOATING = ("/master/", "/main/", "/HEAD/")


def is_sha(value: object) -> bool:
    return isinstance(value, str) and len(value) == 40 and all(c in "0123456789abcdef" for c in value)


def sources_of(manifest: dict) -> dict:
    """The per-source pins. A manifest with no `[sources.*]` is the original
    single-source shape: ACadSharp at `[upstream].commit`."""
    declared = manifest.get("sources")
    if declared:
        return declared
    return {
        "acadsharp": {
            "commit": manifest.get("upstream", {}).get("commit", ""),
            "url_prefix": ALLOWED_URL_PREFIX,
        }
    }


def rows(manifest: dict) -> list[dict]:
    """Every file row of every table, each tagged with the table it came from."""
    out = []
    for table in TABLES:
        for row in manifest.get(table, []):
            out.append(dict(row, _table=table))
    return out


def check_pin(manifest: dict) -> list[str]:
    """The manifest itself has to be pinned before anything is fetched.

    Per source: every row names its source, the source has a full commit sha and
    a URL prefix, and the row's URL starts with that prefix and carries that
    commit. A libviprs-dep row cannot be satisfied by ACadSharp's pin, or the
    other way round.
    """
    problems = []
    commit = manifest.get("upstream", {}).get("commit", "")
    if not is_sha(commit):
        problems.append(
            f"upstream.commit is {commit!r}, which is not a 40-character lowercase sha. "
            "An abbreviated or symbolic ref is not a pin."
        )
    sources = sources_of(manifest)
    for name, src in sources.items():
        if not is_sha(src.get("commit", "")):
            problems.append(
                f"sources.{name}.commit is {src.get('commit')!r}, which is not a 40-character "
                "lowercase sha. An abbreviated or symbolic ref is not a pin."
            )
        if not str(src.get("url_prefix", "")).startswith(ALLOWED_URL_ROOT):
            problems.append(
                f"sources.{name}.url_prefix {src.get('url_prefix')!r} is not on {ALLOWED_URL_ROOT}"
            )
    single_source = not manifest.get("sources")
    for row in rows(manifest):
        rid = row.get("id", row.get("file", "?"))
        name = row.get("source", "acadsharp" if single_source else None)
        if name is None:
            problems.append(f"{rid}: the row names no source, so there is no pin to hold it to")
            continue
        src = sources.get(name)
        if src is None:
            problems.append(f"{rid}: source {name!r} is not defined by any [sources.*] table")
            continue
        for key in ("source_url", "license_evidence"):
            url = row.get(key)
            if url is None:
                if key == "source_url":
                    problems.append(f"{rid}: no source_url")
                continue
            prefix = src.get("url_prefix", "")
            if not url.startswith(prefix):
                problems.append(f"{rid}: {key} is not on the allowlisted prefix for {name}: {url}")
            elif src.get("commit") and f"/{src['commit']}/" not in url:
                problems.append(
                    f"{rid}: {key} does not carry {name}'s commit ({src['commit']}), so it is not "
                    f"pinned to the revision the manifest claims: {url}"
                )
            for floating in FLOATING:
                if floating in url:
                    problems.append(f"{rid}: {key} uses a floating ref {floating!r}")
        for url in row.get("also_source_urls", []):
            if not url.startswith(src.get("url_prefix", "")) or f"/{src.get('commit')}/" not in url:
                problems.append(f"{rid}: also_source_urls entry is not pinned to {name}: {url}")
    return problems


def verify_bytes(fx: dict, data: bytes) -> list[str]:
    """Every check the bootstrap script did, plus the size the manifest froze."""
    problems = []
    is_drawing = fx.get("_table", "fixture") in DRAWING_TABLES
    if is_drawing:
        want_magic = fx["acad_version"].encode("ascii")
        got_magic = data[:6]
        if got_magic != want_magic:
            # Catches an HTML error page, a DXF renamed to .dwg, and a truncated file.
            problems.append(
                f"{fx['id']}: first six bytes are {got_magic!r}, expected {want_magic!r}. "
                "That is not a DWG of the version this fixture claims."
            )
    if len(data) != fx["bytes"]:
        problems.append(f"{fx['id']}: {len(data)} bytes, manifest says {fx['bytes']}")
    # Only a drawing the oracle reads has to look like a real one. A refusal is
    # often a stub: the AC1009 file is 512 bytes and is refused for its version.
    if fx.get("_table", "fixture") == "fixture" and len(data) < 1024:
        problems.append(f"{fx['id']}: {len(data)} bytes is too small to be a real DWG")
    if not is_drawing and not data:
        problems.append(f"{fx['id']}: the file is empty")
    got_sha = hashlib.sha256(data).hexdigest()
    if got_sha != fx["sha256"]:
        problems.append(
            f"{fx['id']}: sha256 {got_sha} does not match the manifest's {fx['sha256']}. "
            "This tool will not update the manifest; decide what the change means first."
        )
    return problems


def download(url: str, *, allowed_host: str = ALLOWED_HOST, max_bytes: int = MAX_BYTES) -> bytes:
    """Bytes from `url`, refusing an off-host redirect or an oversized body.

    `allowed_host` and `max_bytes` default to the production values and are
    parameters only so `tools/tests/` can drive the same code against a local
    stub server. Nothing in this file passes anything else.
    """
    req = urllib.request.Request(url, headers={"User-Agent": "acadsharp-rs-tests-fixtures"})
    with urllib.request.urlopen(req, timeout=120) as resp:
        # Checked after the redirect chain has been followed, not before: the
        # allowlist is about where the bytes actually came from.
        final_host = urllib.parse.urlsplit(resp.geturl()).hostname
        if final_host != allowed_host:
            raise RuntimeError(f"redirected to unexpected host {final_host!r}")
        # One byte past the cap, so a body sitting exactly on it is still
        # distinguishable from one that ran over.
        data = resp.read(max_bytes + 1)
    if len(data) > max_bytes:
        raise RuntimeError(f"response exceeds the {max_bytes} byte cap")
    return data


def fetch_one(fx: dict, dest: pathlib.Path, *, allowed_host: str = ALLOWED_HOST,
              max_bytes: int = MAX_BYTES) -> list[str]:
    """Download one fixture and write it only if it verifies.

    The ordering is the point. Verifying after the write leaves a corrupt file
    on disk for the next run to trip over, and a fetch that half-succeeded is
    harder to reason about than one that did nothing.
    """
    try:
        data = download(fx["source_url"], allowed_host=allowed_host, max_bytes=max_bytes)
    except (urllib.error.URLError, RuntimeError, OSError) as exc:
        return [f"{fx['id']}: download failed: {exc}"]
    problems = verify_bytes(fx, data)
    if problems:
        return problems
    dest.parent.mkdir(parents=True, exist_ok=True)
    dest.write_bytes(data)
    return []


def verify_tree(manifest: dict, root: pathlib.Path) -> list[str]:
    """Everything `verify` checks, against the tree under `root/fixtures`.

    Returned rather than printed so it can be driven against a corpus built to
    be wrong: each row's file exists and is the file the row froze, no two rows
    hold the same bytes, every cross-reference lands on a row that exists, every
    drawing has exactly one tier, and nothing sits in the tree without a row.
    """
    problems = []
    fixtures_dir = root / "fixtures"
    all_rows = rows(manifest)
    if not all_rows:
        return ["the manifest lists no files, so there is nothing to verify"]

    ids = {}
    for row in all_rows:
        rid = row.get("id", "?")
        if rid in ids.setdefault(row["_table"], set()):
            problems.append(f"{rid}: two {row['_table']} rows share an id")
        ids[row["_table"]].add(rid)

    seen_sha: dict[str, str] = {}
    claimed = set()
    for row in all_rows:
        path = fixtures_dir / row["file"]
        claimed.add(path.resolve())
        if not path.is_file():
            problems.append(f"{row['id']}: {row['file']} is missing. Run `fetch` to download it "
                            "from the pinned commit.")
            continue
        problems.extend(f"{p} [{row['file']}]" for p in verify_bytes(row, path.read_bytes()))
        if row["sha256"] in seen_sha:
            problems.append(
                f"{row['id']} and {seen_sha[row['sha256']]} have the same sha256, so a file is "
                "stored twice or one of them is not the file it claims to be"
            )
        seen_sha[row["sha256"]] = row["id"]

    drawing_ids = ids.get("fixture", set())
    refusal_ids = ids.get("refusal", set())
    for row in manifest.get("fixture", []):
        tier = row.get("tier")
        if tier not in TIERS:
            problems.append(f"{row['id']}: tier is {tier!r}; a drawing has exactly one of A, B or C")
    for row in manifest.get("refusal", []):
        if not isinstance(row.get("oracle_exit_code"), int) or not row.get("oracle_exit"):
            problems.append(f"{row['id']}: a refusal names the oracle_exit and oracle_exit_code it expects")
    for row in manifest.get("recording", []):
        if row.get("drawing") not in drawing_ids | refusal_ids:
            problems.append(
                f"{row['id']}: records the drawing {row.get('drawing')!r}, which is neither a "
                "[[fixture]] nor a [[refusal]]"
            )
    for row in manifest.get("oracle_input", []):
        twin = row.get("for_fixture")
        if twin is not None and twin not in drawing_ids:
            problems.append(f"{row['id']}: is the twin of {twin!r}, which is not a [[fixture]]")
    for row in manifest.get("census", []):
        for twin in row.get("for_fixtures", []):
            if twin not in drawing_ids:
                problems.append(f"{row['id']}: describes {twin!r}, which is not a [[fixture]]")

    # The check from the other direction. Everything above asks whether each row
    # has its file; this asks whether each file has a row. A file in the tree
    # with no row has no provenance, no licence and no hash, which means nobody
    # reviewed it. Any extension, any directory. The documents, the licence
    # texts and `expected/` (owned by regenerate_reference.py) need no row.
    # The ezdxf ground truths are claimed by their own rows (the hashes live in
    # the row, and `tools/ground_truth/extract_ezdxf.py verify` checks them).
    for gt in manifest.get("ground_truth", []):
        for key in ("jsonl", "sidecar", "run"):
            claimed.add((fixtures_dir / gt[key]).resolve())
    if fixtures_dir.is_dir():
        for p in sorted(fixtures_dir.rglob("*")):
            if not p.is_file():
                continue
            rel = p.relative_to(fixtures_dir).as_posix()
            if rel in ("manifest.toml", "README.md") or (rel.startswith("LICENSE") and rel.endswith(".txt")):
                continue
            if rel.startswith("expected/"):
                continue
            if p.resolve() not in claimed:
                problems.append(f"fixtures/{rel} is not in the manifest, so it was never vetted")
    return problems


def build_view(manifest: dict, root: pathlib.Path, dest: pathlib.Path, source: str) -> list[str]:
    """Lay down `<dest>/fixtures/<stem>.dwg` beside `<dest>/expectations/<stem>.txt`.

    acadrust's harness wants those two directories with equal stems, and
    libviprs-dep's stems (`real_AC1018`) are not this corpus's file names
    (`sample_AC1018.dwg`). So the view is built from the manifest: every drawing
    and refusal of `source`, under its file's stem, and every recording of
    that source under its own file stem, linked to the drawing it records, in the
    spelling the recording uses. All symlinks; nothing is copied twice.

    A corpus that does not verify gets no view: a view over damaged bytes would
    hand the harness a tally about a corpus nobody froze.
    """
    sources = sources_of(manifest)
    if source not in sources:
        return [f"source {source!r} is not defined; the sources are {sorted(sources)}"]
    problems = verify_tree(manifest, root)
    if problems:
        return problems
    if dest.exists() and any(dest.iterdir()):
        return [f"{dest} is not empty, so I am not laying a view down over it"]

    fixtures_dir = root / "fixtures"
    drawings = {r["id"]: r for r in manifest.get("fixture", []) + manifest.get("refusal", [])}
    links: dict[str, pathlib.Path] = {}
    exp_links: dict[str, pathlib.Path] = {}

    for row in manifest.get("fixture", []) + manifest.get("refusal", []):
        if row.get("source") == source:
            links[f"{pathlib.PurePosixPath(row['file']).stem}.dwg"] = fixtures_dir / row["file"]
    for rec in manifest.get("recording", []):
        if rec.get("source") != source:
            continue
        stem = pathlib.PurePosixPath(rec["file"]).stem
        drawing = drawings[rec["drawing"]]
        # The recording's own spelling of the stem, which is not the drawing's
        # id when the drawing is a Tier A row shared with another source.
        links.setdefault(f"{stem}.dwg", fixtures_dir / drawing["file"])
        exp_links[f"{stem}.txt"] = fixtures_dir / rec["file"]

    (dest / "fixtures").mkdir(parents=True, exist_ok=True)
    (dest / "expectations").mkdir(parents=True, exist_ok=True)
    for name, target in sorted(links.items()):
        (dest / "fixtures" / name).symlink_to(target.resolve())
    for name, target in sorted(exp_links.items()):
        (dest / "expectations" / name).symlink_to(target.resolve())
    return []


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("command", choices=("verify", "fetch", "view"), nargs="?", default="verify",
                    help="verify (offline, default) checks what is committed; fetch downloads "
                         "first; view <dir> lays down acadrust's corpus layout")
    ap.add_argument("dest", nargs="?", help="view: the directory to lay the tree down in")
    ap.add_argument("--source", default="libviprs_dep", help="view: which source's rows to include")
    args = ap.parse_args()

    manifest = load_manifest()
    problems = check_pin(manifest)
    if problems:
        for p in problems:
            print(f"error: {p}", file=sys.stderr)
        return 1

    all_rows = rows(manifest)
    if not all_rows:
        print("error: the manifest lists no files, so there is nothing to verify", file=sys.stderr)
        return 1

    if args.command == "view":
        if not args.dest:
            print("error: view needs a destination directory", file=sys.stderr)
            return 2
        found = build_view(manifest, ROOT, pathlib.Path(args.dest), args.source)
        for p in found:
            print(f"error: {p}", file=sys.stderr)
        if not found:
            print(f"view of source {args.source} laid down in {args.dest}")
        return 1 if found else 0

    if args.command == "fetch":
        sources = sources_of(manifest)
        for row in all_rows:
            path = ROOT / "fixtures" / row["file"]
            # A generated drawing is not downloadable: its source_url names the
            # code that wrote it, and `verify_tree` below holds the committed
            # bytes to the row's hash. The generator is what reproduces it.
            if sources.get(row.get("source"), {}).get("generated"):
                print(f"generated  {row['file']}  {row['bytes']:,} bytes (not fetched)")
                continue
            found = fetch_one(row, path)
            # The same blob is served under other upstream names; each of them
            # has to serve the same bytes, or the "also" is a claim nothing holds.
            for url in row.get("also_source_urls", []):
                try:
                    other = download(url)
                except (urllib.error.URLError, RuntimeError, OSError) as exc:
                    found.append(f"{row['id']}: download of {url} failed: {exc}")
                    continue
                if hashlib.sha256(other).hexdigest() != row["sha256"]:
                    found.append(f"{row['id']}: {url} serves different bytes than the row froze")
            if found:
                for p in found:
                    print(f"error: {p}", file=sys.stderr)
                return 1
            print(f"fetched  {row['file']}  {row['bytes']:,} bytes")
        del sources

    found = verify_tree(manifest, ROOT)
    for p in found:
        print(f"error: {p}", file=sys.stderr)
    if found:
        return 1
    counts = ", ".join(f"{len(manifest.get(t, []))} {t}" for t in TABLES)
    print(f"{len(all_rows)} files ({counts}), pinned per source: "
          + ", ".join(f"{n}@{s['commit'][:8]}" for n, s in sources_of(manifest).items()))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
