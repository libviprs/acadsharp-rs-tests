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
| `layer` | follows in its own change | the entry name |

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
because the two files were written at different moments: `$TDCREATE`,
`$TDUPDATE`, `$TDUCREATE`, `$TDUUPDATE`, `$TDINDWG`, `$TDUSRTIMER`,
`$VERSIONGUID`, `$FINGERPRINTGUID`, `$HANDSEED`, and `$ACADMAINTVER`, which is the
maintenance release of whichever AutoCAD wrote the file. They are listed in
`parity/MODEL1.toml` as `known` for the pairs that have an ezdxf side. Between two
readers of one DWG they are not excused: both read the same bytes.

A variable a DWG of an old version does not store is a second class: each reader
reports its own default, and the two defaults can differ. Those are listed per
version, with the DXF twin of that version as the evidence that the variable is
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

## Comparing

`src/model1.rs` compares two streams by key, never by position. A double passes
when it is inside the `BOUNDED` class of `docs/TOLERANCES.md` (every pair here has
an unrelated reader in it), everything else must be equal, and a difference of
`type` is its own finding. The verdict is the schema 1 vocabulary
(`src/verdict.rs`): `MATCH`, `KNOWN:n` when every difference is a listed row,
`DIFF:n` for the unlisted ones. The rows are in `parity/MODEL1_VERDICTS.toml`.
They are a sibling of `parity/VERDICTS.toml` and not rows in it: this stream has
no native lane to wait for, and a second file cannot conflict with the schema 1
run's.

The control is the same shape as schema 1's: one `$LTSCALE` times 1.001 in a
scratch copy of the oracle's stream is one more `DIFF` and names the variable
(`tests/parity_model1.rs`).

## Freezing the ground truths

`tools/model1.py freeze` runs the oracle (`ACadSharp.Reference --model1`) in the
pinned .NET image over the seven AutoCAD saves, and ezdxf in the pinned Python
image over the six ASCII DXF twins. It writes nothing without `--accept`, for the
reason `tools/regenerate_reference.py` gives: a moved ground truth arrives as a
reviewed diff. `tools/model1.py verify` checks the committed files offline.

## Changing the model

A new kind is a new section above, a new variant in `src/model1.rs`, a new
function in each producer, and a new line in the order table. Adding a field to
an existing kind, or changing a written form, is a change to every producer and
to every frozen file, and it goes through `freeze --accept` with the diff read.
