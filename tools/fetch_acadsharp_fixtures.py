#!/usr/bin/env python3
"""Fetch and verify the committed DWG fixture corpus.

`fixtures/manifest.toml` is the source of truth. This tool reads it and never
writes it. That is the whole point: if an upstream file's bytes change, the
verify run goes red and a human decides what that means. A tool that
"helpfully" refreshed the hash would turn a supply-chain event into a silent
diff, which is the one outcome a fixture corpus exists to prevent.

    python3 tools/fetch_acadsharp_fixtures.py verify   # offline, default
    python3 tools/fetch_acadsharp_fixtures.py fetch    # network, then verify

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
ALLOWED_URL_PREFIX = "https://raw.githubusercontent.com/DomCR/ACadSharp/"

# The largest sample upstream is ~1.3 MB. 16 MiB is generous and still refuses
# a redirect to something enormous.
MAX_BYTES = 16 * 1024 * 1024


def load_manifest() -> dict:
    with MANIFEST.open("rb") as f:
        return tomllib.load(f)


def check_pin(manifest: dict) -> list[str]:
    """The manifest itself has to be pinned before anything is fetched."""
    problems = []
    commit = manifest["upstream"].get("commit", "")
    if len(commit) != 40 or any(c not in "0123456789abcdef" for c in commit):
        problems.append(
            f"upstream.commit is {commit!r}, which is not a 40-character lowercase sha. "
            "An abbreviated or symbolic ref is not a pin."
        )
    for fx in manifest["fixture"]:
        url = fx["source_url"]
        if not url.startswith(ALLOWED_URL_PREFIX):
            problems.append(f"{fx['id']}: source_url is not on the allowlisted prefix: {url}")
        elif commit and f"/{commit}/" not in url:
            problems.append(
                f"{fx['id']}: source_url does not carry upstream.commit, so it is not pinned "
                f"to the revision the manifest claims: {url}"
            )
        for floating in ("/master/", "/main/", "/HEAD/"):
            if floating in url:
                problems.append(f"{fx['id']}: source_url uses a floating ref {floating!r}")
    return problems


def verify_bytes(fx: dict, data: bytes) -> list[str]:
    """Every check the bootstrap script did, plus the size the manifest froze."""
    problems = []
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
    if len(data) < 1024:
        problems.append(f"{fx['id']}: {len(data)} bytes is too small to be a real DWG")
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


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("command", choices=("verify", "fetch"), nargs="?", default="verify",
                    help="verify (offline, default) checks what is committed; fetch downloads first")
    args = ap.parse_args()

    manifest = load_manifest()
    problems = check_pin(manifest)
    if problems:
        for p in problems:
            print(f"error: {p}", file=sys.stderr)
        return 1

    fixtures = manifest["fixture"]
    if not fixtures:
        print("error: the manifest lists no fixtures, so there is nothing to verify", file=sys.stderr)
        return 1

    seen_sha: dict[str, str] = {}
    for fx in fixtures:
        path = ROOT / "fixtures" / fx["file"]

        if args.command == "fetch":
            found = fetch_one(fx, path)
            if found:
                for p in found:
                    print(f"error: {p}", file=sys.stderr)
                return 1
            print(f"fetched  {fx['file']}  {fx['bytes']:,} bytes")
            continue

        if not path.exists():
            print(f"error: {fx['id']}: {path.relative_to(ROOT)} is missing. "
                  "Run `fetch` to download it from the pinned commit.", file=sys.stderr)
            return 1
        found = verify_bytes(fx, path.read_bytes())
        if found:
            for p in found:
                print(f"error: {p}", file=sys.stderr)
            return 1
        if fx["sha256"] in seen_sha:
            print(f"error: {fx['id']} and {seen_sha[fx['sha256']]} have the same sha256, so one of "
                  "them is not the file it claims to be", file=sys.stderr)
            return 1
        seen_sha[fx["sha256"]] = fx["id"]
        print(f"ok       {fx['file']}  {fx['bytes']:,} bytes  {fx['acad_version']}")

    if args.command == "verify":
        # The check from the other direction. Everything above asks whether each
        # manifest entry has its file; this asks whether each file has an entry.
        # A DWG sitting in the tree with no entry has no provenance, no licence
        # and no hash, which means nobody reviewed it.
        claimed = {(ROOT / "fixtures" / fx["file"]).resolve() for fx in fixtures}
        orphans = sorted(
            str(p.relative_to(ROOT))
            for p in (ROOT / "fixtures").rglob("*.dwg")
            if p.resolve() not in claimed
        )
        if orphans:
            print(f"error: not in the manifest, so never vetted: {', '.join(orphans)}",
                  file=sys.stderr)
            return 1

    print(f"\n{len(fixtures)} fixtures, pinned to {manifest['upstream']['commit']}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
