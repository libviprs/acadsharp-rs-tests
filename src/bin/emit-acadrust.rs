//! `emit-acadrust --fixture <id> --out <dir>`: schema 1 from acadrust.
//!
//! Writes `<id>.acadrust.jsonl`, `<id>.acadrust.src.jsonl` (the sidecar) and
//! `<id>.acadrust.run.json` (the fork commit, the thread count, the target and
//! the fixture's sha). This binary is the acadrust adapter and nothing else:
//! the harness is `acadsharp_rs_tests::emit` and the writer is
//! `acadsharp_rs_tests::canonical`, which `emit-acadsharp` goes through too.
//! It also writes `<id>.acadrust.model1.jsonl`, the model-1 stream
//! (docs/MODEL_1.md).
//!
//! Exit codes: 0 written, 1 the engine could not read or the records could not
//! be encoded, 2 called wrong or the fixture is not in the manifest.

use std::path::Path;
use std::process::ExitCode;

use acadsharp_rs_tests::acadrust_emit::{extract_bytes, extract_dxf_bytes};
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
    let sibling = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("acadrust");
    // A `[[fixture]]` is a DWG; an `[[oracle_input]]` is one of AutoCAD's DXF
    // twins. The id says which, and the manifest is the only list of either.
    let (id, sha256, bytes, is_dxf, extracted) = if let Some(fixture) = fixtures::by_id(&args.fixture) {
        let bytes = match fixture.load() {
            Ok(b) => b,
            Err(e) => {
                eprintln!("emit-acadrust: {e}");
                return ExitCode::from(1);
            }
        };
        let extracted = extract_bytes(&bytes);
        (
            fixture.id.clone(),
            fixture.sha256.clone(),
            bytes,
            false,
            extracted,
        )
    } else if let Some(input) = fixtures::oracle_inputs()
        .iter()
        .find(|i| i.id == args.fixture)
    {
        use acadsharp_rs_tests::fixtures::ManifestFile;
        let bytes = match input.load() {
            Ok(b) => b,
            Err(e) => {
                eprintln!("emit-acadrust: {e}");
                return ExitCode::from(1);
            }
        };
        let extracted = extract_dxf_bytes(&bytes);
        (
            input.id.clone(),
            input.sha256.clone(),
            bytes,
            true,
            extracted,
        )
    } else {
        eprintln!(
            "emit-acadrust: no fixture or DXF input {:?} in fixtures/manifest.toml; the manifest is the only list",
            args.fixture
        );
        return ExitCode::from(2);
    };
    let emission = match extracted {
        Ok(e) => e,
        Err(e) => {
            eprintln!("emit-acadrust: {id}: {e}");
            return ExitCode::from(1);
        }
    };
    // model-1 (docs/MODEL_1.md): the header variables, in a file beside the others.
    let model1 = match acadsharp_rs_tests::acadrust_model1::render(&bytes, is_dxf) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("emit-acadrust: {id}: model-1: {e}");
            return ExitCode::from(1);
        }
    };
    let model1_path = acadsharp_rs_tests::model1::output_path(&args.out, &id, "acadrust");
    if let Err(e) =
        std::fs::create_dir_all(&args.out).and_then(|()| std::fs::write(&model1_path, model1))
    {
        eprintln!("emit-acadrust: {id}: {}: {e}", model1_path.display());
        return ExitCode::from(1);
    }
    let run = RunInfo::for_input("acadrust", &pinned(), checkout_head(&sibling), &id, &sha256);
    match write_emission(&args.out, &run, &emission) {
        Ok(paths) => {
            for p in paths {
                println!("{}", p.display());
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("emit-acadrust: {id}: {e}");
            ExitCode::from(1)
        }
    }
}
