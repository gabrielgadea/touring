---
type: Log
title: "Log — chronological history of this loop run"
description: "Append-only history; PreCompact resume notes and phase closes land here."
plan_id: 2026-09-23-jev-touring-otimizacao
okf_version: 0.1
tags: [loop, log]
timestamp: 2026-09-23T18:57:10.120468-03:00
---

# Log

Part of the [bundle](/index.md).

## 2026-09-23T19:54:57.994914-03:00 — N1 done

N1 entregue: KPI flow.compliance recalibrado — started-conditional (complete OR present_ids), janela 14d, per-flow (work-outer/strategy-outer/cross-audit) + nova régua flow.arm_yield (started/armed). Dados: 1629 registros; strategy-outer 14d = 3.9% (devedor), work-outer 100%, cross-audit 100%. Implementação: flow_compliance_breakdown + 3 fontes novas em kpi.rs/sources.rs + 5 checks em commitments.yaml. Testes: suíte flow_compliance reescrita (RED E0425 confirmado → GREEN), 55/55 kpi, cross-check YAML↔arms 2/2, clippy limpo. Efeito live após rebuild do daemon (deploy no fim da wave).

## 2026-09-23T19:55:09.318961-03:00 — N1 done

N1 entregue: KPI flow.compliance recalibrado (started-conditional, 14d, per-flow + arm_yield). 55/55 testes kpi, cross-check 2/2, clippy limpo. Efeito live após rebuild.

## 2026-09-23T20:21:35.786211-03:00 — N2 done

N2 entregue com a hipótese INVERTIDA pela data: corte de lente seca economizaria 1 rodada em 174 ledgers (morta); open_qs>0 em 0/63 ledgers longos. O defeito real era o KPI: explore_rounds_to_dry media rounds.len() (vida do ledger, re-explorações) — mean 8.09 vs 2.33 por episódio (p90 6). Fix: last_episode_len (cauda após último par seco) + fallback lifetime fail-closed + YAML threshold 8→6 com rationale nova + lens_yield_totals no payload do explore (findings/new_this_run por lente). Testes: suíte kpi 56/56 (RED→GREEN), explore 19/19 (RED→GREEN). Deploy via rebuild no fim da wave.

## 2026-09-23T20:23:55.697217-03:00 — N3 done

N3 entregue — causa raiz DUPLA: (1) plan_refine.py só era invocado pelo explore-plan, e (2) esse único chamador passava --topic, arg que o script nunca aceitou (argparse exit 2 mascarado por || true) → no disco só 1 ledger com 1 iteração. Fixes: plan_refine.py aceita --topic/--scope derivando o ledger pelo MESMO ledger_path_for do explorador (teste RED→GREEN, 15/15); refine_plan do explore-plan corrigido (positional plan + skip visível + input plan); strategy-loop ganha nó refine_strategy (1 iteração por re-run quando a strategy doc existe) — prova viva: nó executou, ledger criado, iteração 2 = platô. KPI plan_refine_iters 1.0 ADVISORY → 2.0 PASS LIVE. Instância .touring/adw sincronizada (from-template não sobrescreve; diff só comentários+nó novo).

## 2026-09-23T20:34:02.804783-03:00 — N4 done

N4 entregue: (a) memory suggest-links ganha o 1º chamador (apply_derived_links no loop_phase_close — engine bounded top-3 por campos ESTRUTURADOS, nunca a string apply; + aresta chain determinística extends p/ lição da fase anterior, que a engine sozinha nunca criaria por falta de co-serviço da chave nova); (b) resume-uptake ruler: resume_ledger.py (append fail-open + uptake same-session/later-action/window 72h + Lei L2 no vazio), loop_resume.py grava injeções + modo --uptake, phase_close grava loop-action. Testes: 2 arquivos novos RED→GREEN; suítes scripts/ 251 + hooks/ 138 sem regressão.

## 2026-09-23T20:53:27.090521-03:00 — N5 done

N5 entregue: [purpose] class (fix|create|review) nas 23 specs da library (8 fix / 5 create / 10 review, derivado dos intents; tiers.toml é config) + 5 flows locais (review) + VALID_PURPOSE_CLASSES no adw.py com lint (ausente=warning aimed, inválido=error nomeando as 3) + --purpose-class no adw new (scaffold omite quando ausente; teste canônico passa fix). Três vermelhos pré-existentes corrigidos (REGRA #21): guard de escritores (cross-audit:report opaco ao detector tri-valorado — asserção is-not-False), curation label do strategy-loop (generic), mirror client/ dessincronizado (sync --apply). Gates: 3 testes novos RED→GREEN, suíte test_adw.py 247/247, lint sweep 25 specs 0 falhas, instâncias .touring/adw sincronizadas.

## 2026-09-23T21:24:30.463540-03:00 — N6 done

N6 entregue nas 4 frentes: (1) journal ganha campo hint (top match da escada por run, no funil ctx_execute_impl — CLI+MCP; computado ANTES do move de code, no código do USUÁRIO); (2) KPI code_mode_reuse ganha hinted_runs/hint_coverage — o denominador que separa DESCOBERTA de AUSÊNCIA (reuse baixo × cobertura alta = oferta ignorada); (3) os 3 denies de rajada (G1, G10, burst T3-B) entregam o snippet trusted similar por campos estruturados + 3 testes (memória in-memory; silent sem match/db); (4) harvest_candidates no phase_close (provisional ≥3 exec @≥90%, comando exato; 3 testes). Medida de hoje que o desenho ataca: ladder 390 enrolled × 10 reused, --harvest 0 usos na vida. Gates: 784 code + 576 cli + 1636 server + journal_v2/reuse/snippet_hint/harvest suites, clippy limpo nos 3 crates.
