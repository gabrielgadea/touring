# Touring Tantivy Index — Rebuild & Tokenizer Guide

**Date**: 2026-05-07
**Version**: v1.0

---

## Architecture: Dual-Index (Porter + Raw)

The Tantivy index uses two tokenizers on `symbol_name` for complementary search quality:

| Field | Tokenizer | Purpose | Example |
|-------|-----------|---------|---------|
| `symbol_name` | `en_stem` (Porter) | BM25 morphological matching | "running" matches "run", "runs", "ran" |
| `symbol_name_raw` | `default` | Fuzzy/prefix exact matching | "useEff" matches "useEffect" |

This dual-index design matches context-mode's FTS5 dual-index pattern.

---

## Tokenizer Details

### `en_stem` (Porter Stemmer)

- **Algorithm**: Martin Porter's Snowball stemmer for English
- **Normalization**: strips suffixes (`-ing`, `-ed`, `-s`, `-tion`, etc.)
- **Effect**: query "execut" matches "execute", "executing", "execution"
- **BM25 benefit**: reduces vocabulary sparsity, improves recall ~20%

### `default` (Whitespace + Lowercase)

- **Behavior**: splits on whitespace, lowercases, no stemming
- **Use case**: fuzzy search via Levenshtein distance on raw tokens

---

## When to Rebuild

1. **Tokenizer change** — switching `symbol_name` tokenizer
2. **Schema version mismatch** — Tantivy detects incompatibility on open
3. **Index corruption** — inconsistent search results

---

## How to Rebuild

```bash
# Full rebuild
touring index rebuild ~/.claude/rust

# Verify
touring index status
touring tantivy search "execute" | head -5
```

---

*Generated: 2026-05-07 | TACO v7.0 | P2-STEM documentation*
