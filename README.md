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
against, plus frozen expected outputs for all seven versions. The Rust
side now decodes them too (`emit-acadsharp`, through the safe API) and the parity
run compares the two engines and the oracle.

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

- **Rust 1.99+** (edition 2024)
- For the reference oracle only: the .NET SDK `reference/global.json` pins, and
  Python 3.14+ for `compression.zstd`. Neither is needed to run the Rust suite.

## The native lane

A default `cargo test` links no native library and needs none. The native lane
is the part of the suite that does: it links the NativeAOT archive that
[`libviprs-dep`](https://github.com/libviprs/libviprs-dep) publishes and runs the
tests behind the `native` feature against it.

`NATIVE_ARTIFACT.toml` names the release and, per target (`linux-x64`,
`linux-arm64`, `musl-x64`, `musl-arm64`, `mac-arm64`), the asset and its sha256.
`tools/fetch_native.py` downloads one asset, checks the hash **before** it
extracts anything, unpacks it into a cache and prints the directory to use:

```bash
export ACADSHARP_NATIVE_DIR="$(python3 tools/fetch_native.py --target linux-x64)"
export VIPRS_REQUIRE_NATIVE=1        # a missing archive is a failure, not a skip
export ACADSHARP_REQUIRE_NATIVE=1    # the same, one layer down in the crate's build script
cargo test --features native
```

- `ACADSHARP_NATIVE_DIR` is the unpacked archive's top-level directory (the one
  with `lib/`, `include/` and `metadata/`). `acadsharp-rs`'s build script reads
  it and links `lib/`; this suite's `build.rs` adds the rpath for the test
  binaries, so nothing needs `LD_LIBRARY_PATH`.
- The cache is `$ACADSHARP_NATIVE_CACHE`, else `$XDG_CACHE_HOME/acadsharp-rs-tests/native`,
  else `~/.cache/acadsharp-rs-tests/native`. A cached archive is hashed again
  before it is reused, and a mismatch deletes it.
- A wrong hash deletes the download, extracts nothing and exits 1.
- With `VIPRS_REQUIRE_NATIVE` unset and no archive, the native tests print why
  they skipped. With it set to `1` they panic, naming `ACADSHARP_NATIVE_DIR`.
  Every native CI job sets it.

The `Native (...)` jobs in `ci.yml` do exactly this, then run
`tools/no_dotnet_proof.sh`: the same smoke binary in a `debian:bookworm-slim`
container with nothing installed, beside a control that a framework-dependent
.NET app cannot start in that container, so "no .NET needed" is something that
ran. On macOS there is no container, so the proof is weaker: the smoke runs with
a `PATH` that cannot find `dotnet`.

### Resources, cancellation and threads

Three more native files hold the crate's claims about what it frees, when it stops
and which threads may touch what: `tests/resource_semantics.rs` (a thousand open,
decode and drop loops with an RSS margin), `tests/cancellation.rs` (a cancel is
observed at the next batch boundary, counted in batches and not in time) and
`tests/threading.rs` (two decoders on two threads against the single threaded
output). `tools/lsan_cell.sh` runs the resource loops under LeakSanitizer, in the
`Native (linux-x64, lsan)` job. The crate's `docs/THREADING.md` says what each of
them does and does not establish.

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

### Emitters

Each engine is driven through its own emitter binary, which writes a schema 1
stream, a sidecar naming each record's entity, and a `run.json` with the engine
commit, the thread count and the fixture's sha. `emit-acadrust` (the `acadrust`
feature) and `emit-acadsharp` (no feature; it refuses at run time with
`Error::Unlinked` when no native archive is linked) are the two;
`docs/EMITTERS.md` says what each writes and where it differs from the oracle,
and `src/emit.rs` and `src/canonical.rs` are the harness and the writer they go
through.

### The parity run, and the gate

`tests/parity.rs` runs both emitters over the corpus, reads the oracle's frozen
output, compares every pair under the tables in `parity/TABLES.toml` and holds the
verdicts to `parity/VERDICTS.toml`; `parity/SUMMARY.md` is generated from it.
`parity/emit/` holds a digest table per native archive and target.

`tools/run-tests.sh` is the gate. `--plan` prints the three trees (this checkout,
`../acadsharp-rs`, `../acadrust`, or `--acadsharp-rs PATH` and `--acadrust PATH`)
with their shas beside the pins, the arch and the four REQUIRE variables, and
exits. Without it the script runs the suite in a stock `rust` container (trees
laid in as `git archive` of their commits, a named volume for the registry and the
native archive), and `--in-ci` runs the same lines on the host, which is what the
`Parity` job calls. `-- CMD...` runs one command in that environment instead of
the lines. On the x64 host, point docker at it:
`DOCKER_HOST=ssh://devshell@192.168.0.100 tools/run-tests.sh`. The pre-push hook
libviprs-tests installs runs this script for a push from this repo, and for a push
from `acadsharp-rs` or `acadrust` with that tree in its slot.

### The second sibling: acadrust

`ACADRUST_REV` pins libviprs/acadrust, the fork (never hakanaktt/acadrust, which
is at 0.6.4 against the fork's 0.5.5 and moves its entity API in ordinary
commits), by the same script and the same action as `COUNTERPART_REV`:
`COUNTERPART_NAME=acadrust` in `tools/counterpart.sh`, and `pin: ACADRUST_REV`
on `clone-counterpart`. It is one committed sha, fetched as a sha, verified
after checkout, and refused unless `git merge-base --is-ancestor` finds it on
the fork's `main`. `ACADRUST_REQUIRE=1` makes "I cannot tell which acadrust is
beside this suite" a failure, and CI sets it.

`Cargo.toml` depends on it as `{ path = "../acadrust", optional = true }` behind
the `acadrust` feature, so a default `cargo test` compiles nothing from it. Cargo
still reads the manifest of every path dependency at resolve time, optional or
not (rust-lang/cargo#4544), so `../acadrust` has to exist for any cargo command
to work here, exactly as `../acadsharp-rs` does. On a laptop that means the fork
beside this checkout under the name `acadrust`, which is also the directory name
the pre-push hook keys its slot on.

The suite's `Cargo.lock` is committed and CI builds with `--locked`: the
emitters are built by this suite, so this lock, and not the fork's, decides
which dependency versions a parity run resolves.

Landing order for a paired acadrust and suite change is the one above with the
fork in the crate's place: the suite PR waits, because `ACADRUST_REV` cannot
point at a fork change that has not merged, and once the fork PR merges a
one-line suite PR moves the pin. A sync of the fork from upstream is the same
thing: the sync merges into the fork's `main` first, and moving `ACADRUST_REV`
past it is its own one-line change, so a fork sync and a pin bump are one
change in two steps, never a pin that runs ahead of `main`.

## Licence

MIT.
