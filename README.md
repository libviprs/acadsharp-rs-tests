# acadsharp-rs-tests

The test suite and fixtures for
[`acadsharp-rs`](https://github.com/libviprs/acadsharp-rs).

They live here rather than in the crate because DWG fixtures are large and
numerous, and nobody consuming `acadsharp-rs` should pay for them. This is the
same split `libviprs` and `libviprs-tests` already use.

## Status

Scaffolding. The crate compiles and the CI gate runs, and that is all so far.

## Requirements

- **Rust 1.97+** (edition 2024)

## Open question this repo does not answer yet

How `acadsharp-rs` gets laid down beside this one so the suite can build
against it. That is a design decision for the epic rather than something to
default into, and the org has two scars from getting it wrong: a sync gate that
resolved an unset repository variable to "the default branch" and compared
against whatever moved that morning, and an integration job that could clone a
stale same-named counterpart branch and build against the wrong tree. Whatever
lands should be a committed pin rather than a repository variable, and should
fail rather than quietly fall back.

## Licence

MIT.
