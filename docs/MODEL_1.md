# The model-1 stream

A second canonical stream, written beside schema 1 (`docs/CANONICAL_SCHEMA.md`)
and held to the same rules: JSONL, unrounded, strict, one writer, its own
section on changing it. Schema 1 says what a viewer draws. It has no record for
a header variable, a table entry, a dictionary or an XRecord, and a good part of
what the upstream tests cover is exactly that. This is where those records
live, so a reader that gets every line right can still be shown to get a layer
colour or a `$LTSCALE` wrong.

I kept it a separate stream on purpose. Adding a record type to schema 1 is a
schema change for every reader of it, and these records have a different key (a
name, not a position in a drawing order), so they do not belong in a stream that
the comparator walks in order.

This document is the contract. `src/model1.rs` is the Rust side of it, the
oracle's writer is `reference/ACadSharp.Reference/Model1/`, and the ezdxf
side is `tools/model1.py`.

## Which kinds exist

| kind | status | key |
|---|---|---|
| `header` | this document | the DXF `$NAME` |
| `layer` | this document | the entry name |
| `linetype` | this document | the entry name |
| `textstyle` | this document | the entry name |
| `dimstyle` | this document | the entry name |
| `dictionary` | this document | the path of entry keys from the root |

The rest of the table kinds (`linetype`, `textstyle`, `dimstyle`,
`block_record`, `layout`), `dictionary`, and `xdata` with `xrecord` are each
their own issue and each adds a section here. Nothing below may change for
them: a new kind is a new section, never an edit to an existing one.

## The file

- UTF-8, no byte order mark, LF line endings, no CR anywhere, one JSON object
  per line, a final newline. Exactly the rules schema 1 gives.
- `<id>.<engine>.model1.jsonl` for an emitter's output. The committed ground
  truths are `fixtures/model1/oracle/<id>.oracle.model1.jsonl` and
  `fixtures/model1/ezdxf/<id>.ezdxf.model1.jsonl`, with their hashes in
  `fixtures/model1/MANIFEST.toml`.
- Every line opens `{"model":1,"record":"<kind>"`. The first field is `model`
  and not `schema`, so a model-1 line can never be taken for a schema 1 record
  and a schema 1 parser refuses the file at its first line.
- Numbers are written the schema 1 way: shortest round trip, a `.0` when there is
  no other mark of a double, negative zero as zero, NaN and the infinities
  refused. Strings are escaped the schema 1 way and line endings inside them
  are LF.

### Order

Records are sorted by kind (in the order of the table above), then by key,
compared as UTF-8 bytes. Nothing about the order a library enumerates its
variables in reaches the file, which matters because the three producers
enumerate three different ways. Two records with one key are refused by the
writer and by the parser.

### Keys

A record is named by what it is, so the stream survives a writer round trip and
three readers can agree on what a record *is*:

- `header`: the DXF `$NAME`, the one key all three engines share.
- tables: the entry name, compared exactly, case included.
- dictionaries and XRecords: the path from the named object dictionary.
- XData: the owner.

Handles stay out of the compared stream. They are stable between a DWG and its
AutoCAD DXF twin and not across a rewritten file, so they belong in a sidecar,
the way schema 1 keeps them.

## `header`

```json
{"model":1,"record":"header","name":"$LTSCALE","type":"double","value":1.0}
```

| field | type | meaning |
|---|---|---|
| `name` | string | the DXF name, `$` included |
| `type` | string | one of `string`, `int`, `double`, `bool`, `point2`, `point3` |
| `value` | per `type` | the value, in the one written form below |

One record per variable the engine can enumerate. `CadHeader.GetHeaderMap()` is
the oracle's enumeration (249 names; the oracle itself writes no header record
in schema 1, the `document` record carries a version and nothing else), the
header's `hdrvars` is ezdxf's, and the fields of acadrust's `HeaderVariables`
are acadrust's.

### The written form is chosen by the group code

A header variable is stored in DXF under one group code (or two or three for a
point), and the code decides the type. That is the rule, and it is the same
table in all three producers (`Model1Extractor.HeaderType`,
`model1::header_type`, `header_type` in `tools/model1.py`):

| group code | `type` | written as |
|---|---|---|
| 0 to 9, 100 to 102 | `string` | a JSON string |
| 40 to 59, 140 to 149 | `double` | a schema 1 real |
| 60 to 79, 90 to 99, 160 to 179, 270 to 289, 370 to 389 | `int` | a JSON integer |
| 290 to 299 | `bool` | `true` or `false` |
| two components (10, 20) | `point2` | `[x,y]` |
| three components (10, 20, 30) | `point3` | `[x,y,z]` |

A code outside those ranges is a hole in this table and a producer refuses it
loudly, rather than guessing. Codes that hold the handle of an object (320 to
369, 390 to 399, 480, 481) have no written form: the variable is not written,
and the names a producer left out for that reason are committed beside its
stream.

The library's own type is never the written type. ACadSharp holds `$ORTHOMODE` as
a `bool`, `$PSTYLEMODE` as a `short` and `$DGNFRAME` as a `char`, and acadrust
holds colours as an enum. The code says `int`, `bool` and `int`, and the
producer coerces to that.

### Values, one rule per source type

- **Names.** A variable that is a handle in a DWG (`$CLAYER`, `$CELTYPE`,
  `$TEXTSTYLE`, `$DIMSTYLE`, `$CMLSTYLE`, `$UCSNAME` and friends) is a name in
  DXF. The record carries the resolved name, never the handle. A null reference is
  the empty string.
- **Booleans stored as integers.** A flag stored under code 70 is an `int` that
  is 0 or 1, whatever the library calls it. Only codes 290 to 299 are `bool`.
- **Enums.** The integer the DXF writes.
- **Colours** held in an `int` variable: the AutoCAD colour index (0 by block,
  256 by layer, 257 none), and -1 for a true colour, which has no index.
- **Characters** (`$DIMDSEP`, `$DGNFRAME`): the code point as an integer.
- **Dates** (`$TDCREATE` and the rest): the Julian day number with the time of
  day as the fraction, which is what a DXF holds.
- **Time spans** (`$TDINDWG`, `$TDUSRTIMER`): days, as a double.
- **`$HANDSEED`**: uppercase hexadecimal, no prefix.

### Known from the start

Some variables differ between a DWG and its AutoCAD DXF twin by construction,
because the two files were written at different moments: `$HANDSEED`, `$TDINDWG`,
`$TDUSRTIMER`, `$VERSIONGUID`, `$ACADMAINTVER` (the maintenance release of whichever
AutoCAD wrote the file) and, on the engines that read them, the timestamps. They
are `known` in `parity/MODEL1.toml` for the `dxf-dwg:` pairs, which hold a
reader's DXF against its own DWG of the same drawing. Between two readers of one
file they are not excused: both read the same bytes.

A variable a file of an old version does not store is a second class: each reader
reports its own default, and the two defaults can differ. Those are listed per
fixture, with the DXF twin of that version as the evidence that the variable is
not stored.

### Values that cannot be frozen

A frozen file has to be the same bytes on every freeze, so three kinds of value
never reach a stream:

- `$TDUCREATE` and `$TDUUPDATE` from the oracle: ACadSharp sets both to the time of
  the read and its DWG reader never overwrites them.
- `$TDCREATE` from ezdxf, which stamps it with the time of the read.
- `$VERSIONGUID` and `$FINGERPRINTGUID` for a version that stores neither (R14):
  ACadSharp invents a random GUID per read, so the oracle writes them empty.

The first two are not written, and are `excluded` in `parity/MODEL1.toml` for the
engine that did not write them. The names ezdxf leaves out (these, and every
variable stored under a handle code) are committed beside its stream in
`fixtures/model1/ezdxf/<id>.ezdxf.model1.skipped`.

### The compared set, and what is excluded

For a pair, the compared set is the intersection of the names both sides wrote.
A name on one side only is never silently absent: it is a finding unless
`parity/MODEL1.toml` lists it under `excluded` for the engine that lacks it, with
a reason and an issue. `tests/parity_model1.rs` holds the lists to the data in
both directions, so a listed name the run never needed is as red as an unlisted
one it did.

acadsharp-rs is excluded from the whole stream: the C ABI exposes flattened
primitives only, and growing it is libviprs-dep's call (wire 3,
libviprs-dep#83).

## `layer`

```json
{"model":1,"record":"layer","name":"Walls","flags":0,"color":1,"true_color":-1,"on":true,"linetype":"Continuous","lineweight":-3,"plot":true}
```

One record per LAYER table entry, keyed by the entry name, compared exactly with
its case. Every drawing has a layer `0`, so the set is never empty. The fields
are by the DXF code they come from, in this order, all always present:

| field | type | code | meaning |
|---|---|---|---|
| `name` | string | 2 | the key |
| `flags` | int | 70 | only the bits a layer owns: frozen 1, frozen in new viewports 2, locked 4. The xref bits describe the reference and not the layer |
| `color` | int | 62 | the AutoCAD colour index (its sign is `on`), or -1 when the layer holds a true colour |
| `true_color` | int | 420 | packed `0xRRGGBB`, or -1 when there is none |
| `on` | bool | 62 | false when the stored colour is negative |
| `linetype` | string | 6 | the linetype's name, never its handle |
| `lineweight` | int | 370 | hundredths of a millimetre, or -1 by layer, -2 by block, -3 default |
| `plot` | bool | 290 | the plot flag |

The comparator reports one finding per differing field, named
`layer/<name>.<field>`, and a layer on one side only as `layer/<name>`. A rule
can name every layer's field with a star, `layer/*.lineweight`. A layer's colour
is packed from its red, green and blue components in all three producers: the
packed value ACadSharp exposes is blue first, which is a library convention and
not a property of the layer.

## `linetype`

```json
{"model":1,"record":"linetype","name":"BATTING","description":"Batting SSSS","alignment":65,"pattern_length":0.4001,"segments":[{"length":0.0001,"flags":0,"shape_number":0,"scale":1.0,"rotation":0.0,"offset":[0.0,0.0],"text":"","style":""}]}
```

One record per LTYPE entry, keyed by the entry name. `ByBlock`, `ByLayer` and
`Continuous` are in every file, so the set is never empty and a missing one is a
finding. The fields are by DXF code: `description` (3), `alignment` (72, the
character's code), `pattern_length` (40), and `segments`, a list in file order,
each with `length` (49), `flags` (74), `shape_number` (75), `scale` (46),
`rotation` (50, **radians**, like every angle in schema 1; the DXF writes
degrees), `offset` (44, 45), `text` (9) and `style` (340).

- `style` is the text style's **name**, never its handle. A segment with no
  style has the empty string.
- A code a segment does not write has its default: flags 0, shape number 0, scale
  1.0, rotation 0.0, offset (0, 0), empty text and style.
- Entry names compare exactly, case included. AC1014 stores table names in upper
  case, so its three fixed names read `BYBLOCK`, `BYLAYER` and `CONTINUOUS`.

The comparator unrolls the list: `linetype/<name>.segments.len` and then
`linetype/<name>.segments[i].<field>`. A rule that covers a field of every
segment of every linetype is `linetype/*.segments[*].style`.

## `textstyle`

```json
{"model":1,"record":"textstyle","name":"Standard","font_file":"arial.ttf","big_font_file":"","height":0.0,"width_factor":1.0,"last_height":0.5,"oblique_angle":0.0,"backward":false,"upside_down":false,"is_shape":false,"is_vertical":false,"xref_dependent":false,"annotative":false}
```

One record per STYLE entry, keyed by the name. `Standard` is in every file. The
fields are by DXF code: `font_file` (3), `big_font_file` (4), `height` (40),
`width_factor` (41), `last_height` (42, the **stored** value, not what a reader
would substitute for a zero), `oblique_angle` (50, radians), `backward` and
`upside_down` (the two bits of 71, 2 and 4), `is_shape`, `is_vertical` and
`xref_dependent` (bits 1, 4 and 16 of 70), and `annotative`.

- **An entry with no name has no record.** It cannot be keyed. A STYLE entry
  without a name is a shape file that only a linetype segment points at, and the
  readers disagree on what to call it (ACadSharp invents `unnamed0`, `unnamed1`).
  The oracle drops the names it invented. The count of such entries is not
  compared.
- **The TrueType family name and its flags** live in the style's `ACAD` xdata (1000
  and 1071). Neither engine has the name as a field, so the row for it is the
  `xdata` record on the style, not a field here.
- **A field an engine cannot produce** is written as its zero value there and is
  left out of the comparison by an `excluded` row naming it
  (`textstyle/*.annotative` for the oracle: ACadSharp 3.7.1 has no such property).
  The report counts what was left out, and a row the run never needed is stale.

## `dimstyle`

One record per DIMSTYLE entry, keyed by the name; `Standard` is in every file. The
fields are the `$DIM*` names themselves (`"$DIMASZ"`), in the order of
`DIMENSION_FIELDS` in `tools/model1.py`, which is the order of the schema in
`src/model1.rs` (the parser refuses a line whose fields are in another order).
They are the 75 DIMSTYLE group codes ACadSharp types, and the written form of each
is the one the header table gives the variable of the same name: a reference is the
**name** (`$DIMTXSTY`, `$DIMBLK`, `$DIMBLK1`, `$DIMBLK2`, `$DIMLDRBLK`, `$DIMLTYPE`,
`$DIMLTEX1`, `$DIMLTEX2`), the empty string being no reference; a colour is the
colour index, or -1 for a true colour; a flag is the integer 0 or 1; an angle
(`$DIMJOGANG`) is in radians. A last field, `annotative`, is the
`AcadAnnotative` xdata flag.

- **null.** AutoCAD writes a DIMSTYLE code only when it differs from a default, so a
  `Standard` style in a DXF carries nine codes. A reader of a DXF cannot say what
  the rest are, so ezdxf writes `null` for a field the entry does not store and the
  comparison leaves it out, counted. ACadSharp and acadrust write the value they
  supply for an omitted code, and holding that against the DWG of the same drawing
  is the `dxf-dwg:` rows.
- **annotative** is `excluded` for the oracle, as for the text style.
- An entry with no name has no record.

## `dictionary`

```json
{"model":1,"record":"dictionary","name":"ACAD_MLINESTYLE","hard_owner":false,"cloning":1,"entries":[{"key":"Standard","class":"MLINESTYLE"}],"entry_count":1}
```

One record per DICTIONARY (and DICTIONARYWDFLT) object, keyed by the path of
entry keys from the named object dictionary, `/`-joined. The root is the empty
path. A nested dictionary is an entry of its parent with the class `DICTIONARY`,
and its own record follows under the longer path (`ACAD_GROUP` under the root,
and a dictionary inside it, if there were one, as `ACAD_GROUP/<key>`).

| field | type | code | meaning |
|---|---|---|---|
| `hard_owner` | bool | 280 | the dictionary-level hard ownership flag |
| `cloning` | int | 281 | the duplicate record cloning flag |
| `entries` | list | 3 and 350 | `(key, class)` sorted by key (UTF-8 bytes); `class` is the entry's DXF object name |
| `entry_count` | int | | the number of entries |

- **Entry order is not compared.** ACadSharp keeps entries in a `Dictionary`,
  acadrust and ezdxf in file order, and the record sorts by key. Per-entry hard
  ownership (360 against 350) is acadrust-only and stays out; the
  dictionary-level flag is the compared one.
- **The class is the DXF object name.** `NonGraphicalObject.ObjectName` on the
  oracle, the `ObjectType` variant's DXF name (or `Unknown { type_name }`) on
  acadrust, `dxftype()` on ezdxf. The oracle reads with
  `KeepUnknownNonGraphicalObjects` on, so an entry whose class it has no type for
  stays in the list under the class name the file gives it. The switch keeps an
  object after the reader's notification has fired, and schema 1 emits no
  object, so no schema 1 output moves because of it. The DXF reader needs a
  second switch: it reads the handle of an unknown object only when
  `KeepUnknownEntities` is on, where the name says it means the other one, so
  the oracle sets both for a DXF (libviprs/acadsharp-rs-tests#126). The unknown
  entities that keeps are drawing content, which no model-1 record reads.
- **The key is the file's.** ACadSharp adds each entry under the object's own
  name (`CadDictionaryTemplate.build`), so for a dictionary whose keys are not
  the names (ACAD_SCALELIST, which AutoCAD keys `A0`, `A1`, and so on) the
  oracle's key is the name. Those are `known` rows for the five dictionaries it
  touches (libviprs/acadsharp-rs-tests#124).
- **An entry that names no object** is a dangling reference. A reader that
  resolves the entry has nothing to list, so it is not listed.
- **Extension dictionaries** hang off an owner and not off the root. Those of
  table entries and layouts are in, keyed `table/<kind>/<name>` (the kind is
  the table in lower case: `layer`, `linetype`, `textstyle`, `dimstyle`,
  `block_record`, `appid`, `view`, `vport`, `ucs`) and `layout/<name>`, and the
  dictionaries inside them follow under the longer path. Those of entities are
  keyed by handle only and are not written, until the entity owners of the
  `xdata` kind (R5.5.7) prove the sidecar rule there.
- **A class only one side can name** is listed by name in `parity/MODEL1.toml`
  and is the list R5.6's object rows want.

## Comparing

`src/model1.rs` compares two streams by key, never by position. A double passes
when it is inside the `BOUNDED` class of `docs/TOLERANCES.md` (every pair here has
an unrelated reader in it), everything else must be equal, and a difference of
`type` is its own finding. The verdict is the schema 1 vocabulary
(`src/verdict.rs`): `MATCH`, `KNOWN:n` when every difference is a listed row,
`DIFF:n` for the unlisted ones, and `OPEN_FAILED` when a reader refuses the file.

The rows are in `parity/MODEL1_VERDICTS.toml`, one per (fixture, pair):

| pair | fixture | reads |
|---|---|---|
| `oracle/acadrust` | each of the seven saves | both DWG readers |
| `ezdxf/oracle`, `ezdxf/acadrust`, `oracle/acadrust` | each of the six ASCII twins | all three, the DXF |
| `dxf-dwg:oracle`, `dxf-dwg:acadrust` | each of the six twins | a reader's DXF against its own DWG of that drawing |

They are a sibling of `parity/VERDICTS.toml` and not rows in it: this stream has
no native lane to wait for, and a second file cannot conflict with the schema 1
run's. The ezdxf side is the DXF only, since ezdxf never opens a DWG.

The control is the same shape as schema 1's: one `$LTSCALE` times 1.001 in a
scratch copy of the oracle's stream is one more `DIFF` and names the variable
(`tests/parity_model1.rs`).

## Freezing the ground truths

`tools/model1.py freeze` runs the oracle (`ACadSharp.Reference --model1`) in the
pinned .NET image over the seven AutoCAD saves and the six ASCII DXF twins, and
ezdxf in the pinned Python image over the twins. It writes nothing without `--accept`, for the
reason `tools/regenerate_reference.py` gives: a moved ground truth arrives as a
reviewed diff. `tools/model1.py verify` checks the committed files offline.

## Changing the model

A new kind is a new section above, a new variant in `src/model1.rs`, a new
function in each producer, and a new line in the order table. Adding a field to
an existing kind, or changing a written form, is a change to every producer and
to every frozen file, and it goes through `freeze --accept` with the diff read.
