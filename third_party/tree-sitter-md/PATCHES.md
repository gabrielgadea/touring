# tree-sitter-md 0.5.3, patched by touring

Copy of the crates.io release `tree-sitter-md 0.5.3` (tree-sitter-grammars/tree-sitter-markdown),
wired through `[patch.crates-io]` in the workspace `Cargo.toml` and excluded from the
workspace. Three changes, all in `tree-sitter-markdown/src/scanner.c`:

1. **Open blocks are capped at 254** (`TOURING_MAX_OPEN_BLOCKS` in `push_block`), and
   `pop_block` never underflows. The block scanner counts open blocks in `uint8_t`
   (`matched`, the `(uint8_t)open_blocks.size` casts) and serializes 4 bytes per
   block into tree-sitter's 1024-byte buffer, so it is only sound below 256 blocks.
   A deeper document (nested block quotes or list items) is parsed without tracking
   the extra blocks: its tree degrades, memory stays intact.
2. **`serialize` never writes past the buffer** — defense in depth for the same bound.
3. **List-marker digits are ASCII** (`touring_is_ascii_digit`, 14/09/2026).
   `parse_ordered_list_marker` classified `lexer->lookahead` with `isdigit`, which C
   defines only for `unsigned char` values; the lookahead is a Unicode codepoint, and
   glibc indexes its ctype table with it. `1` followed by U+FF495 at the start of a
   line in an analise document read about 2 MB past the table. Whether a given
   codepoint lands in mapped memory (garbage) or an unmapped page (SIGSEGV) depends
   on the process layout — nothing measured "usually"; the regression test sweeps the
   codepoint space so it faults in any layout. The block cap did not cover it.

Why: 255 or more open blocks wrote past the serialization buffer into the `TSParser`
that owns it (debug builds abort with `ts_parser__external_scanner_serialize:
Assertion 'length <= 1024' failed`; release builds compile the assertion out), and
past 255 the counters wrapped and indexed outside the block stack. The corrupted
parser, reused per thread, crashed the touring daemon LATER, on an unrelated file:
SIGSEGV inside tree-sitter during `index rebuild` (13 and 14/09/2026).

How it was found: a heap-checked in-process rebuild of the analise tree crashed; the
crash-context thread-local read from its core named the file; the extractor alone,
over the analise markdown in sequence, reproduced it; 250 nested quotes pass and 260
abort. The serialize bound alone still crashed a 110-file analise window inside
`scan` (3 of 3 runs); with the cap the window and all 32,979 analise markdown files
parse.

Proof: `crates/touring-code/tests/markdown_scanner_overflow.rs` (child process).
Upstream `master` (last scanner commit 17/02/2026) still has both defects. Drop this
copy when a release carries equivalent bounds.

## AddressSanitizer proof (14/09/2026)

A standalone crate runs the extractor's markdown half — one reused `Parser`, the
`markdown.scm` query, matches and parent walks — with the C sources built with
`-fsanitize=address -DNDEBUG` (assertions off, so only ASan can report):

| Input | Upstream 0.5.3 | This copy |
|---|---|---|
| 250 nested quotes | clean | clean |
| 260 / 400 / 1000 nested quotes, 300-deep list | SEGV in `ts_stack_push` / `stack__iter` | clean |
| analise document with `1` + U+FF495 | SEGV in `parse_ordered_list_marker` | clean (3 passes) |
| all 32,979 analise markdown files | — | 32,979 parsed, 0 ASan reports |

The first full sweep with changes 1 and 2 alone stopped at file 20,409 on change 3's
defect, which is how it was found. `crates/touring-code/tests/markdown_scanner_overflow.rs`
holds the regression tests, and a source guard there fails if either vendored scanner
calls a `<ctype.h>` classifier again.

Census of the other tree-sitter grammars in `Cargo.lock` (29 grammar packages
including this one, `tree-sitter-json` twice — 28 others, 27 distinct names): only
`tree-sitter-bash 0.25.1` calls `isdigit(lexer->lookahead)` (twice, in brace
expansion). Six crafted inputs did not trigger it under ASan, and this file first
recorded it as undefined behaviour without a proven crash. That was wrong: without
ASan (which moves the layout) a C harness died with SIGSEGV 3 of 3 runs, and the
grammar is now patched the same way — see `third_party/tree-sitter-bash/PATCHES.md`
(cross-audit 14/09/2026, D1).

