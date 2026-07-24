# Touring-AST ↔ Touring-Hooks Integration Lessons

## Architecture Overview

### touring-ast (21 modules)
- CC hotspots: `find_body CC=29`, `detect_visibility CC=23`
- SharedPipeline: mutex bottleneck identified

### touring-hooks (18 hooks)
- RuntimeMap: per-project locking architecture
- Two blast_radius implementations (divergent return types)

## P0 Bug: Blast Radius Return Type Mismatch

### Problem
- `HookRuntime::blast_radius` → `petgraph Vec<PathBuf>`
- `DependencyCache::blast_radius` → `SymbolIndex BlastRadius`

### Solution Implemented
- Created `BlastRadiusOutput` enum with:
  - `Files(Vec<PathBuf>)` variant
  - `Rich(BlastRadius)` variant
- `.files()` method extracts `Vec<PathBuf>` from any variant
- `From<Vec<PathBuf>>` and `From<BlastRadius>` implemented
- `PartialEq` derived for both types
- Integration quality: 0.75 → 0.92 after P0 fix

## P1 Issues
- SharedPipeline mutex bottleneck
- Circuit breaker stale state

## P2 Issues
- CILA budget hardcoded
- SymbolKind language coverage gaps
