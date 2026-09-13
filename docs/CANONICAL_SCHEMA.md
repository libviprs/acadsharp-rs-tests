# The canonical schema, version 1

The format the .NET reference oracle writes and a future `acadsharp-rs` has to
reproduce. It is a **test contract**, not a serialization format either side
publishes, and nothing outside this repository should read or write it.

> The reference artefacts describe the observable CAD rendering semantics
> produced directly by the pinned ACadSharp .NET implementation. They are not
> intended to reproduce the complete ACadSharp object model or authoring
> semantics.

That sentence is the whole scope. A record says what a viewer should draw. It
does not say what the DWG contained, and a great deal of what a DWG contains has
no record here at all.

## Why this shape

**JSONL is authoritative and SVG is secondary.** The semantic stream is what a
differential compares: a record type, a count, a string, an integer, a
coordinate. The SVG exists so a human can look at a fixture and a change to it,
and so a rendering regression that does not move a single number is still
visible. A picture cannot be compared field by field, so it is never the thing
that decides whether two implementations agree.

**DXF is not the comparison output.** Writing the drawing back out through
ACadSharp's own writer and diffing that would make the test a test of the
writer, and a bug shared between the reader and the writer would cancel out and
leave the file looking correct. The same argument rules out ACadSharp's
`SvgWriter`: the visual reference is generated from the canonical records this
repository extracted, and from nothing else.

**The oracle is JIT .NET.** The system under test is NativeAOT plus a C ABI plus
Rust. If the reference went through the same normalization path, the two sides
could carry the same bug and still compare equal, and the differential would
have nothing to say. That is also why no code is shared between them: where the
two need the same logic, it is written twice on purpose.

**Curves stay curves.** A circle is a centre and a radius, not four hundred
vertices. A later test cannot prove an arc survived ACadSharp to NativeAOT to
the C ABI to Rust if the oracle had already thrown the arc away, and libviprs
does its own zoom-aware tessellation much further downstream.

## The file

- UTF-8, no byte order mark.
- LF line endings. Never CRLF, and no CR anywhere, including inside text.
- One JSON object per line, no pretty printing, no blank lines.
- A final newline.
- Stored compressed as `<id>.reference.jsonl.zst`. **The sha256 in the manifest
  is of the uncompressed bytes**; see [Compression](#compression).

### Record order

Fixed, and part of the contract:

1. exactly one `document` record;
2. every `view` record, in view order;
3. every entity record, grouped by view in view order, and within a view in the
   order the document lists the entities;
4. every `warning` record, sorted.

Entity order is the drawing's own. ACadSharp exposes a deterministic
enumeration order for a block record's entities and it is kept, because draw
order can matter and because it is the order a Rust implementation gets for
free. `BlockRecord.GetSortedEntities`, which applies the sort-handle draw order,
is a different and equally defensible choice; it is not the one made here.

Views are ordered by `TabOrder`, then by name ordinally. ACadSharp's layout
collection is keyed by name and enumerates alphabetically, which would put
`Layout1` and `Layout2` in front of `Model`. Tab order is stored in the file and
model space is always zero.

Warnings are sorted by `(category, entity_type, detail)`, ordinally. They arrive
in an order that comes partly out of dictionary iteration inside ACadSharp,
which is stable for one process but is not something to pin a hash to.
Duplicates are kept, so the number of warning records still equals the number of
things that went wrong.

### Numbers

- Shortest round-trip formatting, invariant culture. Conceptually
  `value.ToString("R", CultureInfo.InvariantCulture)`.
- A `.0` suffix is appended when the result carries no `.`, `e` or `E`, so every
  number is visibly a double. `100` is written `100.0`.
- Negative zero is normalised to zero. Two implementations can both round-trip
  IEEE-754 correctly and still disagree on the sign of a zero produced by the
  same geometry.
- Geometry is **not** rounded. The comparator handles tolerance; the reference
  carries every digit the source had.
- NaN and the infinities are refused. Encoding either would produce a file that
  is not JSON, and a NaN coordinate is a real finding about the reader rather
  than a formatting problem, so the export fails and names the value.
- Angles are radians. Lengths are drawing units.

### Strings

- Only what JSON forbids bare is escaped: `"`, `\`, and the C0 controls, with
  the usual `\b \f \n \r \t` shorthands and `\u00XX` for the rest.
- **Everything else, including all non-ASCII, is written as UTF-8.**
  `System.Text.Json`'s default encoder would escape it; the oracle hand-writes
  the escaping so the policy is not a framework default that can change.
- Line endings inside CAD text are normalised to LF. The same drawing written on
  Windows and elsewhere must not differ only in that.

### Coordinates

Coordinates live in the frame each record's own fields define.

- Records that carry a normal (`polyline`, `circle`, `arc`, `ellipse`, `text`)
  express their points in the object coordinate system that normal implies,
  derived by the DXF arbitrary axis algorithm. Their angle fields are measured in
  that plane and there is nowhere else to measure them from.
- Records that carry no normal (`line`, `point`, `polygon`, `spline`) are plain
  world space.

For the `+Z` normal that every planar drawing uses, the object coordinate system
**is** world space, so in practice this distinction is only visible on genuinely
tilted geometry.

Block references are resolved: the records describe visible world-space
geometry, with the block base point, the insertion point, the X, Y and Z scales,
the rotation and every nested transform applied.

## The records

Every record opens with `"schema":1` then `"record":"<name>"`. Fields follow in
the order given below, always, and every listed field is always present.

### `document`

```json
{"schema":1,"record":"document","format":"dwg","acad_version":"AC1032","maintenance_version":228}
```

| field | type | meaning |
|---|---|---|
| `format` | string | Container the bytes came from. Always `dwg` today. |
| `acad_version` | string | `AC1014` through `AC1032`. |
| `maintenance_version` | integer | The header's maintenance version. |

Nothing here depends on where the file was, when it was read, which machine read
it, or which runtime did the reading. No absolute path, no timestamp, no
hostname, no random id, no address. That is the entire selection rule.

The brief's section 4 example spells this field `version` and its section 6
example spells it `acad_version`. `acad_version` wins, because section 6 is the
one about this record and because it is what `fixtures/manifest.toml` already
calls the same value.

### `view`

```json
{"schema":1,"record":"view","index":0,"name":"Model","kind":"model"}
```

| field | type | meaning |
|---|---|---|
| `index` | integer | Position in view order, from zero, gapless. |
| `name` | string | The layout's name as the drawing spells it. |
| `kind` | string | `model`, `layout` or `unknown`. |

### `line`

```json
{"schema":1,"record":"line","view":0,"x1":0.0,"y1":0.0,"z1":0.0,"x2":100.0,"y2":0.0,"z2":0.0}
```

### `polyline`

```json
{"schema":1,"record":"polyline","view":0,"closed":false,"nx":0.0,"ny":0.0,"nz":1.0,"points":[[0.0,0.0,0.0]],"bulges":[0.0]}
```

| field | type | meaning |
|---|---|---|
| `closed` | bool | Whether the last vertex joins back to the first. |
| `nx`/`ny`/`nz` | double | World normal of the plane the bulges live in. |
| `points` | array of `[x, y, z]` | Vertices, in document order. |
| `bulges` | array of double | One per vertex, **always present**, same length as `points`. |

A bulge is `tan(theta / 4)` for the arc from that vertex to the next; zero means
a straight segment. The array is emitted even when every value is zero, so the
field set of a polyline record never depends on its contents.

### `circle`

```json
{"schema":1,"record":"circle","view":0,"cx":10.0,"cy":20.0,"cz":0.0,"radius":5.0,"nx":0.0,"ny":0.0,"nz":1.0}
```

### `arc`

```json
{"schema":1,"record":"arc","view":0,"cx":50.0,"cy":50.0,"cz":0.0,"radius":25.0,"start":0.0,"end":1.5707963267948966,"nx":0.0,"ny":0.0,"nz":1.0}
```

`start` is in `[0, 2*pi)` and `end` is strictly greater than it: the arc always
sweeps counter-clockwise from `start` to `end`, and `end` may exceed `2*pi`.

### `ellipse`

```json
{"schema":1,"record":"ellipse","view":0,"cx":0.0,"cy":0.0,"cz":0.0,"mx":4.0,"my":0.0,"mz":0.0,"ratio":0.25,"start":0.0,"end":3.141592653589793,"nx":0.0,"ny":0.0,"nz":1.0}
```

| field | type | meaning |
|---|---|---|
| `mx`/`my`/`mz` | double | Vector from the centre to the end of the major axis. |
| `ratio` | double | Minor over major, in `(0, 1]`. |
| `start`/`end` | double | Parameters, `0` at the major-axis end. A full ellipse is `start` to `start + 2*pi`. |

A circle or arc under a **non-uniform** insert scale becomes an ellipse record.
Keeping a "circle" with one of the two radii would be a lie the differential
would then have to agree with.

### `spline`

```json
{"schema":1,"record":"spline","view":0,"degree":3,"closed":false,"periodic":false,"control_points":[[0.0,0.0,0.0]],"knots":[0.0],"weights":[],"fit_points":[]}
```

`weights` is empty for a non-rational curve and `fit_points` for one with none.
Affine maps commute with the rational B-spline basis, so control and fit points
move under a block transform and degree, knots and weights do not.

### `polygon`

```json
{"schema":1,"record":"polygon","view":0,"points":[[0.0,0.0,0.0],[1.0,0.0,0.0],[1.0,1.0,0.0]]}
```

A closed, filled region, from `SOLID` and `3DFACE`. Points are in **perimeter
order**, not DXF storage order: a DXF `SOLID` stores its corners 1, 2, 4, 3 and
drawing them in stored order produces a bow tie that still renders and still
hashes. A degenerate quad, which is how DXF spells a triangle, comes out with
three points.

### `point`

```json
{"schema":1,"record":"point","view":0,"x":1.0,"y":2.0,"z":0.0}
```

Beyond the brief's minimum record set. The alternative was forty warning records
per fixture for an entity that is trivially representable, which would have
buried the warnings that matter.

### `text`

```json
{"schema":1,"record":"text","view":0,"x":20.0,"y":30.0,"z":0.0,"height":2.5,"rotation":0.0,"width_factor":1.0,"halign":"left","valign":"baseline","style":"Standard","text":"ROOM 101"}
```

| field | type | meaning |
|---|---|---|
| `height` | double | Cap height in drawing units. |
| `rotation` | double | Radians, counter-clockwise, in `[0, 2*pi)`. |
| `width_factor` | double | Horizontal scaling. `1.0` for MTEXT, which has no such field. |
| `halign` | string | `left`, `center`, `right`, `aligned`, `middle`, `fit`. |
| `valign` | string | `baseline`, `bottom`, `middle`, `top`. |
| `style` | string | The text style's **name**. |
| `text` | string | The string, line endings normalised, Unicode untouched. |

`style` is a name and never a resolved font file path: a reference that named
`consola.ttf` would be making a claim about the machine that read it. The oracle
resolves no fonts at all, which is also why it never synthesises a
`missing_font` warning of its own.

The anchor is the DXF one: the entity's second alignment point whenever the text
is anything other than left-baseline, and its insertion point otherwise.

MTEXT inline formatting codes are **not** interpreted. The `text` field carries
what the drawing stores. The oracle is a semantic reference, not a text layout
engine, and inventing a layout would be inventing something for the Rust side to
match rather than something to compare.

### `warning`

```json
{"schema":1,"record":"warning","category":"unsupported_entity","entity_type":"MultiLeader","detail":"no_canonical_mapping"}
```

| field | type | meaning |
|---|---|---|
| `category` | string | One of the seven below. |
| `entity_type` | string | ACadSharp type name, or the DXF name of an unknown class. May be empty. |
| `detail` | string | A short, stable discriminator. May be empty. |

Categories: `unsupported_entity`, `invalid_entity`, `missing_reference`,
`missing_font`, `malformed_geometry`, `reader_notification`, `unknown`.

`detail` never carries an exception message, a stack trace or a DWG handle.
ACadSharp spells a reference as `NAME|1883` and the number is a handle: stable
inside one file and meaningless across two, so the same drawing saved as AC1018
and as AC1032 would produce warning records differing only in numbers nothing
can compare. The console log keeps the richer text; the artefact does not.

Every entity the extractor cannot represent produces one of these. Nothing is
dropped silently, because a differential over a stream that quietly lost a
hundred entities compares two identical absences and passes.

The one deliberate silent skip is the structural markers: `SEQEND`, and the
`BLOCK`/`ENDBLK` pair. None of the three is drawable. An entity the drawing
marks invisible is also absent rather than warned about, because absence is the
correct representation of something a viewer does not draw.

## Entity mapping

| ACadSharp | canonical |
|---|---|
| `Line` | `line` |
| `Point` | `point` |
| `LwPolyline`, `Polyline2D` | `polyline`, bulges kept |
| `Polyline3D` | `polyline`, bulges all zero |
| `Arc` | `arc`, or `ellipse` under a non-uniform scale |
| `Circle` | `circle`, or `ellipse` under a non-uniform scale |
| `Ellipse` | `ellipse` |
| `Spline` | `spline` |
| `TextEntity`, `MText`, `AttributeEntity`, `AttributeDefinition` | `text` |
| `Solid`, `Face3D` | `polygon` |
| `Insert` | the block's entities, transformed, recursively |
| `Dimension` and subclasses | its generated block's entities, transformed |
| `Hatch` | one analytic record per boundary edge |
| everything else | `warning`, `unsupported_entity` |

An `ATTDEF` inside a block definition is the template for an attribute, not
something a viewer draws when the block is inserted, so it is skipped there and
the insert's own `ATTRIB`s are emitted instead, positioned in the space that
contains the insert. At the top of a view an `ATTDEF` is ordinary text.

A dimension's drawn form is the anonymous block the writer generated for it: the
extension lines, the arrowheads and the measurement text. Reconstructing that
from the definition points would be a second dimension engine, and a reference is
not the place for one.

A hatch's **boundary** is emitted as analytic edge records. Schema 1 does not
model the fill or the pattern lines. That is a stated limit rather than a
decoding failure, so it produces no warning: one per hatch would fire on every
hatch in every fixture and drown the warnings that mean something.

### Blocks

Nesting is limited to 16 levels and a self-reference or a longer cycle is
detected on the way in. Both are refusals with a `malformed_geometry` warning
rather than an exception or a stack overflow, and geometry that was not drawn
because the resolver stopped is therefore visible in the file: two
implementations that stopped at different depths would otherwise compare equal.

The cycle check is keyed on block identity, not on the DWG handle. A document
built in memory has not been assigned handles yet, so every block would answer
zero and the first nested block would read as a cycle.

An MInsert array expands to at most 1024 copies; a larger row or column count is
a two-byte field asking for something indistinguishable from a hang, and it
warns instead.

### Transforms

An insert composes, in DXF's order: base point, then scale, then the array
offset, then rotation about the insert's own Z, then the insertion point, then
the insert's object coordinate system.

A curve is carried through by transforming its centre and its two conjugate
semi-diameters, which every affine map takes to conjugate semi-diameters, so the
result is exact and nothing is tessellated and re-fitted. When every insert in
the chain scales X, Y and Z by the same magnitude the fast path applies: the
same kind of curve, the same shape parameters, and an angle offset. Otherwise a
two by two singular value decomposition recovers the principal axes of the image
ellipse, which is the general answer and still exact.

Whether the chain is a similarity is tracked from the scale factors that went
into it, never sniffed back out of the matrix. Comparing computed matrix entries
for equality would classify an ordinary 30 degree rotation as non-uniform on
float noise alone, and every rotated block in the corpus would turn into an
ellipse.

One case schema 1 cannot express exactly: a polyline **bulge** under a
non-uniform scale. A bulge is a circular arc by definition and its image is
elliptical. The bulges are carried through unchanged and a `malformed_geometry`
warning declares the approximation rather than hiding it.

## Compression

`*.reference.jsonl` is compressed to `*.reference.jsonl.zst` with CPython's
`compression.zstd` (libzstd 1.5.7 at the time of writing) at level 19, with the
content size flag on, the checksum flag off, no dictionary id and **zero worker
threads**. That last one matters most: any value above zero lets libzstd split
the input across threads and the frame then depends on how many the machine had.

Those settings make the bytes reproducible for that library version. They are
not a cross-implementation guarantee, so the manifest pins
`reference_jsonl_uncompressed_sha256`, the hash of the uncompressed stream, and
treats the `.zst` purely as storage. The Rust suite decompresses and checks that
hash; it never hashes the compressed file. This is the fallback the brief's
section 16 anticipates, taken deliberately rather than after a failure.

## The SVG dialect, version 1

A secondary, visual oracle, generated from the same canonical records.

- White background rectangle covering the view box, black stroke, `fill="none"`
  except on a `polygon`, black text. No source colour fidelity: a reference that
  tried to reproduce AutoCAD colour indices would need a colour table, and the
  colour table would become a second thing to keep in step.
- `vector-effect="non-scaling-stroke"` with `stroke-width="1"`, so linework is a
  hairline at every zoom.
- **CAD Y points up and SVG Y points down, so the drawing is mirrored about the
  X axis**: `y_svg = -y_cad`, applied by negating each ordinate as it is
  written, never by wrapping the document in a `scale(1,-1)` group. A group
  transform would mirror the glyphs too. Rotations are negated and converted to
  degrees.
- The complete drawing bounds go in the `viewBox`, once, with a margin of 2% of
  the larger dimension on each side. Nothing is cropped per entity. An arc
  contributes the box of its containing circle, which can leave extra margin and
  never crops.
- `<line>`, `<path>`, `<circle>`, `<ellipse>`, `<polygon>` and `<text>` where
  each is native. Arcs and elliptical arcs use SVG's own `A` command.
- A spline becomes a path sampled at **256 uniform parameter samples** by
  Cox-de Boor over the record's own knots and weights. A conic whose plane is
  not parallel to XY becomes a path sampled at **128 uniform angle samples**.
  Uniform rather than adaptive on purpose: an adaptive subdivision's output
  depends on a tolerance comparison and would change the file's hash whenever
  the tolerance was tuned.
- `font-family="sans-serif"`, always. The visual reference must not depend on a
  local AutoCAD or SHX font being installed.

**None of this approximation reaches the JSONL.** The semantic record keeps the
analytic curve whatever the SVG had to do to draw it.

## Changing the schema

`reference_schema` in every manifest row, `CanonicalSchema.Version` in the
oracle, and `SCHEMA` in `tests/expected_outputs.rs` all have to move together,
and the last of those is written out by hand rather than read from the manifest
so that a regeneration cannot quietly carry the suite along with it.

Adding a field to an existing record is a schema change, because the field set
is fixed and a reader may rely on it. Adding a whole new record type is also a
schema change, because a reader that does not know it would skip geometry.
