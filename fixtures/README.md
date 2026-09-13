# DWG fixture corpus

Seven DWG files, one per generation ACadSharp's `DwgReader` documents as
readable, and the baseline every differential test in this suite compares
against.

```
same DWG input
   |
   +-------------------------------+
   |                               |
   v                               v
ACadSharp .NET                 acadsharp-rs
reference implementation           |
   |                               |
   v                               v
canonical expected output      actual output
   |                               |
   +------------ compare ----------+
```

## Where they come from

All seven are ACadSharp's own `samples/*.dwg`, taken from

- repository: <https://github.com/DomCR/ACadSharp>
- version: **v3.7.1**
- commit: `d7dc111023477d8a9fffc2153139459c95b4f345`
- licence: **MIT**, reproduced in [`LICENSE-ACadSharp.txt`](LICENSE-ACadSharp.txt)

They are ACadSharp's own test inputs, which is the point: comparing our decoder
against the reference implementation is only meaningful on files that
implementation is itself exercised against.

**These are not Autodesk files and Autodesk neither owns nor endorses this
corpus.** DWG is Autodesk's format; these particular bytes are ACadSharp's test
samples, used here for compatibility and conformance testing under MIT.

## Why the commit is pinned, and not a branch

A `raw.githubusercontent.com/.../master/...` URL describes a file that can
change without any diff appearing here. Every `source_url` in
[`manifest.toml`](manifest.toml) carries the full 40-character commit above, and
both the Python tool and `tests/fixture_integrity.rs` fail if a URL ever carries
`/master/`, `/main/` or `/HEAD/`, or if `upstream.commit` stops being an exact sha.

## The manifest is the only source of truth

`manifest.toml` holds the sha256, byte count, version, upstream path and
upstream URL for every fixture. Nothing else records them: not the README, not
the Rust code, not CI. There is deliberately no second copy to drift.

It also holds the **reference** side: the paths and hashes of the frozen output
the .NET oracle produced for each input, the ACadSharp version and upstream
commit that produced it, the exact .NET SDK the accepted run used, and the schema
versions of both artefacts. Same file, same reason.

## What lives under `expected/`

Two artefacts per fixture, written by `tools/regenerate_reference.py --accept`:

- `<id>.reference.jsonl.zst`, the authoritative semantic reference, and
- `<id>.reference.svg`, a secondary visual one.

Plus `SUMMARY.md`, a committed census of record counts so a reviewer of a later
change sees what moved without decompressing anything.

The sha256 the manifest pins for the JSONL is of the **uncompressed** stream.
Zstandard frames are reproducible for one library version and one set of frame
parameters, and the tool pins both, but they are not a cross-implementation
guarantee; the uncompressed bytes are. See
[`../docs/CANONICAL_SCHEMA.md`](../docs/CANONICAL_SCHEMA.md).

Nothing here is a DWG, so the orphan check below only ever walks `dwg/`.

## Checking the corpus

Offline, and what CI runs:

```bash
python3 tools/fetch_acadsharp_fixtures.py verify
```

Restoring a missing or damaged fixture from the pinned commit:

```bash
python3 tools/fetch_acadsharp_fixtures.py fetch
```

`fetch` verifies the signature, size and sha256 **before** writing anything, so a
bad download never lands on disk. Neither command will ever rewrite a sha256 in
the manifest. If upstream's bytes change, the run goes red and a human decides
what that means: a tool that quietly refreshed the hash would turn a
supply-chain event into an invisible diff.

The same properties are enforced from Rust under an ordinary `cargo test`, with
no network and no .NET, in `tests/fixture_integrity.rs`.

## Regenerating the reference outputs

```bash
python3 tools/regenerate_reference.py regenerate            # compares, writes nothing
python3 tools/regenerate_reference.py regenerate --accept   # writes, after you read the diff
python3 tools/regenerate_reference.py verify                # offline, no .NET at all
```

The same rule as the fetch tool, for the same reason: without an explicit
acceptance nothing checked in is rewritten, so an ACadSharp upgrade is a diff a
person read rather than something a tool resolved.

## Using the corpus

`fixtures::load` is the only door. It checks size, the DWG signature and the
sha256 before it returns a single byte:

```rust
use acadsharp_rs_tests::fixtures;

let bytes = fixtures::load("acadsharp-ac1018")?;

for fixture in fixtures::all() {
    let bytes = fixture.load()?;
    // fixture.id, .acad_version, .sha256, .path()
}
```

That ordering matters more than it looks. A fixture whose bytes drifted should
fail while it is being read, naming itself, rather than surfacing three layers
later as an unexplained difference in a decode comparison that everyone then
debugs as a decoder bug.

There is a `read_unverified` for the one caller that has to compare the hashes
itself to report both sides, and `tests/fixture_integrity.rs` enforces that
nothing else in the suite uses it, hardcodes a path into `dwg/`, or reaches for
`include_bytes!`.

The fixture layer knows about files on disk and nothing about decoding, so the
same list drives both the .NET reference oracle and `acadsharp-rs`.

## Nothing gets in without being vetted

Two rules, both enforced rather than written down and hoped for:

- every file in `dwg/` has a manifest entry (`nothing_sits_in_the_corpus_without_a_manifest_entry`)
- every manifest entry has a file matching its size, signature and hash

A DWG sitting in the tree with no entry has no provenance, no licence and no
hash, which means nobody reviewed it. Dropping one in turns the suite red.

## Provenance review, for a fixture from anywhere else

All seven of the current files are ACadSharp's own samples under MIT, so the
review was short. Anything from elsewhere needs all of this before it lands:

1. **Redistribution rights.** A licence that actually permits it, recorded in
   the entry's `license`, with `license_evidence` pointing at the licence text
   at a pinned revision. Not the project's home page: the file, at a commit.
2. **A stable pinned URL.** Same rule as everything else here, a commit and not
   a branch. If upstream only offers a moving target, mirror it somewhere that
   does not move and say so.
3. **Its sha256 and size**, obtained from the bytes you are committing, not
   copied from an upstream checksum file.
4. **A written reason the ACadSharp samples would not do.** The corpus is
   deliberately small and the samples are what the reference implementation is
   itself tested against, so a new source needs to be buying something.

Autodesk sample files and anything extracted from a customer drawing do not
clear step 1. Do not add them.

## Tier two, not yet in

Reviewed as candidates, deliberately left out of the first pass so the corpus
stays one file per generation. Each is an ACadSharp sample at the same commit:

| upstream path | what it adds |
|---|---|
| `samples/sample_base/sample_base.dwg` | a minimal drawing, useful as a control |
| `samples/dynamic-blocks/BLOCKROTATIONPARAMETER.dwg` | dynamic blocks and block parameters |
| `samples/aec_objects/AecObjects.dwg` | AEC custom objects, the unsupported-entity path |
| `samples/geolocation/geoloc.dwg` | geodata, which is where libviprs georeferencing will land |

These come in when there is something that decodes them. Adding them now would
mean seven megabytes of files nothing reads.

## Size, and when this moves to LFS

Seven files, about 7.7 MB. Plain git handles that fine, and LFS would buy
nothing but a second thing to configure in CI and a second way for a checkout
to arrive without its fixtures.

**The threshold is 50 MB total.** Past that, move `dwg/` to git LFS rather than
letting clone times creep. Writing the number down here so nobody has to
rediscover it by arguing about it: below 50 MB, plain git, no discussion.

## Adding or changing a fixture

1. Add a `[[fixture]]` row to `manifest.toml` with its real sha256 and size.
2. If it is a new DWG generation, add it to `EXPECTED_VERSIONS` in
   `tests/fixture_integrity.rs`. That list is written out by hand on purpose:
   deriving it from the manifest would mean deleting a fixture also deletes the
   expectation, and the suite would stay green on a shrunken corpus.
3. Run `verify`, then `cargo test`.

`manifest.toml` reaches Rust through `include_str!`, and cargo decides whether
to rebuild from the file's mtime. Rewrite the manifest within the same second
as the last build and cargo will happily re-run the old one, which looks
exactly like a test that ignored your edit. `touch fixtures/manifest.toml` if a
change seems to have had no effect.

If the fixture is not an ACadSharp sample, it goes through the provenance
review above first.
