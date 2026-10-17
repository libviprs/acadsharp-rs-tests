//! The shared link for the suite's own binaries.
//!
//! `acadsharp-rs` finds `libacadsharp_native.so` through an rpath it emits as
//! `cargo::rustc-link-arg`, and cargo binds that to the emitting package's own
//! binaries, tests and examples and no further. So `emit-acadsharp`, which is
//! this package's binary, would link and then die before `main` with exit 127
//! and `error while loading shared libraries`. The crate's documentation gives
//! the answer: a build script in the package that produces the binary, reading
//! the metadata the crate's `links = "acadsharp_native"` hands downstream.
//!
//! Those variables exist only when an archive resolved. Without one this does
//! nothing, the suite still builds, and `Decoder::new()` answers
//! `Error::Unlinked` at run time, which `emit-acadsharp` turns into a non-zero
//! exit. Under `ACADSHARP_REQUIRE_NATIVE=1` the crate's own build script stops
//! the build before this one runs, so there is no build of that binary that can
//! print nothing and exit 0.
//!
//! Static linking needs no rpath and nothing here: build with
//! `--features acadsharp-rs/link-static` on a target whose `LINKINFO.json` says
//! `static_certified: true`.

fn main() {
    println!("cargo::rerun-if-changed=build.rs");
    for var in [
        "DEP_ACADSHARP_NATIVE_LIB_DIR",
        "DEP_ACADSHARP_NATIVE_NATIVE_DIR",
        "DEP_ACADSHARP_NATIVE_ARTIFACT_VERSION",
    ] {
        println!("cargo::rerun-if-env-changed={var}");
    }
    if let Ok(dir) = std::env::var("DEP_ACADSHARP_NATIVE_LIB_DIR") {
        println!("cargo::rustc-link-arg=-Wl,-rpath,{dir}");
    }
    // The archive that was linked, for `run.json`: what the binary was built
    // against, which is not necessarily what `ACADSHARP_NATIVE_DIR` says when
    // it runs.
    if let Ok(root) = std::env::var("DEP_ACADSHARP_NATIVE_NATIVE_DIR") {
        println!("cargo::rustc-env=ACADSHARP_LINKED_NATIVE_DIR={root}");
    }
}
