# Plano de Correcao — Top Issues TACO Audit

> **STATUS:** DOCUMENTO HISTRICO — Snapshot de 08/04/2026 (era v21.1.0)
> **NÃO ATUALIZAR:** Decisoes e achados so registros histricos do momento da analise

08/04/2026 | Overall Score: 0.688 | Status: EM ANDAMENTO

---

## STATUS DOS ISSUES

Issue 1: edit_history schema mismatch — RESOLVIDO
  fix: TABLE_EDIT_HISTORY = "edit_history" (era "file_edit_history")
  resultado: edit_history_count 0->3, knowledge score 0.35->0.467

Issue 2: RL LinUCB inactive — ROOT CAUSE FOUND (scout a3e892 COMPLETED)
  ROOT CAUSE: touring-daemon DOWN desde 05/Apr (SIGKILL)
  daemon morto = post-tool-rl hook nao executa = LinUCB nunca recebe reward
  acao: REINICIAR daemon com binarios atualizados

Issue 3: 89.3% orphan rate — REVISADO (scout a3e892 COMPLETED)
  finding: ESTRUTURAL E INTENCIONAL — 75%+ sao Python/JSON/TOML
  causa real: dynamic dispatch nao rastreavel por static analysis
  acao: NAO CORRIGIR wiring — documentar como limitacao de metric

Issue 4: 133 high-CC files — DISCREPANCIA (scout aca36d)
  finding real (amostra 30 files): apenas 2 files (CC=16,17)
  top offenders: inferlets/lib.rs (CC=17), test_core.py (CC=16)

Issue 5: 294 antipatterns — DISCREPANCIA
  finding real: apenas 3 files na amostra

Issue 6: memory.db schema (accessed_at missing) — PENDING

Issue 7: BUILTIN_HANDLER_COUNT=82 vs 84 — PENDING

---

## SCORING ATUAL vs TARGET

Phase     Current  Target
index     0.883    0.883  (ja bom)
wiring    0.643    0.70+
knowledge 0.467    0.70+
ast       0.575    0.65+
quality   0.900    0.90+
learning  0.343    0.60+

overall   0.688    0.85+

---

## CRITICAL FINDINGS

1. edit_history FIX FUNCIONOU — daemon restartado
2. daemon RESTARTADO COM SUCESSO (PID 1625720)
3. ema_reward=0.0/update_count=0 e ESPERADO em cold start
   — RL loop so atualiza apos tool use real (post-tool-rl hook)
4. Orphan rate e metric invalido — dynamic dispatch nao rastreavel
3. RL LinUCB inactive e o issue mais impactante (peso 0.5 na formula)
4. memory.db schema pode ser simples (adicionar coluna accessed_at)

---

## DAG REVISADO

PARALELO:
  [SCOUT-RL]   Trace post_tool_rl -> LinUCB update chain
  [SCOUT-MEM]  memory.db schema fix

SEQUENCIAL:
  [ARCHITECT] Decidir por issue
  [ENG-1]    Implementar RL fix
  [ENG-2]    memory.db schema
  [ENG-3]    Handler count update
  [AUDIT]    Cross-audit
  [DOC]      Documentacao

---

## PROXIMOS PASSOS

1. Aguardar scout RL (a3e892) completar
2. Executar touring e2e --depth deep para contagem real de CC
3. Verificar memory.db schema
4. Implementar fixes priorizados

---

## METRICAS SUCESSO

edit_history_count >= 10
ema_reward > 0.0
update_count > 0
memory.db sem warnings
cargo test verde
