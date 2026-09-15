# 26 nomes de API registrados na baseline (15/09/2026)

Autorização de Gabriel Gadea (AskUserQuestion, 15/09/2026: "Remover M, integrar D, registrar API"),
registrada em `docs/audits/cross-audit-2026-09-14-r2.md` (Fase 7).

Contexto: depois de corrigir o resolvedor de imports Rust (R2-21), 55 símbolos públicos seguiam sem
consumidor fora de testes. 26 mortos e 3 de contrato não cumprido foram removidos (e 2 mortos em cascata);
7 viraram `pub(crate)`. Os 26 abaixo são API por desenho — templates rkyv exercitados pelos testes de
round-trip, reexports de fachada, aliases e métodos públicos sem chamador —, que o critério por nome da
cláusula `orphans_base` não distingue de código morto (mesma questão da política H6).

- `crates/touring-bindings/src/web/components/elite_shell.rs::PaletteCtx`
- `crates/touring-bindings/src/web/models/wiring.rs::WiringModulesReport`
- `crates/touring-ceg/src/gateway/staging.rs::session`
- `crates/touring-code/src/ast/graph/enriched.rs::ImpactCategory`
- `crates/touring-foundation/src/chunker/graceful.rs::ChunkingResult`
- `crates/touring-foundation/src/types.rs::parse`
- `crates/touring-generator/src/plan/result.rs::GenerateResult`
- `crates/touring-generator/src/source_change/mod.rs::is_empty`
- `crates/touring-generator/src/source_change/text_edit.rs::is_empty`
- `crates/touring-intelligence/src/ann/keyword_matcher.rs::patterns`
- `crates/touring-intelligence/src/reasoning/aco_traits.rs::SharedPheromoneLayer`
- `crates/touring-intelligence/src/reasoning/cognitive_mcts.rs::CognitiveMCTS`
- `crates/touring-intelligence/src/rl/aco/models.rs::IntentSpec`
- `crates/touring-intelligence/src/rl/bandit/decision_ledger.rs::PendingDecision`
- `crates/touring-intelligence/src/rl/metacognitive_pipeline.rs::DecisionMetadata`
- `crates/touring-intelligence/src/rl/n3/aco_delegating_generator.rs::DelegationResult`
- `crates/touring-resilience/src/error.rs::Error`
- `crates/touring-rkyv/src/templates.rs::ArchivedGoTSnapshot`
- `crates/touring-rkyv/src/templates.rs::ArchivedHookEvent`
- `crates/touring-rkyv/src/templates.rs::ArchivedLinUCBSnapshot`
- `crates/touring-rkyv/src/templates.rs::ArchivedQTableSnapshot`
- `crates/touring-rkyv/src/templates.rs::ArchivedSymbol`
- `crates/touring-server/src/server/tools_status.rs::StatusInput`
- `crates/touring-storage/src/embedding/client.rs::EmbeddingClient`
- `crates/touring-storage/src/hybrid_search/hybrid/pipeline.rs::HybridScorer`
- `crates/touring-storage/src/salsa/db.rs::FileMeta`
