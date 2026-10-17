//! `emit-acadrust --fixture <id> --out <dir>`: schema 1 from acadrust.
//!
//! Writes `<id>.acadrust.jsonl`, `<id>.acadrust.src.jsonl` (the sidecar) and
//! `<id>.acadrust.run.json` (the fork commit, the thread count, the target and
//! the fixture's sha). This binary is the acadrust adapter and nothing else:
//! the harness is `acadsharp_rs_tests::emit` and the writer is
//! `acadsharp_rs_tests::canonical`, which `emit-acadsharp` goes through too.
//!
//! Exit codes: 0 written, 1 the engine could not read or the records could not
//! be encoded, 2 called wrong or the fixture is not in the manifest.

use std::path::Path;
use std::process::ExitCode;

use acadsharp_rs_tests::acadrust_emit::extract_bytes;
use acadsharp_rs_tests::emit::{EmitArgs, RunInfo, checkout_head, write_emission};
use acadsharp_rs_tests::fixtures;

/// The sha `ACADRUST_REV` pins: the first line that is neither blank nor a
/// comment, the way `tools/counterpart.sh` reads it.
fn pinned() -> String {
    include_str!("../../ACADRUST_REV")
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
            eprintln!("emit-acadrust: {e}\nusage: emit-acadrust --fixture <id> --out <dir>");
            return ExitCode::from(2);
        }
    };
    let Some(fixture) = fixtures::by_id(&args.fixture) else {
        eprintln!(
            "emit-acadrust: no fixture {:?} in fixtures/manifest.toml; the manifest is the only list",
            args.fixture
        );
        return ExitCode::from(2);
    };
    let bytes = match fixture.load() {
        Ok(b) => b,
        Err(e) => {
            eprintln!("emit-acadrust: {e}");
            return ExitCode::from(1);
        }
    };
    let emission = match extract_bytes(&bytes) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("emit-acadrust: {}: {e}", fixture.id);
            return ExitCode::from(1);
        }
    };
    // model-1 (docs/MODEL_1.md): the header variables, in a file beside the others.
    let doc = match acadrust::io::dwg::DwgReader::from_stream(std::io::Cursor::new(bytes.clone())).read() {
        Ok(d) => d,
        Err(e) => {
            eprintln!("emit-acadrust: {}: acadrust could not read this file: {e}", fixture.id);
            return ExitCode::from(1);
        }
    };
    let model1 = match acadsharp_rs_tests::model1::render_stream(
        &acadsharp_rs_tests::acadrust_model1::model1_records(&doc),
    ) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("emit-acadrust: {}: model-1: {e}", fixture.id);
            return ExitCode::from(1);
        }
    };
    let model1_path = acadsharp_rs_tests::model1::output_path(&args.out, &fixture.id, "acadrust");
    if let Err(e) = std::fs::create_dir_all(&args.out).and_then(|()| std::fs::write(&model1_path, model1)) {
        eprintln!("emit-acadrust: {}: {}: {e}", fixture.id, model1_path.display());
        return ExitCode::from(1);
    }
    let sibling = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("acadrust");
    let run = RunInfo::for_run("acadrust", &pinned(), checkout_head(&sibling), fixture);
    match write_emission(&args.out, &run, &emission) {
        Ok(paths) => {
            for p in paths {
                println!("{}", p.display());
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("emit-acadrust: {}: {e}", fixture.id);
            ExitCode::from(1)
        }
    }
}
