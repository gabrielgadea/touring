# W8 — touring-hooks Split Plan (v5 — leaf invariant enforced)

Total files: 224 | LOC: 152,371
Trivial cycles: **5**
Real cycles: **4** ❌
Leaf violations remaining in shared: **1**

## Bucket distribution

| Bucket | Files | LOC |
|--------|-------|-----|
| `touring-hooks-lifecycle` | 57 | 62,933 |
| `touring-hooks-core` | 82 | 40,575 |
| `touring-hooks-cli` | 18 | 19,995 |
| `touring-hooks-tools` | 24 | 12,868 |
| `touring-hooks-prediction` | 8 | 5,375 |
| `touring-hooks-shared` | 20 | 4,970 |
| `touring-hooks-infra` | 8 | 3,541 |
| `touring-hooks-rl` | 1 | 1,086 |
| `touring-hooks-facade` | 1 | 535 |
| `touring-hooks-misc` | 5 | 493 |

## REAL cycles (need refactor)

- `touring-hooks-infra → touring-hooks-lifecycle → touring-hooks-tools → touring-hooks-core`
- `touring-hooks-lifecycle → touring-hooks-tools → touring-hooks-core`
- `touring-hooks-tools → touring-hooks-core → touring-hooks-shared`
- `touring-hooks-lifecycle → touring-hooks-tools → touring-hooks-cli`

## Leaf invariant violations (move out of shared)

- `crates/touring-hooks/src/tool_output_router.rs` → imports: `sandbox_executor` (touring-hooks-tools)
