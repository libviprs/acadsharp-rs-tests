//! What the family files share: opening a fixture through the safe API and
//! comparing doubles under the comparator's tightest class.

use acadsharp_rs::{Decoder, Document, Item, Limits, Primitive};
use acadsharp_rs_tests::compare::BOUNDED_TIGHT;

use crate::native_common::fixture;

/// A fixture opened by path, with the decoder that must outlive it.
pub struct Opened {
    /// Held so the handshake outlives the document.
    #[allow(dead_code)]
    pub decoder: Decoder,
    pub document: Document,
}

pub fn open(id: &str) -> Opened {
    let decoder = Decoder::new().expect("the handshake");
    let document = Document::open_path(&decoder, fixture(id), &Limits::default())
        .unwrap_or_else(|e| panic!("{id} did not open: {e}"));
    Opened { decoder, document }
}

/// Every item of a view, in order, after checking the decode reached its end.
pub fn items(doc: &Document, view: u32) -> Vec<Item> {
    let mut stream = doc.decode(view).expect("decode_begin");
    let mut out = Vec::new();
    for item in &mut stream {
        out.push(item.expect("a record"));
    }
    assert!(
        stream.is_complete(),
        "view {view} did not reach its end totals"
    );
    out
}

/// The primitives of a view, in order.
pub fn primitives(doc: &Document, view: u32) -> Vec<Primitive> {
    items(doc, view)
        .into_iter()
        .filter_map(|i| match i {
            Item::Primitive(p) => Some(p),
            _ => None,
        })
        .collect()
}

/// Two doubles equal under `BOUNDED_TIGHT` (1e-9 absolute, 1e-12 relative), the
/// class the comparator holds the oracle and this crate to.
pub fn near(a: f64, b: f64) -> bool {
    BOUNDED_TIGHT.within(a, b)
}

pub fn near3(a: [f64; 3], b: [f64; 3]) -> bool {
    (0..3).all(|i| near(a[i], b[i]))
}

#[track_caller]
pub fn assert_near(what: &str, got: f64, want: f64) {
    assert!(
        near(got, want),
        "{what}: got {got}, the generator wrote {want}"
    );
}

#[track_caller]
pub fn assert_near3(what: &str, got: [f64; 3], want: [f64; 3]) {
    assert!(
        near3(got, want),
        "{what}: got {got:?}, the generator wrote {want:?}"
    );
}

/// The polyline's bulge at `i`, which the safe API leaves empty when every span is
/// straight.
pub fn bulge(bulges: &[f64], i: usize) -> f64 {
    bulges.get(i).copied().unwrap_or(0.0)
}

/// The unit normal every planar drawing here uses.
pub const Z: [f64; 3] = [0.0, 0.0, 1.0];
