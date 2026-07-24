#![no_main]
//! Fuzz harness: Rust semantic parsing via `syn`.
//!
//! Feeds arbitrary UTF-8 into `RustSemanticReport::from_source`, which parses
//! Rust source with `syn` and walks the AST. The target proves the parser
//! never panics on malformed input — all errors must surface as `Err`.

use libfuzzer_sys::fuzz_target;
use touring_code::ast::rust_semantic::RustSemanticReport;

fuzz_target!(|data: &[u8]| {
    if let Ok(s) = std::str::from_utf8(data) {
        // Errors are the expected outcome for malformed Rust — only a panic
        // (unwrap/index-out-of-bounds/overflow) constitutes a fuzz finding.
        let _ = RustSemanticReport::from_source(s);
    }
});
