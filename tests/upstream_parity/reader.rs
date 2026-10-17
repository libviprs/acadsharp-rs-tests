//! Mirrors `IO/DWG/DwgReaderTests.cs`, `IO/CadReaderTestsBase.cs` and
//! `IO/IOTestsBase.cs`: every corpus drawing opens, reports the version its name
//! says, lists the model view first and then layouts with names, a reader
//! notification surfaces as a warning, and a truncated file is refused as corrupt.

use acadsharp_rs::{Decoder, Document, Error, Item, Limits, ViewKind};
use acadsharp_rs_tests::fixtures;

use crate::common::*;
use crate::native_common::{fixture, truncated};

/// Tier C drawings that open and are then refused by the default limits at decode
/// time. `g13_dimension_deep` nests dimensions past the depth bound on purpose.
const STRESS_REFUSED: &[&str] = &["g13_dimension_deep"];

fn version_number(acad_version: &str) -> u32 {
    acad_version
        .strip_prefix("AC")
        .and_then(|n| n.parse().ok())
        .unwrap_or_else(|| panic!("{acad_version:?} is not an AC10xx name"))
}

#[test]
fn every_corpus_drawing_opens_and_reports_the_version_its_row_says() {
    let decoder = Decoder::new().expect("the handshake");
    let mut seen = 0;
    for f in fixtures::all() {
        let doc = Document::open_path(&decoder, fixture(&f.id), &Limits::default())
            .unwrap_or_else(|e| panic!("{} did not open: {e}", f.id));
        // The version is on the document-begin record of the first decode.
        let mut stream = doc
            .decode(0)
            .unwrap_or_else(|e| panic!("{}: decode: {e}", f.id));
        let first = stream
            .next()
            .unwrap_or_else(|| panic!("{}: an empty stream", f.id));
        if STRESS_REFUSED.contains(&f.id.as_str()) {
            // A stress drawing that opens and then hits a default limit on the
            // first pull. The refusal is the contract (`Limits` is the defence
            // against it), so it is asserted rather than skipped: if the bound
            // ever stops biting, this fails and the exemption is removed.
            assert!(
                matches!(first, Err(Error::LimitExceeded)),
                "{}: expected the default limits to refuse the decode, got {first:?}",
                f.id
            );
            seen += 1;
            continue;
        }
        assert!(
            matches!(first, Ok(Item::DocumentBegin(_))),
            "{}: {first:?}",
            f.id
        );
        let begin = stream
            .document_begin()
            .unwrap_or_else(|| panic!("{}: the stream has not yet shown its DocumentBegin", f.id));
        assert_eq!(
            begin.drawing_version,
            version_number(&f.acad_version),
            "{}: the row says {}",
            f.id,
            f.acad_version
        );
        seen += 1;
    }
    assert_eq!(seen, fixtures::all().len());
    assert!(
        seen >= 80,
        "only {seen} drawings: the corpus shrank or the loop is wrong"
    );
}

#[test]
fn the_model_view_is_index_zero_and_layouts_follow_with_names() {
    let decoder = Decoder::new().expect("the handshake");
    for f in fixtures::all() {
        let doc = Document::open_path(&decoder, fixture(&f.id), &Limits::default()).unwrap();
        let views = doc
            .views()
            .unwrap_or_else(|e| panic!("{}: views: {e}", f.id));
        assert!(!views.is_empty(), "{}: no views", f.id);
        assert_eq!(views[0].index(), 0, "{}", f.id);
        assert_eq!(views[0].kind(), ViewKind::Model, "{}: view 0", f.id);
        for (i, v) in views.iter().enumerate() {
            assert_eq!(v.index() as usize, i, "{}: indices are gapless", f.id);
            assert!(!v.name().is_empty(), "{}: view {i} has no name", f.id);
            if i > 0 {
                assert_eq!(
                    v.kind(),
                    ViewKind::Layout,
                    "{}: view {i} {:?}",
                    f.id,
                    v.name()
                );
            }
        }
    }
}

#[test]
fn a_reader_notification_surfaces_as_a_warning() {
    // `regression_text_zero_height` holds two entities ACadSharp's reader cannot
    // read back; the reader's own notification is what the stream must carry.
    let o = open("regression_text_zero_height");
    let warnings: Vec<_> = items(&o.document, 0)
        .into_iter()
        .filter_map(|i| {
            if let Item::Warning(w) = i {
                Some(w)
            } else {
                None
            }
        })
        .collect();
    assert!(
        !warnings.is_empty(),
        "no warning at all for a drawing the reader choked on"
    );
    eprintln!(
        "warnings: {:?}",
        warnings
            .iter()
            .map(|w| (w.code.get(), &w.message))
            .collect::<Vec<_>>()
    );
}

#[test]
fn a_truncated_file_is_refused_as_corrupt_and_the_next_open_still_works() {
    let decoder = Decoder::new().expect("the handshake");
    let path = truncated("acadsharp-ac1032", 1, 2);
    let err = match Document::open_path(&decoder, &path, &Limits::default()) {
        Ok(_) => panic!("half a drawing opened"),
        Err(e) => e,
    };
    assert!(matches!(err, Error::CorruptInput), "got {err:?}");
    // Handle accounting for this path is `resource_semantics.rs`'s (#11); what is
    // checked here is that the failure left the decoder usable.
    Document::open_path(&decoder, fixture("acadsharp-ac1032"), &Limits::default())
        .expect("a good file opens after a bad one");
}
