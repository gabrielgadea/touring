---
type: Plan
title: Plano autoresearch RL/inteligência — P0–P5 (revisado pela rodada 2)
description: DAG aprovada por Gabriel em 2026-08-25 após a rodada exaustiva de exploração; task_1787700607622354408.
plan_id: 2026-08-25-autoresearch-rl-intelligence
tags: [plan, autoresearch, rl, dag]
timestamp: 2026-08-25T20:32:00-03:00
okf_version: "0.1"
---

# Plano — P0–P5

Part of the [bundle](/index.md). Base: [estratégia](/strategy-2026-08-25-autoresearch.md) +
[rodada 2](/research-2026-08-25-rodada-2.md). DAG: `task_1787700607622354408` (6 subtasks, sem ciclos).

## Tese

Toda execução vira experimento registrado com outcome; todo outcome alimenta recompensa; toda
recompensa move uma política; e um research loop propõe e testa mutações do próprio harness —
**mas só é autônomo onde existe verificador determinístico independente do avaliado** (MSR
*Agentic Evolution*, ~300 papers). Onde o sinal é proxy, o gate humano é estrutural, não opcional.

## Fases

| Fase | Escopo | Depende de | Critério de convergência (medido) |
|---|---|---|---|
| **P0** SINAL | phase-close deriva `--credit-query`; `adw run` credita por run; `router_accuracy` computado; veredito do gate vira feedback textual | — | coverage de `outcome_reward` sobe de **1,93%** (167/8.671) por probe SQL; retry com feedback provado por teste mutation-proof |
| **P1** ENFORCEMENT | D1 lente pendente não derruba `strategy-loop`; D2 slug normaliza diacríticos; D3 gate exige tema + `converged` | — | `strategy-loop` completa fim-a-fim exit 0 em tema com lente externa; 3 guards mutation-proven |
| **P2** POLÍTICAS | braço LinUCB da apresentação code-mode com recompensa = adoção; QTable com join de outcome | P0 | `update_count` cresce com recompensa real; o braço muda de escolha sob evidência |
| **P3** RESEARCH LOOP | ADW `autoresearch`: mutação → shadow → escalares → keep/discard → `variant_archive`, particionado pela fronteira do verificador | P0,P1,P2 | 1 campanha real sobre T3-B com keep/discard registrado |
| **P4** DSPy/GEPA | otimizar personas/critic-lenses; métrica `dspy.Prediction(score,feedback)` alimentada por vereditos de gate | P0,P2 | 1 alvo otimizado com antes/depois em holdout (piso 30–300 exemplos) |
| **P5** CONSOLIDAÇÃO | A/B contra os baselines da rodada 2; co-evolução docs/skills/rules; propagação com prova comportamental | todas | `loop_converged.py --rust-full` exit 0 |

## Baselines (rodada 2, para medir o delta)

`outcome_reward` 167/8.671 = 1,93% · `never_recalled` 1.717/8.671 = 19,8% ·
`corpus_coverage` 0,861 · `code_mode.adoption_ratio` 0,083 · `flow.compliance_ratio` 0,514 ·
`adw.runs` 35 · `ledger_credited_total` 1 · `adw.py` reward callsites 1/3.823 linhas.


---

## Decisão P2b — resolvida por Gabriel em 2026-08-26: **(b)**

O braço `code_mode` **ganha** o controle da apresentação, atrás de env default-OFF, promovido por
evidência medida. Não é shadow permanente nem controle vivo direto.

**Por que (b) e não as outras.** O sinal que alimenta o braço ("o modelo seguiu a rota?") é
produzido pelo próprio sistema que a política dirige — proxy auto-referencial, a classe que o
survey MSR mede como degradante sob iteração. Além da fronteira do verificador, o survey diz que
evolução confiável depende de **pressão seletiva humana**; `default-OFF + promoção medida` é
exatamente isso, com Gabriel no gate.

### Como ficou implementado

| Peça | Onde | Invariante |
|---|---|---|
| Oferta da rota | `record_route_offer` — escritor **único**, chamado no deny code-mode e na fusão T3 | dois escritores seriam duas versões da mesma decisão |
| Veredito | `claim_route_reward` — classifica **antes** de reivindicar | desfecho ilegível não consome a oferta |
| Evidência | 6 contadores `code_mode_arm_{offered,followed}_{native,both,code}` | lidos direto (sem `capture()`) — caminho quente |
| Armar | `TOURING_CODE_MODE_ARM_ARMED=1` | sem a env, caminho byte-idêntico ao anterior |
| Escolha | `arm_choice_from_counts` — piso de 20 amostras **e** mínimo de 2 braços elegíveis | um braço sozinho não é comparação, é a configuração vigente se confirmando |
| Precedência | política entra **depois** de prefixo/env/alias/`touring.toml` | declaração humana nunca é sobrescrita |

**Fora de escopo por decisão, não por esquecimento:** exploração (oferecer de propósito um braço
sub-amostrado) degradaria a apresentação para colher dado — é outra decisão humana, não um detalhe
de implementação.

### P2c — a fonte da oferta, corrigida por medição

O braço nasceu preso ao fuse T3 e mediu-se `t3_turn_fused = 0`: o gatilho não ocorre neste modelo
de execução. A oferta passou a ser gravada também no **deny code-mode**, que é o que dispara de
fato. Sem essa correção, a política teria ficado com amostra permanentemente abaixo do piso — e a
promoção nunca aconteceria, sem que nada indicasse o porquê.

### Como ler a evidência para decidir a promoção

```bash
cat <projeto>/.claude/touring/code_mode_arm.json     # {"native":{"offered":N,"followed":M}, …}
TOURING_CODE_MODE_ARM_ARMED=1 <comando>              # armar por-comando (teste)
```

A política só age quando **dois** braços passam de 20 amostras — e nunca sobre um `touring.toml`
declarado. Exposição no `touring kpi` fica em `P2d`, com uma exigência: ler a **mesma** fonte que a
política lê. Um KPI que mostrasse os contadores voláteis enquanto a política lê o arquivo faria
você promover com base num número diferente do que decide.

### A assimetria deliberada: coletar sempre, agir só quando armado

`bump_arm` roda **sempre** que a apresentação entrega uma rota — inclusive com a política
desarmada. `arm_counts_durable` só é lido **quando armada** (`code_mode_arm_armed() && let
Some(...)` curto-circuita: desarmado custa zero).

A assimetria é o ponto. Se a coleta também dependesse da env, no dia em que você armasse a
política a evidência estaria em zero, ela ficaria calada pelo piso de amostra, e a leitura natural
seria "não funciona" — quando na verdade nunca teria havido o que ler. Não se promove por
evidência que só começa a existir depois da promoção.

Custo: uma leitura + escrita de ~200 bytes por deny (eventos de escala humana), fora do caminho
das chamadas que passam.

---

## P5b resolvido — medir a economia, não só o canal

A observação de Gabriel: `touring run` é isento de todo gate (`scan_class_of` não o reconhece) e
cai no balde bom da adoção, então N programas diferentes e triviais somam N adoções e zero avisos.

Ao desenhar, uma sutileza mudou a solução: sob a apresentação `code`, `grep`/`cat` de **um** arquivo
são NEGADOS — logo embrulhar em programa é **forçado**, não indisciplina do modelo. Avisar o modelo
seria culpá-lo pela política. O desperdício é real e o custo é da **apresentação**.

Por isso a economia não virou métrica de disciplina: virou o **custo que faltava no braço do P2**.
Sem ela, o braço aprenderia que `code` é ótimo porque todos obedecem — sendo que cada obediência
custou um round-trip.

| Peça | Contrato |
|---|---|
| `program_shape(body)` | `Trivial` = 1 operação sobre 1 alvo; `Fused(n)` = o maior entre operações e alvos distintos |
| `extract_run_body(cmd)` | tira o corpo de `--code '…'`; `None` sem `--code` — nunca adivinhar a forma de um programa não lido |
| `classify_route_outcome` | **0.5** para a casca sobre uma chamada; **1.0** para o programa que funde. Não 0.0: sob `code` a casca é obrigatória |
| 3º eixo na evidência | `{offered, followed, economical}` — obediência e economia são eixos diferentes; fundi-los faria o braço obedecido e caro parecer excelente |
| `touring.code_mode.economy_ratio` | `economical/followed`, lido da mesma fonte que a política; STUB abaixo do piso |

A política **ainda escolhe por obediência**, não por economia: mudar o critério de escolha é
decisão sua, não efeito colateral de ter passado a medir.

### Defeito de produção que os testes pegaram no caminho

A oferta de rota estava guardada como campo do `TurnBurst`, e `record_route_offer` fazia
`get(...).unwrap_or_default()`. No deny code-mode o turno frequentemente **não existe** — então a
oferta inseria um turno falso com `first_passed = false` e a fusão T3 seguinte nunca dispararia.
Apareceu como teste falhando em paralelo e passando com `--test-threads=1`, com a **vítima
alternando** entre vizinhos: a assinatura de estado global. A oferta foi para cache próprio — ela
nunca foi estado de turno, tanto que sobrevive ao `turn_gate_close` por design.

---

## P4 — o bloqueador medido, e o que foi feito sobre ele

**Medição (26/08):** 128 exemplos numa família coerente (`loop:*`, acima do piso de 30 do DSPy),
mas **209 de 225 memórias com `outcome_reward` valem ≥ 0,5** e a média é **0,919** — 93% positivas.
Quantidade sim, **discriminação não**: um trainset quase constante não dá gradiente, e o GEPA
otimizaria contra uma constante.

A causa é estrutural, não acidental: o veredito vem de `status == "done"`, e quem fecha uma fase é
quem acabou de fazê-la funcionar. Sinal **auto-referencial** — exatamente o que o survey MSR diz
que degrada com a iteração.

**Feito:** `adw.py::_store_gate_rejection` — toda reprovação de gate vira caso rotulado
**negativo** (`--reward 0.0`, `#status:negative`, contexto `adw_gate_reject:<nó>`), no mesmo sítio
onde o veredito já era registrado para o retry (W4 S-4.6). Reprovação de gate é falha real medida
por **código**, independente de quem a produziu — a condição que o survey exige para autonomia, e
o rótulo que faltava. Fail-open: um caso não gravado é um exemplo a menos, não um run quebrado.
4 guards.

**O que ainda falta para o P4 começar:** os rótulos negativos **acumularem**. Isso é tempo de uso,
não implementação — e é a razão honesta de o P4 não fechar nesta sessão. O critério de partida é
observável: `SELECT COUNT(*) FROM memory_entries WHERE outcome_reward < 0.5` cruzar algumas dezenas
dentro de uma família de alvo.

### Calibração do predicado de economia: só ao vivo

`run_journal.jsonl` (649 registros) guarda `run_id`, `language`, `duration_ms`, `exit_code`,
`bytes_elided` e o **hash** do stdout — não o corpo do programa. Então não há corpus histórico
contra o qual pré-calibrar `program_shape`: a fronteira Trivial/Fused não pôde ser aferida contra
programas reais antes de entrar em uso.

É uma limitação declarada, não um esquecimento. O `touring.code_mode.economy_ratio` acumula
exatamente essa distribuição ao vivo — e se ela vier degenerada (tudo Trivial ou tudo Fused), o
próprio KPI é o sinal de que o limiar precisa mudar.

---

## P3 — a primeira campanha real, e o que ela mediu

**Substrato:** `eval/autoresearch/extract_corpus.py` extraiu **5.338 comandos Bash de 22 sessões**
reais (40 transcripts). O corpus é **congelado**: quem o consome não o escreve, então a métrica não
pode ser movida durante a campanha. Declarado o que isso NÃO é — os comandos vieram do próprio
modelo cujo gate está sendo calibrado; o congelamento garante imutabilidade, não neutralidade.

**Verificador:** um harness de replay que usa os predicados **reais** (`scan_class_of`,
`command_similarity`), nunca uma cópia — um verificador que reimplementasse a regra mediria a
cópia. Simula com estado local, jamais os caches globais: um experimento que escrevesse no ledger
vivo contaminaria o sistema que mede.

**A curva medida** (1.099 inspeções no corpus):

| limiar | colapsadas | % | sim. média |
|---|---|---|---|
| 0,0 | 1007 | 91,6% | 0,219 |
| 0,3 | 256 | 23,3% | 0,426 |
| 0,4 | 107 | 9,7% | 0,551 |
| **0,5** | **77** | **7,0%** | **0,599** |
| 0,6 | 50 | 4,5% | 0,645 |
| 0,7 | 10 | 0,9% | 0,803 |

Três leituras: **(a)** o limiar 0,5 de Gabriel colapsaria 77 inspeções reais; **(b)** há um joelho
entre 0,6 e 0,7 — as quase-duplicatas reais vivem em 0,5–0,7 e 0,7 perde 80% delas; **(c)** abaixo
de 0,4 a similaridade média das colapsadas cai de 0,55, isto é, o gate passa a fundir comandos
genuinamente diferentes.

**O keep/discard, e por que ele precisou de dois eixos.** Maximizar o escalar sozinho empurra o
limiar para **0** (91,6% de "economia" fundindo tudo). Isso não é um defeito do experimento: é a
demonstração concreta da fronteira do verificador que o survey MSR descreve, agora com número.
O arquivo registra **7 variantes**, com as de similaridade média < 0,55 marcadas `--terminal`
(becos sem saída mantidos como evidência, nunca amostrados como pais) — 4 elegíveis.

**O que NÃO foi construído:** a embalagem como `touring adw run autoresearch`. O experimento roda
por um harness de teste (`cargo test -p touring-cli --lib autoresearch_replay -- --ignored
--nocapture`), que é honesto para a primeira campanha e não é produto. Registrado como `P3b`.
