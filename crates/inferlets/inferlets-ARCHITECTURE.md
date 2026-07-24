# inferlets — Architecture

> **Version**: v0.1.0 | **Updated**: 2026-05-11 | **LOC**: 3687 | **Constraints**: `#![forbid(unsafe_code)]`

## Overview

Sandboxed WASM inferlet plugins for touring-wasm: keyword-based classifiers compiled to a single WebAssembly module with a single `evaluate()` entry point that dispatches to the appropriate inferlet based on the `__inferlet__` discriminator key in the input JSON. The crate avoids `serde_json` at runtime, using direct string manipulation for WASM-compatible JSON parsing.

## Key Types

| Type | File | Purpose |
|------|------|---------|
| `SerializedInferlet` | `serialized.rs` | On-disk cache with SHA-256 integrity and magic header |
| `InferletManifest` | `manifest.rs` | Manifest for a collection of inferlets |
| `InferletDep` | `manifest.rs` | Dependency entry in a manifest |
| `InferletMetadata` | `manifest.rs` | Metadata for a single inferlet |
| `InferletManifestLoadError` | `manifest.rs` | Error enum for manifest loading |
| `ComplexFile` | `top_n_complex_files.rs` | File with complexity metrics |
| `Input` | `top_n_complex_files.rs` | Input for top-N complex files analysis |
| `Output` | `top_n_complex_files.rs` | Output from top-N complex files analysis |
| `Distribution` | `tdg_grade_distribution.rs` | TDG grade distribution result |
| `FlakyTest` | `flaky_test_pattern_detector.rs` | Detected flaky test pattern |
| `Cycle` | `find_circular_imports.rs` | Circular import cycle |
| `Orphan` | `unused_pub_symbols.rs` | Unused public symbol |
| `ChangedCrate` | `dependency_diff.rs` | Crate with changed dependencies |
| `CrateEntry` | `dependency_diff.rs` | Entry in dependency diff |
| `ExtensionCount` | `count_files_via_cli_wrapper.rs` | File count by extension |
| `Trend` | `composite_health_trend.rs` | Health trend direction enum |

## Dependencies

- **No touring-* dependencies** — standalone WASM compilation target
- `sha2` — SHA-256 integrity checking for serialized cache

## Feature Flags

None.

## Key Modules

| Module | Description |
|--------|-------------|
| `lib.rs` | Thread-local INPUT buffer, `set_input()` FFI, `evaluate()` dispatcher, `strip_inferlet_key()` |
| `always_success.rs` | Testing inferlet — always returns 1 |
| `memory.rs` | Detects memory-related input (alloc, heap, stack, leak, fragmentation) |
| `pattern.rs` | Detects pattern/regex matching input (match, regex, search, find) |
| `classifier.rs` | Detects classification input (classify, semantic, tfidf, embedding, intent) |
| `serialized.rs` | Binary cache format: `[magic:4][version:2][name_len:2][name:N][sha256:32][wasm:...]` |
| `manifest.rs` | Inferlet manifest and dependency types |
| `tantivy_query_builder.rs` | Tantivy query builder for inferlet search |
| `synergy_health_check.rs` | Health check for synergy wiring |
| `composite_health_trend.rs` | Composite health trend analysis |
| `top_n_complex_files.rs` | Top-N complex files detector |
| `dependency_diff.rs` | Dependency difference analysis |
| `find_circular_imports.rs` | Circular import detection |
| `unused_pub_symbols.rs` | Unused public symbol detector |
| `flaky_test_pattern_detector.rs` | Flaky test pattern detection |
| `count_files_via_cli_wrapper.rs` | File count by extension via CLI |
| `tdg_grade_distribution.rs` | TDG grade distribution analysis |

## WASM ABI

```
1. Host calls set_input(ptr, len) -> writes JSON to thread_local INPUT
2. Host calls evaluate()          -> reads INPUT, dispatches by __inferlet__ key
3. Return: i32 (0 = no match, 1 = match)
```

Dispatch logic: if `__inferlet__` key is present, routes to that specific inferlet.
Otherwise, tries all 4 inferlets sequentially and returns 1 if any matched.

## Build

- `crate-type = ["cdylib", "rlib"]` -- produces both `.wasm` and linkable library
- Release profile: `opt-level = "s"`, LTO, `codegen-units = 1`, stripped symbols

## Invariants

1. **No serde_json at runtime** -- all JSON parsing is manual string manipulation (WASM-friendly)
2. **Zero-argument evaluate** -- WASM ABI constraint; input flows through thread-local buffer
3. **Cache integrity** -- serialized inferlets are SHA-256 verified before loading

## Tests

- **9 tests** covering dispatch, stripping, and per-inferlet keyword matching
- Run: `cargo test --package inferlets`
