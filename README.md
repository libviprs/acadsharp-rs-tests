# acadsharp-rs-tests

The test suite and fixtures for
[`acadsharp-rs`](https://github.com/libviprs/acadsharp-rs).

They live here rather than in the crate because DWG fixtures are large and
numerous, and nobody consuming `acadsharp-rs` should pay for them. This is the
same split `libviprs` and `libviprs-tests` already use.

## Status

The DWG fixture corpus is in. Seven files, one per generation ACadSharp's
`DwgReader` documents as readable, taken from ACadSharp's own `samples/` at a
pinned commit and verified on every `cargo test`. See
[`fixtures/README.md`](fixtures/README.md) for provenance, licensing and how to
add one.

The reference side is in too: a .NET JIT oracle that reads each fixture through
ACadSharp and writes the canonical records the Rust side will later be compared
against, plus frozen expected outputs for all seven versions. Nothing in this
repository decodes a DWG in Rust yet; that is the next phase.

## The reference oracle

```
              same DWG input
                     |
      +--------------+--------------+
      |                             |
      v                             v
ACadSharp .NET (JIT)           acadsharp-rs
      |                        (not here yet)
      v                             v
canonical JSONL + SVG          actual output
      |                             |
      +----------- compare ---------+
```

`reference/ACadSharp.Reference` is an ordinary JIT console app with exactly one
dependency, ACadSharp, restored in locked mode. It shares no code with the
production NativeAOT shim or with any Rust: two implementations that share code
can share a bug and still agree, which is the one outcome a differential test
must not be able to produce.

- [`docs/CANONICAL_SCHEMA.md`](docs/CANONICAL_SCHEMA.md) is the format, field by
  field, with the ordering, float and escaping rules.
- [`reference/ACadSharp.Reference/README.md`](reference/ACadSharp.Reference/README.md)
  is how to run it and how to upgrade ACadSharp.
- `fixtures/expected/` holds the frozen artefacts, hashed into
  `fixtures/manifest.toml` alongside the input hashes.

Regenerating them:

```bash
python3 tools/regenerate_reference.py regenerate           # compares, writes nothing
python3 tools/regenerate_reference.py regenerate --accept   # writes, after you read the diff
python3 tools/regenerate_reference.py verify                # offline, no .NET
```

Without `--accept` nothing checked in is touched, and a test asserts no workflow
passes it. An ACadSharp upgrade therefore arrives as a diff somebody read.

**An ordinary `cargo test` needs none of this.** No .NET, no network, no
ACadSharp: the frozen artefacts are committed and the Rust suite checks them
against the manifest offline.

## Requirements

- **Rust 1.97+** (edition 2024)
- For the reference oracle only: the .NET SDK `reference/global.json` pins, and
  Python 3.14+ for `compression.zstd`. Neither is needed to run the Rust suite.

## Open question this repo does not answer yet

How `acadsharp-rs` gets laid down beside this one so the suite can build
against it. That is a design decision for the epic rather than something to
default into, and the org has two scars from getting it wrong: a sync gate that
resolved an unset repository variable to "the default branch" and compared
against whatever moved that morning, and an integration job that could clone a
stale same-named counterpart branch and build against the wrong tree. Whatever
lands should be a committed pin rather than a repository variable, and should
fail rather than quietly fall back.

## Licence

MIT.
