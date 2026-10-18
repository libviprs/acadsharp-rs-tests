# DWG fixture corpus

One corpus for every engine that reads it: the seven AutoCAD saves (one DWG
per generation ACadSharp's `DwgReader` documents as readable), fifteen more
AutoCAD-authored drawings, AutoCAD's own DXF twins and object census, and
libviprs-dep's synthetic drawings with their recordings. The baseline every
differential test in this suite compares against.

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

## Tiers and tables

`manifest.toml` has one table per role. A `[[fixture]]` row is a DWG the oracle
reads and freezes, and has exactly one `tier`:

| tier | what | rows | source |
|---|---|---|---|
| A | decision grade, AutoCAD-authored: the seven saves, `sample_base`, `empty`, the ten `dynamic-blocks/BLOCK*`, `AecObjects`, `geoloc`, `export_sample` | 22 | ACadSharp `samples/` |
| B | regression grade: libviprs-dep's synthetics, written by ACadSharp's own `DwgWriter`, and the seventeen drawings this repository generates (the seven regression classes below, and ten parity families for the upstream tests) | 58 (41 + 17) | libviprs-dep, and `generated` |
| C | stress: `g13_many_inserts`, `g13_dimension_deep`, `g13_scale_4x`, `g13_scale_16x`, `g13_wide_polyline`, `g13_wide_spline`, `g13_xref_long`. Both engines open them and agree on counts; timing them is libviprs-bench's job | 7 | libviprs-dep |

The other tables have the same provenance fields: `[[refusal]]` (2 drawings the
oracle refuses, `g13_ac1009` with `UnsupportedVersion` and `g13_nan_bulge` with
`CanonicalSerializationFailure`, each with the exit code it is run against),
`[[recording]]` (45 libviprs-dep `.txt` recordings, each naming the drawing it
records), `[[oracle_input]]` (26 DXF files: AutoCAD's own twins of the saves and
of the dynamic blocks and geolocation) and `[[census]]` (4 distinct blobs of
AutoCAD's recorded document trees; five of the seven per-generation trees are
the same bytes, so one row lists the other upstream names as `also_source_*`
and `fetch` checks each serves the same bytes).

libviprs-dep's `real_AC1018` and `real_AC1032` are byte for byte `sample_AC1018`
and `sample_AC1032`. They are not rows of their own: the two recordings of them
point at the Tier A rows, so nothing is stored twice.

A drawing that is meant to hold no geometry (an empty drawing, a fixture whose
only entity is a kind schema 1 has no record for) carries
`no_primitives = "<reason>"`. Without it a reference holding no primitive record
is refused, because a differential over nothing cannot fail; with it the stream
must still hold a warning, and the row is refused if the stream turns out to hold
geometry after all.

Every row names its `source`; `[sources.acadsharp]`, `[sources.libviprs_dep]` and
`[sources.generated]` each pin one commit and one URL prefix, and a row cannot
borrow another's.

## Where they come from

The seven saves are ACadSharp's own `samples/*.dwg`, and so is Tier A, taken from

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

`expected/` is owned by `tools/regenerate_reference.py`; every other file under
`fixtures/` has to be a row.

## Checking the corpus

Offline, and what CI runs:

```bash
python3 tools/fetch_acadsharp_fixtures.py verify
```

`verify` covers every row of every table. Restoring a missing or damaged file
from the pinned commit:

```bash
python3 tools/fetch_acadsharp_fixtures.py fetch
```

The tree acadrust's conformance harness reads (`fixtures/<stem>.dwg` beside
`expectations/<stem>.txt`, symlinks into this corpus, nothing copied twice):

```bash
python3 tools/fetch_acadsharp_fixtures.py view /tmp/corpus --source libviprs_dep
ACADSHARP_CORPUS=/tmp/corpus  # in the fork
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

- every file under `fixtures/` (any extension, any directory) has a manifest
  row (`nothing_sits_in_the_corpus_without_a_manifest_entry`)
- every manifest row has a file matching its size, signature (for a drawing)
  and hash

A file sitting in the tree with no row has no provenance, no licence and no
hash, which means nobody reviewed it. Dropping one in turns the suite red, and so
does deleting any one of the files a row names. The expected row count per table
and per tier is written out by hand in `tests/fixture_integrity.rs`, so a
shrinking manifest is red too.

## Provenance review, for a fixture from anywhere else

The ACadSharp samples are all under MIT, so the
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

## Size, and when this moves to LFS

About 52.8 MiB in all (55.4 MB, after the code page drawings; see the threshold below): the 26 DXF twins are 25 MB of it, the drawings 12 MB,
the census files 1.2 MB, the references under `expected/` 3 MB (the oracle's
readings of the DXF twins included), the ezdxf ground truths 3.5 MB, and the 22
outputs ACadSharp wrote for the round-trip matrix with the readings of all 48 under
`written/` 4 MB, and the 112 code page drawings with their readings under `authored/` 1.5 MB. Plain git handles that, and LFS would buy nothing but a second thing
to configure in CI and a second way for a checkout to arrive without its fixtures.
Vendoring all of it is what keeps `fixture_integrity.rs` from needing the network or
skipping.

The round-trip matrix is why this moved: ACadSharp's outputs cannot be made again
without .NET, so they are stored, and acadrust's (14 MB) are not, because its writer is
deterministic and the test makes them. Stage two of the matrix (libviprs/acadsharp-rs-tests#41)
must not store its outputs here; it has about fifty sources.

**The threshold is 50 MB total, and the tree is past it.** The code page drawings (R5.4, libviprs/acadsharp-rs-tests#30) took it from 49.2 MiB to 52.8 MiB. The rule below says to ask first whether the DXF twins (25 MB) need vendoring at all before moving anything to LFS, so I have left that question open for a person rather than moving files in a fixtures PR. The number was written without saying which megabyte; this counts it in MiB (49.2 MiB now, 51.5 MB in decimal), and the PR that moved the tree past 50 decimal MB says so. Past that, move the drawings to git LFS rather
than letting clone times creep, and ask first whether the DXF twins need to be
vendored at all. Writing the number down here so nobody has to
rediscover it by arguing about it: below 50 MB, plain git, no discussion.

## Adding or changing a fixture

1. Add a row to the right table of `manifest.toml` with its real sha256 and
   size, its `source` and (for a drawing) its `tier`.
2. If it is a new DWG generation, add it to `EXPECTED_VERSIONS` in
   `tests/fixture_integrity.rs`, and move the expected count for its table or
   tier there. That list is written out by hand on purpose:
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

## Regression fixtures: the classes ACadSharp's tracker keeps returning to

Seven drawings, generated by `reference/ACadSharp.Generator` (source
`generated`, tier B, written by ACadSharp 3.7.1 as AC1032), each with a claim
test in `tests/regression_claims.rs` over its committed expected output. The
claim tests need no .NET and no native library: they fail if a fixture stops
exercising its class, for example because the generator or ACadSharp changed.

| class | what the generator writes | what the claim test holds the expected output to |
|---|---|---|
| `bulge` | an LWPOLYLINE of three bulge segments (chord 10, sagitta 2, 5 and -1.25) | one `polyline` whose bulges are `2 * sagitta / chord`, exactly three non-zero, and no `line` and no `arc` |
| `insert_transform` | a block inserted at (100, 50) with scale (2, 3), 30 degrees, a second block nested one level inside | one `line` whose endpoints equal the hand-computed transformed coordinates |
| `text_zero_height` | a TEXT and an MTEXT of height 0, and a 6000-character MTEXT | the long string intact in one `text` record, and exactly two `reader_error` warnings for the two zero-height entities |
| `dimension_fields` | one dimension of each of the seven kinds | seven `text` records: `10`, `5`, `4`, `8`, `90°`, `60°`, `120` |
| `hatch_loops` | a hatch with an outer loop and two islands, each a closed polyline path | three closed `polyline` records, the islands strictly inside the outer loop and apart from each other, no `line` |
| `unsupported_entity` | a LINE, a MESH and a CIRCLE | a line and a circle, and exactly one `unsupported_entity` warning naming `Mesh` on top of the warnings every generated drawing has |
| `large_entity_count` | 50,000 lines on a 250-column grid | 50,000 `line` records, each at its grid position |

A new class needs a claim test, and a negative control for it that breaks the
stream and expects the claim to fail, before its fixture is accepted. A claim
that cannot fail is a claim about nothing.

Where this differs from the issue text (#12), and why:

- **Layout.** Inputs are under `generated/dwg/`, not `inputs/`, beside the
  other tiers' `dwg/` directories, and each is a `[[fixture]]` row so the oracle
  and `tools/regenerate_reference.py` handle it unchanged.
- **`bulge` and `hatch_loops`.** The issue asks for three `arc` records and for one
  `polygon` with three rings. Schema 1 keeps a polyline's bulges and draws a
  hatch as one record per boundary edge, so the same claims are stated in those
  terms (above).
- **`text_zero_height`.** The issue asks for a `text` record per entity with the
  zero height preserved. ACadSharp 3.7.1 cannot do that: its writer writes a zero
  height and its reader throws reading it back. The fixture pins the loss, as two
  warnings, instead of pretending otherwise.
- **`dimension_fields`.** `UpdateBlock` draws no text for the two angular kinds and
  formats an ordinate's 120 as `35°`; the generator authors those three texts
  (see `DimensionFields.cs`). `Measurement` has no setter, so a "measured value"
  is the points that give it.
- **`large_entity_count`.** The streaming half (RSS during a native decode) is
  `tests/native_streaming.rs`, with `native_streaming_control.rs` as its control.
  It asserts growth *after a warm-up of twelve batches* is under four batches,
  not growth from the post-open RSS: measured, the first decode ramps about
  10 MiB over ten batches and is then flat, so a post-open bound would measure
  the runtime warming up and not the stream.

Nothing here claims the crate fixes an upstream bug. The differential shows that
Rust matches the pinned reference; an intentional deviation would get its own
documented expected-deviation test rather than a wider tolerance.

Reproducing a drawing: `ACadSharp.Generator generate <dir> <name>` rewrites it,
and `ACadSharp.Generator check fixtures/generated/dwg` fails if a committed file
differs from what the generator writes today. Each row's
`generator_source_sha256` is the hash of the one file that defines it, and
`tests/generated_fixtures.rs` recomputes it.

## Parity families (H4.2)

Ten more generated drawings, `parity_arc`, `parity_circle`, `parity_ellipse`,
`parity_spline`, `parity_line`, `parity_polyline`, `parity_text`, `parity_insert`,
`parity_hatch` and `parity_viewport`, written by `reference/ACadSharp.Generator/Parity/`.
They exist so `tests/upstream_parity/*.rs` can assert values the generator wrote
(a radius of 5.0, a nested insert that lands at (94,54)-(94,56)) rather than only
that a decode succeeded. Their expected outputs are frozen like any other row's, so
the differential covers them too.
