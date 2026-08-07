---
okf_version: "1.0"
type: Strategy
title: "Memento → Touring: refinar a dinâmica de RL a partir do M-MDP e da política de recuperação por valor"
description: "Análise profunda do Memento (arXiv 2508.16153 + código oficial) e extração de insights acionáveis para o subsistema de RL do Touring. Inclui diagnóstico do RL atual com 8 defeitos verificados por execução, um deles com divergência provada por simulação."
tags: [rl, memento, cbr, m-mdp, soft-q-learning, memory, retrieval-policy, learning-memory]
timestamp: "2026-08-04T20:05:00-03:00"
plan_id: 2026-08-04-memento-rl-insights
---

# Memento → Touring — refinar a dinâmica de RL

> **Tese em uma linha**: o Touring já tem as duas metades do Memento — um banco de
> casos (`touring memory`) e um aprendiz por reforço (`touring learning`) — mas elas
> **nunca se tocam**. O recall ordena por similaridade e ignora valor; o RL aprende
> sobre `hash(tool)%8` e não sobre o caso recuperado. O Memento é exatamente a
> demonstração de que a *política de recuperação* **é** a política de RL.

---

## 1. O que é o Memento — formalismo completo

Memento (Zhou et al., arXiv 2508.16153; 2.558 ★; código oficial em Python) resolve
adaptação contínua de agentes **sem tocar nos pesos do LLM**. Reformula planejamento
como um MDP aumentado por memória e aprende uma política *neural de seleção de casos*
por soft Q-learning online.

### 1.1 Memory-Based MDP (Definição 3.1)

Uma tupla `⟨S, A, P, R, γ, M⟩` onde `M = (S × A × ℝ)*` é o **espaço de memória**. A
diferença para um MDP padrão é exatamente essa: a memória entra no estado.

Banco de casos no passo *t*: `M_t = {c_i}` com **`c_i = (s_i, a_i, r_i)`** — cada caso
carrega o **reward** que ele obteve. Essa é a peça que o Touring não tem.

### 1.2 A política do agente CBR (Eq. 1)

```
π(a | s, M) = Σ_{c ∈ M} μ(c | s, M) · p_LLM(a | s, c)
```

`μ` é a **política de recuperação** (o que se aprende); `p_LLM` é o LLM **congelado**.
Toda a capacidade de aprender vive em `μ`.

### 1.3 O ciclo de 5 fatores (Eq. 2)

```
p(τ) = Π_t  μ(c_t|s_t,M_t) · p_LLM(a_t|s_t,c_t) · 1[r_t = R(s_t,a_t)] · 1[M_{t+1} = M_t ∪ (s_t,a_t,r_t)] · P(s_{t+1}|s_t,a_t)
        └─ (1) Retrieve ─┘  └── (2) Reuse&Revise ──┘ └─ (3) Evaluation ─┘ └──── (4) Retain ────┘  └─ (5) Transition ─┘
```

(1), (2) e (4) são **comportamento do agente**; (3) e (5) são o ambiente. O Touring
tem (2) e (5); tem (4) só parcialmente (grava sem `r`); e **não tem (1) aprendida nem
(3) sistemática**.

### 1.4 Soft Q-learning sobre casos (Eqs. 3-8)

Objetivo de máxima entropia (Haarnoja et al., 2018):

```
J(π) = E_τ [ Σ_t  R(s_t,a_t) + α·H(μ(·|s_t,M_t)) ]                        (3)
V^π(s,M) = Σ_c μ(c|s,M)[ Q^π(s,M,c) − α log μ(c|s,M) ]                    (4)
Q^π(s,M,c) = E[ R(s,a) + γ·V^π(s',M') ]                                   (5)
```

**A solução fechada — o coração do paper (Eq. 7):**

```
μ*(c | s, M) = exp(Q*(s,M,c)/α) / Σ_{c'∈M} exp(Q*(s,M,c')/α)
```

> **A política ótima de recuperação é um softmax sobre o Q dos casos.** Recuperar
> deixa de ser "buscar o mais parecido" e passa a ser "escolher o de maior valor
> esperado, com temperatura α controlando exploração".

Atualização por TD em soft Q-learning (Eq. 8):

```
Q(s_t,M_t,c_t) ← Q + η[ r_t + γα·log Σ_{c'∈M_{t+1}} exp(Q(s_{t+1},M_{t+1},c')) − Q(s_t,M_t,c_t) ]
```

### 1.5 Episodic control — aprender Q sem rede profunda (Eqs. 9-11)

Aprender Q direto por TD sobre linguagem natural é difícil. A saída (seguindo Neural
Episodic Control, Pritzel et al. 2017) é **regressão por kernel sobre a memória
episódica** `D = {(s, c, Q)}`:

```
Q_EC(s,M,c;θ) = Σ_{(s',c',Q')∈D_c}  k_θ(s,s')·Q'  /  Σ_{(ŝ,ĉ,Q̂)∈D_c} k_θ(s,ŝ)          (9)
```

onde `D_c` são as interações passadas **com o mesmo caso recuperado c**. Q vira uma
**média ponderada por similaridade dos Q históricos daquele caso** — nenhuma rede
profunda é necessária, só um kernel de similaridade e os valores guardados. Isso é
diretamente implementável em Rust sobre o que o Touring já tem.

### 1.6 O que a implementação real faz (o pragmatismo que interessa)

Aqui o paper e o código divergem do formalismo — e a simplificação é a lição:

- **Colapso para single-step** (§4.2): "o planner CBR pode ser simplificado a um
  cenário de passo único... isso colapsa o alvo TD da Eq. 10 no reward imediato,
  **evitando alvos não-estacionários**. Sem bootstrapping."
- **MSE → Cross-entropy** (Eq. 14 → 15): como `r ∈ {0,1}`, trocam MSE por CE porque
  "MSE sofre de gradientes que somem perto de 0/1, enquanto CE dá sinal
  numericamente mais estável":

```
L(θ) = E[ −r·log Q(s,c;θ) − (1−r)·log(1 − Q(s,c;θ)) ]                     (15)
```

  com `Q = p(r=1 | s,c;θ)` — a probabilidade de que o caso `c` seja **boa referência**
  para o estado `s`.
- **Read determinístico** (Eq. 16): `Read_P(s,M) = TopK Q(s,c_i;θ)` — TopK em vez de
  amostrar do softmax, "para reduzir a aleatoriedade e aumentar a interpretabilidade".
- **Write sempre** (Eq. 12): `M_{t+1} = M_t ∪ {(s_t,a_t,r_t)}` — sucessos **e**
  falhas. "Acumulando tanto sucessos quanto falhas, a memória permite evitação
  retrospectiva de erros passados."
- **Só o passo final da trajetória vira caso** (§5.3): "para evitar armazenamento
  redundante, apenas estado, ação e reward do **passo final** de cada trajetória são
  escritos, mantendo o banco compacto e informativo."
- **A rede é minúscula**: `MemoryRetrieverClassifier` = SimCSE congelado +
  `Linear(hidden·2 → 512) → ReLU → Dropout(0.2) → Linear(512 → 2)`. Um cross-encoder
  de duas camadas. Todo o ganho vem da *estrutura*, não do tamanho.

---

## 2. O que o Memento prova empiricamente

| Resultado | Número |
|---|---|
| GAIA validation (Pass@3) | **87,88%** — top-1 entre frameworks open-source |
| GAIA test | 79,40% |
| DeepResearcher (7 datasets) | 66,6 F1 / 80,4 PM |
| SimpleQA | 95,0% |
| HLE | 24,4 PM (GPT-5: 25,32) |
| Ganho OOD do CBR | **+4,7 a +9,6 pontos absolutos** |

### 2.1 A ablação de K — a lição mais transferível (Tabela 3)

| K | 0 | 1 | 2 | **4** | 8 | 16 | 32 |
|---|---|---|---|---|---|---|---|
| F1 | 59,9 | 63,6 | 63,7 | **64,5** | 64,1 | 63,9 | 63,9 |
| PM | 72,2 | 77,9 | 78,1 | **78,5** | 78,2 | 78,1 | 78,1 |

**K=0 → K=1 vale +3,7 F1 / +5,7 PM. K=1 → K=4 vale +0,9 / +0,6. K > 4 piora.**

> O primeiro caso carrega quase todo o valor. O paper: *"CBR se beneficia de uma
> memória pequena e de alta qualidade, ao contrário de few-shot prompting, onde mais
> exemplos costumam ajudar. Seleção cuidadosa e curadoria de memória são cruciais."*

### 2.2 Aprendizado contínuo em 5 iterações (Tabela 4)

| Config | Iter 1 | Iter 5 |
|---|---|---|
| sem CBR | 78,65 | 84,47 |
| CBR não-paramétrico | 79,84 | 84,85 |
| CBR paramétrico | **80,46** | **85,44** |

O Q aprendido bate a similaridade pura em **todas** as iterações — mas por ~0,6
ponto. O salto grande (+1,8 na iteração 1) é ter memória *alguma*.

**Saturação**: "com apenas ~3k dados de treino, o Case Bank satura rápido... ganhos
marginais após poucas iterações". Memória boa satura cedo — o que importa é a
curadoria, não o volume.

### 2.3 O achado contraintuitivo — planner rápido vence planner deliberativo (Tabela 6)

| Planner | Executor | Média GAIA |
|---|---|---|
| **gpt-4.1 (rápido)** | o3 | **70,91%** |
| o3 (deliberativo) | o3 | 63,03% |

**−7,9 pontos ao tornar o planner mais deliberativo, com o mesmo executor.** A causa
diagnosticada: "o planner com o3 ou responde direto — pulando a geração do plano — ou
produz planos verbosos demais, que enganam o executor com instruções incompletas...
planejamento excessivamente deliberativo induz **confusão de papéis**, minando a
própria especialização que a arquitetura de dois estágios existe para explorar."

---

## 3. O RL do Touring hoje — mapa por execução

Estado vivo (`touring learning status`, daemon 30.3.1, PID 2639979):

```json
{"update_count":5, "ema_reward":0.3198, "mean_td_error":75.04,
 "linucb_loaded":true, "bandit_type":"linucb", "arm_count":8,
 "agentic_rl_state":{"active":true, "update_count":1}}
```

Arquitetura (verificada em `crates/touring-intelligence/src/rl/`, 43.605 LOC):

| Peça | Onde | Papel |
|---|---|---|
| `OnlineRLEngine` | `rl/online_rl.rs` (1.368 L) | orquestra reward → QTable + LinUCB |
| `QTable` TD(λ) | `rl/rl/qtable.rs` (1.439 L) | α=0,1 γ=0,99 λ=0,9 |
| `LinUCBBandit` | `rl/bandit/linucb.rs` (1.473 L) | 8 arms, FEATURE_DIM=25 |
| `memory/recall.rs` | `rl/memory/recall.rs` (954 L) | **RRF puro (k=60) sobre listas de similaridade** |

---

## 4. Diagnóstico — 8 defeitos, cada um com evidência

### D1 · BLOCK · Bootstrap em self-loop faz o Q divergir ~100×

`online_rl.rs:341` chama:

```rust
self.last_td_error = qtable.update(update_state, update_action, n_step_return,
                                   state,   // ← next_state == state: SELF-LOOP
                                   None, true);
```

E `qtable.rs:514` computa `td_error = reward + gamma·next_q − current_q` com
`next_state == state` e `next_action = None` (max sobre o **mesmo** estado). O ponto
fixo deixa de ser o reward e passa a ser `G/(1−γ)`.

**Provado por simulação da regra transcrita** (`touring run --lang python`, constantes
lidas da fonte):

```
n_step_return G (r=1.0 constante) = 6.7316
Q após 4000 updates: [655.4, 655.4, 655.4]
ponto fixo teórico G/(1-gamma) = 673.2
```

**Q converge para ~655 enquanto o reward vive em [−1, 1].** Inflação de duas ordens
de grandeza. O `mean_td_error` de 75,04 é consistente com Q persistido nessa escala
(INFERÊNCIA 0,85 — o mecanismo está provado; o valor 75 específico não foi isolado).

Memento evita isso por construção: **colapsa para single-step, sem bootstrap** (§1.6).

### D2 · BLOCK · `terminal = true` é ignorado

Em `qtable.rs:508`, `terminal` só é honrado quando `current_q == 0.0` (primeira
visita). Depois disso o código bootstrapa mesmo com `terminal = true`. O chamador
pede semântica terminal e não recebe.

### D3 · BLOCK · Dupla contagem do desconto

`n_step_return = Σ γ_n^i · r_i` (γ_n = 0,95, cap 8) já é um retorno descontado
multi-passo — e é passado como o argumento `reward`, que a `update` desconta de novo
com γ = 0,99. O horizonte é contado duas vezes.

### D4 · WARN · Dois γ inconsistentes

`GAMMA = 0.95` (`online_rl.rs:41`) para o n-step; `gamma = 0.99` (`qtable.rs:36`) para
o bootstrap. Dois horizontes efetivos diferentes no mesmo update.

### D5 · BLOCK · Crédito vai para o arm errado — o laço está aberto

`online_rl.rs:380`:

```rust
let arm_index = (djb2_hash(&reward.tool_name) % NUM_ARMS as u64) as usize;
linucb.update(arm_index, &features, raw_reward);
```

O arm que **recebe crédito** é um hash do nome da ferramenta módulo 8. O arm que
**tomou a decisão** (via `select_arm`, usado em `cli/suggest.rs`, `task_list.rs`,
`smart_cache.rs`) nunca é registrado. **A recompensa não volta para quem decidiu** —
é a definição de laço aberto em RL. Também há colisão: ferramentas distintas caem no
mesmo bucket de 8.

### D6 · BLOCK · A superfície de decisão exposta ao usuário não consulta o bandit

`cli/suggest.rs:18` monta `features = vec![query.len()/100.0, 0,0,0,0,0]` — **6 dims,
5 delas zeros fixos**, para um bandit de 25 dims; e o único sinal vivo é o
**comprimento da string**. Pior, lê `rt.learning.bandit` direto em vez do
inicializador preguiçoso `get_bandit()`.

**Provado por execução** — duas queries radicalmente diferentes:

```
$ touring suggest next "find the auth validator symbol"
{"confidence":0.5,"source":"fallback","suggested_action":"explore"}
$ touring suggest next "<400 chars>"
{"confidence":0.5,"source":"fallback","suggested_action":"explore"}
```

Sempre o mesmo fallback constante. O LinUCB treinado nunca é consultado ali.

### D7 · WARN · 5 de 13 sites de reward emitem constante

| Site | Reward |
|---|---|
| `cli-tantivy-search` / `-fuzzy` / `-suggest` | `1.0` fixo |
| `cli-ast-semantic` / `cli-ast-quality` | `1.0` fixo |
| `pre_write` (×2), `post_edit` (×2), `session`, `session_productivity`, `batch` | expressão real |

Um reward constante tem **variância zero** — a regressão do LinUCB aprende `x·θ = 1`
para todo `x`, sem poder discriminar nada. Com `ema_alpha = 0,1` e
`min_reward_delta = 0,01`, um reward constante ainda é filtrado como ruído após ~44
updates (`0,9^n < 0,01`), e a partir daí o sinal some de vez.

### D8 · ADVISORY · `mean_td_error` não é média

`cli/learning.rs:32`: `let mean_td_error = online.map(|e| e.last_td_error())`. O campo
JSON diz "mean" e entrega o **último**. `RlMetrics` já mantém um `td_error_history`
(ring buffer) que daria a média de verdade.

### O que NÃO é defeito (verificado e descartado)

- `update_count = 5` **não** é o filtro `min_reward_delta` mordendo — é daemon jovem
  (reiniciado várias vezes hoje na propagação 30.3.1). Cheguei a formular essa cadeia
  causal e a aritmética a refutou.

---

## 5. O mapeamento Memento → Touring

| # | Mecanismo Memento | Touring hoje | Transplante |
|---|---|---|---|
| **T1** | Caso = `(s, a, r)` (Eq. 12) | entrada = `{key, tier, type, value}` — **sem `r`** | adicionar `outcome`/`reward` ao registro de memória |
| **T2** | `Read_P = TopK Q(s,c;θ)` (Eq. 16); `μ* = softmax(Q/α)` (Eq. 7) | RRF k=60 sobre similaridade — **zero termo de valor** (`recall.rs` não contém `reward`/`value`/`utility`) | reordenar o recall por `Q(s,c)` |
| **T3** | Write sempre — sucessos **e** falhas | `memory store` manual, sem outcome | gravar automaticamente no fecho de fase, com o veredito do gate como `r` |
| **T4** | Q = classificador binário CE, **single-step, sem bootstrap** (Eq. 15) | TD(λ) com bootstrap em self-loop (D1) | colapsar para single-step; CE em vez de MSE |
| **T5** | K=4; memória pequena e curada | recall top-20 | reduzir o K default e curar por valor |
| **T6** | Planner rápido > planner deliberativo (Tab. 6) | TACO empilha deliberação no orquestrador | tratar deliberação extra como custo, não virtude |

### O insight estrutural

O Touring tem o **feromônio ACO** descrito na constituição (`memory store` +
`learning reward`) — mas as duas trilhas são **paralelas e desconectadas**:

```
memory store ──→ memory.db ──→ recall (RRF por similaridade) ──→ contexto
learning reward ──→ QTable/LinUCB (hash%8) ──→ ninguém consome na decisão
```

O Memento mostra a topologia correta: **uma única trilha**, onde o reward de ontem é
o que ordena o recall de hoje.

```
outcome ──→ caso (s,a,r) ──→ Q(s,c) ──→ recall ordenado por valor ──→ decisão ──→ outcome
                 └──────────────────── mesmo laço ────────────────────────┘
```

### Onde o Memento vai além do baseline de mercado (Context7)

Consultei o Context7 (`/websites/langchain_oss_python_langgraph`). O padrão canônico
do LangGraph Store é:

```python
memories = await runtime.store.asearch(namespace, query=state["messages"][-1].content, limit=3)
```

**Busca semântica pura, sem qualquer sinal de outcome no ranking** — exatamente onde o
Touring já está. Ou seja: o recall por RRF do Touring está *em paridade com o estado
da arte de framework*; o Memento é que aponta o passo seguinte. O `limit=3` do
LangGraph, aliás, converge de forma independente com o K=4 ótimo do Memento.

---

## 6. Estratégia proposta — 4 fases

Ordenadas por (valor ÷ risco). As fases 1 e 2 são **pré-requisito**: não adianta
sofisticar a política de recuperação enquanto o aprendiz diverge.

### Fase 1 — Consertar a dinâmica de RL (D1-D4, D8)

Sem isto, qualquer coisa construída em cima herda Q inflado ~100×.

- Colapsar para **single-step sem bootstrap** quando `terminal = true` (a lição
  explícita do Memento §4.2: evita alvo não-estacionário) **ou** passar o
  `next_state` verdadeiro. Honrar `terminal` incondicionalmente.
- Parar de passar `n_step_return` como reward imediato para um update que desconta de
  novo (D3); unificar o γ (D4).
- `mean_td_error` passa a usar o `td_error_history` que já existe (D8).
- **Gate**: `|Q| ≤ 1/(1−γ)·max|r|` como invariante testada; `td_error` convergindo.

### Fase 2 — Fechar o laço de crédito (D5, D6, D7)

- Registrar o `(arm, features)` escolhido por `select_arm` numa tabela de decisões
  pendentes; o reward subsequente credita **aquele** arm (é o `(s,c)` do Memento).
- `cli_suggest_next` passa pelo `get_bandit()` e monta as 25 features reais.
- Substituir os 5 rewards constantes por sinal com variância (latência, nº de hits,
  se o resultado foi de fato consumido).

### Fase 3 — Transplantar o banco de casos com valor (T1, T3)

- Estender o registro de memória com `outcome: {reward: f32, gate_verdict, task_id}`.
- Gravar caso automaticamente no `loop_phase_close.py` — o veredito de
  `loop_converged.py` (exit 0/1) é um `r ∈ {0,1}` **já existente e binário**, exatamente
  a forma que a Eq. 15 do Memento assume. O Touring já produz o sinal; só não o guarda.
- Gravar **falhas também** (Eq. 12). Hoje só o que dá certo vira lição.

### Fase 4 — Recall ordenado por valor (T2, T5)

- `Q_EC` por kernel (Eq. 9) sobre os casos: média dos `r` históricos ponderada pela
  similaridade que o RRF **já computa**. Nenhuma rede nova — é aritmética sobre o que
  o recall já tem em mãos.
- Reordenação final: `score = RRF ⊕ Q_EC`, com TopK reduzido (K≈4, seguindo a Tabela 3
  e o `limit=3` do LangGraph).
- Só depois, se medir ganho, considerar o Q paramétrico (cross-encoder de 2 camadas).

**Fase 4 é onde o ganho aparece; fases 1-2 são o que o torna possível.**

---

## 7. Como medir — e o que falsificaria a tese

| Hipótese | Métrica | Falsificação |
|---|---|---|
| A dinâmica está quebrada (F1) | `\|Q\|` máximo; `td_error` médio | Q dentro de `[−1/(1−γ), 1/(1−γ)]` e td convergindo já hoje |
| O laço fechado ensina (F2) | `ema_reward` sobe e `select_arm` diverge do uniforme | arms permanecem indistinguíveis após N decisões |
| Recall por valor > por similaridade (F4) | A/B do recall: taxa de reuso do caso recuperado | ordenação por valor não bate RRF puro — **plausível**: o Memento só ganhou +0,6 do paramétrico sobre o não-paramétrico |
| K pequeno basta | qualidade vs K ∈ {2,4,8,20} | curva sobe monotonicamente até 20 |

**Nota honesta de calibração**: a Tabela 4 do Memento mostra que o Q *paramétrico*
supera a similaridade pura por apenas ~0,6 ponto. O ganho grande (+1,8 na primeira
iteração) vem de **ter memória com valor algum**, não de aprendê-lo com sofisticação.
Isso desloca o valor esperado para as fases 1-3 e recomenda ceticismo com a fase 4
paramétrica.

---

## 8. Gate humano

Este documento é **análise e estratégia** — nenhuma linha de código foi alterada. Os
defeitos D1-D8 são reais e verificados, e a REGRA #21 manda corrigi-los; mas a ordem,
o escopo e o momento são decisão do Gabriel.

Decisões pedidas:

1. **Fase 1 é aprovada?** (é a correção de bug pura — D1-D4 e D8, sem feature nova)
2. **Escopo**: parar na Fase 2 (laço fechado), ir até a 3 (banco com valor), ou até a 4
   (recall por valor)?
3. A Fase 4 paramétrica fica explicitamente **fora** até a 1-3 medirem ganho?

---

## Proveniência

| Fonte | Como foi lida |
|---|---|
| Paper (arXiv 2508.16153v2) | PDF baixado e lido — págs. 3-19, equações transcritas verbatim |
| Código oficial | `gh api` → `memory/{np_memory,parametric_memory,train_memory_retriever}.py` |
| RL do Touring | leitura de fonte + `touring learning status` + execução de `touring suggest next` |
| Divergência do Q | **simulação da regra transcrita** via `touring run --lang python` (Code Mode) |
| Best practices | Context7 `/websites/langchain_oss_python_langgraph` — LangGraph Store |

Antecessores: `/log.md` · diagnóstico OKF em `diagnostics/touring-20260804T195221.md`.
