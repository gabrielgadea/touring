# VP-Scout — Worked Examples + Historical Origins

Companion reference for `~/.claude/rules/VP-Scout.md`. The rule keeps the 9 chains as operational checklists; this file holds the worked-through examples that demonstrate how each chain catches a specific FP, plus the historical origins of chains 4b, 5, 6, 7 (added after specific real-world misses).

## Cadeia 4b — Origem (konverter 2026-05-02)

Diagnóstico do projeto konverter mostrou que Cadeia 4 (originalmente Rust-only) missava `UrnLex` em `lexcore-br/src/lexcore_br/models.py:14`. Workspaces polyglot (Rust + Python + TypeScript) precisam verificar homônimos em todas as linguagens simultaneamente.

**Exemplo real**:
- Cadeia 4 só verificou Rust → reportou "UrnLex em apenas 2 lugares"
- Cadeia 4b adiciona `grep --include='*.py'` → encontra `UrnLex` em `lexcore_br/models.py:14`
- Total: 3 implementações (2 Rust + 1 Python) — SCOUT MISSED via Cadeia 4 isolada
- Confidence aplicação Cadeia 4b: 0.85 — mandatory em polyglot workspaces.

## Cadeia 5 — Detail: cargo check return codes

**IMPORTANTE**: `cargo check` retorna exit code 0 (success) mesmo sem warnings. Exit code != 0 significa erros reais. Contar linhas com `^error\[` no output.

## Cadeia 7 — Exemplo Real (Wave Preditiva 2026-04-20)

`wiring orphans` reportou `ShadowRolloutResult.as_hint` como órfão.
```bash
grep -rn "as_hint" crates/
# → encontrado em plan_mode/enter.rs:433 via mcts_shadow_rollout_hint
```
**VERDICT**: WIRING_STALE — não era orphan real. Falso positivo evitado.

---

## Exemplo: Aplicando VP-Scout a Opp3 (simd-search)

```
OPP3: "Habilitar simd-search em touring-ast"

→ Cadeia 1 (Feature Trace):
  touring index find "simd-search" → touring-ast/Cargo.toml: optional = true
  touring wiring modules touring-hooks → features = ["simd-search"] ← ATIVOU!
  touring ast find "CosineComputer" → touring-ast/semantic_search.rs usa

→ Cadeia 2: Não cruza crate boundary (mesmo crate touring-ast)

→ Cadeia 3 (Already Implemented):
  touring index find "CosineComputer" → touring-hooks/semantic_classifier.rs importa
  touring wiring orphans -j → CosineComputer tem consumer em touring-hooks

→ Cadeia 4: Nome não genérico — pulado

VERDICT: JÁ IMPLEMENTADO — touring-hooks já habilitou simd-search ao importar touring-ast
FALSE POSITIVE AVOIDED
```

## Exemplo: Aplicando VP-Scout a Opp5 (ACO pheromone)

```
OPP5: "ACO pheromone loop incompleto — conectar AcoPheromone ao MetacognitivePipeline"

→ Cadeia 1: Feature gate — não aplicável

→ Cadeia 2 (Dependency Cycle):
  touring-graph-dependencies: touring-simd → touring-hooks → touring-learning
  touring-simd NÃO depende de touring-hooks → sem ciclo direto
  touring-hooks NÃO depende de touring-simd → sem ciclo direto

→ Cadeia 3 (Already Implemented):
  touring index find "AcoPheromone" → touring-simd/learning.rs + touring-hooks/post_tool_rl.rs
  touring ast find "AcoPheromone" touring-simd → corpo: adjust_threshold_from_feedback(batch, outcome)
  touring ast find "AcoPheromone" touring-hooks → touring-simd, não touring-hooks
  touring ast find "HookQualityAssessment" → touring-hooks/post_tool_rl.rs: ACO já wired

→ Cadeia 4 (Homonimia):
  touring index find "ACO" → touring-simd (AcoPheromone) ≠ touring-hooks (HookQualityAssessment)
  touring-hooks/ACO = metrics de hook effectiveness
  touring-simd/ACO = adaptive threshold para SIMD batch
  SÃO HOMÔNIMOS com module_paths DIFERENTES

VERDICT: BLOCKED_HOMONYMIA — touring-simd::AcoPheromone ≠ touring-hooks::ACO
São sistemas INDEPENDENTES. A связывание propuesta não faz sentido arquiteturalmente.
FALSE POSITIVE AVOIDED
```

## Exemplo: Homonimia Intra-Crate via Type Alias (Wave Preditiva 2026-04-20)

```
OPP: "CognitiveMCTS — integrar struct com pipeline preditivo"

→ Cadeia 4 (Homonimia):
  touring index find "CognitiveMCTS" → 2 resultados no MESMO crate touring-cognitive:
    cognitive_mcts.rs:170  → type alias CognitiveMCTS = GraphInformedMCTS  (canônico, público)
    mcts.rs:649            → struct PheromoneMCTS (ex-CognitiveMCTS, renomeado 2026-04-20)

  ANÁLISE: mesmo crate, nomes DIFERENTES após rename, mas vestígios no índice ainda apontam
  para nome antigo. touring index pode mostrar ambos temporariamente por staleness.

→ Cadeia 7 (Wiring Cache Staleness):
  grep -rn "CognitiveMCTS" crates/touring-cognitive/src/ → 
    cognitive_mcts.rs:170: pub type CognitiveMCTS = GraphInformedMCTS;  (type alias real)
    mcts.rs: nenhuma ocorrência (foi renomeado para PheromoneMCTS)

VERDICT: BLOCKED_HOMONYMIA (intra-crate, type alias vs struct)
  - CognitiveMCTS em cognitive_mcts.rs = type alias para GraphInformedMCTS → usar este
  - PheromoneMCTS em mcts.rs = struct independente (ex-CognitiveMCTS)
  São ENTIDADES DISTINTAS. Integrar usando o type alias canônico (cognitive_mcts.rs).

LESSON: homonimia pode ocorrer DENTRO do mesmo crate via type alias + struct com nomes que
divergiram por rename. Sempre verificar via grep se o nome no índice é alias ou struct real.
FALSE POSITIVE AVOIDED
```

## Exemplo: Orphan Falso por Wiring Staleness (Wave Preditiva 2026-04-20)

```
REPORT: "ShadowRolloutResult.as_hint é orphan symbol — sem consumidores"

→ Cadeia 7 (Wiring Cache Staleness):
  touring wiring orphans -j → lista ShadowRolloutResult.as_hint como orphan

  grep -rn "as_hint" crates/ --include="*.rs" | head -10
  → crates/touring-cognitive/src/plan_mode/enter.rs:433:
      let hint = mcts_shadow_rollout_hint(state).as_hint();

  RESULTADO: consumer encontrado em plan_mode/enter.rs:433

VERDICT: WIRING_STALE — o wiring DB ainda não refletiu o edit de plan_mode/enter.rs
O símbolo TEM consumer real. NÃO é orphan. Não gerar subtask de wiring.

ACTION: `touring index rebuild` para forçar re-sync, ou aguardar próximo ciclo automático.
FALSE POSITIVE AVOIDED — ref: docs/2026-04-20-predictive-wave.md
```
