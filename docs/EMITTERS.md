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
  views, the warnings) carry the handle `0`;
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
