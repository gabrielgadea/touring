---
type: Results
title: "r3 — Resultados mecânicos (A1 relevância · A2 desfecho de rota)"
description: "Vereditos dos critérios congelados aplicados por r3_results_gate.py, provas de entrega S1–S6, defeitos pegos e limitações declaradas"
plan_id: 2026-10-02-jev-harness-r3
okf_version: 0.1
tags: [r3, results, jev, laya, shadow]
timestamp: 2026-10-02T16:10:00-03:00
---

# r3 — Resultados

> Veredito **só** por `scripts/r3_results_gate.py` sobre `r3-replay/{a1,a2}/results-*.json`.
> Nada abaixo foi decidido por leitura. Critérios: [CRITERIOS-ACEITACAO.md](/CRITERIOS-ACEITACAO.md) (Erratas 1–6).
> Plano: [plan.md](/plan.md). Prova ao vivo e reprodução: [PROVA-PRATICA.md](/PROVA-PRATICA.md).

## 1. Vereditos

| Caso | Veredito mecânico | Por quê (saída do gate, regenerada em 02/10/2026 15:59 BRT) |
|---|---|---|
| **A1** relevância das injeções | **NAO_ADOTADO_PELOS_DADOS** | C-A1-1 reprova: B-D economiza `bytes_saved` = **0,1094** (piso 0,25) na partição de decisão `test` (262 blocos, 128 EXEMPT, `exempt_retained` = 1,0). C-A1-2 (harm) é *moot*: não pode mudar o veredito, por isso o pacote cego do A1 foi cancelado (custaria ~115 mil tokens do avaliador não-Claude). |
| **A2** desfecho de rota | **GOLD_INSUFICIENTE** | κ = `null`: o gold exige 2 avaliadores cross-vendor e o avaliador 1 não-Claude não existe (dependência do Gabriel, E3.3). Sem gold, nenhum braço é pontuado; o gate recusa concluir. |

**Leitura honesta.** O A1 não é "adotado" nem "quase": a regra B-D falhou o piso nos dados de decisão. O A2 não é "reprovado": é **não decidido**, e continuará assim até existir o gold.

### A1 — fragilidade que NÃO foi suavizada
B-D economiza 35,7 % em `train`, 21,7 % em `val` e 10,9 % em `test`. A partição `test` tem **3 sessões**: o veredito mecânico vale, mas a potência é baixa e isto fica declarado. Os braços de modelo (B-L, B-J) são pontuados por `agreement_com_bd` (C-A1-3); o `bytes_saved` deles **não foi pré-registrado** e não foi inventado depois de ver o número.

### A2 — diagnóstico secundário (NÃO é veredito, NÃO é gold)
Predições dos braços sobre os 180 itens, 0 erros, comparadas ao rótulo **B0** (uma regra, não verdade): 132 `followed` / 48 `not_followed`.

| Braço | média p(followed) em B0=followed | em B0=not_followed | separação | AUC vs B0 |
|---|---|---|---|---|
| B-L (Laya base, CPU) | 0,075 | 0,149 | −0,074 | **0,002** |
| B-J (Jev, features redigidas) | 0,467 | 0,446 | +0,021 | 0,871 |

Dois avisos, ambos de **instrumento**: (1) o AUC 0,002 do B-L é quase uma inversão perfeita (1 − 0,002 = 0,998). Zero-shot ruim dá ~0,5, não 0,002; isso aponta para **polaridade não verificada** da chave `noul` lida por `r3_replay_a2._prob_de_resposta`, ou para uma feature que o B0 usa ao contrário. Nenhum veredito depende disto, mas **não se interpreta o B-L até a polaridade ser provada** (item do r4). (2) o B-J tem 12 valores distintos e quase tudo em 0,5 (sem informação), com p95 de 0,0 ms por vir de cache.

## 2. Entrega (S1–S6) — release 30.4.91, shadow ligado às 15:09:22 BRT

| Critério | Resultado | Evidência |
|---|---|---|
| **S1a** custo direto | **PASSA** | OFF p99 = 963 ns (limite 50 µs); emissão p99 = 27 µs (limite 1 ms) |
| **S1b** envelope de latência | **PASSA** | `cli-suggest` p95 550 ms ≤ 583; `post-tool-rl` p95 793 ms ≤ 8294 (n = 206 / 200) |
| **S1** original (só informativo, Errata 5) | **reprovaria** `cli-suggest` | 401 → 550 ms (limite 441). Declarado: a Errata 5 o rebaixou por estar dentro do ruído natural, mas o número existe |
| **S2** fail-open | **PASSA** | não-ok 0,00 % = linha de base |
| **S3** volume de ofertas | **interino** | ~1 oferta/34 min no início; janela de 24 h não cabe nesta sessão |
| **S4** byte-identidade ON×OFF | `pre-bash` **fraco** · `cli-suggest` **anulado** | `pre-bash` passes=True com **1/200** comparações com conteúdo; `cli-suggest` 0/200 (anulado, passes=None). Predicado corrigido em 18691c14; reexecução real não mudou nenhum número |
| **S5** `touring kpi -j` | **PASSA** | `shadow.status = "measured"`; A1: 16 eventos/16 decisões B-D; A2: 5 eventos/5 decisões B-L |
| **S6** privacidade | **PASSA** | schema fechado + varredura de segredo nos 2 journals |

## 3. Defeitos que o método pegou antes de virarem número

1. O pacote cego quebrava com o formato real de evento (fixture inventada) → refeito com teste de contrato via extrator real.
2. 51 % das rotas estavam cortadas por regex não-gulosa → validado por **dois parsers independentes** (180/180).
3. O gate quebrava com `harm: null` → lógica de três valores (`harm_moot`, `INCONCLUSIVO_SEM_GOLD`, `INCONCLUSIVO_SEM_HARM`).
4. Uma suíte de um par **sobrescreveu 3 resultados reais** (caminho calculado no import) → restaurados de `recovery-1431/`; guarda de isolamento provada por hash + `mtime_ns`.
5. S4 "vazio": harness passava com 0 bytes == 0 bytes → `passes=None` quando toda saída é vazia.
6. S4 "não-vazio": `{}\n` contava como conteúdo → `has_content` parseia o protocolo do hook (RED 12 falhas → 38 verdes; mutação hermética 6/6 mortos). A suíte completa pegou uma regressão minha (saída `0` é JSON válido) **antes** do commit.
7. **Falso achado meu, retratado:** "o emissor A1 emitiu zero eventos" não era defeito. Era taxa-base zero: o daemon reiniciou às 18:09Z, o Pensieve nasceu vazio e o `pre_bash` não injetou nada depois (censo do transcript: 125 injeções antes, 0 depois). Reproduzido o gatilho (1 falha + 1 comando de mesma forma), o emissor gravou o `a1_block` na hora (ver PROVA-PRATICA §2).

## 4. Limitações declaradas (nada disto foi escondido)

- **A1 online cobre só o `pre-bash` (braço Context).** Os 4 pontos de emissão do `cli_suggester` (S-9b) não foram feitos.
- **A amostra online do A1 (16 eventos, 14 `would_drop`) está contaminada por sondas minhas** (as falhas de `ls` ensinaram o Pensieve; as linhas repetidas viram *drop* por dedup). Prova a tubulação, **não** mede eficácia. A eficácia é a do replay (NAO_ADOTADO).
- **B0 online ≠ B0 do replay**: ordena por chegada do hook (atraso do `post-tool-rl` ~3–5 s) e tem TTL de 60 s (Errata 6).
- **S3 interino, S4 não provado ponta a ponta**, e a primeira latência do B-L é 3.621 ms (carga preguiçosa do modelo), depois ~340–380 ms.
- **Teste do A1 = 3 sessões.** O orçamento restante da API Jev (~486 chamadas) não foi gasto.
- **Release 30.4.91 NÃO propagado** aos projetos pinados (analise, konverter seguem na 30.4.90).
- **Juiz de convergência** (15:17, sobre cópia dos arquivos r3): `judge_intact`, `quality_gold` (Gold 0,8737), `no_p0_fail`, `measured_whole_scope` PASSAM; `dag_done` reprova (S-5/S-6/S-12/S-14 abertos); `orphans_base`, `cargo_green`, `cross_audit` são **N/A, não aprovação** (escopo sem `Cargo.toml`; os gates de cargo rodaram no `propagate-release`, rc = 0).

## 5. O que depende do Gabriel

1. **Avaliador 1 não-Claude do A2** (179 itens, `adj_pack/rater_view/`, saída esperada `adj_out/adj_minimax.jsonl`): `/clear` da analise-ef ou sessão MiniMax nova. É o único bloqueio do A2.
2. **Go/no-go do r4** (A3, A4 e Tier B não foram aprovados e **não** foram tocados).
3. Se quer propagar a 30.4.91 aos pinados; se quer parar a unit `r3-shadow-evaluator`; se autoriza gasto da API Jev.

## 6. Candidatos do r4 (advisory — nada é promovido sem o go dele)

- Provar a **polaridade do `noul`** do Laya com casos de verdade independente, antes de qualquer leitura do B-L.
- S-9b: emissão A1 nos 4 pontos do `cli_suggester`.
- S4 forte: entradas que gerem contexto de fato (hoje o scratch limpo dá 0 bytes na maioria).
- Janela S3 de 24 h; B0 online ordenado por evento (não por chegada).
