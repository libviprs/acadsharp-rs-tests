//! Resident-set sampling shared by the streaming test and its control. They are
//! separate test binaries on purpose: two tests in one process run on parallel
//! threads and measure each other, and whichever ran first would leave the
//! allocator holding pages the second reuses for free.
#![allow(dead_code)]

/// Resident set size in KiB, or `None` where there is no instrument.
pub fn rss_kib() -> Option<u64> {
    #[cfg(target_os = "linux")]
    {
        let statm = std::fs::read_to_string("/proc/self/statm").ok()?;
        let pages: u64 = statm.split_whitespace().nth(1)?.parse().ok()?;
        Some(pages * 4096 / 1024)
    }
    #[cfg(target_os = "macos")]
    {
        let out = std::process::Command::new("ps")
            .args(["-o", "rss=", "-p", &std::process::id().to_string()])
            .output()
            .ok()?;
        String::from_utf8(out.stdout).ok()?.trim().parse().ok()
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        None
    }
}

/// `true` if RSS can be read here. Where it cannot, the test skips, unless
/// `VIPRS_REQUIRE_NATIVE=1`, which makes a missing instrument a failure.
pub fn instrument() -> bool {
    if rss_kib().is_some() {
        return true;
    }
    assert!(
        !std::env::var("VIPRS_REQUIRE_NATIVE").is_ok_and(|v| v == "1"),
        "VIPRS_REQUIRE_NATIVE=1 but this platform has no RSS instrument"
    );
    eprintln!("skipped: no RSS instrument on this platform");
    false
}

pub const ID: &str = "regression_large_entity_count";
pub const LINES: usize = 50_000;
pub const COLUMNS: usize = 250;

/// Line `i` as the generator wrote it.
pub fn line_at(i: usize) -> ([f64; 3], [f64; 3]) {
    let (x, y) = ((i % COLUMNS) as f64, (i / COLUMNS) as f64);
    ([x, y, 0.0], [x + 1.0, y, 0.0])
}

/// How many batches the decode is given to settle before growth is counted.
///
/// Measured on MARS (x86_64 Linux, the viprs.1 archive), with the per-batch series
/// printed by every run: on most runs RSS climbs about 0.7 MiB per batch for the
/// first ten batches, steps up once, and is then flat to the last of the 55
/// batches (about 10 MiB above the post-open sample in all); on a minority of runs
/// the post-open sample already sits on the plateau and there is no ramp at all.
/// The ramp is the runtime touching memory for the first decode, not growth with
/// the number of records, so the claim is stated on what would scale: growth after
/// the warm-up batches. The control, which keeps every record, keeps climbing after
/// the same point.
pub const WARMUP_BATCHES: usize = 12;

#[cfg(native_linked)]
pub struct Drained {
    pub after_open: u64,
    /// RSS in KiB sampled after each batch pull, in order.
    pub series: Vec<u64>,
    pub largest_batch: usize,
    pub lines: usize,
    pub kept: usize,
}

#[cfg(native_linked)]
impl Drained {
    /// RSS once the warm-up batches are in.
    pub fn settled(&self) -> u64 {
        self.series[WARMUP_BATCHES - 1]
    }

    /// The highest RSS sampled after the warm-up.
    pub fn peak_after_warmup(&self) -> u64 {
        *self.series[WARMUP_BATCHES - 1..].iter().max().unwrap()
    }

    /// Four of the largest batches the stream actually pulled, in KiB.
    pub fn budget_kib(&self) -> u64 {
        (4 * self.largest_batch as u64).div_ceil(1024)
    }

    pub fn report(&self) -> String {
        format!(
            "rss after open {} KiB, after {} warm-up batches {} KiB (step {} KiB), final peak {} KiB \
             over {} batches; growth after warm-up {} KiB; largest batch {} bytes, budget {} KiB; \
             kept {} records",
            self.after_open,
            WARMUP_BATCHES,
            self.settled(),
            self.settled().saturating_sub(self.after_open),
            self.peak_after_warmup(),
            self.series.len(),
            self.peak_after_warmup() - self.settled(),
            self.largest_batch,
            self.budget_kib(),
            self.kept
        )
    }
}

/// Decode view 0 of the large drawing, sampling RSS after every batch pull. With
/// `keep`, every item is also held, which is what the control does.
#[cfg(native_linked)]
pub fn drain(keep: bool) -> Drained {
    use acadsharp_rs::{Decoder, Document, Item, Limits, Primitive};
    use acadsharp_rs_tests::fixtures;

    let fx = fixtures::by_id(ID).expect("the fixture row");
    // The path goes to the library, but a drawing that is not the one the row
    // froze would make this measurement about something else.
    fx.load().expect("the committed drawing matches its row");
    let decoder = Decoder::new().expect("handshake");
    let doc = Document::open_path(&decoder, fx.path(), &Limits::default()).expect("open");
    let after_open = rss_kib().unwrap();
    let mut stream = doc.decode(0).expect("decode");

    let mut out = Drained {
        after_open,
        series: Vec::new(),
        largest_batch: 0,
        lines: 0,
        kept: 0,
    };
    let mut held: Vec<Item> = Vec::new();
    let mut last_batch = 0;
    while let Some(item) = stream.next() {
        let item = item.expect("a record");
        if let Item::Primitive(Primitive::Line(l)) = &item {
            let (a, b) = line_at(out.lines);
            assert_eq!((l.start, l.end), (a, b), "line {}", out.lines);
            out.lines += 1;
        }
        if keep {
            held.push(item);
        }
        if stream.batches_pulled() != last_batch {
            last_batch = stream.batches_pulled();
            out.largest_batch = out.largest_batch.max(stream.buffer_len());
            out.series.push(rss_kib().unwrap());
        }
    }
    assert!(stream.is_complete(), "the decode did not reach DocumentEnd");
    assert_eq!(out.lines, LINES, "every line arrived");
    assert!(
        out.series.len() > 2 * WARMUP_BATCHES,
        "only {} batches were sampled; the claim needs well more than the {WARMUP_BATCHES} warm-up ones",
        out.series.len()
    );
    assert!(out.largest_batch > 0, "buffer_len never reported a batch");
    out.kept = held.len();
    out
}
