---
type: Log
title: "Log — chronological history of this loop run"
description: "Append-only history; PreCompact resume notes and phase closes land here."
plan_id: 2026-09-22-jev-system-one
okf_version: 0.1
tags: [loop, log]
timestamp: 2026-09-22T08:22:36.287640-03:00
---

# Log

Part of the [bundle](/index.md).

- **2026-09-22T08:12-03:00** — OUTER armado (`strategy-outer`) por `/loop-engineering`. Recall do tema: 0 resultados (tema inédito).
- **2026-09-22T08:14** — Fonte primária baixada: `llms-full.txt` (903 KB, 110 páginas), blog de 15/09/2026, página de evals e repositórios da org `typesafe-ai`. Dois agentes em paralelo: cookbooks e ecossistema/recepção.
- **2026-09-22T08:17** — Context7: `/websites/typesafe_ai` (2 consultas), `/qew7/jev-feels`, `/browser-use/jev-ultrafast`; descobertos SDK .NET e clientes Ruby comunitários.
- **2026-09-22T08:20** — Pontos de decisão do Touring lidos no código: `factory.py` (6 regras regex em inglês + `claude -p --model haiku`, 120 s), `loop_outer_arm.is_default_work`, `tags.rs::derive_tags`, 23 specs ADW com `[purpose]`.
- **2026-09-22T08:22** — `strategy-loop` rodado à mão: diagnóstico OKF + ledger CCE convergido (11 achados, 5 rodadas; lente externa marcada como visitada). Achado colateral: o hook OUTER anuncia artefatos que não disparou quando o flow é `strategy-outer`.
- **2026-09-22T08:40** — [Estratégia](/strategy-2026-09-22-jev-system-one.md) escrita; memória `research:jev-system-one:2026-09-22` gravada.
- **2026-09-22T08:55** — [Digest dos cookbooks](/research/digest-cookbooks.md) integrado; números-chave conferidos na fonte.
- **2026-09-22T08:58** — DAG `task_1790076481161575147` (R1–R4) registrado; implementação (F1–F4) fica fora do DAG até a decisão do Gabriel.
- **2026-09-22T09:20** — [Digest do ecossistema](/research/digest-ecossistema.md) integrado (blog e FAQ literais, evals completos, org GitHub, clientes, recepção). Dados do site de evals conferidos no HTML bruto. Estratégia atualizada: §1 com números, §4.4 práticas 23–26, §6.4 contrato `/v1/systemone` com servidor local, §7 R8–R9, §8 F1 com `system-one-adapter`, §9.2.

## 2026-09-22T08:34:46.764915-03:00 — R4 done

Bundle da pesquisa Jev fechado: estratégia (veredito, ficha técnica, 9 limites, 26 boas práticas, predicado de encaixe, candidatos C1-C6 ancorados em código, anti-usos, arquitetura, riscos R1-R9, plano F0-F4, cadeia causal), digests de 18 cookbooks e do ecossistema. Evals da TypeSafe conferidos no HTML (Jev 67,8%/US$0,0004/0,4s vs haiku 4.5 53,6%). Doc-link gate ok (6 docs, 0 links quebrados). Implementação aguarda decisão do Gabriel (chave, dados, aprovar F1).
- **2026-09-22T14:10** — Rodada 2 aberta (DAG `task_1790097198101133178`). Fonte nova: `laya-mlx` → Laya (Convai, Apache-2.0). Dois agentes: Laya/local e lacunas do Jev.
- **2026-09-22T14:15** — Experimento local: venv isolado no scratchpad, `laya` 0.3.5 + torch 2.14 cu130 na RTX 4060. Latência 6–15 ms/pergunta; zero-shot fraco nos dados do Touring. [Relatório](/experiments/laya-local/report.md).
- **2026-09-22T14:20** — Varredura: JSON forçado a LLM só em `factory.py`; factory com 9 roteamentos na vida; `prompt_enhance.rs` classifica todo prompt por palavra-chave (pesquisa profunda → GENERAL); ~2.553 prompts humanos em 667 transcripts.
- **2026-09-22T14:35** — [Digest Laya/local](/research/digest-laya-local.md): laya-mlx só em macOS; MLX tem CUDA em Linux mas o port não usa; pt-BR nunca medido; issues #126/#131/#156; RTT até a AWS us-west-2 ~210–230 ms.
- **2026-09-22T15:05** — [Digest Jev r2](/research/digest-jev-r2.md): subprocessadores só nos EUA; DPA sem LGPD; MCA de 19/09 com licença perpétua para Telemetry (conferido no texto atual), JAMS em SF, teto US$ 50; ZDR via Vercel/OpenRouter com contexto de 32k.
- **2026-09-22T15:15** — [Estratégia v2](/strategy-2026-09-22-jev-system-one.md): §1b, ordem P1–P7, professor → aluno, R10–R15, plano G0–G5, cadeia causal 8–12, achados colaterais 3–6.

## 2026-09-22T14:48:43.886048-03:00 — S5 done

Rodada 2 fechada: Laya/laya-mlx pesquisado (laya-mlx só macOS; rota PyTorch na RTX 4060), experimento local com dados do Touring (6-15 ms/pergunta; zero-shot fraco: tipo de memória 0,50, roteamento 0,36 < maioria 0,58, relevância 0/2, intent real 42%), varredura (JSON forçado só no factory; factory 9 roteamentos; prompt-enhance por palavra-chave em todo prompt), contrato do Jev (subprocessadores nos EUA, DPA sem LGPD, MCA 19/09 com licença perpétua p/ Telemetry, JAMS, teto US$ 50). Estratégia v2: ordem P1-P7, professor→aluno, R10-R15, plano G0-G5. Página republicada (v2).
- **2026-09-22T21:20** — **G0 implementado** (aprovado por Gabriel): `AUTOMATED_PAYLOAD_PREFIXES` + `is_automated_payload` em `prompt_enhance.rs`, consultados por `compose_json` e `run_user_prompt_submit`. TDD: teste primeiro (falha de compilação), implementação, prova por mutação (3 testes morrem), suíte do crate 434 verde, clippy limpo.
- **2026-09-22T21:45** — **G1 executado**: corpus (800 prompts, 486 memórias, 300 pares) + 1.586 rótulos do professor (0 lotes falhos) + folha de revisão de 158 itens. Achados: recall com 21,3% de úteis; 20% do corpus "humano" era máquina; concordância do prefixo de chave 65,4%.
- **2026-09-22T21:50** — Lista do G0 estendida de 8 para 14 prefixos (medida no corpus). Versão 30.4.64 propagada: toolchain default e os 3 projetos. **G0 verificado ao vivo**: payload de máquina → `skipped=automated_payload`, sem intent; prompt humano → DEBUG com contexto.
- **2026-09-22T21:55** — Aberto: a prova comportamental do code mode falha 2/40 (gate de rajada), reproduz com o binário anterior — pré-existente, precisa de front próprio.

## 2026-09-22T21:48:47.988676-03:00 — G0 done

G0 entregue e vivo: filtro determinístico de payload automático no prompt-enhance (14 prefixos medidos no corpus do G1, 6 testes novos incluindo guard D8 cruzado, prova por mutação com 3 testes mortos, suíte do crate 434 verde, clippy limpo), propagado como 30.4.64 para a toolchain default e os 3 projetos, verificado ao vivo (payload de máquina -> skipped sem intent; prompt humano -> DEBUG com contexto). Aberto e registrado: a prova comportamental do code mode falha 2/40 no gate de rajada, pré-existente (reproduz com 30.4.62).
- **2026-09-22T22:15** — Falha aberta diagnosticada até a causa: a prova reprovava 2/40 porque o **daemon herdou `TOURING_CODE_MODE=native`** do shell que o reiniciou (meu próprio comando), desligando os gates de code mode na máquina. Sinal que eu deveria ter lido antes: contadores `g1_inspect_*` em zero. Com o daemon reiniciado com ambiente limpo: **40/40**.
- **2026-09-22T22:40** — Correção da causa raiz nos **dois** launchers: `daemon_spawn::PER_COMMAND_RELAXATIONS` (Rust) e `env -u` em `launch_daemon_and_wait` (`update-touring`), com as listas cruzadas por `test_update_touring.py` e porta deliberada `TOURING_DAEMON_CODE_MODE`. Guard cruzado prova que as duas rotas limpam (mutação mata), 543 testes do crate verdes, 26 guards do script verdes, shellcheck limpo. CLAUDE.md item 30 documenta os três contratos.

## 2026-09-22T22:26:36.848241-03:00 — G0b done

Causa raiz da prova 38/40 encontrada e corrigida: o daemon herdava TOURING_CODE_MODE=native do shell que o reiniciou, desligando os gates de code mode da máquina inteira (doctor verde, contadores em zero). Correção nos DOIS launchers (daemon_spawn::PER_COMMAND_RELAXATIONS no Rust; env -u em launch_daemon_and_wait do update-touring), listas cruzadas por test_update_touring.py, porta deliberada TOURING_DAEMON_CODE_MODE, guard cruzado provando que as duas rotas limpam (mutação mata). Somado: o hook OUTER (loop_outer_arm) deixou de anunciar artefatos que não disparou. Propagado como 30.4.65 nos 3 projetos, com a prova comportamental 40/40 dentro do próprio pipeline.
