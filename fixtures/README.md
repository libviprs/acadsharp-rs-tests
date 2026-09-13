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

## Using the corpus

```rust
for fixture in acadsharp_rs_tests::fixtures::all() {
    let bytes = fixture.read()?;
    // fixture.id, .acad_version, .sha256, .path()
}
```

The fixture layer knows about files on disk and nothing about decoding, so the
same list drives both the .NET reference oracle and `acadsharp-rs`.

## Adding or changing a fixture

1. Add a `[[fixture]]` row to `manifest.toml` with its real sha256 and size.
2. If it is a new DWG generation, add it to `EXPECTED_VERSIONS` in
   `tests/fixture_integrity.rs`. That list is written out by hand on purpose:
   deriving it from the manifest would mean deleting a fixture also deletes the
   expectation, and the suite would stay green on a shrunken corpus.
3. Run `verify`, then `cargo test`.

A fixture from anywhere other than ACadSharp needs redistribution rights, a
stable pinned URL, its licence, its sha256, and a written reason why the
upstream sample would not do. None of the current seven is in that position.
