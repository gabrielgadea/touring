---
type: Research
title: Rodada 2 — exploração exaustiva (infraestrutura Touring × fontes externas × Context7)
description: Segunda rodada pedida por Gabriel antes da decisão. Corrige três afirmações da rodada 1, prova o laço de crédito ao vivo e traz o eixo que restringe a estratégia.
plan_id: 2026-08-25-autoresearch-rl-intelligence
tags: [autoresearch, rl, credit-assignment, dspy, gepa, context7]
timestamp: 2026-08-25T20:30:00-03:00
okf_version: "0.1"
---

# Rodada 2 — o que a exploração exaustiva mudou

Part of the [bundle](/index.md). Antecede a decisão do §4 da [estratégia](/strategy-2026-08-25-autoresearch.md).

## 1. A descoberta central: o laço de crédito existe, está registrado e nunca foi usado

`cli_memory_credit` (`crates/touring-cli/src/cli/memory.rs:317-388`) implementa atribuição de
crédito completa — Memento (arXiv 2508.16153) Eq. 9 na forma online, ledger que reivindica cada
recall **uma única vez**, `blend_case_value` para a média móvel, federado sobre os 7 `memory.db`
da máquina. `memory recall` **já registra** as chaves servidas no ledger (`memory.rs:782-785`).
`touring memory credit` é subcomando documentado da CLI.

O símbolo aparece em **exatamente 3 arquivos**: a definição, o registro no hook registry e o
dispatch. **Zero chamadores** em qualquer ADW, skill, hook ou script.

### Prova ao vivo (mutação medida nesta sessão)

```
$ touring memory credit --reward 0.9 "loop-state:2026-08-25-autoresearch-rl-intelligence"
{"credited":20,"served":20,"ledger_credited_total":1,"ledger_unclaimed_evictions":0}

outcome_reward IS NOT NULL:  147 → 167   (+20 entradas, +13,6% relativo, UM comando)
```

`ledger_credited_total: 1` — este foi o **primeiro crédito reivindicado na história do daemon**.
Os 1,7% não são um sistema que aprende devagar: são um sistema cujo laço nunca foi fechado
nenhuma vez.

## 2. Três correções à rodada 1 (afirmações que a medição derrubou)

| Rodada 1 afirmou | Medido na rodada 2 | Consequência |
|---|---|---|
| "DSPy: 0 arquivos em `crates/`" | `touring-cortex/src/handlers/dspy_compile.rs` (52 ocorrências) + `crates/touring-cortex/src/dspy/mod.rs`; 23 arquivos prod citam dspy | A afirmação valia só para o símbolo `dspy_cluster`. **Existe substrato DSPy no Rust** — P3 parte de mais alto |
| "Sistema A: construir o sinal de outcome" | `loop_phase_close.py:144-159` **já tem** `credit_recalls()` chamando `touring memory credit`, acionado por `--credit-query` (:396) que **nunca recebe valor** | P0 deixa de ser construção e vira **afordância**: o executor deve derivar as queries, não esperar que o chamador lembre |
| "`router_accuracy` é STUB — o router nunca recebeu feedback" | `factory.py:155-158` **já** chama `touring learning reward factory_router_<adw>` no fecho (`:208`) | O reward existe; o STUB é o **cálculo do KPI**, não a ausência de sinal. Alvo muda de "instrumentar" para "computar" |

Confirmado sem correção: `adw.py` tem **1** sítio de reward (`:3296`, só `campaign`) em 3.823 linhas.

## 3. Lente externa

### 3.1 Context7 (best practices — exigência explícita de Gabriel)

- **`/nousresearch/hermes-agent-self-evolution`** (481 snippets) — a arquitetura de referência do
  que queremos: laço de 6 passos *selecionar alvo → construir dataset de avaliação minerando o
  `session_db` de uso real → embrulhar como módulo DSPy → rodar GEPA (fallback MIPROv2) →
  avaliar em holdout com significância → deploy sob aprovação, rollback por `git revert`*.
  Otimiza **skills, descrições de ferramentas, system prompts e código**; sem GPU, US$2–10 por run.
- **`/websites/dspy_ai`** — o diferencial do GEPA é o **canal de feedback textual** na métrica:
  `dspy.Prediction(score, feedback)`, com amostragem por fronteira de Pareto. A métrica pode
  devolver feedback **por predictor** (`pred_name`/`pred_trace`). Dataset: **30–300 exemplos**
  para train e val cada — um piso concreto que P3 precisa atingir antes de começar.

> **Convergência arquitetural**: o par (score, feedback textual) que o GEPA exige é exatamente a
> forma que nossos gates já produzem — exit code + veredito em texto. E é exatamente o canal que
> falta no retry cego do `adw.py` (memória `adw-retry-sem-feedback-do-gate`). Um único conserto
> — propagar o veredito do gate como feedback — serve ao retry hoje e ao GEPA depois.

### 3.2 Survey de ~300 papers — o eixo que **restringe** a estratégia

*Agentic Evolution: From Self-Improving Agents to Co-Evolving Human–AI Systems* (Microsoft
Research, 07/2026). Taxonomia de 3 eixos: **substrato** S = ⟨Π Cortex, A Action, M Memory&Sense⟩ ×
**via de consolidação** (∆ estrutural / ∇ paramétrica) × **pressão seletiva** (autônoma H=0 →
humana H≠0).

A tese que nos constrange, citada:

> *"Autonomous evolution produces its strongest reported results where deterministic verifiers,
> independent of the system being evaluated, are available; absent such verifiers, self-referential
> and proxy-based signals yield diminishing returns and can degrade with iteration."*

Duas consequências diretas:

1. **Somos 100% estruturais (∆)** — prompts, código, fatos em slots discretos. Os modos de falha
   catalogados (Echo Trap, Template Collapse, Information Self-Locking, o rebote trifásico de
   reward hacking) são **paramétricos** e não nos atingem. Os nossos são outros dois, e ambos já
   aparecem nos nossos números: **acumulação sem limite** (8.671 memórias, 19,8% nunca recuperadas)
   e **teto de orçamento de busca** (`explore_rounds_to_dry`, `max_rounds` — vimos hoje o teto
   recusar rodada legítima).
2. **A fronteira do verificador particiona nossos alvos**. Onde temos verificador determinístico e
   independente (cargo check/test/clippy, `loop_converged.py`, exit code de gate) a evolução pode
   ser autônoma. Onde o sinal é proxy (`adoption_ratio`, quality50, "o programa fundiu bem?") ela
   **degrada com a iteração** e exige o gate humano. `judge_attest.py` já é nossa defesa de
   independência (o juiz não pode ser gravável pelo julgado) — o survey a valida como necessária,
   não opcional.

Adjacente e **descartado com veredito**: *Tree-based Credit Assignment* (arXiv:2605.04811, TreeMem)
credita **agentes** por média Monte Carlo sobre ramos de rollout — exige orçamento de rollout que
não temos e não credita entradas de memória. Nossa linha é a do Memento, já implementada.

## 4. O que isso faz com o plano

- **P0 encolhe e fica mais afiado**: não é "construir o sinal", é fechar 3 fios existentes —
  (a) `loop_phase_close` derivar sozinho as `--credit-query`; (b) `adw run` creditar por run como
  `campaign` já faz; (c) computar `router_accuracy` a partir do reward que `factory.py` já emite.
  Critério: `outcome_reward` coverage medido (baseline **167/8.671 = 1,93%** após a prova de hoje).
- **P2 (research loop) ganha uma restrição de projeto**: só pode ser autônomo sobre alvos com
  verificador determinístico; alvos-proxy sobem para o gate humano por construção, não por escolha.
- **P3 (DSPy/GEPA) ganha um piso**: 30–300 exemplos rotulados por alvo — que só existem **depois**
  do P0. A dependência P0→P3 deixa de ser preferência e vira pré-requisito medido.
- **Novo item candidato**: propagar o veredito do gate como feedback textual — conserta o retry
  cego hoje e é a métrica GEPA amanhã.

## 5. Limitação registrada

O `case_ledger` é **em memória** (`hook_runtime.rs:882`, `Default::default()`): reinício do daemon
descarta os recalls pendentes. Créditos precisam ser reivindicados dentro da vida do daemon — ou o
ledger precisa persistir. Não medido: quantos recalls se perderam assim (`unclaimed_evictions`
zerou com o restart).

---

## 6. O pré-requisito do P4, medido (2026-08-26 00:00) — presença ≠ discriminação

Com o P0 no ar, a pergunta do P4 deixou de ser hipotética. Probe sobre as 225 entradas creditadas:

| Corte | Valor |
|---|---|
| Total com `outcome_reward` | 225 |
| Maior família coerente | **128** com prefixo `loop:` (127 creditadas por `loop_phase_close`) |
| Média do reward | **0,919** |
| Negativos (`< 0.5`) | **16 / 225 = 7%** |

O piso de **quantidade** do DSPy (30–300 por alvo) está atingido dentro de uma única família. O que
falta é **variância**: um trainset 93% positivo não produz gradiente — o GEPA otimiza contra uma
métrica que quase não distingue. É a classe de falha já registrada em
`uma-execucao-nao-distingue-constante` (o `predict-action` devolvia 0,990 para todo comando,
inclusive `false`).

A causa é estrutural e não acidental: o phase-close deriva o veredito de `status == "done"`, e
fases raramente fecham como falha — quem fecha a fase é quem acabou de fazê-la funcionar. **Sinal
auto-referencial**, exatamente o que o survey MSR diz que degrada.

**Consequência para o plano:** o pré-requisito do P4 não é "outcome_reward coverage sobe", é
"existe um alvo cujos outcomes VARIAM". Dois candidatos com variância genuína, ambos com
verificador independente do avaliado:

1. **A rota do code mode (P2)** — `touring run` vs token de relaxamento é um desfecho binário
   observável que o modelo produz das duas formas na prática.
2. **Vereditos de gate reprovador** — `cargo`/`clippy`/`loop_converged` reprovando é falha real,
   medida por código; hoje esses vereditos alimentam o retry (W4 S-4.6) e **não** viram caso rotulado.

O item 2 é o candidato mais forte e ainda não está no plano: é reward negativo **determinístico**,
que é justamente a metade que falta ao corpus.

---

## 7. O P2 medido contra a realidade (2026-08-26 00:03) — o gatilho escolhido não dispara

Com o braço no ar, exercitei-o: duas inspeções da mesma classe na MESMA mensagem, que é a receita
da rajada de turno. Resultado nos contadores vivos:

```
t3_turn_first_passed_count = 2      t3_turn_fused_count = 0
```

As duas contaram como **primeira do turno**. O PostToolUse de cada chamada fecha o turno por
construção (`turn_gate_close`), e neste modelo de execução as chamadas do orquestrador são
serializadas — a assinatura que o T3-B persegue (batch paralelo SEM PostToolUse intercalado) não
ocorre aqui. **Um braço preso ao fuse T3 quase nunca receberia recompensa.** Prender a política ao
gatilho mais raro do sistema é o mesmo erro de `afordancia-desligada-quebra-em-silencio`: adoção
zero não prova que a política é ruim, prova que ela nunca foi consultada.

O deny que de fato dispara o tempo todo é o **code-mode comum** (`detect_code_mode`), e ele também
entrega uma rota escrita — mas hoje não registra o braço.

### E, de novo, o mecanismo já existe

Procurando onde enxertar, encontrei o **F2 suggestion-uptake**: `pending_suggestion()` armado por
sessão em `record_emission`, resolvido no PreToolUse seguinte por `eval_uptake` via
`action_is_touring_redirect(tool_name, tool_input)` — literalmente "o modelo seguiu o
redirecionamento?", com contadores `suggestion_emitted` / `suggestion_followed`. É a terceira
ocorrência do MESMO padrão nesta sessão (após `cli_memory_credit` e `credit_recalls`): a medição
existe e **nenhuma política a consome**.

**Consequência de projeto:** o braço não deve pendurar-se no T3. Deve consumir o F2, que já cobre
todo deny/nudge e já está armado por sessão. O que construí (RouteOffer + `claim_route_reward` +
depósito no PostToolUse) é o *transporte* correto e testado; o que muda é **de onde vem a oferta**.

Isso é decisão de projeto sobre o caminho quente dos hooks, e chega junto com a decisão P2b já
aberta — as duas são a mesma pergunta vista de dois lados: **quanto da apresentação passa a ser
política, e a partir de qual sinal.**
