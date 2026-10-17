# Tolerances

How `src/compare.rs` judges two canonical streams. A class per row, never a
number per test: the classes are modelled on the ones libviprs-tests holds its
CLI cells to (`EXACT`, `EXACT-AFTER-CAST`, `BOUNDED-TOL`, `FOURIER`,
`GOLDEN-ONLY`, `EXCLUDED`, listed in libviprs-cli's `OP_MAP.md`).

**Every bound here is provisional.** A bound only moves with fixture evidence:
the comparator's report carries the largest absolute and relative error it saw
per field kind, so a bound sitting a thousand times above anything the corpus
produced is visible in every run, and tightening it is a mechanical change. The
numbers below are read by `tests/tolerances_doc.rs` and compared with the
constants in `src/compare.rs`, so widening a bound in code alone, or in this
file alone, fails a test.

## The rule for a float

A float passes when `|a - b| <= max(abs, rel * max(|a|, |b|))`. It is written
out so "rel" has one meaning: symmetric, no division, and two zeros are equal.
There is no global epsilon, because one would hide millimetre errors in
large-coordinate drawings.

## Bounds

| class | abs | rel | why |
|---|---|---|---|
| `BOUNDED_TIGHT` | `1e-9` | `1e-12` | Oracle against acadsharp-rs. H1.4's opening values. Not "the same library on two runtimes": the oracle flattens in `reference/` and the archive flattens in libviprs-dep's `Flattener.cs`, so the noise is transform-order noise. The first R3.1 run measures it, and the number moves only with that evidence. |
| `BOUNDED` | `1e-9` | `1e-6` | Any pair with acadrust or ezdxf in it. The decision plan's number, with a `1e-9` absolute floor. |

## Classes

| class | applies to |
|---|---|
| `EXACT` | Record type, counts, view names and kinds, strings, integers, flags, warning categories. Equality of the parsed value and nothing else: an NFC/NFD difference is a finding, and so is a trailing space. |
| `BOUNDED_TIGHT` | Floats, for a pair where both sides are the oracle or acadsharp-rs. |
| `BOUNDED` | Floats, for any pair involving acadrust or ezdxf. |
| `ANGLE` | The fields the schema defines on a circle: `text.rotation`, and the `start` of an `arc` and an `ellipse`, all in `[0, 2*pi)`. The wrapped difference in `(-pi, pi]` is judged by the pair's bound. `end` is **not** wrapped: the schema spells it `start + sweep` and lets it exceed `2*pi`, so a wrapped `end` would call a full turn a match. It is compared unwrapped, after allowing for the whole turn that `start` itself wrapped by, which is the same as comparing `end - start` unwrapped and keeps a perturbed `start` one finding rather than two. |
| `KNOWN_DIVERGENCE` | A listed (fixture, pair, record, field) with both values as recorded, a reason and an issue. The field must still differ by exactly those values. A divergence that disappears is red and asks for the row to be tightened; one that moved to a different wrong value is red and asks to be re-argued. The record is named by the oracle side's index when the oracle is one side of the pair (the one coordinate that cannot shift), and by sidecar handle path otherwise. |
| `EXCLUDED` | A record kind, or a field, one side cannot produce by design, with a reason and an issue. Never silently absent: the report counts what was left out. Each pair has its own explicit `warning` row, because `entity_type` is an ACadSharp type name and `reader_notification` is a claim about one adapter. |
| `DUAL_ORACLE` | A disagreement no ground truth settles. Listed with a reason and an issue, reported, not scored. |

A row of `EXCLUDED`, `KNOWN_DIVERGENCE` or `DUAL_ORACLE` with no reason or no
issue is refused before anything is compared.

## Pairs

The class is chosen per pair. Oracle and acadsharp-rs are one reader behind two
flatteners, so what separates them is structural and the float noise is small;
acadrust and ezdxf are unrelated readers. A pair is unordered here, and an
engine against itself (one engine across the seven saves) gets the class of its
own row.

| pair | float class |
|---|---|
| `oracle-oracle` | `BOUNDED_TIGHT` |
| `oracle-acadsharp` | `BOUNDED_TIGHT` |
| `oracle-acadrust` | `BOUNDED` |
| `oracle-ezdxf` | `BOUNDED` |
| `acadsharp-acadsharp` | `BOUNDED_TIGHT` |
| `acadsharp-acadrust` | `BOUNDED` |
| `acadsharp-ezdxf` | `BOUNDED` |
| `acadrust-acadrust` | `BOUNDED` |
| `acadrust-ezdxf` | `BOUNDED` |
| `ezdxf-ezdxf` | `BOUNDED` |

What a pair excludes or lists as a known divergence is that pair's rows, added
with the run that found them, and they are data: `parity/TABLES.toml`, read by
`src/tables.rs` into the comparator's class tables. Every row has a reason and an
issue, and the loader refuses one without either. The rows as of the first parity
run are below, and `tests/tables_doc.rs` holds this section to the file.

### Pair rows

| pair | row | class | issue |
|---|---|---|---|
| `oracle/acadsharp`, `acadsharp/acadrust` | record `point` | `EXCLUDED` | libviprs/libviprs-dep#104 |
| `oracle/acadsharp`, `acadsharp/acadrust` | record `polygon` | `EXCLUDED` | libviprs/acadsharp-rs-tests#59 |
| `oracle/acadsharp`, `acadsharp/acadrust` | `document.maintenance_version` | `EXCLUDED` | libviprs/acadsharp-rs-tests#61 |
| `oracle/acadsharp`, `acadsharp/acadrust` | `spline.fit_points` | `EXCLUDED` | libviprs/acadsharp-rs-tests#58 |
| `oracle/acadsharp`, `acadsharp/acadrust` | `text.width_factor`, `text.halign`, `text.valign`, `text.style` | `EXCLUDED` | libviprs/acadsharp-rs-tests#66 |
| `oracle/acadrust` | `document.maintenance_version` | `EXCLUDED` | libviprs/acadsharp-rs-tests#23 |
| `oracle/acadsharp` | warnings | `EXCLUDED` | libviprs/acadsharp-rs-tests#22 |
| `oracle/acadrust` | warnings | `EXCLUDED` | libviprs/acadsharp-rs-tests#23 |
| `acadsharp/acadrust` | warnings | `EXCLUDED` | libviprs/acadsharp-rs-tests#25 |
| `ezdxf/oracle`, `ezdxf/acadsharp`, `ezdxf/acadrust` | `document.format`, `document.maintenance_version`, warnings | `EXCLUDED` | libviprs/acadsharp-rs-tests#24 |
| `ezdxf/acadsharp` | the wire 2 gaps above (`point`, `polygon`, `spline.fit_points`, the four `text` fields) | `EXCLUDED` | as above |
| `self/oracle`, `self/acadsharp`, `self/acadrust` | `document.acad_version`, `document.maintenance_version` (a save's version is what makes it a different save) | `EXCLUDED` | libviprs/acadsharp-rs-tests#24 |
| `self/oracle`, `self/acadsharp`, `self/acadrust` | warnings | `EXCLUDED` | libviprs/acadsharp-rs-tests#25 |
| `self/oracle` | the differences `parity/oracle_invariance.toml` lists as exceptions | `KNOWN` (the verdict) | per row, in that file |
| `oracle/acadrust` | `text.text` of `acadsharp-ac1014`, `ac1015`, `ac1018` | `KNOWN_DIVERGENCE` | libviprs/libviprs-dep#111 |
| `oracle/acadrust` | `text.text` of `acadsharp-ac1032`, `acadsharp-sample-base` | `KNOWN_DIVERGENCE` | libviprs/libviprs-dep#112 |
| `oracle/acadrust` | `spline.weights.len` of `g13_hatch` | `KNOWN_DIVERGENCE` | libviprs/acadsharp-rs-tests#67 |

The DXF lane (R5.1) keys its tables `dxf:a/b` (reader against reader on a DXF, the class the same pair has on the DWG), `dxf-dwg:e` (a reader's DXF against its own DWG reading of the same drawing, `BOUNDED` whichever reader it is) and `dxf-ascii-binary:e` (a reader's ASCII against its binary reading, `BOUNDED_TIGHT`: sixteen digits against the raw doubles). acadsharp-rs has no key: its C ABI opens DWG only, so its cells are `EXCLUDED` in the verdict file.

| pair | row | class | issue |
|---|---|---|---|
| `dxf:oracle/acadrust`, `dxf:ezdxf/oracle`, `dxf:ezdxf/acadrust` | warnings | `EXCLUDED` | libviprs/acadsharp-rs-tests#27 |
| `dxf-dwg:oracle`, `dxf-dwg:acadrust` | warnings, `document.format`, `document.maintenance_version` | `EXCLUDED` | libviprs/acadsharp-rs-tests#27 |
| `dxf-ascii-binary:oracle`, `dxf-ascii-binary:acadrust`, `dxf-ascii-binary:ezdxf` | warnings | `EXCLUDED` | libviprs/acadsharp-rs-tests#27 |
| `dxf:oracle/acadrust`, `dxf:ezdxf/acadrust` | `document.maintenance_version` | `EXCLUDED` | libviprs/acadsharp-rs-tests#96 |
| `dxf-ascii-binary:ezdxf`, `dxf:ezdxf/oracle`, `dxf:ezdxf/acadrust` | `text.text` of the four MTEXTs whose paragraph break is `\P` in AutoCAD's binary DXF | `KNOWN_DIVERGENCE` | libviprs/acadsharp-rs-tests#24 |
| `dxf-dwg:acadrust` | `text.text` of `dxf-sample-ac1015-ascii`, `-binary` (the MIF escape) | `KNOWN_DIVERGENCE` | libviprs/libviprs-dep#111 |
| `dxf:oracle/acadrust`, `dxf:ezdxf/oracle` | `text.text` of the embedded-MTEXT attribute in `dxf-sample-ac1032-*` | `KNOWN_DIVERGENCE` | libviprs/libviprs-dep#112 |
| `dxf-dwg:oracle`, `dxf-dwg:acadrust` | the spline's `control_points.len`, `knots.len`, `fit_points.len` from AC1027 | `KNOWN_DIVERGENCE` | libviprs/libviprs-dep#150 |

The round-trip lane (R5.3) keys its tables `w-<writer>:<reader>`: a reader's reading of a drawing a writer wrote, held against the writer's own reading of the source (the oracle for ACadSharp, acadrust for acadrust). The class is the one the same two engines have on the DWG. A finding is explained, and counts as `KNOWN`, when it is a difference R3.4's invariance list names for that version (ACadSharp's rewrites, judged against the oracle's reference) or one the reader already has against the same reference record on the source itself (R4.1's verdict for the pair, inherited); anything else a round trip adds is the writer's.

| pair | row | class | issue |
|---|---|---|---|
| `w-acadsharp:*`, `w-acadrust:*` (eight keys) | warnings | `EXCLUDED` | libviprs/acadsharp-rs-tests#29 |
| the same eight | `document.acad_version`, `document.format`, `document.maintenance_version` (what was written is what changed; the version is held by a `w-<writer>:version` row against the oracle) | `EXCLUDED` | libviprs/acadsharp-rs-tests#29 |
| `w-acadsharp:acadsharp`, `w-acadrust:acadsharp` | the wire 2 gaps above | `EXCLUDED` | as above |
| `w-acadsharp:acadrust`, `w-acadsharp:acadsharp` | the `KNOWN_DIVERGENCE` rows of `oracle/acadrust` and `oracle/acadsharp` for `acadsharp-ac1032`, copied to each output | `KNOWN_DIVERGENCE` (red if it stops holding) | per row, as in R4.1 |

Everything else the run finds that no row lists is a `DIFF` or a `COUNT` in
`parity/VERDICTS.toml`, with an issue, and is debt rather than an exception.

## What the comparator does not do

It compares parsed values and never normalises: whatever the schema forbids (a
CR, `-0.0`, a non-finite number, an integer token where a double belongs) is the
parser's refusal. It never re-derives geometry: an arc spelled with the opposite
normal and swapped angles is the same curve, and still a finding, because
deciding equivalent forms is the schema's job (`CANONICAL_SCHEMA.md`,
Coordinates). Number spelling is never compared: the oracle's `R` formatting
can write `1E-05` where a Rust emitter writes `1e-5`, and both parse to one
double.

## Alignment

Records are aligned on content before they are compared, with or without a
sidecar, because the frozen oracle has none. The key is the record type, the
view, and the first coordinate triple rounded to `1e-3` (for a warning, its
three strings). Two keys agree when they are within one rounding step, so a
perturbation small enough to be a tolerance question keeps its key and lands as
one field mismatch, never a delete plus an insert. One dropped record is one
finding, a swap is one order finding, and a replaced record is one finding.
