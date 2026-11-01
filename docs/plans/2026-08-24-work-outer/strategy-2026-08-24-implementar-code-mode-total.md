---
okf_version: "1.0"
type: Strategy
title: "Implementar o plano Code Mode Total (W0-W8)"
description: "Estratégia do turno: executar o plano aprovado docs/plans/2026-08-24-code-mode-antecipa/plan.md wave a wave, com prova viva por deploy e convergência por exit code"
tags: [strategy, code-mode, execucao]
timestamp: 2026-08-24T22:30:00-03:00
plan_id: 2026-08-24-work-outer
---

# Estratégia — implementar o Code Mode Total

A estratégia deste turno **não é re-derivada**: é o plano Pln2 aprovado por
Gabriel em [`../2026-08-24-code-mode-antecipa/plan.md`](../2026-08-24-code-mode-antecipa/plan.md)
(9 waves / 38 subtasks, DAG `task_1787614930715946363`), executado sob o
protocolo INNER do loop-engineering:

1. **Ordem do DAG**: W0 → (W1 ∥ W4) → W2 → W3; (W4,W1) → W5 → W6; (W1,W0) → W8; → W7.
2. **Regra de deploy**: cada wave que toca o hook Rust termina com
   `update-touring` + prova por COMPORTAMENTO novo (nunca rótulo).
3. **Regra de exercício**: toda afordância nova é exercitada num caso mínimo
   real antes de ligada em massa.
4. **Fechamento por fase**: `loop_phase_close.py --facts` (claim-ledger) +
   relatório OKF no bundle do plano.
5. **Convergência**: `loop_converged.py` exit 0 é o único "pronto" (Lei L2).

Evidência do OUTER deste turno: diagnóstico em `diagnostics/` e ledger CCE em
`.touring-explore/implementar-o-plano-code-mode-total-w0-w8.ledger.json`.
Relatórios por fase: [`../2026-08-24-code-mode-antecipa/phases/`](../2026-08-24-code-mode-antecipa/phases/).
