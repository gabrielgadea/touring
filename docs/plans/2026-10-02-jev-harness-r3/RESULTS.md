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
| **A2** desfecho de rota | **NAO_ADOTADO_PELOS_DADOS** | Com o gold (κ = 0,7707): **B0 concorda 0,906 com o gold; B-J 0,835 (oscila 0,80–0,92 entre 3 amostras da mesma API, Errata 11); B-L 0,706.** Nenhum braço passa C-A2-1 (Δ ≥ +0,05 com IC > 0) nem C-A2-3 (precisão ≥ 0,90 e cobertura ≥ 0,25). Nenhum está "sem potência" (`underpowered` = false). |

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
| **S1b** envelope de latência | **NÃO PASSA** (leitura definitiva com o 1º dia ON completo, 03/10 21:01: Errata 15, E15.1; antes: passou com n = 206 em 02/10 ~17h e não passou na releitura de 03/10 00:45, Errata 14) | Antes: `cli-suggest` p95 550 ms ≤ 583; `post-tool-rl` 793 ms ≤ 8294 (n = 206 / 200). **Releitura (n = 3.862 / 3.812): `cli-suggest` p95 598 ms > 583 (não passa); `post-tool-rl` 918 ms ≤ 8294 (passa **pela letra**: o 8294 é a fatia parcial de 25/09, n = 4.956; com o envelope em dias completos, 874, o veredito é **indeterminado**: ON agregado 913 e fatia de 02/10 927 ficam acima, a fatia de 03/10 até agora, 792, fica abaixo; E14.6, E14.7).** Atribuição **inconclusiva** (Errata 14, E14.5; `scripts/r3_latency_by_day.py`, corte UTC do critério): `cli-suggest` p95 por dia sem shadow 569 · **583** (= o envelope) · 543 ms e com shadow 588 (02/10) · 666 (03/10, n = 423), mediana 2–3 ms em todas as fases. No `post-tool-rl` a **mediana subiu** de 33–35 ms (sem shadow) para 39–43 ms (com shadow; mesmo dia 02/10: 34 → 39) e o p95 foi de 729–768 para 927 e 796 ms. A release 30.4.91 e a flag entraram **no mesmo instante** e a máquina esteve a 90–96 °C: nem carga nem release foram controladas. Sem A/B de latência. **Leitura definitiva (E15.1, dia ON completo 03/10 UTC): `cli-suggest` 615 ms (n = 1.623) > 583 em todos os recortes; `post-tool-rl` 724 ms ≤ 874 no dia completo (agregado 885 e fatia parcial 927 ficam acima), sem robustez: há um único dia ON completo.** O A/B está adiado por decisão do Gabriel (só entra se abrir um r5) |
| **S1** original (só informativo, Errata 5) | **reprovaria** `cli-suggest` | 401 → 550 ms (limite 441); declarado |
| **S2** fail-open | **PASSA** | não-ok 0,00 % = linha de base |
| **S3** volume de ofertas | **NAO_DEMONSTRADO** (fechado por código às 15:12 de 03/10; não é PASS) | ~1 oferta/34 min no início; janela de 24 h não cabia na 1ª leitura. Evidência parcial às 18:12: `touring kpi -j` → `shadow.emitter_dropped_since_start = 0`, `foreign_lines = 0`, `unreadable_lines = 0` (140 decisões A1 e 10 A2 avaliadas); 0 é o valor saudável. A dúvida de o `kpi` rodar fora do daemon **fechou pela leitura do código**, não por um valor ≠ 0 observado: `cli_kpi` recebe o `HookRuntime` do daemon e o CLI chama `daemon_query("cli-kpi")` (comentário em `kpi/code_mode.rs:601-602`); o contador volta a 0 se o daemon reiniciar. **Fechou às 15:12 de 03/10 por código, sem depender desta sessão** (Errata 13): `scripts/r3_s3_close.py` (49 testes, mutação 43/43, commits `a035283f`, `98059848`, `8ce499bc` e `61cdba1a`; as emendas da E13.4, da E13.5 e da E13.6 vieram da verificação do `touring-a4`, que também leu o pareamento `offered` × `a2_offer_closed` no código e o confirmou) rodou pelo timer transitório `r3-s3-close.timer` às 15:10 e gravou `r3-data/s3_close.json`. Regra: denominador = ofertas de `code_mode_arm.json` menos a base 485 das 15:15:50; numerador = linhas `a2_offer_closed` cuja oferta é posterior à base; PASS ≥ 0,95; déficit sozinho = `NAO_DEMONSTRADO` (oferta pendente no fim da sessão não emite linha); `REPROVADO` só com perda direta nos 3 contadores do `kpi`. O S3 mede o **emissor**; a unit `r3-shadow-evaluator` não é condição. Ensaio às 20:50 (janela de 5,6 h, `INTERINO`): 11 ofertas, 10 linhas na janela, 1 sem explicação, contadores de perda 0. **Fechamento real (15:12:01 de 03/10, `r3-data/s3_close.json`, janela de 23,936 h): 15 ofertas, 14 linhas `a2_offer_closed`, 1 sem explicação, cobertura 0,9333 (< 0,95) ⇒ `NAO_DEMONSTRADO`** pela regra do déficit sozinho; contadores de perda 0/0/0, daemon sem reinício desde a base (início 1790964561370 ms), 1 sessão distinta (`1ef2a43bf3e2`), `settled = true`. A unit `r3-shadow-evaluator` foi parada pelo próprio script (`stop_rc 0`, `inactive`), como aprovado (veredito ∈ {PASS, NAO_DEMONSTRADO}, perdas 0, daemon intacto). O serviço transitório saiu com status 1 por desenho (exit 0 só no PASS) e o systemd o registra como `failed`. **Leitura honesta:** o emissor não perdeu evento, mas a cobertura exigida não foi atingida, e a oferta sem linha pode ser pendência de fim de sessão — a causa **não foi provada**. Com 1 sessão e 15 ofertas o critério mal consegue passar: tolerar 1 pendente exige N ≥ 20 (`(N−1)/N ≥ 0,95`) |
| **S4** byte-identidade ON×OFF | **PASSA, forte** em `pre-bash`, `cli-suggest` e `post-bash`; `post-tool-rl` **anulado** (silencioso) | 200 entradas × 3 daemons por projeto (ON/OFF/controle, base virgem, texto de comando idêntico): controle OFF×ctl **0/200** e ON×OFF **0/200** nos 4 hooks; comparações com conteúdo 175 · 57 · 71 · 0; 178 eventos do emissor no ON (174 `a1_block` + 4 `a2_offer_closed`, 33 sessões); 0 hashes do experimento no journal real (que ganhou 6 linhas `a1_block` de 2 sessões vivas, fora do conjunto do experimento). **Reproduzido na rodada 3** (réplica pré-registrada E9.8-q, base virgem nova: mesmas contagens por hook, 0 linhas novas no journal real). A **rodada 1 foi VOID por defeito do meu harness** (§3.12) |
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

- **B-J: 180 itens colapsam em 25 payloads redigidos distintos** (o maior grupo tem 86 itens, 48 %) e cada payload recebe um único p (12 valores distintos no total): o braço só discrimina entre esses 25 baldes. As respostas vieram do **cache do cliente Jev** (latência 0,0 ms), e a **origem está registrada** (correção de 02/10 19:3x; eu tinha escrito o contrário): o `jev_journal.jsonl` tem, para as 25 chaves, exatamente 25 chamadas reais (`route_taken: api`, `jev-1.13.0`, 13:54:09–13:59:06) e 358 servidas do cache. **Reconsulta ao vivo (Errata 11, 50 chamadas, todas `api`, 0 `cache`; ~436 do saldo restam):** o Jev **não é estável bit a bit** — o mesmo payload devolve p diferente (±0,01–0,02; 8/25 iguais ao cache na passada 1, 13/25 entre as passadas 1 e 2; máx |Δ| = 0,02, "estável" pela tolerância pré-registrada de 0,05). Como os 12 valores de p ficam numa faixa estreita (0,31–0,50) e o τ cai em 0,46–0,47, esse ruído cruza o limiar: **a concordância do B-J com o gold é 0,835 (cache), 0,918 (passada 1) e 0,800 (passada 2)**, amplitude de 10 itens em 85. **O veredito é o mesmo nas três** (`NAO_ADOTADO_PELOS_DADOS`; C-A2-1 e C-A2-3 falsos em todas: Δ vs B0 −0,071 / +0,012 com IC inferior 0,000 / −0,106; precisão 0,806 / 0,892 / 0,887, abaixo de 0,90). O que **não** se sustenta é a frase "o B-J fica abaixo do B0": ele oscila de 0,80 a 0,92 contra 0,906. Latência real do Jev (n = 50): mediana 368 ms, p95 446 ms, máx 574 ms (teto de API: 800 ms), no lugar do "0,0 ms (cache)" da tabela.
- **A correção do `MAX_PENDING` (oferta deslocada por capacidade agora fecha 1 linha `replaced`) está no código e testada, mas o daemon em execução (30.4.91) ainda não a tem**: só vale após a próxima release/propagação. Na carga atual (~16 ofertas/dia) o defeito era inalcançável.
- **Os braços foram informacionalmente prejudicados por desenho.** O B-J só recebe features redigidas (família, modo, `closed_by`, contagem, **1º token** de cada comando); o B-L **também** recebeu esse payload redigido, **não** o texto truncado (≤ 200 chars) que o §1 previa para o modelo local; o B0 e o gold veem a janela completa dos comandos. O veredito vale para **esta entrada**, não para modelos com acesso ao texto.
- **B-L não é interpretável como P(seguida).** AUC contra o gold = 0,12 (0,09 só com o gold em acordo): associação inversa sistemática (o padrão canônico `touring` + `consumed` recebe o valor mais baixo, ~0,077). A sonda de polaridade do `touring-a4` (8 casos sintéticos, hipótese pré-declarada) deu AUC **0,500** e o caso mais claramente "ignorado" teve o p mais alto: **hipótese não confirmada**, o `noul` não se comporta como P(seguida). Nenhuma inversão foi aplicada (E8.8). Eu leio o 0,12 como estrutura (não sorteio) e o 0,500 sintético como fora-de-distribuição; as duas leituras ficam registradas.
- **O gold depende de um desempate de mesma família**: o rater 2 (Claude cego) e o juiz de desempate (Claude Sonnet, contexto limpo) são da mesma família do agente julgado (E3.3); o rater 1 foi um subagente MiniMax de contexto limpo da sessão a4 (E3.3-bis; verificado por código, ver Errata 7). O desempate foi 15/15 contra `followed`; a sensibilidade sem ele dá o mesmo veredito.
- **A1 online cobre só o `pre-bash` (braço Context)**; os 4 pontos do `cli_suggester` (S-9b) não foram feitos. A amostra online do A1 (16 eventos) está contaminada por sondas minhas: prova a tubulação, não a eficácia.
- **B0 online ≠ B0 do replay** (ordem por chegada do hook, TTL 60 s). **S3 NAO_DEMONSTRADO** (15:12 de 03/10: cobertura 0,9333, 1 oferta sem linha em 15, causa não provada), 1ª latência do B-L = 3.621 ms (carga do modelo).
- **S4: 2 rodadas passam (a 3ª é réplica pré-registrada, contagens idênticas); como entradas e produto são determinísticos, a réplica prova reprodutibilidade, não independência de ambiente. Os comandos entram com caminho relativo** (E9.8-n/o): o diretório que o `pre-bash` extrai fica raso, igual nos 3 braços; 99/200 comandos ainda levam algum caminho absoluto (39 só `/dev/null`; 60 com outro, como `/tmp/…`; o `touring-a4` contou 92 com um padrão mais estreito), idêntico nos 3 braços; `post-tool-rl` não emitiu nada em nenhum braço (nada a comparar); a máscara mínima (`reliability N%`, `velocity`, `há Ns`) segue aplicada e a rodada sem ela não foi feita.
- **Teste do A1 = 3 sessões; teste do A2 = 85 itens** (IC ≈ ±0,08).
- **Release 30.4.91 NÃO propagado** aos projetos pinados (analise, konverter seguem na 30.4.90).
- **Juiz de convergência (16:40, escopo = cópia dos 34 arquivos r3): `converged: true`, exit 0.** `judge_intact`, `dag_done` (14/14), `quality_gold` (Gold 0,8745), `no_p0_fail`, `measured_whole_scope` passam; `orphans_base`, `cargo_green`, `cross_audit` são **N/A, não aprovação** (escopo sem `Cargo.toml`; o cargo dos crates tocados rodou no `propagate-release`, rc = 0, e a verificação Rust desta rodada está no PROVA-PRATICA).

## 5. O que depende do Gabriel

**Decididas por ele em 02/10 (resposta ao canvas de 5 decisões):** (1) E3.3-bis: *"lance um juiz cego sonnet"* — o desvio **segue em aberto** até o resultado desse juiz (Errata 10, §8); (2) **r4 mínimo aprovado**
(só o B-L com texto truncado; feito, §7); (3) a unit `r3-shadow-evaluator` fica **até 15:09 de 03/10** (o que fazer depois foi decidido em seguida; ver abaixo); (4) **não propagar** a 30.4.91 agora; (5) **até ~50 chamadas Jev** (feitas: 50, saldo ~436; §4).
**Resolvidas por ele depois (02/10, mensagem "recomndações aprovadas" ao canvas das 4 decisões; Errata 13):** (1) **E3.3-bis aceito**: o A2 segue `NAO_ADOTADO_PELOS_DADOS`, agora com o desvio aceito, e o risco de gold quase todo Claude continua declarado; (2) **unit**: mantida até 15:09 de 03/10 e parada **depois de o S3 ser lido** (leitura por `r3_s3_close.py`, acima; a unit só é parada se os 3 contadores de perda estiverem em 0 **e** o daemon não tiver reiniciado desde a base, porque o contador é de processo (E13.4); senão fica de pé para investigação); (3) **propagação**: esperar um release com ganho, com o gatilho "release que corrija defeito reproduzido num pinado, ou ganho medido" (o r3 não traz ganho aos pinados: A1/A2 não adotados, emissor default OFF); (4) **r5: não abrir agora** (reabre só com hipótese nova, dele ou do dado sombra). **Nada fica pendente do que foi aprovado.** Sem aprovação e sem toque: A3, A4, Tier B, S-9b, C-A2-3 com margem e qualquer r5.

## 6. Candidatos do r4 (advisory — nada é promovido sem o go dele)

- ~~B-L com o texto truncado (≤ 200 chars) previsto no §1~~ — **respondida pelo r4 mínimo (§7): não passa** (AUC 0,331 no primário). Resta, para um r5 só se ele pedir: um tipo de pergunta do Laya que meça "seguida" de forma calibrada (o `noul` não mediu, com texto ou sem).
- S-9b: emissão A1 nos 4 pontos do `cli_suggester`.
- Janela S3 de 24 h; B0 online ordenado por evento, não por chegada.
- Critério C-A2-3: escolher τ com margem (p. ex. precisão ≥ 0,95 em `val`) em vez de na borda.

## 7. r4 mínimo (Errata 12) — B-L (Laya local) com texto truncado de ≤ 200 caracteres

Pré-registrado em `f9ceb88a` (19:35:43) e rodado depois (19:37:30–19:41:54), sob `hw-slot`, em CPU. Estado do modelo = rota + até 3 comandos seguintes, cada um com espaços colapsados e cortado em 200 caracteres; mesma pergunta `noul` do r3; os mesmos 180 itens congelados (171 no gold); gold registrado; pontuador puro; veredito do gate sobre um braço por vez.

| Braço | checkpoint | concordância | Δ vs B0 (B0 = 0,9059) | IC95 % inf. | precisão | cobertura | τ | AUC vs gold | veredito (gate) |
|---|---|---|---|---|---|---|---|---|---|
| **B-LT-EN** (primário) | inglês (ModernBERT-large, 512/192) | **0,3176** | −0,5882 | −0,7176 | 0,000 | 0,000 | 0,9998 | **0,331** | `NAO_ADOTADO_PELOS_DADOS` |
| B-LT-ML (exploratório) | multilíngue (mmBERT-base, 1024/256) | 0,3059 | −0,6000 | −0,7294 | 0,429 | 0,082 | 0,9997 | 0,551 | `NAO_ADOTADO_PELOS_DADOS` |
| B-L do r3 (referência) | inglês, payload redigido | 0,7059 | −0,2000 | −0,2941 | 0,699 | 0,977 | 0,0731 | 0,120 | `NAO_ADOTADO_PELOS_DADOS` |

**Leitura (a regra estava fixada antes, E12.5):** o AUC do primário ficou abaixo de 0,5 (0,331: o p é **maior** nos "não seguidos", 0,79, do que nos "seguidos", 0,72), então o `noul` zero-shot do Laya **não mede "seguida", nem com texto nem sem**, e o r4 mínimo fecha aqui. O texto melhorou o AUC (0,120 → 0,331) sem torná-lo útil. A concordância baixa **não** é p degenerado (167 valores distintos entre 0,16 e 0,9998): vem da regra de ponto de operação da Errata 8, que só acha τ com precisão ≥ 0,90 em `val` na cauda extrema (cobertura de 1,2 %). A probabilidade a priori que registrei antes de ver (< 0,2) condiz. O B-LT-ML tem AUC 0,551, que é acaso; um braço secundário não sustenta a hipótese (E12.2).

**Divergência do r3 descoberta no caminho, medida e não só lida (E12.1, E12.8):** o B-L do r3 rodou o checkpoint **inglês** (`laya.Agent` sem `subfolder` reporta `encoder: answerdotai/ModernBERT-large`, 512/192, temperaturas [1,637; 1,251; 1,983] que o próprio `laya` avisa serem inválidas), não o "multilíngue" que o pré-registro nomeia. O veredito do A2 não muda; o resultado do B-L do r3 vale para o checkpoint inglês com o payload redigido.

**Limitações:** 1 rodada; só a pergunta `noul` do r3 foi testada (não as outras cabeças do checkpoint); o texto é cortado em 200 caracteres (pelo levantamento do `touring-a4`, 408 de 534 comandos do corpus passam disso); latência em CPU com o chip a 91–94 °C (mediana: inglês 1.068 ms, multilíngue 390 ms; p95 do inglês 1.250 ms, acima do teto de 800 ms que o gate usa; o C-A2-4 só é reportado).

## 8. Juiz Sonnet cego e sensibilidade ao E3.3-bis (Errata 10)

Pedido por ele (*"lance um juiz cego sonnet"*). **Tentativa 1 reprovou** na verificação mecânica (falso positivo de prefixo de caminho; rótulos descartados sem leitura, E10.7); **tentativa 2 válida** (exit 0, 0 violações, `claude-sonnet-5-5`, 0 `Agent`). O gold registrado e o veredito primário **não mudam**; isto é só sensibilidade.

| leitura | n do gold | B0 | B-J | B-L | veredito (gate) |
|---|---|---|---|---|---|
| gold registrado (R1 × R2 + desempate) | 171 | 0,9059 | 0,8353 | 0,7059 | `NAO_ADOTADO_PELOS_DADOS` |
| maioria de 3 (R1, R2, R3) | 176 | 0,9080 | 0,8391 | 0,6897 | `NAO_ADOTADO_PELOS_DADOS` |
| unanimidade entre os votos válidos (156 com os três votando + 4 com uma abstenção) | 160 | 0,9231 | 0,4487 | 0,7564 | `NAO_ADOTADO_PELOS_DADOS` |

**κ por par:** R1 (MiniMax) × R2 (Claude) **0,7707** · R1 × R3 (Sonnet) **0,7818** · R2 × R3 **0,9867**. **Pela regra fixada antes (E10.4), o A2 é "robusto ao E3.3-bis" no sentido da regra:** os três vereditos do gate coincidem e κ(R1,R3) ≥ 0,60 (isto não afirma nada sobre o risco de ponto cego compartilhado, que segue declarado, abaixo).

**O que o resultado mostra além disso (não estava no desenho):** (1) o R3, cego, concorda com o gold registrado em **171/171** (R2 170/171; R1 157/171), inclusive nos 15 itens do desempate, que ele decidiu igual; (2) o rater 1 (MiniMax), o único de fora da família, tem **viés direcional bem evidenciado nesta amostra**: 134 itens "seguida" (R2 117, R3 118) e, nos 16 itens em que diverge do R3, **diz "seguida" 16 de 16** (p ≈ 1,5×10⁻⁵ sob a moeda justa); a taxa não é estimada por 16 itens de 177, e ele também foi o que menos se absteve (1, contra 7 e 2). Como κ(R2,R3) = 0,987, os dois Claude pesam como um voto só.

**O que isto NÃO resolve:** o risco de ponto cego compartilhado da E3.3. O gold registrado é quase todo rótulo da família do agente julgado, e este juiz também é Claude. O veredito não depende do rater 1, mas a independência de família do gold continua sendo uma limitação declarada, e a decisão sobre o E3.3-bis segue com ele.
