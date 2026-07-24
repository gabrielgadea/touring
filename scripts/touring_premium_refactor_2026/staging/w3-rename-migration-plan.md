# W3.1 — Rename touring-core → touring-foundation

Generated: 2026-05-12T01:45:53.597624+00:00

## Scope

- **Cargo.toml files** declaring `touring-core`: **17**
- **Source files** using `touring_core::`: **81**
- **Total use statements**: **233**

## Sub-modules of touring-core used by consumers

| Sub-module | Use count |
|------------|-----------|
| `touring_core::TouringConfig` | 45 |
| `touring_core::TouringError` | 42 |
| `touring_core::diagnostic` | 28 |
| `touring_core::schema` | 27 |
| `touring_core::truncate_str` | 17 |
| `touring_core::plugin` | 15 |
| `touring_core::health` | 12 |
| `touring_core::mvkl` | 8 |
| `touring_core::DeltaOutcome` | 7 |
| `touring_core::config` | 6 |
| `touring_core::health_events` | 3 |
| `touring_core::char_classes` | 3 |
| `touring_core::migration` | 3 |
| `touring_core::feedback` | 3 |
| `touring_core::embedding` | 3 |

## Estimate

- Cargo.toml updates: **0.85h**
- ast-grep rewrites: **1.62h**
- Manual review: **8.1h**
- cargo check loops: **1.0h**
- **Total**: **11.57h (1.45 engineer-days)**

## Migration steps (manual / staged, NOT DESTRUCTIVE)

1. **Create new crate** `crates/touring-foundation/` (copy `touring-core/src/`)
2. **Update workspace.members** in root `Cargo.toml`: add `crates/touring-foundation`
3. **Update consumer Cargo.toml** (17 files):

```bash
for f in $(jq -r '.cargo_decl_files[]' data/w3-touring-core-consumer-map.json); do
  sed -i 's/touring-core/touring-foundation/g' "$f"
done
```

4. **Update source files** (81 files) via ast-grep
5. **Validate**: `cargo check --workspace` exit 0
6. **Remove old crate**: `rm -rf crates/touring-core` only after step 5

## Consumer breakdown (top 10 by use count)

| Crate | Files | Total uses |
|-------|-------|------------|
| `touring-hooks` | 29 | 90 |
| `touring-server` | 20 | 88 |
| `touring-analysis` | 15 | 26 |
| `touring-cortex` | 6 | 10 |
| `touring-offensive` | 3 | 5 |
| `touring-embeddings` | 1 | 4 |
| `touring-search-fusion` | 1 | 4 |
| `touring-ast` | 2 | 2 |
| `touring-antt` | 1 | 1 |
| `touring-ast-polyglot` | 1 | 1 |
