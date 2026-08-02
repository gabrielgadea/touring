#![no_main]
//! Fuzz harness: polyglot AST rewrite.
//!
//! Feeds arbitrary bytes — split on byte boundaries into source / pattern /
//! replacement — into `touring_code::polyglot::rewrite` with `Lang::JavaScript`.
//! Exercises both the ast-grep pattern compiler and the replacement renderer.
//!
//! `rewrite` returns `Err(Error::InvalidPattern)` on malformed patterns — a
//! panic from this target is now a genuine finding, so libfuzzer catches it.

use libfuzzer_sys::fuzz_target;
use touring_code::polyglot::{rewrite, Lang};

fuzz_target!(|data: &[u8]| {
    if data.len() < 3 {
        return;
    }
    // Split on BYTE boundaries into three thirds, then validate UTF-8.
    let third = data.len() / 3;
    let src_bytes = &data[..third];
    let pat_bytes = &data[third..third * 2];
    let rep_bytes = &data[third * 2..];
    let (Ok(source), Ok(pattern), Ok(replacement)) = (
        std::str::from_utf8(src_bytes),
        std::str::from_utf8(pat_bytes),
        std::str::from_utf8(rep_bytes),
    ) else {
        return;
    };
    let _ = rewrite(Lang::JavaScript, source, pattern, replacement);
});
