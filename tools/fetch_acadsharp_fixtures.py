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


def download(url: str) -> bytes:
    req = urllib.request.Request(url, headers={"User-Agent": "acadsharp-rs-tests-fixtures"})
    with urllib.request.urlopen(req, timeout=120) as resp:
        final_host = urllib.parse.urlsplit(resp.geturl()).hostname
        if final_host != ALLOWED_HOST:
            raise RuntimeError(f"redirected to unexpected host {final_host!r}")
        data = resp.read(MAX_BYTES + 1)
    if len(data) > MAX_BYTES:
        raise RuntimeError(f"response exceeds the {MAX_BYTES} byte cap")
    return data


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
            try:
                data = download(fx["source_url"])
            except (urllib.error.URLError, RuntimeError) as exc:
                print(f"error: {fx['id']}: download failed: {exc}", file=sys.stderr)
                return 1
            # Verify before writing, so a bad download never lands on disk.
            found = verify_bytes(fx, data)
            if found:
                for p in found:
                    print(f"error: {p}", file=sys.stderr)
                return 1
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(data)
            print(f"fetched  {fx['file']}  {len(data):,} bytes")
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

    print(f"\n{len(fixtures)} fixtures, pinned to {manifest['upstream']['commit']}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
