//! `rewrite-acadrust --out <dir> [--only <id>]`: acadrust as a writer (R5.3).
//!
//! For every output of the plan whose writer is acadrust, reads the source
//! fixture, writes it again at the plan's version and container, and does it a
//! second time to check the two are byte-identical (acadrust is deterministic,
//! and the round-trip matrix is only reproducible if it stays so). The files go
//! to `<dir>/<id>.dwg` or `<dir>/<id>.dxf`; a write that fails goes to
//! `<dir>/<id>.error.txt` with the writer's own message, because a version a
//! writer claims and cannot write is a finding and not a missing file.
//!
//! `tools/regenerate_written.py` ingests the directory. This binary writes
//! nothing under `fixtures/`.
//!
//! Exit codes: 0 every output was attempted and written or recorded as failed,
//! 1 two writes of one output differed or a source could not be loaded, 2 called
//! wrong.

use std::path::PathBuf;
use std::process::ExitCode;

use acadsharp_rs_tests::acadrust_emit::rewrite;
use acadsharp_rs_tests::fixtures;
use acadsharp_rs_tests::written;
use sha2::{Digest, Sha256};

fn main() -> ExitCode {
    let mut out = None;
    let mut only = Vec::new();
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--out" => out = args.next().map(PathBuf::from),
            "--only" => only.extend(args.next()),
            other => {
                eprintln!("rewrite-acadrust: I do not know the argument {other:?}");
                return ExitCode::from(2);
            }
        }
    }
    let Some(out) = out else {
        eprintln!("usage: rewrite-acadrust --out <dir> [--only <id>]");
        return ExitCode::from(2);
    };
    if let Err(e) = std::fs::create_dir_all(&out) {
        eprintln!("rewrite-acadrust: {}: {e}", out.display());
        return ExitCode::from(1);
    }
    let mut attempted = 0usize;
    for stage in ["one", "two"] {
        for o in written::outputs(stage)
            .into_iter()
            .filter(|o| o.writer == "acadrust")
        {
            let id = o.id();
            if !only.is_empty() && !only.contains(&id) {
                continue;
            }
            attempted += 1;
            let bytes = match fixtures::load(&o.source) {
                Ok(b) => b,
                Err(e) => {
                    eprintln!("rewrite-acadrust: {id}: {e}");
                    return ExitCode::from(1);
                }
            };
            // A writer that panics is a writer that cannot write this: record it.
            let attempt = || {
                std::panic::catch_unwind(|| rewrite(&bytes, &o.container, &o.version))
                    .unwrap_or_else(|_| Err("the writer panicked".to_string()))
            };
            match (attempt(), attempt()) {
                (Ok(a), Ok(b)) => {
                    if a != b {
                        eprintln!("rewrite-acadrust: {id}: two writes of one output differ");
                        return ExitCode::from(1);
                    }
                    let path = out.join(format!("{id}.{}", o.extension()));
                    if let Err(e) = std::fs::write(&path, &a) {
                        eprintln!("rewrite-acadrust: {}: {e}", path.display());
                        return ExitCode::from(1);
                    }
                    println!("{id} {:x} {}", Sha256::digest(&a), a.len());
                }
                (Err(e), _) | (_, Err(e)) => {
                    let path = out.join(format!("{id}.error.txt"));
                    if let Err(w) = std::fs::write(&path, format!("{e}\n")) {
                        eprintln!("rewrite-acadrust: {}: {w}", path.display());
                        return ExitCode::from(1);
                    }
                    println!("{id} FAILED {e}");
                }
            }
        }
    }
    if attempted == 0 {
        eprintln!("rewrite-acadrust: nothing matched, which is not a success");
        return ExitCode::from(1);
    }
    ExitCode::SUCCESS
}
