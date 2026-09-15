//! tree-sitter-bash 0.25.1's external scanner classifies `lexer->lookahead` — a
//! Unicode codepoint — with `isdigit` while scanning a brace expression
//! (`scanner.c`, two `while (isdigit(lexer->lookahead))` loops). C defines the
//! `<ctype.h>` classifiers only for `unsigned char` values, and glibc indexes its
//! ctype table with the codepoint: a `.sh` file holding `{` followed by a
//! high-plane character reads outside the table. Cross-audit 14/09/2026 (D1): a C
//! harness over the registry grammar died with SIGSEGV 3 of 3 runs, and the same
//! build with the codepoints replaced by ASCII digits exited 0. The index parses
//! every `.sh` it walks with this grammar, so the fault took the daemon down.
//!
//! The parse runs in a CHILD process — the fault is a signal, not a panic — and
//! sweeps the codepoint space in steps of 0x100 (about 2.000 table entries per
//! page, so every page past the table is touched). A fixed handful of codepoints
//! passes or fails with the memory layout; the sweep does not.

use std::process::Command;

const CHILD: &str = "TOURING_BASH_CTYPE_CHILD";

/// Every non-surrogate codepoint above Latin-1, one per 0x100.
fn swept_codepoints() -> impl Iterator<Item = char> {
    (0x100u32..=0x10_FFFF)
        .step_by(0x100)
        .filter_map(char::from_u32)
}

#[test]
fn bash_codepoint_sweep_child() {
    if std::env::var(CHILD).is_err() {
        return;
    }
    use touring_code::ast::languages::Lang;
    use touring_code::ast::symbols::extract_symbols;
    let mut parsed = 0usize;
    for c in swept_codepoints() {
        // The three shapes that reach both `isdigit` loops of the brace scanner.
        let document =
            format!("f() {{ echo {{{c}..2}}; }}\necho a{{{c}}}\nx={{{c}\necho {{1..{c}}}\n");
        let _ = extract_symbols(&document, Lang::Bash);
        parsed += 1;
    }
    let symbols =
        extract_symbols("probe() { echo ok; }\n", Lang::Bash).expect("ordinary bash parses");
    assert!(
        !symbols.is_empty(),
        "the parser still extracts functions after {parsed} swept documents"
    );
}

#[test]
fn a_codepoint_in_a_bash_brace_expression_never_reads_outside_the_ctype_table() {
    let out = Command::new(std::env::current_exe().expect("test binary"))
        .args([
            "--exact",
            "bash_codepoint_sweep_child",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(CHILD, "1")
        .output()
        .expect("spawn child");
    assert!(
        out.status.success(),
        "parsing bash with a codepoint after `{{` killed the process ({:?}); stderr tail: {}",
        out.status,
        String::from_utf8_lossy(&out.stderr)
            .lines()
            .rev()
            .take(6)
            .collect::<Vec<_>>()
            .join(" | ")
    );
}
