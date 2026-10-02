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
