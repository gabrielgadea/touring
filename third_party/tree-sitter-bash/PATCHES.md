# tree-sitter-bash 0.25.1, patched by touring

Copy of the crates.io release `tree-sitter-bash 0.25.1`
(tree-sitter/tree-sitter-bash), wired through `[patch.crates-io]` in the workspace
`Cargo.toml` and excluded from the workspace. One change, in `src/scanner.c`:

1. **Brace-expression digits are ASCII** (`touring_is_ascii_digit`, 14/09/2026). The
   external scanner walks `{1..2}` with `while (isdigit(lexer->lookahead))` twice.
   `lexer->lookahead` is a Unicode codepoint; C defines `isdigit` only for `unsigned
   char` values, and glibc indexes its ctype table with the argument. A `.sh` file
   holding `{` followed by a high-plane character reads outside the table.

Why: the index parses every `.sh`/`.bash`/`.zsh` it walks with this grammar
(`Lang::Bash`), so one such character in a repository took the daemon down with
SIGSEGV during `index rebuild` — the same class as the markdown scanner defect of
the same day.

How it was found: the markdown patch's census recorded this call as undefined
behaviour without a proven crash (six crafted inputs under AddressSanitizer did not
fault — ASan changes the memory layout). The cross-audit of 14/09/2026 (critic D)
built a C harness over the registry grammar and runtime 0.26.9 without
AddressSanitizer: SIGSEGV 3 of 3 runs (`gdb`: `scan () at scanner.c:1158`), and the
same build with the codepoints replaced by ASCII digits exited 0. Measured against
the live daemon's `/proc/<pid>/maps`, 20.480 codepoints map to unmapped pages.

Proof: `crates/touring-code/tests/bash_scanner_ctype.rs` sweeps the codepoint space in
a child process (red before this patch: exit 139; green after), and
`the_vendored_scanners_never_call_ctype_on_a_codepoint` in
`crates/touring-code/tests/markdown_scanner_overflow.rs` rejects any `<ctype.h>`
classifier in a vendored scanner. `the_patched_grammars_are_the_ones_the_workspace_builds`
fails if `Cargo.lock` resolves the grammar from a registry again.

Drop this copy when an upstream release classifies the lookahead with an explicit
ASCII range (or `iswdigit`); re-run the sweep test before dropping it.
