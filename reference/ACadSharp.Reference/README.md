# ACadSharp.Reference

The .NET JIT reference oracle. It reads a DWG through the pinned ACadSharp NuGet
package and writes two artefacts: the canonical semantic stream that a future
`acadsharp-rs` is compared against, and a deterministic SVG of the same records
for a human to look at.

**It is never shipped.** Nothing consumes it but this repository's tests and
`tools/regenerate_reference.py`. It is not packable, it is not published, and
there is no scenario in which it ends up in anything a user installs.

The format it writes is specified in [`docs/CANONICAL_SCHEMA.md`](../../docs/CANONICAL_SCHEMA.md).
Read that first; this file is only about running the thing.

## Why it is the way it is

**Exactly one `PackageReference`, ACadSharp, and no `ProjectReference`.** Two
tests assert it, one here and one in the Rust suite. The oracle must not import
or share code with the production NativeAOT shim, the `acadsharp-rs` FFI, the
VIPRS ABI encoder or the Rust canonical exporter: two implementations that share
code can share a bug and still agree, and an agreement like that is exactly what
a differential test is supposed to be unable to produce. Where the two sides
need the same logic, it is written twice on purpose.

**Ordinary JIT, never NativeAOT.** `<PublishAot>false</PublishAot>` is spelled
out in the csproj so a later "let's just AOT it for speed" has to delete a line
that says why not. JIT ACadSharp is the reference implementation; NativeAOT plus
the C ABI plus Rust is the system under test. If both went through the same
normalization path the differential would compare a bug against itself and pass.

**Locked restore.** `RestorePackagesWithLockFile` plus a committed
`packages.lock.json`, so an ACadSharp bump is a diff in a file rather than
something that happens on whichever machine restores next. `reference/NuGet.config`
clears the package sources and adds only nuget.org, so a machine-level config
with a private feed cannot resolve ACadSharp from somewhere this repository has
never seen while the lock file still looks clean.

**`InvariantGlobalization`.** Set at the project level as well as passing
`CultureInfo.InvariantCulture` at every call site. A missed call site would
format a double with a comma on somebody's de-DE machine and nowhere near CI,
and the whole corpus would turn into noise. The test project deliberately does
*not* set it, so its German-locale test is testing something.

## Running it

Everything below needs the SDK `../global.json` pins. There is no need to
install it: an SDK container works, and is what the regeneration tool uses when
there is no local `dotnet`.

One file:

```bash
dotnet run --project reference/ACadSharp.Reference -- \
    fixtures/dwg/sample_AC1032.dwg \
    --jsonl out/acadsharp-ac1032.reference.jsonl \
    --svg   out/acadsharp-ac1032.reference.svg
```

The whole corpus, driven by the canonical manifest:

```bash
dotnet run --project reference/ACadSharp.Reference -- \
    --manifest fixtures/manifest.toml --all --output out/
```

Output files are named `<fixture id>.reference.jsonl` and
`<fixture id>.reference.svg`, from the `id` in the manifest row.

| option | meaning |
|---|---|
| `--jsonl <path>` | Where the canonical JSONL goes. Required for a single file. |
| `--svg <path>` | Where the SVG goes. Optional for a single file. |
| `--manifest <path>` | `fixtures/manifest.toml`. Needs `--all` and `--output`. |
| `--all` | Process every `[[fixture]]` row. |
| `--output <dir>` | Directory for `--all`. Must already exist. |
| `--no-svg` | With `--all`, skip the SVG artefacts. |
| `--summary <path>` | Write a JSON census of what was produced. |
| `--max-depth <n>` | Block nesting limit, default 16. |
| `--svg-margin <f>` | SVG margin as a fraction of the drawing size, default 0.02. |
| `--verbose` | Echo the reader's own diagnostics to stderr. |

Exit codes: `0` ok, `2` usage, `3` reader failure, `4` unsupported version, `5`
invalid output path, `6` canonical serialization failure, `7` SVG generation
failure, `8` unhandled ACadSharp error.

It never compresses and it never writes into a checked-in directory of its own
accord. `tools/regenerate_reference.py` owns both, and owns the acceptance gate.

## Tests

xunit.v3 test projects are self-hosting executables, and the .NET 10 SDK's
`dotnet test` needs a separate opt-in to reach them, so run the assembly:

```bash
dotnet run --project reference/ACadSharp.Reference.Tests
```

Among them is the self-test: the test project builds a small drawing, writes it
as DWG with ACadSharp's own `DwgWriter`, reads it back and compares the canonical
output against
[`../ACadSharp.Reference.Tests/Expected/selftest.reference.jsonl`](../ACadSharp.Reference.Tests/Expected/selftest.reference.jsonl).
It needs no corpus. A companion test changes the arc's radius and asserts the
output differs, so the comparison is known to be reading the file it thinks it
is rather than passing against anything at all.

To update that expectation after a deliberate change:

```bash
ACADSHARP_REFERENCE_ACCEPT_SELFTEST=1 dotnet run --project reference/ACadSharp.Reference.Tests
```

then read `git diff` before committing it. A Rust test asserts no workflow sets
that variable: an expectation a test run can rewrite is an expectation that
catches nothing.

## Upgrading ACadSharp

1. Change the `Version` on the `PackageReference` and run
   `dotnet restore reference/ACadSharp.Reference.sln` (without `--locked-mode`)
   to update `packages.lock.json`. Commit both.
2. Update `[upstream]` in `fixtures/manifest.toml` if the corpus is moving to
   the new release's samples as well. It does not have to.
3. Run `python3 tools/regenerate_reference.py regenerate`. It will print, per
   fixture, both hashes before and after, the record-count and record-type
   moves, and the first differing JSONL lines, then exit non-zero having written
   nothing.
4. Read that. It is the review, and it is the reason the tool cannot accept its
   own output.
5. `python3 tools/regenerate_reference.py regenerate --accept`, then
   `cargo test` and `git diff`.
6. Rerun the .NET tests; the self-test's expectation may also need accepting,
   which is a second deliberate step for the same reason.

If a release cannot read one of its own samples, that is a finding: fail
explicitly and write it down. Do not drop the fixture.

## Layout

```
ACadSharp.Reference.csproj   one PackageReference, locked, not AOT, invariant
packages.lock.json           the pin
Program.cs                   the CLI, and nothing else
ExportResult.cs              exit codes and the Result type expected failure uses
FixtureManifest.cs           a deliberately tiny TOML subset for the manifest
Canonical/
  CanonicalSchema.cs         the record model
  CanonicalWriter.cs         the only thing that serialises a record
  NumericFormatting.cs       the only thing that formats a number or a string
Cad/
  DocumentExtractor.cs       read, walk the views, order the stream
  EntityExtractor.cs         ACadSharp entity to canonical record
  BlockResolver.cs           recursion policy and the insert transform
  CadTransform.cs            the affine map, and the arbitrary axis algorithm
  CurveMapping.cs            carrying a conic through a transform without flattening it
  WarningCollector.cs        reader notifications to deterministic categories
Svg/
  SvgWriter.cs               the visual oracle
  SvgFormatting.cs           the coordinate convention and XML escaping
```

`FixtureManifest.cs` parses a small TOML subset by hand. The alternative was a
second NuGet package, and the dependency list being exactly ACadSharp is a
property two tests assert. Duplicating a hundred lines of parsing on purpose is
cheaper than weakening that.
