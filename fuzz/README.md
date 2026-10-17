# Fuzzing and Miri for the pure-Rust parsers

Five `cargo-fuzz` targets, one per parser this suite owns or leans on. Every
target body is a function in `src/fuzz_harness.rs`, so the fuzzer, the stable
replay and Miri all run the same code.

| target | bytes go into | what it must never do |
| --- | --- | --- |
| `batch_reader` | `acadsharp_rs::batch::BatchReader::new`, then `records()` until the end or an error | panic |
| `canonical_parser` | `canonical::parse_stream`, then render and parse again | panic, or change a record (down to the bits of a float) on the round trip |
| `fixture_manifest` | `fixtures::parse_manifest_text` | panic |
| `parity_manifest` | `parity::parse_manifest_text` | panic |
| `comparator` | two record sequences built from the bytes with `arbitrary`, into `compare::compare` | panic, or disagree on equal-or-not when the two sides are swapped |

Every target also runs under a counting global allocator that aborts the process
when one input takes more than 64 MiB beyond where it started, and a replay
fails an input that takes longer than five seconds.

**What this does not cover.** Everything here is pure Rust. Miri does not see
foreign code, so none of this says anything about the NativeAOT library or the C
ABI, and nothing in this directory claims to. The leak and sanitizer harnesses
for that boundary are H4.3.

## Running it

The corpus replay is an ordinary test and runs in `cargo test`:

```
cargo test --test fuzz_corpus_replay --test alloc_cap
```

The fuzzer needs nightly and `cargo-fuzz`. New units go to a scratch directory
so the committed corpus only changes when somebody means it to:

```
mkdir -p /tmp/work
cargo +nightly fuzz run batch_reader /tmp/work fuzz/corpus/batch_reader -- -max_total_time=600
cargo +nightly fuzz run canonical_parser /tmp/work fuzz/corpus/canonical_parser -- -dict=fuzz/canonical.dict -max_total_time=600
```

A crash lands in `fuzz/artifacts/<target>/`. Fix the bug, then copy the input
into `fuzz/corpus/<target>/` as `found_<what>` so `fuzz_corpus_replay` fails on
every PR that brings it back.

The seeds come from `cargo run --example fuzz_seeds`. It builds the wire seeds
with `acadsharp-rs`'s own test builder (`tests/wire/mod.rs`), copies its four
captured batches, takes the first lines of four frozen expected outputs, and
writes the malformed cases the unit tests already name. It never deletes, so the
`found_*` entries stay.

Miri runs the targets listed in `miri-allowlist.txt` (the file the CI job reads
and `tests/miri_allowlist.rs` checks). The three tests it skips read
multi-megabyte frozen outputs through the zstd decoder and still run natively on
every PR, and it replays only corpus entries up to 16 KiB.

## What I ran, and where

All of it inside throw-away containers on MARS (native x86_64), nightly Rust
with `miri` and `cargo-fuzz` installed into a local image I removed afterwards.

### Real findings

The first fuzz run of the `comparator` target panicked in debug-assertion
builds, twice, on integer overflow in the "same record in another place" search
in `src/compare.rs`: `p[0] + o[0]` on a quantised coordinate at the end of the
`i64` range (a `1e300` coordinate casts there), and `(p - q).abs()` in `compat`
between `i64::MAX` and `i64::MIN`. Fixed with `saturating_add` and `abs_diff`
(libviprs/acadsharp-rs-tests#51). The input that reproduces the first is
`corpus/comparator/found_i64_overflow.bin`.

After the fixes each target ran clean against the real code: 90 to 120 seconds
each, between 15 thousand (the 200 KB manifests) and 12 million (the wire
reader) executions.

### Bugs I introduced on purpose

For each parser I made a change in a scratch copy, started the fuzzer from a
single seed, and watched it find the change. The crash input was then added to
the corpus as a `found_*` file (it replays clean on the real code), and the
replay was run against the scratch copy with that input added to show the PR
step goes red.

| target | bug introduced | found by the fuzzer | replay on the bug |
| --- | --- | --- | --- |
| `batch_reader` | the record `length > remaining` check removed and the payload taken by a direct slice | yes, from one captured batch, panic in `step` | red |
| `canonical_parser` | the writer stops escaping `\` in strings | yes, from one `text` line plus `fuzz/canonical.dict` | red |
| `comparator` | both overflow fixes above reverted | yes, from one 128 byte seed, overflow panic | red |
| `fixture_manifest` | `toml::from_str(..)?` became `.unwrap()` | yes, on the empty input, which is the cheapest case | red |
| `parity_manifest` | the same change | yes, on the empty input | red |

The two `.unwrap()` crashes are the empty file, which `empty.toml` already seeds,
so those entries are not added a second time.

One attempt did **not** work and I am saying so: I first tried to make
`canonical_parser` accept negative zero (the check at the `double` function). The
fuzzer did not find it in three minutes, twice, from a 9 KB seed and from a
one-line seed with the dictionary. The committed seed
`corpus/canonical_parser/negative_zero.jsonl` does catch it on replay (the bits
of the float change on the round trip), so the replay is the guard for that
class, and the fuzzer is not. I swapped the bug in the table for the backslash
one, which it does find.

### The checks on the checks

* **The cap is live.** `ALLOC_CAP` set to `usize::MAX / 4` in a scratch copy:
  `tests/alloc_cap.rs` fails with "a 128 MiB allocation went through; the cap is
  not live". With the real cap it passes, and the child process aborts with
  "allocation cap exceeded".
* **The allowlist refuses a stub-linking test.** A scratch `tests/stubby.rs`
  that uses `acadsharp_rs::Decoder`, added to `miri-allowlist.txt`:
  `nothing_on_the_allowlist_reaches_the_native_library` fails and names
  `tests/stubby.rs`.
* The 2^31-points batch (`polyline_two_pow_31_points.bin`) is refused by
  arithmetic in the reader and never allocates, so it does not exercise the cap.
  The cap is shown by the allocation test above instead.

### Toolchain and final runs

rustc 1.101.0-nightly (5d8fec066 2026-10-16), miri from the same nightly,
cargo-fuzz 0.13.2, stable 1.99.1 for the gate. On the final tree, with the real
code:

* `cargo +nightly miri test` with `MIRIFLAGS="-Zmiri-strict-provenance -Zmiri-disable-isolation"`, over
  `miri-allowlist.txt`: `lib` 1 passed, `canonical_parser` 26 passed (1 skipped),
  `canonical_writer` 8 passed (1 skipped), `comparator` 57 passed (about five
  and a half minutes), `fuzz_corpus_replay` 3 passed. No undefined behaviour reported.
* 60 seconds of each fuzz target from the committed corpus: no crash. 6.3 million
  runs of `batch_reader`, 303 thousand of `canonical_parser`, 4.3 thousand of
  `fixture_manifest`, 75 thousand of `parity_manifest`, 84 thousand of `comparator`.
  That is a smoke run. The weekly workflow runs ten minutes each.
