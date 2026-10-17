//! `emit-acadsharp --fixture <id> --out <dir>`: schema 1 from acadsharp-rs.
//!
//! Writes `<id>.acadsharp.jsonl`, `<id>.acadsharp.src.jsonl` (the partial
//! sidecar) and `<id>.acadsharp.run.json` (the crate commit, the native archive
//! it ran against, the target and the fixture's sha). This binary is the wire 2
//! adapter and nothing else: the harness is `acadsharp_rs_tests::emit` and the
//! writer is `acadsharp_rs_tests::canonical`, which `emit-acadrust` goes through
//! too.
//!
//! No cargo feature gates it. `acadsharp-rs` is a normal dependency of this
//! suite and builds without an archive, so the binary exists everywhere and
//! refuses at run time: `Error::Unlinked` is a non-zero exit with a message.
//!
//! Bytes come from `fixtures::load(id)` and never from a path.
//! `tests/fixture_integrity.rs` scans `src/` for reads behind the loader's back,
//! and this file lives under `src/`.
//!
//! Exit codes: 0 written, 1 the engine could not read, no library is linked, or
//! the records could not be encoded, 2 called wrong or the fixture is not in the
//! manifest.

use std::path::Path;
use std::process::ExitCode;

use acadsharp_rs_tests::acadsharp_emit::extract_bytes;
use acadsharp_rs_tests::emit::{
    EmitArgs, RunInfo, checkout_head, native_provenance, write_emission,
};
use acadsharp_rs_tests::fixtures;
use sha2::{Digest, Sha256};

/// The sha `COUNTERPART_REV` pins: the first line that is neither blank nor a
/// comment, the way `tools/counterpart.sh` reads it.
fn pinned() -> String {
    include_str!("../../COUNTERPART_REV")
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty() && !l.starts_with('#'))
        .unwrap_or_default()
        .to_string()
}

fn main() -> ExitCode {
    let args = match EmitArgs::parse(std::env::args().skip(1)) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("emit-acadsharp: {e}\nusage: emit-acadsharp --fixture <id> --out <dir>");
            return ExitCode::from(2);
        }
    };
    // A round-trip output arrives on stdin (R5.3), a fixture is loaded by id.
    let (id, sha256, bytes) = if let Some(kind) = &args.stdin_as {
        if kind != "dwg" {
            eprintln!(
                "emit-acadsharp: acadsharp-rs opens DWG only, so --stdin-as {kind} has no reader here"
            );
            return ExitCode::from(2);
        }
        let mut bytes = Vec::new();
        if let Err(e) = std::io::Read::read_to_end(&mut std::io::stdin(), &mut bytes) {
            eprintln!("emit-acadsharp: stdin: {e}");
            return ExitCode::from(1);
        }
        let sha = format!("{:x}", Sha256::digest(&bytes));
        (args.fixture.clone(), sha, bytes)
    } else {
        let Some(fixture) = fixtures::by_id(&args.fixture) else {
            eprintln!(
                "emit-acadsharp: no fixture {:?} in fixtures/manifest.toml; the manifest is the only list",
                args.fixture
            );
            return ExitCode::from(2);
        };
        match fixture.load() {
            Ok(b) => (fixture.id.clone(), fixture.sha256.clone(), b),
            Err(e) => {
                eprintln!("emit-acadsharp: {e}");
                return ExitCode::from(1);
            }
        }
    };
    let emission = match extract_bytes(&bytes) {
        Ok(e) => e,
        // `Error::Unlinked` is a failure with its message, never an empty
        // success. The two arms read the same on purpose.
        Err(e) => {
            eprintln!("emit-acadsharp: {id}: {e}");
            return ExitCode::from(1);
        }
    };
    let sibling = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("acadsharp-rs");
    let mut run = RunInfo::for_input(
        "acadsharp",
        &pinned(),
        checkout_head(&sibling),
        &id,
        &sha256,
    );
    run.extra = native_provenance(option_env!("ACADSHARP_LINKED_NATIVE_DIR"));
    match write_emission(&args.out, &run, &emission) {
        Ok(paths) => {
            for p in paths {
                println!("{}", p.display());
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("emit-acadsharp: {id}: {e}");
            ExitCode::from(1)
        }
    }
}
