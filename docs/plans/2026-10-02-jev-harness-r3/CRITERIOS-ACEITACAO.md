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
*(Nota de 02/10, E9.8-p: "NUNCA" é o enunciado de desenho e não foi medido. O que o S4 mediu é saída byte-idêntica com a flag ligada, nos 4 hooks, em
200 entradas de 1 rodada (RESULTS §2); o resto é projeto mais testes. O "NUNCA" fica revogado como afirmação empírica.)*

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

---

# Errata 4 — 02/10/2026 13:20:22 BRT — registrada ANTES de existir qualquer rótulo e ANTES da re-extração

> Motivos (medidos pela coordenação ao verificar o dataset congelado, antes de gerar o pacote cego; nenhum
> depende de resultado de braço). **(i) Falsos positivos de oferta**: dos 199 candidatos nos transcripts, 179
> são `tool_result` com `is_error = true` (deny genuíno) e **20 são o marcador CITADO** na saída de outro comando
> (`is_error = false`) — a prova comportamental do `propagate`, `grep` sobre transcripts, e até a saída da própria
> sessão de coordenação (auto-contaminação). Quatro dos falsos **têm rota casada**: a regex
> `touring run --lang … --code '…'` sozinha não discrimina. **(ii) Truncagem**: o B0 foi calculado sobre a janela
> completa, mas o dataset guarda `cmds[:200]`; reaplicando o B0 sobre o texto guardado, **9 de 198 decisões mudam**
> (todas `followed → not-followed`). O adjudicador cego discordaria do B0 por apresentação, não por mérito da regra;
> e um braço de modelo que lê o mesmo texto truncado ganharia vantagem de visão compartilhada sobre o B0.
> **Nenhum limiar muda.**

**E4.1 — definição de oferta.** Oferta = `tool_result` com `is_error == true` cujo conteúdo contém o marcador
`[CODE MODE · rajada]` (o deny do G1 é erro de hook por construção). Citação (`is_error = false`) **não é oferta**.
Esta definição substitui "o marcador aparece no texto" (Errata 1/E3.1). "Todas as ofertas são adjudicadas" (E3.1)
segue valendo, sobre a população CORRIGIDA; os "198 = 136/62" da Errata 3 ficam superados e o n real é reportado
após a re-extração. As candidatas excluídas por citação são **contadas** (`n_excluded_quoted`), nunca descartadas
em silêncio.
**E4.2 — texto completo.** `cmds` e `rota_proposta` passam a ser guardados **sem truncagem** (em `r3-data/`, fora
do repo). Invariante guardada por teste e por mutação: `B0(cmds guardados) == b0_decisao guardado` para todo item,
de modo que a decisão do B0 e o que o adjudicador vê vêm do MESMO texto. Cada braço de modelo pode truncar ao
próprio contexto (Laya: 1024 tokens) — isso é parte do que o braço é, e é reportado.
**E4.3 — saída para terceiros.** O pacote vai a um vendor externo; como o texto deixa de ser truncado, passa por
`r3_event_schema.scan_privacy` antes de sair. Item com segredo detectado não é enviado: é contado
(`n_withheld_privacy`) e fica fora do gold.
**E4.4 — o que NÃO muda.** Limiares, κ ≥ 0,60, E2.1–E2.4, os estados de veredito da Errata 3 e a divisão
`audit-val`/`audit-test` (40/60, seed 20261002, por sessão) — recomputada sobre a população corrigida. A potência
declarada em E3.2 (≈ ±0,07) vale como ordem de grandeza e é recalculada com o n final antes de qualquer resultado.

## Errata 4 — adendo, 02/10/2026 13:25:14 BRT — ainda ANTES de existir qualquer rótulo

> Duas coisas, ambas achadas pela verificação independente do dataset re-congelado. **Nenhum limiar muda.**

**E4.3 (retificada) — o instrumento de privacidade citado estava ERRADO.** A E4.3 acima manda passar o pacote por
`r3_event_schema.scan_privacy`. Esse detector proíbe **texto livre** em linha de shadow — isto é, marcaria 100% dos itens
de um pacote cujo conteúdo É texto — e não procura credenciais. O instrumento correto é
`r3_adjudication.secret_findings` (8 famílias de credencial; varre o texto BRUTO, não o `json.dumps`, que escapa as
aspas e esconde `"token": "…"`), aplicado em `pack` à `rota_proposta` e aos `cmds`. Item com achado é **retido**,
**contado** (`withheld.json`: item → nomes dos padrões, jamais o valor) e fica fora do gold. Provado com controle
positivo por padrão e 16/16 mutantes mortos (`scripts/test_r3_adjudication.py`); sem controle positivo, "0 retidos"
não seria medida.
**E4.5 — a `rota_proposta` estava cortada em 51% dos itens.** O extrator lia o programa com `'(.*?)'`; o deny o embute em
shell-quoting, com aspa interna escapada `'\''`, e a regex fechava na primeira. Medido nas 180 ofertas genuínas: **91
(51%) com a rota cortada** e 1 com rota vazia. Mesma família da E4.2 (o avaliador vê ≠ o que o nudge ofereceu). O B0 não
lê a rota, então a decisão de B0 não muda; mas o avaliador cego e os braços de modelo a leem. Correção: extrair o
programa como shell-quoting (`'[^']*'` e `\'`), com re-extração e re-congelamento **antes** do pacote. Invariantes de
aceite: `rota cortada em '\''` = 0 e `rota None/vazia` = 0.
**E4.6 — n provisório.** Após E4.1–E4.2 o dataset tem **n = 180 (132 followed / 48 not-followed; 66/24 em cada metade)**,
verificado de forma independente. Os "198 = 136/62" da Errata 3 e o "≈ 119 itens em `audit-test`" da E3.2 ficam
superados: `audit-test` tem 90, e a meia-largura do IC do delta passa a ≈ ±0,08. A E3.2 já previa exatamente isso (o
estado `INCONCLUSIVO_POR_POTENCIA` existe para a amostra que não decide). O n é refeito se E4.5 mudar a população.

## Errata 4 — adendo 2, 02/10/2026 13:38:51 BRT — ainda ANTES de existir qualquer rótulo

> Fecha a E4.5 com a medição final e registra duas regras novas. **Nenhum limiar muda.** O re-congelamento
> (n = 180, 132 followed / 48 not-followed, 66/24 em cada metade) foi verificado de forma independente.

**E4.5 (retificada) — o invariante de aceite estava mal especificado.** Eu escrevi "rota cortada em `'\''` = 0 e rota
vazia = 0": tratava o token de escape do shell-quoting como evidência de corte. A medição desfez isso. Um parser de
shell-quoting PRÓPRIO (que desfaz o escape) e a gramática final do extrator (`--code '(.*)'\n`) concordam na EXTENSÃO
do programa nas 180 ofertas: 89 rotas idênticas e 91 que diferem só na representação (`guardada == escape(minha)`),
**0 outras**. O invariante correto é esse — concordância entre dois parsers independentes — e vale como aceite. A
rota guardada preserva `'\''`; o texto **não** é reescrito para o avaliador (o braço de modelo lê o texto guardado:
todos os leitores veem o mesmo), e o `INSTRUCOES.md` ensina a leitura. Seis rotas terminam em `'\''` porque o próprio
programa termina numa aspa.
**E4.7 — oferta com programa vazio.** 1 oferta (`a2-00019`) traz literalmente `--code ''` no deny (o programa fundido
não coube no orçamento): é dado, não falha de extração. Sem ação recomendada não há contra o que julgar os comandos:
o item sai do pacote, é **contado** (`withheld.json`, motivo `empty_route`) e fica fora do gold — o mesmo tratamento do
`indeterminate` (E2.2). A regra olha o TEXTO da oferta, nunca a decisão de B0, e foi fixada antes de existir rótulo.
`rota_proposta = None` (a extração não achou o programa) segue recusando o pacote inteiro.
**E4.8 — o congelamento tem de ser estável.** O teste `test_check_detects_a_single_changed_byte` do S-4 executa o
congelamento REAL sobre o corpus vivo (e de novo no `finally`): fora do sandbox, regrava a amostra e o manifesto a cada
execução (observado: `audit_sample.jsonl` regravado às 13:34:46, conteúdo idêntico por hash). Como os `item_id` são
posicionais, a primeira oferta nova renumera tudo e embaralharia o mapeamento rótulo→item. Medidas: (i) cópia
verificada com hashes em `r3-data/frozen-verified-*/`; (ii) o gold é juntado por `key.jsonl` (que carrega
`session`+`ts`), nunca por posição; (iii) o teste passa a ser hermético (diretório temporário) por pedido ao dono.

---

# Errata 5 — 02/10/2026 13:53:41 BRT — registrada ANTES de o shadow ser ligado: não existe dado online algum

> O S1 pré-registrado não pode falhar por causa do shadow e deixa sem medição o hook que mais executa o
> observador. Evidência (só dados de BASE, `~/.claude/touring/hook_trace.jsonl`, 474.924 linhas desde 01/09, nenhum
> com o shadow ligado). **Nenhum limiar de qualidade (A1/A2) muda**; muda como a ENTREGA é provada.

**Medição 1 — onde o observador A2 executa.** `record_route_offer` roda no hook `cli-suggest`
(`cli_suggester.rs:5695`; n = 30.012 em 7 dias, p95 = 395 ms). `claim_route_reward` roda no hook **`post-tool-rl`**
(`post_tool_rl.rs:298`, a CADA Bash; n = 26.812, p95 = 875 ms). O S1 só nomeava o primeiro.
**Medição 2 — o ruído natural do p95.** p95 por dia (n ≥ 200): `cli-suggest` **122 a 596 ms** (amplitude 389%);
`post-tool-rl` **315 a 3.244 ms** (amplitude 930%). O limite do S1 (1,10 × o p95 da janela anterior, ou +20 ms) está dentro
dessa variação: o p95 de hoje (518 ms) já excede o limite (≈ 434 ms) **sem nenhum shadow**. Comparar janela com janela
com margem de 10% reprovaria ou aprovaria por acaso, nunca pelo shadow — não falsifica nada.

**E5.1 — S1 cobre os DOIS hooks** (`cli-suggest` e `post-tool-rl`), e S2 idem (não-ok ≤ +0,5 p.p. sobre a base, que é
0,00% nos dois).
**E5.2 — S1 é substituído por dois testes que PODEM falhar:**
 - **S1a (custo direto):** a emissão de 1 evento num arquivo real tem **p99 ≤ 1 ms** (alvo do plano) e o caminho com a
   flag OFF tem **p99 ≤ 50 µs**. Medido por teste Rust `#[ignore]` (`r3_emission_cost_is_bounded`) no perfil **debug**:
   um limite superior conservador (debug é mais lento que release; se passa em debug, passa em release).
 - **S1b (sanidade fim a fim):** com o shadow ligado e n ≥ 200 por hook, o p95 de cada hook fica **≤ o maior dos 7 p95
   diários anteriores** (o envelope natural observado). Pega uma interação patológica, não +10%; S1a prende o custo
   fino. O S1 original (janela contra janela, 1,10×) continua sendo REPORTADO, só não decide.
**E5.3 — S4 tem o alcance dito** *(a prova standalone foi ANULADA pela E6.1, porque o standalone não emite `cli-suggest`; a prova de ponta a ponta é a rodada 2, E9.8-n/o)*: o modo standalone (`TOURING_NO_DAEMON=1`) prova que a flag não altera o texto
injetado nas 200 entradas reais; o estado da oferta vive na memória do processo, então o CICLO DE VIDA (oferta → desfecho)
só existe no daemon e sua identidade é provada pelos testes
`r3_claim_route_reward_is_identical_with_the_flag_on` e `r3_emission_failure_never_changes_the_return` (cada um com
mutante morto), não por S4.
**E5.4 — S3 tem o denominador dito:** elegíveis = ofertas que FECHAM (consumed/window/replaced) numa janela de 24 h
com o shadow ligado; uma oferta pendente quando a sessão acaba não emite linha por desenho e é contada à parte
(`pending_at_end`), nunca escondida.

---

# Errata 6 — 02/10/2026 15:17:24 BRT — registrada DEPOIS do resultado do S4 (e por isso só pode ENFRAQUECER afirmações)

> O shadow está ligado desde 15:09:22 (daemon 30.4.91). O S4 foi executado e **não foi demonstrável** pelo método
> pré-registrado. Esta errata declara o alcance real do que foi provado; **não move nenhum limiar, não troca um
> não-resultado por aprovação e não toca A1/A2.**

**E6.1 — S4 (byte-idêntico) NÃO provado de ponta a ponta contra o binário real.** Rodado com as 200 entradas reais,
no modo standalone, em projeto de rascunho novo por execução, com controle OFF×OFF (determinístico, 0 divergências):
`cli-suggest` devolveu **0 bytes nas 200 entradas** (e também 0 bytes para `cat` de código, `grep -rn` e `find`, que
no daemon geram nudge: o standalone não emite para esse hook) ⇒ prova **ANULADA** (`passes: null`, exit 2);
`pre-bash` devolveu 1 saída não-vazia em 200 ⇒ "passa", mas com UMA comparação com conteúdo, sem força. A injeção do
`pre-bash` depende de falha anterior do mesmo comando, e semear essa falha pelo `post-bash` num projeto limpo não a
reproduziu. **O que sustenta a identidade**: por construção (o contexto entra por referência imutável; a decisão de
produção é calculada antes e devolvida por valor) e os testes de unidade com mutante morto (A1: chamada em `pre_bash`,
valor de `bytes` e do sha da 1ª linha; A2: decisão de produção intocada, gate da flag, janela K). Um S4 forte exigiria
A/B de dois daemons por projeto (um com a flag ON, outro OFF) com as mesmas entradas — fica para o r4.
**E6.2 — S3 mede-se pelo contador durável de ofertas**, `code_mode_arm.json:code.offered` (base 485 às 15:15:50,
`r3-data/s3_baseline.json`), contra as linhas `a2_offer_closed`; ofertas naturais são ~16/dia, então a janela de 24 h
do S3 pré-registrado NÃO cabe nesta sessão: o que se reporta é o intervalo medido, declarado como interino.
**E6.3 — o B0 online não é o B0 do replay.** O `post-tool-rl` roda com atraso (~3-5 s) e a janela do shadow ordena por
CHEGADA do hook: o `cmd_heads` de um evento real incluiu o comando anterior à oferta; a produção também tem TTL de 60 s
e chave de ator. O replay ordena pelo transcript (`classify_route_outcome` puro). Os dois medem coisas próximas, não
idênticas, e o relatório não os mistura.
**E6.4 — latência do braço local online.** A 1ª decisão do B-L carrega o modelo (3.621 ms, carga preguiçosa); as
seguintes levam ~377 ms. A latência do B-L é reportada sobre decisões aquecidas, com a 1ª separada.

---

# Errata 7 — 02/10/2026 16:25 BRT — registrada ANTES de existir qualquer rótulo do rater 1

> Motivo: o rater 1 da E3.3 ("MiniMax em sessão **nova**") não existe. A analise-ef recusou, o Kimi está sem cota
> e nenhuma sessão MiniMax nova foi aberta; sem ele o A2 fica `GOLD_INSUFICIENTE` indefinidamente. Hoje
> `adj_out/` contém só o rótulo do rater 2 (Claude cego); **nenhum arquivo do rater 1 existe** (conferido por `ls`
> às 16:23). Esta Errata não altera limiar, métrica nem protocolo de κ: muda só QUEM e COMO, e **só enfraquece**.

**E7.1 — desvio proposto.** Rater 1 = **subagente de contexto limpo lançado pela sessão MiniMax `touring-a4`**,
que herda o modelo MiniMax. O prompt do subagente cita APENAS `rater_view/INSTRUCOES.md` e
`rater_view/pack_blind.jsonl` e manda gravar `adj_out/adj_minimax.jsonl`. A sessão-mãe NÃO rotula, NÃO resume itens
e NÃO influencia o subagente além desse prompt. O que a E3.3 protege — o rater não conhecer a lógica do
classificador B0 — vale para o subagente (contexto vazio), mas a garantia passa a ser **de contexto, não de
sessão**: a mãe portou o B0. Isto é **limitação E3.3-bis**, divulgada no relatório final.

**E7.2 — verificações mecânicas, ANTES de o rótulo entrar no `merge`** (qualquer falha invalida o rater e o A2
permanece `GOLD_INSUFICIENTE`; **não se repete com o mesmo rater**):
(a) o transcript do subagente é varrido por leitura/menção de caminhos proibidos: `key.jsonl`, `withheld.json`,
`adj_claude.jsonl`, `a2_offers`, `audit_sample`, `predictions-a2`, `results-a2`, `r3-replay`, `r3_b0`,
`r3_replay_a2`, `r3_arms`, `r3_results_gate`, `RESULTS.md`, `CRITERIOS-ACEITACAO`, `frozen-verified`;
(b) o modelo registrado no transcript do subagente tem de ser MiniMax (não Claude);
(c) o formato é validado pelo `r3_adjudication.py merge` (schema fechado, 179 itens, rótulos no domínio).

**E7.3 — o resto vale sem mudança**: κ ≥ 0,60 (§2), E2.1–E2.4, desempate meu cego ao B0, `--gold` no replay A2,
veredito só por `r3_results_gate.py`. O relatório final cita E3.3-bis, o fato de o rater 2 ser da mesma família do
agente julgado (E3.3) e o de a mãe do rater 1 conhecer o B0.

**E7.4 — veto.** Se o Gabriel vetar o desvio, o rótulo do rater 1 é descartado e o A2 volta a `GOLD_INSUFICIENTE`.

**E7.5 — execução e achados (escrito às 16:25 BRT, DEPOIS da rotulagem do rater 1; nada aqui altera limiar, métrica ou
protocolo de κ).**
(a) **Rater 1** = subagente `a9694…` da sessão MiniMax `touring-a4`; modelo `MiniMax-M3.1-Flash-Preview` nos 93 turnos;
54 chamadas (Read 35, Bash 18, Write 1, **0 `Agent`**); 179 rótulos (followed 134 · bypass 25 · ignored 19 ·
indeterminate 1); sha256 `f1ee9d457c47a158`. O prompt dele foi escrito pelo a4, não por mim (diz "os 18 que você já
rotulou na memória", falso para contexto vazio) e não contém B0 nem rótulo algum.
(b) **Tentativa anterior sem rótulo no arquivo**: o 1º subagente (`af11…`) rotulou 18 itens só em contexto, delegou lotes a
sub-subagentes via `Agent` (o prompt que enviei não proibia delegação) e parou sem gravar. Nenhum rótulo dele entrou no
arquivo final. O relato do a4 ("transcript de 148 B") diverge do arquivo real (923 KB, 19 turnos): vale o transcript.
(c) **Falso-positivo do verificador, achado e corrigido DEPOIS de ver este transcript**: 8 violações, todas o arquivo
PERMITIDO `pack_blind.jsonl` com uma `\` de continuação de linha colada. Correção mínima (commit 04e8ff40), teste
dedicado e mutação 11/11; um inventário independente (outro regex) confirmou que o rater só tocou os 3 arquivos e
rascunhos em `/tmp`. Re-varredura: válido, 0 violações.
(d) **κ (Claude cego × MiniMax) = 0,7707** (≥ 0,60): 179 itens, 172 pontuáveis, 7 `indeterminate` fora; **156** com acordo
(gold), **16** em desacordo (14 bypass×followed, 1 ignored×followed, 1 followed×bypass).
(e) **E7.6 — desempate.** Eu já fui o rater 2 e vi a fila com os dois rótulos; desempatar eu mesmo faria o gold dos 16
refletir o rater 2. O desempate foi feito por **subagente Claude (Sonnet) de contexto limpo**, que recebeu só o
`INSTRUCOES.md` e os 16 `item_id`, validado pelo mesmo verificador (`--model claude --allow-file
adj_out/adj_tiebreak.jsonl`). Limitação divulgada: mesma família do agente julgado e do rater 2 (E3.3).

---

# Errata 8 — 02/10/2026 16:40 BRT — regras de pontuação do A2 contra o gold, registradas ANTES de qualquer número braço×gold

> Motivo: o `--gold` de `r3_replay_a2.py` era aceito e **ignorado** (`args.gold` nunca é lido; `_dump` sempre grava
> `agreement: null`); a comparação ao gold (S-6) nunca foi implementada. Estado verificado às 16:3x: o gold existe
> (171 itens: 156 acordo + 15 desempate; κ = 0,7707), as predições dos braços existem (180 itens, 0 erros, geradas
> ANTES do gold) e **nenhuma comparação braço×gold foi calculada**. Esta Errata fixa o que o código fará; não altera
> limiar nem critério (§2), só preenche o que o §2 deixa implícito.

**E8.1 — população.** Gold = itens com `followed` ∈ {True, False} (acordo dos dois ou desempate; `indeterminate` e
`empty_route` fora, E2.2/E4.7). C-A2-1..3 medem-se na partição `audit-test`; τ de cada braço é escolhido em
`audit-val` (campo `split` do audit sample congelado).
**E8.2 — predição.** Braço de modelo: "followed" ⇔ `p_followed ≥ τ`; B0: a própria decisão. Acordo = fração igual ao gold.
**E8.3 — τ.** Grade = os valores distintos de `p_followed` em `audit-val`. τ = o que **maximiza a cobertura**
(fração de itens de val com `p ≥ τ`) sujeita a **precisão ≥ 0,90** (precisão = fração `followed` no gold entre os
cobertos); empate → o maior τ. **Inviável** (nenhum τ dá precisão ≥ 0,90): C-A2-3 reprova por construção
(`precision`/`coverage` reportados no τ de reserva) e o τ de reserva é o que **maximiza o acordo em val** (empate →
maior τ). Cobertura 0 ⇒ `precision = 0,0` (nunca `null`: o gate não aceita ausência).
**E8.4 — B0 suficiente.** `b0.agreement` = acordo do B0 com o gold em `audit-test` (população do §2); o acordo na
totalidade do gold é reportado ao lado, informativo.
**E8.5 — IC.** Bootstrap pareado sobre os itens de `audit-test`, 10 000 reamostragens, `random.Random(20261002)`;
`delta_ci_low` = o valor de posto `int(0,025·N)` dos deltas ordenados.
**E8.6 — estratos.** Por decisão de B0 (`followed` / `not_followed`) em `audit-test`, com `n_test`, `agreement_model`
e `agreement_b0`; o gate só julga estratos com n_test ≥ 20.
**E8.7 — sensibilidade (secundária, nunca muda o veredito).** Tudo repetido só com os itens de gold de proveniência
`agree` (sem o desempate): o desempate foi 15/15 contra `followed` (mesma família do rater 2), e esta leitura mostra
quanto o resultado depende dele.
**E8.8 — diagnóstico de polaridade (não é critério).** AUC de `p_followed` contra o gold, por braço, em todo o gold.
Nenhuma inversão é aplicada: se o AUC vier ≪ 0,5 o braço é reportado como está e a polaridade vai para o r4.
**E8.9 — latência** de `predictions-a2.jsonl` (p95 sobre as predições sem erro). **E8.10 — ligação:** `--gold` do
replay passa a chamar o pontuador (`scripts/r3_score_a2.py`, puro, sem chamada de modelo nem de API).

---

# Errata 9 — 02/10/2026 17:20 BRT — S4 forte (A/B de daemons por projeto), registrada ANTES de qualquer saída ON/OFF/controle

> Motivo: a E6.1 deixou o S4 "fraco/anulado" (standalone, projeto limpo → quase tudo `{}`) e disse que um S4 forte exigiria
> A/B de dois daemons por projeto. O mecanismo (W12.5) existe e está em uso (daemons por projeto de konverter e analise,
> verificado por `daemon-ctl list-all`). Esta Errata muda COMO o S4 é medido; limiares e demais critérios ficam como estão.
> Achado de operação: `touring init-project --root X` **ignora `--root`** e inicializa o diretório atual; o scaffold roda
> sempre com `cwd` = o projeto.

**E9.1 — desenho.** Três projetos descartáveis idênticos (`/tmp/r3s4/{on,off,ctl}`), toolchain 30.4.91 (a que tem os
emissores), `[daemon] per_project = true`, `[code_mode] mode = "code"`, diferindo SÓ em `[shadow] r3` (true / false /
false). Cada projeto tem o próprio daemon (auto-spawn pelo 1º hook); nenhum estado é compartilhado.
**E9.2 — entradas.** As 200 entradas reais de `r3-data/s4_inputs.jsonl` (sha 7af62ab0390fb933), mesma ordem e mesmos
`session_id`, `cwd` = raiz do projeto. Para cada entrada, em lockstep (ON, OFF, controle, sem pausa), os hooks `pre-bash` e
`cli-suggest` pelo caminho do daemon; a saída é comparada byte a byte.
**E9.3 — validade.** O controle (OFF × controle) tem de ser byte-idêntico em todas as comparações; senão o resultado é
**VOID** (`passes = None`) e nenhuma diferença é atribuída à flag (o dedup "há Ns" é relógio de parede: se for a causa,
é reportado como causa, nunca contornado).
**E9.4 — força** (por hook, com `has_content` do commit 18691c14): `n_nonempty ≥ 50` ⇒ **forte**; 1–49 ⇒ **fraco**; 0 ⇒ **anulado**.
**E9.5 — a flag tem de ter efeito no braço ON.** Pelo menos 1 evento (`a2_offer_closed` ou `a1_block`) gravado pelo daemon
ON; sem isso o A/B não diz nada sobre o emissor e é reportado como "emissor ocioso".
**E9.6 — isolamento do journal real.** O experimento NÃO pode escrever no `r3_events.jsonl` do shadow vivo. Uma sondagem de
3 entradas com `session_id` únicos roda ANTES, e o journal real é varrido pelos hashes desses ids; se algum evento do
experimento cair lá, o experimento é ABORTADO (e a contaminação, declarada com os hashes).
**E9.7 — veredito.** S4-forte `passes` ⇔ controle idêntico E ON == OFF em todas as comparações; a classificação de força
vem da E9.4. Se resultar "forte", substitui a classificação "fraco/anulado" da Errata 6; se resultar VOID ou fraco, o
S4 segue como estava e o motivo é registrado. Limpeza: daemons parados com `daemon-ctl stop --socket` (REGRA #19), nunca `pkill`.

**E9.8 — adendo de operação (17:35 BRT; ainda ANTES de qualquer comparação ON × OFF).** Fatos medidos ao montar o A/B e as
regras que decorrem deles; nenhum muda limiar nem veredito.
(a) **Hooks comparados:** `pre-bash`, `cli-suggest`, `post-bash` e `post-tool-rl` (os quatro que uma chamada Bash toca), cada um
com a sua `n_nonempty`/força (E9.4); o veredito do S4 forte é o AND dos quatro, e a força é reportada por hook.
(b) **O daemon por projeto recusa pedidos que nomeiam caminhos fora da sua árvore** (`daemon.rs:1905-1922`, `pin_admits`). Regra
única para os três braços: `/home/gabrielgadea/projects/touring` → `<raiz>`, `/home/gabrielgadea` → `<raiz>/home`,
`cwd` = `<raiz>`, `transcript_path` = `<raiz>/t.jsonl`; a ÚNICA normalização da saída é `<raiz>` → `@ROOT@`.
(c) **Ambiente:** todo `TOURING_*` da sessão é removido, exceto `TOURING_PILLAR_INDUCTION_ARMED`; `CLAUDE_PROJECT_DIR=<raiz>` (como
o Claude Code faz; sem ela o cliente infere a raiz errada); só o ON recebe `TOURING_R3_EVENTS_FILE=/tmp/r3s4/on-events.jsonl`
(verificado: o daemon herda a variável no spawn). `touring init-project --root X` **ignora `--root`** (usa o cwd).
(d) **`session_id`** prefixado com `s4ab-` (E9.6 busca no journal real os hashes dos ids prefixados).
(e) **Desfecho dos comandos:** `post-bash` e `post-tool-rl` recebem `tool_response.exit_code = 1` se `sha256(comando)[0] % 3 == 0`, senão
0 (≈ 1/3 falha), de modo que o Pensieve aprende e o `pre-bash` injeta contexto como em produção. Verificado antes, num 4º projeto
descartável (`probe`, fora do A/B): um payload de falha faz o `pre-bash` seguinte devolver "Similar command failed recently
(similarity=100%)". O payload `exitCode`+`is_error` NÃO foi lido como falha pelo `post-bash`; vale `exit_code`.
(f) **Ordem:** para cada entrada e cada hook, ON, OFF, controle, nessa ordem e sem pausa. Eventos do ON contados = crescimento do
arquivo de sondagem NA FASE MEDIDA (o aquecimento é excluído). Emissor ocioso (0 eventos) ⇒ `passes = None`, nunca `True`.
(g) **Divulgação das sondagens exploratórias anteriores ao registro:** 6 e 20 entradas pelo daemon do `on` para descobrir (b)-(c)
(8 saídas não vazias de `cli-suggest` em 20, 2 eventos no arquivo de sondagem), e o 4º projeto para (e). Nenhuma comparação ON×OFF foi feita.
(h) **Ferramenta:** `scripts/r3_s4_ab.py` (18 testes; mutação hermética 12/12). Limpeza: `daemon-ctl stop --project`, nunca `pkill`.
**E9.8-i (17:40 BRT)** — A rodada real usa projetos VIRGENS em `/tmp/r3s4b/{on,off,ctl}`: o 1º conjunto (`/tmp/r3s4`) recebeu as sondagens
exploratórias de (g) — o daemon, a sessão e o banco do `on` já tinham estado enquanto `off`/`ctl` eram novos — e foi descartado
(daemons parados com `daemon-ctl stop --project`, SIGTERM; os daemons global, de konverter e de analise ficaram intactos).
**E9.8-j/k/l/m (17:55 BRT — ainda ANTES de qualquer comparação ON × OFF da rodada real).** Duas verificações curtas do encanamento (8
entradas, em projetos descartáveis `/tmp/r3s4c`) devolveram **VOID**: o controle (OFF × ctl, braços idênticos) diferiu. Causas
observadas, não supostas: (1) `post-bash` imprime estatísticas de relógio/banco (`reliability N% (7d)`, `velocity V edits/day`) que variam
entre daemons idênticos, e o dedup do `cli-suggest` carrega `há Ns`; (2) `post-bash` registra a falha no Pensieve com
`try_borrow_mut` e **pula** o registro quando o Pensieve está emprestado (`post_bash.rs:204-215`), então três daemons com as mesmas
entradas aprenderam falhas diferentes (o `pre-bash` injetou "Similar command failed recently" no `ctl` e não em `on`/`off`).
**(j) Mascaramento mínimo:** só `reliability \d+%`, `velocity [\d.]+ edits/day` e `há \d+s` (além de `<raiz>` → `@ROOT@`); NUNCA a
decisão (`similarity=N%`) nem o orçamento (`N B`). **(k) Ordem:** rotação ON/OFF/ctl por entrada (a ordem fixa alinhava um braço ao 1º/último).
**(l) Pausa:** 0,2 s após cada chamada, para o trabalho assíncrono do daemon assentar. **(m) Leitura "vs ruído" (SECUNDÁRIA):** contagens
pareadas ON×OFF, ON×ctl e OFF×ctl; `noise_excess = max(ON×OFF, ON×ctl) − OFF×ctl`; por hook, "indistinguível do ruído" ⇔ excesso ≤ 2 + 25 % de
OFF×ctl, senão "excede o ruído"; o agregado é "indistinguível" só se todos os hooks forem e o ON tiver emitido eventos. **O S4 estrito (E9.7)
não muda** — controle diferente ⇒ VOID — e a leitura "vs ruído" é reportada ao lado, **nunca promovida a PASS do S4**.
**Ferramenta:** `scripts/r3_s4_ab.py` (23 testes; mutação hermética 17/17).

**E9.8-n (02/10/2026, escrito às 17:40 -03:00 — depois da rodada 1, antes da rodada 2).**
**(1) Resultado da rodada 1** (17:15:03 → 17:23:50; 200 entradas; 182 eventos do emissor no braço ON; journal real: 0 linhas e 0 hashes de
contaminação; arquivo `s4-ab-run1-void.json`, sha256 `84b67229148ad1c9…`): **VOID** (`passes = None`) por E9.3. Por hook (n não vazias · ON×OFF ·
ON×ctl · OFF×ctl): `pre-bash` forte (187) 153·147·**164**; `cli-suggest` forte (57) **0·0·0** (PASS estrito isolado); `post-bash` forte (99)
68·59·**67**; `post-tool-rl` anulado (0). Leitura secundária "vs ruído" (E9.8-m): indistinguível (excessos −11, 0, +1, 0), nunca promovida.
**(2) Retratação.** O E9.8-j/k/l/m chamou de "observadas" duas causas do ruído do controle; a segunda (`try_borrow_mut` pulando o registro no
Pensieve, `post_bash.rs:204-215`) era **leitura de código**, não observação, e foi **falsificada**. Diagnóstico exploratório (projetos
descartáveis `/tmp/r3s4d|e|f`, SÓ OFF × ctl, nenhum dado do braço ON): pausa 0,2 s → `pre-bash` 7/20 e `post-bash` 4/20 divergem; pausa 2,0 s
→ 4/20 e 3/20 (a pausa não é a causa; latência p95 13–22 ms, sem timeout); comandos inéditos sem caminho, repetidos 3× em cada daemon →
12/12 idênticos; **com o texto do comando idêntico nos dois braços → 0/40 nos quatro hooks** (`pre-bash` 21 não vazias, `post-bash` 12,
`cli-suggest` 9), na mesma pausa de 0,2 s. **Causa:** o `rewrite` embutia a raiz de cada braço (`/tmp/r3s4b/off/…` × `/tmp/r3s4b/ctl/…`) no
texto do comando; o Pensieve (por processo, em memória, sem persistência — `hook_runtime.rs:913` `Pensieve::new(32).with_threshold(0.4)`)
grava o comando INTEIRO como hash por token (`post_bash.rs`, FIX6), então daemons idênticos aprenderam vetores diferentes (similaridade
43/46/47 %, limiar 0,4 cruzado por um e não pelo outro) e, como o estado só cresce, nunca reconvergiram. O defeito era do **harness**, não do
produto nem da flag. A máscara (j) e a pausa (l) ficam como estão (a máscara cobre só relógio/estatística de banco; não testei a rodada sem ela).
**(3) Correção de horários.** Os rótulos "17:35", "17:40" e "17:55 BRT" do E9.8, E9.8-i e E9.8-j/k/l/m **não vieram do relógio** e estão errados.
O que prova a ordem são os commits (`-03:00`): `8d82fc77` 17:03:02 (E9.1–E9.7), `f0e0d1e9` 17:09:00 (harness + E9.8), `39f8f733` 17:09:35
(E9.8-i), `ca0d2559` 17:13:10 (E9.8-j/k/l/m); a rodada 1 começou às 17:15:03. A ordem declarada ("antes de qualquer comparação") era verdadeira;
os horários, não.
**(4) Regra da rodada 2** (única mudança de procedimento): o texto do comando é **byte-idêntico nos três braços** — caminhos reais viram
**relativos** (`/home/gabrielgadea/projects/touring/x` → `x`; `/home/gabrielgadea/x` → `home/x`), e só `cwd` e `transcript_path` carregam a raiz
do braço. Base nova e virgem `/tmp/r3s4g/{on,off,ctl}`; a rodada 2 grava `s4-ab.json`. **Não mudam:** E9.3, E9.4, E9.5, E9.7, os limiares, as
200 entradas (`s4_inputs.jsonl`, sha256 `7af62ab0390fb933…`), a pausa de 0,2 s, a rotação, a máscara, a leitura "vs ruído".
**(5) Divulgação.** A decisão de reexecutar foi tomada **depois** de ver os números da rodada 1, inclusive ON×OFF; o diagnóstico usou só o
controle. A rodada 1 permanece no registro: se a rodada 2 também for VOID ou fraca, o S4 fica como estava e o motivo é escrito.
**Ferramenta:** `scripts/r3_s4_ab.py` (24 testes; mutação hermética 19/19; o mutante "raiz do braço dentro do comando" é morto por teste).

**E9.8-o (02/10/2026, escrito às 17:51 -03:00 — depois da rodada 2; só correções e divulgação, nenhum limiar muda).**
**(1) Rodada 2** (início 17:40:58, fim 17:49:48, de `r3-s4-ab-run2.{start,log}`; `s4-ab.json` sha256 `4f1d3ecbc4ca9e3d…`): `passes = true`; controle
OFF×ctl **0/200** e ON×OFF **0/200** nos quatro hooks; não vazias 175 (`pre-bash`) · 57 (`cli-suggest`) · 71 (`post-bash`) · 0 (`post-tool-rl`, anulado);
178 eventos do emissor no ON (174 `a1_block` do `pre-bash` + 4 `a2_offer_closed`: `replaced` 3, `window` 1; 33 sessões); nenhum hash do experimento no journal real. As 6 linhas que o journal real ganhou na janela são `a1_block` de duas
sessões vivas (hashes fora do conjunto do experimento, nenhuma cita `r3s4`).
**(2) Correção do E9.8-b.** `pin_admits` compara só a raiz do projeto do pedido (`project_root`) com a árvore do daemon
(`crates/touring-dispatch/src/daemon.rs:144-148` e `:1915`; o `daemon.rs:1905-1922` do E9.8-b omitia o crate) e **nunca lê o texto do comando**. A
reescrita dos caminhos do comando serve à identidade entre braços e a não deixar hook ler o repositório vivo, não ao pin. Achado do `touring-a4`, verificado por mim.
**(3) Proveniência de horários.** O "17:15:03" da rodada 1 é a hora do lançamento no transcript da sessão; o JSON da rodada 1 não grava tempo, e o fim
(17:23:50) está em `r3-s4-ab.log`.
**(4) Limitações do desenho da rodada 2.** Comandos com caminho relativo deixam raso o diretório que o `pre-bash` extrai (`extract_dir`,
`pre_bash.rs:347-352`, leitura do `touring-a4`), igual nos 3 braços; 99/200 comandos ainda levam algum caminho absoluto (39 só `/dev/null`; 60 com outro, como `/tmp/...`;
o `touring-a4` contou 92 com um padrão mais estreito), idêntico nos 3 braços. `TOURING_R3_EVENTS_FILE` existe só no ON porque é o tratamento: um eco desse caminho na saída apareceria como diferença
ON×OFF, e não houve nenhuma. É 1 rodada, sem repetição.
**(5) Conferência cruzada.** O `touring-a4` (MiniMax, outra sessão) recontou 13 números do E9.8-n(1) contra o JSON da rodada 1 (13/13 iguais; sha256 completo
`84b67229148ad1c954b15353a6a99bd4aca207aa9df61b073881e7f8e7f15564`) e os horários dos commits `8d82fc77`, `f0e0d1e9`, `39f8f733`, `ca0d2559` (iguais). O
docstring do `rewrite` e um comentário de teste afirmavam a razão errada do E9.8-b e foram corrigidos; o comportamento do código não mudou desde `2f297e58`.
**(6) Conferência da rodada 2 pelo `touring-a4`:** todos os números do E9.8-o(1), do `RESULTS` e da `PROVA` batem com o JSON (sha256 completo
`4f1d3ecbc4ca9e3df7457719b6f7bddf500fca39d854de2b8ef8201304d0922d`), com `/tmp/r3s4g/on-events.jsonl` (178 linhas) e com o diff do `31aa69ed` (só docstring e um
comentário de teste). Dois trechos antigos diziam mais do que o medido e ganharam nota, sem reescrever o registro: o "NUNCA" do desenho (E9.8-p, na seção de
flag) e o E5.3 (anulado pela E6.1).

**E9.8-q (02/10/2026, escrito às 18:02 -03:00 — antes da rodada 3).** Réplica da rodada 2 para tirar a ressalva "1 rodada, sem repetição". Nenhuma
mudança de procedimento: mesmo código (`2f297e58`, mais o docstring do `31aa69ed`), mesmas regras do E9.8-n, mesmas 200 entradas
(`s4_inputs.jsonl` sha256 `7af62ab0390fb933…`), base nova e virgem `/tmp/r3s4h/{on,off,ctl}`, saída `s4-ab-run3.json`. **Regra de leitura fixada antes:** o S4 só é
"reproduzido" se a rodada 3 também der `passes = true` (controle OFF×ctl 0/200, ON×OFF 0/200, `on_events ≥ 1`, 0 contaminação do journal real). Se der VOID ou
qualquer diferença, os dois resultados são reportados lado a lado, o S4 passa a "passou em 1 de 2 rodadas" e a diferença é investigada, nunca contornada.
**Resultado da rodada 3** (início 18:02:04, fim 18:10:55; `s4-ab-run3.json` sha256 `7e9e8cf01be1299b…`): `passes = true`, reproduz a rodada 2 — controle OFF×ctl 0/200 e ON×OFF 0/200 nos 4 hooks, não vazias 175 · 57 · 71 · 0, as MESMAS contagens por hook da rodada 2; 178 eventos no ON (174 `a1_block` + `a2_offer_closed` `replaced` 3 / `window` 1, 33 sessões); 0 linhas novas no journal real e 0 hashes do experimento. **Alcance:** entradas e produto são determinísticos, então a réplica prova reprodutibilidade em estado virgem, não independência de ambiente.

---

# Errata 10 — 02/10/2026 19:26 -03:00 — registrada ANTES de existir qualquer rótulo do juiz Sonnet cego

> Motivo: o Gabriel respondeu ao item 1 do canvas de decisões (E3.3-bis) com "lance um juiz cego sonnet". Leio como: um **3º avaliador cego**, Claude Sonnet de
> contexto limpo, rotula os 179 itens do pacote; serve de **análise de sensibilidade** ao desvio E3.3-bis. **Não substitui o gold registrado, não revoga nem
> aceita o E3.3-bis (segue em aberto, decisão dele), e o veredito primário continua o do gold registrado.** Conferido por `ls` às 19:26: `adj_out/` contém só
> `adj_claude.jsonl`, `adj_minimax.jsonl`, `adj_tiebreak.jsonl`; **`adj_sonnet3.jsonl` não existe**.

**E10.1 — desenho.** Juiz R3 = subagente (ferramenta `Agent`, `model: sonnet`, contexto limpo), cego a: decisão do B0, rótulos dos outros avaliadores, `key.jsonl`,
gold. Prompt EXATO (a sessão-mãe não rotula, não resume itens e não influencia além dele): *"Leia
`/home/gabrielgadea/.claude/touring/r3-data/adj_pack/rater_view/INSTRUCOES.md` e rotule os 179 itens de
`/home/gabrielgadea/.claude/touring/r3-data/adj_pack/rater_view/pack_blind.jsonl` seguindo exatamente as instruções. Grave uma linha JSON por item em
`/home/gabrielgadea/.claude/touring/r3-data/adj_out/adj_sonnet3.jsonl`, no formato {"item_id": "...", "label": "..."}, e confirme no fim a contagem de linhas
(deve ser 179). Regras: não leia nenhum outro arquivo e não liste outros diretórios; não use a ferramenta Agent nem delegue a ninguém; não pule nem resuma
itens; rotule cada item você mesmo, lendo-o."* É da **mesma família** do agente julgado (limitação E3.3, não resolvida por este juiz).

**E10.2 — verificação mecânica, ANTES de qualquer uso do rótulo** (falha ⇒ R3 inválido, a análise de sensibilidade NÃO é feita e isso é reportado; o juiz não
se repete): `python3 scripts/r3_scan_rater_transcript.py <transcript> --model sonnet --allow-file adj_out/adj_sonnet3.jsonl --forbid adj_minimax.jsonl --forbid
adj_tiebreak --forbid adj_merge_final --forbid gold_a2 --forbid-tool Agent` (exit 0). Extensão feita para isto (commit da Errata 10): o verificador **permitia por
padrão ler `adj_out/adj_minimax.jsonl`** (o arquivo do rater 1) e não via delegação; agora há `--forbid` e `--forbid-tool` (32 testes, mutação 17/17). Mais: o
arquivo tem 179 linhas, um `item_id` por item, rótulos no domínio (`r3_adjudication.py`).

**E10.3 — análise (pré-declarada; nada aqui altera o gold registrado nem a regra do `r3_results_gate.py`).** `scripts/r3_sensitivity_gold.py`
(12 testes, mutação 14/14): (1) κ de Cohen **por par** — R1×R2 (já registrado, 0,7707), **R1×R3** (MiniMax × Sonnet, a única comparação entre famílias que
este juiz traz) e R2×R3 (mesma família) —, com a mesma projeção binária (seguida × não seguida) e a mesma exclusão de `indeterminate` do `merge`; (2) **gold
alternativo por maioria** dos três (um avaliador `indeterminate` se abstém; menos de 2 votos válidos ou empate exclui o item; unanimidade entre os votos válidos
= `agree`, 2×1 = `tiebreak`), pontuado com o MESMO pontuador (`r3_score_a2.py`, Errata 8) sobre as predições congeladas
`~/.claude/touring/r3-replay/a2/predictions-a2.jsonl`; a leitura `agree-only` do pontuador é a leitura **unânime**; (3) concordância do R3 com o gold
registrado (n de itens iguais / n de itens do gold). Saída fora do corpus congelado: `~/.claude/touring/r3-data/adj_sens3/` (nada é regravado em
`adj_merge_final/` nem em `r3-replay/`). Comando: `python3 scripts/r3_sensitivity_gold.py --r1 adj_out/adj_minimax.jsonl --r2 adj_out/adj_claude.jsonl --r3
adj_out/adj_sonnet3.jsonl --out-dir adj_sens3 --predictions r3-replay/a2/predictions-a2.jsonl --registered r3-replay/a2/results-a2-gold.json`.

**E10.4 — regra de leitura, fixada antes.** O A2 é "**robusto ao E3.3-bis**" se, e só se, os três vereditos do gate coincidirem: registrado, alternativo por
maioria e alternativo unânime. Se algum divergir, o resultado é "**sensível ao gold**" e os três vereditos saem lado a lado, com os números dos braços; o veredito
primário segue o do gold registrado até decisão do Gabriel. Se κ(R1,R3) < 0,60, isso é **evidência contra a confiabilidade do rater 1** e é reportada como tal,
sem veto automático. Os κ de cada par saem sempre, qualquer que seja o resultado.

---

# Errata 11 — 02/10/2026 19:30 -03:00 — registrada ANTES de qualquer chamada à API na reconsulta do Jev

> Motivo: o Gabriel aprovou (item 5 do canvas) "até ~50 chamadas para reconsultar ao vivo os 25 payloads distintos e conferir contra o cache, fechando a
> proveniência". **Fato achado antes de chamar a API, só por leitura:** a proveniência **já está registrada**. O journal do cliente
> (`~/.claude/touring/logs/jev_journal.jsonl`) tem, para as 25 chaves do cache com `a2_followed`, exatamente **25 chamadas reais** (`route_taken: api`, modelo
> reportado `jev-1.13.0`, 02/10 13:54:09–13:59:06, latência 326–400 ms, mediana 358 ms) e 358 respostas servidas do cache. A frase do RESULTS §4
> ("origem das entradas do cache não está registrada") estava errada e é corrigida no mesmo commit. O que a reconsulta ainda mede é **estabilidade**: o
> mesmo payload recebe a mesma probabilidade?

**E11.1 — desenho.** `scripts/r3_jev_recheck.py` (11 testes, mutação 14/14). 180 itens → 25 payloads redigidos distintos (validados por
`validate_jev_payload`: nenhum texto livre sai da máquina). A ferramenta contorna o cache do cliente (arquivo de cache vazio por chamada), grava num journal
separado (`r3-data/jev_recheck/jev_journal_recheck.jsonl`; o journal vivo e o KPI não são tocados), **recusa `--max-calls` > 50 e não faz a chamada seguinte ao
teto**. Passada 1 = 25 chamadas. Passada 2 (≤ 25) só se a passada 1 diferir do cache em qualquer payload. Falha de chamada é **erro**, nunca `p = 0`.
**E11.2 — leitura, fixada antes.** "Estável" ⇔ max |p_vivo − p_cache| ≤ 0,05 em TODOS os payloads (passada 1 × cache); senão "instável". Com passada 2, sai
também |p2 − p1|. **E11.3 — re-pontuação.** B-J com os p vivos no lugar dos do cache, contra o MESMO gold registrado, com o pontuador puro; veredito pelo gate.
O veredito primário continua o registrado; saem a concordância do B-J (registrada × com p vivos) e os dois vereditos. **E11.4 — saldo:** ~486 − chamadas feitas
(a ferramenta imprime o total e as rotas do journal: o esperado é `api` = chamadas, `cache` = 0).

**E11.5 — resultado (19:31 -03:00, depois da execução).** Rodaram as 2 passadas (a passada 1 diferiu do cache em 17 de 25 payloads, então a regra disparou a 2ª): **50
chamadas, todas `api`, 0 `cache`, 0 erros**; saldo ~436. Passada 1 × cache: 8/25 iguais, máx |Δ| = 0,02, média 0,0084 ⇒ **"estável"** pela tolerância 0,05; passada
2 × passada 1: 13/25 iguais, máx 0,02, média 0,0068. Latência real (n = 50): mediana 368 ms, p95 446 ms, máx 574 ms. **Re-pontuação do B-J (E11.3):** o texto do E11.3 dizia
"os p vivos" sem dizer qual passada; a ferramenta usou a **passada 1**, e eu pontuei também a **passada 2** com a mesma função, para não escolher a que convém:
concordância do B-J **0,8353 (cache) · 0,9176 (passada 1) · 0,8000 (passada 2)**, B0 = 0,9059; Δ vs B0 −0,0706 · +0,0118 (IC inferior 0,0000) · −0,1059; τ 0,46 · 0,47 · 0,47;
precisão 0,806 · 0,892 · 0,887; **veredito do gate nas três: `NAO_ADOTADO_PELOS_DADOS`** (C-A2-1 e C-A2-3 falsos nas três). **Leitura:** o veredito é robusto ao ruído do
Jev; a concordância do B-J, não (amplitude de 10 itens em 85, porque os p ficam entre 0,31 e 0,50 e o τ cai em 0,46–0,47). O veredito primário e o gold registrado não
mudam. Artefatos: `r3-data/jev_recheck/` (`recheck.json`, `predictions-a2-jevlive{,-1,-2}.jsonl`, `jev_journal_recheck.jsonl`).

---

# Errata 12 — 02/10/2026 19:35 -03:00 — r4 mínimo (B-L com texto truncado), pré-registrado ANTES de rodar

> Autorização: o Gabriel, no item 2 do canvas, escolheu "A. r4 mínimo: só o B-L (Laya local) com texto truncado de ≤200 caracteres, pré-registrado".
> Escopo: **só isso**. A3, A4, Tier B, S-9b e o C-A2-3 com margem não fazem parte, e o veredito registrado do A2 não muda.

**E12.1 — divergência do r3, achada ao preparar o r4 e verificada no código.** O B-L do r3 rodou o checkpoint **inglês** (raiz de `convaiinnovations/laya`:
encoder ModernBERT-large, `max_len` 512), **não** o "multilíngue" que o pré-registro nomeia (CRITERIOS:71, plan.md:25). Prova: `laya.Agent(subfolder=None)` é o
default, `r3_arms.py:84` e `r3_replay_a2.py` não passam `subfolder`, e o multilíngue só carrega com `subfolder="multilingual"` (`laya/agent.py:514` e `:1861`;
encoder mmBERT-base, `max_len` 1024). O resultado do B-L do r3 vale, portanto, para o checkpoint inglês com o payload redigido. Divulgada; não altera o veredito.
(O levantamento do `touring-a4` apontou isto; a parte dele sobre "200 chars não cabem na cabeça" estava errada: `head_max_len` orça as opções da cabeça, o texto do estado
entra por `max_len`; e a alegação de que o venv do Laya não existe também: ele existe e `import laya` funciona.)

**E12.2 — hipótese primária, única.** **B-LT-EN** = checkpoint inglês (o mesmo do r3), pergunta idêntica à do r3, estado = texto truncado, passa os critérios C-A2-1..3 do r3 (Errata 8) contra
o B0. Comparar com o B-L do r3 isola o efeito do texto. **B-LT-ML** = checkpoint multilíngue (o que o pré-registro nomeou) é **secundário/exploratório**: se ele passar e o
primário não, sai como "exploratório, exigiria réplica" e **não** sustenta a hipótese.
**E12.3 — entrada, fixada.** `Recommended route: <rota_proposta>`, `Next commands:` e até 3 comandos; cada texto com os espaços colapsados e cortado em **200** caracteres
(política de dados do §1/E4.2); nada mais do item (nem a decisão do B0, nem a sessão). O texto não é gravado em nenhuma saída (as predições são números); local, CPU, sem rede.
**E12.4 — pontuação.** Os 180 itens congelados; τ em `audit-val` pela regra da Errata 8; teste em `audit-test`; **gold registrado** (`adj_merge_final/gold_a2.jsonl`, κ 0,7707); IC bootstrap do
pontuador; veredito **por braço** pelo gate, computado sobre UM braço (`evaluate`). O gate trata nomes de braço novos como braços de API (teto de latência 800 ms; C-A2-4 só é reportado).
AUC de polaridade reportado, sem inversão (E8.8).
**E12.5 — expectativa registrada antes de ver, para não racionalizar depois.** O B-L do r3 teve AUC 0,12 (inverso) e o `noul` zero-shot não mediu "seguida" numa sonda sintética (AUC
0,500). Minha probabilidade a priori de o B-LT-EN passar é **baixa (< 0,2)**. Se o AUC dele continuar < 0,5, a leitura é "o `noul` zero-shot do Laya não mede seguida, com texto ou sem" e **fecha aqui**;
não se escolhe outra pergunta depois de ver o resultado (isso seria um r5 novo, pré-registrado antes).
**E12.6 — execução.** `hw-slot -- python3 scripts/r4_replay_text.py` (CPU, 2 threads, 2 checkpoints); saídas em `~/.claude/touring/r4-replay/`; o corpus congelado e `r3-replay/` não são
regravados. Ferramenta: `scripts/r4_replay_text.py` (15 testes, mutação 17/17). Um smoke test com `--limit` e `--out-dir` descartável existe para provar que a chamada real ao modelo funciona
(estado como texto, `subfolder`) e **nunca é a rodada registrada**; correção de bug que ele revele é divulgada, desenho não muda.

**E10.7 — a tentativa 1 do juiz R3 reprovou na verificação mecânica; única repetição, registrada ANTES de lançá-la (19:37 -03:00).**
(a) **Fato.** O juiz R3 (tentativa 1) gravou `adj_out/adj_sonnet3.jsonl` (179 linhas, sha256 `2009ff123d93f966…`; modelo `claude-sonnet-5-5` nos 137 turnos, 68 chamadas, 0 `Agent`) e o
verificador do E10.2 devolveu **exit 1, 1 violação**: `out_of_scope_path` `~/.claude/touring/r3-data/` numa chamada `touring run` cujo programa definia essa raiz como
**prefixo literal** (`D = ".../r3-data/"`) e abria `D + "adj_pack/rater_view/pack_blind.jsonl"` e o arquivo de saída. Lido só esse trecho: nenhuma listagem de diretório, nenhum
arquivo proibido (os nomes proibidos seguem pegos por `FORBIDDEN`). **Pela regra do E10.2 a tentativa 1 é inválida**, e o verificador **não é afrouxado** depois de ver a violação.
(b) **Rótulos da tentativa 1 NÃO lidos e NÃO usados** em nenhuma análise (arquivo mantido como está, hash acima). Divulgação: vi as 4 contagens que o próprio juiz relatou na mensagem final
(followed 117 · bypass 39 · ignored 22 · indeterminate 1), antes da verificação.
(c) **Repetição, uma só**, permitida porque a invalidação é **processual e anterior a qualquer resultado**: mesmo desenho do E10.1, saída `adj_out/adj_sonnet3b.jsonl`, e o prompt ganha duas regras
(caminhos sempre por extenso e completos, nunca montados por prefixo; o pacote pode ser lido por faixas com `Read` ou impresso por faixas com `touring run`, que a tentativa 1 precisou usar porque o pacote
excede o limite de uma leitura). Verificação: `--allow-file adj_out/adj_sonnet3b.jsonl` e `--forbid adj_sonnet3.jsonl` além das flags do E10.2. **Se a tentativa 2 também reprovar, não há terceira**:
a análise de sensibilidade sai como "não realizada". O E10.3 (análise) e o E10.4 (regra de leitura) valem sem mudança, com `--r3 adj_out/adj_sonnet3b.jsonl`.

**E11.6 — suplemento (19:45 -03:00).** A conferência cruzada do `touring-a4` notou que o p95 (446 ms) e a coluna da passada 2 estavam só na minha conversa, não num artefato:
`r3_jev_recheck.py --supplement` (sem API) gera `jev_recheck/recheck_supplement.json` e reproduz os números do E11.5. Latência real do Jev (n = 50): **mín 322 ms**, mediana 368 ms, **p95 446 ms**,
máx 574 ms. A dúvida dele — uma repetição servida por deduplicação do journal em vez da API — se fecha por dois fatos: o journal do cliente é só de escrita (`_journal`, o cache é lido do
arquivo de cache, que a ferramenta esvazia por chamada) e a menor latência das 50 chamadas é 322 ms, impossível para uma resposta de cache (0 ms nas predições do replay).

**E12.8 — correções ao E12.1/E12.4 (19:45 -03:00, antes de ler o resultado do r4).** (a) **Checkpoint, agora medido** (`scripts/r4_checkpoint_probe.py`, `Agent.cfg` de cada carga): o agente carregado como os
braços do r3 o carregaram (sem `subfolder`) reporta `encoder: answerdotai/ModernBERT-large`, `max_len 512`, `head_max_len 192`, temperaturas [1,637; 1,251; 1,983]; com `subfolder="multilingual"`,
`jhu-clsp/mmBERT-base`, 1024, 256, [1; 1; 1]. O E12.1 deixa de ser leitura de código. (b) **Denominador:** o corpus congelado tem 180 itens e o pacote dos avaliadores 179 (um item sem rota); o r4 prediz os 180
e **pontua os que estão no gold registrado (171)**; `check_alignment` falha alto se o conjunto de `item_id` das predições e o da amostra diferirem. (c) O teste de vazamento do estado ganhou a sentinela de
`n_cmds` (apontada pelo `touring-a4`; mutação 20/20).

**E12.7 — resultado do r4 mínimo (19:45 -03:00; rodada registrada 19:37:30 → 19:41:54; 180 itens preditos, 171 no gold; `~/.claude/touring/r4-replay/results-a2-text.json`).**
**Primário B-LT-EN** (checkpoint inglês, texto truncado): concordância **0,3176** (B0 0,9059; B-L do r3 0,7059), Δ vs B0 −0,5882, IC95 % inferior −0,7176, precisão 0,000, cobertura 0,000, τ 0,9998,
AUC de polaridade **0,331** ⇒ `NAO_ADOTADO_PELOS_DADOS`. **Secundário B-LT-ML** (exploratório): 0,3059, Δ −0,6000, precisão 0,429, cobertura 0,082, τ 0,9997, AUC **0,551** ⇒ `NAO_ADOTADO_PELOS_DADOS`.
Distribuição dos p (os 171 do gold): B-LT-EN tem 167 valores distintos (0,16–0,9998; mediana 0,78; 153 de 171 ≥ 0,5), com média **0,72 nos seguidos e 0,79 nos não seguidos (invertido)**; o B-L do r3 ficava
comprimido em ~0,07 (0,05–0,21). A concordância baixa **não** vem de p degenerado: a regra de ponto de operação da Errata 8 (maior cobertura com precisão ≥ 0,90 em val) só acha τ na cauda extrema
(cobertura 1,2 % em val), e o braço então prediz "não seguida" quase sempre. Latência em CPU (chip a 91–94 °C): mediana inglês 1.068 ms e multilíngue 390 ms; **p95 inglês 1.250 ms** (acima do teto de 800 ms que o gate usa para nome de braço novo; o C-A2-4 só é reportado) e p95 multilíngue 494 ms. **Leitura pré-registrada (E12.5): AUC do B-LT-EN
< 0,5 ⇒ o `noul` zero-shot do Laya não mede "seguida", nem com texto nem sem; o r4 mínimo fecha aqui.** A probabilidade a priori que eu registrei (< 0,2) condiz. Limitações: 1 rodada; o texto é cortado em
200 caracteres (pelo levantamento do `touring-a4`, 408 de 534 comandos do corpus passam disso, então o modelo vê o começo); só a pergunta `noul` do r3 foi testada.
**E12.9 — retratação (19:45 -03:00).** O E12.8 (commit `0c4b61ed`, 19:45:19) se diz escrito "antes de ler o resultado do r4": **falso**. O r4 rodou de 19:37:30 a 19:41:54 (arquivo de resultado gravado 19:41:53) e eu
li o resultado **antes** de escrever o E12.8. O E12.8 só corrige texto (checkpoint medido, denominador, sentinela de teste) e nada nele depende do resultado, mas a frase de ordem estava errada, e a ordem
que vale é a do git: pré-registro `f9ceb88a` 19:35:43 → rodada 19:37:30 → resultado 19:41:53 → E12.8 19:45:19.

**E10.8 — resultado do juiz R3 (tentativa 2) e da análise do E10.3 (19:49 -03:00).** Verificação do E10.2/E10.7 na tentativa 2: **exit 0, 0 violações**, 162 turnos, 99 chamadas, modelo `claude-sonnet-5-5`, 0 `Agent`
(com `--forbid adj_sonnet3.jsonl`, `adj_minimax.jsonl`, `adj_tiebreak`, `adj_merge_final`, `gold_a2`); `adj_sonnet3b.jsonl` com 179 linhas (followed 118 · bypass 38 · ignored 21 · indeterminate 2).
**κ por par** (projeção binária, sem `indeterminate`): R1×R2 **0,7707** · R1×R3 (MiniMax × Sonnet) **0,7818** · R2×R3 (Claude × Sonnet) **0,9867**. **Gold por maioria dos 3:** 176 itens (160 de unanimidade entre os votos válidos, sendo 156 com os três avaliadores votando e 4 com uma abstenção, e 16 em 2×1);
pontuado pelo pontuador da Errata 8: B0 0,9080 · B-J 0,8391 (Δ −0,0690, IC inf −0,1379) · B-L 0,6897 (Δ −0,2184, IC inf −0,3218); leitura **unânime** (160 itens): B0 0,9231 · B-J 0,4487 · B-L 0,7564.
**Vereditos do gate: registrado `NAO_ADOTADO_PELOS_DADOS` · maioria `NAO_ADOTADO_PELOS_DADOS` · unânime `NAO_ADOTADO_PELOS_DADOS` ⇒ pela E10.4, o A2 é "robusto ao E3.3-bis", no sentido da regra: os três vereditos do gate coincidem.** κ(R1,R3) = 0,78 ≥ 0,60: sem evidência
contra a confiabilidade do rater 1. O gold registrado e o veredito primário **não mudam**; o E3.3-bis segue para decisão do Gabriel.
**Dois fatos que o resultado trouxe e que o desenho não previa (reportados, nada foi alterado por eles).** (1) O R3 concorda com o gold registrado em **171/171** (R2: 170/171; R1: 157/171): o R3, cego, decidiu **igual** os 15 itens
do desempate, então aquela decisão não era artefato do contexto do juiz anterior. (2) O R1 (MiniMax, o único avaliador de fora da família) tem **viés direcional bem evidenciado nesta amostra**: rotulou 134 itens como seguidos (R2 117, R3 118) e,
nos 16 itens binários em que R1 e R3 divergem, **R1 diz "seguida" e R3 não em 16 de 16** (0 no sentido oposto; p ≈ 1,5×10⁻⁵ sob a moeda justa). A **taxa** não é estimada por 16 itens de 177 (9 %) numa rodada, e o R1 também foi o que menos se absteve (1, contra 7 do R2 e 2 do R3). Como κ(R2,R3) = 0,987, os dois Claude pesam como um voto só: a "maioria dos 3" é, na prática, o rótulo dos Claude.
**Consequência:** o veredito não depende do rater 1 (a leitura sem ele e a unânime dão o mesmo veredito), mas o gold registrado é quase todo rótulo da família do agente julgado; o risco de ponto cego compartilhado da E3.3
**não foi resolvido por este juiz** (também Claude) e segue declarado. Artefatos: `r3-data/adj_sens3/` (`gold_alt3.jsonl`, `adjudication_summary.json`, `results-a2-alt3.json`).

---

# Errata 13 — 02/10/2026 20:51:52 -03:00 — decisões do Gabriel e regra do S3 fixada antes do resultado

> Hora lida por `date`; a ordem que vale é a do git: `6b427322` 19:54:13 → script do S3 `a035283f` 20:50:54 → esta errata (hash no commit seguinte). Nada aqui move um limiar de A1/A2 nem troca um veredito.

**E13.1 — decisões.** O Gabriel respondeu "recomndações aprovadas" ao canvas das quatro decisões, isto é, aprovou as recomendações do §8 do canvas. (1) **E3.3-bis aceito.** O gold registrado e o veredito do A2 não mudam (`NAO_ADOTADO_PELOS_DADOS` pelas três leituras do gold, E10.4); aceitar o desvio deixa de rebaixar o A2 a `GOLD_INSUFICIENTE` e **não cria aprovação**; o risco de ponto cego compartilhado da E3.3 (gold quase todo Claude) segue declarado. (2) **Unit** `r3-shadow-evaluator` mantida até 15:09 de 03/10 e parada depois da leitura do S3, **só se** os 3 contadores de perda estiverem em 0 (senão fica de pé para investigação; essa ressalva é minha, do §9 do canvas). (3) **Propagação**: esperar um release com ganho; gatilho proposto: "release que corrija defeito reproduzido num pinado, ou ganho medido" (se a 30.4.92, com o `MAX_PENDING` `68d70d0f`, afeta `analise` ou `konverter` não foi verificado). (4) **r5 não aberto**; A3, A4, Tier B, S-9b e C-A2-3 com margem seguem sem aprovação e sem toque.
**Como li a mensagem:** ela chegou colada a um aviso automático do Stop hook; tratei como do Gabriel porque o `UserPromptSubmit` a registrou como prompt dele e o texto, com o erro de digitação, é o dele. Se não for, o que foi feito é reversível: `systemctl --user stop r3-s3-close.timer` e um erratum que retrate.

**E13.2 — regra do S3, fixada antes do resultado (E5.4 + E6.2 + esta).** `scripts/r3_s3_close.py` (commit `a035283f`; 33 testes; mutação 25/25 com baseline verde, depois de 2 sobreviventes que eram fraqueza do meu teste e foram corrigidos). Denominador = `offered` somado dos braços em `code_mode_arm.json` menos a base 485 (`r3-data/s3_baseline.json`, 15:15:50); numerador = linhas `a2_offer_closed` cuja oferta (`offered_secs`, época) é posterior à base; o script espera o contador ficar parado 120 s (teto 600 s) antes de ler, porque uma oferta aberta fecha por TTL de 60 s. Degraus, nesta ordem: contadores do `kpi` indisponíveis → `NAO_MEDIDO`; qualquer de `emitter_dropped_since_start`, `foreign_lines`, `unreadable_lines` > 0 → `REPROVADO` (perda direta); contador de ofertas abaixo da base → `NAO_MEDIDO`; janela < 23,5 h → `INTERINO`; mais linhas que ofertas → `INCONSISTENTE`; cobertura ≥ 0,95 → `PASS`; senão `NAO_DEMONSTRADO`. **Um déficit sozinho nunca é `REPROVADO`**: uma oferta pendente quando a sessão acaba não emite linha por desenho (E5.4) e o contador não separa isso de perda. Só `PASS` e `NAO_DEMONSTRADO` autorizam parar a unit. Saída em `r3-data/s3_close.json`; exit 0 só no `PASS`.
**Dois números novos, meus e declarados:** o mínimo de 23,5 h e a espera de 120 s não estão no pré-registro de 24 h. Às 15:10 de 03/10 a janela medida é de **23,903 h**: a base foi tomada 388 s depois de o shadow ser ligado (15:09:22), e as ofertas desses 388 s ficam fora do numerador e do denominador. A janela é 23 h 54 min 10 s (15:15:50 → 15:10:00), 5 min 50 s aquém das 24 h nominais, e é reportada como tal.

**E13.3 — execução e limites.** Timer transitório `r3-s3-close.timer` (`systemd-run --user`, `--on-calendar='2026-10-03 15:10:00'`, fuso America/Sao_Paulo confirmado por `systemd-analyze calendar`), serviço `python3 r3_s3_close.py --stop-unit`. **Ensaio no dado real às 20:50** (sem `--stop-unit`, saída no scratchpad): `INTERINO` (janela 5,58 h), 11 ofertas, 10 linhas na janela, **1 sem explicação**, contadores de perda 0, unit continuou ativa. **Limites:** (a) o déficit de 1 do ensaio não é atribuível (oferta pendente ou perda); com ~16 ofertas por dia, 1 sem linha dá 0,9375, abaixo de 0,95, e o veredito de amanhã pode sair `NAO_DEMONSTRADO` sem que haja perda; (b) o contador de descartes é do processo do daemon e volta a 0 se ele reiniciar antes das 15:10; (c) a unit e o timer são transitórios (`/run`): se a máquina reiniciar antes, ambos somem e o fechamento passa a ser manual (`python3 scripts/r3_s3_close.py --stop-unit`); (d) que o `kpi` roda no daemon foi lido no código (`kpi/code_mode.rs:601-602`), não provado por um valor ≠ 0.
