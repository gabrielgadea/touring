# Infraestrutura de Telemetria — Efetividade do Acoplamento Touring ↔ Claude Code ↔ LLM

> **Data**: 2026-06-27 | **Versão**: v2 (aprofundada) | **Tipo**: design doc | **Autoridade**: Gabriel Gadea
> **Fundamenta-se em**: os 6 docs de research `2026-06-26-*`; a telemetria real (142 gate-metrics counters,
> sources VGP-verificados); o sistema `touring kpi` (commitments falsificáveis); o `action_world_model.json`
> (21.748 exemplos); OpenTelemetry GenAI semantic conventions + Views + Exemplars (context7).
> **Pergunta**: *o acoplamento está funcionando? como sabemos, e como aprendemos a refiná-lo?*
> **v2 acrescenta** (sobre a v1): modelo formal/econômico de `U(a)`, o **grafo causal** da dinâmica,
> 7 papers→7 meters, atribuição causal (ablation/variância), schema+governança, maturidade — e
> **sources verificados** + **thresholds calibrados ao baseline real medido**.
> **🔎 Revisão 2026-07-01**: doc conferido contra o código — F1–F7 + Task #6 refletem a realidade. **+F8
> SHIPPED** (actor queue-wait): a fila serial do actor por-projeto era **invisível** (`hook_dispatch_latency`
> só começa no dequeue) — percepção de contenção cross-sessão do Gabriel confirmada arquiteturalmente e
> instrumentada: histogram `actor_queue_wait` + counters `actor_{budget,send}_timeout_count` (§5-D5, §7, §14-F8).

---

## 0. Sumário executivo

O backend de acoplamento (C1–C14 + I1–I10) foi **construído e entregue** (C1 −99.999% em `wiring audit`;
P0 cumulativo −54.332 tok/sessão). O que falta é **maturidade de medição**: há **142 counters de
_atividade_** ("a feature disparou") e quase nenhum de **_efetividade_** ("a feature mudou o comportamento
da LLM"). A **métrica-mãe** declarada nos docs — a **adesão** — **não tem um único counter**; e o sinal
decisivo (a LLM **seguiu** a sugestão?) tem **zero tracking**.

Esta proposta não inventa: **estende** o `touring kpi` + `commitments.yaml` (já com snapshots datados =
série temporal) com uma família `touring.coupling.*`; ancora-a num **modelo econômico-causal explícito**
(`U(a)=P(sucesso)·V−C(tokens)` → adesão → outcome → aprendizado); instrumenta os ~8 gaps (liderados por
**suggestion-uptake**); e fecha um **loop de refinamento** atribuível por contrafactual. O objetivo é
**compreender e aprender** a dinâmica — não só observá-la.

**Achado de fundo (v2)**: a métrica mais "teórica" — a utilidade `U(a)` — é **computável hoje**, porque
`P(sucesso|tool_class,intent,context)` já vive no `action_world_model.json` (21.748 exemplos, sobrevive a
restart) e `C(tokens)` vem do gate-metrics. O substrato existe; falta **derivar, persistir e fechar o laço**.

---

## 1. Diagnóstico (ground truth)

### 1.1 O que JÁ existe (reusar, não duplicar) — VGP-verificado

| Camada | O que é | Estado |
|---|---|---|
| **gate-metrics** (142 counters) | `crates/touring-foundation/src/gate_metrics.rs`; `touring gate-metrics -j` | ✅ rico, in-memory (zera no restart) |
| **`touring kpi`** | `kpi-commitments-v1`; `source: daemon:<handler>:<json_pointer>`; resolver em `crates/touring-cli/src/cli/kpi.rs:231`. Handlers reais: `cli-wiring-status`, `cli-learning-status`, `cli-gate-metrics`, `cli-gotcha-stats`. `external:` = **STUB** | ✅ **a fundação** — 9 commitments, **0 de coupling** |
| **STR / TR-2** | `enrichment_{context_bytes_total,emit_count,mean_bytes_per_emit}` | ✅ existe (medido: 1029 B/emit) |
| **health-delta** | `health_delta_{regression,improvement,recovery,streak}_count` por path | ✅ **efetividade real** (in-memory) |
| **RL / outcome** | `ema_reward`, `mean_td_error`; `outcome_learner_brier_running_sum` | ✅ efetividade real |
| **`action_world_model.json`** | `(tool_class,intent,context)→{successes,failures}`, **21.748 exemplos, 95 features, warm-load sobrevive restart**; persiste a cada 16 outcomes | ✅ **substrato de P(sucesso\|a)** — único durável + rico |
| **activity.jsonl** | `{id,seq,action,actor,timestamp_ns,payload,projection_hash}`; `action ∈ {hook_fired, tool_invoked, …}`; `touring activity replay/verify` | ✅ **disk-backed, ordenado por seq** — substrato do uptake |
| **τ-bench** (`docs/agentic-bench/`) | composite = 0.40·sel + 0.40·out + 0.20·conf → 1.0 Diamond | ✅ live (base do A/B) |
| **`touring kpi --snapshot`** | escreve `docs/kpi/YYYY-MM/YYYY-MM-DD.json` on-demand | ✅ writer existe — **scheduler ⬜ não** |
| **`touring repo-score`** | dashboard 11-categorias 0–269, grade A–F (Wave R1) | ✅ surface a integrar |

### 1.2 O que NÃO é medido (os gaps)

| # | Gap | Por que importa | Hoje |
|---|-----|-----------------|------|
| **G1** | **Adesão** `(touring+code_mode)/(grep+cat+find)` | métrica-mãe; mede a inversão prior-bash→prior-touring | sem counter |
| **G2** | **Suggestion-uptake** — a LLM seguiu o hint? | converte atividade→efetividade; valida a tese | **zero correlação** (eventos existem em activity.jsonl) |
| **G3** | **Ativação por C-item** — C1 brief %, C2 curated-vs-all, C13 skip-rate | engines prontos mas subutilizados | sem counter |
| **G4** | **Code-Mode adoption** — `ctx_execute` vs N atômicas | I2/I5: −60-85% tok prometidos | parcial (`ctx_*_count`) |
| **G5** | **Cache churn cross-session** — pausa >5min → ~223K tok | 86% cache-read intra-sessão é bom; o custo das pausas é invisível | sem observabilidade |
| **G6** | **Curva de aprendizado cross-session** — EMA/regret/world-model ao longo do tempo | RL e world-model só têm o ponto atual; sem trend não há "aprender a dinâmica" | sem time-series (snapshot é point-in-time) |
| **G7** | **C9 silent-failure** + **C13 skip-rate** + **C14 GED merges** | gates construídos sem counter dedicado | sem counter |
| **G8** | **Correlação enrichment→outcome** — injetar contexto reduziu erro? | STR mede eficiência, não correção | não correlacionado |

### 1.3 A distinção que organiza tudo: **Atividade vs Efetividade**

> Counter de **atividade**: *"isto aconteceu N vezes"* (`blast_inject_count`).
> Counter de **efetividade**: *"isto fez a LLM ir melhor / gastar menos / acertar a tool"*.

Dos 142 counters, **~6 são efetividade real**. A proposta inteira é uma máquina para **converter atividade
em efetividade** — e o conversor universal é **a correlação entre o que o coupling _ofereceu_ e o que a LLM
_fez_ em seguida** (uptake, delta de qualidade, world-model success-rate).

### 1.4 Baseline real medido (calibra os thresholds — "meça antes de otimizar")

| Sinal | Valor medido | Leitura honesta |
|---|---|---|
| `enrichment_mean_bytes_per_emit` (STR) | **1029 B/emit** | a v1 propôs `lte 400` — **irreal**; o baseline é 1029. Recalibrado: `lte 800` (força melhora sem ser fantasia). |
| `hook_dispatch_latency` | **p50=249µs, p90=38ms, p99=429–561ms** | p50 valida "~ms no caso comum"; **a cauda é pesada** (cold-start/cargo). O KPI deve separar `p50<1ms` de `p99` (vigiar a cauda, não assumir 1ms). |
| `ema_reward` | **0.18 → 0.32** entre duas medições (n=5) | in-memory + amostra minúscula → **instável**. O KPI exige baseline **acumulado** (world-model/datado), não snapshot instantâneo. |
| `query_cache_hit_ratio` | 0.0 → 0.71 (pós-warm) | zera no restart; confirma o gap de persistência. |
| `action_world_model` | 21.748 exemplos, 95 features | **rico e durável** — base sólida para P(sucesso\|a). |

**Conclusão do baseline**: thresholds só viram `direction` falsificável **após 2 semanas de coleta**; antes
disso, são `advisory`. E a instabilidade dos counters in-memory é a justificativa nº1 para a camada L3.

---

## 2. Modelo formal & econômico (a base teórica)

### 2.1 A economia que governa a escolha — `U(a)`
A LLM não obedece sermão; ela **maximiza utilidade gulosamente** sob racionalidade limitada (coupling-strategy §4):

```
U(a) = P(sucesso | a) · V(sucesso) − C(t · tokens)
```

| Ação | P(sucesso) | C(tokens) | U(a) hoje |
|---|---|---|---|
| `grep -r "Foo"` | alto | baixo | **alto** ✅ |
| `touring wiring orphans -j` | alto | **173K** (pré-C1) | **negativo** ❌ |
| 1 MCP `touring_*` (de 160) | médio | 38K schema + paralisia | baixo ❌ |
| script Code-Mode (`touring`+filtra) | alto | **baixo** (só sumário) | **alto** 🎯 |

**Por que isto é a base da telemetria**: cada KPI mapeia a um **termo de `U(a)`** — STR/brief/curadoria ↓`C`;
summarizer/route/index-find ↑`P`; adesão é o **resultado observável** de `U` ter virado positivo. E — achado
v2 — `U(a)` é **computável agora**: `P(sucesso|tool_class,intent,context)` = `successes/(successes+failures)`
no `action_world_model.json`; `C(tokens)` = bytes do gate-metrics. Logo `touring.coupling.utility{class}` é
um KPI derivável **sem instrumentação nova** (só um derivador).

### 2.2 A métrica-mãe — adesão (3 eixos)
```
adesão = (touring_cmds + code_mode_scripts) ÷ (grep + cat + find atômicos)   [por sessão]
```
Decompõe em 3 eixos mensuráveis (coupling-strategy §7): **(a) base estática** (cache_creation 223K→110K),
**(b) dinâmico/sessão** (hooks 50K→15K), **(c) banner-blindness** (chars de `additionalContext` **agidos vs
ignorados** — é exatamente o **suggestion-uptake**). A adesão é o **lagging** central; o uptake é o seu
**leading** mais direto.

### 2.3 STR como ganho de informação
O `enrichment_mean_bytes_per_emit` é o denominador de um **information-gain**: o valor real é
`Δ(decisão correta) / bytes injetados` — *bits acionáveis por token*. Um emit que muda a próxima ação para a
correta tem STR alto; um ignorado tem STR **zero** (e vira `wasted_enrichment`). Isto liga D2 (eficiência) a
D3 (uptake): **STR só conta o sinal que foi adotado**.

### 2.4 O harness de validação — τ-bench
```
composite = 0.40·selection + 0.40·outcome + 0.20·conformance
  selection   = (precision@1 + MRR)/2          # intent→tool certo
  outcome     = correct_verdicts/total          # a tool retorna verdict certo (anti Class-D)
  conformance = (ann_pct + desc_pct + curated_ok)/3   # tools anotadas, documentadas, curadas 18–26
```
É o **gate intervencional** (rodado sob controle) que complementa a telemetria **observacional** (produção).
Medido 1.0 Diamond. Papel na telemetria: **§10 (atribuição)** usa o τ-bench como o experimento controlado.

---

## 3. O grafo causal — *compreender a dinâmica*

A telemetria só "ensina" se houver um **modelo causal** contra o qual ler os números. Reconstruído verbatim
da coupling-strategy §0-5:

**O laço-problema** (o que o coupling combate):
```
indução semântica cara (110K rules + 160 MCP) → prior-zero p/ touring → escolha gulosa: grep (U alto)
  → LLM evita infra-aware → reusa bash/grep ineficiente → baixa acurácia + desperdício de token
    → baixa adesão + caos de exit-code  ↺
```

**A cadeia-solução** (os 4 caminhos causais, cada um um termo de `U`):
```
densify CLI (status 41K→2K)        → ↓C → ↑U(touring) → +exploração → ↑adesão
redirect grep→index find (suggest) → ↓C +↑P → ↑U → ↑adesão            (+WarpGrep: −17% tok, +3.7pp)
MCP curated 22 (de 160)            → ↓C schema −86% → fim da paralisia → ↑adesão
Active Summarizer (C5, preserva erro) → ↑P(próxima tentativa) → bloqueia Class-D → loop mais curto → ↑adesão
```

**O DAG de mediação** (onde cada KPI se posiciona):
```
  [feature coupling]
        │
   ┌────┴────┐
   ▼         ▼
 C(tokens)↓  P(sucesso)↑        ← mediadores (D2 STR, D5 latency / D3 selection, D4 world-model)
   └────┬────┘
        ▼
      U(a)↑                      ← derivável (world-model × gate-metrics)
        ▼
     adesão↑                     ← métrica-mãe (D1) [LAGGING]
        ▼
  outcome (health-delta, τ-bench) ← D4 [LAGGING]
        ▼
   reward / world-model update    ← D6 aprendizado
        ▲                         │
        └───────── L5 loop ───────┘   (RL flywheel: TD(λ)+ACO+CUSUM drift)
```

**Leading vs Lagging** (a chave para *prever* e *course-correct*, não só constatar):

| Leading (preditivo, age mid-action) | Lagging (resultado, mede pós-ação) |
|---|---|
| `U(a)` por classe (suprime sugestão de U baixo) | **adesão** (touring/atômicas) |
| **suggestion-uptake / redirect-rate** | composite τ-bench |
| STR (bytes/emit) + schema bytes transmitidos | health-delta net + session tokens |
| `error_lines` preservadas (risco Class-D) | gotcha hit-rate (aprendizado) |
| conformal-τ confidence da sugestão | exit-code vs sucesso-narrado (Class-D escapadas) |

> **A hipótese falsificável central**: *se os leading sobem (uptake↑, STR↓, U↑) e os lagging NÃO seguem
> (adesão estagnada, health-delta plano), o modelo causal está errado* — e isso é informação, não ruído.
> É assim que a telemetria nos ensina a refinar a estratégia.

---

## 4. Princípios de design (e por quê)

| Princípio | Fundamentação |
|---|---|
| **P1 — Adesão é a estrela polar** | Tudo orbita `U(a)`; o coupling só funciona se ↑adesão E adesão→melhor outcome com menos token. |
| **P2 — Medir efetividade, não atividade** | §1.3 — cada dimensão fecha o laço _oferta→comportamento_. |
| **P3 — Estender `touring kpi`** | herda CI-gate, série temporal datada e o padrão `source: daemon:<handler>:<pointer>` (4 handlers reais). |
| **P4 — Disciplina de instrumento OTel** | Counter (delta monotônico) · UpDownCounter (não-monotônico) · **Histogram** (P50/P99 + **exemplars**) · **Async Gauge** (absoluto). Nomes: `usage`/`utilization`(0–1)/`time`. **Views** controlam cardinalidade. |
| **P5 — Falsificável + acionável** | todo KPI tem `direction`+`threshold`+`rationale`; calibrado a baseline (§1.4) antes de armar. |
| **P6 — Atribuição por contrafactual** | toggles `TOURING_SUGGESTER_DISABLED`/`TOURING_MCP_ALL_TOOLS` → A/B; separa "modelo melhorou" de "coupling funcionou" (§10). |
| **P7 — Custo de medir < valor medido** | coleta in-process (~ns); snapshot 1×/dia; surface `--brief`; o KPI `str_bytes_per_emit` vigia a própria telemetria. |
| **P8 — Alinhar à OTel GenAI semconv** | (context7) nomear spans/atributos no padrão: `gen_ai.operation.name=execute_tool`, `gen_ai.tool.name`, `gen_ai.client.token.usage` (Histogram), `mcp.session.id`. Interoperabilidade com qualquer backend OTLP — não um dialeto privado. |

---

## 5. Modelo de efetividade — 6 dimensões (D1–D6)

Cada dimensão: pergunta · sinais (✅ existe / ⬜ instrumentar) · instrumento OTel · [L]eading/[G]lagging.

**D1 — ADESÃO** *("usa Touring em vez de bash cru?")* — a inversão do prior. [Lagging, mas o motor de tudo]
- ✅ `coupling.adoption_ratio` = `touring/(touring+grep+cat+find+sed)` entre ações Bash — **F3 SHIPPED** (2 counters→derived, online in-daemon). **G1**
- ⬜ `coupling.antipattern_per_session` (baseline forense: BashGrepRaw 35.975, ReadWithoutLocate 46.307) — Counter
- ⬜ `coupling.feature_activation{c_item}` (**G3**) — Counter/feature
- ✅ `antipattern_converted_count` (proxy bloqueado→corrigido)

**D2 — EFICIÊNCIA DE TOKEN (STR)** *("reduz custo preservando sinal?")* [Leading]
- ✅ `enrichment_mean_bytes_per_emit` → Histogram + exemplar do pior emit
- ✅ `query_cache_hit_ratio` (já é commitment)
- ⬜ `coupling.compression_ratio{brief,summarizer,codemode}` = `in/out` — Histogram
- ⬜ `coupling.cache_churn_tokens` (**G5**) — Counter

**D3 — ACERTO DE SELEÇÃO** *("acha/escolhe a tool certa?")* [Leading]
- ✅ **`coupling.suggestion_uptake`** = `followed/emitted` (**G2 — decisivo**, §9) — **F2 SHIPPED** (Counter par→derived, online)
- ✅ τ-bench `precision@1`/`MRR` (C3) — exportar p/ KPI
- ⬜ `coupling.route_accuracy` (RGAO/CILA) — Counter {hit,miss}

**D4 — QUALIDADE DE RESULTADO** *("as ações dão certo?")* [Lagging]
- ✅ `health_delta_{regression,improvement,recovery}_count` — efetividade real
- ✅ `ema_reward` + `outcome_learner_brier` — Async Gauge; ✅ `action_world_model` success-rate por classe
- ✅ `ceg_blocked_count` (execuções perigosas barradas)
- ⬜ `coupling.silent_failure_caught` (C9 — **G7**) — Counter

**D5 — CUSTO / OVERHEAD** *("é barato o bastante?")* [Leading]
- ✅ `hook_dispatch_latency` **p50 E p99** (Histogram) — separar caso-comum da cauda (§1.4)
- ✅ `actor_queue_wait` (**F8**, 2026-07-01) — Histogram enqueue→dequeue do `RunHook` no actor **serial
  por projeto**; separa fila (contenção cross-sessão no mesmo project_root) de execução do handler —
  `hook_dispatch_latency` só começa no dequeue, então esta espera era invisível. Clamp em 60s
  (`max_us == 60_000_000` ⇒ saturado).
- ✅ `actor_budget_timeout_count` + `actor_send_timeout_count` (**F8**) — Counter; desistências por budget
  (15s light / 300s heavy) e drops por queue-full (send > `REQUEST_TIMEOUT`).
- ⬜ `coupling.wasted_enrichment_pct` = bytes em emits **ignorados** (D3×D2) — Counter

**D6 — APRENDIZADO** *("melhora ao longo do tempo?")* [Lagging, cross-session]
- ✅ `ema_reward` trend + `mean_td_error` decay (só in-memory — **G6**)
- ✅ `health_delta_streak_alert_count` (drift)
- ⬜ `coupling.world_model_success_rate` datado (curva de P(sucesso) ao longo de semanas — **G6**) — Async Gauge
- ⬜ `coupling.gotcha_rematch` (gotcha evitou repetir erro — **G7**) — Counter

---

## 6. Arquitetura em camadas (L0–L5)

```
L5  REFINAMENTO   KPI → RL reward-shaping · threshold auto-tune · tool curation · drift alert · A/B verdict
L4  COMMITMENTS   touring.coupling.* em commitments.yaml → `touring kpi --check` (CI gate falsificável)
L3  PERSISTÊNCIA  scheduler (⬜) → `touring kpi --snapshot` (✅ writer) → docs/kpi/YYYY-MM/*.json + activity.jsonl
L2  INDICADORES   D1–D6: KPIs derivados (ratios, P99, U(a), success-rate) — não 142 counters crus
L1  AGREGAÇÃO     gate-metrics (Counter/UpDownCounter/Histogram/Gauge — OTel) + Views (cardinalidade)
L0  COLETA        hooks · CEG X0-X9 · cli-suggester · RL sink · world-model · +SUGGESTION-UPTAKE
```

**Justificativa por camada**: **L0/L1 já existem** (não tocar; só +8 gaps). **L2 é o valor** — 142 counters
não respondem "funciona?"; 6 indicadores derivados sim (alto STR aplicado à própria telemetria). **L3 é o gap
real** — o *writer* existe (`--snapshot`), falta o *scheduler*; sem ele os counters in-memory zeram (§1.4) e
não há trend. **L4** torna falsificável. **L5** fecha o laço (§12).

---

## 7. Catálogo de KPIs `touring.coupling.*` — sources VGP-verificados + thresholds calibrados

`source` real entre crases; `⬜` = instrumentar. Thresholds calibrados ao baseline §1.4 (advisory por 2 semanas).

| id | Dim · L/G | source (verificado) ou ⬜ | OTel | dir · threshold | rationale |
|---|---|---|---|---|---|
| `touring.coupling.adoption_ratio` | D1·G | ✅ **F3 online in-daemon** (`classify_adoption`) | 2 counters→ratio derived | gte · 0.50 | métrica-mãe; <0.5 = prior-bash vence; runtime 0.75 |
| `touring.coupling.suggestion_uptake` | D3·L | ⬜ **G2** (activity.jsonl seq) | Counter→ratio | gte · 0.40 | **o KPI decisivo**: hint adotado |
| `touring.coupling.str_bytes_per_emit` | D2·L | ✅ `daemon:cli-gate-metrics:/enrichment_mean_bytes_per_emit` (=1029) | Histogram | lte · **800** | recalibrado ao baseline 1029 |
| `touring.coupling.utility{class}` | D2/D4·L | ✅ derivado: world-model × gate-metrics | Async Gauge | gte · 0 | `U(a)` por classe — computável já |
| `touring.coupling.health_delta_net` | D4·G | ✅ `daemon:cli-gate-metrics:/health_delta_{improvement,regression}_count` | Counter | gte · 0 | melhora > piora |
| `touring.coupling.ema_reward` | D4/D6·G | ✅ `daemon:cli-learning-status:/ema_reward` (=0.18–0.32, instável) | Async Gauge | gte · 0.20 | RL convergindo (baseline acumulado) |
| `touring.coupling.world_model_success` | D6·G | ✅ `action_world_model.json` Σsucc/Σtotal | Async Gauge | gte · 0.70 | P(sucesso) médio cross-session |
| `touring.coupling.hook_latency_p50_us` | D5·L | ✅ `daemon:cli-gate-metrics:/hook_dispatch_latency/p50_us` (=249) | Histogram | lte · 1000 | caso-comum barato (~ms) |
| `touring.coupling.hook_latency_p99_us` | D5·L | ✅ `…/hook_dispatch_latency/p99_us` (=429311) | Histogram | lte · **50000** | vigia a cauda cold-start (não 1ms) |
| `touring.coupling.wasted_enrichment_pct` | D5·L | ⬜ (uptake×bytes) | Counter | lte · 0.30 | os "57% de ruído" do cli-suggest |
| `touring.coupling.silent_failure_caught` | D4·G | ⬜ **G7** (C9) | Counter | gte · 0 | C9 pega falhas mascaradas |
| `touring.coupling.code_mode_adoption` | D4·L | ⬜ **G4** (`ctx_*_count`/loops) | Counter→ratio | gte · 0.25 | I2/I5: −60-85% tok |
| `touring.coupling.cache_churn_tokens` | D2·G | ⬜ **G5** | Counter | lte · 500k/dia | custo invisível das pausas |
| `touring.coupling.actor_queue_wait_p99_us` | D5·L | ✅ **F8** `daemon:cli-gate-metrics:/actor_queue_wait/p99_us` | Histogram | lte · 1000000 | fila do actor serial por projeto; >1s = contenção cross-sessão (baseline 2 sem antes de armar) |

> Os 9 commitments existentes permanecem (saúde do **backend**); os `coupling.*` medem saúde do **acoplamento**.

---

## 8. Os 7 fenômenos externos → 7 meters Touring (a ponte de validação)

Cada paper que fundamenta o backend vira um **indicador concreto** (não citação) — harness-insights §8:

| Fenômeno (paper) | Achado | Meter Touring |
|---|---|---|
| **Context-Rot** (Chroma, 18 LLMs) | acurácia cai com input grande | `input_tokens / composite_score` por sessão; alerta se >2.0 (peso sem ganho) |
| **Codex pathology** (*Is Grep All You Need?*) | retrieval por file-ref: 93%→55% | % de outcomes que exigiram **re-read/clarificação** (file-ref sem sumário inline) → alvo 0 |
| **WarpGrep** (Anthropic) | +3.7pp, −17% tok, −28% tempo | `index_find/(index_find+grep)` por sessão; medir Δ-outcome quando >0.6 vs control |
| **BiasBusters** (2510.00307) | seleção manipulável 20%→81% | `run_bench.py::precision@1` direto; top-3 contêm a tool certa? |
| **harness > model** (SWE-bench Pro) | +22pp só pelo harness | mesma task/codebase: `composite_before vs after`; hipótese +0.15 |
| **CodeAct / Code Mode** (MS+Anthropic) | −60% tok, −50% latência | `code_mode/(grep+cat+find)` = numerador da adesão; efeito quando >0.5 |
| **Tool Search** (Anthropic 2603.20300) | −85% tok via descoberta | `bytes(all_schemas)/bytes(discovered)`; 160→22 ≈ 6.7× (84%) |

**Por que importa**: estes meters transformam afirmações de papers em **hipóteses testáveis no Touring** — e
amarram a telemetria interna à literatura, dando ao A/B (§10) alvos quantitativos esperados.

---

## 9. O gap nº 1 — instrumentar **suggestion-uptake** (G2)

**Por que é o coração**: hoje o hook injeta `[TOURING SUGGEST] MUST touring index find` e **nunca sabe** se a
LLM seguiu ou ignorou e fez `grep`. Sem isso, *nenhum* sinal de hook é efetividade — todos são atividade.
Fecha **toda** a D3 e valida a tese (banner-blindness, eixo (c) da adesão).

> **⚠ CORREÇÃO 2026-06-28 (VP-Scout Cadeia 5)**: a premissa abaixo era **falsa contra os dados**. O
> `activity.jsonl` real (10029 eventos) contém **só `hook_fired`** (lifecycle: pre_compact, etc.), **zero
> `tool_invoked`**, e o payload do `hook_fired` **não** carrega `action_signature`/`suggested_cmd`. Logo a via
> **offline NÃO funciona com o schema atual** — não há par para correlacionar. F2 foi entregue pela via
> **online** (DashMap in-daemon no `cli_suggester`), a única viável. O texto abaixo descreve a visão original.

**O substrato (parcial)**: o `activity.jsonl` registra eventos ordenados por `seq` dentro do mesmo `actor`.
A visão original assumia tanto o `hook_fired` (a sugestão, com `action_signature` `outcome:<tool>:<intent>:<ctx>`)
quanto o `tool_invoked` (a ação tomada) — mas **só o primeiro existe hoje**. A correlação **t↔t+1** foi feita
**online** (em memória do daemon via `pending_suggestion`), não por replay. Duas vias consideradas:

```
ONLINE  (daemon, baixo custo):
  hook_fired(t):   LastSuggestion[session] = (sig_t, suggested_class)   # +1 campo no payload: suggested_cmd
  tool_invoked(t+1): followed = action_class casa com suggested_class?
                     record_suggestion_uptake(followed)              # Counter par
                     record_wasted_enrichment(bytes_t) se !followed  # D5×D2
OFFLINE (replay, zero risco):
  `touring activity replay` varre o seq → reconstrói uptake histórico sem tocar o hot-path
```

Custo online: 1 `DashMap<session,LastSuggestion>` (igual ao health-delta cache) + 1 comparação. **Ajuste
mínimo necessário**: o `suggested_cmd` não é armazenado durável hoje — basta adicioná-lo ao `payload` do
`hook_fired` (1 campo). A via offline já funciona com o schema atual via `seq` ordering.

---

## 10. Atribuição causal — *foi o coupling ou foi o modelo?*

O risco nº1 de toda telemetria de agente: confundir melhora do **modelo** com efeito do **coupling**. Três
níveis de rigor, do barato ao caro:

1. **A/B contrafactual** (P6): sessões `control` (`TOURING_SUGGESTER_DISABLED=1` / `TOURING_MCP_ALL_TOOLS=1`)
   vs `treatment`, mesmo τ-bench. Compara D1/D2/D4. É a diferença entre *"a sessão foi boa"* e *"o coupling a
   tornou boa"*.
2. **Ablation fatorial**: ligar/desligar **uma feature por vez** (C1 brief, C5 summarizer, cli-suggest) para
   isolar a contribuição marginal de cada uma — evita creditar ao conjunto o que é de uma peça (e vice-versa).
   Aproxima um **Shapley** das features sobre a adesão/outcome.
3. **Redução de variância**: como a tarefa e o modelo são confounders dominantes, usar **pareamento por
   `action_signature`** (comparar control vs treatment dentro da mesma classe `outcome:<tool>:<intent>:<ctx>`)
   e CUPED (covariável pré-período = world-model success-rate prévio) para tirar a variância do modelo.

**Online vs offline**: a telemetria de produção é **observacional** (detecta correlação, sujeita a
confounder); o τ-bench é **intervencional** (controlado, causal, mas estreito). Complementares: produção
**levanta a hipótese** (uptake caiu para hint X), o τ-bench/ablation **confirma a causa** antes de qualquer
atuador L5 mexer na estratégia.

---

## 11. Schema de dados & governança

### 11.1 `coupling-telemetry-v1` (estende `kpi-commitments-v1`)
O snapshot datado `docs/kpi/YYYY-MM/YYYY-MM-DD.json` ganha um bloco `coupling`:
```json
{ "schema": "coupling-telemetry-v1", "snapshot_date": "YYYY-MM-DD",
  "coupling": {
    "dimensions": { "D1_adoption": {...}, "D2_str": {...}, ... },
    "kpis": [ { "id": "touring.coupling.suggestion_uptake", "actual": 0.0, "threshold": 0.40,
                "direction": "gte", "status": "advisory",
                "exemplar": { "session_id": "...", "seq": 11 } } ],
    "ab": { "arm": "treatment", "control_composite": null } } }
```
**Exemplars** (OTel): cada KPI degradado carrega a **sessão+seq exemplar** que o causou — liga o agregado ao
caso concreto para debug (P99 alto → a sessão que o gerou).

### 11.2 Cardinalidade (Views) — o risco silencioso
Labels por-path e por-sessão **explodem** a cardinalidade (cada arquivo/sessão = uma série). Solução OTel
**Views**: agregar/renomear/`DropAggregation` — manter labels de **baixa cardinalidade** (tool_class, intent,
dimensão) na série persistida; a alta cardinalidade (path, session) só vive nos **exemplars** e no
`activity.jsonl` (não na métrica agregada). Sem isto, a telemetria reintroduz o bloat que o backend removeu.

### 11.3 PII / segredos
A telemetria de tool-args pode capturar segredos. **Gen_ai semconv** torna `gen_ai.tool.call.{arguments,result}`
**opt-in** por esse motivo. Aqui: a coleta usa `action_signature` (tool_class+intent+context — **sem
conteúdo**) e bytes/counts; nunca o conteúdo do comando. O CEG `Env(KeyScope)` deny-by-default + PII-scan já
protege; a telemetria **herda** essa fronteira (registra a forma, não o segredo).

---

## 12. Loop de refinamento (L5) — como a telemetria **ensina**

| Sinal (KPI) | Atuador |
|---|---|
| **suggestion_uptake** baixo p/ um hint | cli-suggester **despromove** o hint + re-arma com número (I5: "−17% tok") |
| **str_bytes_per_emit** subindo | aperta o threshold de elisão do `--brief`/summarizer (auto-tune) |
| **adoption_ratio** baixo p/ capability | promove via hook só no gatilho alto-sinal-raro |
| **health_delta_net < 0** num path | streak → gotcha + reward −0.5 (já existe; expor alerta) |
| **wasted_enrichment** alto | corta o enrichment naquele tipo de op |
| **world_model_success / ema** datado | RL reward-shaping (flywheel: TD(λ)+ACO, drift **CUSUM**) |
| **A/B: treatment > control** (confirmado §10) | mantém a feature; senão, reverte |

O RL flywheel (QTable+TD(λ), DoubleQ, PrioritizedReplay, ACO, OnlineRL+CUSUM-drift) já é o motor; a
telemetria fornece o **sinal de recompensa de alto nível** (outcome/adesão) que hoje falta — fechando
*compreender → aprender → refinar*.

---

## 13. Modelo de maturidade da observabilidade (M0–M4)

| Nível | Característica | Onde o coupling está |
|---|---|---|
| **M0 — cego** | nenhum sinal | — |
| **M1 — atividade** | counters de "aconteceu" (gate-metrics) | **← hoje** (142 counters, in-memory) |
| **M2 — efetividade** | KPIs derivados + falsificáveis (uptake, adesão, U(a)) | F1–F4 |
| **M3 — temporal** | série datada + trend + drift | F5 |
| **M4 — causal/auto** | A/B atribuível + loop L5 que se auto-refina | **F6–F7 ✅ ← ALCANÇADO (2026-06-29)** |

O alvo é **M4**; a proposta é o caminho M1→M4. Cada fase do roadmap (§14) sobe um degrau.

---

## 14. Surface & Roadmap

**Surface** (onde consultar): novo `touring telemetry {suggestion-uptake, coupling, health}` **ou** estender
`touring kpi`; integrar `touring repo-score` (11-cat A–F), `touring world-model-status`, `touring activity replay`.

| Fase | Entregável | Tam | Dep | Risco |
|---|---|---|---|---|
| **F1** ✅ | **SHIPPED 2026-06-28** — `touring.coupling.*` (str, hook_p50/p99, health_delta_net, world_model_success) + mecanismo **advisory** (calibração 2 sem, não trip o `--check`) + **derivador** (`derived:` em `crates/touring-cli/src/cli/kpi.rs`). `utility{class}` per-classe → **F1.1**. Runtime-proven: 5 commitments, `world_model_success`=0.998, advisory funcionando (p50/p99 cold → ADVISORY) | **S** | — | ✅ DONE (clippy 0, 15 tests, Diamond 0.9787) |
| **F2** ✅ | **SHIPPED 2026-06-28** — uptake via caminho **online**: `pending_suggestion` DashMap **in-daemon** (`cli_suggester`, dispatch `hook_registry`) + 2 counters `suggestion_uptake_{emitted,followed}` (gate-metrics) + `derived:suggestion_uptake` KPI. **Offline (`activity replay`) era INVIÁVEL** — VP-Scout (Cadeia 5) achou `activity.jsonl` só com `hook_fired` (10029), **zero `tool_invoked`**, payload sem `action_signature`/`suggested_cmd` (§9 corrigido). Runtime-proven: emitted=6/followed=1, KPI=0.167 ADVISORY, cross-call persistence provada | **M** | F1 | ✅ DONE (clippy 0, 16 tests, 0 P0) |
| **F3** ✅ | **SHIPPED 2026-06-28** — `adoption_ratio` (a métrica-mãe) via caminho **online** in-daemon: helper `classify_adoption` no `cli_suggester` (reusa `action_is_touring_redirect` do F2 + `detect_antipattern` shared, gated em `tool_class=="bash"` p/ excluir os antipadrões stateful Edit/Read) + 2 counters `adoption_{touring,antipattern}_count` + 7º KPI `derived:adoption_ratio`. **Offline (activity.jsonl) era a mesma premissa falsa do F2** (sem `tool_invoked`). Runtime-proven: touring=3/antipattern=1 → ratio=0.75 PASS, family=7 KPIs | **M** | F2 | ✅ DONE (clippy 0, 5 tests, 0 P0, Platinum/Diamond) |
| **F4** | ativação C-item (G3) + Code-Mode (G4) + C9/C13 (G7) — counters pontuais | **M** | F1 | LOW |
| **F5** ✅ | **SHIPPED 2026-06-28** — scheduler **daemon-interno** (não cron): task periódica `tokio::interval` (6h, espelha idle watchdog) + flush on-shutdown no `graceful_shutdown` (captura a acumulação da sessão **antes** do reset) + completa o órfão `record_gate_metrics_daily_flush` (REGRA #0). Reusa `dispatch_request_async("cli-kpi",{snapshot:true})` (o writer provado). **Default-ON (2026-06-29)** — `gate_metrics_daily_enabled() != "0"` (opt-OUT via `=0`). **+ flush-fix (2026-06-29)**: roteava `project_root=""` → `dispatch_request_async` forçava `HookRuntime::new("")` (runtime WASM/cognitive/CRDT pesado p/ path inexistente dentro do `graceful_shutdown` → snapshot NÃO persistia); fix = `flush_kpi_snapshot` roteia por projeto warm do `RuntimeMap` (`map.keys().next()`) ou skip se vazio. Runtime-proven no binário de produção: trace `routing via project_root=… → flush OK` + snapshot reescrito (`stat %y` ns-precision; mtime 1s mascarava) | **M** | F1–F4 | ✅ DONE (default-ON + flush + daemon-reaper provados; gates 0, 50-dim ≥Gold, 6 P0) |
| **F6** ✅ | **SHIPPED 2026-06-29** — A/B contrafactual (§10 nível-1): `run_bench.py --arm {control,treatment,compare}` (control = `TOURING_SUGGESTER_DISABLED=1`+`TOURING_MCP_ALL_TOOLS=1`, treatment = default) + `attribution()` (delta atribuível, ε=0.01) + bloco `ab` no snapshot `touring kpi` (`build_ab_block` lê `docs/agentic-bench/.ab-latest.json` — `null` honesto sem A/B). Runtime-proven: control **0.8763 Gold** vs treatment **1.0000 Diamond**, Δ=**+0.1237** → `coupling_helps` attributable (dirigido pela conformance: control com all-tools → fora do range curado) | **L** | F2–F5 | ✅ DONE (24 pytest + 2 Rust, gates 0, Diamond) |
| **F7** ✅ | **SHIPPED 2026-06-29** — loop L5: motor puro `recommend_refinements` (§12 → `Vec<RefinementAction>` demote_hint/tighten_elision/promote_capability/alert_drift, **A/B-gated** via `ab_attributable`) + surface `touring kpi --refine` (advisory read-only) + atuador `hint_demotion_bump` graduado por confiança conformal no `cli_suggester` **default-OFF** (`TOURING_F7_ACTUATOR_ARMED`, dupla-gated: armed AND A/B; arming no daemon vivo = decisão humana pós-A/B). Runtime-proven end-to-end: `touring kpi --refine` emitiu `tighten_elision` **actionable** (str=871>800 + A/B `coupling_helps` confirmado) | **L** | F6 | ✅ DONE (9 unit tests, gates 0, 6 P0, Diamond) |

| **Task #6** ✅ | **SHIPPED 2026-06-29** — estrutura de **compounding de indução** (4 camadas × 4 pilares: code-mode/master-cli/learning-memory/intelligence). Camada **ativa** = `cli_suggester` pillar induction graduada **default-OFF** (`TOURING_PILLAR_INDUCTION_ARMED`, espelha F7c): `classify_pillar` + `master_cli_nudge`/`learning_memory_nudge` que **derivam o arg real** (**invariante de densidade**, feedback Gabriel 2026-06-29 — nudge nunca usa `<placeholder>` quando derivável; teste `pillar_nudges_carry_real_arg_no_placeholder`) cobrindo os 2 gaps (master-cli/learning-memory; code-mode/intelligence já disparam upstream C8/read-rust). Camadas **passivas** = rule `touring-4-pillars.md` (auto-load) + skill Touring + CLAUDE.md-pointer. **8º KPI** `derived:pillar_induction_ratio` (followed/emitted) → F7 promote/demote. + densificação REGRA #0 do `code_mode_command` loop (carrega o glob real do `for X in GLOB`). Tese ①: se ratio baixo armado → evidência afordância>persuasão (→ ⑨) | **L** | F6/F7 | ✅ DONE (8 tests incl. invariante densidade, gates 0, 50-dim Platinum/Diamond, 6 P0) |

| **F8** ✅ | **SHIPPED 2026-07-01** — **actor queue-wait observability** (origem: percepção de contenção cross-sessão do Gabriel + diagnóstico do turno de 11min). O actor por-projeto processa `RunHook` **serialmente** (mpsc 128); `hook_dispatch_latency` só começa no dequeue → a espera na fila era **invisível** (podia chegar aos budgets 15s/300s sem rastro). Entregue: campo `enqueued_at: Instant` no `ProjectCommand::RunHook` (`daemon_protocol.rs`) → `record_actor_queue_wait_us` no dequeue (`daemon.rs::run_project_actor`) + counters `actor_budget_timeout_count` (desistência no budget do oneshot) e `actor_send_timeout_count` (queue-full > `REQUEST_TIMEOUT`) nos 2 sites de `timeout` do dispatch. Histogram+counters em `gate_metrics.rs`/`gate_metrics_snapshot.rs` (`#[serde(default)]`); expostos no `touring gate-metrics -j`. Medição empírica pré-fix: leve 0,43s→1,04s sob 3 heavies quentes (mesmo projeto); projeto distinto imune (0,06s). Discrimina fila-Touring vs API na próxima lentidão | **S** | — | ✅ DONE (fmt/clippy 0; foundation 407 + dispatch 1310 + hook-runtime 366 tests) |

**Caminho crítico**: ~~F1~~ ✅ → ~~F2~~ ✅ → ~~F3~~ ✅ → ~~F5~~ ✅ → ~~F6~~ ✅ → ~~F7~~ ✅ → ~~Task #6~~ ✅ → ~~F8~~ ✅
(2026-06-29: **loop L5 fechado** — A/B atribuível runtime-proven `coupling_helps` Δ+0.1237 + atuadores graduados gated por A/B; `touring kpi --refine` emitiu `tighten_elision` actionable. **Task #6**: a estrutura de compounding ativa a indução por pilar — o experimento que mede se a persuasão funciona, alimentando a decisão afordância/⑨). **Arco ⑦ em M4 (causal/auto). F4 (counters pontuais G3/G4/G7) é o único opcional restante.**

---

## 15. Riscos e mitigações

| Risco | P×I | Mitigação |
|---|---|---|
| Telemetria reintroduz token-bloat | MED×HIGH | P7 + Views (cardinalidade) + KPI `str_bytes_per_emit` vigia a si mesmo |
| **Cardinalidade explode** (path/session labels) | MED×HIGH | §11.2: alta-card só em exemplars/activity.jsonl; agregado = tool_class/intent |
| Correlação uptake imprecisa (t+1 ≠ resposta) | MED×MED | janela 1-passo + casamento por sig; ambíguo = "não-seguido" (conservador); offline replay valida |
| Atribuir ao coupling o que é do modelo | HIGH×HIGH | §10: A/B + ablation obrigatórios antes de F7 |
| **Thresholds irreais** (str 400, p99 1ms) | (corrigido) | §1.4: calibrados ao baseline; advisory por 2 semanas |
| Counters zeram no restart | (resolvido) | L3 scheduler → snapshots datados |
| **PII/segredo na telemetria** | LOW×HIGH | §11.3: só signature+bytes, nunca conteúdo; herda CEG Env-scope |

---

## 16. Justificativa de fechamento — por que ESTA estrutura

1. **Reusa o que existe** (`touring kpi`, 142 counters, world-model 21.748ex, activity.jsonl, τ-bench, repo-score)
   — o backend já carrega ~90% do substrato; o que falta é **derivar, persistir e fechar o laço**.
2. **Ancorada num modelo causal** (`U(a)`→adesão→outcome→aprendizado), não numa lista de counters — é o que
   permite *compreender e aprender a dinâmica*, ler os números contra uma hipótese, e refutá-la.
3. **Ataca a métrica-mãe + o gap decisivo** (adesão, uptake) — sem eles, dashboard é teatro de atividade.
4. **Computável hoje no caminho crítico**: `U(a)` e `world_model_success` derivam de dados já duráveis — F1
   entrega sinal sem instrumentação nova.
5. **Honesta no baseline**: thresholds calibrados a valores reais medidos (str 1029, p99 429ms); o que não
   existe está marcado ⬜ (scheduler, external gates, uptake correlation).
6. **Falsificável + persistente + atribuível + governada** (cardinalidade, PII) — as pernas que separam
   "compreender a dinâmica" de "olhar números".
7. **Fecha o loop** — despromove hints ruidosos, aperta thresholds, realimenta o RL: o flywheel
   *compreender → aprender → refinar* que o Gabriel pediu, com maturidade M1→M4 explícita.

---

## Referências

- Research interno: `docs/2026-06-26-{token-footprint-diagnosis,touring-llm-coupling-strategy,harness-architecture-insights,touring-capability-map,coupling-backlog,master-plan-v3-tracker}.md`
- Telemetria real (VGP-verificada): `crates/touring-foundation/src/gate_metrics.rs`, `crates/touring-cli/src/cli/kpi.rs:231` (resolver), `crates/touring-hooks-shared/src/action_signature.rs`, `crates/touring-ceg/src/gateway/outcome_learner.rs` (`action_world_model.json`), `docs/kpi/commitments.yaml`
- Cross-audit do backend: `docs/2026-06-27-cross-audit-coupling-backlog.md`
- OpenTelemetry (context7): GenAI semconv (`gen_ai.operation.name=execute_tool`, `gen_ai.tool.name`, `gen_ai.client.token.usage`, `mcp.session.id`); metrics data-model (Histogram/Gauge temporality); Views (cardinalidade); Exemplars (métrica→trace); instrument selection; naming (usage/utilization/time)
- Papers: Confucius (harness>model +22pp), CodeAct/Tool-Search (−60-85%), WarpGrep (+3.7pp/−17%), BiasBusters 2510.00307 (20-81%), Chroma Context-Rot, "Is Grep All You Need?" (93→55%)
- Eval methodology: `docs/agentic-bench/run_bench.py` (τ-bench composite 0.40/0.40/0.20)
```
