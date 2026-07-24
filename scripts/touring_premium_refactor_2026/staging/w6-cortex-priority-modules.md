# W6 — touring-cortex Test Debt (v2 — 3 metrics)

## Blocker decision: **🟢 SKIP**

**Rationale**: all 3 metrics within target

## 3 distinct metrics

| Metric | Value | Target | Status |
|--------|-------|--------|--------|
| Pub-ratio (test_fns/pub_items) | 236.22% | 15.00% | 🟢 |
| LOC-ratio (test_loc/src_loc)   | 73.21% | 10.00% | 🟢 |
| File-gap (pub≥3 & tests=0) | 5 | <20 | 🟢 |

## Top 10 priority modules

| Rank | File | LOC | Pub | Tests | Pub% | Priority |
|------|------|-----|-----|-------|------|----------|
| 1 | `crates/touring-cortex/src/dspy/dspy_teleprompter.rs` | 141 | 13 | 0 | 0% | 13.0 |
| 2 | `crates/touring-cortex/src/dspy/dspy_signature.rs` | 93 | 11 | 0 | 0% | 11.0 |
| 3 | `crates/touring-cortex/src/dspy/dspy_compiler.rs` | 94 | 9 | 0 | 0% | 9.0 |
| 4 | `crates/touring-cortex/src/dspy/dspy_module.rs` | 69 | 8 | 0 | 0% | 8.0 |
| 5 | `crates/touring-cortex/src/runtime.rs` | 292 | 5 | 0 | 0% | 5.0 |
