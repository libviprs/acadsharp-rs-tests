# The differential

The test the whole effort exists to run: the same DWG goes through ACadSharp on
.NET and through `acadsharp-rs` over the NativeAOT archive, both sides write
canonical schema 1 (`CANONICAL_SCHEMA.md`), and a comparator says whether they
agree.

```
              same DWG input
                     |
      +--------------+--------------+
      |                             |
      v                             v
ACadSharp .NET (JIT)           acadsharp-rs over the NativeAOT archive
reference/ACadSharp.Reference  src/acadsharp_emit.rs (R3.1)
      |                             |
      v                             v
frozen: fixtures/expected/     actual: written on every run
      |                             |
      +---------- compare ----------+
                src/compare.rs
```

Three pieces own different parts of that picture, and it matters which is which.

| piece | what it owns |
|---|---|
| `tests/parity.rs`, `parity/VERDICTS.toml` (R4.1) | the comparison itself: one verdict per fixture and pair, a reason and an issue on every row that is not a match |
| `src/acadsharp_emit.rs` (R3.1) | the exporter: the safe API's items onto schema 1 records |
| `tests/fixture_parity.rs`, the matrix in `ci.yml`, this page (H4.1) | the lanes: the same decode on every published target, shared and static, and what makes a green cell mean something |

## What is compared, and how exactly

Nothing is compared as text. Both sides are parsed (`src/canonical.rs` refuses
anything the schema forbids) and `src/compare.rs` judges the parsed values under
a class per field kind, from `TOLERANCES.md`:

* **exact**: record kinds, record counts, view names and kinds, strings,
  integers, flags and warning categories. Equality of the parsed value.
* **a float bound**: `abs <= 1e-9 or rel <= 1e-12` for the oracle against
  acadsharp-rs (`BOUNDED_TIGHT`). They are two flatteners over one reader, so
  what separates them is structural and the float noise is small. Angles are
  compared as wrapped differences, except an arc's and ellipse's `end`, which is
  not wrapped.
* **not compared, with a reason**: what the wire cannot carry. Wire 2 has no
  `point` record, no hatch entity type, no `fit_points`, none of a text's
  `width_factor`, `halign`, `valign` and `style`, and no `maintenance_version`.
  Each is an `EXCLUDED` row with its issue (`parity/TABLES.toml`, and the table
  in `EMITTERS.md`), counted in the report and never silently absent.

The exporter shares no code with the oracle. The schema document is the only
thing they have in common, and `tests/exporter_independence.rs` is the tripwire
for the cheap way that stops being true: it extracts the identifiers of both
sides and fails on any shared one the schema does not name. It cannot see a
shared idea, only a shared word. The proof of independence is that the two
disagree where the wire cannot say what the oracle says.

## What `fixture_parity` adds to every cell

`tests/fixture_parity.rs` runs in every cell of the matrix below.

1. **Version lockstep.** `acadsharp_reference_version` in every manifest row
   with a frozen output has to be a version the crate's `COMPAT.toml` lists
   (every build, no archive needed), and in a cell with an archive it has to be
   the `acadsharp_version` that archive's `LINKINFO.json` reports. A differential
   between two ACadSharp versions is not a differential, and this fails before
   anything is decoded.
2. **A non-trivial stream.** Every drawing that should have geometry yields at
   least one primitive natively, and the comparison looks at real fields: more
   than zero scalars compared over more than zero aligned records.
   Drawings that cannot are listed with the reason: `no_primitives` rows, tier C
   (opened and counted only), and four the published archive can only warn about
   because it predates the lowering of SOLID and 3DFACE and the POINT record.
   `g13_dimension_deep` is refused by one of the reader's own bounds, by design.
3. **The test can fail.** `acadsharp-ac1032` decoded natively against
   `acadsharp-ac1024`'s frozen output must report a mismatch (at least ten red
   findings). If the two drawings ever became the same drawing, the differential
   could pass vacuously; this is the case that would then go red.
4. **The cell is the cell it claims to be.** `VIPRS_CELL_MODE` names the link the
   job intends. The suite's `build.rs` records how it was actually linked
   (`SUITE_NATIVE_LINK_KIND`), and a static cell that linked shared fails.

## The matrix

Every published target runs shared, and again static where the archive is
certified. The cells are planned from `NATIVE_ARTIFACT.toml` by
`tools/native_cells.py`, with no download:

| target | runner | shared | static |
|---|---|---|---|
| `linux-x64` | `ubuntu-latest` | `Native (linux-x64, shared)`, with the no-.NET proof | yes |
| `linux-arm64` | `ubuntu-24.04-arm` | yes | yes |
| `musl-x64` | `ubuntu-latest`, in `rust:1-alpine` | yes | yes |
| `musl-arm64` | `ubuntu-24.04-arm`, in `rust:1-alpine` | yes | yes |
| `mac-arm64` | `macos-15` | `Native (mac-arm64, shared)` | no |

`static_certified` in that file is a measurement the producer made (it linked the
static archive into a probe program and ran it on that target), copied from each
archive's `LINKINFO.json`, and `tools/fetch_native.py` refuses an archive that
disagrees with it. A target that says `false` gets no static cell, not a skipped
one.

Each cell runs `tools/native_cell.sh`: the smoke, `fixture_parity` and the native
emitter's own tests. `fixture_parity` writes the canonical JSONL for every
drawing into the cell's output directory, `ci.yml` uploads it as `cell-<target>-<mode>`,
and the final job, `Native (shared versus static)`, runs
`tools/compare_cells.py`: for every target with both cells the file sets must be
the same and every file byte-identical, with no tolerance. A static link and a
shared link of one archive are the same code, so a difference is a finding about
the link.

musl cells run inside the image with `docker run`, not as a job container,
because GitHub's JavaScript actions do not run in an Alpine job container. They
build with `-C target-feature=-crt-static`, since a static libc cannot load a
shared library.

## Reading a diff

A finding is one line, the way `src/compare.rs` renders it. This one is a circle
whose radius is 5.0 on one side and 5.005 on the other, and
`tests/differential_doc.rs` regenerates it and fails if the line below is not
what the comparator prints:

```
fixture oracle-acadsharp: view 0 a#4 b#4 circle.radius: 5.0 vs 5.005 (abs 4.999999999999893e-3, rel 9.990009990009777e-4) [BOUNDED_TIGHT]
```

fixture (a real run puts the fixture's id there), pair, view, the record's position on each side, the entity handles
where a sidecar names them, then `record.field: expected vs actual`, the errors,
and the class that judged it. `only in A` and `only in B` are records one side
has and the other lacks; a trailing `known, <issue>` marks a divergence the pair
table lists. `parity/SUMMARY.md` has the per-fixture verdicts built from these.

## How to regenerate

The frozen oracle outputs are never regenerated by a test or by CI. See
`reference/ACadSharp.Reference/README.md` and `tools/regenerate_reference.py`:
`regenerate` compares and writes nothing, `regenerate --accept` writes after you
have read the diff. An ACadSharp upgrade arrives as a diff somebody read.
