# Upstream test parity

ACadSharp's own test suite is the closest thing to a specification of what a DWG
reader has to do. This directory records where every one of its test files
stands relative to this suite, so upstream coverage stops drifting silently.

It classifies. It does not port. Turning rows into real Rust tests is
[H4.2](https://github.com/libviprs/acadsharp-rs-tests/issues/10).

```
ACadSharp @ d7dc111
  src/ACadSharp.Tests/**.cs          112 files
            |
            v
   manifest.toml, one row each
            |
            +--> mirrored            a Rust test reproduces it
            +--> differential        the oracle comparison covers it
            +--> native_conformance  checked against ACadSharp itself
            +--> not_applicable      out of scope, with the reason
```

## The four statuses

**`mirrored`** means a Rust test in this repository reproduces what the upstream
test asserts. The row lists the files in `rust`, and every one of them has to
exist. A row cannot claim this without something behind it.

**`differential`** means the JSONL comparison against the .NET oracle covers it
end to end. This is real coverage, not a placeholder: the same DWG goes through
ACadSharp and through `acadsharp-rs`, and the canonical records are compared.
For a rendering-contract suite that is often a stronger check than a mirrored
unit test, because it compares against the reference implementation's actual
output rather than against somebody's reading of it.

**`native_conformance`** means the contract lives in the object model rather
than in anything rendered, so it is checked against ACadSharp directly. Header
variables, symbol tables, dictionaries, XData and the mesh entities are here:
the canonical schema has no record for them, so there is nothing for a rendering
comparison to assert.

**`not_applicable`** means out of scope, and the row says why. Three families:
writer suites (this crate exposes no writer), DXF suites (it opens only DWG),
and test infrastructure that makes no assertions of its own.

## `not_applicable` never counts as coverage

This is the rule the status exists for. A writer suite is not a gap in our
coverage, it is something we deliberately do not do. If `not_applicable`
counted, the coverage number would fall every time upstream added a writer test
and rise every time they deleted one, which tells nobody anything about this
crate.

So the denominator is **applicable**, meaning total minus `not_applicable`, and
`tools/parity_inventory.py` prints both numbers so the split is always visible.

At `d7dc111`: 112 files, 29 `not_applicable`, so 83 applicable.

## `planned_rust`, and why it is not `rust`

A row that will become `mirrored` carries `planned_rust` pointing at where the
test will go. It stays `differential` until the file actually exists.

Two rows of the same table cannot both be true, and the manifest has to pick the
one that is. A `mirrored` row whose `rust` path does not exist yet is a claim of
coverage with nothing behind it, and it would read as done in every count. So
`rust` means it exists and `planned_rust` means it does not, and
`tests/parity_manifest.rs` enforces both directions.

The second direction is the useful one: once a `planned_rust` file appears, the
test goes red until the row is promoted to `mirrored`. Without that, H4.2 could
write every test and the manifest would still report zero coverage forever,
because nothing would force anybody to go back and update it.

## Checking it

```bash
python3 tools/parity_inventory.py                       # offline, the committed snapshot
python3 tools/parity_inventory.py --upstream-dir ../ACadSharp
python3 tools/parity_inventory.py --upstream-commit <sha>   # a proposed bump, over the API
python3 tools/parity_inventory.py --write-snapshot          # refresh the snapshot for a bump
```

Offline by default, and that is what CI runs, alongside `cargo test`.

`inventory.<sha>.txt` is the committed file list for the pinned commit, named
after the commit rather than being a fixed filename. A fixed name would let a
bump compare the new manifest against the previous revision's file list, which
would pass and mean nothing.

The tool never edits the manifest. Deciding what a new upstream test file means
here is the one thing in this directory that needs a person, and a tool that
guessed would turn a coverage decision into a silent diff.

## Bumping ACadSharp

[H3.4](https://github.com/libviprs/acadsharp-rs/issues/6) wires this into every
proposed bump. By hand:

1. `python3 tools/parity_inventory.py --upstream-commit <new sha>` and read what
   it lists. Additions need a status and a reason; removals mean a row goes or
   follows a rename.
2. Update `manifest.toml`, including `upstream.commit` and `upstream.version`.
3. `python3 tools/parity_inventory.py --write-snapshot` to record the new file
   list, then delete the old `inventory.<old sha>.txt`.
4. `cargo test --test parity_manifest`.

As a live example, upstream `master` today carries 115 test files against this
manifest's 112. The three it adds are
`Tables/Collections/LayersTableTests.cs`, `Tables/Collections/LineTypesTableTests.cs`
and `Tables/Collections/TableEntryCommonTests.cs`, and the tool names all three
rather than letting the bump through.

## The baselines

`manifest.toml` also records upstream's `Data/sample_AC10xx_tree.json`, one per
DWG generation, which are ACadSharp's own expected document structures. They are
a second oracle, a baseline ACadSharp holds itself to, so the reference
generator may consult them. They live beside the test sources rather than under
`samples/`, which is worth writing down because the design document says
otherwise.
