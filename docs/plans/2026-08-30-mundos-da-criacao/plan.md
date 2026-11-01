---
type: Plan
title: "Mundos da Criação — plano operacional F0+F4"
description: "Execução das duas primeiras fases aprovadas no gate humano de 30/08: F0 Atziluth (Painel de Emanação) e F4 (ciclo de medição formal v0)."
plan_id: 2026-08-30-mundos-da-criacao
tags: [plan, atziluth, cognicao, painel, f0, f4]
timestamp: 2026-08-30T21:05:00-03:00
okf_version: "0.1"
---

# Plano operacional — F0 + F4 (DAG `task_1788133871023751850`)

> Estratégico: [strategy doc](strategy-2026-08-30-mundos-da-criacao.md) ·
> Tático: este arquivo · Operacional: DAG `task_1788133871023570839`
> (f0_1 → f0_2 → f0_3; f4_1 → f4_2 → f0_3; doc_1) — os três níveis nomeados,
> como o gate (c) decidiu.

## Entregas (todas executadas e provadas nesta sessão)

| Subtask | Artefato | Prova |
|---|---|---|
| f4_1 | `~/.claude/skills/loop-engineering/scripts/cognicao_formal.py` — esquema das 7 operações NOMEADAS (Emanação, Telos, Imagem, Fronteira, Cadeia, Custo Invisível, Pronto) + `medir` (antecipacao_ratio) + `registrar` (espelho dos gates humanos) + `padrao` (agregado) | suíte 12 testes OK |
| f4_2 | `scripts/test_cognicao_formal.py` | 12/12, journal em tempdir |
| f0_1 | `scripts/hooks/painel_emanacao.py` — coletas paralelas com timeout (RETOMAR*.md, kpi FAIL/STUB, espelho F4), GUT determinístico, render top-8, journal de emissão (adesão medida do dia 1), kill switch `PAINEL_EMANACAO_DISABLED=1` | smoke real: painel emitido contra o projeto vivo |
| f0_2 | `scripts/hooks/test_painel_emanacao.py` | 11/11 (fail-open, GUT, frontmatter OKF, adesão) |
| f0_3 | registro `SessionStart` no settings.json (append `python3 <path>`, backup `.bak-1788134154`) | releitura confirma; smoke pós-registro |
| doc_1 | este plano + sync client mirror + commits | git log |

## Fontes v0 do Painel (e as excluídas, ditas)

Incluídas: `docs/plans/*/RETOMAR*.md` · `touring kpi -j` (FAIL não-advisory,
STUB com stub_reason a partir da 30.4.27) · espelho F4. Excluídas e por quê:
DAGs do decompose (não há superfície de listagem global com títulos —
`decompose status` é só contagem; candidata à v1) · scout perpétuo (hook
SessionStart próprio; duplicar é ruído).

## O dogfood da estreia (dados reais no journal)

- Registro real da decisão do gate (30/08) gravado: journal + memória
  (`#domain:cognicao-gabriel`).
- **Baseline de antecipação de Gabriel (régua v0 corrigida): 0.714** —
  ausentes: `imagem`, `fronteira`. A régua foi corrigida ANTES da série
  oficial (2 falsos negativos de formulação natural: "com qual finalidade",
  "percebo que podemos"); a correção virou teste.
- Defeito achado pelo smoke e corrigido: frontmatter OKF (`---`) virava
  título de RETOMAR — parser agora exige heading (+2 testes).
- REGRA #21 paga no caminho: 3 falhas pré-existentes em `test_flow_guard.py`
  (fixture plantava ledger `{}` que não satisfaz `verdict.converged: true`
  do manifesto) — fixture corrigido, **68/68**.

## F1-F3 + painel v1 — ENTREGUES (ordem de Gabriel, 30/08, mesma noite)

DAG `task_1788135201845450484` (4/4 completed):

| Fase | Artefato | Prova |
|---|---|---|
| **F1 Briah** | `~/.claude/skills/briah/SKILL.md` — os 2 protocolos (executivo default, profundo com lente junguiana), roteador I0-I3, **fading por construção** (medir antecipação ANTES; o rito cobre só as ausências; espelho invertido em I2+), oferta do profundo (nunca entrada), gate de saída executável (`cognicao_formal medir --arquivo criacao.md` → ratio 1.0) | skill descoberta pelo harness; gate é exit code |
| **F2 Δ-Yetzirah** | `loop_doc_link_gate.py` ganhou `world_rites`: Strategy exige cadeia causal ≥5 elos se→então; Plan exige os 2 outros níveis (strategy-link + DAG). Advisory; blocking sob `--strict` (a escada) | suíte NOVA `test_doc_link_gate.py` 8/8 (o gate nunca teve teste — REGRA #0); dogfood: este bundle CLEAN |
| **F3 Δ-Assiyah** | `loop_phase_close.py --mundo {atziluth,briah,yetzirah,assiyah}` → faceta `#process:<mundo>` na lição + transição no journal cognitivo | ESTE fechamento usou `--mundo assiyah`: `{"kind":"mundo","mundo":"assiyah",…}` no journal |
| **Painel v1** | fonte DAGs (markers do loop → `decompose get`, etapa X/Y + próxima) + **apontador de isomorfismo** (par cross-domínio endereçado no espelho; o apontamento semântico é do turno de abertura) | testes 18/18 painel + 14/14 cognicao; smoke real |

Dogfood F2 na própria estratégia: seção "Cadeia causal" (§5c, 6 elos) no
strategy doc; `index.md` criado (o gate achou o bundle sem índice — 3
broken_links + 5 órfãos, corrigido).
