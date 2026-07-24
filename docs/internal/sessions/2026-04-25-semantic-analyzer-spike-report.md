# Semantic-Analyzer Spike Report (2026-04-25)

**Date**: 2026-04-25
**Spike Crate**: `crates/touring-semantic-spike/` (edition 2024, rust 1.88)
**Result**: COMPILES ✅ — but integration is HARD

---

## Key Finding: Rust 1.88 Compatibility

**✅ CONFIRMED**: semantic-analyzer v0.4.7 compiles with Rust 1.88 (2024 edition).

This means the **BLOCKER is not compilation** — it's the API mismatch.

---

## API Surface Analysis

### What Works

| Item | Status | Notes |
|------|--------|-------|
| `Ident::new()` | ✅ | Works, simple API |
| `CodeLocation::new()` | ✅ | Works, line + offset |
| `SemanticContextInstruction` trait | ✅ | Implementable (Debug + Clone + PartialEq) |
| `SemanticStack` | ✅ | Available in types |
| `State<E, I>::new()` | ✅ | Works with proper type bounds |

### Friction Points (HIGH complexity)

| Issue | Severity | Details |
|-------|----------|---------|
| `State<E, I>` requires `E: ExtendedExpression<I>` | HIGH | Complex trait bound — `Expression` doesn't implement it |
| `SemanticContextInstruction` — no `location()` method | HIGH | The trait has no required methods, but `State` uses generics |
| tree-sitter AST → semantic-analyzer AST mapping | HIGH | Different AST ownership models |
| tree-sitter-rust `LanguageFn` vs `&Language` | MEDIUM | tree-sitter API mismatch, not semantic-analyzer |
| GlobalState created inline in State::new() | MEDIUM | No standalone constructor, must use `State` |

### API Shape

```rust
// What we successfully used:
Ident::new("name")                     // → &str fragment
CodeLocation::new(line, offset)        // → u32, usize

// What requires more investigation:
State::<E, I>::new()                   // needs E: ExtendedExpression<I>
SemanticStack::<I>::new()             // internal structure unknown
```

---

## Verdict: ARCHIVE or CONTINUE?

**Recommendation: ARCHIVE the spike, not the capability.**

### Rationale

1. **Semantic-analyzer is designed for compiler frontends that own their AST**.
   It expects AST nodes from its own parser, not tree-sitter.

2. **The adapter challenge is non-trivial**:
   - tree-sitter: `Node<'a>` with byte ranges
   - semantic-analyzer: `Ident<'a>` with `LocatedSpan`
   - Mapping between them requires significant boilerplate

3. **Overlap with existing syn-based analysis**:
   - `RustSemanticReport` (touring-ast) already provides: generics, trait bounds, lifetimes, derives, unsafe/async counts
   - `semantic-analyzer` would add: actual type resolution (concrete types)
   - The value-add is real, but the integration cost is high

4. **syn + semantic-analyzer compose** (parsing → semantic analysis), but the bridge requires a custom adapter that doesn't exist.

---

## Recommendations

### Option A: ARCHIVE (recommended)
Archive `touring-semantic-spike`. The API mismatch makes this a significant engineering effort without guaranteed payoff.

**Keep**: `RustSemanticReport` (syn-based, already production)
**Skip**: semantic-analyzer integration for now

### Option B: INVEST (if concrete use case emerges)
If a concrete downstream consumer needs actual type resolution (not just syntactic analysis), revisit with:
1. Clear API contract between tree-sitter nodes and semantic-analyzer input
2. Budget of 1-2 weeks for adapter development
3. POC with a real Rust file (not synthetic test)

### What Was Learned

| Learning | Impact |
|----------|--------|
| Rust 1.88 compatibility | ✅ Can use 2024 edition crates if needed |
| API complexity | Medium — trait bounds require understanding full semantic stack |
| AST ownership model | semantic-analyzer owns AST; tree-sitter is a consumer |

---

## Files Created

- `crates/touring-semantic-spike/Cargo.toml` — isolated 2024 edition crate
- `crates/touring-semantic-spike/src/lib.rs` — 5 passing tests confirming API surface

## Files Removed

- `src/tree_sitter_adapter.rs` — removed (API mismatch too complex for spike)

---

*Spike conducted by TACO v6.2 — Touring Agentic Code Orchestrator*
*Confidence: 0.85 (compile verified, API surface tested, verdict documented)*