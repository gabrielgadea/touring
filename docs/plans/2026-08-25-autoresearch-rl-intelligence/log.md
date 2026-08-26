---
type: Log
title: Log — chronological history of this loop run
description: Append-only history; PreCompact resume notes and phase closes land here.
plan_id: 2026-08-25-autoresearch-rl-intelligence
tags: [loop, log]
timestamp: 2026-08-25T18:19:16.009469-03:00
okf_version: "0.1"
---

# Log

Part of the [bundle](/index.md).

## 2026-08-25T22:00 — congelamento de sessão

OUTER completo (diagnóstico + ledger CCE convergido + strategy doc + memória semantic). Estratégia apresentada ao Gabriel; sessão encerrada antes da decisão do HUMAN GATE. Estado de retomada em [RETOMAR-AQUI.md](/RETOMAR-AQUI.md). Baselines KPI registradas lá.

## 2026-08-25T20:48:41.486269-03:00 — P0 done

P0 SINAL fechado. Achado que mudou a fase: o laco de credito ja existia inteiro (cli_memory_credit, Memento Eq.9, ledger, CLI documentada) com ZERO chamadores — ledger_credited_total=0 na vida do daemon. Entregue: (a) Ledger::drain_pending() + touring memory credit --all-pending (quem sabe o VEREDITO credita sem lembrar as queries) + loop_phase_close cai nesse fallback sozinho; (b) adw.py deposit_run_outcome() em execute() fora do lock — reward por run (adw:run:<spec>) + credito das memorias, para TODO caminho de run (antes: 1 sitio, so campaign, em 3.823 linhas). NAO precisou codigo: (c) router_accuracy ja implementado (le .touring/factory/stats.json; STUB e ausencia honesta — o factory nunca rodou aqui); (d) canal de feedback do gate JA existe (W4 S-4.6, com lint) — memoria adw-retry-sem-feedback-do-gate corrigida para RESOLVIDO. Medido: outcome_reward 147/8641=1,70% -> 203/8684=2,34% (+38% relativo). Gates: 3.450 testes verdes (1924 intelligence+cli, 1526 server), clippy exit 0, 5 guards novos do credit_recalls.

## 2026-08-25T20:49:07.504947-03:00 — P1 done

P1 ENFORCEMENT fechado — 3 defeitos que o proprio OUTER expos, todos guardados por teste. D1: o no explore_round era ESTRUTURALMENTE incapaz de iterar — touring explore sai 1 em toda rodada nao-convergida (contrato: 0 convergido/1 continuar/3 degradado) e o no loop do adw.py faz return on_fail para qualquer exit != 0; corrigido em 3 specs com [ rc -le 1 ], preservando falha real no degradado (3). Bug espelhado em explore-plan.toml: sem pipefail o exit era do tail, nunca falhava. Endurecido tambem o NEW_FINDINGS= (impresso no TOPO, cortado por tail -c 1800 — medido 1457 B hoje, cabe, mas cresce com o ledger). D2: slug do ledger nao dobrava diacriticos (ê.isalnum() é True) — inteligencia/inteligência abriam ledgers distintos para a MESMA pergunta; corrigido com NFKD + adocao nao-destrutiva do ledger legado. D3: o gate do flow casava QUALQUER *.ledger.json fresco do escopo — deu present apontando para o ledger NAO-convergido nesta sessao; agora exige require_json verdict.converged, fail-closed. Guards: 5 slug + 6 gate + 5 credit = 16 testes novos, todos mutation-proof.

## 2026-08-25 20:50 — rodada 2 + P0/P1
Rodada exaustiva pedida por Gabriel (infra + externo + Context7) → `research-2026-08-25-rodada-2.md`.
Aprovação "P0–P4 completo, revisado" → DAG `task_1787700607622354408` (6 fases, sem ciclos).
P0 SINAL e P1 ENFORCEMENT fechados com prova medida: outcome_reward 1,70% → 2,59%; 3.450 testes
Rust verdes; 16 guards novos. P2 liberado.

## 2026-08-25 20:56 — P2 (metade que aprende)
Braço `code_mode:<mode>` ligado: o T3, ao entregar a rota, registra `RouteOffer` no estado de
turno; o PostToolUse reivindica a oferta UMA vez e deposita recompensa conforme o comando
seguinte (`touring run` = rota tomada 1.0; token de relaxamento = recusada 0.0; qualquer outra
coisa = desconhecido, sem depósito). O depósito passa por `inject_reward`, que alimenta o
OnlineRLEngine/QTable — logo a QTable passa a receber outcome, não só telemetria de ferramenta.
Ponto de decisão levantado para Gabriel: dar ao braço o CONTROLE da apresentação é exatamente o
caso que a restrição do P3 manda submeter ao gate humano (sinal proxy produzido pelo próprio
sistema avaliado). Metade que aprende entregue; metade que decide, em aberto.

## 2026-08-26 00:29 — P2c provado ao vivo (decisão (b) implementada)
Gabriel decidiu **(b)**: o braço ganha controle, atrás de env default-OFF, com promoção por
evidência medida. Implementado: `record_route_offer` como escritor único (deny code-mode + fusão
T3), `arm_choice_from_counts` (piso de 20 amostras E mínimo de 2 braços elegíveis),
`TOURING_CODE_MODE_ARM_ARMED` default-OFF, e a política entrando **depois** de prefixo/env/toml —
declaração humana nunca é sobrescrita.

Prova ao vivo no binário implantado: deny code-mode → `code_mode_arm_offered_code = 1`;
uma chamada intermediária ilegível **não** consumiu a oferta (a correção de ordem); `touring run`
→ `followed_code = 1`. `t3_turn_fused` permaneceu **0** — o gatilho original nunca dispara, que é
exatamente o que o P2c corrigiu.

Bloqueador achado e corrigido no caminho: os contadores de gate-metrics são voláteis (medido:
`t3_turn_first_passed` caiu 2→0 num restart de daemon), então uma política com piso de amostra
nunca promoveria nada entre deploys. A decisão passou a ler evidência **durável por projeto**
(`.claude/touring/code_mode_arm.json`), com falha de escrita observável por `warn` em vez de
silenciosa.

Achado de Gabriel registrado como `P5b`: `scan_class_of` não reconhece `touring run`, então
G1/T3/gate-modo-code nunca veem a rota sancionada — N programas diferentes e triviais somam N
adoções e zero avisos. A métrica mede o canal, não a economia.

## 2026-08-25T21:33:49.260164-03:00 — P2c done

P2c fechado — a fonte da oferta corrigida por MEDICAO. O braco nascera preso ao fuse T3 e mediu-se t3_turn_fused=0: o gatilho nao ocorre neste modelo de execucao porque o PostToolUse de cada chamada fecha o turno. A oferta passou a ser gravada tambem no deny code-mode (code_mode_gates), que e o que dispara de fato. record_route_offer virou escritor UNICO dos dois sitios. PROVA AO VIVO no binario implantado: deny -> offered_code=1; chamada intermediaria ilegivel NAO consumiu a oferta (correcao de ordem classificar-antes-de-reivindicar); touring run -> followed_code=1; t3_turn_fused permaneceu 0 o tempo todo. BLOQUEADOR achado no caminho (a partir da observacao de Gabriel sobre a sequencia de bashs): contadores de gate-metrics sao volateis — t3_turn_first_passed caiu 2->0 num restart — logo uma politica com piso de amostra nunca promoveria entre deploys. A decisao passou a ler evidencia DURAVEL por projeto (.claude/touring/code_mode_arm.json), provada em disco: {code:{offered:1,followed:1}}. Falha de escrita virou warn em vez de silencio.

## 2026-08-25T21:35:11.928388-03:00 — P2 done

P2 POLITICAS fechado conforme a decisao (b) de Gabriel. O braco code_mode GANHOU o controle da apresentacao, atras de TOURING_CODE_MODE_ARM_ARMED (default-OFF), com promocao por evidencia medida. Invariantes fixadas por teste (28/28 no modulo): desarmado o caminho e byte-identico ao anterior; declaracao humana (prefixo/env/touring.toml) NUNCA e sobrescrita — a politica so preenche o espaco deixado aberto; piso de 20 amostras E minimo de 2 bracos elegiveis (um braco sozinho nao e comparacao, e a configuracao vigente se confirmando); margem minima de 0.05 (diferenca dentro do ruido nao e evidencia, e empate jamais desempata pela ordem do vetor); evidencia corrompida ou ilegivel deixa a politica calada em vez de inventar escolha. Assimetria deliberada: bump_arm coleta SEMPRE (mesmo desarmado) e a leitura so acontece armada — se a coleta dependesse da env, no dia de armar a evidencia estaria em zero e a leitura natural seria 'nao funciona'. QTable passa a receber outcome real via inject_reward em vez de so telemetria de ferramenta. FORA DE ESCOPO POR DECISAO: exploracao (oferecer de proposito braco sub-amostrado) degradaria a apresentacao para colher dado — outra decisao humana. Follow-ups: P2d (expor no touring kpi lendo a MESMA fonte que a politica le).

## 2026-08-26 00:38 — P2d: a evidência ficou legível, pela fonte certa
Três commitments novos em `docs/kpi/commitments.yaml` — `touring.code_mode.arm_{native,both,code}`
= rota tomada / rota oferecida. O resolvedor lê `<projeto>/.claude/touring/code_mode_arm.json`,
a **mesma** fonte que `arm_choice_from_counts` consulta, e o piso virou constante compartilhada
(`ARM_MIN_SAMPLE`) em vez de dois números que divergiriam no primeiro ajuste. Abaixo do piso o KPI
reporta **STUB**, nunca um 0 falso — amostra insuficiente é desconhecido, não adesão nula.

O motivo de não ler `gate-metrics` aqui: aqueles contadores zeram no restart (medido, 2 → 0 em dois
minutos), então o painel diria "pronto para promover" com base num número que o juiz não usa — a
classe `verificador-usa-menos-que-o-extrator`.

## 2026-08-26 01:03 — calibração do G1 por similaridade (ideia de Gabriel)
*"Se o comando repetir pelo menos 50% do comando anterior"* — preenche um buraco real: o G6 pega só
byte-idêntico, o G7 só re-inspeção do MESMO arquivo, e o G1 só ao acumular N da mesma classe.
`grep X a.rs` → `grep X b.rs` é a assinatura canônica do fan-out serial e não era pego por nenhum.

Implementado como **calibração do G1** (mesmo remédio, gatilho mais cedo), não como gate novo:
Jaccard sobre tokens ≥ 0,5 dispara já na SEGUNDA chamada. Jaccard e não prefixo comum porque
`sed -n 1,20p f` vs `sed -n 40,60p f` divergem cedo no texto e compartilham quase todo o trabalho.

**Falso positivo que o teste pegou**: `g6_mutacao_no_meio_reseta_a_repeticao` quebrou — reler
DEPOIS de um Edit virava "repetição". Reler o que acabou de mudar é legítimo. A época de mutação
entrou no `burst_ledger` e o gatilho só dispara na MESMA época, a mesma proteção que o G6 já tinha.

## 2026-08-25T22:07:24.020440-03:00 — PreCompact snapshot

Loop active. Pending: [P3,P4,P5,P5b,P2d]. Resume: `touring decompose ready task_1787700607622354408`.

## 2026-08-25T22:12:34.559409-03:00 — P2d done

P2d fechado: a evidencia do braco ficou legivel PELA FONTE CERTA. Tres commitments novos em docs/kpi/commitments.yaml (touring.code_mode.arm_{native,both,code}) mais touring.code_mode.economy_ratio, todos resolvidos por leitura de <projeto>/.claude/touring/code_mode_arm.json — a MESMA fonte que arm_choice_from_counts consulta. Ler gate-metrics aqui exibiria um numero diferente do que decide (eles zeram no restart: t3_turn_first_passed caiu 2->0 em dois minutos), que e a classe verificador-usa-menos-que-o-extrator. O piso virou constante COMPARTILHADA (ARM_MIN_SAMPLE) em vez de dois numeros que divergiriam no primeiro ajuste. Abaixo do piso o KPI reporta STUB, nunca 0 falso — amostra insuficiente e desconhecido, nao adesao nula. Verificado: os 4 checks aparecem em touring kpi -j com status STUB.

## 2026-08-25T22:19:02.259644-03:00 — P3 done

P3 RESEARCH LOOP: primeira campanha real, determinstica, sobre corpus CONGELADO. extract_corpus.py tirou 5338 comandos Bash de 22 sessoes reais (40 transcripts). O harness de replay usa os predicados REAIS (scan_class_of, command_similarity) e estado LOCAL — um verificador que reimplementasse a regra mediria a copia, e um experimento que escrevesse no ledger vivo contaminaria o que mede. CURVA sobre 1099 inspecoes: limiar 0.0 colapsa 1007 (91.6%, sim media 0.219); 0.4 colapsa 107 (sim 0.551); 0.5 colapsa 77 (sim 0.599); 0.6 colapsa 50 (sim 0.645); 0.7 colapsa 10 (sim 0.803). TRES LEITURAS: o 0.5 de Gabriel colapsaria 77 inspecoes reais; ha joelho entre 0.6 e 0.7 (as quase-duplicatas vivem em 0.5-0.7); abaixo de 0.4 a similaridade media cai de 0.55 e o gate passa a fundir comandos diferentes. KEEP/DISCARD: maximizar o escalar sozinho empurra o limiar para 0 — a demonstracao CONCRETA da fronteira do verificador do survey MSR, agora com numero. 7 variantes no variant_archive, 3 marcadas terminal (sim media < 0.55), 4 elegiveis. NAO CONSTRUIDO: a embalagem como touring adw run autoresearch (registrado como P3b) — o experimento roda por harness de teste, honesto para a primeira campanha e nao e produto.
