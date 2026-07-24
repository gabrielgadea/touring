# How to add a language to the index

> A **how-to** (Diátaxis): task-oriented. You want Touring's index/AST to
> understand a new source language. Master Plan D.W4.P3.

## Goal

Get a new language parsed into the symbol index and AST queries (`ast meta`,
`ast overview`, `ast grep`) so blast-radius and search work for it.

## Honest limitation first (A12)

Touring's multi-language support is **tree-sitter-based and syntactic**. Adding a
language gives you parsing, symbol extraction, and structural search/rewrite —
**not** cross-file type inference. Semantic resolution at that depth is
Rust-only today and full LSP-grade inference is roadmap (Master Plan A.W4,
salsa-backed). Set expectations accordingly: "add a language" means "index and
navigate it", not "type-check it".

## Steps

1. **Confirm a tree-sitter grammar exists** for the language and is on a
   compatible ABI. The polyglot layer (`touring-code`) wraps `ast-grep`/tree-
   sitter; an ABI mismatch is the most common failure (e.g. tree-sitter-go ABI
   v15 has bitten this project before — see the fuzz findings).

2. **Register the grammar** in the polyglot module so `sniff_language` maps the
   file extension to the parser. This is where the CEG's X1 CLASSIFY stage and
   `ast grep --lang <name>` both resolve the language.

3. **Map node kinds to symbols.** Tell the extractor which tree-sitter node types
   are definitions (functions, structs/classes, methods) so they land in the
   symbol index with the right `kind`.

4. **Rebuild and reindex:**
   ```bash
   update-touring                 # rebuild release + restart daemon
   touring index rebuild "$PWD"   # re-parse the workspace with the new grammar
   ```

## Verify

```bash
# Structural search must now work for the new language
touring ast grep path/to/file.<ext> '<pattern>' --lang <name>

# A known symbol must resolve in the index
touring index find <KnownSymbol>

# Metadata must populate (blast radius, etc.)
touring ast meta path/to/file.<ext> --depth summary -j
```

If `ast grep --lang <name>` returns matches and `index find` locates a symbol you
know exists, the language is wired. If `index find` is empty but `grep` finds the
text, the node-kind→symbol mapping (step 3) is incomplete.

## Pitfall: ABI version

If parsing panics or returns nothing after a grammar bump, suspect the
tree-sitter ABI. Pin the grammar crate to a version whose ABI matches the
`ast-grep-core` in the workspace `Cargo.toml`; mismatches surface as
`.expect()`/parse failures in the node layer, not as graceful empties.
