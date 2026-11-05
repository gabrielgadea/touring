---
okf_version: "1.0"
type: Strategy
title: "Estratégia — D07 riscos aceitos no F2.5"
description: "Filtro temporal de achados OSV com evidência e compatibilidade retroativa."
plan_id: 2026-11-05-d07-risco-aceito
timestamp: "2026-11-05T14:45:00-03:00"
links:
  - /RETOMADA.md
---

# Estratégia — D07 riscos aceitos no F2.5

## Desenho

Manter `score_pip_audit(stdout)` como wrapper sem riscos para preservar os testes e o comportamento atuais. Acrescentar uma rota que recebe riscos e data, usada por `f2_5` tanto na cache quanto no live audit.

O parser lê apenas `[[aceito]]` com `id`, `aliases` e `expira`; `controle` explica o aceite e `descricao` é fallback de evidência. Entradas `[[em_onda]]`, malformadas ou sem data não desbloqueiam nada. O caminho é `python_root/scripts/utils/gate_risco_aceito.toml`.

O filtro particiona ids por correspondência exata e vigência. O score usa somente ids abertos; a evidência separa achados aceitos vigentes de aceites vencidos. A data vem de `AuditConfig.now_secs`, nunca de uma constante de teste ou do relógio escondido no filtro.

## Limites

Sem nova dependência, alteração de daemon/toolchain, refresh OSV forçado ou mudança no backend RustSec. O aceite cobre o backend Python e o caminho real do `analise`.
