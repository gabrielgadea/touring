# Touring Sprint 2 — Resultados (2026-03-26)

## H1-C: petgraph DependencyCache
- Arquivo novo: `crates/touring-hooks/src/dependency_cache.rs`
- `DependencyCache` com `StableGraph<PathBuf, ()>` + `HashMap<PathBuf, NodeIndex>`
- `blast_radius()` via `petgraph::visit::Reversed` + BFS (O(V+E))
- `build_from_relations()` para seed a partir do SQLite
- `add_relation()`, `remove_file()`, `direct_dependents()`, `direct_deps()`
- Integrado ao `HookRuntime`: campo `pub dependency_cache: Option<DependencyCache>`
- Métodos: `init_dependency_cache()`, `add_dependency()`, `petgraph_blast_radius()`
- `all_file_relations()` adicionado ao `FileKnowledgeDB` em `knowledge.rs`
- petgraph adicionado ao Cargo.toml do touring-hooks
- 10 novos testes unitários no módulo

## H1-D: RL Evolution — 3 novas features + FEATURE_DIM 19→25
- GOTCHA: file_size_bucket e recent_errors JÁ EXISTIAM — não duplicar
- Novas features adicionadas:
  - [19] `error_count_session`: f64 contínuo [0,1], normalizado /10
  - [20] `recent_tool_success_rate`: f64 contínuo [0,1], default 0.5
  - [21..24] `time_of_day_bucket`: one-hot 4 dims (night/morning/afternoon/evening)
- `extract_features_rich()` expandida com 3 novos parâmetros opcionais
- `extract_features()` mantém assinatura (chama _rich com None,None,None)
- Reward JÁ ERA contínuo (-1.0 a 1.0) via latency+quality blend — não alterado
- Modelos salvos com d=19 são auto-descartados (from_snapshot retorna Err por dim mismatch)
- Call site em online_rl.rs atualizado com None,None,None para novos params

## Resultados dos Gates
- FEATURE_DIM: 19 → 25
- cargo clippy --workspace: 0 warnings
- cargo test --workspace --exclude touring-python: 2388 passed (baseline=2378, +10)
- Daemon: rebuilt + running
- 24 hook event types configurados
