#![no_main]
//! Fuzz harness: Rust public API surface extraction.
//!
//! Feeds arbitrary UTF-8 into `RustSemanticReport::public_api_surface`, which
//! parses Rust source with `syn` and stringifies every `pub` item. The target
//! proves the extractor never panics — all parse failures yield `Err`.

use libfuzzer_sys::fuzz_target;
use touring_code::ast::rust_semantic::RustSemanticReport;

fuzz_target!(|data: &[u8]| {
    if let Ok(s) = std::str::from_utf8(data) {
        let _ = RustSemanticReport::public_api_surface(s);
    }
});
