# ES1 P2.7.6 — Real Array Semantics (cvc5)

**Release Note** · **Date**: 2026-06-03 · **Wave**: ES1 P2.7.6
**Predecessor**: ES1 P2.7.5 (TranslationContext + dispatcher refactor) · **Status**: SHIPPED

## What changed

`cvc5_backend.rs` TranslationContext is extended with array-specific
state, and the top-level dispatcher (`translate_symbol`) routes
`ArraySelect` / `ArrayStore` through real `Kind::Select(154)` /
`Kind::Store(155)` semantics when the base is array-typed. Non-array
bases (and `Load`) fall back to the P2.6 base-recursion behavior.

### TranslationContext extensions

```rust
struct TranslationContext<'a, 'tm> {
    // ... existing fields (tm, int_sort, bv_sort, vars, bv_vars) ...
    array_sort: &'a cvc5::Sort<'tm>,         // NEW: cached Array(int, int) sort
    array_vars: &'a mut HashMap<String, cvc5::Term<'tm>>,  // NEW: per-recursion array var identity
    array_counter: &'a mut u32,              // NEW: unique name source for fresh_const_array
}
```

### New helpers (impl block)

| Helper | Purpose |
|---|---|
| `declare_array_var(name) -> Term` | Per-recursion Const identity for arrays (same name → same cvc5 array Const). |
| `fresh_const_array(v: i64) -> Term` | `tm.mk_const_array(array_sort, mk_integer(v))` with unique counter. |
| `translate_array_base(expr) -> Term` | Dispatch: Variable → declare_array_var, Constant → fresh_const_array, ArrayStore/ArraySelect → recurse via translate_symbol_int, else → translate as int (caller falls back). |
| `is_array_base_kind(kind) -> bool` | Free fn predicate: returns true for Variable, Constant, ArrayStore, ArraySelect. |

### Dispatcher wiring

```rust
// P2.7.6: real array semantics via Kind::Select/Kind::Store
SymbolKind::Load(base) => translate_symbol(ctx, base),  // P2.6 fallback (ambiguous intent)
SymbolKind::ArraySelect(arr, idx) => {
    if is_array_base_kind(&arr.kind) {
        let arr_t = ctx.translate_array_base(arr);
        let idx_t = translate_symbol_int(ctx, idx);
        ctx.tm.mk_term(cvc5::Kind::Select, &[arr_t, idx_t])
    } else {
        translate_symbol(ctx, arr)  // P2.6 fallback
    }
}
SymbolKind::ArrayStore(arr, idx, val) => {
    if is_array_base_kind(&arr.kind) {
        let arr_t = ctx.translate_array_base(arr);
        let idx_t = translate_symbol_int(ctx, idx);
        let val_t = translate_symbol_int(ctx, val);
        ctx.tm.mk_term(cvc5::Kind::Store, &[arr_t, idx_t, val_t])
    } else {
        translate_symbol(ctx, val)  // P2.6 fallback
    }
}
```

## What's new for users

- `ArraySelect(arr, idx)` and `ArrayStore(arr, idx, val)` now produce
  real cvc5 array terms (not stub recursion to the base or value).
- The existing integration test
  `cvc5_translate_arrayselect_arraystore_load_fallback` still passes:
  - `Load(var)` uses P2.6 fallback (returns the base var as int).
  - `ArraySelect(arr, idx)` produces a real `Kind::Select` int term
    that coerces to bool via `Equal(X, X)`.
  - `ArrayStore(arr, idx, val)` produces a real `Kind::Store` array
    term that coerces to bool via `Equal(X, X)` (cvc5 `Equal` is
    polymorphic over any sort).
- All assertions of array terms are sat (trivially true via
  self-equality), matching the P2.6 baseline behavior.

## What's not yet shipped

- **Multidimensional arrays** — only `Array(int, int)` is cached.
  Extending to `Array(int, Array(int, int))` requires an additional
  sort cache + per-dimension var maps. Future scope.
- **Array variable MODEL extraction** — `collect_int_variables` only
  walks int-sorted vars. Array var values are not extracted in
  `get_model()`. Future scope (low priority; array terms in
  constraints are rare in concolic fuzzing).
- **`is_array` predicate usage in coerce_to_bool** — currently
  `coerce_to_bool` wraps non-bool terms in `Equal(X, X)`. For
  array terms this is fine (Equal is polymorphic). If future
  scenarios need a "default-true" for array terms, switch to
  `mk_true()` when `term.sort().is_array()`.

## Migration notes

- **TranslationContext signature changed**: callers that construct
  `TranslationContext` directly must add the 3 new fields. Two
  production sites (`check_sat`, `get_model`) and the test helper
  `make_test_ctx` (8 unit-test callers) updated.
- **Public API unchanged**: `SolverBackend` trait + `CVC5SolverBackend`
  methods all have identical signatures.
- **coerce_to_bool unchanged**: existing `Equal(X, X)` wrapping
  already handles array-typed terms correctly (cvc5 `Equal` is
  polymorphic over any sort).
- **Logic preserved**: `Load` still uses P2.6 fallback (single-arg
  primitive with ambiguous intent in this context).

## Test results

| Build | Lib | Integration | Doc | Total | Δ vs P2.7.5 |
|---|---:|---:|---:|---:|---:|
| Default features | 277 | 14 | 18 | **309** | 0 (zero regression) |
| cvc5 + z3 features | 320 | 14 | 18 | **352** | +3 (3 new array unit tests) |

**+3 delta breakdown** (cvc5+z3 features only):
- `cvc5_array_translation_unit_test_declare_array_var` — per-recursion
  identity for array vars (declare + lookup returns same cvc5 term).
- `cvc5_array_translation_unit_test_fresh_const_array` — counter
  increments on each call + array sort verification.
- `cvc5_array_translation_unit_test_select_store_real_semantics` —
  end-to-end via `translate_symbol` free fn, verifies ArraySelect
  produces int term + ArrayStore produces array term + array_vars
  map has exactly 1 entry.

## Critical discoveries (persisted to memory)

1. **cvc5 API confirmed via Context7** (daemon-degraded fallback to
   `~/.cargo/registry/src/.../cvc5-0.4.0/src/{sort,term_manager}.rs`):
   - `tm.mk_array_sort(index, elem) -> Sort<'_>` (term_manager.rs:68)
   - `tm.mk_const_array(sort, val) -> Term<'_>` (term_manager.rs:381)
   - `Sort::is_array() -> bool` (sort.rs:142)
2. **`coerce_to_bool` already array-aware** via `Equal(X, X)` —
   no change needed.
3. **cvc5 canonicalizes const arrays** — two
   `tm.mk_const_array(sort, 0)` calls MAY return the same `Term::id`.
   The counter is for naming uniqueness, not term uniqueness.
4. **Per-sort var maps required** — `array_vars` separate from
   `vars` (int) and `bv_vars` (BV) to avoid sort conflicts when
   the same name appears in multiple sort contexts.
5. **is_array_base_kind cleanly separates** array-typed bases
   (Variable/Constant/ArrayStore/ArraySelect) from non-array (Load
   + arith variants like Add, Sub, etc.).
6. **Free function `translate_symbol`** (top-level dispatcher) is
   the right place for the real array path. The method
   `translate_symbol_int` keeps the P2.6 fallback for arith
   variants only.

## Compatibility

- **No breaking changes** to public API.
- **No breaking changes** to coercion behavior.
- **All P2.6 + P2.7 + P2.7.5 invariants preserved**:
  - 12 concolic tests pass.
  - 12 z3↔cvc5 consistency oracles pass.
  - 8 BV unit tests pass.
  - 2 quantifier tests pass.
  - 1 arrayselect/arraystore/load test passes.
  - 3 NEW array unit tests pass.

## Acknowledgments

- Predecessor ES1 P2.7.5 (2026-06-03) — TranslationContext + dispatcher refactor
- Predecessor ES1 P2.7 (2026-06-03) — 8 BV variants + real quantifiers
- Predecessor ES1 P2.6 (2026-06-02) — 3-dispatcher split for sort safety
- cvc5 0.4.0 source verified at `~/.cargo/registry/src/.../cvc5-0.4.0/`
- Context7 docs (cvc5 TermManager + Array Operators + cvc5.Kind enum)
- `z3_backend.rs` — reference pattern for 3-dispatcher structure
