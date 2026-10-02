---
type: Criteria
title: "Critérios de aceitação pré-registrados — r3 (A1 e A2 em shadow)"
description: "Métricas, braços, limiares e regras de decisão do r3, fixados ANTES de qualquer extração, replay ou chamada de modelo."
plan_id: 2026-10-02-jev-harness-r3
tags: [pre-registro, criterios, jev, laya, shadow, a1, a2]
timestamp: 2026-10-02T12:45:17-03:00
okf_version: "0.1"
---

# Critérios de aceitação pré-registrados — r3

> **Registrado em 02/10/2026 12:45:17 BRT** (relógio do sistema, lido por `date` imediatamente
> antes de escrever), **ANTES** de qualquer extração de dados, replay ou chamada de modelo do r3.
> Regra herdada do S-1 do r2: depois de ver o número, um critério reescrito é racionalização com
> formato de critério. **Única exceção:** ERRATA DE MÉTRICA — erro na *definição do que se mede*
> (nunca no limiar), datada pelo relógio real, registrada ANTES de uma re-execução e citando o erro.
> Parte do plano: [plan](/plan.md) · [estratégia](/strategy-2026-10-02-jev-harness-r3.md).

## 0. Cinco lições do r2 aplicadas aqui

1. **Métrica antes de limiar.** O S-1 reprovou por "acurácia global ≥ 0,90", que é invariante sob T;
   a métrica certa era precisão@suportado. Aqui cada métrica tem **fórmula** (§1), e o limiar só
   vale sobre ela.
2. **O modelo tem de bater um baseline trivial.** Um arm determinístico (regra) e o classificador
   atual (regex) são os adversários; um modelo que não os supera **não é adotado**, mesmo "funcionando".
3. **Resultado negativo é resultado.** Falhar um critério fecha o braço como "não adotado pelos
   dados"; a instrumentação entregue permanece (S1–S6 são critérios de ENTREGA, independentes).
4. **Congelar por hash.** Datasets e splits são gravados com sha256 em `data/MANIFEST.sha256` ANTES
   do primeiro replay; cada relatório cita o hash do que leu. Gravar não é congelar: um `--check`
   inter-processos prova que o hash segue o mesmo.
5. **Adjudicação independente de quem é julgado.** O agente que gerou as ações é da família Claude;
   os adjudicadores são de **outras famílias** (Kimi K3 e MiniMax), independentes entre si.

## 1. Definições formais

**Evento de oferta (A2).** Uma emissão, pelo `cli_suggester`, de um nudge que propõe uma rota
sancionada (família `touring-suggest`/`code-mode-*`). **Rótulo de desfecho** — lido por um
adjudicador sobre: o texto do nudge + as **próximas 3 chamadas de ferramenta** da mesma sessão
(texto de comando truncado a 200 chars):

| rótulo | significado |
|---|---|
| `followed` | nas 3 chamadas seguintes o agente executou a ação recomendada (mesma rota ou equivalente semântico) |
| `ignored` | executou outra coisa e a ação recomendada não aparece |
| `bypass` | executou a ação por rota NÃO sancionada (ex.: `python3` inline em vez de `touring run`) |
| `indeterminate` | não dá para decidir com as 3 chamadas → **excluído** das métricas, contado à parte |

**Baseline A2 (B0).** O classificador vigente `classify_route_outcome` (regex + limiar), lido como
está no commit `b8d6f8ed`, sem alteração.

**Bloco de injeção (A1).** Um evento `attachment` com `hook_additional_context`/`hook_success` nos
transcripts de sessão do touring (campo de conteúdo injetado), classificado por **família** (assinatura
de 1ª linha, a taxonomia de `explore-v2.md`: `touring-suggest`, `code-mode-deny`, `loop-outer`,
`past-lessons`, `session-start`, `post-tool-echo`, `other`). **Proposições** = o que o KPI
`context_budget` já conta (a definição única é a do código de `context_budget`; este plano NÃO a
redefine). **Followed (A1)** só existe para blocos de nudge com rota (herda o rótulo da A2).

**EXEMPT (A1).** Nenhum braço pode descartar: blocos de família `code-mode-deny`; qualquer bloco que
carregue marcador `MUST`, `REGRA #`, `CRITICAL`, `BLOCK`/`deny`, erro de hook ou permissão. A lista
definitiva de marcadores é congelada junto com o dataset (`data/exempt_markers.json`, hash no
manifesto). **Um único bloco EXEMPT descartado por um braço invalida esse braço.**

**Split.** Temporal 60/20/20 (train/val/test) por `timestamp` do evento, seed `20261002` para
desempates; nenhum evento de uma sessão cruza partições (a sessão é a unidade de partição).
**Audit sample (A2):** n ≥ 240 sorteado por seed `20261002`, estratificado por família/modo
(≥ 30 por estrato quando o estrato tiver ≥ 30 eventos), dividido 40% `audit-val` / 60% `audit-test`.

**Braços.** `B0` status quo (nada descartado; classificador vigente) · `B-D` regra determinística
(A1: descartar bloco com 0 proposições e não-EXEMPT, e duplicata exata na janela de 5 turnos) ·
`B-L` Laya local (checkpoint multilíngue base, zero-shot + τ calibrado em val) · `B-J` Jev API
(zero-shot, **só features redigidas**, τ calibrado em val). Modelos NÃO são treinados nem
ajustados com rótulos do r3 (só τ é calibrado, em val).

**Política de dados.** Laya local e adjudicadores veem texto truncado (≤ 200 chars por comando; os
adjudicadores já estão no mesmo perímetro de dados da sessão). O **Jev API só recebe features
redigidas**: família, classe da ferramenta, 1º token do comando, caminhos substituídos por hash,
contagens e tamanhos — nunca texto livre de prompt ou de comando. Teste de contrato (§4, S6) varre
isso por regex.

## 2. A2 — desfecho de rota e recompensa

Unidade: evento de oferta com rótulo ≠ `indeterminate`. Gold = acordo entre os dois adjudicadores
independentes; desacordo → desempate registrado por Claude (Sonnet). **Pré-condição do gold:** κ de
Cohen entre os dois adjudicadores ≥ 0,60 no audit sample; abaixo disso o gold é declarado
INSUFICIENTE e o A2 não conclui (sem número de modelo) até re-adjudicar.

| id | critério (sobre `audit-test`, τ congelado em `audit-val`) | limiar |
|---|---|---|
| **C-A2-1** | acordo(melhor arm modelo, gold) − acordo(B0, gold) | **≥ +0,05**, com limite inferior do IC95% (bootstrap pareado, 10 000 reamostragens, seed 20261002) **> 0** |
| **C-A2-2** | nenhuma regressão por estrato com n_test ≥ 20: acordo(modelo) − acordo(B0) | **≥ −0,03** em TODOS |
| **C-A2-3** | precisão@coberto do modelo, com τ escolhido em `audit-val` para maximizar cobertura sujeita a precisão ≥ 0,90 | **precisão ≥ 0,90 E cobertura ≥ 0,25** em `audit-test` |
| **C-A2-4** | latência do arm (fora do caminho quente): p95 local · p95 API | **≤ 50 ms · ≤ 800 ms** (reportado; falhar é limitação, não reprova a qualidade) |

Custo da API reportado (alvo ≤ US$ 0,05 por 1 000 eventos). **Se B0 já tiver acordo ≥ 0,95 com o gold,
o A2 fecha como "regex basta" e nenhum modelo é adotado** — resultado válido.

## 3. A1 — relevância das injeções

| id | critério (sobre a partição `test`; τ congelado em `val`) | limiar |
|---|---|---|
| **C-A1-1** (B-D) | bytes injetados economizados · followed-recall · EXEMPT retidos | **≥ 25%** · **= 1,00** · **= 100%** |
| **C-A1-2** (harm de B-D) | taxa de "inofensivo descartar" nos blocos que B-D descartaria — professor/adjudicadores cross-vendor, amostra estratificada n = 100 | limite inferior do IC95% **≥ 0,90** |
| **C-A1-3** (incremento do modelo) | B-L ou B-J economiza a mais, sobre B-D, com followed-recall ≥ 0,95 | **≥ +10 pontos percentuais** de bytes |
| **C-A1-4** (informativo) | melhor arm alcança `injected_bytes_per_turn` ≤ 3 000 B (o teto do KPI `context_budget`) | reportado, sem pass/fail |

Se C-A1-1 e C-A1-2 passam e C-A1-3 **não**: o ganho é da regra determinística e o modelo **não é
adotado para A1** (resultado válido; a economia do B-D segue elegível a advisory). Dois adjudicadores
cross-vendor julgam C-A1-2 de forma independente; o limite inferior usa a união conservadora (o
**pior** dos dois).

## 4. Shadow online — critérios de ENTREGA (independentes da qualidade do modelo)

| id | critério | prova |
|---|---|---|
| **S1** | caminho quente intacto: p95 de `cli-suggest` com shadow ON ≤ 1,10 × p95 dos 7 dias anteriores **ou** ≤ +20 ms (o que for maior) | `hook_trace.jsonl`, antes/depois, mesmo script |
| **S2** | fail-open: com o avaliador parado/inalcançável, 0 erros de hook atribuíveis ao shadow e distribuição de `exit_reason` inalterada | derrubar o avaliador e medir |
| **S3** | cobertura: ≥ 95% dos eventos elegíveis (nudges emitidos e desfechos) deixam 1 linha de evento em 24 h | contagem emitidos × linhas |
| **S4** | **byte-idêntico**: `additionalContext` com shadow ON == shadow OFF para 200 entradas reais de hook reproduzidas | diff = 0 bytes |
| **S5** | KPI visível: `touring kpi -j` expõe n_eventos, acordo, descartes, latência p50/p95 por `use_case` | saída do comando |
| **S6** | privacidade: nenhuma linha de evento tem texto livre > 80 chars de prompt/comando; o payload do Jev API contém só features redigidas | teste de contrato por regex |

Flag de ativação **default OFF** (`TOURING_R3_SHADOW`). O shadow NUNCA altera o que é injetado nem o
que é classificado em produção: ele só observa e registra.

## 5. Estados finais

- **ENTREGUE** ⟺ S1–S6 passam **e** os testes novos passam **e** o juiz `--rust-full` converge.
- Por braço científico, um de: `PASSA` (todos os C-* do braço) → candidato a advisory num r4, **só
  com o go do Gabriel**; `PARCIAL` (registra quais falharam e por quê); `NÃO ADOTADO PELOS DADOS`
  (C-*-1 ou C-*-3 falhou) → instrumentação fica, comportamento de produção não muda.
- Nenhum limiar muda depois do primeiro replay. Errata de métrica segue o protocolo do cabeçalho.

## 6. Fora do r3

Não treinar especialista, não promover nada a advisory/gate, não propagar a projetos pinados,
não tocar em A3 (comando destrutivo), A4 (roteamento de modelo) nem no Tier B. O r4 decide, com os
números deste.

---

# Errata 1 — 02/10/2026 12:48:12 BRT — registrada ANTES de qualquer extração, replay ou chamada de modelo

> Motivo: duas definições do §1 não correspondem ao código. Verificado em
> [harness-scan-t03](/harness-scan-t03.md) §1.1 e §2.1 e por leitura direta de
> `cli_suggester.rs:4377-4392`, `:4482-4510`, `:4533-4545` e `:4880-4895`.
> **Nenhum limiar muda. Nenhum braço foi executado.** Divulgação: os volumes agregados por família
> do relatório M8 (`explore-v2.md`) já eram conhecidos e orientaram a escolha do ESCOPO (E1.1);
> nenhum resultado de braço existia. É errata de MÉTRICA (o que se mede), no sentido do cabeçalho.

**E1.1 — A1, unidade de análise.** "Bloco de injeção" passa a significar **apenas blocos emitidos por
hooks do Touring**, não os do harness do Claude Code nem de outros plugins. Operacionalmente: as
famílias `touring-suggest`, `code-mode-deny`, `loop-outer`, `past-lessons`, `session-start` e
`post-tool-echo` (tabela de `context_budget.rs:141-178`). A família `other` é o **catch-all do KPI**
(`context_budget.rs:184`), e seus maiores blocos são a listagem de skills do harness (≈ 90 KB nos 3
transcripts medidos) e o buffer do plugin REMEMBER (≈ 22 KB) — o Touring não os emite e nenhum gate
dele pode descartá-los. Os denominadores de C-A1-1 e C-A1-3 passam a ser os bytes dessas seis
famílias. C-A1-4 continua sobre o total do KPI, agora com a **decomposição Touring × harness**
explícita no relatório.

**E1.2 — A2, o baseline é um detector de 1ª ocorrência, não um classificador de 3 classes.**
`classify_route_outcome` é chamado em cada Bash seguinte e, ao devolver `None`, **não consome a
oferta** (`cli_suggester.rs:4885-4886`); só devolve `Some` para `touring run `/`touring exec ` (valor
1,0, ou 0,5 se o programa for trivial) ou para um token de bypass por env (valor 0,0). Ele nunca
rotula "ignorou". Definição de **B0**: `classify_route_outcome` aplicado *sequencialmente* aos
próximos **K = 3 comandos Bash** da mesma sessão; o primeiro `Some(v)` decide (`v > 0` ⇒ followed;
`v = 0` ⇒ not-followed); sem nenhum `Some` ⇒ not-followed. **A métrica primária de A2 passa a ser o
acordo BINÁRIO em `followed` × `not-followed`** (gold: `followed` contra {`ignored`, `bypass`}); o
acordo em 3 classes fica como secundário. Sem isto, B0 seria penalizado por construção por uma
classe que ele não tem.

**E1.3 — A2, estratificação do audit sample.** Estratificar pela **decisão de B0** (followed /
not-followed) em partes iguais (120/120 de n = 240), não por modo: só o modo `code` tem ofertas
(`code_mode_arm.json`: `both` e `native` = 0), e o que importa é incluir os casos em que B0 pode
errar. C-A2-2 passa a ser por estrato de B0 (n_test ≥ 20), com o mesmo limiar (≥ −0,03).

**E1.4 — A2, janela.** K = 3 chamadas **Bash** (não "3 ferramentas quaisquer"), da mesma `session_id`,
cortada na próxima oferta da mesma sessão (uma oferta nova substitui a anterior:
`record_route_offer` insere pela chave da sessão, `:4536`).

---

# Errata 2 — 02/10/2026 12:56:50 BRT — registrada ANTES de existir qualquer rótulo de adjudicação

> Esclarece COMO o κ e o gold são calculados (a pré-condição do §2 não dizia sobre qual conjunto de classes).
> **Nenhum limiar muda** (κ ≥ 0,60 segue valendo). Fixado em `scripts/r3_adjudication.py` (6 mutantes mortos) antes de
> qualquer pacote ser gerado; nenhum rótulo existia.

**E2.1 — κ sobre o binário primário.** O κ de Cohen entre os dois adjudicadores é calculado sobre
`followed` × `not-followed` (a mesma métrica primária da Errata 1, E1.2). `ignored` × `bypass` **não** é
desacordo. Um κ de 3 classes é reportado como secundário.
**E2.2 — `indeterminate`.** Item que qualquer dos dois adjudicadores marca `indeterminate` é excluído do κ e
do gold e **contado** (`n_indeterminate`).
**E2.3 — gold e desempate.** Gold = acordo dos dois (Kimi K3 e MiniMax, cegos entre si e à decisão de B0);
cada desacordo vai à fila de desempate e é fechado por um terceiro juiz (Claude Sonnet), registrado com
`provenance = tiebreak`. Desempate `indeterminate` mantém o item fora do gold.
**E2.4 — cegueira.** O pacote entregue aos adjudicadores contém só `item_id`, texto do nudge e os ≤ 3 comandos
(`scripts/r3_adjudication.py::pack`); a decisão de B0 vive num arquivo-chave que eles não recebem; a ordem é
embaralhada por seed.

---

# Errata 3 — 02/10/2026 13:06:55 BRT — registrada ANTES de existir qualquer rótulo de adjudicação

> Motivos (todos verificados, nenhum depende de resultado de braço): (i) o extrator achou **198 ofertas
> (136 followed / 62 not-followed por B0) em 16 sessões** — o 1º relatório de 114 era bug de recall
> (âncora `^` no marcador; o Claude Code embrulha o deny em `PreToolUse:Bash hook error: `), corrigido com teste de
> recall; os transcripts só existem desde 02/09 (retenção), então 26/08–01/09 é irrecuperável; (ii) o
> **Kimi K3 esgotou a cota semanal** (403 às 12:57) — um dos dois adjudicadores deixou de existir.
> **Nenhum limiar muda.** Esta errata só fixa o TAMANHO da amostra e a COMPOSIÇÃO dos adjudicadores.

**E3.1 — amostra = todas as ofertas.** O "n = 240, 120/120" da E1.3 é inatingível (o estrato `not-followed`
tem 62). Não há amostragem: **todas as 198 ofertas são adjudicadas** (sem variância de amostragem). A estratificação
por decisão de B0 passa a valer para a DIVISÃO `audit-val`/`audit-test` (40/60, seed 20261002, por sessão),
garantindo `not-followed` nas duas metades.
**E3.2 — potência (declarada antes do dado).** Com ≈ 119 itens em `audit-test`, o IC95% do delta de acordo
tem meia-largura ≈ ±0,07; logo C-A2-1 (delta ≥ +0,05 com limite inferior > 0) só passa se o efeito real for
folgado. Para não rotular falta de potência como "o modelo não funciona", o veredito ganha o estado
**`INCONCLUSIVO_POR_POTENCIA`**: nenhum braço passa C-A2-1 por causa do IC, **mas** algum braço tem delta ≥ +0,05.
Ele NÃO promove nada (equivale a "sem adoção"); só diz que a amostra não decide. Ordem dos vereditos:
PASSA > PARCIAL > INCONCLUSIVO_POR_POTENCIA > NAO_ADOTADO_PELOS_DADOS.
**E3.3 — adjudicadores.** Substituição do Kimi, **divulgada como limitação**: rater 1 = MiniMax em sessão **nova**
(a que não portou o B0 e, portanto, não conhece a lógica do classificador); rater 2 = **subagente Claude em contexto
limpo, cego à decisão de B0** (mesma família do agente julgado — risco de ponto cego compartilhado, mitigado pelo
κ reportado e pelo desempate registrado). Se uma segunda sessão MiniMax/outra família ficar disponível a tempo,
ela substitui o rater 2 antes de qualquer rótulo existir. O κ ≥ 0,60 (§2) e o resto do protocolo (E2.1–E2.4) valem
sem mudança; o relatório final cita explicitamente esta limitação.
