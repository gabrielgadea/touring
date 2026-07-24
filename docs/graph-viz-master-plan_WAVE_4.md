---
name: graph-viz-wave-4
description: Wave 4 (Resilience Patterns + Semantic Foundation) — Deliverables D18, D19, D20, D21, D31, D33
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

# Wave 4 — Resilience Patterns + Semantic Foundation

**Target**: v30.9.0 | **Data**: 2026-05-02

---

## D18 — CheckpointSettingsFingerprint family-aware 🟡 PARCIAL (70%)

**Implementado**:
- `touring-core/src/checkpoint/` com fingerprint logic
- `CheckpointSettingsFingerprint` struct com `config_type`, `primary_chunker`, `family`, `config_hash`

**Falta**:
- [ ] `is_compatible_with()` method — symmetric/asymmetric logic
- [ ] Integration em `touring session start` para decide reuse vs reindex
- [ ] `ChangeImpact` enum (None/Compatible/BreakingMinor/BreakingMajor)
- [ ] CLI `touring session start` mostra "config changed: Compatible"

**Testes**: 14 unit (symmetric/asymmetric × same/different)

---

## D19 — FailoverService cross-subsystem 🟡 PARCIAL (50%)

**Implementado**:
- `touring-core/src/failover/` trait `Failover<P, B>` com `primary_health()`, `activate_backup()`, `sync_backup()`, `restore_to_primary()`

**Falta**:
- [ ] Default impl para 3 subsystems:
  - tantivy primary → fallback indices
  - daemon primary → CLI-only mode
  - vector store primary → local sqlite-vec backup
- [ ] Health monitor periódico (30s)
- [ ] Counters em gate-metrics: `failover_active_count`, `failover_transitions_count`, `failover_recovery_count`

**Testes**: 12 unit + 2 integration

---

## D20 — rignore-style file filtering audit 🔴 PENDENTE (0%)

**FASE 1 (audit)**:
```bash
grep -r "gitignore\|ignore_hidden\|read_git_ignore\|override" touring-vfs/src/ --include="*.rs"
```

**Gap report checklist**:
- ✅/❌ .gitignore parent dirs respect
- ✅/❌ Global gitignore (~/.config/git/ignore)
- ✅/❌ Override patterns (.github, .vscode, .claude, .circleci)
- ✅/❌ Hidden file handling (whitelist)
- ✅/❌ Extension filter (~360 types)

**Se gap**: implementar via `ignore` crate.

**Testes**: 8 unit (se gaps) ou 0 (se paridade)

---

## D21 — Knowledge base de node types JSON exposto 🟡 PARCIAL (30%)

**Implementado**:
- tree-sitter grammars wired em touring-ast

**Falta**:
- [ ] `touring ast node-types <lang>` — JSON com node_count + node_types
- [ ] Importance scoring: definition (0.9-1.0), declaration (0.6-0.8), statement (0.3-0.5), expression (0.1-0.2)
- [ ] `touring ast importance <file> --threshold 0.5`
- [ ] MCP tools: `touring_ast_node_types(language)`, `touring_ast_importance(file, threshold)`

**Testes**: 6 unit

---

## D31 — Semantic Classification crate 🔴 PENDENTE (0%)

**Status**: `touring-definitions/` crate NÃO existe. SemanticClassifier em touring-hooks é diferente (TF-IDF + SIMP para Q-Learning reranking, não classificação semântica de nodes).

**Falta**:
- [ ] Criar `touring-definitions/` crate
- [ ] 22 SemanticClass categories (FunctionDef, StructDef, EnumDef, etc.)
- [ ] Pipeline: override → file_detection → token_purpose → universal_exact → universal_majority → category → name_heuristic → unclassified
- [ ] Data files: `universal_rules.json` (2.444 rules), `categories.json`, `scoring.json`
- [ ] Language overrides (~41 LOC TOML per language)
- [ ] Integration: touring-ast enrich `ast meta` com SemanticClass
- [ ] CLI: `touring definitions classify <file>`

**Dependencies**: D21 (node types KB) como input.

**Testes**: 18 unit + 27 integration + 5 corner cases

---

## D33 — Multi-tier conflict detection com SLAs 🔴 PENDENTE (0%)

**Falta**:
- [ ] `touring-core/src/conflict/` module
- [ ] `ConflictTier` enum: AstDiff (<100ms), Semantic (<1s), GraphImpact (<5s)
- [ ] `SlaSpec` struct com p99_ms
- [ ] 3 detectors: AstDiffDetector, SemanticConflictDetector, GraphImpactDetector
- [ ] `touring conflict detect <a> <b> --tier 1|2|3`
- [ ] 6 counters em gate-metrics: `conflict_tier{1,2,3}_p99_ms`, `conflict_tier{1,2,3}_violations_count`
- [ ] CI gate: iai-callgrind benchmark fails se P99 > SLA

**Dependencies**: D19 (failover), D37 (overlay), D38 (perf benchmarks).

**Testes**: 12 unit + 6 integration

---

## VALIDAÇÃO GATE WAVE 4

```bash
# family-aware compatibility
touring session start fam-test type "test"
touring config set chunker.tree_sitter_version 0.20.10  # minor bump same family
touring session start fam-test type "test"  # should NOT trigger reindex

# failover service
touring failover status -j  # → {primary_active: true, backup_ready: true}

# rignore parity
echo "secret.txt" >> .gitignore
touring file-knowledge extended secret.txt 2>&1 | grep -q "ignored"

# node types KB
touring ast node-types rust -j | jq '.node_types | length'  # ≥ 100
```