---
name: graph-viz-wave-7
description: Wave 7 (Polish & DI Runtime + Extensibility) — Deliverables D27, D28, D29, D30, D32, D38, D42, D44, D45, D47, D49
type: project
related_files:
  - graph-viz-master-plan_OVERVIEW.md
  - graph-viz-master-plan_STATUS.md
  - graph-viz-master-plan_WAVES_1_2.md
  - graph-viz-master-plan_WAVE_3.md
  - graph-viz-master-plan_WAVE_4.md
  - graph-viz-master-plan_WAVE_5.md
  - graph-viz-master-plan_WAVE_6.md
  - graph-viz-master-plan_WAVE_7.md
  - graph-viz-master-plan_WAVE_8.md
  - graph-viz-master-plan_DEPENDENCIES.md
---

# Wave 7 — Polish & DI Runtime + Extensibility

**Target**: v31.5.0 | **Data**: 2026-05-02

---

## D27 — Plugin architecture runtime swap (DI) 🔴 PENDENTE (0%)

**Falta**:
- [ ] `Provider` trait + `ProviderRegistry`:
  ```rust
  pub trait Provider: Send + Sync + Any {
      fn id(&self) -> &str;
      fn version(&self) -> &str;
      fn capabilities(&self) -> ProviderCapabilities;
  }
  ```
- [ ] Registry: embedding, vector_store, reranker HashMaps
- [ ] CLI: `touring config providers list|set|test`
- [ ] Integration: D22, D23, D24 providers registered via inventory

**Testes**: 16 unit + 4 integration

---

## D28 — MCP overhead self-report 🔴 PENDENTE (0%)

**Falta**:
- [ ] `touring-server/src/telemetry/mcp_overhead.rs` (~80 LOC)
- [ ] `estimate_mcp_overhead()` — per-tool token counting
- [ ] CLI: `touring mcp-overhead [--format json|table] [--top N]`
- [ ] Report em `touring instructions-loaded` session start

**Testes**: 4 unit

---

## D29 — TouringFlowBuilder declarative API 🔴 PENDENTE (0%)

**Falta**:
- [ ] Criar `touring-flow/` crate
- [ ] `TouringFlowBuilder` fluent API (12 steps, 5 targets)
- [ ] Steps: parse_ast, extract_symbols, extract_imports, extract_calls, extract_blast_radius, extract_quality_score, extract_wiring_audit, extract_cycles, extract_cognitive_metrics, extract_rust_semantic, extract_clones, apply_intent_classification
- [ ] Targets: target_sqlite, target_dot, target_mermaid, target_json
- [ ] Filter chain: `filter(predicate)`, `filter_dsl(yaml)`
- [ ] CLI: `touring flow run <yaml-config>`

**Testes**: 40 unit + 6 integration

---

## D30 — YAML Rule Engine + fix transformations 🔴 PENDENTE (0%)

**Falta**:
- [ ] Criar `touring-rule-engine/` crate
- [ ] DSL parser: pattern, kind, regex, inside, not, all, any, has
- [ ] Fix transformations: `pattern → fix` com speculative validation
- [ ] Built-in catalog: ≥30 rules (Rust/TS/Python/Go)
- [ ] CLI: `touring rule list|run|test|explain|fix`
- [ ] Integration: touring-assists (10 handlers refactor to be YAML-callable)

**Testes**: 62 unit + integration

**Risk**: MEDIUM — fix correctness crítico. Mitigação: default dry-run + score >= 0.8

---

## D32 — Tier-based language support honest UX 🔴 PENDENTE (0%)

**Falta**:
- [ ] `touring-language/` crate
- [ ] `LanguageTier` enum: Tier1Primary, Tier2Full, Tier3Community, Tier4Specialized
- [ ] Tier matrix: Rust/TS/Python (T1), Go/Java/C/C++ (T2), JS/Ruby/Swift/Kotlin/Scala/PHP/C# (T3), Bash/YAML/JSON/HCL/Nix/Solidity/CSS/HTML/SQL (T4)
- [ ] CLI: `touring lang status [--tier 1]`, `touring lang capabilities <lang>`
- [ ] SKILL.md section "Language Support Tiers"

**Testes**: 5 unit

---

## D38 — Cross-language perf benchmarks 🔴 PENDENTE (0%)

**Falta**:
- [ ] `benches/throughput.rs` — 9 benchmark scenarios (3 langs × 3 ops)
- [ ] `benches/incremental.rs` — 1% update benchmarks
- [ ] Targets: Rust 1.365+ f/s, TS 944+ f/s, Python 1.188+ f/s, Go 1.870+ f/s
- [ ] CI gate: regression > 10% fails build
- [ ] `docs/perf-baseline.json` committed

**Testes**: 9 benchmark scenarios

---

## D42 — touring init --cc-setup 🟡 PARCIAL (30%)

**Implementado**: `merge_settings_json` logic existe.

**Falta**:
- [ ] Hook scripts embedded via `include_str!`: `touring-pretool-{bash,grep,glob,startup}.sh`
- [ ] `cc_setup()` function: write hooks + merge settings.json + apply profile + register MCP
- [ ] `cc_uninstall()` — symmetric remove
- [ ] Idempotency: 2x install não duplica entries

**Testes**: 14 unit + 4 integration

---

## D44 — Speckit-style slash commands (11) 🔴 PENDENTE (0%)

**Falta**:
- [ ] 11 commands em `~/.claude/commands/`:
  - `touring.health.md` — FASE 0
  - `touring.scout.md` — FASE 1
  - `touring.architect.md` — FASE 2
  - `touring.context7.md` — FASE 3
  - `touring.decompose.md` — FASE 4
  - `touring.audit-pre.md` — FASE 4.5
  - `touring.implement.md` — FASE 5
  - `touring.audit-post.md` — FASE 6
  - `touring.scribe.md` — FASE 7
  - `touring.find.md` — D26 shortcut
  - `touring.flow.md` — D29 shortcut

**Testes**: 12 unit + integration

---

## D47 — Multi-project registry 🔴 PENDENTE (0%)

**Falta**:
- [ ] `touring-server/src/projects/` module
- [ ] `ProjectRegistry`: alias → ProjectEntry (path, daemon_socket, last_used, default)
- [ ] Storage: `~/.claude/touring/projects.json`
- [ ] CLI: `touring project add|list|remove|current`
- [ ] `--project <alias>` em comandos read-only
- [ ] Integration: D26 `find_code --project`

**Testes**: 16 unit + 4 integration

---

## D49 — Handoff frontmatter system (sub-task D44) 🔴 PENDENTE (0%)

**Dependencies**: D44

**Falta**:
- [ ] YAML frontmatter `handoffs:` em cada command
- [ ] Handoff flow: touring.scout → touring.architect/decompose → touring.audit-pre → touring.implement → touring.audit-post → touring.scribe

**Testes**: 4 hand-validate

---

## VALIDAÇÃO GATE WAVE 7

```bash
# plugin runtime swap
touring config providers list -j | jq '.embedding | length'  # ≥ 3 registered
touring config providers set embedding=fastembed
touring config providers test fastembed -j | jq '.health'  # → "ok"

# MCP overhead report
touring mcp-overhead --top 10 --format table
touring mcp-overhead -j | jq '.total_tokens'

# slash commands
/touring.scout crates/touring-hooks
# → handoff buttons appear
```

---

## TOTAL WAVE 7 LOC

| Deliverable | LOC |
|-------------|-----|
| D27 | ~800 |
| D28 | ~80 |
| D29 | ~600 |
| D30 | ~800 |
| D32 | ~80 |
| D38 | ~400 |
| D42 | ~350 |
| D44 | ~500 |
| D47 | ~400 |
| D49 | ~50 |
| **Total** | **~4.060** |