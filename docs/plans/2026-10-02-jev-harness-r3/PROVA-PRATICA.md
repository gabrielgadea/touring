---
type: PracticalProof
title: "r3 — Prova prática (o que roda ao vivo, como reproduzir, o que NÃO está provado)"
description: "Evidência viva do shadow A1/A2 na release 30.4.91: eventos reais, pipeline até o kpi, entrega S1–S6, comandos de reprodução e lacunas declaradas"
plan_id: 2026-10-02-jev-harness-r3
okf_version: 0.1
tags: [r3, proof, shadow, live]
timestamp: 2026-10-02T16:12:00-03:00
---

# r3 — Prova prática

Pedido do Gabriel: *"me prove que tudo funciona de forma perfeita, na prática"*. Resposta direta: **a tubulação do shadow funciona ao vivo, de ponta a ponta, nos dois casos; o valor dos modelos (Jev/Laya) foi MEDIDO e, com a entrada que receberam, é negativo (nenhum supera o B0 regra)** — e dois critérios ficaram fracos ou não demonstrados (S1b não passa no `cli-suggest`; S3 `NAO_DEMONSTRADO`). "Perfeito" não é o que os dados sustentam; o que segue é o que sustentam.

## 0. Quadro-resumo

| Capacidade | Estado | Onde está a prova |
|---|---|---|
| Emissor A2 (oferta → desfecho) observe-only, default OFF | **Funciona ao vivo** | §3 |
| Emissor A1 (bloco injetado pelo `pre-bash`) | **Funciona ao vivo** (provado nesta sessão, 15:55 BRT) | §2 |
| Avaliador online (B-D regra, B-L Laya em CPU) → journal → `touring kpi` | **Funciona ao vivo** | §2, §3 |
| Custo no caminho quente (S1a/S1b), fail-open (S2), privacidade (S6) | S1a, S2 e S6 **passam**, medidos. **S1b passou com n = 206 e NÃO passa na releitura de 03/10** (`cli-suggest` p95 598 > 583 ms, n = 3.862; atribuição inconclusiva). **Definitivo no 1º dia ON completo (03/10 21:01, Errata 15): `cli-suggest` 615 > 583, o S1b NÃO PASSA; `post-tool-rl` 724 ≤ 874** | RESULTS §2, Errata 14 e 15 |
| Vereditos A1/A2 pelos critérios congelados | **Computados**: A1 NAO_ADOTADO, A2 NAO_ADOTADO (gold κ = 0,77; reimplementação independente bate) | RESULTS §1 |
| Gold do A2 (2 avaliadores cross-vendor + desempate) | **Existe** (171 itens), com limitações de família/desvio E3.3-bis (desvio **aceito pelo Gabriel**, Errata 13); verificado por código. **Um 3º juiz cego (Sonnet) concorda 171/171 com ele e o veredito do A2 é o mesmo em 3 leituras do gold**; o rater 1 (MiniMax) diverge sempre na mesma direção (16/16, mais "seguida") e o gold é quase todo rótulo Claude | RESULTS §8, Errata 7 e 10 |
| Byte-identidade ON×OFF (S4) | **Passa, forte** em 3 hooks (175 · 57 · 71 comparações com conteúdo; 0/200 diferenças; controle 0/200); `post-tool-rl` anulado; reproduzido numa réplica pré-registrada (rodada 3, contagens idênticas). A rodada 1 foi VOID por defeito do meu harness, corrigido e pré-registrado antes da rodada 2 | RESULTS §2, §3.12; §7.3 abaixo |
| Valor dos modelos (Jev/Laya) no A2 | **Medido, negativo para a entrada recebida**: B0 0,906 · B-J 0,835 (0,80–0,92 em 3 amostras da mesma API: o Jev não é estável bit a bit, e o veredito é o mesmo nas três) · B-L 0,706 de acordo com o gold; braços só viram features redigidas (≠ B0/gold). **O r4 mínimo testou o Laya COM texto truncado (≤ 200 chars): também não passa** (AUC 0,331; RESULTS §7); outros modelos e outras perguntas não foram testados | RESULTS §1, §4 |
| A1 online nos 4 pontos do `cli_suggester` | **Não feito** (S-9b) | RESULTS §4 |

## 1. Ambiente da prova

Release 30.4.91 (commit `d4f62279`), daemon `touring-daemon` PID 2546525 reiniciado às 15:09 BRT com ambiente limpo (sem `TOURING_R3_SHADOW`, sem `TOURING_CODE_MODE`). A flag vem de `.touring/touring.toml` → `[shadow] r3 = true` (o daemon lê o toml uma vez por projeto). Avaliador: unit transitória `r3-shadow-evaluator` (python do `laya-venv`). Eventos em `~/.claude/touring/logs/r3_events.jsonl`; decisões em `r3_shadow.jsonl`.

## 2. Prova viva do A1 (feita nesta sessão)

**Por que foi preciso provar.** Depois do shadow ligar, o A1 mostrava 0 eventos em ~45 min. Antes de suspeitar do emissor, medi a taxa-base: o censo do transcript (`scratchpad/prebash_ctx_census.py`) mostra que o `pre_bash` injetou o aviso do Pensieve **125 vezes** entre 10Z e 17Z (último às 17:56:39Z) e **0 vezes** depois das 18:09:22Z. O reinício do daemon zerou o Pensieve (só o `post_bash` grava falha). Zero eventos era o resultado esperado de um gatilho que não disparou.

**Reprodução do gatilho, pelo daemon real, com o shadow ligado:**

```bash
ls /nonexistent-r3-probe-a1-second          # exit 2, SEM pipe (um `| head` mascara o exit e o hook vê 0)
ls /home/gabrielgadea/projects/touring/.touring   # mesma forma de comando
tail -n 2 ~/.claude/touring/logs/r3_events.jsonl
```

Observado: o `pre-bash` injetou `Similar command failed recently (similarity=74%)` e `⚡ last fail: ls — Exit code 2`, e o emissor gravou:

```json
{"bytes":82,"first_line_head":"Similar command failed recently (similarity=74%)","hook":"pre-bash","kind":"a1_block","session_hash":"1ef2a43bf3e2","use_case":"a1_relevance","v":1}
{"bytes":33,"first_line_head":"⚡ last fail: ls — Exit code 2","hook":"pre-bash","kind":"a1_block","session_hash":"1ef2a43bf3e2","use_case":"a1_relevance","v":1}
```

(Linhas de 15:55:56 e 15:56:00 BRT; o schema é fechado: contagem de bytes, 1ª linha truncada, hash da linha e da sessão, nunca o texto completo.)

**Até o kpi:** `r3_shadow.jsonl` recebeu decisões `a1_relevance` do braço B-D (`model: rule:dedup+exempt`) e `touring kpi -j` passou a mostrar `shadow.use_cases.a1_relevance` com `n_events = 16`, `n_decisions = 16`, `would_drop = 14`, `status = "measured"`.

> **Ressalva obrigatória:** esses 16 eventos são, em maioria, **minhas sondas e as repetições que elas causaram** (o Pensieve aprendeu a falha de `ls` e a repetiu; o dedup do B-D descarta repetição). Provam que o caminho emissor → avaliador → journal → kpi funciona; **não medem eficácia**. A eficácia do A1 é a do replay (NAO_ADOTADO).

## 3. Prova viva do A2 (tráfego real)

Desde 15:10:20 BRT o emissor A2 grava `a2_offer_closed` quando uma oferta do `cli-suggest` é fechada (consumida, ignorada ou por janela). O `kpi` mostra `a2_outcome`: 5 eventos, 5 decisões do B-L (Laya base, CPU), `agreement = 0.0` com n = 5 contra o B0 online, latência p50 342 ms / p95 3.621 ms (a 1ª decisão paga a carga de ~842 MB do modelo; as seguintes ficam em ~340–380 ms). O B-L decidiu `followed = False` nos 2 primeiros eventos reais, **discordando** do B0. Com n = 5 eventos online e sem gold do tráfego online, isso não diz quem tem razão; a comparação com gold é a do replay (RESULTS §1: o B-L concorda menos com o gold que o B0, 0,706 × 0,906).

## 4. Entrega e saúde (medidas, não prometidas)

- **S1a** OFF p99 963 ns · emissão p99 27 µs. **S1b** `cli-suggest` p95 550 ms (≤ 583) · `post-tool-rl` p95 793 ms (≤ 8294), n = 206/200. **S2** não-ok 0,00 %. **S6** passa nos 2 journals. **Releitura 03/10 00:45 (n = 3.862 / 3.812): `cli-suggest` p95 598 ms > 583, o S1b NÃO passa; `post-tool-rl` 918 ms passa pela letra (o envelope 8294 é uma fatia parcial de 25/09; em dias completos é 874 e o veredito é indeterminado: 913 agregado > 874, mas 792 na fatia de 03/10 < 874); S2 0,00 % e S6 passam** (Errata 14, E14.6). **Leitura definitiva (03/10 21:01, 1º dia ON completo em UTC, Errata 15): `cli-suggest` p95 615 ms (n = 1.623) > 583, o S1b não passa em nenhum recorte; `post-tool-rl` 724 ms (n = 1.646) ≤ 874 no dia completo, sem robustez.**
- `touring e2e -j`: overall **0,857** (baseline mediana 0,857) · ast 0,776 · wiring 0,810 · knowledge 0,900 · quality 1,000 · learning 0,886 · index 0,849 (vs 0,852: diluição de cobertura 5643/7537, não regressão de qualidade).
- Cadeia de gates do `propagate-release --stop-before-propagate`: rc = 0 em 8 min 37 s (gates, build, rótulo, testes pós-build, snapshot da toolchain, canal default).

## 4b. Verificação independente (feita por quem não escreveu o código)

| Verificação | Quem | Resultado |
|---|---|---|
| Pontuador do A2 reimplementado só a partir da Errata 8 | subagente Claude de contexto limpo | **todos os números e o veredito batem** (só o p95 do B-L difere na 2ª casa, por método de percentil) |
| Auditoria cruzada de todo o código r3 (emissores Rust, scripts, contratos) | `touring-auditor` | score composto **0,92**; 7 achados, todos verificados: 4 corrigidos (gate C-A2-2 por vácuo; 9 testes Rust no-op; "quase tudo em 0,5" errado; `MAX_PENDING` sem linha, só no código até a próxima release), 1 falso-positivo (arquivo solto que nunca existiu), 1 era leitura de versão antiga do relatório, 1 confirmado só em parte (B-J do cache); recontou 8 números, 6 batem e os 2 restantes eram o item já reescrito e a frase errada |
| `doctor` (9 checks) e `e2e` | CLI | `doctor` **todo ok** depois de limpar resíduos de wiring (`kind_unknown` 2 → 0); `e2e` 0,846 (faixa documentada 0,846–0,857) |
| Rater 1 e juiz de desempate | subagentes MiniMax e Claude de contexto limpo | validados por código (`r3_scan_rater_transcript.py`), não por leitura |
| Sonda de polaridade do `noul` do Laya | `touring-a4` | hipótese pré-declarada **não confirmada** (AUC 0,500 em 8 casos sintéticos) |
| Juiz de convergência (`loop_converged.py`) | código | **`converged: true`** (DAG 14/14, qualidade Gold 0,8745, 0 P0 BLOCK, escopo inteiro medido; 3 cláusulas N/A) |
| Correção dos testes Rust, `hw-slot --class rust` | cargo | ambiente limpo **18 passam**; `TOURING_R3_SHADOW=1` **9 falham em voz alta** (antes: verdes); `fmt` e `clippy -D warnings` limpos |

O `touring-4b` recebeu a mesma tarefa de reimplementação mas não confirmou o recebimento (provável mensagem retida por permission mode); foi coberta pelo subagente.

## 5. Testes e mutantes (a prova de que os testes provam)

| Módulo | Testes | Mutantes mortos |
|---|---|---|
| `scripts/r3_adjudication.py` | 27 | 20 |
| `scripts/r3_results_gate.py` | 16 | 8 |
| `scripts/r3_prove_delivery.py` | **38** | 19 (anteriores) **+ 6** do fix S4 (cópia hermética, baseline verde impresso antes) |
| `scripts/r3_score_a2.py` (pontuador do A2, Errata 8) | **20** (inclui fixture calculado à mão) | **18/18** (1 equivalente identificado e trocado) |
| `scripts/r3_scan_rater_transcript.py` (E7.2) | **29** | **13/13** |
| `scripts/r3_results_gate.py` | **18** | **2/2** do fix do C-A2-2 (o 2º só morreu depois de fortalecer o teste) |
| `r3_shadow.rs` (guardas de ambiente dos testes) | 18 passam / 9 falham com a flag forçada | a guarda agora é audível |
| Emissor A1 (`pre_bash.rs`, S-9) | testes de unidade + S1a | 4/4 (a 1ª entrega tinha 1/4 e foi recusada) |

Limite da prova: os harnesses de mutação vivem no scratchpad da sessão (não no repo); os resultados estão registrados em `phases/S-*.md` e no `log.md`. Os testes que eles exercitam estão no repo e rodam sem eles.

## 6. Reproduzir (sem depender desta sessão)

```bash
cd ~/projects/touring
# vereditos mecânicos
python3 scripts/r3_results_gate.py a1 ~/.claude/touring/r3-replay/a1/results-a1.json
python3 scripts/r3_score_a2.py --predictions ~/.claude/touring/r3-replay/a2/predictions-a2.jsonl \
  --gold ~/.claude/touring/r3-data/adj_merge_final/gold_a2.jsonl --out /tmp/results-a2-gold.json   # puro: sem modelo, sem API
python3 scripts/r3_results_gate.py a2 /tmp/results-a2-gold.json                                   # NAO_ADOTADO_PELOS_DADOS
python3 scripts/r3_scan_rater_transcript.py <transcript do rater> [--model claude --allow-file adj_out/adj_tiebreak.jsonl]
# suítes
python3 -m pytest scripts/test_r3_prove_delivery.py scripts/test_r3_results_gate.py scripts/test_r3_adjudication.py -q
# estado vivo do shadow
touring kpi -j | jq '.. | .shadow? // empty'
tail -n 5 ~/.claude/touring/logs/r3_events.jsonl ~/.claude/touring/logs/r3_shadow.jsonl
```

## 7. O que NÃO está provado (e por quê)

1. **Que QUALQUER modelo com texto não superaria o B0.** O r4 mínimo testou **uma** pergunta (`noul`) do Laya com o texto truncado e ela não mede "seguida" (AUC 0,331 no checkpoint inglês; 0,551 no multilíngue). O Jev nunca recebe texto (privacidade), e outras cabeças do Laya não foram testadas.
2. **O que o `noul` do Laya mede.** AUC 0,12 contra o gold (associação inversa sistemática) e 0,500 numa sonda sintética de 8 casos: não é P(seguida), e não se inverte (E8.8).
3. **S4**: provado em `pre-bash`, `cli-suggest` e `post-bash` numa rodada de 200 entradas (base virgem, comandos com caminho relativo); `post-tool-rl` não emitiu nada em nenhum braço, então não há o que comparar. Foram 2 rodadas passando (a 3ª, réplica pré-registrada, com contagens idênticas); entradas e produto são determinísticos, então a réplica prova reprodutibilidade e não independência de ambiente (RESULTS §4).
4. **S3: `NAO_DEMONSTRADO`** (fechado por código às 15:12 de 03/10 por `scripts/r3_s3_close.py`, Errata 13; `r3-data/s3_close.json`). Janela de 23,936 h: 15 ofertas, 14 linhas `a2_offer_closed`, cobertura 0,9333 (< 0,95), 1 oferta sem linha; contadores de perda 0/0/0; daemon sem reinício desde a base; 1 sessão distinta. Pela regra pré-registrada, déficit sozinho é `NAO_DEMONSTRADO`, não `REPROVADO`, e o script parou a unit `r3-shadow-evaluator` como aprovado. **Não é PASS**: o emissor não perdeu evento, mas a cobertura exigida não foi atingida, e a oferta sem linha pode ser pendência de fim de sessão (a causa não foi provada). Com 1 sessão e 15 ofertas o critério mal passa (tolerar 1 pendente exige N ≥ 20).
5. **A1 fora do `pre-bash`**: `cli_suggester` não emite A1 (S-9b).
6. **Propagação**: 30.4.91 só no daemon global; pinados seguem na 30.4.90 (decidido: esperar um release com ganho; o r3 não traz ganho aos pinados).

Nada disso é "falha escondida": são os limites do que um shadow de ~1 hora, com um único humano e um único avaliador Claude, consegue provar. O r4 mínimo entrou com o go dele e fechou negativo; um r5 **não** foi aberto (decisão dele, Errata 13).
