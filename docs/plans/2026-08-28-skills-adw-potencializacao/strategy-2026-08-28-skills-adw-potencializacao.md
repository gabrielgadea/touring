---
plan_id: 2026-08-28-skills-adw-potencializacao
type: Strategy
title: "Potencialização das 5 skills TACO — o modus operandi de cada uma como ADW"
description: "cross-audit, plan-excellence, skill-refine e converge-close viram flows com os scripts layer-3 como nós code; analysis-loop verificada por execução; fix estrutural: agentes headless de ADW desarmam o work-outer"
tags: [adw, skills, code-mode, cross-audit, planning, skilling, loop-engineering, analysis-loop]
timestamp: 2026-08-28T09:50:00-03:00
plan: /log.md
---

# Potencialização das skills via ADW + code mode (28/08/2026)

> Ordem de Gabriel: "aproveite e potencialize as skills /TACO-cross-audit, /TACO-planning,
> TACO-skilling, /loop-engineering e /analysis-loop criando adw e potencializando o code
> mode em todo modus operandis delas". OUTER: strategy-loop + explore CONVERGED
> (12 rodadas, cauda seca, 7 lentes incl. external marcada com as fontes verificadas).
> Prior-art (lente portfolio): gap — nenhum artefato cobre o intento → create_new.

## O diagnóstico

As 5 skills somam **51 scripts layer-3** (4 + 10 + 10 + 19 + 8), mas a adw-library só
carrega o **craft** delas (fragments de 1 nó agent: `audit-pack`, `plan-pack`,
`skilling-pack`) — os **pipelines determinísticos** dos scripts não são nós de flow
nenhum. A tese da adoção estrutural (touring-4-pillars, corolário ADW): "masters são os
nós de código dos ADWs — o runner os chama por construção". O que era manual em cada
skill vira comando.

## Defeito estrutural achado ANTES do plano (já corrigido — REGRA #21)

A estreia do `exercise-idle-infra` reprovou 4× e o post-mortem revelou: **o agente
headless herdava os hooks da sessão e o `loop_outer_arm` armava `work-outer` DENTRO
dele** — o Stop hook do subagente passava a exigir os artefatos do OUTER e a resposta
final virava narrativa do guard, nunca o contrato `FACT=`/`VERDICT=` (propose#0:
"✅ Fluxo work-outer pronto para encerramento"; o agente até criou um bundle próprio).
Fix D8 no executor: `_agent_claude` agora spawna com `TOURING_WORK_OUTER_DISABLED=1`
— um nó de ADW já é gatado pelo gate do PRÓPRIO flow (Lei L2); guard duplo compete
pelo turno. Esse fix potencializa TODOS os agentes de todos os ADWs.
Bônus da rodada: o guard de template-injection pegou `{{vars.*}}` interpolado no
`memory-curation.toml::inventory` (meu) — convertido a posicionais; 241 testes verdes.

## As fases propostas (specs novos + verificação)

| Fase | Skill | Spec/ação | Nós `code` (code mode) | Evidência de saída |
|---|---|---|---|---|
| F1 | TACO-cross-audit | **`cross-audit.toml`** — as 7 fases sobre uma árvore (o `audit.toml` atual audita 1 arquivo) | `harmony_map.py` (MAP+HARMONY), `scan_debt.py` (DEBT), `prove_invariants.py` (E2E PROOF), todos sandbox | report `docs/audits/cross-audit-<date>.md` (o artefato que o flow guard da skill já exige) + estreia real |
| F2 | taco-planning | **`plan-excellence.toml`** — os 4 stages como pipeline | `ground_truth_collector.py` → agente planner (craft `taco-planning`) → `dimension_scorer.py` + `gap_detector.py` + `plan_validator.py` como gate fail-closed | plano Pln2 real validado por exit 0 + estreia |
| F3 | TACO-skilling | **`skill-refine.toml`** — o loop REFINE por evidência | `mine_transcripts.py` (mine) → agente diagnose+propose (craft `TACO-skilling`) → `quality_gate.py` (gate) | veredito por skill + diff proposto; humano aplica (regra da skill: nunca blind) |
| F4 | loop-engineering | **`converge-close.toml`** — o CLOSE (16-17) + convergência como comando | `judge_attest.py` → `loop_converged.py` → `loop_doc_link_gate.py`, encadeados fail-closed | exit codes reais dos 3 juízes num alvo vivo |
| F5 | analysis-loop | **verificação por execução** do `profile_to_adw.py` (a skill já é ADW-nativa): gera spec, `adw lint`, e o gerador aplica `sandbox = true` nos nós readonly? Se não, corrigir o GERADOR | lint 0 erros no spec gerado; sandbox nos leitores | spec gerado lint-clean; correção no gerador se o convite `readonly_sem_sandbox` acusar |
| F6 | — | docs + sync + commit + relatório | — | CLAUDE.md nota, sync espelho, commit, strategy doc linkado |

Padrões obrigatórios em todo spec novo (lições das estreias de 28/08): posicionais
(nunca `{{vars}}` no script), gates lendo `full_ref` com fallback ao summary, feedback
do gate no prompt do retry, `retries = 1` de transporte, `[purpose]` completo com
`when_not_to_use`, `prior_art_verdict` registrado, `[[use]] phase-close`, sandbox nos
leitores, estreia real antes de promover.

## Resultado das estreias (fechado 28/08, tarde)

| Spec | Estreia | Produto real |
|---|---|---|
| `cross-audit` | **completed**, gate de primeira | `docs/audits/cross-audit-scripts-2026-08-28.md` — 9 FACT=, 2 desvios reais achados (script gravando na árvore congelada; duplicata sem symlink) |
| `plan-excellence` | **completed** — author convergiu na 3ª guiado pelos gaps verbatim | `plan-xaudit-gates/plan.md` (Pln2 do residual xaudit-gates, 0 gaps P0, validator OK) |
| `skill-refine` | **completed**, gate de primeira | 1 lição minerada e roteada: bug de instrumento do `mine_transcripts` (conta eco do SKILL.md como correção) → memória `mine-transcripts-conta-eco-do-skill` |
| `exercise-idle-infra` | **completed** (3ª estreia; as 2 falhas ensinaram os fixes) | 7 exercícios com comando verificado, classificados safe/colateral |
| `converge-close` | estreia = o gate de fechamento desta própria wave (dogfooding) | veredito por exit code |

Todos promovidos com evidência comportamental (`promotions.json`).

## As 3 lições de executor que as estreias pagaram (e o que as institucionaliza)

1. **Agente headless capturado pelo flow guard da sessão** — `_agent_claude` spawna
   com `TOURING_WORK_OUTER_DISABLED=1` (um nó de ADW já é gatado pelo próprio flow).
2. **Gate mudo degrada o retry** (13→6→0 FACT medidos) — gates FALANTES: a REASON
   ensina a correção (A5).
3. **Texto de agente nunca entra no sandbox** — o X6 classifica o programa inteiro
   (posicionais inclusos) e prosa vira deny não-determinístico. Lint novo
   `sandboxed_gate_reads_agent_text` (com precedência sobre o convite dual
   `readonly_sem_sandbox`), 2 fragments corrigidos (worker-critic-pair,
   critic-panel), e o guard da library nomeia os 4 escritores deliberados
   (os placeholders `<x>` em REASON enganavam o detector de redirect).

## Residual consciente

- Os specs de análise POR PERFIL seguem no projeto `analise` (domínio ANTT); aqui só
  o gerador e a forma foram exercitados (lint 0/0, veredito F5).
- Desvios achados pelo cross-audit (generate_context_mode_plan.py → árvore congelada;
  disk-watch/safe-clean duplicados) e os 7 exercícios do exercise-idle-infra são
  PROPOSTAS para a próxima curadoria de Gabriel.
