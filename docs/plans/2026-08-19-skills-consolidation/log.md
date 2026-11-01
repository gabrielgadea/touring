---
type: Log
title: Log — chronological history of this loop run
description: Append-only history; PreCompact resume notes and phase closes land here.
plan_id: 2026-08-19-skills-consolidation
tags: [loop, log]
timestamp: 2026-08-19T23:51:56.995018-03:00
okf_version: "0.1"
---

# Log

Part of the [bundle](/index.md).

## 2026-08-20T00:05:35.731825-03:00 — F2 done

F2 divergencia constitucional TACO-subagent: skill v6.0 -> v6.3, REGRA #15 symbol_verification exigida (era zero), binding ao reference canonico como fonte de verdade, regra de permissoes propagada do CLAUDE.md. 3 copias removidas (tabela + catalogo CLI) por composicao. 515L -> 490L dentro do limite REGRA #13. Instrumentos: skill_overlap.py + skill_pin_gate.py, ambos provados por mutacao; 28 skills pinadas; extrator estendido a references de skills apos lacuna encontrada.

## 2026-08-20T00:23:07.967974-03:00 — F3 done

F3 gates de co-evolucao: skill_pin_gate.py estendido (deteccao de comandos fantasma com distincao uso-vs-negacao; --claude-root para rodar no CI; fonte ausente != deletada). Registrado no ci.yml. Achado: 5 comandos ensinados que nao existem em 3 skills (touring-scip inteira, touring-evolve, touring-generator); 2 do master corrigidos (eram hooks). Falso positivo evitado: 5 skills AVISAM sobre touring quality, nao o ensinam.

## 2026-08-20T00:44:01.946723-03:00 — F5 done

F5 medicao de proposito: skill_purpose.py (BM25 sobre descriptions, gatilhos declarados como consulta; calibrado com controle positivo e negativo). Resultado: apenas 2 pares colidem de proposito em 25 skills (plan-protocol<->plan-execution margem 10%; touring-evolve<->touring-generator margem 25%). Nenhuma micro-skill colide. Achado maior: 57 de 76 triggers declarados (75%) sao INERTES - ausentes da description, que e o unico texto lido na escolha. plan-protocol: 21 de 26.

## 2026-08-20T00:49:56.167312-03:00 — F6 done

F6 colisao plan-protocol<->plan-execution eliminada. Descoberta material: plan-protocol e symlink para ~/projects/analise/.claude/skills (102 das 180 skills sao symlinks; 5 apontam para o analise). Removido o symlink, nao a skill: alvo intacto com 4 arquivos. Medido antes/depois: gatilhos inertes 13->3, pares em colisao 2->1. Efeito colateral proprio corrigido: pins.json que eu escrevera dentro do projeto analise foi removido.

## 2026-08-20T00:53:04.128108-03:00 — F7 done

F7 remocao de touring-evolve e touring-generator (decisao do Gabriel). Antes de apagar, 18 comandos VALIDOS que so elas documentavam (8 generate + 10 evolution) foram extraidos por script para Touring/references/generate-and-evolution.md, com ponteiro no master — REGRA #0: a skill sumiu, a informacao nao. Espelho podado (296 arquivos). Resultado medido: colisoes de proposito 1 -> ZERO; corpus 180 -> 177 skills. Restam 2 comandos fantasma em touring-scip e 3 gatilhos inertes.

## 2026-08-20T00:59:32.363374-03:00 — F8 done

F8 remocao de touring-scip. Correcao de afirmacao propria: eu dissera que scip_emit era orfao (refs=0) — consultei o MODULO, nao os SIMBOLOS; ScipEmitter e ScipDocument tem 1 consumidor cada e 14,6KB de codigo vivo. A API Rust e real; so a CLI (touring scip emit / touring symbol index) nao existe, e a skill era a unica fonte deles no corpus — a sugestao ja escapara para o enriquecimento de prompt. Preservado em Touring/references/scip-export.md com o que existe, o que nao existe e a alternativa real (touring query). Gate de pins: exit 0, zero fantasmas. Corpus 180 -> 176 skills.

## 2026-08-20T01:10:11.659343-03:00 — F9 done

F9 gatilhos inertes removidos: 3 de TACO-subagent (touring-orchestrator, decompor em DAG, spawning subagents) + 3 de subagent-driven-development (no projeto analise, via symlink, com consentimento). Corpus agora com ZERO gatilhos inertes. Correcao de instrumento: meu detector contava tags/metadata/allowed-tools como gatilhos (regex de item de lista em vez de parser YAML) — os '13' reportados eram 6; memoria corrigida. Read/Grep/Glob/Bash estavam em allowed-tools, declaracao legitima. Description truncada de subagent-driven-development e defeito pre-existente, nao causado pela edicao.

## 2026-08-20T01:23:21.141725-03:00 — PreCompact snapshot

Loop active. Pending: [W1,W2,W3,W4,W5]. Resume: `touring decompose ready task_1787199696728443105`.

## 2026-08-20T01:32:47.077384-03:00 — W1 done

Reference canonico dos 5 principios: skills/Touring/references/skill-operating-principles.md (154L). P1 compor-nao-copiar (13 fragments medidos, nao os 11 da constituicao), P2 convergencia medida (8 clausulas lidas por AST + a licao do timeout 20s<24s), P3 gauntlet cego (quorum contado por codigo), P4 grafo plano (adw explain + when_not_to_use + on_branch_fail), P5 Wayfinder (claim/ticket/frontier). Todo comando citado foi executado antes de ser escrito. Regra anti-banner: instancia derivada por skill, nunca texto identico.

## 2026-08-20T01:34:08.334399-03:00 — W2 done

Wayfinder instanciado em 5 skills que decompoem/despacham trabalho, cada uma com instancia DERIVADA distinta: loop-engineering (INNER next: ready->claim --owner), TACO-subagent (claim antes de spawnar engineer na FASE 5), subagent-driven-development (claim antes do dispatch, lease devolve se o subagent morre), TACO-skilling (claim no design do DAG de skill), taco-planning (ticket --kind/--fog na autoria + frontier separando decisoes abertas). Medido: so 3 skills citavam decompose; nenhuma citava claim.

## 2026-08-20T01:35:01.914555-03:00 — W3 done

P1-P4 instanciados onde cada um pertence, texto distinto por skill: TACO-cross-audit (P3 quinta forma de errar veredito = auditor julgando o proprio trabalho -> critic-panel com lentes distintas e quorum contado por codigo; P2 veredito e exit code do loop_converged, judge_intact primeiro), subagent-driven-development (P3 review so vale se o revisor for cego -> worker-critic-pair), TACO-skilling (P1 se o procedimento ja e fragment, cite; prosa que restata e segunda implementacao), taco-planning (P4 DAG Mermaid e o grafo plano; fase parallel declara on_branch_fail; when_not_to_use permite DESCARTAR).

## 2026-08-20T01:38:21.272278-03:00 — W4 done

skill_principle_gate.py: papel implica obrigacao. Populacao MEDIDA (25 skills que invocam o CLI), nunca lista chumbada. P5 por fato MECANICO no corpo (chama a API do DAG ou despacha worker -> exige claim); P3 por fato de PROPOSITO na description + julgamento DELEGADO no corpo (scorer deterministico ja satisfaz P3 estruturalmente). Primeira versao casava vocabulario: 10 achados, 7 falsos (skill de redacao de documentos acusada pela palavra cross-audit). Por sinal: 3 reais, todos corrigidos (TACO-skilling comando abreviado, TACO-subagent auditor nao-cego na FASE 6, lexcore-execute spawn sem claim). Provado por mutacao (claim->READ_ONLY = exit 1). Wired no CI contra client/skills.

## 2026-08-20T01:38:45.556218-03:00 — W5 done

Verificacao: pin gate 176 skills nenhuma divergiu; purpose 0 colisoes (contestados sao pares gws/remotion de terceiros, pre-existentes); overlap NENHUM par das skills editadas acima do limiar -> as 9 instancias derivadas nao viraram banner, medido e nao afirmado; sync client CLEAN 297 arquivos; principle gate exit 0 (25 skills vivas, 14 espelhadas no CI). Ressalva honesta: lexcore-execute nao e espelhada em client/, entao o gate do CI nao a cobre - cobertura viva 25, CI 14.

## 2026-08-20T01:49:33.381014-03:00 — W10 done

5 skills atomicas complementadas com instancia P1/P2 distinta (search->scout, query->map+Code Mode, file-metadata->read/blast, wiring-suggest->clausula orphans_base, token-efficient->touring run elimina as chamadas em vez de baratea-las). VGP achou fantasma real: touring search symbols/docs nao existem. Isso expos defeito no pin gate: ghost_commands comparava so match.group(1), o primeiro nivel - o regex ja capturava group(2) e ninguem usava. Corrigido com live_subcommands() memoizado (clap Commands: + argparse choices), fail-open quando o help nao lista subcomandos. Achou 4 fantasmas de 2o nivel em 3 skills, inclusive na master. touring-search removida a pedido do Gabriel, sintaxe real preservada em Touring/SKILL.md.

## 2026-08-20T01:51:04.705684-03:00 — W6 done

Touring master: indexa o reference canonico e declara os masters como P1 aplicado a descoberta (um comando funde as N buscas atomicas). Corrigido tambem o fantasma touring search symbols na L276 -> 4 modos reais (unified/exact/bm25/tools), preservando a sintaxe da touring-search removida.

## 2026-08-20T01:51:04.776782-03:00 — W7 done

loop-engineering: e a ENCARNACAO do P2, nao uma instancia derivada - quando outra skill diz 'o veredito e um exit code', e este exit code. Ligada ao canonico.

## 2026-08-20T01:51:04.851976-03:00 — W8 done

TACO-wt: P5 (forensic_runner paraleliza dentro de UMA sessao; entre sessoes nao ha exclusao, dois 'validate W12' concorrentes rodam a mesma wave -> claim --owner wt-<wave>) + P2 (os gates da wave sao clausulas locais; o veredito do plano e loop_converged exit 0).

## 2026-08-20T01:51:04.925713-03:00 — W9 done

touring-elite: e um GRADER, e graders moram na arvore que o julgado edita -> judge_intact e a primeira clausula; clausula que some bloqueia, grader que mudou fala sem bloquear; mudanca deliberada e judge_attest --attest --why. Origem arXiv:2505.22954 Ap.H.

## 2026-08-20T01:51:05.009382-03:00 — W11 done

touring-excellence removida a pedido do Gabriel. Medicao antes: sem frontmatter (o runtime caia no H1 como description, sem vocabulario de gatilho -> irrotavel), pinada em v28.13.0 (atual 30.4.13), sem slash command correspondente em commands/, e ensinava 'touring graph dependencies' (fantasma). Nada unico perdido: mcts search e workspace-info ja documentados no touring-cli-index e nas references.

## 2026-08-20T01:52:39.042902-03:00 — W12 done

Verificacao: principle gate 23 skills no escopo todas conformes (vivo e CI); purpose 0 colisoes 0 gatilhos inertes; overlap NENHUM par da familia acima do limiar (as instancias sao derivadas, nao banner); pin gate 175 skills 0 fantasmas 0 drift; sync CLEAN 293. Escrita a suite que faltava: test_skill_gates.py, 18 testes cobrindo os DOIS defeitos que ja tinham passado despercebidos (ghost de 2o nivel; acusacao por vocabulario), o principal provado por mutacao. Wired no CI.
