# ACadSharp.Generator

Writes this suite's generated DWG fixtures with ACadSharp's own `DwgWriter`, at
the ACadSharp version the oracle reads them with (3.7.1, held equal by
`tests/generator_pin.rs`).

It is a separate project from `ACadSharp.Reference` on purpose. The oracle has
one package and no project reference so it stays an independent witness, and a
writer inside it would end that. The two share a package version and nothing
else.

## Commands

```
ACadSharp.Generator list                          name, version, defining source file
ACadSharp.Generator generate <out-dir> [name...]  write <name>.dwg, print name, version, bytes, sha256
ACadSharp.Generator check <dir> [name...]         rewrite in memory, exit 1 if a file in <dir> differs
ACadSharp.Generator selftest                      write one drawing twice, an interval apart, exit 1 if the bytes differ
```

A drawing is a `Drawing` record in `Catalog.cs`: a name (the fixture id and the
file stem), a DWG version, the one source file that defines it, and a function
that fills an empty, pinned document. Add a drawing by adding a file under
`Drawings/` and one line to `Catalog.All`.

## Which versions

`DwgWriter` 3.7.1 writes AC1015, AC1018, AC1024, AC1027 and AC1032 and throws
`CadNotSupportedException` for AC1021 (measured, not read from the docs). The
drawings default to **AC1032**, the generation the real-world samples
(`sample_AC1032.dwg`) are, and a drawing that needs another says so in its
catalog entry and in its manifest row.

## Why the bytes are the same every time

`DwgWriter` writes the time and two GUIDs into the file. Written twice, an
interval apart, one document differs by 160 to 4000 bytes, all inside the
compressed header sections. `Writer.NewDocument` therefore sets the creation and
update stamps (local and universal), both elapsed-time counters, the
fingerprint and version GUIDs and the summary info's two dates to constants. The
two GUID properties have no public setter in 3.7.1, so they are set through the
private one by reflection, and the generator fails loudly if that setter ever
goes away.

`selftest` checks this rather than trusting it: it writes one drawing twice 1.5
seconds apart and compares, and then does the same with the stamps left alone as
a control, which has to differ. A control that did not differ would make the
first comparison prove nothing.

## Running it

Any .NET SDK matching `../global.json` works. On a machine without one, in a
throw-away container:

```
docker run --rm --platform linux/amd64 -v "$PWD/reference":/work -w /work/ACadSharp.Generator \
  -e HOME=/tmp -e DOTNET_CLI_TELEMETRY_OPTOUT=1 \
  mcr.microsoft.com/dotnet/sdk:10.0.401-noble dotnet run -c Release -- selftest
```

## Provenance in the manifest

A generated drawing is a `[[fixture]]` row like any other, with
`source = "generated"`, tier B, and three extra fields: `generated_by` (the
generator project), `generator_source` (the file that defines the drawing) and
`generator_source_sha256` (that file's hash), next to `acadsharp_generator_version`.
