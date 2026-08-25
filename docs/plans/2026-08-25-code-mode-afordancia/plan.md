<!-- OKF document -->
---
okf_version: "1.0"
type: Plan
title: "Plano — afordância de code mode em cinco fases"
description: "DAG task_1787656862986274376: reparar o caminho preferido, fazer o nudge entregar o programa, colapsar o executor por escopo e fundir a rajada do turno automaticamente."
plan_id: 2026-08-25-code-mode-afordancia
tags: [code-mode, afordancia, plano, dag]
timestamp: 2026-08-25T11:35:00-03:00
authority: Gabriel Gadea
dag: task_1787656862986274376
---

# Plano — afordância de code mode

Estratégia e fundamentação: [strategy](strategy-2026-08-25-code-mode-afordancia.md).
Aprovação: Gabriel, 25/08/2026, com a emenda *"o nudge de persuasão deve injetar contexto
com snippet que substitua as n+ tool calls"* — que reclassifica T1 de **deleção** para
**conversão**: a injeção deixa de exortar e passa a entregar o programa derivado.

## Critério de aceitação do programa inteiro

Uma afordância só está provada quando o code mode executa numa chamada que eu emiti
**como atômica** — nunca num programa que eu decidi escrever. Esse é o gate de P3, e é o
critério que a crítica do Gabriel estabeleceu em 25/08.

## Fases

### P0 — Reparar o caminho preferido `[em execução]`

Pré-requisito de tudo: enquanto o caminho preferido for pior que o atômico, coagir para
ele é empurrar para um caminho degradado.

| item | mudança | prova |
|---|---|---|
| **T0.1** sandbox mais fraco que o Bash | duas partes: (a) `Sandboxed` concede `Run` para 22 binários de inspeção; (b) `gate.rs` **deriva o binário real** da chamada em vez de pedir `Run(any)` | 5 testes de PONTA A PONTA (pedido→perfil→veredito) + 3 de perfil |
| **T0.2** `--brief` mente | `OutputSummary.elided_lines` novo; `truncated = upstream \|\| elided_lines > 0` | asserção de que `truncated ⟺ elided_lines > 0` sem corte upstream |
| **T0.3** nudge recorre sobre si | `code_mode_kind` devolve `None` quando o comando já contém `touring run`/`touring exec` | 3 testes, incl. o negativo (o laço de shell cru **continua** induzindo) |
| **T0.4** fusão entrega programa truncado | `fuse_burst_program`: comando inteiro ou nenhum, omissão declarada | 5 testes, **provados por mutação** (reintroduzir a truncagem reprova 2) |
| **T0.5** `--description` | pendente — espelha o contrato de dois argumentos do `run_code` | — |

### P1 — SDK uma vez + o nudge entrega o programa

- Seção `SessionStart` com `touring run --lang python --sdk-stub` (1,3 KB, byte-estável).
- **Emenda do Gabriel**: para `CodeModeKind::Loop`, parar de traduzir para Python com o
  placeholder `# then your per-file op over files` e emitir o laço **verbatim** dentro de
  `touring run --lang bash --code '<laço>'` — a forma que `loop_rewrite_candidate` já
  deriva e que substitui as N chamadas de fato. Um snippet que conta arquivos e depois
  diz "agora faça sua operação" não substitui nada.
- Retirar as injeções por chamada que não carregam programa derivado.

### P2 — `TOURING_CODE_MODE` por escopo

`native | code | both`, resolvido **sessão (env) → projeto (`.touring/touring.toml`) →
default `both`**. É o `presentAs` do DeepSeek. Classes calibradas pela medição:
`grep`/`cat`/`find` colapsam; `ls` não (chamada única domina — a recusa deles); mutação e
build jamais.

### P3 — Fusão automática da rajada do turno

Variante **B (first-wins, fold-the-rest)**: a primeira chamada executa intacta, as K−1
irmãs do mesmo turno são negadas com um único programa derivado dos comandos verbatim.
Custo: um round-trip. A variante A (hold-and-fuse, zero round-trip) fica para P5, depois
que a telemetria de B provar a frequência e o acerto da detecção.

### P4 — Medição e correção do registro

A/B com as **três grandezas separadas** (payload, comando, nudge) — as que o benchmark
anterior misturou. Substituir a memória `prova:code-mode-eficiencia-medida:2026-08-25`,
que carrega os 84,8% inflados.

### P5 — Variante A `[condicional]`

Só se P4 justificar.

## Invariantes que atravessam as fases

1. **Nenhum comando parcial jamais é entregue** — inteiro ou ausente, com a omissão declarada.
2. **Nenhum sinal afirma completude que não tem** — `truncated`, `converged`, `omitidos`.
3. **A negação carrega a rota** — a lição do postmortem do DeepSeek: sem ela o modelo
   conclui que o deployment quebrou em vez de se corrigir.
4. **Nenhuma reivindicação de economia antes do número** — nem a fonte reivindica.
5. **Ligar modo é decisão humana** — env do daemon, jamais a LLM.

## Artefatos OUTER desta rodada

- Diagnóstico: [diagnostics/touring-20260825T081347.md](diagnostics/touring-20260825T081347.md)
- Ledger CCE: `.touring-explore/afordância-de-code-mode-no-harness--colapso-do-e.ledger.json` (convergido, 4 rodadas, lente `external` visitada com o clone do DeepSeek)
- Estratégia: [strategy-2026-08-25-code-mode-afordancia.md](strategy-2026-08-25-code-mode-afordancia.md)
