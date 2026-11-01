---
type: Review
title: "Avaliação do plano complementacao-hooks — recuperação e veredito"
description: "Recuperação do plano original (Briah + strategy v0.1 + specs H1-H15 + kpi.md) e avaliação de objetivos, propósito, coerência, pertinência e validade — com os 4 critérios de pronto re-medidos ao vivo em 01/09/2026"
plan_id: 2026-08-31-complementacao-hooks
content_blake2b: f29b5dae25a7dec2ef67521a1801d951
tags: ["#kind:decision", "#domain:hooks", "#process:review", "#status:delivered"]
timestamp: 2026-09-01T17:11:26.650804-03:00
---

## Recuperação — o plano original

O plano original da Frente 2, reconstruído dos artefatos em disco (13 arquivos no bundle):

- **criacao.md** (Briah, ratio 1.0, 31/08 18:46): estender a cadeia de hooks com **15 handlers novos registrados em `~/.claude/settings.json`** (PreToolUse/PostToolUse/SessionStart), invocando módulos Rust JÁ existentes via subprocess fire-and-forget (padrão `arch:generator-hooks-integration`). Pronto = AND de 4 critérios medidos; 6 anti-goals; pré-mortem nomeado (latência acumulada mata a criação).
- **strategy-v0.1** (reconhecimento): matriz 15 sinais × evento × bridge × budget de latência; fases F1 (specs) → F2 (benchmarks) → F3 (registro settings.json) → F4 (testes + gate Warn).
- **specs/H1-H15.md** (F1): spec por handler com VGP — revelou que 4 sinais (H4 pub_api_diff, H6 scan_vulnerabilities, H11 find_references, H12 entity_id) **não tinham CLI** → F1.5 criou 4 subcmds.
- **kpi.md**: régua `hooks_complement.composite = 0.5×test_pass_rate + 0.5×signal_emit_rate ≥ 0.80 por ≥7 dias`, com escada Warn→fail-closed.
- **strategy-actions-v1** (pós-cross-audit 0.7798 Silver): 7 ações de followup; contém o anti-goal NOVO que documenta o pivot: "NÃO modificar settings.json (**F3 já migrou para SignalLayer Rust direto**)".
- Execução: DAG `task_1788202012024662508` 6/6 completed; cross-audit final CONVERGED composite **0.9281 Platinum** (memória 31/08 23:06, trilha Silver 0.7798 → Gold 0.8984 → Platinum 0.9281).


## Avaliação — objetivos, propósito, coerência, validade

**Objetivos — nota alta.** Mensuráveis por construção (AND de 4 critérios, cada um com comando verificador), fronteira explícita (6 anti-goals), pré-mortem com mecanismo de morte nomeado (latência composta >100ms×15 por tool_use). O Briah é exemplar como forma.

**Propósito — pertinente e empiricamente ancorado.** Nasce de medição, não de opinião: a análise 57-vs-hooks (31/08 15:35) mostrou que 72% dos sinais que o CLI produz nunca chegam ao contexto do LLM; a partição 15 reativos (esta frente) / 27 sob-demanda (code-mode-sinal, frente irmã) é limpa e correta — reativo entra empurrado por evento, sob-demanda entra puxado por SDK. Serve diretamente ao telos TACO ("ambiente assistido por sinais estruturais em cada tool_use").

**Coerência — boa no papel, com 3 fissuras que a execução expôs:**

1. **F1.5 vs anti-goal #1**: o plano proibia "criar touring-hook subcmds novos", e a execução criou 4 subcmds novos (`pub-api`, `scan`, `identity`, find_references). Defensável — o anti-goal protegia o SHIM v11.0 congelado, e os subcmds são do binário `touring` — mas o texto do Briah não fazia essa distinção; a fronteira era ambígua e só a execução a precisou.
2. **Pivot arquitetural não retrofitado**: o mecanismo CENTRAL do plano (15 entradas novas no settings.json) foi abandonado em F3 em favor do **SignalLayer trait Rust direto** (`touring-hooks-shared/src/signal_layer.rs`, 4+ implementadores vivos). Engenharia MELHOR que a planejada — menos subprocess, latência <5ms, enforcement no executor (D8) em vez de config — mas o criacao.md nunca ganhou v0.2, e o critério de pronto #2 (jq lengths 32/22/12) ficou órfão da arquitetura.
3. **Inflação numérica leve**: H10 é alias declarado de H5, e H3/H9 são o mesmo comando com depth diferente — os "15 sinais" são ~13 distintos.

**Validade dos 4 critérios de pronto — medida AO VIVO neste turno:**

| # | Critério original | Estado hoje | Veredito |
|---|---|---|---|
| 1 | 15/15 testes verdes | rodado agora: **15/15 passed** | ✅ VÁLIDO E CUMPRIDO |
| 2 | settings.json lengths 32/22/12 | medido: **13/12/2** | ❌ OBSOLETO (invalidado pelo pivot SignalLayer; nunca emendado) |
| 3 | `touring kpi -j hooks_complement.composite ≥0.80 ≥7d` | campo **ausente** do kpi -j (a régua vive só no gate best_practices + `hooks_complement_journal.rs`); janela de 7 dias nunca começou | ❌ NÃO MEDÍVEL COMO ESCRITO |
| 4 | latência <100ms p95 por handler | benchmark F2: 3 subcmds p95<10ms ✅; mas harness hoje: avg 2.0s, p95 28.8s wall-clock (H14 drift 798ms) e `hook_latency_p95` por handler nunca instrumentado como escrito | ⚠️ PARCIAL/AMBÍGUO |

**Execução vs plano**: substantivamente entregue e ATÉ ALÉM (followup de 7 ações: gate `hooks_complement` no best_practices ✅, journal counter ✅, H7/H12/H15 wirados em session_hooks.rs:650-713 ✅, composite 0.7798→0.9281 Platinum). A adaptação em execução foi de alta qualidade; o débito é DOCUMENTAL, não de engenharia.


## Veredito e recomendações

**VEREDITO: plano PERTINENTE e VÁLIDO no propósito; PARCIALMENTE OBSOLETO na régua.** A criação cumpriu o telos por rota melhor que a planejada (SignalLayer > settings.json sprawl — coerente com a lição institucional "enforcement mora no executor, não no anúncio"). O que resta não é engenharia, é honestidade de régua:

1. **Emendar o Briah para v0.2** (ou registrar obsolescência formal): critério #2 reescrito para o mundo SignalLayer (ex.: "15/15 SignalLayers registrados e emitindo, verificado por teste"); nunca deixar critério morto fingindo vigência — é a mesma lição do judge_attest (cláusula que some tem de bloquear ou ser atestada, jamais sumir em silêncio).
2. **Expor `hooks_complement.composite` no `touring kpi -j`** como o kpi.md especifica — hoje a régua declarada não é consultável pelo comando declarado (KPI: fonte declarada sem resolvedor, gotcha conhecido).
3. **Instrumentar `hook_latency_p95` por handler** (critério #4 como escrito) e só então armar a janela de 7 dias do critério #3.
4. Higiene menor: deduplicar H10/H5 e H3/H9 na contagem (15 → 13 sinais reais) na próxima emenda.

**Relação com a wave em curso (Rota A)**: complementar, sem colisão — a complementacao-hooks empurra sinais para o `additionalContext` (canal LLM); a PostToolUse-wiring alimenta o `sdk_signal_mirror.jsonl` (canal KPI/gate). O item 2 acima é primo-irmão do que a Rota A faz para o code-mode-sinal, e pode entrar como fase futura da mesma esteira se Gabriel quiser.
