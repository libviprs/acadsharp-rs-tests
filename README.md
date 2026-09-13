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

## How `acadsharp-rs` gets beside this one

`Cargo.toml` depends on the crate as `{ path = "../acadsharp-rs" }`, so a
checkout of it has to sit next to this one. `COUNTERPART_REV` says which commit
that is, and `.github/actions/clone-counterpart` puts it there:

```
COUNTERPART_REV          one 40-hex sha under a block of comments
  |
  v
clone-counterpart        git fetch --depth 1 origin <that sha>
  |                      checkout FETCH_HEAD, then check HEAD is that sha
  |                      compare/main...<sha> must say identical or behind
  v
../acadsharp-rs          the crate every job here builds against
```

A branch name is never allowed in that file, and neither is a repository
variable. Both shortcuts have already cost this org a wrong answer: a sync gate
resolved an unset repository variable to the empty string, which
`actions/checkout` reads as "the default branch", and compared against whatever
moved that morning; and an integration job clones a same-named counterpart
branch, so a stale branch that shares a name builds against the wrong tree
(libviprs#1013). A missing sha, an unfetchable one, or a checkout that lands
somewhere else all fail the job. There is no fallback.

**The merged-only rule**: `COUNTERPART_REV` may only name a commit that is on
`acadsharp-rs`' `main`. The action checks that through the compare API and
accepts `identical` or `behind`, because a pin naming an older commit that is
still an ancestor of `main` is merged and that is what every pin becomes as soon
as the next crate PR lands. `ahead` or `diverged` means the pin names something
that is not on `main`, and it is refused: a pin has to name something that will
still be there tomorrow, and a PR head can be force-pushed or closed.

Bumping the pin is a one-line PR. Everything the mechanism does lives in
`tools/counterpart.sh`, which CI calls and `tests/counterpart_pin.rs` drives, so
there is one implementation of each rule rather than a shell copy and a Rust
copy that drift.

### A change that needs both repos

The crate pins this suite the other way, with a `SUITE_REV` and a
`Suite (acadsharp-rs-tests)` job, and there an unmerged pin is allowed on a PR
branch. That is what makes a breaking change landable. Order it like this, and
expect the first two steps to be red:

1. Open the suite PR here. It stays red, because `COUNTERPART_REV` cannot point
   at a crate change that has not merged yet.
2. Open the crate PR with `SUITE_REV` at this PR's head. Its `Suite` job prints
   a `::notice` that the pin is not on the suite's `main` yet, and stays green.
3. The crate PR goes green and merges.
4. This PR bumps `COUNTERPART_REV` to the crate's merge commit, goes green and
   merges.
5. A one-line crate PR moves `SUITE_REV` onto this PR's merge commit.

A change that touches only one repo needs none of that, and most do not. Both
pins stay on `main` commits, and each side moves on its own.

## Licence

MIT.
