---
type: Plan
title: Economia de Contexto — o token como recurso físico no harness do Touring
description: Plano de aperfeiçoamento do Touring sob a lei da janela como mana; diagnóstico medido do custo de injeção e do gate OUTER, e as sete frentes que o corrigem.
tags: [context-engineering, harness, cila, injecao, outer, kpi]
timestamp: 2026-09-04
plan: 2026-09-04-economia-de-contexto
plan_id: 2026-09-04-economia-de-contexto
intent: fazer cada token da janela atingir o maximo de utilidade e convergencia com o objetivo da tarefa
okf_version: 1
---

# Economia de Contexto (Pln2)

> **Nível**: L4 (arquitetural, atravessa hooks + cortex + cli + loop-engineering)
> **Lei que governa**: *a janela é o recurso mais precioso; não se trata de usá-la
> menos, e sim de fazer cada token atingir o máximo de utilidade e convergência
> com o objetivo da tarefa.*
> **Fonte teórica**: `agent-harness/Engenharia de Contexto e Harness.md` (696 L,
> 4 eixos). **Fonte empírica**: 10 transcripts, 406 turnos, 965 avaliações do Stop
> hook, censo F2.4 sobre 592 arquivos — tudo medido nesta sessão.
> **Nível operacional**: DAG `task_1788534325883539352` (2ª rodada — S-5.1b e
> S-4.3, 2/2 done). A 1ª rodada correu por `/goal`, sem DAG registrada.
> **Veredito**: `loop_converged.py` exit 0, 8 cláusulas (Platinum 0,937).

---

## 1. Ground Truth — o que a janela realmente paga

Medido em 10 transcripts reais deste projeto (406 turnos de usuário):

| origem | bytes | % da janela |
|---|---:|---:|
| **tool results** | 5.205.880 | **51,3%** |
| **injeção de hook** | 2.700.170 | **26,6%** |
| texto do usuário | 1.470.993 | 14,5% |
| texto do assistente | 774.052 | 7,6% |

**A injeção custa 6.650 bytes por turno (~1.662 tokens).** Decomposta por família:

| família | bytes | blocos | médio | repetido | desperdício |
|---|---:|---:|---:|---:|---:|
| `touring-suggest` | 1.655.956 | 1.183 | 1.399 | 24,1% | 398.715 |
| `outros` (post-tool, eco) | 640.799 | 1.838 | 348 | **65,5%** | 418.644 |
| `code-mode-deny` | 157.714 | 135 | 1.168 | 37,8% | 59.568 |
| `loop-outer` | 100.388 | **13** | **7.722** | 23,1% | 23.166 |
| `session-start` | 82.071 | 85 | 965 | 11,8% | 9.650 |
| `post-tool-eco` | 51.962 | 449 | 115 | 56,1% | 28.980 |
| `licoes-passadas` | 11.280 | 235 | 48 | **86,4%** | 9.744 |

**948.467 bytes (35,1% da injeção) são blocos byte-IDÊNTICOS a um já presente na
janela.** Um bloco idêntico ao anterior tem entropia condicional zero: `IDR = 0`.

Os campeões de densidade nula, contados: `[gabrielgadea]` **384×**,
`[generic]` **188×**, `post-tool-batch: batch of 1 tools (Edit) may have…`
**422×** (somando as variantes Edit/Write/2-tools).

### O maior injetor compra meia adesão

`touring-suggest` responde por **61% de toda a injeção**. Efeito medido:

```
MUST emitidos: 1.187   seguidos no Bash seguinte: 584   ratio: 0,492
```

E 0,492 é **teto superior**, não causalidade: um MUST "seguido" pode ter sido
seguido porque eu faria aquilo de qualquer forma. O piso honesto é desconhecido.

### O gate OUTER: 965 avaliações, 26% de conclusão

| medida | valor |
|---|---|
| avaliações do Stop hook (`compliance.jsonl`) | 965 |
| runs distintos | 38 |
| runs com manifesto completo na **1ª** avaliação | **2 (5,3%)** |
| runs que completaram **alguma vez** | 10 (26,3%) |
| runs que **nunca** completaram | **28 (73,7%)** |
| `explore-ledger` faltando | 418× |
| `diagnostic-okf` faltando | 269× |
| `strategy-doc` faltando | 252× |
| bundles com diagnóstico e **zero** ledger | **56 de 79** |

Markers reais encerrados no teto de continuações: `cont=30` (o cap), `30`, `26`,
`19`, `16`. Esses runs não convergiram — foram **liberados pelo runaway guard**.

**FACT [1.0]** — todos os números acima vêm de execução nesta sessão
(`~/.claude/loop-engineering/compliance.jsonl`, `docs/plans/*/`, transcripts).

---

## 2. Diagnóstico — quatro defeitos, um mecanismo

### D1 · O gate OUTER cobra do plano errado

O manifesto mistura duas espécies de artefato e cobra as duas com a mesma severidade:

| espécie | exemplo | quem produz | falta |
|---|---|---|---:|
| derivado por **código** | `diagnostic-okf` ← `loop_diagnose.py` | executor | 269× |
| escrito pelo **modelo** | `explore-ledger` (lente externa é MANUAL) | raciocínio | **418×** |
| escrito pelo **modelo** | `strategy-doc` | raciocínio | 252× |

Os que código produz saem, porque não custam raciocínio. Os que exigem parar o
raciocínio no meio não saem — **74% dos runs nunca os produziram**. É a violação
literal da ontologia do Eixo 3: o harness deve pôr no **plano de execução** tudo
que exige integridade determinística, e o OUTER pôs no **plano de raciocínio**.

Um gate contornado em 74% dos casos por esgotamento de contador não é um gate.
É pedágio: cobra o custo sem entregar a garantia.

### D2 · Três escalas rivais para "o orçamento de contexto do nível CILA N"

| nível | `cila_budget_read`<br>`touring-hooks-shared/src/cila.rs:33` | `compute_context_budget`<br>`touring-cortex/src/enrichment.rs` |
|---|---:|---:|
| L0–L1 | 800 | 800 |
| L2 | **2000** | **1200** |
| L3 | 2000 | 2000 |
| L4–L5 | **4000** | **3200** |
| L6+ | **4000** | **4800** |

Duas funções, um conceito, números divergentes em 3 dos 5 níveis. É a mesma
família de defeito de `cognitive_score` (04/09): duas fontes onde deve haver uma.

### D3 · O maior emissor não está sob nenhum dial

`crates/touring-cli/src/cli_suggester.rs` — 6.032 linhas, **61% de toda a injeção**:

```
referências a cila_budget_*  : 0
referências a CilaLevel/cila : 0
constantes próprias          : LESSON_BUDGET=800, BUDGET=4000, G1_BODY_BUDGET=3000
```

O teto por chamada existe, é testado, e **não governa o emissor que domina o
consumo**. Por isso apertar o CILA nunca moveu o número: ele não alcança 61% dele.

### D4 · O invariante de densidade é declarado e não é aplicado

`~/.claude/rules/touring-4-pillars.md` exige que **toda** injeção seja *dense,
specific, clear, complete, grounded*. Não existe executor verificando. Resultado
medido: `[gabrielgadea]` 384×, `[generic]` 188×, "batch of 1 tools" 422×,
`licoes-passadas` 86,4% repetidas. **Anti-padrão D8 em estado puro** — o mesmo
que a própria rule nomeia: *"enforcement mora no executor, não no anúncio"*.

### O mecanismo comum

Os quatro são a mesma coisa: **uma regra declarada num lugar e um executor que
decide por conta própria noutro**. D1 declara "veredito por artefato" e deixa o
artefato a cargo do raciocínio. D2 declara um orçamento em duas escalas. D3
declara um dial que o maior emissor ignora. D4 declara densidade sem juiz.

---

## 3. A tese do Gabriel, formalizada — e a emenda que a preserva

> *"Talvez o raciocínio e 'gate' possa ser como é esta skill que estamos
> trabalhando, na qual a etapa de interação é o próprio gate."*

Está certo, e o motivo é preciso: o que torna `decision-canvas` um gate barato
não é a interação — é que **a saída do gate É o entregável**. As 9 seções são o
que o Gabriel ia ler de qualquer forma; o custo marginal do gate é ~0.

O OUTER erra exatamente aí: exige um artefato que **não coincide** com o
entregável. O ledger não é o que foi pedido; é imposto de passagem.

**Princípio de projeto que emerge** (candidato a regra constitucional):

> **Um gate só é barato quando seu artefato é subproduto do trabalho, nunca um
> desvio dele.** Se satisfazer o gate exige interromper o raciocínio, o gate está
> cobrando do plano de raciocínio o que deveria cobrar do plano de execução.

Isso NÃO revoga a Lei L3 (veredito por artefato, jamais narrativa). Trocar
artefato por "eu digo que raciocinei" seria voltar à persuasão que já foi medida
e falhou (`protocol-adherence-diagnosis`). O artefato continua obrigatório —
mudam o **autor** e o **momento**:

| classe | custo de contexto | quando é o gate certo |
|---|---|---|
| **A · derivado por código** — `loop_diagnose.py`, exit code de `loop_converged.py`, trace do hook | **zero** | padrão, sempre que a exigência for **evidência** |
| **B · subproduto da resposta** — canvas, tabela de veredito que já vai na resposta | **zero marginal** | quando a exigência for **decisão** (a tese do Gabriel) |
| **C · escrito à mão** — ledger, estratégia | **alto** (desvio) | só quando o humano **pediu o documento** |

O OUTER hoje usa **C por padrão**. Deve usar **A por padrão**, **B** para
decisões, e **C** apenas sob pedido explícito.

---

## 4. 9-Dimension Scores

| dim | atual | alvo | delta | amplificação aplicada |
|---|---:|---:|---:|---|
| **a** Precisão | 9 | 9 | 0 | todo arquivo citado foi lido nesta sessão; `cila.rs` foi RELOCALIZADO após a citação de memória falhar (`touring-cli/src/cila.rs` não existe — é `touring-hooks-shared/src/cila.rs:33`) |
| **b** Escalabilidade | 8 | 8 | 0 | o piso de densidade (S-2.1) é um predicado reusável por qualquer emissor futuro, não um remendo por bloco |
| **c** Performance | 8 | 8 | 0 | alvos numéricos declarados por frente (6.650 B/turno → 3.000; 0,351 → 0,05; 5,3% → 80%) |
| **d** Funcionalidade | 9 | 9 | 0 | F3 conecta `cli_suggester` (6.032 L, 61% da injeção) ao dial CILA de que hoje está desligado — órfão funcional |
| **e** Qualidade | 8 | 8 | 0 | cada subtarefa nomeia o teste; F0 é pré-requisito estrutural das demais |
| **f** Detalhe | 8 | 8 | 0 | tabelas de baseline com o número medido, não faixa |
| **g** Integração | 8 | 8 | 0 | as 3 escalas rivais de orçamento reconciliadas por guard cruzado (S-3.1) |
| **h** Dependências | 7 | 7 | 0 | nenhuma dependência nova; tudo em crates existentes + scripts da loop-engineering |
| **i** Potencialização | 9 | 9 | 0 | matriz §7; F0 dá ao RL um sinal de custo que hoje não existe |

---

## 5. Phases

### Phase 0 — A régua (bloqueia todas as demais)

Otimizar antes de medir é o anti-padrão que este plano diagnostica em D4.

#### S-0.1: Família `context_budget` em `touring kpi -j` [P0] [confidence: FACT]
- **File**: `crates/touring-cli/src/cli/kpi.rs`
- **Change**: 5 métricas — `injected_bytes_per_turn`, `duplicate_injection_ratio`,
  `injection_follow_ratio` (por família), `tpcd`, `idr_by_family`.
- **Source truth**: hoje o KPI expõe `code_mode_adherence` e `code_mode_reuse`;
  nada mede a janela (verificado por leitura).
- **Test**: `test_context_budget_declares_five_metrics_each_with_a_resolver` — guard cruzando contrato×resolvedor nas duas direções, como o de
  `kpi-fonte-declarada-sem-resolvedor` — métrica declarada sem braço reprova.
- **Enables**: F1–F6 viram antes/depois medido.
- **Blast**: `crates/touring-cli/src/cli/kpi.rs` → **3 consumidores** (`touring ast blast`, medido 04/09/2026)

#### S-0.2: Coletor sobre transcript + `hook_trace` [P0] [confidence: FACT]
- **File**: novo, `crates/touring-hook-runtime/src/hook_trace.rs` já grava 1 linha
  JSON por invocação (`TOURING_HOOK_TRACE_FILE`, verificado: `append_line:153`).
- **Change**: registrar bytes emitidos e família por invocação; o coletor agrega.
- **Blast**: `hook_trace.rs` tem 265 L e 5 testes próprios — superfície contida.
- **Test**: `test_hook_trace_line_carries_emitted_bytes_and_family` — baseline desta sessão reproduzido pelo coletor (6.650 / 0,351 / 0,492).
- **Enables**: sinal de custo para o RL, que hoje aprende sem saber o que gastou.

### Phase 1 — Deduplicação por conteúdo

#### S-1.1: Conjunto de hashes de blocos já injetados na sessão [P0] [confidence: FACT]
- **Change**: bloco byte-idêntico a um já emitido vira `↑ mesmo aviso, N turnos atrás`.
- **Source truth**: 948.467 B (35,1% da injeção) são blocos byte-idênticos.
- **Por que o cache atual não resolve**: a chave do cache de 300 s é a assinatura
  da ação (`sig=outcome:bash:cat:plain`), não o conteúdo — sobrevivem 24,1% de
  repetição no suggester e **86,4%** em `licoes-passadas`.
- **Test**: `test_a_byte_identical_block_is_emitted_once_per_session` — `duplicate_injection_ratio` ≤ 0,05 em 10 transcripts novos.
- **Enables**: libera orçamento para enriquecimento hoje cortado pelo teto.

### Phase 2 — Piso de densidade no emissor

#### S-2.1: Predicado de proposições acionáveis [P0] [confidence: FACT]
- **Change**: um bloco declara suas proposições (comando derivado, símbolo real,
  número medido). Zero proposições → não emitido.
- **Source truth**: `[gabrielgadea]` 384×, `[generic]` 188×,
  `post-tool-batch: batch of 1 tools` 422×.
- **Test**: `test_a_block_with_zero_actionable_propositions_is_not_emitted` — nenhum bloco com 0 proposições no trace; `idr_by_family` ≥ piso.
- **Enables**: fecha D8 na rule `touring-4-pillars`, que exige densidade e não tem juiz.

#### S-2.2: Expurgo do eco de falha superada [P1] [confidence: FACT]
- **Change**: `last fail:` / `lições de erros passados` não são reinjetados depois
  de o erro ter sido corrigido no mesmo turno.
- **Source truth**: `licoes-passadas` é 86,4% repetição; Eixo 3 nomeia o padrão
  (*expurgo seletivo de rastros superados*, contenção de cascata de erros).
- **Test**: `test_a_resolved_failure_is_not_re_echoed_in_the_same_turn`
- **Enables**: a contencao de cascata de erro (Eixo 3) vira mecanismo, nao conselho — reusavel por qualquer emissor.

### Phase 3 — Um orçamento, por turno, alcançando todo emissor

#### S-3.1: Uma fonte para o orçamento CILA [P0] [confidence: FACT]
- **File**: `crates/touring-cortex/src/enrichment.rs` → delega a
  `crates/touring-hooks-shared/src/cila.rs:33`
- **Source truth**: divergem em 3 de 5 níveis (L2 1200≠2000, L4-5 3200≠4000, L6+ 4800≠4000).
- **Test**: `test_compute_context_budget_agrees_with_cila_budget_read_at_every_level` — guard estrutural cruzando as duas escalas — a mesma forma do guard D8.
- **Enables**: calibração de nível CILA passa a ser possível (hoje mover uma
  escala não move a outra).
- **Blast**: `crates/touring-cortex/src/enrichment.rs` → **10 consumidores** (`touring ast blast`, medido 04/09/2026)

#### S-3.2: `cli_suggester` sob o dial [P0] [confidence: FACT]
- **File**: `crates/touring-cli/src/cli_suggester.rs`
- **Source truth**: 0 referências a `cila_budget_*`, 0 a `CilaLevel`; constantes
  próprias `LESSON_BUDGET=800`, `BUDGET=4000`, `G1_BODY_BUDGET=3000`.
- **Correção durante a execução (04/09)**: a leitura desfez o agrupamento acima —
  só `LESSON_BUDGET` é orçamento de INJEÇÃO. `BUDGET` (rajada python-inline) e
  `G1_BODY_BUDGET` dimensionam o **programa que o remédio entrega**; encolhê-los
  cortaria a correção, não o ruído. O grep os juntou; a leitura os separou.
  Quem de fato alcança os 61% é S-3.3, o orçamento de turno.
- **Blast**: alto — 6.032 linhas, emissor de 61% da injeção. Exige `ast blast` e
  execução da suíte de hooks completa antes do merge.
- **Test**: `test_suggester_ceiling_responds_to_touring_cila_budget_env` — o teto do suggester responde a `TOURING_CILA_BUDGET_*`.
- **Enables**: o dial CILA passa a alcancar 61% do consumo; sem isso, calibrar nivel nao move o numero.

#### S-3.3: Acumulador por TURNO com prioridade declarada [P0] [confidence: FACT]
- **Change**: orçamento de turno; estourou, corta pela cauda da ordem
  `deny de gate` > `enriquecimento estrutural` > `nudge de pilar` > `eco de post-tool`.
- **Source truth**: o teto é por chamada (`cila.rs:11-27`); com 26,6% da janela em
  injeção, limitar a chamada não limita nada.
- **Test**: `test_turn_budget_drops_from_the_tail_of_the_declared_priority` — `injected_bytes_per_turn` ≤ 3.000 em 10 transcripts novos.
- **Risco**: pode cortar enriquecimento útil → a ordem sai do
  `injection_follow_ratio` por família medido em F0, nunca de intuição.
- **Blast**: `crates/touring-hooks-shared/src/cila.rs` → **13 consumidores** (`touring ast blast`, medido 04/09/2026)
- **Enables**: a prioridade declarada vira politica auditavel — quem foi cortado e por que fica no trace.

### Phase 4 — O OUTER vira FSM com saída derivada

#### S-4.1: Manifesto separa artefato-de-código de artefato-de-modelo [P0] [confidence: FACT]
- **File**: `~/.claude/skills/loop-engineering/scripts/hooks/flow_manifests.json`
- **Change**: cada artefato declara sua classe (A derivado / B subproduto / C manual).
- **Test**: `test_no_default_flow_requires_a_class_c_artifact` — nenhum flow default exige classe C.
- **Blast**: `hooks/flow_manifests.json` → 3 consumidores diretos (`loop_outer_gate.py`, `loop_stop_guard.py`, `loop_resume.py`); fora do indice do Touring, contado por leitura.
- **Enables**: a classe do artefato vira dado, entao qualquer flow futuro nasce declarando o que custa.

#### S-4.2: `work-outer` deve apenas o que código produz [P0] [confidence: FACT]
- **Source truth**: `work-outer` é 392 das 965 avaliações; `explore-ledger` falta
  418× e 56 de 79 bundles nunca o tiveram.
- **Change**: `explore-ledger` sai do caminho crítico do default e permanece em
  `strategy-outer`, onde o humano pediu a exploração.
- **Test**: `test_work_outer_manifest_is_satisfied_without_model_authored_files` — `runs_complete_on_first_eval` ≥ 80% em 20 runs novos; zero runs
  encerrados pelo runaway guard (hoje há markers em `cont=30`, o cap).
- **Enables**: o default deixa de gastar 74% das suas avaliacoes num artefato que nunca sai.

#### S-4.3: A classe B — a interação como gate [P1] [confidence: FACT 1.0 — CONSTRUÍDO 04/09]
- **Change**: para exigência de **decisão** (não de evidência), o gate aceita a
  resposta estruturada e o **executor materializa** o registro do turno.
- **Por que não é regressão da Lei L3**: o artefato continua obrigatório e
  continua sendo escrito por código; muda o autor, não a exigência.
- **Entregue**: `hooks/loop_turn_record.py` (novo) + `_materialize_class_b` em
  `loop_outer_gate.py` + artefato `turn-record` no `work-outer`, que passa a
  exigir `["A","B"]`. Roda no **Stop hook**, com o turno já concluído: não há
  raciocínio a interromper, porque o texto de que o registro é feito já foi
  escrito, para o humano, pelo motivo do humano.
- **Prova viva**: registro de **10.817 B** materializado nesta sessão — 21
  blocos, 12 linhas de espinha, **64 ações** (`Bash` 29× · `Edit` 28× · `Read`
  6× · `Write` 1×) — com **zero** bytes escritos à mão.
- **Fail-open é a razão de ser**: executor que falha sai do caminho crítico e
  aparece em `class_b_unmaterialized`; cobrar seria devolver a escrita manual
  que a classe elimina. Exercitado: `missing=[]`, `complete=True`.
- **Test**: `test_the_turn_record_exists_on_disk_with_no_mid_reasoning_write` +
  `test_class_b_never_becomes_a_manual_write_when_the_executor_fails` +
  `test_a_working_turn_is_not_an_empty_record` (130 testes verdes na suíte).
- **Enables**: a interacao estruturada vira gate reusavel — o padrao que `decision-canvas` ja prova em prosa passa a ter executor.

### Phase 5 — Tool results (51,3% da janela)

#### S-5.1: Medir antes de agir [P1] [confidence: INFERENCE 0,80]
- **Change**: quais ferramentas dominam o volume e qual fração da saída é lida.
- **Nota de honestidade**: o volume está medido (51,3%); a fração **lida**, não.
  Aplicar digest sem isso seria repetir D4 — afirmar efeito sem instrumento.
- **Enables**: digest por padrão nas dominantes, com `retrieval_hint` (contrato do
  spill que já existe).
- **MEDIDO 04/09/2026** (10 transcripts, 4.042 chamadas, 5,39 MB de tool result):

  | ferramenta | bytes | % | chamadas | média |
  |---|---:|---:|---:|---:|
  | **Bash** | 3.378.164 | **62,7%** | 2.243 | 1.506 |
  | **Read** | 1.507.386 | **28,0%** | 428 | **3.521** |
  | Edit | 122.148 | 2,3% | 574 | 212 |
  | Write | 44.363 | 0,8% | 212 | 209 |

  Bash + Read = **90,7%**; Edit/Write são ~210 B e não valem instrumentação.
- **Test**: `test_tool_output_volume_is_attributed_per_tool`

#### S-5.1b: A fração USADA — e a ação que ela refuta [P1] [confidence: FACT 1.0 — MEDIDO]

**Resultado (04/09/2026, 2ª rodada): o digest por TRUNCAGEM está refutado; o
digest DERIVADO DO CONTEÚDO, que já existe, está confirmado.**

"O modelo leu?" não é mensurável. A pergunta que decide a mesma coisa é
**quantos bytes um digest teria de carregar para preservar todo fato que o
modelo USOU** — as linhas e identificadores do resultado que reaparecem no que
o assistente escreveu ou nos argumentos das chamadas seguintes, até o próximo
turno humano. Piso do uso, portanto teto do que se pode descartar.

| grandeza | valor |
|---|---:|
| reuso global dos tool results | **0,067** |
| zero-reuso (Read / Bash) | **2,0% / 5,5%** |
| joelho da economia (digest de 2 KB) | **limiar 2 KB → 44,4%** |
| cabeça+cauda(40) cobre ≥90% do reuso | **45% dos resultados grandes** |

Decis da posição das linhas reusadas — 9,6% · 5,7% · 6,3% · 6,6% · 7,4% · 6,8%
· 6,0% · 6,3% · 6,8% · 5,9%: **uniformes**. Cortar por posição perde o que foi
usado, e não há população descartável (quase todo resultado contribui algo).
Logo a ação correta é a que o workspace já tem — agregado calculado pelo
programa (`--brief`) e spill com `retrieval_hint` — e não um segundo digest por
limiar, que criaria a segunda fonte que este plano inteiro diagnostica.

- **Test**: `o_prefixo_de_numeracao_do_read_nao_zera_o_reuso` (red→green por
  mutação: sem a normalização do `cat -n` a métrica inteira lê 0,0%),
  `um_fato_citado_depois_do_turno_humano_nao_conta`,
  `resultado_que_ninguem_tocou_conta_como_zero_reuso` + sonda viva.
- **Enables**: a maior fatia da janela (51,3%) entra sob a régua; e o resultado
  NEGATIVO fica registrado como resultado, nunca como fase pendente.

### Phase 6 — Estabilidade de prefixo (CUR)

#### S-6.1: Verificar cache hit real [P2] [confidence: FACT 1.0 — MEDIDO]

**Resultado (04/09/2026, 7.963 registros com `usage` em 10 transcripts): a
hipótese se confirma e esta fase NÃO tem trabalho.**

| grandeza | tokens |
|---|---:|
| `cache_read_input_tokens` | **2.795.125.945** |
| `cache_creation_input_tokens` | 27.896.373 |
| `input_tokens` (sem cache) | 54.186.679 |

**CUR = 0,971** contra a meta 0,80–0,90 do documento. O bloco do SessionStart é
constante dentro da sessão e a injeção é append-only, então o prefixo não é
quebrado. Nada a corrigir na ordem do payload — e saber disso vale a medição,
porque era o maior ganho unitário do plano se tivesse dado o contrário. Um
resultado NEGATIVO registrado como resultado, nunca como fase pendente.

- **Hipótese**: o bloco do SessionStart é constante dentro da sessão, logo não
  quebra o prefixo — mas isso é presunção.
- **Test**: `test_cache_read_tokens_dominate_cache_creation_within_a_session` — `cache_read_input_tokens` vs `cache_creation_input_tokens` em sessões
  reais. Se confirmado, nada a fazer; se não, é o maior ganho unitário do plano
  (leitura −90%, TTFT −85%).
- **Blast**: nenhum arquivo modificado — fase de medicao pura; se a hipotese cair, o alvo sera a ordem do payload no SessionStart.
- **Enables**: se confirmado, nada a fazer (e o saber disso vale a medicao); se refutado, e o maior ganho unitario do plano.

---

## 6. Verification Protocol

```bash
# Régua (Phase 0) — precisa existir antes das demais
touring kpi -j | jq '.context_budget'          # 5 métricas, baseline registrado

# Gates de entrega, por fase
cargo check --workspace --tests && cargo clippy --workspace --all-targets -- -D warnings
cargo test -p touring-hooks-shared -p touring-cortex -p touring-cli
cargo test -p touring-hook-handlers --features pre-hooks,post-hooks   # exige as features

# 6 dims P0 BLOCK em cada arquivo tocado
touring-quality check --gate F2.1 --target <FILE>   # + F2.4 F2.5 F2.6 F4.3 F4.5
touring-quality score <scope> --workspace --fail-below 0.80   # piso Gold

# O juiz — exit 0 e o unico "pronto"
python3 ~/.claude/skills/loop-engineering/scripts/loop_converged.py --task <id> --scope .
touring e2e -j                                  # baseline composite 0,8749
```

**Antes/depois obrigatório** por fase: repetir a medição desta sessão (10
transcripts, 406 turnos) e comparar contra o baseline **6.650 B/turno · 0,351
duplicação · 0,492 adesão · 5,3% conclusão na 1ª avaliação**.

---

## 7. Potentiation Matrix

| fase | habilita |
|---|---|
| P0 régua | torna P1–P6 mensuráveis; dá ao RL um sinal de custo que hoje não existe |
| P1 dedup | libera orçamento para enriquecimento hoje cortado pelo teto (~9% da janela) |
| P2 densidade | o piso IDR vira gate reusável por qualquer emissor futuro |
| P3 orçamento | uma fonte de verdade sobre CILA — fecha D2/D3 e desbloqueia a calibração de nível |
| P4 OUTER-FSM | o Stop hook volta a poder recusar, porque o que ele exige custa zero |
| P5 tool results | o maior bloco da janela entra sob a mesma régua |
| P6 CUR | se confirmado, −90% no custo de leitura do prefixo estável |

---

## 8. Sequenciamento

```
P0 (régua) ──┬── P1 (dedup, ganho imediato ~9% da janela)
             ├── P2 (densidade, ~700 KB)
             ├── P3 (orçamento único + acumulador)   ← depende de P0 para a prioridade
             ├── P4 (OUTER FSM)                       ← independente, maior ganho qualitativo
             └── P5 (tool results)                    ← depende de P0 para escolher alvos
P6 (CUR) ── verificação isolada, a qualquer momento
```

**P0 primeiro e sozinho.**
