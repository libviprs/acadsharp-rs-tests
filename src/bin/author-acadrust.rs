//! `author-acadrust --out <dir>`: acadrust as the writer of the code page fixtures
//! (R5.4). For every acadrust output of `fixtures/codepage_plan.toml`, builds the
//! authored drawing and writes it twice, refusing a pair that differs. Files go to
//! `<dir>/<id>.dwg` or `.dxf`; a write that fails goes to `<dir>/<id>.error.txt`.
//! `tools/regenerate_codepage.py` ingests the directory.

use std::path::PathBuf;
use std::process::ExitCode;

use acadsharp_rs_tests::acadrust_emit::author;
use acadsharp_rs_tests::codepage;
use sha2::{Digest, Sha256};

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let mut out = None;
    while let Some(a) = args.next() {
        match a.as_str() {
            "--out" => out = args.next().map(PathBuf::from),
            other => {
                eprintln!("author-acadrust: I do not know the argument {other:?}");
                return ExitCode::from(2);
            }
        }
    }
    let Some(out) = out else {
        eprintln!("usage: author-acadrust --out <dir>");
        return ExitCode::from(2);
    };
    if let Err(e) = std::fs::create_dir_all(&out) {
        eprintln!("author-acadrust: {}: {e}", out.display());
        return ExitCode::from(1);
    }
    let plan = codepage::plan();
    for o in codepage::outputs()
        .into_iter()
        .filter(|o| o.writer == "acadrust")
    {
        let family = plan
            .family
            .iter()
            .find(|f| f.page == o.page)
            .expect("a family");
        let texts: Vec<String> = family
            .texts(&plan.unheld.text)
            .into_iter()
            .map(|a| a.text)
            .collect();
        let id = o.id();
        let attempt = || {
            std::panic::catch_unwind(|| author(&texts, &family.header, &o.container, &o.version))
                .unwrap_or_else(|_| Err("the writer panicked".to_string()))
        };
        match (attempt(), attempt()) {
            (Ok(a), Ok(b)) => {
                if a != b {
                    eprintln!("author-acadrust: {id}: two writes of one output differ");
                    return ExitCode::from(1);
                }
                let path = out.join(format!("{id}.{}", o.extension()));
                if let Err(e) = std::fs::write(&path, &a) {
                    eprintln!("author-acadrust: {}: {e}", path.display());
                    return ExitCode::from(1);
                }
                println!("{id} {:x} {}", Sha256::digest(&a), a.len());
            }
            (Err(e), _) | (_, Err(e)) => {
                let path = out.join(format!("{id}.error.txt"));
                let _ = std::fs::write(&path, format!("{e}\n"));
                println!("{id} FAILED {e}");
            }
        }
    }
    ExitCode::SUCCESS
}
