//! tree-sitter-md 0.5.3's block scanner serializes 5 bytes plus 4 per open block
//! into tree-sitter's 1024-byte buffer with no bound, so a document with 255 or
//! more open blocks writes past the buffer into the parser it lives in. The
//! corrupted, thread-reused parser then crashed the daemon on a LATER file:
//! SIGSEGV inside tree-sitter during `index rebuild` (13 and 14/09/2026), found
//! by a heap-checked in-process rebuild of the analise tree and pinned to this
//! threshold (250 nested quotes pass, 260 abort). The fix is the open-block cap of
//! 254 in `push_block` (`third_party/tree-sitter-md`); the bounded `serialize` is
//! defense in depth — alone it still crashed 3 of 3 runs (`PATCHES.md`).
//!
//! The parse runs in a CHILD process: the overflow is undefined behaviour, and in
//! a debug build tree-sitter's own assertion aborts the process that hosts it.

use std::process::Command;

const CHILD: &str = "TOURING_MD_OVERFLOW_CHILD";

#[test]
fn overflow_child() {
    if std::env::var(CHILD).is_err() {
        return;
    }
    use touring_code::ast::languages::Lang;
    use touring_code::ast::symbols::extract_symbols;
    let normal = "# Normal\n\nSome text.\n\n- a\n- b\n";
    let mut deep_documents: Vec<String> = [260usize, 400, 1000, 5000]
        .iter()
        .map(|&depth| format!("# Deep\n\n{} texto\n\nfim\n", ">".repeat(depth)))
        .collect();
    // Nesting that grows line by line (list items by indentation, quotes and
    // lists interleaved): the scanner's uint8_t counters wrap past 255 even when
    // the serialized state is bounded.
    deep_documents.push(
        (0..300)
            .map(|i| format!("{}- item {i}\n", "  ".repeat(i)))
            .collect::<String>(),
    );
    deep_documents.push(
        (1..300)
            .map(|i| format!("{} texto {i}\n", "> -".repeat(i)))
            .collect::<String>(),
    );
    for deep in deep_documents {
        // Several rounds on the thread-local parser: the corruption surfaced on
        // a later parse, never on the deep document itself.
        for _ in 0..5 {
            let _ = extract_symbols(&deep, Lang::Markdown);
            let symbols = extract_symbols(normal, Lang::Markdown).expect("normal markdown parses");
            assert!(
                !symbols.is_empty(),
                "the reused parser still extracts headings"
            );
        }
    }
}

#[test]
fn deeply_nested_markdown_never_corrupts_the_parser() {
    let out = Command::new(std::env::current_exe().expect("test binary"))
        .args([
            "--exact",
            "overflow_child",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(CHILD, "1")
        .output()
        .expect("spawn child");
    assert!(
        out.status.success(),
        "parsing markdown with >= 255 open blocks killed the process ({:?}); stderr tail: {}",
        out.status,
        String::from_utf8_lossy(&out.stderr)
            .lines()
            .rev()
            .take(6)
            .collect::<Vec<_>>()
            .join(" | ")
    );
}

const UNICODE_CHILD: &str = "TOURING_MD_CTYPE_CHILD";

/// The second defect in the same scanner: `parse_ordered_list_marker` classified
/// the character after a line's leading digits with `isdigit`, which C defines
/// only for `unsigned char` values. tree-sitter hands it a Unicode codepoint, and
/// glibc indexes its ctype table with it: `1` followed by U+FF495 in an analise
/// document read about 2 MB past the table (AddressSanitizer sweep of the analise
/// markdown, 14/09/2026; the block cap did not cover it). Whether one fixed
/// codepoint faults depends on the memory layout — the first version of this child
/// used six and passed with or without the fix — so it sweeps the codepoint space in
/// steps of 0x100: every page past the ctype table is touched, and the upstream
/// scanner faults in any layout (measured on the bash twin of this defect, 14/09).
/// The source guard [`the_vendored_scanners_never_call_ctype_on_a_codepoint`] is the
/// structural proof; the AddressSanitizer run in `PATCHES.md` is the memory proof.
#[test]
fn unicode_after_list_digits_child() {
    if std::env::var(UNICODE_CHILD).is_err() {
        return;
    }
    use touring_code::ast::languages::Lang;
    use touring_code::ast::symbols::extract_symbols;
    for c in (0x100u32..=0x10_FFFF)
        .step_by(0x100)
        .filter_map(char::from_u32)
    {
        let document = format!("# Seção\n\n1{c} texto\n\n12{c}. item\n\n   7{c}) item\n");
        let _ = extract_symbols(&document, Lang::Markdown);
    }
    let symbols = extract_symbols("# Seção\n\n1. item\n", Lang::Markdown).expect("markdown parses");
    assert!(
        !symbols.is_empty(),
        "the headings are still extracted after the sweep"
    );
}

#[test]
fn a_codepoint_after_list_digits_never_reads_outside_the_ctype_table() {
    let out = Command::new(std::env::current_exe().expect("test binary"))
        .args([
            "--exact",
            "unicode_after_list_digits_child",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(UNICODE_CHILD, "1")
        .output()
        .expect("spawn child");
    assert!(
        out.status.success(),
        "a codepoint after a line's leading digits killed the process ({:?}); stderr tail: {}",
        out.status,
        String::from_utf8_lossy(&out.stderr)
            .lines()
            .rev()
            .take(6)
            .collect::<Vec<_>>()
            .join(" | ")
    );
}

/// The `<ctype.h>` classifiers, each undefined for a value outside `unsigned char`.
const CTYPE_CLASSIFIERS: [&str; 14] = [
    "isdigit", "isalpha", "isalnum", "isspace", "ispunct", "isupper", "islower", "isxdigit",
    "isprint", "iscntrl", "isgraph", "isblank", "tolower", "toupper",
];

/// `source` with `//` and `/* */` comments blanked and string/char literal contents
/// blanked, line structure kept — a classifier name inside a comment or a literal is
/// not a call, and a `//` inside a literal does not end the line's code.
fn code_only(source: &str) -> String {
    let bytes = source.as_bytes();
    let mut out = String::with_capacity(source.len());
    let mut i = 0;
    while i < bytes.len() {
        let rest = &bytes[i..];
        if rest.starts_with(b"//") {
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
        } else if rest.starts_with(b"/*") {
            i += 2;
            while i < bytes.len() && !bytes[i..].starts_with(b"*/") {
                out.push(if bytes[i] == b'\n' { '\n' } else { ' ' });
                i += 1;
            }
            i = (i + 2).min(bytes.len());
        } else if bytes[i] == b'"' || bytes[i] == b'\'' {
            let quote = bytes[i];
            out.push(' ');
            i += 1;
            while i < bytes.len() && bytes[i] != quote && bytes[i] != b'\n' {
                if bytes[i] == b'\\' {
                    i += 1;
                }
                i += 1;
            }
            i += 1;
        } else {
            let ch = source[i..].chars().next().expect("char boundary");
            out.push(ch);
            i += ch.len_utf8();
        }
    }
    out
}

/// Every use of a ctype classifier in C `source` that can reach a call: a direct
/// call (`isdigit(`), a parenthesized call (`(isdigit)(`), or an alias (`#define X
/// isdigit` — every later `X(` is the same call).
fn ctype_uses(source: &str) -> Vec<(usize, String)> {
    let code = code_only(source);
    let word = |line: &str, at: usize, len: usize| {
        let before = line[..at].chars().next_back();
        let after = line[at + len..].chars().next();
        !before.is_some_and(|c| c.is_ascii_alphanumeric() || c == '_')
            && !after.is_some_and(|c| c.is_ascii_alphanumeric() || c == '_')
    };
    let mut uses = Vec::new();
    for (n, line) in code.lines().enumerate() {
        for name in CTYPE_CLASSIFIERS {
            let mut from = 0;
            while let Some(found) = line[from..].find(name) {
                let at = from + found;
                if word(line, at, name.len()) {
                    let after = line[at + name.len()..].trim_start();
                    let parenthesized =
                        line[..at].trim_end().ends_with('(') && after.starts_with(')');
                    let aliased = line.trim_start().starts_with("#define");
                    if after.starts_with('(') || parenthesized || aliased {
                        uses.push((
                            n + 1,
                            source.lines().nth(n).unwrap_or("").trim().to_string(),
                        ));
                    }
                }
                from = at + name.len();
            }
        }
    }
    uses
}

/// Every `<ctype.h>` classifier is undefined for a value outside `unsigned char`,
/// and the scanners feed them `lexer->lookahead`, a codepoint. No vendored scanner
/// may call one — markdown (block and inline) and bash, the two grammars whose
/// upstream releases did (cross-audit 14/09/2026, D1 and the markdown patch). A
/// future upstream release dropped in over a patch is caught here instead of by a
/// daemon crash.
#[test]
fn the_vendored_scanners_never_call_ctype_on_a_codepoint() {
    let third_party = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../third_party");
    let scanners = [
        "tree-sitter-md/tree-sitter-markdown/src/scanner.c",
        "tree-sitter-md/tree-sitter-markdown-inline/src/scanner.c",
        "tree-sitter-bash/src/scanner.c",
    ];
    let mut offences = Vec::new();
    for rel in scanners {
        let path = third_party.join(rel);
        let source =
            std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        for (line, text) in ctype_uses(&source) {
            offences.push(format!("{rel}:{line}: {text}"));
        }
    }
    assert!(
        offences.is_empty(),
        "ctype classifier called in a vendored scanner:\n{}",
        offences.join("\n")
    );
}

/// The guard reads C the way a compiler would: calls hidden behind a comment-like
/// literal, a parenthesized name or an alias are found; names in comments and
/// literals, and longer identifiers, are not.
#[test]
fn the_ctype_guard_sees_through_comments_literals_parentheses_and_aliases() {
    let found = |src: &str| ctype_uses(src).len();
    assert_eq!(found("while (isdigit(lexer->lookahead)) {}"), 1);
    assert_eq!(found("x = (isdigit)(c);"), 1, "parenthesized call");
    assert_eq!(
        found("#define DIGIT isdigit\nDIGIT(c);"),
        1,
        "alias definition"
    );
    assert_eq!(
        found("f(\"a // b\"); isalpha(c);"),
        1,
        "`//` inside a literal"
    );
    assert_eq!(found("// isdigit(c)\n/* toupper(c) */"), 0, "comments");
    assert_eq!(found("puts(\"isdigit(c)\");"), 0, "literal");
    assert_eq!(
        found("touring_is_ascii_digit(c); my_isdigit(c);"),
        0,
        "longer identifiers"
    );
}

/// The patched copies are what compiles: `[patch.crates-io]` resolves them as path
/// dependencies, so `Cargo.lock` carries no registry `source` for either grammar.
/// Dropping the patch table leaves the source guard green over files nobody builds;
/// this is the check that notices.
#[test]
fn the_patched_grammars_are_the_ones_the_workspace_builds() {
    let lock = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Cargo.lock"),
    )
    .expect("workspace Cargo.lock");
    for grammar in ["tree-sitter-md", "tree-sitter-bash"] {
        let entries: Vec<&str> = lock
            .split("[[package]]")
            .filter(|entry| entry.contains(&format!("name = \"{grammar}\"\n")))
            .collect();
        assert_eq!(entries.len(), 1, "one {grammar} in the lock");
        assert!(
            !entries[0].contains("source = "),
            "{grammar} resolves from a registry, not from third_party/: {}",
            entries[0].trim()
        );
    }
}
