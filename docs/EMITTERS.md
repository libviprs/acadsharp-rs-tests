# Emitters

What each engine's `emit-*` binary writes, and where it differs from the
oracle. Each emitter is a process, not a library call: a reader that aborts on a
malformed file takes only its own row down, every run stamps itself with
provenance, and the outputs are files a human can diff and CI can upload.

For a fixture `<id>`, `emit-<engine> --fixture <id> --out <dir>` writes

- `<id>.<engine>.jsonl`, the schema 1 stream (`docs/CANONICAL_SCHEMA.md`),
  written through `src/canonical.rs`, the same writer for every engine;
- `<id>.<engine>.src.jsonl`, the sidecar: one line per record,
  `{"i": <index>, "h": ["<insert handle>", ..., "<entity handle>"], "t": "<record type>"}`,
  handles as uppercase hex with no prefix. It names entities in a diff and is not
  part of the contract. Records with no entity behind them (the document, the
  views, a warning about the document) carry the handle `0`; a warning raised for an
  entity carries that entity's handle path, so the census check can count it;
- `<id>.<engine>.run.json`, the provenance: the engine commit the suite pins
  and the one actually beside it, the fixture's sha256, the target, and
  `RAYON_NUM_THREADS` as the process saw it.

## acadrust

`emit-acadrust` needs the `acadrust` feature and `../acadrust` at
`ACADRUST_REV`:

```
cargo run --features acadrust --bin emit-acadrust -- --fixture acadsharp-ac1032 --out /tmp/out
```

The adapter is `src/acadrust_emit.rs`. It is written against
`reference/ACadSharp.Reference/Cad/`, which defines the mapping, and carries over
from the fork's conformance harness (`tests/viprs/mod.rs`) the arbitrary-axis and
insert algebra. Nothing of upstream acadrust is copied; a test hashes every `.rs`
under the pinned `../acadrust/src` and asserts no file here matches.

Conventions worth knowing, all from the schema or the oracle and all tested:

- views are ordered by `tab_order`, then by name ordinally, and named as the
  drawing names them; the framing records come first and the warnings last, sorted
  by (category, entity type, detail);
- an `ATTDEF` inside a block is skipped and the insert's `ATTRIB`s are emitted
  after the block body in the insert's own space; a top-level `ATTDEF` is text;
- a dimension is its block, the inserts inside it included (the arrowheads);
- a circle or arc under a non-uniform scale is an ellipse; a polyline bulge
  under one is carried unchanged with a `malformed_geometry` warning;
- block nesting past 16 and a block cycle are `malformed_geometry` warnings, an
  MInsert past 1024 copies expands once with a warning, and an insert of a block
  that is not there is a `missing_reference` warning;
- invisible entities, `SEQEND`, `BLOCK` and `ENDBLK` are skipped silently;
  everything else with no mapping (a viewport, a leader, a mesh) is an
  `unsupported_entity` warning whose entity type is the DXF name, which the
  oracle-to-acadrust pair table maps or excludes against the oracle's ACadSharp
  class names;
- `document.maintenance_version` is acadrust's `CadDocument.maintenance_version`,
  the file header's byte 0x0B, which is zero throughout the corpus. The oracle's
  is ACadSharp's `Header.MaintenanceVersion` (228 for the AC1032 save), a
  different quantity that acadrust does not decode. The oracle-to-acadrust pair
  excludes the field.

### Where acadrust and the oracle differ

Measured with the comparator (`BOUNDED`, warnings and `document.maintenance_version`
excluded) over every drawing the oracle froze, 87 of them. 78 agree to the
comparator's bound on every field of every record, including the record counts per
kind for `sample_AC1032` (`tests/emit_acadrust.rs` holds that to the oracle's
frozen output). The rest are below, and the same test fails if this table is not
what the comparator finds: a row for a difference that has gone is stale, and a
difference with no row is unexplained.

| fixture | record.field | count | why |
|---|---|---|---|
| acadsharp-ac1014 | text.text | 1 | The drawing stores `\U+220545,6` (AutoCAD's own DXF twin holds exactly that). The oracle keeps it, as the schema says the field carries what the drawing stores; acadrust decodes the escape to `∅45,6`. |
| acadsharp-ac1015 | text.text | 1 | The same text, the same escape. |
| acadsharp-ac1018 | text.text | 1 | The same text, the same escape. |
| acadsharp-ac1032 | text.text | 1 | An `ATTRIB` whose value lives in an embedded MTEXT. The oracle emits an empty string; acadrust emits the MTEXT's value, `my multi line text for the attrrib`, which is what AutoCAD's own DXF twin holds. |
| acadsharp-sample-base | text.text | 1 | The same attribute. |
| acadsharp-export-sample | polyline.points | 11 | Eleven lightweight polylines in view 7 whose points the oracle puts at z 1.0 and acadrust at z 0.0. Not adjudicated: acadrust reports an elevation of 0 for them, and no DXF twin exists for this drawing to say which is right. R3.4's ground truth is where this gets settled. |
| acadsharp-export-sample | spline.periodic | 1 | A closed spline defined by fit points: the oracle says `periodic`, acadrust's flags say closed and not periodic. Not adjudicated, for the same reason. |
| acadsharp-export-sample | spline.weights | 1 | A non-rational spline with nine control points. The oracle emits nine weights of 0.0; the schema says `weights` is empty for a non-rational curve, which is what acadrust emits. A zero weight is meaningless, so this is the oracle's artefact. |
| g13_hatch | spline.weights | 1 | The same, on a hatch boundary spline: four weights of 0.0 from the oracle, none from acadrust. |
| regression_text_zero_height | text. | 2 | A TEXT and an MTEXT of height 0. ACadSharp's writer writes the zero and its reader throws reading it back, so the oracle drops both and says so with two `reader_error` warnings; acadrust reads both and emits `text` records of height 0.0. The oracle is the one that cannot hold them, which is what the fixture pins (`tests/regression_claims.rs`). |
| parity_viewport | view.kind | 2 | The two layouts the generator adds by name (`Plan A`, `Plan B`). The oracle says `layout`; acadrust says `unknown` for both and `layout` for the default `Layout1`. Tracked in #90; the cause is not investigated: the geometry in each view agrees. |

Two things that are not differences but are limits, because no drawing in the
corpus exercises them:

- a hatch boundary polyline edge is emitted with its vertices at the hatch's
  elevation and the bulge taken from the vertex's third coordinate (acadrust
  stores `(x, y, bulge)`); the oracle passes that third coordinate through as a
  point coordinate, which looks like a defect and has no fixture to show it;
- a hatch boundary spline carries its weights only when it is rational, taken from
  the control points' third coordinate, which is where acadrust's own DWG writer
  puts them.

## acadsharp-rs

`emit-acadsharp` has no cargo feature. `acadsharp-rs` is a normal dependency of
this suite and builds with no archive, so the binary exists everywhere and
refuses at run time: `Decoder::new()` answering `Error::Unlinked` is exit 1 with
the message, and nothing is written. Under `ACADSHARP_REQUIRE_NATIVE=1` the
crate's `build.rs` stops the build when no archive resolves, so there is no
build of this binary that can print nothing and exit 0.

```
ACADSHARP_NATIVE_DIR=/path/to/unpacked/acadsharp-linux-x64 \
  cargo run --bin emit-acadsharp -- --fixture acadsharp-ac1032 --out /tmp/out
```

The suite's own `build.rs` emits the rpath that finds the shared library
(`DEP_ACADSHARP_NATIVE_LIB_DIR`, the three lines the crate's documentation gives);
without it the binary links and then dies before `main` with exit 127. On a
target whose `LINKINFO.json` says `static_certified: true`, build with
`--features acadsharp-rs/link-static` instead and there is nothing to find.

The adapter is `src/acadsharp_emit.rs`. The walk is
`Decoder -> Document::open_bytes -> views() -> decode(i) -> PrimitiveStream`, with
`is_complete()` checked per view (an incomplete stream is a failure, not a short
output). Bytes come from `fixtures::load(id)` and never from a path. The mapping is
a pure function of what a walk saw ([`map_walk`]), so every row below is tested on
hand-built items with no native library, and the native lane is tested on top.

`run.json` for this engine adds what the archive says about itself, all read from
the unpacked archive the binary was linked against: `native_artifact_version`,
`native_acadsharp_version`, `native_target`, `native_abi_header_sha256`,
`native_library_sha256` (from the archive's own `CHECKSUMS.txt`, which is what
tells two archives apart that both call themselves `3.7.1-viprs.N`),
`native_viprs_dep_commit` and `native_shim_sha256` where `LINKINFO.json` has them
(the published `acadsharp-3.7.1-viprs.1` does not), `native_driver_commit` from
`BUILDINFO.json`, and `native_archive_sha256`, which the gate sets after it has
verified the download. Nothing in it moves between two runs on one machine.

The sidecar from this side is partial and says so: wire 2 carries one
`item_handle` plus a from-insert flag, so `h` is `[leaf]`, `t` is absent (there
is no entity type on the wire), and `"x": true` marks a record that was expanded
out of an insert. A nested entity carries its own handle and not the insert's,
and a hatch edge carries the hatch's.

### Wire 2 onto schema 1

Wire 2 and schema 1 are two flatteners over the same reader, not two spellings of
one. Every gap is a row in `parity/TABLES.toml` with its class and an issue (or,
where the comparator cannot hold it, a `DIFF` row in `parity/VERDICTS.toml` whose
issue is where it gets decided), never a silent absence. The pair tables are in
`docs/TOLERANCES.md`; the verdicts are in `parity/SUMMARY.md`.

What the wire cannot carry, and so the adapter cannot write, is `EXCLUDED` for the
two pairs that have acadsharp-rs in them (`oracle/acadsharp`, `acadsharp/acadrust`):

| gap | wire 2 | schema 1 | issue |
|---|---|---|---|
| `point` | no record; a POINT arrives as warning 100 (`acadsharp-ac1032` has 40) | a `point` record | libviprs-dep#104 (wire 3) |
| hatch boundaries and `polygon` | one `Polygon` with bulges per loop; no entity type, so a hatch polygon and a SOLID polygon look the same (160 polygons against 15 on `acadsharp-ac1032`) | one analytic record per edge | acadsharp-rs-tests#59 |
| `document.maintenance_version` | `DocumentBegin` has `view_count` and `drawing_version` only | the header's maintenance version | acadsharp-rs-tests#61 |
| `spline.fit_points` | not on the wire | carried | acadsharp-rs-tests#58 |
| `text.width_factor`, `halign`, `valign`, `style` | position, height, rotation, the string | all four | acadsharp-rs-tests#66 |

What the adapter writes for those is the schema's default (`0`, `[]`, `left`,
`baseline`, `Standard`, `1.0`), so the stream is well formed and the field is
excluded from the comparison rather than compared against an invention.

What the run found that the adapter cannot fix, measured over the corpus with the
published archive (`parity/SUMMARY.md` has the rows, each with its issue). None of
these is hidden by an exclusion; each is a `DIFF` or `COUNT` and the
`acadsharp/acadrust` pair shows acadrust agreeing with the oracle in every one:

| what | where | wire 2 | schema 1 | issue |
|---|---|---|---|---|
| dimension blocks | the seven saves, `sample-base`, `export-sample` (30 lines and 5 polylines on `acadsharp-ac1032`) | `Flattener.cs` skips every INSERT inside a dimension's block, which is where the arrowheads live | expands them | acadsharp-rs-tests#55 |
| ATTDEF | the same (78 text records against 69 on `acadsharp-ac1032`) | the `TextEntity` arm takes `AttributeDefinition` too, so one inside a block is emitted; an insert's ATTRIBs come before the block body | skipped inside a block; ATTRIBs after | acadsharp-rs-tests#54 |
| text anchor | `acadsharp-ac1032` (1.72 off in y), `export-sample` | the insertion point | the alignment point for anything but left/baseline | acadsharp-rs-tests#66 |
| non-uniform insert scale | `g13_nonuniform`, `g13_ocs_plane`, `g13_ocs_skew` | keeps the circle, warning 106; no transform on the wire | an `ellipse` | acadsharp-rs-tests#64 |
| mirrored insert | `g13_ocs_mirror`, `g13_mirrored_bulge` | reflects the geometry, normal stays +Z | keeps `nz: -1` and the OCS coordinates | acadsharp-rs-tests#63 |
| full ellipse | one per save, 14 in `export-sample` | start 0, end 2pi | the stored start (pi, 3pi) | acadsharp-rs-tests#57 |
| invisible entities | `acadsharp-blockvisibilityparameter` (8 records) | drawn | skipped | acadsharp-rs-tests#60 |
| mesh edges | `g13_polyface_mesh`, `g13_polygon_mesh` | polylines (viprs.1 predates libviprs-dep#82) | a warning | acadsharp-rs-tests#62 |
| block depth | `g13_dimension_deep` | the decode stops at `max_block_depth` | warns and carries on | acadsharp-rs-tests#68 |
| spline flags and weights | `export-sample`, `g13_hatch` | `closed` and `periodic` disagree three ways; the oracle writes weights of 0.0 | | acadsharp-rs-tests#65, #67 |

What this adapter converts, with the rule it applies:

- a polyline's vertices and a circle's, arc's and ellipse's centre (and the
  ellipse's major axis, as a vector) go from world space into the OCS the normal
  implies, with the arbitrary axis algorithm (`src/ocs.rs`, the one the oracle
  runs and the one `emit-acadrust` shares). `line`, `spline` and `polygon` carry no
  normal and stay in world space;
- an arc's `start` is reduced into `[0, 2pi)` and its `end` is made strictly
  greater (an in-range angle is bit-identical to the wire's); an ellipse likewise;
- a text's rotation is reduced into `[0, 2pi)` and its line endings to LF;
- a polyline's `bulges` is always present, one per vertex (`bulge_count` of 0 is
  every span straight); a spline's `weights` is empty unless the rational flag is
  set; `closed` and `periodic` come from the flags;
- warnings: 100 and 109 are `unsupported_entity` (the entity type is the
  message's first token), 105 `missing_reference`, 106 `malformed_geometry`, 107
  `invalid_entity`, 101 `reader_notification`, anything else `unknown`; the detail
  is `wire_<code>` and never the message, which can carry a handle. The library's
  own forward-compatibility probe (record 32512 and up, in every stream by design)
  is skipped, and any other unknown record is a warning, never dropped;
- views come in tab order already; their names and kinds are the wire's.

### Two archives

The runs are labelled by the archive that produced them and never confused. The
file name of a digest table says which (`parity/emit/<artifact_version>+<library
sha8>.<target>.tsv`), and so does `run.json`.

- **The published `acadsharp-3.7.1-viprs.1`** (`acadsharp-linux-x64.tgz`, sha256
  `4cbdbbdd...`; `acadsharp-linux-arm64.tgz`, sha256 `dd938ef9...`) is what a
  consumer gets, and it is the archive the committed verdict table is about.
- **The rebuilt one** is libviprs-dep `bc351a9` built on MARS with the recipe in
  `acadsharp-rs`'s `tests/data/linkinfo/REBUILD.md` (R3.3). It calls itself
  `3.7.1-viprs.2`, which is the string a future release will carry too, so the
  label is `native_artifact_version` plus `native_library_sha256` (`670cd1e7...`,
  which is the shared library `docs/AOT_PARITY.md` names) and the commit passed as
  `--dep-commit`. The tarball built here is `bec5fdc1...`, and a rebuild is not
  promised to be byte-identical, so that digest labels this build only.

The header is byte-identical at `8bf5ad2`, `bc351a9` and `main` (`0502ac0f...`),
so both archives pass the crate's handshake. They diverge from the oracle on
different kinds, so a verdict table belongs to one archive. Against the rebuilt
archive 24 rows of the published archive's table move, 12 in each acadsharp pair:

- `g13_leader` and `g13_mline` go from `MATCH` to `COUNT`, because the rebuild lowers
  LEADER and MLINE onto polylines and the oracle warns about both;
- `g13_polyface_mesh` and `g13_polygon_mesh` go from `COUNT` to `MATCH`, because the
  rebuild refuses them as the oracle does;
- the seven saves and `sample-base` go from `DIFF:52` to `DIFF:54` (`DIFF:53` to
  `DIFF:55` in the `acadsharp/acadrust` pair for `acadsharp-ac1014` to `ac1018`),
  two more differing records than the published archive has.

Both archives have a digest table on x64 and on arm64, and on every fixture the
x64 and the arm64 table hold the same sha256 for the same archive: the emitters are
byte-identical across the two architectures. The rebuilt arm64 archive (library
`744e0ead...`, tarball `5e0280b0...`) was built from the same commit with the same
recipe on a native arm64 docker host (`--arch arm64`).

The committed verdict table stays the published archive's. The rebuilt lane is held by its
digest table, which is what a change to either archive has to move.

## ezdxf

Not an emitter in the sense above: nothing here runs per test. `ezdxf` is a ground
truth, a third reader that shares no code with ACadSharp, acadrust or acadsharp-rs,
over files AutoCAD itself wrote. It never opens a DWG, so it reads the DXF twins in
`fixtures/dxf/` (all 26 of them: the twins of `sample_AC1015` to `AC1032` in ASCII
and binary, the ten dynamic blocks, `geoloc`, and the `AC1009` and `geoloc_AC1021`
files, which are inputs for R5.1 and R5.2 and are compared with nothing yet).

The extractor is `tools/ground_truth/extract_ezdxf.py` with `ezdxf==1.4.4`, pinned by
hash with its four dependencies in `tools/ground_truth/requirements.txt`, and it runs
in `python:3.14-slim` pinned by digest. Its output is frozen the way the oracle's is:

- `fixtures/ground_truth/ezdxf/<input>.ezdxf.jsonl.zst`, the schema 1 stream;
- `<input>.ezdxf.src.jsonl.zst`, the sidecar, with `t` (the record type), `d` (the
  leaf's DXF name) and `h` (the insert handles down to the entity), handles as
  uppercase hex. A warning carries the handle of the entity it was raised for;
- `<input>.ezdxf.run.json`: `ezdxf_version`, `requirements_sha256`, the image digest,
  the input's id and sha256, and the record count. Nothing that moves between runs.

`fixtures/manifest.toml` holds a `[[ground_truth]]` row per input with the sha256 of
each file uncompressed (the `.zst` is storage), so `src/fixtures.rs` verifies them
the way it verifies the oracle's.

```
python3 tools/ground_truth/extract_ezdxf.py verify              # offline, stdlib
python3 tools/ground_truth/extract_ezdxf.py freeze              # runs the image, writes nothing
python3 tools/ground_truth/extract_ezdxf.py freeze --accept     # writes
python3 -m unittest discover -s tools/ground_truth/tests        # inside the image: needs ezdxf
```

`freeze` never writes without `--accept`. `--runner local` is for a shell that is
already inside the image, for example one on a remote docker host.

What is ezdxf's and what is the schema's. Reading, the block table, hatch paths and
the spline and ellipse parameters are ezdxf's. The schema's conventions (record
order, perimeter order for a `SOLID`, the arbitrary-axis frame a record is written
in, the singular value decomposition for a curve under a non-uniform scale, the
warning vocabulary) come from `docs/CANONICAL_SCHEMA.md` and are written in the
extractor, not copied from either reader. `ezdxf.path` is never used, because it
approximates arcs with Beziers.

Where the extraction is not the oracle's, all of it tested:

- **Caret notation.** DXF writes a control character as `^` and the character
  XOR 0x40, so a line feed in an MTEXT is `^J` in AutoCAD's ASCII DXF. ezdxf hands
  that back raw; the extractor decodes it, because the schema carries the character.
- **Blocks are expanded here, not by `Insert.virtual_entities`.** The virtual copies
  lose the handle path the sidecar and the census check name, so the extractor walks
  the block table itself and applies the insert algebra in DXF's order. The same goes
  for a dimension (its geometry block, arrowheads included) and an `ACAD_TABLE`
  (its block, placed by the insertion point and the rotation of its horizontal
  direction; ezdxf does not read a table's normal, so +Z is assumed). Doing it this
  way means ezdxf's own two choices for a non-uniform scale never enter: it explodes
  a bulged polyline into pieces, where the schema carries the bulge and warns, and
  it documents text under a non-uniform scale as possibly wrong. The schema's rules
  apply instead. No DXF twin in the corpus has a non-uniform insert, so nothing
  measured depends on it.
- **Warnings** name an unsupported entity by its DXF name (`MULTILEADER`, `VIEWPORT`),
  where the oracle uses the ACadSharp class. The pair table treats the warning stream
  as excluded and at most compares counts.
- **`document`.** `format` is `dxf`, and `maintenance_version` is `$ACADMAINTVER`,
  which is not the DWG header's number (329 against the oracle's 228 for the AC1032
  save).
- **ASCII against binary.** AutoCAD's own two files for a save agree except in one
  place: from the R2004 save on, the ASCII DXF writes four MTEXT paragraph breaks as
  `^J` and the binary DXF as `\P`. `tests/ground_truth_ezdxf.rs` lists them by handle.

What the ground truth has already found, on the oracle's frozen outputs for the seven
saves (`parity/oracle_invariance.toml`): the AC1032 save reads one spline as two fit
points and no control points where AutoCAD's own DXF of every save, AC1032 included,
holds four control points and eight knots (libviprs/libviprs-dep#150), and the
multiline block attribute that AC1032 loses (libviprs/libviprs-dep#112). Against the
oracle's AC1015 output the extraction differs in no record at all except the
container's name, and against AC1032 in exactly those two records.

### The census

`src/census.rs` checks a reader's model space against AutoCAD's own object census
(`fixtures/census/`), by handle and by class. The full check is for a sidecar that
names the top-level entity and its DXF name (acadrust, ezdxf): the outermost handles
of the model-space records are the census' 163 visible entities, and a record that
stands for the entity itself carries the DXF name its class maps to. The weaker check
is for acadsharp-rs's sidecar, which has one handle and an `x` flag: the handles on
records whose flag is clear must be in the census and must cover every census entity
that is not a block reference or a dimension. An entity the drawing marks invisible
may be absent, because the schema skips it by design. For acadrust to pass, its
sidecar names the entity on a warning as well, so an unsupported entity (a viewport,
a multileader) still counts.

acadsharp-rs runs through the weak check over the real archive, on native x64, on all
seven saves, and what it misses or adds is `parity/census_exceptions.toml`: the
unsupported kinds (the oracle warns about them too, and wire 2's warning has no
entity handle), the POINTs (libviprs-dep#83), the kinds the published viprs.1
archive does not lower, and the four attribute handles. A finding with no row is
red, and so is a row with no finding.

## The DXF lane (R5.1)

Three readers open a DXF: the oracle (ACadSharp's `DxfReader`, chosen by
extension in `DocumentExtractor`), acadrust (`emit-acadrust` takes an
`[[oracle_input]]` id as well as a fixture id, and acadrust tells ASCII, binary and
the pre-AC1012 group codes apart on its own) and ezdxf (R3.4, unchanged). The
oracle's reading of each DXF is frozen beside the DWG ones (`reference_jsonl` on
the `[[oracle_input]]` row, written by `tools/regenerate_reference.py regenerate
--accept` and checked by `verify`). acadsharp-rs is not a reader here: its C ABI
opens DWG only, so each of its cells is `EXCLUDED` with that reason.

The lane covers the 24 DXFs the oracle reads. The two AC1009 files are R5.2's: the
oracle reads R12 into a document with no layouts, so its extractor finds no view,
and the row says so (`no_reference`). AC1014 has no DXF twin (the save script wrote
none), so it has no cell.

| pair key | what it holds | float class |
|---|---|---|
| `dxf:oracle/acadrust`, `dxf:ezdxf/oracle`, `dxf:ezdxf/acadrust` | reader against reader on one DXF | as for the same pair on the DWG |
| `dxf-dwg:oracle`, `dxf-dwg:acadrust` | a reader's DXF against its own DWG reading of the same drawing | `BOUNDED` |
| `dxf-ascii-binary:oracle`, `dxf-ascii-binary:acadrust`, `dxf-ascii-binary:ezdxf` | a reader's ASCII against its binary reading of the same save | `BOUNDED_TIGHT` |

The positive control is R3.4's: `a_moved_coordinate_in_a_scratch_copy_of_a_twin_...`
in `tests/parity.rs` moves one LINE's group 10 in a scratch copy of the AC1015
twin and requires the `dxf-dwg:acadrust` verdict to leave the committed one.

## The round-trip lane (R5.3)

What each library writes, read back by the readers that are not it. The matrix
is `fixtures/written_plan.toml` (stage one: `sample_AC1032.dwg` through ACadSharp
and acadrust, every version each claims, in DWG and in ASCII and binary DXF);
each output is a `[[written]]` row of `fixtures/manifest.toml`, made by
`tools/regenerate_written.py` and `src/bin/rewrite-acadrust.rs`.

- ACadSharp's outputs are made by the oracle's `--rewrite-to` and **stored**
  (compressed, hashed uncompressed) under `fixtures/written/`, because nothing
  offline can make them again. ACadSharp stamps a fresh GUID pair and the clock
  into the header of everything it writes, so the rewrite pins them, and the tool
  writes every output twice and refuses a pair that differs.
- acadrust's outputs are **not stored**: its writer is deterministic, the test
  runs it and holds the result to the row's sha256, so the frozen readings of
  the oracle and ezdxf (made from those bytes) are readings of the same file.
  `rewrite-acadrust` is its own control (it writes twice and compares).
- The oracle's reading of each output is frozen on the row (`reference_jsonl`),
  and ezdxf's on a `[[ground_truth]]` row whose `input` is the output's id.
  A file a reader refuses is a row saying so (`no_reference`, `failed`,
  `write_failed`), never a hole.
- The readers are the oracle, acadrust and acadsharp-rs for a DWG (acadsharp-rs
  reads AC1014 to AC1032) and the oracle, acadrust and ezdxf for a DXF. acadrust
  and acadsharp-rs run in processes of their own, fed the bytes on standard input
  (`--stdin-as`), so a reader that aborts on a writer's malformed file takes one
  cell down.

Each output has a cell per reader and one `w-<writer>:version` row, which is
where a writer that writes the wrong version is caught (every cell excludes
`document.acad_version`). Stage two (libviprs/acadsharp-rs-tests#41) runs the
rest of the sources through the same plan in the nightly.
