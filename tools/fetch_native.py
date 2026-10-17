#!/usr/bin/env python3
"""Fetch one libviprs-dep native archive, verify it, extract it, print where.

    python3 tools/fetch_native.py --target linux-x64
    export ACADSHARP_NATIVE_DIR="$(python3 tools/fetch_native.py --target linux-x64)"

`NATIVE_ARTIFACT.toml` is the source of truth: a release tag and, per target,
the asset name and the sha256 GitHub lists for it. This tool reads that file and
never writes it, for the same reason `fetch_acadsharp_fixtures.py` never
refreshes a fixture hash: a release asset whose bytes moved under a tag is a
supply-chain event, and a tool that quietly adopted the new hash would turn it
into a diff nobody read.

The order is the point. The download lands in the cache as a `.part` file, its
sha256 is computed while it streams, and only a match is renamed into place and
only a renamed file is ever handed to `tarfile`. A wrong hash deletes what was
downloaded, extracts nothing, prints nothing on stdout and exits 1.

stdout carries exactly one thing on success, the directory to use as
`ACADSHARP_NATIVE_DIR` (the single top-level directory inside the archive, which
holds `lib/`, `include/` and `metadata/`). Everything else goes to stderr, so
the command substitution above cannot pick up a progress line.

Exit codes: 0 done, 1 refused (hash, size, bad archive, network), 2 called wrong.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import pathlib
import re
import shutil
import sys
import tarfile
import tempfile
import tomllib
import urllib.error
import urllib.request

ROOT = pathlib.Path(__file__).resolve().parent.parent
MANIFEST = ROOT / "NATIVE_ARTIFACT.toml"

DEFAULT_BASE = "https://github.com/libviprs/libviprs-dep/releases/download"

# libviprs-dep's naming rule for an asset.
ASSET_RULE = re.compile(r"^acadsharp-(linux|musl|mac)-(x64|arm64)\.tgz$")
HEX64 = re.compile(r"^[0-9a-f]{64}$")

# The largest archive on the release today is a few tens of MiB. A body past
# this is not the archive, whatever the server says.
MAX_BYTES = 512 * 1024 * 1024


class Refused(Exception):
    """A policy outcome: exit 1."""


def log(message: str) -> None:
    print(f"fetch_native: {message}", file=sys.stderr)


def load_manifest(path: pathlib.Path) -> dict:
    with path.open("rb") as f:
        return tomllib.load(f)


def check_manifest(manifest: dict) -> dict[str, dict]:
    """The shape rules. `tests/native_artifact.rs` asserts the same ones."""
    tag = manifest.get("release_tag")
    if not isinstance(tag, str) or not tag.startswith("acadsharp-"):
        raise Refused("release_tag must be a string starting with 'acadsharp-'")
    if "/" in tag or ".." in tag:
        raise Refused(f"release_tag {tag!r} is not a plain tag name")
    targets: dict[str, dict] = {}
    for row in manifest.get("target", []):
        name = row.get("name")
        asset = row.get("asset")
        digest = row.get("sha256")
        if not isinstance(name, str) or not name:
            raise Refused(f"a target has no name: {row!r}")
        if name in targets:
            raise Refused(f"target {name!r} appears twice")
        if not isinstance(asset, str) or not ASSET_RULE.match(asset):
            raise Refused(f"target {name}: asset {asset!r} does not match acadsharp-<platform>-<cpu>.tgz")
        if asset != f"acadsharp-{name}.tgz":
            raise Refused(f"target {name}: asset {asset!r} is not named for its target")
        if not isinstance(digest, str) or not HEX64.match(digest):
            raise Refused(f"target {name}: sha256 must be 64 lowercase hex characters")
        if not isinstance(row.get("static_certified"), bool):
            raise Refused(f"target {name}: static_certified must be true or false")
        if not isinstance(row.get("runner"), str) or not row["runner"]:
            raise Refused(f"target {name}: runner must name a GitHub runner")
        if "container" in row and (not isinstance(row["container"], str) or not row["container"]):
            raise Refused(f"target {name}: container, when present, must be an image name")
        targets[name] = row
    return targets


def cache_dir(arg: str | None) -> pathlib.Path:
    if arg:
        return pathlib.Path(arg)
    env = os.environ.get("ACADSHARP_NATIVE_CACHE")
    if env:
        return pathlib.Path(env)
    base = os.environ.get("XDG_CACHE_HOME") or str(pathlib.Path.home() / ".cache")
    return pathlib.Path(base) / "acadsharp-rs-tests" / "native"


def sha256_file(path: pathlib.Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def download(url: str, part: pathlib.Path) -> str:
    """Stream `url` into `part`, returning the sha256 of what arrived."""
    h = hashlib.sha256()
    total = 0
    try:
        with urllib.request.urlopen(url, timeout=120) as response, part.open("wb") as out:
            while True:
                chunk = response.read(1 << 20)
                if not chunk:
                    break
                total += len(chunk)
                if total > MAX_BYTES:
                    raise Refused(f"{url} sent more than {MAX_BYTES} bytes, which is not this archive")
                h.update(chunk)
                out.write(chunk)
    except urllib.error.URLError as e:
        raise Refused(f"could not download {url}: {e}") from e
    return h.hexdigest()


def check_linkinfo(root: pathlib.Path, row: dict) -> None:
    """The manifest copies what the archive says about itself; they must agree."""
    try:
        info = json.loads((root / "metadata" / "LINKINFO.json").read_text())
    except (OSError, ValueError) as e:
        raise Refused(f"{row['asset']}: metadata/LINKINFO.json is not readable JSON: {e}") from e
    certified = info.get("static_certified")
    if certified != row["static_certified"]:
        raise Refused(
            f"{row['asset']}: LINKINFO.json says static_certified={certified!r} and "
            f"NATIVE_ARTIFACT.toml says {row['static_certified']!r}. The matrix is planned from the "
            "manifest, so a disagreement would give the wrong cells."
        )


def extract(archive: pathlib.Path, dest: pathlib.Path) -> pathlib.Path:
    """Extract into `dest` and return the single top-level directory."""
    dest.mkdir(parents=True)
    try:
        with tarfile.open(archive, "r:gz") as tar:
            # `data` refuses absolute names, `..`, device nodes and links that
            # point outside, which is what an archive from the network gets.
            # Python before 3.11.4 (and 3.12.0 betas) has no filters, so the
            # same refusals are written out for that case rather than skipped.
            if hasattr(tarfile, "data_filter"):
                tar.extractall(dest, filter="data")
            else:
                for member in tar.getmembers():
                    target = (dest / member.name).resolve()
                    if not target.is_relative_to(dest.resolve()):
                        raise Refused(f"{archive.name} has a member outside the directory: {member.name!r}")
                    if member.isdev() or member.issym() or member.islnk():
                        raise Refused(f"{archive.name} has a link or device member: {member.name!r}")
                tar.extractall(dest)
    except (tarfile.TarError, OSError) as e:
        shutil.rmtree(dest, ignore_errors=True)
        raise Refused(f"{archive.name} is not an archive I can unpack: {e}") from e
    entries = sorted(p for p in dest.iterdir())
    if len(entries) != 1 or not entries[0].is_dir():
        shutil.rmtree(dest, ignore_errors=True)
        raise Refused(f"{archive.name} should hold one top-level directory, and holds {[p.name for p in entries]}")
    root = entries[0]
    if not (root / "metadata" / "LINKINFO.json").is_file():
        shutil.rmtree(dest, ignore_errors=True)
        raise Refused(f"{archive.name} has no metadata/LINKINFO.json, so it is not a libviprs-dep archive")
    return root


def fetch(target: str, manifest_path: pathlib.Path, cache: pathlib.Path, base: str) -> pathlib.Path:
    manifest = load_manifest(manifest_path)
    targets = check_manifest(manifest)
    if target not in targets:
        raise Refused(f"no target {target!r} in {manifest_path.name}; it has {sorted(targets)}")
    row = targets[target]
    tag, asset, want = manifest["release_tag"], row["asset"], row["sha256"]

    home = cache / tag
    home.mkdir(parents=True, exist_ok=True)
    archive = home / asset
    extracted = home / f"{target}-{want[:12]}"

    if archive.is_file() and sha256_file(archive) != want:
        # A cached file that no longer matches is not trusted and not kept.
        log(f"the cached {asset} does not match {want[:12]}, removing it")
        archive.unlink()
        shutil.rmtree(extracted, ignore_errors=True)

    if not archive.is_file():
        url = f"{base.rstrip('/')}/{tag}/{asset}"
        log(f"downloading {url}")
        part = home / f"{asset}.part"
        try:
            got = download(url, part)
        except BaseException:
            part.unlink(missing_ok=True)
            raise
        if got != want:
            part.unlink(missing_ok=True)
            raise Refused(
                f"sha256 mismatch for {asset}: NATIVE_ARTIFACT.toml says {want}, the download is {got}. "
                "The download was deleted and nothing was extracted."
            )
        part.replace(archive)

    if extracted.is_dir():
        tops = [p for p in extracted.iterdir() if p.is_dir()]
        if len(tops) == 1:
            log(f"using the extracted archive already in {extracted}")
            check_linkinfo(tops[0], row)
            return tops[0].resolve()
        shutil.rmtree(extracted)

    # Extract beside the final name and rename, so an interrupted extraction
    # never leaves a directory that looks finished.
    staging = pathlib.Path(tempfile.mkdtemp(prefix=f".{target}.", dir=home))
    try:
        extract(archive, staging / "x")
        (staging / "x").replace(extracted)
    finally:
        shutil.rmtree(staging, ignore_errors=True)
    top = [p for p in extracted.iterdir() if p.is_dir()][0]
    try:
        check_linkinfo(top, row)
    except Refused:
        shutil.rmtree(extracted, ignore_errors=True)
        raise
    return top.resolve()


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--target", required=True, help="a [[target]] name from NATIVE_ARTIFACT.toml")
    parser.add_argument("--manifest", default=str(MANIFEST), help="the manifest to read")
    parser.add_argument("--cache", help="cache directory (default: $ACADSHARP_NATIVE_CACHE or ~/.cache)")
    parser.add_argument("--base-url", default=DEFAULT_BASE, help="release download root; for tests")
    try:
        args = parser.parse_args(argv)
    except SystemExit as e:
        return 2 if e.code else 0
    try:
        out = fetch(args.target, pathlib.Path(args.manifest), cache_dir(args.cache), args.base_url)
    except Refused as e:
        log(f"refused: {e}")
        return 1
    except (OSError, tomllib.TOMLDecodeError) as e:
        log(f"refused: {e}")
        return 1
    print(out)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
