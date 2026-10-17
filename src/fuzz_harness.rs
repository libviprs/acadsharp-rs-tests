//! Stub: the harness lands in the next commit. The tests that need it are red.
use std::alloc::{GlobalAlloc, Layout, System};

pub struct CappedAlloc;

// SAFETY: forwards to `System`.
unsafe impl GlobalAlloc for CappedAlloc {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        unsafe { System.alloc(l) }
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        unsafe { System.dealloc(p, l) }
    }
}

pub fn arm() {}

pub type Target = (&'static str, fn(&[u8]));
pub const TARGETS: &[Target] = &[];
