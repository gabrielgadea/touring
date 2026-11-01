---
type: Analysis
title: "Impacto da injeção de contexto dos hooks do Touring no comportamento do LLM — análise consolidada (rodadas 1 e 2) + interruptor"
description: "Consolidação de duas rodadas: volume e mapa de todo o contexto injetado (1.028 sessões), o comportamento medido por canal em telemetria de produção, o veredito por canal, o interruptor entregue e a Wave 2 nativa em Rust."
plan_id: 2026-10-26-injecao-contexto-hooks
okf_version: 1
tags: [hooks, injecao, contexto, harness, cila, kpi, comportamento]
timestamp: 2026-10-26
---

# Impacto da injeção de contexto dos hooks do Touring — análise consolidada

> **Pedido (Gabriel, 26/10/2026)**: *compreender o impacto que a injeção de contexto dos hooks do
> touring tem no comportamento do llm que está utilizando-o; se ajuda de fato, se atrapalha, se
> melhora etc. E precisamos de uma opção de ligar/desligar a injeção de contexto dos hooks.*
>
> Este documento **consolida as duas rodadas**. Rodada 1: volume, mapa de todo o contexto
> injetado, desenho e implementação do interruptor. Rodada 2: **o comportamento, medido em
> telemetria de produção**, e a Wave 2 nativa em Rust.
>
> **Etiquetas**: `FACT [1.0]` medido/lido no código ou no disco · `INFERENCE [0.7–0.9]` derivado ·
> `SPECULATION [<0.7]` hipótese. **Janela dos contadores**: cumulativos no processo do daemon
> (`AtomicU64` em `global()`), desde o último start do daemon (PID 3872 na leitura).

---

## 1. Veredito

**A injeção não é uma coisa: são três canais, com três vereditos medidos diferentes.**

| canal | o que é | veredito medido |
|---|---|---|
| **persuasão** (advisory) | MUST/SHOULD/MAY num bloco `additionalContext` | **não funciona**: 92 sugestões emitidas → **7 seguidas (7,6%)**; das 84 rotas `touring run` oferecidas, **46 resistidas (54,8%)** e 10 seguidas (11,9%). No corpus: **adesão 1,3%–2,2%** por evento/hook (§4.6) |
| **coerção** (gate) | o gate que reescreve o comando ou **nega** | **funciona, pela coerção**: 13 negações, 6 conversões por reescrita (adesão 1,000 **por construção**, não por persuasão), **581,9 KB de saída elidos** em 86 execuções no sandbox |
| **fato** (enriquecimento) | blast radius, file context, syntax pre-check, símbolos do arquivo | **é o único que move a ação seguinte**: **59,9% de impacto** (proxy: o modelo tocou o alvo nomeado) e **56% do volume do Touring** — mas **não existe contador de adesão** para ele na telemetria de produção, e **nada** mede o *resultado* (§4.6) |
| **dashboard** (plugin-hook) | painel ATZILUTH, dashboard slim, sugestão de skill | **não funciona**: **5,8%** de impacto, 0% de adesão (§4.6) |
| **debug-stdout** | stdout cru do quality gate do projeto | **não é contexto**: 9% do volume, 1% de unicidade, zero proposições |

**A frase que resume**: *a injeção atrapalha na proporção do que é sugestão e ajuda na proporção
do que é fato ou coerção.* — e agora há **número por hook** (§4.6) para sustentar cada parte
dessa frase.

**Consequência de projeto (agora com base empírica, não em intuição)**: o interruptor não pode ser
binário, e o nível recomendado é o que **remove o canal medido como inoperante** e **preserva os
dois outros** — um deles porque foi medido como eficaz, o outro porque **ninguém o mediu**.

**Achado adicional da rodada 2 — a régua mede a coisa errada**: o KPI `injection_follow_ratio`
(com piso 0,60) compara `followed` com `emitted`, e **esses dois não são o mesmo denominador**
para os gates que negam (§7). O veredito de "adesão" que a régua emite não é uma medida de
adesão.

---

## 2. A janela: onde a injeção compete

Medição sobre 200 sessões de 3 projetos (transcripts reais):

| origem dos bytes | bytes | % da janela |
|---|---:|---:|
| **tool results** | 15.547.326 | **50,2%** |
| **injeção de hook** | 8.606.515 | **27,8%** |
| texto do usuário | 5.150.979 | 16,6% |
| texto do assistente | 1.668.873 | 5,4% |

**FACT [1.0]** — isto **replica de forma independente** o baseline de 04/09/2026
(`docs/plans/2026-09-04-economia-de-contexto/plan.md` §1: tool results 51,3%, injeção 26,6%,
usuário 14,5%, assistente 7,6%). Um mês e um corpus depois, a proporção é a mesma.

Em escala de corpus (1.028 sessões, 21 dias, 440.867 invocações):

| métrica | valor |
|---|---:|
| bytes injetados | **61.499.972** |
| bytes **únicos** | 46.089.292 |
| **repetição (byte-idêntico)** | **25,1%** |
| latência acumulada de hook | **75.146.513 ms** (20,9 h) |
| razão injeção / texto do usuário | **1,30×** |

**Acumulação por turno** (10.404 turnos): mediana **168** · média **825** · p90 **2.201** ·
p99 **8.165** · máximo **45.306**. A mediana é inofensiva; a cauda é o problema.

---

## 3. O mapa: todo o contexto injetado, todos os eventos

**134 combinações injetoras** em **7 eventos**, reconstruídas do corpus (cada invocação deixou
gravado `hookEvent`, `hookName`, `command` e o `stdout` real).

| evento | invocações | bytes injetados | média | % da injeção |
|---|---:|---:|---:|---:|
| **PreToolUse** | 189.882 | 38.442.252 | 202 | **62,5%** |
| **PostToolUse** | 81.706 | 13.307.586 | 162 | 21,6% |
| PostToolBatch | 65.985 | 2.501.727 | 37 | 4,1% |
| SessionStart | 4.019 | 6.868.991 | 1.709 | 11,2% |
| PostToolUseFailure | 2.833 | 290.737 | 102 | 0,5% |
| UserPromptSubmit | 2.189 | 30.646 | 14 | 0,05% |
| Stop | 1.933 | 58.033 | 30 | 0,1% |

**FACT [1.0]** — `PreToolUse` + `PostToolUse` = **84%** de toda a injeção. `UserPromptSubmit` e
`Stop` são desprezíveis em volume: qualquer análise que foque neles erra o alvo.

### 3.1 As quatro espécies de conteúdo

| espécie | % da injeção | quem emite | o que é |
|---|---:|---|---|
| **KIND-1 · FATO** | ~44% | *analyzing blast radius* (12,2 MB, 95% único), *analyzing file context* (3,9 MB, 97% único), *speculative validation* (4,2 MB), *syntax pre-check* (1,8 MB), *verifying edit quality* (3,1 MB), *loop_resume* (892 KB) | conhecimento do repositório sobre o arquivo que está sendo tocado |
| **KIND-2 · PERSUASÃO** | ~20% | *cli-suggest* (11,9 MB em 4 eventos), *checking command history* (2,05 MB, 6% único), *PostToolBatch* (2,5 MB), *evidence-guard* (694 KB), dashboards | MUST/SHOULD/MAY, lições passadas, sugestoes de skill |
| **KIND-3 · LOG DE DEPURAÇÃO** | **9,0%** | `post_quality_gate.py` (5,3 MB, **1% de unicidade**) | stdout cru: `[EVENT_READER] Reading hook event [POST_QUALITY_GATE_V4] Hook execution started …` |
| **KIND-4 · GATE** | 0% de volume | `touring-quality` BLOCK, `Git guard`, `config_protection`, `touring-process-guard`, `loop_outer_effect`, `loop_stop_guard`, `agent_model_gate`, `fable_consent_guard` | **34 combinações bloqueiam sem injetar um byte** |

**FACT [1.0]** — o dump de depuração é o maior ruído isolado: 13.191 invocações, 5,3 MB, 1% de
unicidade. Não há leitura em que ajude.

---

## 4. O comportamento: o que a injeção produz no modelo

Esta é a rodada 2, e é a resposta direta à pergunta *"ajuda de fato, atrapalha, ou melhora?"*.

### 4.1 O canal da persuasão — MEDIDO, e não funciona

Contadores de produção (`touring gate-metrics -j`):

| contador | valor | leitura |
|---|---:|---|
| `suggestion_uptake_emitted_count` | 92 | sugestões de redirect emitidas |
| `suggestion_uptake_followed_count` | **7** | a ação seguinte na mesma sessão adotou o redirect (`touring`/code-mode) |
| **adesão** | **7,6%** | **92,4% das sugestões são ignoradas** |
| `native_injection_code_route_count` | 84 | rotas `touring run`/`touring exec` oferecidas |
| `native_injection_followed_count` | 10 | seguidas (11,9%) |
| `native_injection_resisted_count` | **46** | **resistidas (54,8%)** — o modelo ficou na ferramenta atômica |

**FACT [1.0]** — a doc do próprio contador (`crates/touring-foundation/src/gate_metrics.rs:847-850`)
define: *"emitted suggestions whose next same-session action followed the redirect"*. O
instrumento existe, é o correto, e o número é **7,6%**.

**INFERENCE [0.9]** — isto **replica** o que o registro do transcript da rodada 1 já indicava
(follow-ratio 0,094 sobre 6.000 MUSTs reconstruídos). Duas medições independentes, mesmo
veredito: o canal persuasivo raramente muda o comportamento.

### 4.2 O canal da coerção — MEDIDO, e funciona (pela coerção)

A matriz completa de `gate_events` (o enum `GateEvent` tem **quatro** colunas —
`crates/touring-foundation/src/gate_metrics.rs:1500-1509`):

| gate | emitted (advisory mostrado) | followed | **denied** | bypassed | conversão (followed+denied) |
|---|---:|---:|---:|---:|---:|
| g7_reinspect | 6 | 0 | **4** | 0 | 66,7% |
| g2_pipe_exit | 4 | **4** | 0 | 0 | **100%** |
| g1_burst | 3 | 0 | 0 | 0 | 0% |
| g3_edit_sem_read | 2 | 0 | **9** | 0 | 450% |
| g8_loop_rewrite | 2 | **2** | 0 | 0 | **100%** |
| **TOTAL** | 17 | 6 | **13** | 0 | — |

**FACT [1.0]** — `g2_pipe_exit` e `g8_loop_rewrite` são os dois gates que **reescrevem** o
comando. A doc de `hook_rewrite_applied_count` (`gate_metrics.rs:597-599`) é explícita:
*"A rewrite that fires needs no follow-through from the model, so emitted == followed **by
construction**"*. A adesão 1,000 deles **não é persuasão** — é o comando trocado.

**FACT [1.0]** — `g3_edit_sem_read` tem `emitted=2` e `denied=9`: **o gate negou 9 vezes e só
"emitiu" advisory 2 vezes**. Logo, `emitted` e `denied` **não são o mesmo denominador** — ver §7.

**FACT [1.0]** — o valor do canal coercivo, em bytes, é medido: `code_mode_runs_count = 86` e
`code_mode_bytes_elided_total = 595.825` — **581,9 KB de saída de ferramenta elidos** porque o
gate mandou a execução para o sandbox.

**INFERENCE [0.9]** — o canal da coerção funciona **e o canal da persuasão não**, e a diferença
não é de redação: é de **mecanismo**. O gate que nega não precisa convencer — precisa **impedir**.
O nudge precisa convencer, e 92,4% das vezes não convence.

### 4.3 O canal do fato — medido por transcript, **sem contador em produção** e **sem resultado**

**FACT [1.0]** — não existe, em `gate_metrics.rs` nem em `gate_metrics_snapshot.rs`, nenhum
contador de adesão para o canal de enriquecimento (blast radius, file context, syntax
pre-check). O que existe para esse canal é `record_emission`
(`crates/touring-cli/src/cli_suggester.rs:6742`), que conta **bytes emitidos** — não adesão.

**A §4.6 mede esse canal pelo corpus** (impacto de 59,9%: o modelo tocou o alvo nomeado),
mas isso é uma **medição externa**, reconstruída dos transcripts — não uma métrica de
produção. Continuam em falta duas coisas: (a) um contador nativo de adesão, e (b) a medida
de **resultado**, que só o A/B responde (§8.8).

**INFERENCE [0.9]** — é por isso que o interruptor preserva o KIND-1: **não se desliga o canal
que ninguém mediu**. Desligar um canal medido como inoperante é economia; desligar um canal
não medido é uma aposta. O plano de 04/09 já suspeitava disso ao propor a dedup por conteúdo e
o "piso de densidade": faltava o contrafactual.

**SPECULATION [<0.7]** — a hipótese de que o KIND-1 ajuda é plausível (ele é insubstituível: um
modelo não descobre sozinho que `fase_procedencia` tem CC=16 nem que 6 chamadores dependem de
`harness_concordancia`), mas **não está provada**. A prova exigiria um A/B com corpus rotulado
de resultado, não de escolha — o instrumento `crates/touring-hooks-shared/tests/s8_corpus_ab.rs`
existe e é o caminho (§8.6).

### 4.4 O canal que nunca acordou

**FACT [1.0]** — `ctx_budget_alert_emitted_count = 0` e `ctx_budget_warning_emitted_count = 0`.
O orçamento de contexto por turno (`turn_budget.rs`, teto 2.000 B/turno) **nunca disparou um
alerta em produção**. A régua reporta `injected_bytes_per_turn = 2.757` contra teto 2.000
(FAIL) — mas o alarme silencioso. Isso é a família *infra desligada não é infra pronta*.

### 4.5 A observação ao vivo, dentro desta sessão

A sessão inteira é uma **experiência natural** do mecanismo:

| evento | o que o gate entregou | o que eu fiz |
|---|---|---|
| 3ª leitura do mesmo arquivo | deny com o programa fundido | obedeci — fundi a leitura em 1 programa |
| 2ª inspeção `cat` em 300 s | advisory com o lote fundido | obedeci |
| 1ª edição do turno | deny único com recall de memória e gotchas | re-fiz a mesma edição |
| `PreToolUse:Edit` MUST `ast meta`/`blast`/`pre-edit` | sobre um script descartável em `/tmp` | **ignorei** |
| `PreToolUse:Write` MUST `--intent "<intent>"` | placeholder literal | **ignorei** |
| `PreToolUse:Agent` MUST `touring decompose validate <task_id>` | placeholder literal | **ignorei** |
| `C08` sobre `basename` | "61 call sites em 14 arquivos" de `pptx/` — falso positivo | **ignorei** |
| `PostToolBatch` | o mesmo path listado 5× | ignorei |
| `prose_integrity_gate` | recusou meu Write por 3 caracteres CJK | **obedeci** (reescrevi) |

**INFERENCE [0.9]** — o padrão se mantém: **gate obedece, sermão não**. E a razão específica
está visível nos dados: os sermões ignorados carregavam **placeholder** (`<intent>`,
`<task_id>`) ou apontavam para alvo inaplicável — violando a invariante de densidade de
`touring-4-pillars.md` ("never a `<placeholder>` when the value is derivable").

---

### 4.6 Adesão e impacto por hook e por evento (a tabela que faltava)

Medição instrumentada sobre o corpus (`scripts/measure_injection_impact.py`, 400 sessões, janela
de 3 chamadas de ferramenta à frente). Para cada bloco injetado extraem-se as **proposições**
(comandos `MUST/SHOULD/MAY`, caminhos, símbolos) e examina-se a ação seguinte do modelo:

| classe | o que significa |
|---|---|
| **aderiu** | a próxima chamada **executa/usou o comando** que o bloco sugeriu — adesão de um *sermão* |
| **tocou** | a próxima chamada **toca um caminho/símbolo** que o bloco nomeou — impacto de um *fato* |
| **nenhum** | nenhuma das duas — o bloco não mudou nada |
| **com_prop** | blocos que carregam ao menos uma proposição (a taxa é calculada só sobre estes) |

**POR HOOK (Touring)** — 54 combinações, 15,7 MB injetados:

| evento | hook | família | bytes | blocos | com_prop | aderiu | tocou | nenhum |
|---|---|---|---:|---:|---:|---:|---:|---:|
| PreToolUse | Edit *(blast radius)* | pre-edit | 5.738.591 | 6.174 | 5.048 | **3** | **3.119** | 1.926 |
| PreToolUse | Bash | outros | 1.919.937 | 6.384 | 4.098 | **164** | 730 | 3.204 |
| PreToolUse | Read *(file context)* | pre-edit | 1.831.367 | 2.014 | 1.970 | **7** | **1.255** | 708 |
| PreToolUse | Write *(speculative validation)* | pre-edit | 1.703.731 | 2.480 | 2.441 | **3** | **1.942** | 496 |
| PostToolUse | Edit *(edit quality)* | pre-edit | 706.655 | 2.190 | 432 | 0 | 104 | 328 |
| SessionStart | startup *(dashboards)* | plugin-hook | 673.533 | 625 | 448 | 0 | 25 | 423 |
| SessionStart | compact *(loop_resume)* | code-mode-deny | 577.567 | 185 | 165 | 0 | 73 | 92 |
| PostToolUse | Bash | outros | 456.247 | 4.294 | 383 | 0 | 193 | 190 |
| PostToolBatch | | post-tool-echo | 358.957 | 1.329 | 1.311 | 0 | **985** | 326 |
| PostToolUse | Write | post-edit | 128.284 | 949 | 120 | 0 | 36 | 84 |
| PostToolUseFailure | Bash | outros | 116.841 | 504 | 334 | 0 | 214 | 120 |
| SessionStart | resume | plugin-hook | 42.832 | 41 | 30 | 0 | 12 | 18 |
| PreToolUse | SendMessage | outros | 13.802 | 41 | 0 | 0 | 0 | 0 |
| SessionStart | clear | plugin-hook | 12.696 | 12 | 9 | 0 | 1 | 8 |
| SessionStart | fork | plugin-hook | 9.883 | 11 | 7 | 0 | 2 | 5 |
| PreToolUse | Agent *(model nudge)* | outros | 9.722 | 39 | 39 | **0** | **0** | **39** |
| PreToolUse | TaskCreate / TaskUpdate / Write / Read | outros | ~8,4 kB | 49 | 0 | 0 | 0 | 0 |
| PostToolUse | SendMessage / Read / Agent / TaskStop / mcp__plugin | outros | ~4,3 kB | 23 | 0 | 0 | 0 | 0 |
| Stop | | outros | 432 | 2 | 0 | 0 | 0 | 0 |

**POR EVENTO (Touring)**:

| evento | bytes | blocos | com_prop | aderiu | tocou | **adesão** | **impacto** |
|---|---:|---:|---:|---:|---:|---:|---:|
| **PreToolUse** | 11.222.187 | 17.156 | 13.596 | 177 | 7.046 | **1,3%** | **51,8%** |
| SessionStart | 1.316.511 | 874 | 659 | 0 | 113 | 0,0% | 17,1% |
| PostToolUse | 1.295.705 | 7.456 | 935 | 0 | 333 | 0,0% | 35,6% |
| PostToolBatch | 358.957 | 1.329 | 1.311 | 0 | 985 | 0,0% | **75,1%** |
| PostToolUseFailure | 116.841 | 504 | 334 | 0 | 214 | 0,0% | 64,1% |

**POR FAMÍLIA (Touring)**:

| família | bytes | blocos | com_prop | aderiu | tocou | **adesão** | **impacto** |
|---|---:|---:|---:|---:|---:|---:|---:|
| **pre-edit** (enriquecimento) | 8.540.118 | 9.142 | 6.750 | 0 | 4.044 | 0,0% | **59,9%** |
| outros (cli-suggest, lições passadas) | 4.067.878 | 13.033 | 8.041 | **177** | 3.525 | **2,2%** | 43,8% |
| plugin-hook (dashboards) | 509.641 | 575 | 361 | 0 | 21 | 0,0% | **5,8%** |
| code-mode-deny | 407.588 | 252 | 252 | 0 | 80 | 0,0% | 31,7% |
| post-tool-echo | 358.957 | 1.329 | 1.311 | 0 | 985 | 0,0% | 75,1% |
| evidence-guard | 307.755 | 2.081 | **0** | 0 | 0 | — | — |
| post-edit | 118.264 | 907 | 120 | 0 | 36 | 0,0% | 30,0% |

**O que a tabela diz, hook a hook:**

1. **O enriquecimento é o canal que funciona — e não por obedecer.** `PreToolUse:Edit` (blast
   radius) tem **3.119 de 5.048** blocos que levaram o modelo a **tocar o alvo nomeado**
   (61,8%) e apenas **3** a executar o comando sugerido. O bloco não manda fazer nada: ele
   **informa** ("`harness_concordancia` tem 6 chamadores"). É o **maior volume do Touring**
   (8,54 MB = 56% da injeção) e o de **maior impacto (59,9%)**.
2. **O sermão quase não adere.** `PreToolUse:Bash` — 4.098 blocos com proposição: **164**
   aderiram ao comando (4,0%), **3.204 não fizeram nada (78,2%)**.
3. **O nudge de modelo é letra morta.** `PreToolUse:Agent` — **39 de 39 blocos: nenhum
   adhesion, nenhum alvo tocado**. O MUST `touring decompose validate <task_id>` nunca mudou
   nada em 400 sessões.
4. **O dashboard quase não serve.** `plugin-hook`: 361 blocos, 21 tocaram (**5,8%**), 0
   aderiram. O painel ATZILUTH, o dashboard slim e a sugestão de skill são, no conjunto,
   ruído caro.
5. **O `PostToolBatch` tem o maior "impacto" (75,1%) — e é o defeito §5.3.** Ele "impacta"
   porque nomeia paths — **o mesmo path repetido**. O número é alto *e* a qualidade é nula.
6. **`evidence-guard` não é medível por este método**: 2.081 blocos, **zero** proposições
   extraíveis (ele nomeia lições, não comandos nem alvos). A tabela não diz que ele não
   funciona — diz que **ninguém mediu**.

**⚠ RESSALVAS honestas do método** (o que invalidaria a leitura):

- **"tocou" é um proxy de impacto, não resultado.** Correlação: o bloco nomeou o arquivo e o
  modelo foi lá. Isso **não** prova que o resultado melhorou — só que o contexto mudou a
  ação seguinte. O contrafactual (o modelo teria ido lá de qualquer jeito?) **não está
  controlado** aqui.
- **A janela é de 3 chamadas.** Uma janela maior elevaria "tocou" e reduziria "nenhum".
- **Os blocos sem proposição ficam fora das taxas** — e neles está o `debug-stdout`
  (2.907 blocos, 9% do volume) e o `evidence-guard`.
- **Amostra: 400 sessões** de 3 projetos, não o corpus inteiro.
- **`pre-edit` e `outros` são rótulos de família porsubstring** — a fronteira é heurística.

**⇒ O veredito por canal, agora com número por hook:**

| canal | adesão a comando | impacto (proxy) | veredito |
|---|---:|---:|---|
| **fato** (enriquecimento) | ~0% (não manda) | **59,9%** | **funciona** — é o canal de maior volume |
| **sermão** (cli-suggest, lições) | **1,3%–2,2%** | 43,8% | **quase não funciona** |
| **dashboard** (plugin-hook) | 0% | **5,8%** | **não funciona** |
| **debug-stdout** | — | — | **não é contexto** (9% do volume) |

**Isto confere, por hook, a recomendação do interruptor**: `no-advisory` remove o canal medido
em **1,3%** e o dashboard em **5,8%**, e preserva o canal de **59,9%** — o único que carrega
conhecimento do repositório.

---

## 5. As quatro maneiras mensuráveis pelas quais a injeção atrapalha

1. **Repetição — 25,1%.** 15.410.680 bytes byte-idênticos a um bloco já na janela: entropia
   condicional zero. O plano de 04/09 mediu 35,1%; a dedup por conteúdo (S-1.1) **não foi
   entregue**. E a chave do cache do suggester é a **assinatura da ação**
   (`sig=outcome:edit:transcript-…`), não o conteúdo — sobrevive repetição com função igual.
2. **Log de depuração — 9,0%.** 5,3 MB de stdout cru, 1% de unicidade.
3. **Listas com item repetido.** O `PostToolBatch` monta a lista por **chamada**, não por
   **arquivo distinto**: `…/cli_suggester.rs` ×5.
4. **Contaminação entre projetos.** O *checking command history* consulta
   `key LIKE "outcome:{tool_class}:%:failure" LIMIT 15` — casa qualquer intent/arquivo sem
   filtro de relevância, e injeta falha de **outro projeto** dentro da sessão do `touring`.

---

## 6. O interruptor (entregue e provado)

```bash
TOURING_HOOK_INJECTION = full | no-advisory | off
```

| nível | o que faz | base empírica da recomendação |
|---|---|---|
| **`full`** (padrão) | nada muda; o shim faz `exec` direto, **zero overhead** | — |
| **`no-advisory`** | suprime as famílias de **sermão** e o **log de depuração**; mantém o **fato** e **todos** os gates, inclusive o payload do deny | remove o canal **medido em 7,6%**; preserva o canal **medido em 1,000 por construção** (coerção) e o canal **não medido** (fato) |
| **`off`** | suprime **todo** o contexto; os hooks **continuam bloqueando** | para medir o baseline sem injeção |

**A separação que sustenta o desenho** (verificada no código, não inferida):

| função | campos que escreve | file:line |
|---|---|---|
| `deny_response` | `permissionDecision` + `permissionDecisionReason` — **zero `additionalContext`** | `crates/touring-cli/src/cli_suggester.rs:5426-5435` |
| `advisory_response` | `additionalContext` — **zero `permissionDecision`** | `crates/touring-cli/src/cli_suggester.rs:4176-4184` |

⇒ **Desligar a injeção não cega o gate**: o deny chega ao modelo com o remédio (o programa
fundido) e só o sermão desaparece.

**Onde mora**: `client/hooks/injection_policy.py` (fonte única) + filtragem no shim
`scripts/hooks/touring-hook-shim.sh` (cobre ~95% dos bytes) + guard em cada hook Python/shell
advisory.

**Guarda de conformidade**: `scripts/guard_injection_policy.py` enumera os **152 registros** de
hook de `~/.claude/settings.json` e reprova quando um injetor não consulta a política —
**152/152 cobertos, 0 lacunas**.

**Testes**: `scripts/test_injection_policy.py` — **36/36 PASS**.

**Limitação conhecida**: `crates/touring-hook-runtime/src/verdict.rs:27` (`deny_reason_with`)
**cola os avisos dentro do `permissionDecisionReason`** (marcador `[também, sem bloquear]`).
Sob `off`, esses avisos ainda chegam — junto do deny. O corte exige agir ali, em Rust (§8).

---

## 7. A régua: o que ela mede e onde ela mente

`touring kpi -j` → `context_budget` já existe e já reprova:
`status: FAIL`, `failing: ["injected_bytes_per_turn", "injection_follow_ratio"]`.

**FACT [1.0]** — a régua tem **três** defeitos, e o segundo é o mais grave:

**(a) Amostra.** `transcripts_scanned = 3`, `user_turns = 9`, `must_emitted = 6`. O
`injection_follow_ratio = 0,5` que ela reporta é medido sobre **6** MUSTs; a medição de corpus
com 6.000 MUSTs dá **0,094**. Um veredito FAIL sobre n=6 é ruído com aparência de régua.

**(b) O ratio de adesão é matematicamente mal-formado.** O enum `GateEvent` tem **quatro**
colunas: `Emitted` (*"an advisory was shown (the gate spoke without denying)"*),
`Followed` (*"the next call adopted the canonical conversion"*), `Denied`, `Bypassed`
(`gate_metrics.rs:1500-1509`). O KPI compara `followed` com `emitted` — mas **`emitted` conta
os gates que *falaram*, e `denied` conta os que *impediram***. São populações diferentes:
`g3_edit_sem_read` tem `emitted=2` e `denied=9`, e `g7_reinspect` tem `emitted=6` e `denied=4`.
**Para um gate que nega, o desfecho correto é `Denied`, não `Followed`** — logo `followed=0`
num gate de negação é o **sinal de sucesso**, e o ratio mede o oposto do que diz medir.

**(c) Não há contador para o canal do fato.** A régua conta *proposições* (ações propostas),
não *fatos* (informação sobre o código) — e o valor do Touring está nos fatos. O `plugin-hook`
aparece com 10 blocos e **0 propositions**; `session-start`, `pre-edit`, `code-mode-deny` e
`evidence-guard`, também 0.

**Wave 2 da régua**: `fact_kb` (bytes de conhecimento único por família) e
`nudge_follow_ratio` (adesão do canal de permissão, sobre a população), além de separar
`followed` de `denied` no denominador.

---

## 8. Wave 2 nativa em Rust

### 8.1 O problema do interruptor atual

Ele vive **na borda**: um script bash + um módulo Python. Consequências medidas:

| limitação | detalhe |
|---|---|
| **custo por invocação** | 1 processo `python3` por hook (~15–25 ms) quando o nível ≠ `full`; a mediana do hook é 60–430 ms |
| **`systemMessage` — RESOLVIDO, e é canal de MODELO (verificado)** | ele é **mensagem, não decisão**: saiu de `DECISION_FIELDS` e passou a ser tratado como contexto (removido sob `off`, classificado sob `no-advisory`). **FACT [1.0]** — a régua de custo da janela **já o conta como injeção**: `context_budget.rs:369` (`for k in ["additionalContext", "systemMessage"]`) e `:375` (raiz). O instrumento que mede *o que entra na janela do modelo* o trata como entrada de janela; e `crates/touring-cortex/src/types.rs:145` diz *"All other events must use top-level `systemMessage` **for context injection**"*. ⇒ **minha inferência anterior (canal de usuário) estava ERRADA** — corrigida em 26/10/2026. O interruptor nativo tem de cobri-lo (sítio 1) |
| **não alcança os avisos colados no reason** | `verdict.rs:27` (§6) |
| **não tem telemetria própria** | o que foi suprimido não é contabilizado |

### 8.2 Onde a implementação nativa deve nascer

**FACT [1.0]** — `crates/touring-hooks/src/main.rs:670-709` (`forward_daemon_output`) é o
**único ponto de impressão do caminho daemon**, e **já é um pós-processador JSON no wire**:

```rust
687|     match serde_json::from_str::<serde_json::Value>(output.trim()) {
688|         Ok(mut value) => {
689|             let stamped = hook_response::stamp_invoking_event(&mut value);
690|             let removed = hook_response::split_off_invoking_event_violations(&mut value);
694|             if stamped || removed.is_some() {
695|                 println!("{value}");
```

O predicado entra ali como **mais um filtro no mesmo padrão** — parse → muta → re-serializa.
Por operar sobre o JSON final, ele pode remover `additionalContext` **sem tocar**
`permissionDecision`, `permissionDecisionReason`, `decision`, `reason`, `continue`, `stopReason`
nem `updatedInput`.

**FACT [1.0]** — `crates/touring-hooks/src/main.rs:268-271` faz
`let _ = touring_hooks::cli_suggester::run(&runtime, &input);` — o suggester (61% da injeção)
**é silencioso no caminho standalone**. Logo, para a família dominante, `daemon.rs:623` e
`main.rs:670` são o **mesmo nó lógico**, e o filtro do shim a alcança porque o shim repassa a
saída do daemon.

### 8.3 Os sítios de aplicação

| # | sítio | o que cobre | o que fica fora |
|---|---|---|---|
| 1 | `crates/touring-hooks/src/main.rs:687-699` (`forward_daemon_output`) | **100% do caminho daemon**, todos os eventos, `additionalContext` **e** `systemMessage` | o caminho standalone |
| 2 | `crates/touring-hooks-core/src/hook_response.rs` (`emit` :156-265, `to_json` :338-) | todo handler que devolve o **tipo** `HookResponse`, nos dois caminhos | `cli_suggester` (devolve `String`), `prompt_enhance::compose_json` (`Value`), `classify.rs::build_output` (`Value`), `post_compact_handler::run` (`String`), `team_hooks::subagent_bootstrap_context` (`String`) |
| 3 | `crates/touring-hook-runtime/src/hook_runtime.rs` (`emit` :113-240, `to_json` :140-153) | o **tipo duplicado** do (2) — sem ele, o `use` diverge (lição *declaração lida como referência*) | — |

E um quarto, para a contaminação do §6:
| 4 | `crates/touring-hook-runtime/src/verdict.rs:27` e `:37` | sob `off`, os avisos **não** são colados no `permissionDecisionReason` nem no `additionalContext` — **sem tocar a razão original do deny** | — |

### 8.3.1 Fatos verificados da implementação nativa (A4, 26/10/2026)

**FACT [1.0]** — **`TOURING_HOOK_INJECTION` tem ZERO ocorrências em Rust.** Varredura de todos os
`.rs` de `crates/`. ⇒ A Wave 2 é **campo livre**: não há código nativo a reconciliar, e a
premissa "dois lugares onde a regra mora" já nasce eliminada.

**FACT [1.0]** — existem **exatamente duas** definições de `HookResponse`
(`grep -rn "enum HookResponse" crates --include=*.rs`), e a **cadeia de reexport** que decide
qual chega a cada consumidor:

| caminho público | resolve para | file:line |
|---|---|---|
| `touring_hook_runtime::hook_response::HookResponse` | **canônico** (`touring-hooks-core`) | `crates/touring-hook-runtime/src/lib.rs:98` → `pub use touring_hooks_core::hook_response;` |
| `touring_hook_runtime::HookResponse` | **duplicado** (`hook_runtime.rs:60-107`) | `crates/touring-hook-runtime/src/lib.rs:82` → `pub use runtime::{HookResponse,…}` → `runtime/mod.rs:21-23` → `pub use crate::hook_runtime::{HookResponse,…}` |
| `touring_dispatch::hook_response::HookResponse` | **canônico** | `crates/touring-dispatch/src/lib.rs:245` |
| `touring_dispatch::hook_runtime::HookResponse` | **duplicado** | `crates/touring-dispatch/src/lib.rs:242` → `pub use touring_hook_runtime::hook_runtime;` |

⇒ **O mesmo crate expõe os DOIS tipos, em caminhos diferentes.** Qualquer consumidor que
importar pelo caminho curto recebe o duplicado, e o predicado implementado num não cobre o
outro — exatamente a lição *declaração lida como referência* do repo. **A correção mínima é
na raiz**: `runtime/mod.rs:21-23` deve reexportar o tipo **canônico**, e o `enum` duplicado
(`hook_runtime.rs:60-107`) sai. Aí passa a existir **um** tipo, com dois caminhos de acesso
— e o predicado tem um só lugar por definição.

**FACT [1.0]** — o caminho **standalone** do binário `touring-hook` **não tem choke point
único**: as impressões são dispersas em `crates/touring-hooks/src/main.rs` — `:261`, `:284`
(`.emit()`), `:342`, `:413`, `:432`, `:458`, `:481`, `:543`, `:555`, `:564`, `:606`, `:742`,
`:1175`. O caminho **daemon** tem dois pontos de saída, e **ambos** passam por
`forward_daemon_output` (`:179` e `:830`). ⇒ **A estratégia da Wave 2 tem dois braços:**
(a) o daemon, coberto por 1 sítio; (b) o standalone, que precisa ou passar a filtrar também
— seguindo o precedente de `injection_dedup::dedup_hook_output`
(`crates/touring-hooks-shared/src/injection_dedup.rs:271-312`), que já faz exatamente isto no
daemon: extrai o campo, transforma, re-serializa, **fail-open**.

**FACT [1.0]** — `crates/touring-dispatch/src/daemon.rs:623`
(`let output = maybe_dedup_output(runtime, &payload, output);`) é onde **todos** os handlers
da dispatch table convergem, e `maybe_dedup_output` (`:522-537`) chama
`injection_dedup::dedup_hook_output` (`:529`). **É o precedente mais próximo do que a Wave 2
precisa** — e o lugar onde o filtro nativo caberia se o critério fosse "ponto central do
daemon" em vez de "ponto de impressão".

**FACT [1.0]** — `cli-suggest` standalone **descarta** a saída
(`crates/touring-hooks/src/main.rs:268-271`: `let _ = touring_hooks::cli_suggester::run(...)`).
⇒ **Para a família dominante (61% da injeção), o caminho que importa é o do daemon**, e ele é
coberto pelo sítio 1.

### 8.3.2 O `systemMessage` — resolvido, e com uma consequência operacional

**FACT [1.0]** — a decisão: **`systemMessage` é removido sob `off`**; sob `no-advisory` só sai
se for **exclusivamente** sermão — a mesma regra das demais famílias. O interruptor Python já
está assim (`client/hooks/injection_policy.py`: `CONTEXT_FIELDS = ("additionalContext",
"systemMessage")`, `DECISION_FIELDS` sem `systemMessage`, e `_strip_context` varre os **dois**
holders — raiz e `hookSpecificOutput`). O Rust tem de ser igual.

**FACT [1.0]** — o que o repo **prova** sobre a natureza do campo: `systemMessage` é
**top-level e distinto** de `additionalContext`
(`crates/touring-hooks-core/src/hook_response.rs:475-484`;
`crates/touring-hooks/tests/standalone_parity.rs:14-25`), e apenas **3 eventos** aceitam
`hookSpecificOutput` (`crates/touring-cortex/src/types.rs:148-155`; fábrica
`crates/touring-cortex/src/context.rs:281-343`).

**FACT [1.0] — e é canal de MODELO, provado.** A evidência decisiva é a **própria régua de
custo da janela**: `crates/touring-cli/src/cli/context_budget.rs:369` itera
`for k in ["additionalContext", "systemMessage"]` dentro de `hookSpecificOutput`, e `:375` lê
`parsed.get("systemMessage")` na raiz. **O instrumento que mede "o que entra na janela do
modelo" conta `systemMessage` como conteúdo de janela.** E
`crates/touring-cortex/src/types.rs:145` afirma, na primeira pessoa do código:
*"All other events must use top-level `systemMessage` **for context injection**"*.

⇒ **A inferência que eu havia publicado nesta análise — "canal de usuário, preservado" —
estava ERRADA**, e a revisão de segurança que a contestou estava certa. Corrigido em
26/10/2026. A consequência é material: **o interruptor agora cobre todo o contexto que o
modelo vê, e não só o `additionalContext`.**

**FACT [1.0] — e a consequência que fecha um círculo:** `systemMessage` e `additionalContext`
**nunca coexistem** hoje (`crates/touring-cortex/src/context.rs:288-332` escolhe **um** ou o
outro). ⇒ **remover `systemMessage` só produz efeito nos eventos que NÃO têm
`hookSpecificOutput`** — ou seja, `SessionStart`, `Stop`, `PreCompact`, `SubagentStart/Stop`,
`TeammateIdle`, `TaskCompleted`.

**⇒ E é exatamente aí que moram os dashboards.** Isto liga o `systemMessage` ao conflito da
§8.4.2: sob `off`, os dashboards do `SessionStart` **saem** pelo canal do `systemMessage`
(sem depender da classificação de família); sob `no-advisory`, eles só saem se a família for
classificada como sermão. **A classificação decide o `no-advisory`; o `off` não depende dela.**

**O filtro em Rust passa a ter 4 alvos, não 2:**

```rust
for (path, field) in [("", "additionalContext"), ("", "systemMessage"),
                      ("/hookSpecificOutput", "additionalContext"),
                      ("/hookSpecificOutput", "systemMessage")] {
    strip_field(value, path, field, level, &mut outcome);
}
```

### 8.4 O módulo da política — onde ele mora, e por quê

**FACT [1.0]** — o destino é **`crates/touring-foundation/src/injection_policy.rs`**, com
**re-export** em `crates/touring-hooks-shared/src/lib.rs`. Duas evidências independentes:

**(a) A direção de dependência.** `touring-hooks` — o binário que vai chamar a política —
declara `touring-foundation` **direto** e **não** declara `touring-hooks-shared`. Pôr o módulo
em `touring-hooks-shared` **criaria uma aresta nova** (`touring-hooks → touring-hooks-shared`);
`touring-foundation` **já é** dependência direta do binário. `touring ast blast` de
`crates/touring-foundation/src/lib.rs` devolve `blast_radius: 232`, e **todos** os candidatos ao
uso da política estão nessa lista (`context_budget.rs`, `cli_suggester.rs`, `main.rs`,
`daemon.rs`, `hook_registry.rs`, `hook_runtime.rs`, `hook_response.rs`, os `hooks/*` do
`touring-hook-handlers`, `turn_budget.rs`, `cila.rs`, `index_policy.rs`).

**(b) O precedente que é O MESMO PADRÃO, já resolvido uma vez.**
`crates/touring-foundation/src/cila.rs:1-13` documenta, com as próprias palavras:

> *"Lives in `touring-foundation` because two crates were computing it independently and
> disagreeing: `touring-hooks-shared::cila` (read 800/2000/4000) and
> `touring-cortex::enrichment::compute_context_budget` (L2 = 1200, L4-5 = 3200, L6+ = 4800) —
> different numbers at 3 of 5 levels, for the same question. Both crates depend on this one, so
> the answer can be given once. `touring-hooks-shared::cila` re-exports these, which keeps its
> thirteen call sites untouched."*
>
> *"Measured consequence of the split (04/09/2026): tightening one scale never moved the other,
> so the dial appeared not to work."*

⇒ **É a mesma falha que a política de injeção vai ter** (a régua e o executor contando
famílias em lugares diferentes), **com a mesma lição**, e **já foi resolvida uma vez** — do
mesmo jeito: uma fonte, com re-export que mantém os call sites intactos. E a consequência
medida ("apertar uma escala nunca moveu a outra, então o dial parecia não funcionar") é
**exatamente** o sintoma que a §7 descreve para a régua de injeção.

**O módulo, portanto** (forma, não implementação):

- **a taxonomia de famílias é UMA tabela**, hoje duplicada em
  `crates/touring-cli/src/cli/context_budget.rs` (`injection_family` ~:200-244,
  `attributed_family` ~:298-307, `EMITTERS` ~:264-283) — ela migra para
  `touring-foundation`, e a régua passa a ler de lá. **É o que impede que política e régua
  contem coisas diferentes** (D8: *enforcement mora no executor, não no anúncio*).
- **a resolução do nível** lê `TOURING_HOOK_INJECTION` (hoje com **0 ocorrências em Rust** —
  campo livre), com precedência sobre a config do projeto e default `full`.
- **o predicado** é **uma** função, `permite_injecao(familia) -> bool`, chamada pelos 4
  sítios. Se a chave é lida em 3 lugares, os 3 chamam a mesma função — não 3 cópias da regra.
- **o tipo da linha de contagem** é uma `struct` (não `HashMap<String, i64>` com literais
  espalhados): é o que impede em Rust a divergência que o Python só pegou em runtime
  (§8.10).

### 8.4.1 O esqueleto Rust (A4, 26/10/2026)

`crates/touring-foundation/src/injection_policy.rs`, com `pub mod injection_policy;` em
`crates/touring-foundation/src/lib.rs`:

```rust
pub const ENV_VAR: &str = "TOURING_HOOK_INJECTION";
/// Marcador dos avisos colados na razão de um deny (migrado de
/// `crates/touring-hook-runtime/src/verdict.rs:20`).
pub const ALSO_NON_BLOCKING: &str = "[também, sem bloquear]";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InjectionLevel { Full, NoAdvisory, Off }

/// Default fail-open: valor mal digitado NÃO apaga a injeção.
pub const DEFAULT_LEVEL: InjectionLevel = InjectionLevel::Full;

/// Decisão pura, injetável (molde de `code_gates_disabled_from`,
/// `crates/touring-cli/src/cli_suggester.rs:3940`).
#[must_use]
pub fn level_from_raw(raw: Option<&str>) -> InjectionLevel {
    raw.and_then(InjectionLevel::parse).unwrap_or(DEFAULT_LEVEL)
}

/// Só o CLIENTE resolve pelo ambiente: o env do daemon é o do boot
/// (`crates/touring-cli/src/cli_suggester.rs:3935-3936`).
#[must_use]
pub fn level_from_env() -> InjectionLevel {
    level_from_raw(std::env::var(ENV_VAR).ok().as_deref())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FamilyKind { Advisory, Fact, Unknown }

#[derive(Debug, Clone, Copy)]
pub struct Family {
    pub name: &'static str,
    pub kind: FamilyKind,
    pub markers: &'static [&'static str],
}
```

**Três decisões que o esqueleto já fixa, e que valem registrar:**

1. **A decisão é uma função PURA e injetável** (`level_from_raw`), com o molde exato de
   `code_gates_disabled_from` (`cli_suggester.rs:3940`). Isso é o que permite testar a
   política **sem ambiente** — e o que permite ao **daemon** resolver uma vez e propagar.
2. **O default é fail-open** (`full`): valor mal digitado **não** apaga a injeção.
3. **`ALSO_NON_BLOCKING` migra para o módulo compartilhado.** É o que permite ao sítio 4
   (`verdict.rs:27`) **parar de colar os avisos dentro da razão do deny** sob `off` — o
   predicado passa a ter acesso ao marcador de onde ele precisa agir, sem duplicar a
   constante (e sem o risco de divergência que a §8.10 documenta).

### 8.4.2 O conflito era uma **colisão de NOME**, não de conteúdo — resolvido

**FACT [1.0]** — o nome `session-start` designava **duas coisas diferentes**:

| lado | marcadores | o que é |
|---|---|---|
| **régua** (`context_budget.rs:219-227`) | `Touring Knowledge`, `ATZILUTH`, `SCOUT PERP`, `TOURING (slim)`, `Painel` | os **dashboards** |
| **interruptor Python** | `CODE MODE — contrato`, `Touring: registering new file` | o **contrato do Code Mode** — o payload que carrega o remédio do gate |

**FACT [1.0]** — a medição da §4.6 mediu os dashboards (na família `plugin-hook` do Python)
com **5,8% de impacto e 0% de adesão** (25 de 361 blocos tocaram o alvo), contra **59,9%** do
enriquecimento — **10× menos eficaz por byte**.

⇒ **O Python já estava semanticamente certo** (dashboards em `plugin-hook` = Advisory;
contrato em `session-start` = Fact). O que estava errado era o **NOME**, que permitia que a
régua e a política falassem sobre coisas diferentes.

**Correção aplicada (26/10/2026):**

| taxonomia | antes | depois |
|---|---|---|
| **interruptor Python** | `session-start` (contrato) | **`session-start-contract`** (Fact) — nome próprio, colisão impossible |
| **interruptor Python** | `plugin-hook` (dashboards) | `plugin-hook` (**Advisory**) — inalterado |
| **régua** (`context_budget.rs`) | `session-start` (dashboards) | **`plugin-hook`** — fundir com a família `plugin-hook` que a régua já tem (`context_budget.rs:274`), em vez de criar um terceiro nome. Custo zero na contagem (a régua conta por nome, não por kind — `:670`). Um terceiro nome trocaria uma colisão por outra, e o princípio "o nome é o contrato" passaria a valer entre dois pares em vez de um |

**O NOME passa a ser o contrato entre a régua e a política** — e nenhuma das duas precisa
saber a classificação da outra. Custo da renomeação na régua: **zero** (ela conta por nome,
não por kind — `context_budget.rs:670`).

**A decisão de classificação (que é do Gabriel):** dashboards = **`Advisory`**. Minha leitura:
os **6,9% ≠ 0** são coincidência de path — o dashboard nomeia *muitos* arquivos, e o modelo
estava editando um deles de qualquer jeito; e a adesão de comando é **0%**. A alternativa
(um 4º `FamilyKind::Menu`, preservado em `no-advisory` e cortado em `off`) engrossa o
predicado para preservar um valor que a medição não comprova.

**Recorte que a §8.3.2 acrescenta**: o `off` **não depende desta classificação** — os
dashboards do `SessionStart` saem pelo canal do `systemMessage` de qualquer forma. A
classificação só decide o **`no-advisory`**, que é justamente o nível recomendado. Então a
divergência não é acadêmica: ela define **o que o interruptor recomendado faz**.

**Decisão aplicada (26/10/2026): dashboards = `Advisory`**, cortados pelo `no-advisory`.
Razão: **6,9% de alcance com 0% de adesão de comando** e 10× menos eficaz por byte que o
enriquecimento — a assinatura de um sermão, não de um fato.

| alternativa | efeito | quando escolher |
|---|---|---|
| `FamilyKind::Menu` (novo): preservado em `no-advisory`, cortado em `off` | preserva os 25 blocos de 361 que tocaram um alvo | se o A/B (§8.8) mostrar que o dashboard tem valor de resultado |
| `Advisory` (aplicado) | corta 5,8% do volume | se o A/B confirmar 6,9% ≈ 0 |

**É uma linha**, se a decisão mudar: a taxonomia é uma tabela de `FamilyKind`, e nenhum outro
código depende da escolha.

**Nota de método**: a divergência nasceu porque a taxonomia Rust nasce da **régua**
(`context_budget.rs:201-237`, que classifica por *atributo do emissor*) e a §4.6 mede por
*efeito observado*. As duas respondem perguntas diferentes — e a §4.6 mostra que a pergunta
da régua **não basta** para decidir o que cortar. O conserto não é escolher uma das duas: é
**fazer o NOME ser o contrato**, para que as duas possam ser comparadas sem colidirem.

### 8.5 Telemetria nova

Contadores no formato de `crates/touring-foundation/src/gate_metrics.rs` /
`gate_metrics_snapshot.rs`:

| contador | o que mede |
|---|---|
| `injection_suppressed_bytes_count` | bytes de contexto **suprimidos** pelo interruptor |
| `injection_suppressed_blocks_count` | blocos suprimidos |
| `injection_suppressed_by_family` | o mesmo, por família — a régua passa a poder mostrar *o que* foi cortado |
| `injection_policy_active` | o nível em vigor (`full`/`no-advisory`/`off`), para que a régua diga **sob qual política** foi medida |

Sem isso, medir o interruptor exige reconstruir do transcript — que é o que a rodada 1 fez, e
custa minutos.

### 8.6 Plano de testes

| # | teste | onde | prova |
|---|---|---|---|
| 1 | `off` preserva `permissionDecision` + `permissionDecisionReason` | `crates/touring-hooks/tests/` | o gate continua falante |
| 2 | `off` remove `additionalContext` | idem | o contexto some |
| 3 | `no-advisory` preserva o payload de enriquecimento | idem | o fato não se perde |
| 4 | `no-advisory` remove o advisory | idem | o sermão sai |
| 5 | `full` é **byte-idêntico** ao comportamento atual | idem | o default não mexe |
| 6 | o predicado é **uma** função, chamada pelos 3 sítios | `crates/touring-hooks-shared/tests/` | nada diverge |
| 7 | `verdict.rs` sob `off` não cola aviso no reason | `crates/touring-hook-runtime/` | a contaminação morre |
| 8 | A/B de corpus: `s8_corpus_ab.rs` roda com `full` e com `no-advisory` e compara o resultado | `crates/touring-hooks-shared/tests/s8_corpus_ab.rs` (existente) | **o contrafactual do canal do sermão** |

### 8.7 Sequência de implementação

| passo | o quê | reversibilidade |
|---|---|---|
| **0** | **unificar o tipo**: `runtime/mod.rs:21-23` reexporta o `HookResponse` **canônico** de `touring-hooks-core`; o `enum` duplicado (`hook_runtime.rs:60-107`) sai. Sem isto, o predicado teria dois lugares por definição | reversível; é refactor puro, gate de testes |
| 1 | módulo da política em Rust em **`crates/touring-foundation/src/injection_policy.rs`** + re-export em `touring-hooks-shared` + testes unitários | total (módulo novo, não usado) |
| 2 | mover a taxonomia de família de `context_budget.rs` para `touring-foundation`, com a régua lendo de lá (o mesmo movimento do `cila`) | total (refactor puro, gate de testes) |
| 3 | aplicar em `forward_daemon_output` (sítio 1) | reversível por 1 env var |
| 4 | aplicar em `hook_response.rs` + `hook_runtime.rs` (sítios 2 e 3) | reversível |
| 5 | `verdict.rs` (sítio 4) | reversível |
| 6 | telemetria nova + `touring kpi -j` expõe o nível | aditiva |
| 7 | **A/B de corpus** (`s8_corpus_ab.rs`): `full` × `no-advisory` × `off`, comparando resultado, não adesão | é o que fecha a lacuna do canal do fato |
| 8 | propagação: `update-touring` (build release do workspace) | exige o gate de release |

**O passo 7 é o que a rodada 2 provou que falta**: até agora só medimos **adesão**, e adesão não
é **resultado**. A pergunta "o enrichment melhora o resultado do modelo?" só o A/B responde.

### 8.8 A prioridade mudou depois da §4.6

A §4.6 estableceu **quais** canais comparar, e isso reordena a Wave 2:

| # | experimento A/B | o que decide | por quê é o primeiro |
|---|---|---|---|
| 1 | `full` × `no-advisory`, no mesmo corpus, comparando **resultado** (a solução final, os testes, o número de erros) | se o canal de sermão (1,3% de adesão) tem **algum** valor de resultado | é o único canal com adesão quase nula — se não ajuda, sai sem custo |
| 2 | `full` × `off` | se o canal de **fato** (59,9% de impacto) tem valor de resultado | decide se o interruptor pode ir além de `no-advisory` |
| 3 | `full` × `no-advisory` × `off` no **mesmo transcript** (replay) | o contrafactual que falta | as três arms, um corpus, resultado comparável |

**Instrumento existente**: `crates/touring-hooks-shared/tests/s8_corpus_ab.rs` já faz A/B de
injeção sobre corpus (`turn_budget 2000 no cli-suggest — o binário novo completo, o que o juiz
mede`). **Não há que inventar o harness** — há que estendê-lo às três arms da política.

### 8.9 Correções de segurança aplicadas nesta rodada

Duas, vindas da revisão de segurança automatizada, e **as duas estavam certas**:

| achado | correção | prova |
|---|---|---|
| **`systemMessage` estava em `DECISION_FIELDS`** — o nível `off` não desligava tudo, porque uma mensagem continuava chegando | `systemMessage` saiu da lista de decisões e virou **campo de contexto** (`CONTEXT_FIELDS`), com o mesmo regime de `additionalContext`: removido sob `off`, classificado sob `no-advisory` | 5 regressões novas em `scripts/test_injection_policy.py` (41/41 no total) |
| **`instructions_loaded_log.py` escrevia um log com `file_path` e `cwd` em modo 0644** (confirmado: `-rw-r--r--`) | arquivo criado e mantido em **0600** (`os.open(..., 0o600)` + `os.fchmod`, que fecha também um arquivo já permissivo); diretório criado com `mode=0o700`; e o campo **`cwd` foi removido** — o agregador (`scripts/diagnostics/kpi_contexto_sessoes.py`) **não o usa**, enquanto `session_id`, `file_path`, `load_reason` e `bytes` **precisam** ficar (redigir quebraria a medição que o hook existe para produzir) | log real fechado para `0600` e reescrito com sucesso, com a linha sem `cwd` |

**FACT [1.0]** — o `cwd` foi o único campo removível: a varredura do agregador mostra uso de
`session_id` (agrupamento), `file_path` (`tipo_da_fonte`) e `load_reason`, e **nenhum uso de
`cwd`**. A exposição residual do `file_path` é tratada por permissão, não por apagamento.

### 8.9.1 A segunda rodada da revisão de segurança — e o que era defeito vs. decisão

A revisão voltou com dois achados, ambos marcados "fail-open", e **sem sugestão de correção**
(*"Suggested fix: N/A"*). Medi o alcance real de cada um antes de mexer:

**Achado 1 — `allows_payload()` deixa passar o que não reconhece.**

**FACT [1.0]** — o `off` é **incondicional**: `allows_payload("texto sem nenhum marcador",
"off")` → `False`. **O nível que promete "off" não tem fail-open.** O fail-open existe só no
`no-advisory`, onde `allows_payload(..., "no-advisory")` → `True`.

**INFERENCE [0.9]** — e ele é **uma decisão medida, não um descuido**: o canal de enriquecimento
(blast radius, file context, syntax pre-check) é o de **maior volume e maior impacto** (59,9%)
e **não tem marcador confiável** — classificá-lo por substring descartaria conhecimento do
repositório. O ruído que existe (o log de depuração, 9% do volume) tem marcador próprio
(`debug-stdout`) e **é cortado**.

**O que foi feito**: a assimetria foi **declarada** no docstring do módulo (tabela por nível),
**fixada por 4 testes** (inclusive "off é incondicional" e "no-advisory preserva o
enriquecimento"), e a mitigação — a guarda que valida os NOMES de família — está no lugar.
**O que NÃO foi feito**: mudar o comportamento. Fechar o `no-advisory` para o desconhecido
significaria descartar o canal de maior impacto em troca de cortar um sermão com marcador fora
da taxonomia — que é um preço pior, não melhor.

**Achado 2 — a guarda usava heurística.**

**FACT [1.0]** — duas fragilidades reais, medidas:

| fragilidade | escopo | correção |
|---|---|---|
| a guarda cobria um hook procurando o **NOME do módulo** no arquivo | **3 falsos positivos** (`doc-coverage-gate.sh` ×2 e `check_context.sh` mencionam o módulo no comentário mas chamam a CLI `--allows`, não a função Python) | agora exige uma **CHAMADA**: `allows(` (Python) **ou** `--allows` (shell) |
| um hook "misto" (injeta **e** bloqueia) era considerado coberto **por bloquear** | **0 "misto" hoje** — buraco latente, não vivo | "misto" passa a exigir a política como qualquer injetor: bloquear não cobre o contexto que ele injeta |

**Verificação**: a guarda fecha em **152/152, 0 lacunas, 0 nomes obsoletos, rc 0**, e os hooks de
shell seguem cobertos pela forma estrita.

### 8.10 Uma lição da própria implementação da Wave 2 — com um incidente que **chegou ao vivo**

Enquanto o módulo nativo era escrito, o script de medição encostou **dois** bugs da mesma
família — e ambos são exatamente o que a Wave 2 vai repetir em Rust:

1. **Chave escrita como literal em N lugares diverge.** A chave de família estava `familias` na
   definição e `families` (inglês) num dos acessos; o grep pela grafia portuguesa **não achava**
   o acesso em inglês. ⇒ **A tabela de famílias em Rust tem de ser UMA constante**, e o tipo
   da linha de contagem tem de ser **um tipo**, não um `HashMap<String, i64>` com literais
   espalhados. A lição do repo já existe ("predicado escrito duas vezes nunca se encontra":
   `finalized` × `completed`) — aqui com o agravante da **outra língua**.
2. **`defaultdict` + `+=` em chave ausente executa a aritmética sobre o valor da fábrica.**
   `target[k] += v` dispara `__missing__`, que **arma o dict inteiro** no lugar da chave ⇒
   `dict + int`. ⇒ Em Rust, o acumulador é `HashMap<Chave, Contadores>` com `Contadores` como
   **struct** — o compilador impede a divergência que o Python só pegou em runtime.

### 8.11 ⚠ O incidente: um rename que **chegou ao vivo** com falha silenciosa

**FACT [1.0]** — o rename `session-start` → `session-start-contract` (§8.4.2) deixou **4 call
sites** com um nome que **não existe**. E o modo de falha era o pior possível: **`allows()`
com família desconhecida devolvia `True`** (porque o nome não estava em `ADVISORY_FAMILIES`) ⇒
o hook **seguia injetando**, e **nada acusava**.

| call site | injetava | efeito do bug |
|---|---|---|
| `client/hooks/session_startup_intelligence.py:845` | dashboard | **não era cortado** sob `no-advisory` — o nível recomendado não fazia o que prometia |
| `client/hooks/investigate_on_start.py:88` | painel | idem |
| `client/hooks/code_mode_sdk_section.py:186` | contrato do Code Mode | injetava (por acaso, certo) |
| `client/skills/loop-engineering/scripts/hooks/loop_resume.py:281` | `[OUTER · flow` | injetava (por acaso, certo) — e **já estava errado antes do rename** |

**E a guarda passava verde**, porque ela só verificava que o hook **chama** `allows(...)` — não
que o **nome** existe. É a armadilha *"ausência de sinal tem duas causas"* aplicada ao
**próprio guard**: "o hook consultou a política" e "a política reconheceu o que o hook pediu"
são afirmações diferentes, e só a segunda detectava o defeito.

**Correção em duas frentes (as duas aplicadas):**

1. **`allows()` falha FECHADO para família desconhecida** — devolve `False` (não injeta). O
   modo de falha passa de "injeta por engano" para "não injeta", e a sessão não quebra (o
   predicado não levanta exceção).
2. **A guarda valida os NOMES** — extrai os `allows("<familia>")` dos call sites e reprova
   qualquer nome fora de `ADVISORY_FAMILIES ∪ FACT_FAMILIES`.

**Prova negativa (executada):** com um nome obsoleto injetado no `skill_picker_hook.py`, a
guarda passa a listar `NOMES INVALIDOS` e **sai com rc 1**; restaurado, volta a **rc 0**.

**Prova de que o bug era real (não teórico):** `session_startup_intelligence.py` sob
`no-advisory` passou de **934 B** (o dashboard sendo injetado) para **0 B** depois do conserto.

**Verificação no final (26/10/2026):** `152/152` registros cobertos · **0** nomes obsoletos ·
`46/46` testes · `rc 0` na guarda.

**A lição, que é a mesma do §8.10 item 1**: *o nome é o contrato*. Um nome de família escrito
em N call sites não é uma constante — é uma **cópia**, e cópias divergem. Em Rust, o nome da
família tem de ser um **tipo** (enum, com o taxonomia em um único lugar), não uma `&'static str`
repetida em 10 sítios. E o guard tem de verificar **o que foi pedido**, não apenas **que algo
foi pedido**.

---

## 9. Réplica do baseline

| métrica | plano 04/09 (10 transcripts) | medição 26/10 (200 sessões) | veredito |
|---|---:|---:|---|
| tool results na janela | 51,3% | 50,2% | **replica** |
| injeção na janela | 26,6% | 27,8% | **replica** |
| texto do usuário | 14,5% | 16,6% | **replica** |
| texto do assistente | 7,6% | 5,4% | **replica** |
| blocos byte-idênticos | 35,1% | 25,1% | **piora** (nunca corrigido) |
| `touring-suggest` como fração | 61% | ~20% | **divergência material**: o motor de enriquecimento amadureceu e passou a ser o bulk |

---

## 10. Mapa de cobertura da análise

| o que | evidência | onde |
|---|---|---|
| volume da injeção | 1.028 sessões / 61,5 MB / 27,8% da janela | §2 |
| mapa de todo o contexto injetado | 134 combinações / 7 eventos | §3 |
| **comportamento: persuasão** | 92 → 7 (7,6%); 46/84 resistidas; **adesão 1,3% por evento** | §4.1, §4.6 |
| **comportamento: coerção** | 13 negações; 1,000 por construção; 581,9 KB elidos | §4.2 |
| **comportamento: fato** | **59,9% de impacto** por transcript; **sem contador** de adesão em produção | §4.3, §4.6 |
| **comportamento: dashboard** | 5,8% de impacto, 0% de adesão | §4.6 |
| **adesão e impacto por hook e evento** | 54 combinações do Touring, 15,7 MB, 400 sessões | §4.6 |
| o orçamento nunca disparou | `ctx_budget_alert=0`, `ctx_budget_warning=0` | §4.4 |
| separação contexto × enforcement | `deny_response` vs `advisory_response` | §6 |
| interruptor | 36/36 testes, 152/152 guarda | §6 |
| a régua e suas 3 falhas | `followed` vs `emitted` | §7 |
| Wave 2 nativa | 4 sítios, telemetria, 8 testes, 8 passos | §8 |

---

## 11. ⚠ RODADA 3/4 — correções obrigatórias (drift) e o documento que a substitui

> **Esta seção corrige este documento.** A rodada 3/4 (documento
> **`IMPACTO-POR-HOOK.md`**) reescreveu o instrumento de medição e encontrou
> **quatro defeitos de instrumento**, um deles com efeito no volume real do
> contexto injetado. As seções §4.1, §4.6 e §10 **superadas** estão marcadas
> abaixo; o que **não** mudou está declarado.

### 11.1 O que a rodada 3/4 corrigiu

| # | defeito | consequência |
|---|---|---|
| D1 | a régua classifica a família **por cabeçalho**; o caminho de *orçamento de turno esgotado* emite o payload **sem cabeçalho** | **5.630 blocos caíam em "outros"** — a régua não via ~19% do contexto do Touring, e o invisível era o **sermão** |
| D2 | o SPEC de `evidence-guard`/`post-tool-echo` assumia "o bloco traz a falha anterior" | 100% **INDETERMINADO** por modelo de sinal errado (não por ausência de efeito) |
| D3 | o sinal de `code-mode-deny` media "o modelo rodou o comando" | o gate **reescreve** a chamada — o efeito não é uma tool_use seguinte |
| D4 | o instrumento **não filtrava `is_touring`** | media hooks de terceiros junto (2.171 blocos alheios) |

### 11.2 Números superados (usar os novos)

| # | onde | antes | **agora** |
|---|---|---|---|
| C1 | §4.1, §4.6, §10 | sermão = 906 blocos / 92→7 | **4.647 blocos; adesão proximal 0,0%; 2,1% cita sem agir** |
| C2 | §4.6 | `pre-edit` 58,5% conforme | **75,6%** |
| C3 | §10 | "persuasão 1,3% de adesão" | **≈ 0,4% agregado; 16,4% do contexto do Touring** |
| C4 | §7 | "a régua mede o que emite" | **a régua não vê 19% do contexto** (defeito de taxonomia) |
| C5 | §10 | "fato sem contador de adesão em produção" | mantido — e agora se sabe que **o fato é a única classe que funciona (≈52%)** |
| C6 | §6 (interruptor) | "`no-advisory` corta o sermão, os dashboards e o log de depuração" | **cortava só 13,6% do contexto** — a taxonomia por cabeçalho deixava passar o sermão sem cabeçalho. **Corrigido: 19,7%** (§8.1 de `IMPACTO-POR-HOOK.md`) |

### 11.3 O que **não** mudou

- **§2 (a janela)**: injeção = 27,8% — replicado, e coerente com 04/09 (26,6%).
- **§4.2 (coerção)**: 13 negações, 1,000 por construção, 581,9 KB elidos.
- **§6 (interruptor)**: `full` byte-idêntico; `no-advisory` corta sermão, dashboard e
  log de depuração; `off` corta tudo e **preserva as decisões**.
- **§8 (Wave 2 nativa)**: o desenho vale; a implementação **não** foi feita.

### 11.4 Os três achados que a rodada 3/4 adds

1. **`similar-fail` é um alerta sem alvo** (1.676 blocos, 5,9%): o
   `Pensieve` guarda o `reason` (`pensieve.rs:26-33`) e o produtor grava o
   comando (`post_bash.rs:199-206`), mas a API `check_known_failure` devolve
   **só o score** (`pensieve.rs:117,139`) e a mensagem sai sem nome
   (`pre_bash.rs:236`). **Defeito de interface, não de dado.**
2. **`[generic]`** — 1.499 blocos (5,3%) cujo payload é literalmente essa
   palavra. Ruído puro.
3. **O harness viola o próprio invariante** — `pre_read.rs:1-7` declara
   *"SILENCE IS THE DEFAULT / So what?"*, com teste em
   `pre_read_tests.rs:1220-1241` ("core invariant"), e **~34%** do contexto
   injetado viola os três testes que ele declara.

---

## 12. A RODADA 4 — o diagnóstico qualitativo (com as fontes externas)

> **Esta seção complementa (não substitui) §1–§11.** A rodada 4 mediu o impacto
> qualitativo à luz das melhores práticas externas: Context7 (65 docs first-party
> da Anthropic), notebook NotebookLM **"Claude Code "** (91 fontes, criado em
> 2026-09-25 — o do Gabriel, certificado em `round4/CERTIFICACAO-FONTES.md`),
> notebook **"GitHub Repos"** (107 fontes) e `google-research/harness2` + o paper
> **arXiv 2610.14224**. O documento da rodada é `round4/IMPACTO-QUALITATIVO-EXTERNO.md`;
> a matriz de 16 famílias é `round4/MATRIZ-DIAGNOSTICO.md`.

### 12.1 Os três mecanismos que explicam o veredito

1. ⚡ **A ordem de leitura do PreToolUse** — corrigida em C13: o `additionalContext` é gravado **depois** do `tool_use` no transcript (ordem de gravação), mas o hook roda **antes** da ferramenta executar (ordem de chamada). O `additionalContext` informa a **próxima** ação; o `permissionDecision`/`updatedInput` muda a **atual**. Ver `round4/CORRECAO-ORDEM-DE-LEITURA.md`.

---

2. **O canal é "contexto da aplicação", não ordem do operador** (doc oficial,
   `agent-sdk/modifying-system-prompts:1224`): *"Treat them as context from the
   application, not as messages from the user."* ⇒ um **fato** nesse canal é lido
   como dado; um **sermão** é lido como conselho que o modelo não deve obedecer.

3. **69,8% dos bytes injetados são repetição** — e a repetição dilui o resto
   (harness2: *"duplicated instruction dilutes the rest"*). O bloco repetido mais
   vezes é o **melhor** bloco do sistema (`[G2 … → REESCRITO]`, 399×) —
   **repetição e qualidade são eixos independentes**.

### 12.2 As correções da rodada 4 (C1–C9 + as minhas)

| # | onde | antes | agora |
|---|---|---|---|
| **C1** | a premissa | "o 'So what?' é a regra oficial da Anthropic" | **não é publicado como critério para o canal de hook** (65 docs, negativo); **mas** coincide com o conselho do time de AI aplicada |
| **C2** | §8.8 | "`s8_corpus_ab.rs` mede resultado" | **impossível** — mede **bytes**, não desfecho |
| **C3** | §8.3.2 | "os dashboards vão por `systemMessage`" | **contra** — o corpus mede **zero** `systemMessage`; os dashboards vão por `additionalContext` |
| **C4** | §4.6 | `outros` = residual | **`outros` é o resto, e contém os dois extremos** |
| **C5** | §4 | o contrato aparece em 4 lugares | **a letra existe em 1 lugar** (`pre_read.rs:3-7`); os outros são paráfrase |
| **C6** | §5.1 | `FailureMatch` é correção de superfície | **metade não é de superfície** — `FailedPath` não tem campo de comando |
| **C7** | §7 | "a régua mede o que emite" | **duas taxonomias em duas linguagens divergem em 9 famílias** |
| **C8** | §2 | "injeção = 27,8% da janela" | **12,0% medido hoje** — divergência metodológica **a reconciliar** |
| **C9** | a premissa do A/B | "a política é a lacuna" | **o dedup já existe e é maior** — o que falta é **alcance** |
| **C10** | (meu) a ordem de leitura | (não medido) | **100% dos `PreToolUse` chegam depois da decisão** |
| **C11** | (meu) as ordens inválidas | "0% de adesão" | **58,0% das ordens MUST chegam sintaticamente inválidas** (1.076/1.854) |
| **C12** | (meu) o teto de 10.000 | "10,2% dos bytes" | **1,83%** — a medição anterior contava terceiros |
| **C13** | §12.1 #1 (a ordem de leitura) | "a injeção chega tarde demais para influenciar a decisão que ela pretendia orientar" | **precisa de precisão** — o `additionalContext` **não** pode mudar a chamada atual (o protocolo não permite); mas **pode** informar a próxima. O veredito correto é: **o `additionalContext` é o canal do "depois"; o `permissionDecision`/`updatedInput` é o canal do "agora"**. A rodada 4 mediu a **ordem de gravação** do transcript, não a **ordem de percepção** do modelo — e o hook roda **antes** da ferramenta executar, mas **depois** do modelo decidir chamar. Ver `round4/CORRECAO-ORDEM-DE-LEITURA.md`. |

### 12.3 O que a rodada 4 entregou em código

| # | o que | onde |
|---|---|---|
| 1 | a taxonomia da régua passou a ser **por conteúdo** (catch-all 77,2% → **11,5%**) | `crates/touring-cli/src/cli/context_budget.rs` |
| 2 | o `similar-fail` passa a **nomear o comando que falhou** (`FailedPath.subject` + `check_known_failure_detail`) | `crates/touring-intelligence/src/reasoning/pensieve.rs` |
| 3 | a política Python passou a espelhar a régua Rust (**27 famílias, mesmos nomes**) | `client/hooks/injection_policy.py` |
| 4 | a suíte de testes da política passou de 70 → **89 PASS / 0 FAIL** | `scripts/test_injection_policy.py` |
| 5 | a ordem MUST passa a ser **executável** (S-7, em curso) | `crates/touring-hooks-shared/src/turn_budget.rs` |
| 6 | o alcance do dedup (S-4p, em curso) | `crates/touring-hooks-shared/src/injection_dedup.rs` |
| 7 | a política nativa em Rust (S-4, em curso) | `crates/touring-foundation/src/injection_policy.rs` |

### 12.4 O que a rodada 4 não fechou

- **S-4** (política nativa em Rust) · **S-5** (A/B de corpus com modelo no
  circuito) · **S-4p** (alcance do dedup) · **S-7** (ordem executável) — em curso
  ou pendentes.
- **4 engineers morreram por rate-limit** (HTTP 429) — o S-2 foi recuperado do
  working tree (cargo check + clippy + testes verdes); os outros estavam
  incompletos.
- **2 P0 workspace** (pré-existentes, não da rodada 4): `F2_1` (`build.rs` 0.2) e
  `F2_5` (**34 CVEs** em `ctutils` e `quinn-proto`).
- **1 clippy FAIL pré-existente** (`touring-quality/f3_1_py_imports.rs:130,131,198,261,387`
  — commit `e1bcd5a9`, fora do escopo).

### 12.5 O que a rodada 4 recomenda

1. **A regra operacional** (o "So what?" executável): injete só quando o bloco
   trouxer, **em uma frase**, (1) o estado medido com o número, (2) a ação com
   verbo no imperativo e alvo nomeado, (3) o porquê. **E cale** quando o fato já
   está na tela, é o conteúdo do arquivo, não tem consequência, ou não cabe numa
   frase.
2. **Para influenciar, o hook precisa reescrever ou negar** — informar em
   `PreToolUse` chega depois do ato. A alternativa é ir para um evento que
   precede a decisão.
3. **O dedup já existe; o que falta é alcance** (hooks Python, caminho
   standalone, campos da raiz). O A/B de desfecho exige **harness novo** (com
   modelo no circuito).

---

_Rodada 4 — documento principal: `round4/IMPACTO-QUALITATIVO-EXTERNO.md`.
Matriz: `round4/MATRIZ-DIAGNOSTICO.md`. Evidências: `round4/evidence-*.md`.
Correções: C1–C9 (architect) + C10–C12 (meu)._

---

## 13. A RODADA 5 — o canal de ferramentas (CORRIGIDA em 27/10/2026)

> ⚠ **Correção de 27/10**: os números de trio da rodada 5 (Edit 61,5% retry,
> `cat` 49,7% de eficácia, `ctx_execute` 95,1%, `cd`/`export` como classes)
> eram, em boa parte, **artefato do instrumento**: classe Bash pelo 1º token,
> retry por alvo e negação contada como erro. O instrumento foi versionado e
> corrigido (`scripts/measure_tool_trio.py` + fixture de 9 casos). Revisão:
> `../2026-10-26-touring-janela-eficacia/REVISAO-2026-10-27.md`. Plano v2:
> `../2026-10-26-touring-janela-eficacia/PLANO.md`.

A partição da janela da rodada 4 continua de pé (corpus J: resultados 43,2%,
chamadas 24,7%). Mudou o que há **dentro** do canal:

| medida (200 sessões recentes, 27/10) | valor |
|---|---|
| chamadas | 12.791 · resultado 23,0 MB |
| retry (chamada byte-idêntica, janela 8 = predicado do G6) | **1,5%** |
| negado por hook | **4,2%** — Edit 9,6% · Write 8,8% · `bash` 10,5% · `cp` 10,4% |
| Edit retry | **4,3%** (não 61,5%) |
| `cat` | `cat-write` 330 (heredoc; vazio = sucesso), 94,0% · `cat-read` 78, 94,9% |
| `grep` | eficácia **91,0%** (exit 1 = sem match, não erro) |
| `cd`/`export` | prefixos de comando composto (2.246/2.246 `cd`) — não são classe |
| `ctx_execute` 95,1% | artefato: o alvo (`code`) ficou fora do `cmd_norm` |
| maior canal de resultado | **Read 39,6%** (9,1 MB; 4,07 MB só texto) · `touring run` 15,4% · `python3` 12,0% |

**O que isso muda**: não há grande desperdício por repetição a cortar. A perda
medida está nas **negações dos gates** (cada uma custa uma volta; falta medir se a
REASON leva à rota certa) e no **volume do Read**. A compressão "estilo rtk" só
reduz a janela pelo canal `updatedToolOutput` (PostToolUse, todas as
ferramentas); o `maybe_capture_and_persist` atual **soma** texto
(`additionalContext`). O `PreCompact` não tem saída de contexto, então a evicção
na compactação está fora do alcance do Touring. `updatedInput` não devolve bytes
que o modelo já emitiu na chamada (`cd`/`export` repetidos).

As fontes externas (rtk, prime-agent, harness2, context-mode, fast-jev) seguem
em `round4/ESTRATEGIA-FERRAMENTAS.md` §2. As decisões aprovadas e as fases estão
no plano v2.


---

_Análise por TACO sob ordem de Gabriel Gadea, 26/10/2026 — rodadas 1, 2, 3/4, 4, 5 (§13 corrigida em 27/10).
Rodada 3/4: **`IMPACTO-POR-HOOK.md`** (conformidade por família, forma do efeito,
confronto com o contrato declarado) e `evidence/conformidade-por-familia-r3.txt`.
Rodada 4: **`round4/IMPACTO-QUALITATIVO-EXTERNO.md`** (o diagnóstico qualitativo,
com as fontes externas) e `round4/MATRIZ-DIAGNOSTICO.md` (a matriz).
Reprodutível: `scripts/measure_injection_effect.py`, `scripts/measure_injection_impact.py`,
`evidence/injectors-corpus.txt`, `scripts/test_injection_policy.py`,
`scripts/guard_injection_policy.py`, `touring gate-metrics -j` (comportamento)._
