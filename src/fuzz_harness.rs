//! The functions the fuzz targets, the corpus replay and Miri all call.
//!
//! One body per parser, written once, taking bytes. `fuzz/fuzz_targets/*.rs`
//! hand it libFuzzer's input, `tests/fuzz_corpus_replay.rs` hands it every
//! committed corpus entry on stable, and the Miri job runs that same test, so
//! the three cannot drift apart.
//!
//! None of this touches the native library. Miri does not see foreign code and
//! nothing here claims otherwise: it covers pure-Rust parsers only, and the
//! leak and sanitizer harnesses for the FFI boundary belong to H4.3.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use acadsharp_rs::batch::{BatchReader, Record as WireRecord};
use arbitrary::{Result as ArbResult, Unstructured};

use crate::canonical::{self, Record};
use crate::compare::{ClassTable, Engine, Pair, Stream, WarningRow, compare};

/// What one input may allocate on top of what was live when it started.
pub const ALLOC_CAP: usize = 64 * 1024 * 1024;

/// What one input may take. libFuzzer has its own timeout, this one is the
/// replay's, so a corpus entry that became slow fails on every PR.
pub const TIME_BUDGET: Duration = Duration::from_secs(5);

static LIVE: AtomicUsize = AtomicUsize::new(0);
static BASE: AtomicUsize = AtomicUsize::new(0);

/// A global allocator that counts live bytes and aborts the process when an
/// input has taken more than [`ALLOC_CAP`] beyond the point [`arm`] was called.
///
/// Install it with `#[global_allocator]` in the binary that runs the harness.
/// Without it the counters stay at zero and the cap is not enforced, which is
/// why `tests/alloc_cap.rs` proves it is live.
pub struct CappedAlloc;

fn over(live: usize) -> bool {
    live > BASE.load(Ordering::Relaxed).saturating_add(ALLOC_CAP)
}

fn die() -> ! {
    use std::io::Write;
    let _ = std::io::stderr().write_all(b"fuzz harness: allocation cap exceeded\n");
    std::process::abort()
}

// SAFETY: every call is forwarded to `System` unchanged; the counters are only
// bookkeeping.
unsafe impl GlobalAlloc for CappedAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let live = LIVE.fetch_add(layout.size(), Ordering::Relaxed) + layout.size();
        if over(live) {
            die();
        }
        // SAFETY: same contract as ours.
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        LIVE.fetch_sub(layout.size(), Ordering::Relaxed);
        // SAFETY: same contract as ours.
        unsafe { System.dealloc(ptr, layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let live = LIVE.fetch_add(layout.size(), Ordering::Relaxed) + layout.size();
        if over(live) {
            die();
        }
        // SAFETY: same contract as ours.
        unsafe { System.alloc_zeroed(layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let live = if new_size >= layout.size() {
            LIVE.fetch_add(new_size - layout.size(), Ordering::Relaxed) + (new_size - layout.size())
        } else {
            LIVE.fetch_sub(layout.size() - new_size, Ordering::Relaxed) - (layout.size() - new_size)
        };
        if over(live) {
            die();
        }
        // SAFETY: same contract as ours.
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

/// Live bytes the installed [`CappedAlloc`] has counted. Zero when it is not
/// the global allocator.
pub fn live_bytes() -> usize {
    LIVE.load(Ordering::Relaxed)
}

/// Starts a new input: the cap is measured from here.
pub fn arm() {
    BASE.store(LIVE.load(Ordering::Relaxed), Ordering::Relaxed);
}

/// Armed on creation, and checks the time budget when it goes out of scope.
struct Input(Instant);

impl Input {
    fn begin() -> Self {
        arm();
        Self(Instant::now())
    }
}

impl Drop for Input {
    fn drop(&mut self) {
        // Miri is orders of magnitude slower than the budget assumes.
        if !cfg!(miri) && !std::thread::panicking() {
            let took = self.0.elapsed();
            assert!(
                took <= TIME_BUDGET,
                "input took {took:?}, the per-input budget is {TIME_BUDGET:?}"
            );
        }
    }
}

/// A wire batch into `BatchReader`, walked until the end or the first error.
pub fn batch_reader(data: &[u8]) {
    let _input = Input::begin();
    let Ok(reader) = BatchReader::new(data) else {
        return;
    };
    for record in reader.records() {
        let Ok(record) = record else { return };
        match record {
            WireRecord::Polyline(p) | WireRecord::Polygon(p) => {
                let _ = p.vertices.iter().count() + p.bulges.iter().count();
            }
            WireRecord::Spline(s) => {
                let _ =
                    s.knots.iter().count() + s.controls.iter().count() + s.weights.iter().count();
            }
            WireRecord::Text(t) => {
                let _ = t.text.len();
            }
            WireRecord::Warning(w) => {
                let _ = w.message.len();
            }
            _ => {}
        }
    }
}

/// Arbitrary bytes as a canonical JSONL stream. A stream that parses is
/// rendered and parsed again, and the second parse has to agree.
pub fn canonical_parser(data: &[u8]) {
    let _input = Input::begin();
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };
    let Ok(records) = canonical::parse_stream(text) else {
        return;
    };
    // What the writer renders, the reader has to read back as the same records.
    // A reader that accepts something the writer would never write (negative
    // zero is the example the schema names) fails here.
    let Ok(rendered) = canonical::render_stream(&records) else {
        return;
    };
    let again =
        canonical::parse_stream(&rendered).expect("a stream the writer rendered has to parse");
    assert_eq!(again, records, "parse, render, parse changed the records");
    // `==` calls -0.0 and 0.0 equal, so compare the bits as well.
    let (mut first, mut second) = (records, again);
    for (a, b) in first.iter_mut().zip(second.iter_mut()) {
        let (fa, fb) = (a.floats_mut(), b.floats_mut());
        assert_eq!(fa.len(), fb.len());
        for (x, y) in fa.iter().zip(fb.iter()) {
            assert_eq!(
                x.to_bits(),
                y.to_bits(),
                "a float changed bits on the round trip"
            );
        }
    }
}

/// Arbitrary bytes as `fixtures/manifest.toml`.
pub fn fixture_manifest(data: &[u8]) {
    let _input = Input::begin();
    if let Ok(text) = std::str::from_utf8(data) {
        let _ = crate::fixtures::parse_manifest_text(text);
    }
}

/// Arbitrary bytes as `upstream_parity/manifest.toml`.
pub fn parity_manifest(data: &[u8]) {
    let _input = Input::begin();
    if let Ok(text) = std::str::from_utf8(data) {
        let _ = crate::parity::parse_manifest_text(text);
    }
}

fn arb_f64(u: &mut Unstructured<'_>) -> ArbResult<f64> {
    // Mostly small round numbers, so two streams generated from related bytes
    // line up, with the odd NaN, infinity and negative zero for the edges.
    Ok(match u.int_in_range(0..=15u8)? {
        0 => f64::NAN,
        1 => f64::INFINITY,
        2 => -0.0,
        3 => u.arbitrary()?,
        _ => f64::from(u.int_in_range(-8..=8i8)?) * 0.5,
    })
}

fn arb_vec3(u: &mut Unstructured<'_>) -> ArbResult<[f64; 3]> {
    Ok([arb_f64(u)?, arb_f64(u)?, arb_f64(u)?])
}

fn arb_vec3s(u: &mut Unstructured<'_>) -> ArbResult<Vec<[f64; 3]>> {
    let n = u.int_in_range(0..=5u8)?;
    (0..n).map(|_| arb_vec3(u)).collect()
}

fn arb_f64s(u: &mut Unstructured<'_>) -> ArbResult<Vec<f64>> {
    let n = u.int_in_range(0..=5u8)?;
    (0..n).map(|_| arb_f64(u)).collect()
}

fn arb_string(u: &mut Unstructured<'_>) -> ArbResult<String> {
    let n = u.int_in_range(0..=2u8)?;
    Ok(["", "a", "Model", "ROOM 101"][usize::from(n)].to_string())
}

/// One typed record, built from the bytes `u` hands out.
pub fn arb_record(u: &mut Unstructured<'_>) -> ArbResult<Record> {
    use canonical as c;
    let view = u64::from(u.int_in_range(0..=2u8)?);
    Ok(match u.int_in_range(0..=11u8)? {
        0 => Record::Document(c::Document {
            format: "dwg".into(),
            acad_version: arb_string(u)?,
            maintenance_version: u.int_in_range(0..=300u16)?.into(),
        }),
        1 => Record::View(c::View {
            index: view,
            name: arb_string(u)?,
            kind: arb_string(u)?,
        }),
        2 => Record::Line(c::Line {
            view,
            a: arb_vec3(u)?,
            b: arb_vec3(u)?,
        }),
        3 => {
            let points = arb_vec3s(u)?;
            let bulges = if u.arbitrary()? {
                vec![0.0; points.len()]
            } else {
                arb_f64s(u)?
            };
            Record::Polyline(c::Polyline {
                view,
                closed: u.arbitrary()?,
                normal: arb_vec3(u)?,
                points,
                bulges,
            })
        }
        4 => Record::Circle(c::Circle {
            view,
            center: arb_vec3(u)?,
            radius: arb_f64(u)?,
            normal: arb_vec3(u)?,
        }),
        5 => Record::Arc(c::Arc {
            view,
            center: arb_vec3(u)?,
            radius: arb_f64(u)?,
            start: arb_f64(u)?,
            end: arb_f64(u)?,
            normal: arb_vec3(u)?,
        }),
        6 => Record::Ellipse(c::Ellipse {
            view,
            center: arb_vec3(u)?,
            major: arb_vec3(u)?,
            ratio: arb_f64(u)?,
            start: arb_f64(u)?,
            end: arb_f64(u)?,
            normal: arb_vec3(u)?,
        }),
        7 => Record::Spline(c::Spline {
            view,
            degree: u.int_in_range(0..=5u8)?.into(),
            closed: u.arbitrary()?,
            periodic: u.arbitrary()?,
            control_points: arb_vec3s(u)?,
            knots: arb_f64s(u)?,
            weights: arb_f64s(u)?,
            fit_points: arb_vec3s(u)?,
        }),
        8 => Record::Polygon(c::Polygon {
            view,
            points: arb_vec3s(u)?,
        }),
        9 => Record::Point(c::Point {
            view,
            at: arb_vec3(u)?,
        }),
        10 => Record::Text(c::Text {
            view,
            at: arb_vec3(u)?,
            height: arb_f64(u)?,
            rotation: arb_f64(u)?,
            width_factor: arb_f64(u)?,
            halign: arb_string(u)?,
            valign: arb_string(u)?,
            style: arb_string(u)?,
            text: arb_string(u)?,
        }),
        _ => Record::Warning(c::Warning {
            category: arb_string(u)?,
            entity_type: arb_string(u)?,
            detail: arb_string(u)?,
        }),
    })
}

fn arb_records(u: &mut Unstructured<'_>) -> ArbResult<Vec<Record>> {
    let n = u.int_in_range(0..=6u8)?;
    (0..n).map(|_| arb_record(u)).collect()
}

/// Two arbitrary record sequences through the comparator, which must neither
/// panic nor change its equal-or-not verdict when the sides are swapped.
pub fn comparator(data: &[u8]) {
    let _input = Input::begin();
    let mut u = Unstructured::new(data);
    let (Ok(a), Ok(b)) = (arb_records(&mut u), arb_records(&mut u)) else {
        return;
    };
    let (a, b) = (Stream::new(a), Stream::new(b));
    let pair = Pair::new(Engine::Oracle, Engine::Oracle);
    let table = ClassTable::for_pair(pair, WarningRow::Compare);
    let ab = compare("fuzz", &a, &b, &table).expect("the default table validates");
    let ba = compare("fuzz", &b, &a, &table).expect("the default table validates");
    assert_eq!(
        ab.is_match(),
        ba.is_match(),
        "compare(a, b) and compare(b, a) disagree on equal-or-not\n a = {:?}\n b = {:?}",
        a.records,
        b.records
    );
}

/// A fuzz target: its name, which is also its corpus directory, and its body.
pub type Target = (&'static str, fn(&[u8]));

/// Every target, by name. `tests/fuzz_corpus_replay.rs` walks this.
pub const TARGETS: &[Target] = &[
    ("batch_reader", batch_reader),
    ("canonical_parser", canonical_parser),
    ("fixture_manifest", fixture_manifest),
    ("parity_manifest", parity_manifest),
    ("comparator", comparator),
];
