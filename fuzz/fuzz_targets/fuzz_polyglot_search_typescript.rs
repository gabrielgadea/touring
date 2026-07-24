#![no_main]
//! Fuzz harness: polyglot AST search over TypeScript source.
//!
//! Feeds arbitrary bytes — split on a byte boundary into source + pattern —
//! into `touring_ast_polyglot::search` with `Lang::TypeScript`.
//!
//! `search` returns `Err(Error::InvalidPattern)` on malformed patterns — a
//! panic from this target is now a genuine finding, so libfuzzer catches it.

use libfuzzer_sys::fuzz_target;
use touring_ast_polyglot::{search, Lang};

fuzz_target!(|data: &[u8]| {
    if data.len() < 2 {
        return;
    }
    // Split on a BYTE boundary (never on a &str) so the harness itself never
    // panics on a multi-byte char split.
    let mid = data.len() / 2;
    let (src_bytes, pat_bytes) = data.split_at(mid);
    let (Ok(source), Ok(pattern)) = (
        std::str::from_utf8(src_bytes),
        std::str::from_utf8(pat_bytes),
    ) else {
        return;
    };
    let _ = search(Lang::TypeScript, source, pattern);
});
