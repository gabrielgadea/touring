---
type: Results
title: "r3 — Resultados mecânicos (A1 relevância · A2 desfecho de rota)"
description: "Vereditos dos critérios congelados aplicados por r3_results_gate.py (A1 e A2 com gold), provas de entrega S1–S6, defeitos pegos e limitações declaradas"
plan_id: 2026-10-02-jev-harness-r3
okf_version: 0.1
tags: [r3, results, jev, laya, shadow]
timestamp: 2026-10-02T17:00:00-03:00
---

# r3 — Resultados

> Veredito **só** por `scripts/r3_results_gate.py`. Nada abaixo foi decidido por leitura.
> Critérios: [CRITERIOS-ACEITACAO.md](/CRITERIOS-ACEITACAO.md) (Erratas 1–8). Prova viva e reprodução: [PROVA-PRATICA.md](/PROVA-PRATICA.md).

## 1. Vereditos

| Caso | Veredito mecânico | Por quê |
|---|---|---|
| **A1** relevância das injeções | **NAO_ADOTADO_PELOS_DADOS** | C-A1-1 reprova: B-D economiza `bytes_saved` = **0,1094** (piso 0,25) em `test` (262 blocos, 128 EXEMPT, `exempt_retained` = 1,0). C-A1-2 (harm) é *moot*: não muda o veredito, por isso o pacote cego do A1 foi cancelado. |
| **A2** desfecho de rota | **NAO_ADOTADO_PELOS_DADOS** | Com o gold (κ = 0,7707): **B0 concorda 0,906 com o gold; B-J 0,835; B-L 0,706.** Nenhum braço passa C-A2-1 (Δ ≥ +0,05 com IC > 0) nem C-A2-3 (precisão ≥ 0,90 e cobertura ≥ 0,25). Nenhum está "sem potência" (`underpowered` = false). |

**Leitura honesta.** Nos dois casos os dados **não** sustentam adotar um modelo (A1: nem a regra B-D) agora. Isso é diferente de "o modelo não funciona": ver §4 (os braços partiram com **menos informação** que o B0 e o gold).

### A1 — fragilidade que NÃO foi suavizada
B-D economiza 35,7 % em `train`, 21,7 % em `val` e 10,9 % em `test`, e `test` tem **3 sessões**: o veredito mecânico vale, mas a potência é baixa. O `bytes_saved` dos braços de modelo **não foi pré-registrado** e não foi inventado depois.

### A2 — números (população `audit-test`, n = 85; τ de cada braço escolhido em `audit-val`, n = 86)
Gold = 171 itens (156 em acordo + 15 do desempate; 116 `followed` / 55 não); κ(Claude cego × MiniMax) = **0,7707**.

| | acordo c/ gold | Δ vs B0 | IC95 % inferior | precisão | cobertura | τ (viável?) | AUC vs gold | p95 |
|---|---|---|---|---|---|---|---|---|
| **B0** (regra) | **0,9059** | — | — | — | — | — | — | — |
| **B-J** (Jev, features redigidas) | 0,8353 | −0,0706 | −0,1412 | 0,8056 | 0,8471 | 0,46 (**não**) | 0,734 | 0,0 ms (cache) |
| **B-L** (Laya base, CPU) | 0,7059 | −0,2000 | −0,2941 | 0,6988 | 0,9765 | 0,0731 (**não**) | **0,120** | 140 ms |

Cláusulas do gate — B-L: C-A2-1 ✗ · C-A2-2 ✓ · C-A2-3 ✗ · C-A2-4 ✗ (140 > 50 ms; só reportado). B-J: ✗ · ✓ · ✗ · ✓ (**0,0 ms vem de cache**: C-A2-4 passa por vácuo).
**Atenção ao C-A2-2 "✓":** só o estrato `followed` (n = 66) é julgado (n ≥ 20); no estrato `not_followed` (**n = 19, não julgado**) os modelos são muito piores que o B0 (B-L 0,00 · B-J 0,63 · B0 1,00). O ✓ não significa que não houve regressão.
**B0 não é "suficiente"**: 0,906 < 0,95, então não é o desfecho "regex basta".

**Sensibilidade (E8.7, secundária)** — só o gold em acordo (156 itens; teste n = 77): B0 0,922 · B-L 0,766 (Δ −0,156, IC −0,247) · B-J 0,442 (Δ −0,481, IC −0,597). **Mesmo veredito.** O desempate decidiu 15/15 contra `followed` (juiz Claude, mesma família do rater 2); a sensibilidade mostra que o veredito não depende dele.

**Reimplementação independente** (subagente Claude de contexto limpo, só com o texto da Errata 8, sem ver o meu código): **todos os números e o veredito batem**; só o p95 do B-L difere na 2ª casa por método de percentil (140,07 × 140,12 ms), sem efeito. A tarefa também foi enviada ao `touring-4b`, que não confirmou o recebimento.

## 2. Entrega (S1–S6) — release 30.4.91, shadow ligado às 15:09:22 BRT

| Critério | Resultado | Evidência |
|---|---|---|
| **S1a** custo direto | **PASSA** | OFF p99 = 963 ns (limite 50 µs); emissão p99 = 27 µs (limite 1 ms) |
| **S1b** envelope de latência | **PASSA** | `cli-suggest` p95 550 ms ≤ 583; `post-tool-rl` p95 793 ms ≤ 8294 (n = 206 / 200) |
| **S1** original (só informativo, Errata 5) | **reprovaria** `cli-suggest` | 401 → 550 ms (limite 441); declarado |
| **S2** fail-open | **PASSA** | não-ok 0,00 % = linha de base |
| **S3** volume de ofertas | **interino** | ~1 oferta/34 min no início; janela de 24 h não cabe |
| **S4** byte-identidade ON×OFF | **PASSA, forte** em `pre-bash`, `cli-suggest` e `post-bash`; `post-tool-rl` **anulado** (silencioso) | 200 entradas × 3 daemons por projeto (ON/OFF/controle, base virgem, texto de comando idêntico): controle OFF×ctl **0/200** e ON×OFF **0/200** nos 4 hooks; comparações com conteúdo 175 · 57 · 71 · 0; 178 eventos do emissor no ON (174 `a1_block` + 4 `a2_offer_closed`, 33 sessões); 0 hashes do experimento no journal real (que ganhou 6 linhas `a1_block` de 2 sessões vivas, fora do conjunto do experimento). A **rodada 1 foi VOID por defeito do meu harness** (§3.12) |
| **S5** `touring kpi -j` | **PASSA** | `shadow.status = "measured"`; A1 e A2 com eventos e decisões |
| **S6** privacidade | **PASSA** | schema fechado + varredura de segredo nos 2 journals |

## 3. Defeitos que o método pegou antes de virarem número

1. Pacote cego quebrava com o formato real (fixture inventada) → refeito com teste de contrato via extrator real.
2. 51 % das rotas cortadas por regex não-gulosa → validado por **dois parsers independentes** (180/180).
3. O gate quebrava com `harm: null` → lógica de três valores.
4. Suíte de um par **sobrescreveu 3 resultados reais** → restaurados; guarda de isolamento provada por hash + `mtime_ns`.
5. S4 "vazio" passava com 0 == 0 bytes → `passes=None`; S4 "não-vazio" contava `{}\n` como conteúdo → `has_content` (38 testes, 6/6 mutantes).
6. **O `--gold` de `r3_replay_a2.py` era aceito e ignorado** (a comparação ao gold nunca existiu; `agreement: null` sempre) → pontuador `r3_score_a2.py` (20 testes, mutação 18/18; 1 mutante equivalente identificado e trocado), `--gold` ligado a ele.
7. **O verificador da E7.2 reprovou o rater válido** (falso-positivo: `\` de continuação de linha colada no caminho permitido) → corrigido DEPOIS de ver o transcript, divulgado, com teste e mutação 13/13; um inventário independente concordou. O teste do oráculo também iterava a constante testada (mutante M10 sobreviveu) → lista escrita no teste.
8. **Falso achado meu, retratado:** "o emissor A1 emitiu zero eventos" era taxa-base zero (daemon reiniciado, Pensieve vazio: 125 injeções antes, 0 depois); reproduzido o gatilho, o emissor gravou o `a1_block`.
9. **Auditoria cruzada independente (`touring-auditor`, score composto 0,92; recontou 8 números, 6 batem)** — 7 achados, todos verificados antes de agir:
    - *P1 "RESULTS diz GOLD_INSUFICIENTE"*: era uma leitura da versão anterior deste arquivo (16:10); já estava reescrito.
    - *P2 testes Rust viram no-op verde com `TOURING_R3_SHADOW` no ambiente*: **confirmado e maior que o relatado** (9 testes, não 4, incl. o "ctx byte-idêntico") → helper `require_flag_unset()` que falha em voz alta; provado com o ambiente limpo (passa) e forçado (falha).
    - *P2 B-J servido do cache / "quase tudo em 0,5"*: o 2º trecho estava errado (a moda é 0,46: 104/180; já removido) e o 1º foi **parcialmente certo**: ver §4 (25 payloads distintos; origem do cache não registrada).
    - *P3 C-A2-2 aprova por vácuo quando nenhum estrato tem n ≥ 20* (`all([])`): **confirmado** → sem estrato julgável, C-A2-2 = `None` (não medido) e `passes` exige `True`; os vereditos reais não mudam; 18 testes, mutação 2/2.
    - *P3 `MAX_PENDING` descarta oferta sem linha*: **corrigido no código** (68d70d0f): RED com o valor exato do defeito (`left: 0, right: 6`) → 11 testes `r3_` verdes, `fmt` e `clippy -D warnings` limpos. **Só entra no daemon na próxima release** (a 30.4.91 em execução não a tem); a lacuna só aparecia com > 1024 ofertas pendentes.
    - *P3 `results-a1.json` solto em `r3-replay/a2/`*: **falso-positivo do auditor** (listou `a1/` e `a2/` como um só); `a2/` contém só `predictions-a2.jsonl`, `results-a2.json` e `results-a2-gold.json`.
10. **`doctor` `wiring_diagnostic` em `warning` (`kind_unknown=2`), anterior ao r3 — corrigido, não declarado:** eu tinha dito que exigia mexer no extrator e numa release, e estava errado. As duas linhas eram resíduos: `touring wiring repair --purge-phantom` (1 aresta fantasma cujo símbolo é a palavra-chave `super`, de `super::super::…` em `kpi.rs`; o extrator atual já a classifica como `ScopeKeyword`) e `--resolve-kinds` (1 linha). Resultado: `kind_unknown=0`, **`doctor` com os 9 checks `ok`**, sem tocar código.
11. **Achado de desenho do critério:** C-A2-3 escolhe τ na **borda** de "precisão ≥ 0,90" em `val`, então a precisão em `test` regride por acaso até para um modelo bom; o teste sintético aleatório de "modelo bom passa" ficou instável por isso e foi trocado por um mundo determinístico.
12. **S4 forte, rodada 1 VOID — o defeito era do meu harness, e uma causa que eu chamei de "observada" era falsa.** O controle (OFF×ctl, daemons idênticos) divergiu em 164/200 (`pre-bash`) e 67/200 (`post-bash`). Eu havia escrito no E9.8-j/k/l/m que a causa observada era o `try_borrow_mut` do Pensieve: era **leitura de código** e estava errada. Diagnóstico só no controle, sem dado do braço ON (pausa 0,2 s → 7/20 e 4/20; pausa 2,0 s → 4/20 e 3/20; comandos sem caminho → 12/12 iguais; **comando idêntico nos 2 braços → 0/40**): o `rewrite` embutia a raiz de cada braço no texto do comando e o Pensieve (por processo, em memória, `hook_runtime.rs:913`) faz hash do comando inteiro. Corrigido (caminhos relativos; 24 testes; mutação 19/19), pré-registrado antes da rodada 2 (E9.8-n, commit `2f297e58`, 17:40:49) e reexecutado em base virgem: **0/200 em tudo**. A decisão de reexecutar veio **depois** de eu ver os números da rodada 1; a rodada 1 fica no registro. Também corrigi horários que eu tinha escrito errado no pré-registro (os commits provam a ordem) e a justificativa de um teste ("o daemon recusa pedido que nomeia o caminho": o `pin_admits` só compara `project_root`, `crates/touring-dispatch/src/daemon.rs:144-148`; achado do `touring-a4`, que também recontou 13/13 números da rodada 1).

## 4. Limitações declaradas (nada disto foi escondido)

- **B-J: 180 itens colapsam em 25 payloads redigidos distintos** (o maior grupo tem 86 itens, 48 %) e cada payload recebe um único p (12 valores distintos no total): o braço só discrimina entre esses 25 baldes. As respostas são do **cache do cliente Jev** (latência 0,0 ms); a **origem das entradas do cache não está registrada** (`jev_cache.json` é compartilhado e foi tocado hoje às 16:27 por outra sessão). Os números do B-J são, portanto, "respostas do Jev a 25 vetores, vindas do cache", não uma medição de latência nem de chamadas novas.
- **A correção do `MAX_PENDING` (oferta deslocada por capacidade agora fecha 1 linha `replaced`) está no código e testada, mas o daemon em execução (30.4.91) ainda não a tem**: só vale após a próxima release/propagação. Na carga atual (~16 ofertas/dia) o defeito era inalcançável.
- **Os braços foram informacionalmente prejudicados por desenho.** O B-J só recebe features redigidas (família, modo, `closed_by`, contagem, **1º token** de cada comando); o B-L **também** recebeu esse payload redigido, **não** o texto truncado (≤ 200 chars) que o §1 previa para o modelo local; o B0 e o gold veem a janela completa dos comandos. O veredito vale para **esta entrada**, não para modelos com acesso ao texto.
- **B-L não é interpretável como P(seguida).** AUC contra o gold = 0,12 (0,09 só com o gold em acordo): associação inversa sistemática (o padrão canônico `touring` + `consumed` recebe o valor mais baixo, ~0,077). A sonda de polaridade do `touring-a4` (8 casos sintéticos, hipótese pré-declarada) deu AUC **0,500** e o caso mais claramente "ignorado" teve o p mais alto: **hipótese não confirmada**, o `noul` não se comporta como P(seguida). Nenhuma inversão foi aplicada (E8.8). Eu leio o 0,12 como estrutura (não sorteio) e o 0,500 sintético como fora-de-distribuição; as duas leituras ficam registradas.
- **O gold depende de um desempate de mesma família**: o rater 2 (Claude cego) e o juiz de desempate (Claude Sonnet, contexto limpo) são da mesma família do agente julgado (E3.3); o rater 1 foi um subagente MiniMax de contexto limpo da sessão a4 (E3.3-bis; verificado por código, ver Errata 7). O desempate foi 15/15 contra `followed`; a sensibilidade sem ele dá o mesmo veredito.
- **A1 online cobre só o `pre-bash` (braço Context)**; os 4 pontos do `cli_suggester` (S-9b) não foram feitos. A amostra online do A1 (16 eventos) está contaminada por sondas minhas: prova a tubulação, não a eficácia.
- **B0 online ≠ B0 do replay** (ordem por chegada do hook, TTL 60 s). **S3 interino**, 1ª latência do B-L = 3.621 ms (carga do modelo).
- **S4 (rodada 2) é 1 rodada, sem repetição, com comandos de caminho relativo** (E9.8-n/o): o diretório que o `pre-bash` extrai fica raso, igual nos 3 braços; 99/200 comandos ainda levam algum caminho absoluto (39 só `/dev/null`; 60 com outro, como `/tmp/…`; o `touring-a4` contou 92 com um padrão mais estreito), idêntico nos 3 braços; `post-tool-rl` não emitiu nada em nenhum braço (nada a comparar); a máscara mínima (`reliability N%`, `velocity`, `há Ns`) segue aplicada e a rodada sem ela não foi feita.
- **Teste do A1 = 3 sessões; teste do A2 = 85 itens** (IC ≈ ±0,08).
- **Release 30.4.91 NÃO propagado** aos projetos pinados (analise, konverter seguem na 30.4.90).
- **Juiz de convergência (16:40, escopo = cópia dos 34 arquivos r3): `converged: true`, exit 0.** `judge_intact`, `dag_done` (14/14), `quality_gold` (Gold 0,8745), `no_p0_fail`, `measured_whole_scope` passam; `orphans_base`, `cargo_green`, `cross_audit` são **N/A, não aprovação** (escopo sem `Cargo.toml`; o cargo dos crates tocados rodou no `propagate-release`, rc = 0, e a verificação Rust desta rodada está no PROVA-PRATICA).

## 5. O que depende do Gabriel

1. **Veto ou aceite do desvio E3.3-bis / Errata 7** (rater 1 = subagente MiniMax de contexto limpo; o `merge`, o desempate e o veredito já foram calculados com ele — se vetar, o gold cai e o A2 volta a `GOLD_INSUFICIENTE`).
2. **Go/no-go do r4** (A3, A4 e Tier B não foram aprovados nem tocados).
3. Propagar a 30.4.91 aos pinados? Parar a unit `r3-shadow-evaluator`? Autorizar gasto da API Jev (~486 chamadas restantes)?

## 6. Candidatos do r4 (advisory — nada é promovido sem o go dele)

- **B-L com o texto truncado (≤ 200 chars) previsto no §1**, pré-registrado antes de rodar — é a pergunta que este r3 não respondeu; e um tipo de pergunta do Laya que meça "seguida" de forma calibrada (o `noul` não mediu).
- S-9b: emissão A1 nos 4 pontos do `cli_suggester`.
- Janela S3 de 24 h; B0 online ordenado por evento, não por chegada.
- Critério C-A2-3: escolher τ com margem (p. ex. precisão ≥ 0,95 em `val`) em vez de na borda.
