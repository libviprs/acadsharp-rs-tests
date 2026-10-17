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

The adapter for the other engine, `emit-acadsharp`, is R3.1 and gets its own
section here when it lands.

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
excluded) over every drawing the oracle froze, 70 of them. 63 agree to the
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

Two things that are not differences but are limits, because no drawing in the
corpus exercises them:

- a hatch boundary polyline edge is emitted with its vertices at the hatch's
  elevation and the bulge taken from the vertex's third coordinate (acadrust
  stores `(x, y, bulge)`); the oracle passes that third coordinate through as a
  point coordinate, which looks like a defect and has no fixture to show it;
- a hatch boundary spline carries its weights only when it is rational, taken from
  the control points' third coordinate, which is where acadrust's own DWG writer
  puts them.

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
