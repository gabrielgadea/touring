//! A parse that never completes is halted (cross-audit R2-25, 15/09/2026).
//!
//! `proptest_parser_fuzz::extract_symbols_typescript_never_panics` hung on
//! `PROPTEST_RNG_SEED=2` and crashed the full workspace run. Delta debugging cut
//! the input to the 86 bytes below; under gdb the thread sat in
//! `ts_parser_parse` → `ts_language_next_state` (TypeScript grammar 0.23.2,
//! tree-sitter 0.26.9) with no end. The input lives here as a Rust literal, not
//! as a `.ts` fixture: a daemon without the budget would hang indexing it.

use std::sync::mpsc;
use std::time::{Duration, Instant};

use touring_code::ast::{Lang, extract_symbols};

const HANGS_TYPESCRIPT: &str = "[``2n?%\nYm\t$??32{\t\n\n\"\t\n,\nU<.\n\n\t/>.\n\n\n/Y-IQ^w='$|X%\t?'<<\nq\trq:u\"Y{MF[+/x?,#\"=&tp\t9-:Y\t\\";

#[test]
fn a_parse_that_never_completes_is_halted_and_the_parser_recovers() {
    assert_eq!(HANGS_TYPESCRIPT.len(), 86, "the literal is the minimized input");
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let started = Instant::now();
        let halted = extract_symbols(HANGS_TYPESCRIPT, Lang::TypeScript);
        let elapsed = started.elapsed();
        // Same thread, same thread-local parser: a halted parser that kept its
        // state would resume the old document instead of parsing this one.
        let next = extract_symbols("export function greet(name: string) { return name; }", Lang::TypeScript);
        let _ = tx.send((halted.is_err(), elapsed, next));
    });
    let (halted_is_err, elapsed, next) = rx
        .recv_timeout(Duration::from_secs(60))
        .expect("the parse still never returns: no budget cut it");
    assert!(halted_is_err, "a halted parse is an error, never an empty tree read as a file with no symbols");
    assert!(elapsed < Duration::from_secs(10), "cut near the 2 s budget, took {elapsed:?}");
    let symbols = next.expect("the next document parses");
    assert!(
        symbols.iter().any(|s| s.name == "greet"),
        "the parser was reset and reads the next document: {symbols:?}"
    );
}
