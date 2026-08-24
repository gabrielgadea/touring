---
okf_version: "1.0"
type: Strategy
title: "Rodada 3 — o que morre em tempo de execução"
description: "Terceira rodada: telemetria que não sobrevive ao restart, contadores desligados do caminho que dispara, o fan-out do runner inexercitado, e o feromônio que os fluxos consomem sem nunca depositar"
plan_id: 2026-08-20-crates-capability-audit
tags: [adw, loop-engineering, telemetry, aco, rl, fanout, ceg]
timestamp: 2026-08-20T02:55:00-03:00
---

# Rodada 3 — o que morre em tempo de execução

Ligado a [index](./index.md) · [rodada 1](./strategy-2026-08-20-capability-reach.md) · [rodada 2](./strategy-2026-08-20-rodada2-inalcancavel.md)

## O erro que esta rodada quase cometeu

Comecei medindo os contadores do `gate-metrics`: **mais de 100 em zero**. Era uma tabela
pronta de "capacidade morta". Antes de publicar, medi o instrumento:

```
daemon PID 2032653 — uptime 25:31
```

**Vinte e cinco minutos.** Os contadores cobrem essa janela e nada mais. A tabela inteira
teria sido artefato de uptime, não achado. Fica registrado como método, não como nota de
rodapé: *provar o instrumento antes de acusar o sistema*.

O que sobrevive a essa correção é mais forte do que a tabela seria.

## Achado 1 — a telemetria de adoção não pode existir

Não há caminho de persistência. `record_gate_metrics_daily_flush()` está definido em
`gate_metrics_snapshot.rs:1170` e **nada o chama** — não há agendador, não há arquivo em
disco. Os contadores são `AtomicU64` em memória e zeram a cada `daemon-ctl restart`.

A consequência é constitucional. `touring-4-pillars.md` afirma:

> "Adoption is **measured, not assumed**: `pillar_induction_{emitted,followed}` counters →
> `touring.coupling.pillar_induction_ratio` KPI → F7 promote/demote."

O KPI que decide promover ou rebaixar uma indução é calculado sobre estado que morre a
cada restart do daemon. Nesta janela: `pillar_induction_emitted_count: 1`,
`followed: 0`. Não é ruído amostral — é a amostra inteira que existe.

## Achado 2 — dois contadores para o mesmo fenômeno, um alimentado

Nestes 25 minutos o hook `cli-suggest` me injetou advice cerca de **dez vezes**, incluindo
`anti-pattern-bash-edit` explicitamente. Ao mesmo tempo:

```
workflow_antipattern_detected_count : 0
workflow_advice_emitted_count       : 0
adoption_antipattern_count          : 5     ← alimentado
```

A causa é uma assimetria dentro de um único arquivo. `cli_suggester.rs` chama **10
recorders** — `record_adoption_touring`, `record_adoption_antipattern`,
`record_pillar_induction_emitted/followed`, `record_suggestion_emitted/followed`,
`record_enrichment_emitted`, `record_hook_rewrite_applied` — e executa
`detect_antipattern()` / `advise_next_step()` **3 vezes sem gravar nenhuma das duas**.

Os recorders existem e têm chamadores: `touring-ceg/src/gateway/metrics.rs` e o verbo
manual `touring gate`. **O caminho que realmente dispara em toda sessão não é nenhum dos
dois.** É a forma já vista nesta sessão (`definer-module-cinco-sitios`): consertar um
sítio mascara o defeito enquanto o sítio quente segue mudo.

## Achado 3 — o Sandbox-First não acontece

```
ceg_captured_count : 60
ceg_fast_path_count: 60
ceg_sandboxed_count:  0
ceg_blocked_count  :  0
```

Sessenta comandos capturados na janela, **sessenta pelo caminho rápido, zero
sandboxados**. O Reflexo #9 ("rotear execução pelo CEG X0..X9 antes de qualquer run
real") não tem realização observável — o gateway captura e libera.

## Achado 4 — o fan-out do runner é inexercitado

O runner `adw.py` implementa um subsistema de paralelismo com invariantes cuidadosas
(`max_branches` verificado **antes** de qualquer ramo rodar; `template` clonado por valor
em runtime; `on_branch_fail` com `all|any|ignore|best_effort|quorum:N`). Medindo as 9
specs da library **pela chave TOML real** — a primeira passada, por regex solto, reportou
`template` 7/9 e `resume_on_fail` 4/9, ambos falsos:

| recurso do runner | specs que usam |
|---|---:|
| `type = "parallel"` | **0/9** |
| `branches` | **0/9** |
| `template` (fan-out dinâmico) | **0/9** |
| `on_branch_fail` | **0/9** |
| `max_branches` | **0/9** |
| `budget_usd` | **0/9** |
| `resume_on_fail` | **0/9** |
| `[[use]]` (composição) | **0/9** |
| `type = "code"` | 8/9 |
| `type = "agent"` | 5/9 |
| `skill` | 3/9 |
| `type = "gate"` | 4/9 |
| `type = "loop"` | 3/9 |
| `type = "human"` | 2/9 |

Some-se a rodada 2 (0 de 16 fragmentos compostos): **a metade avançada do runner —
paralelismo, orçamento, retomada e composição — não é exercitada por spec algum.**

## Achado 5 — o feromônio é consumido e nunca depositado

O RL **está vivo**. Não é lacuna:

```
update_count 136 · ema_reward 0.859 · linucb_loaded true · arm_count 8
agentic_rl_state: active, política com pesos treinados
```

Mas quem deposita? `touring learning reward` aparece em exatamente dois lugares:
`fragments/phase-close.toml` (fragmento órfão — nenhum spec o compõe) e
`loop_phase_close.py` (o script do loop). **Nenhuma spec da library chama `touring
learning` em forma alguma.**

Um `touring adw run feature` completa e não escreve nada no RL. O feromônio só é
depositado quando um humano fecha uma fase do loop.

## Achado 6 — a anatomia do maior crate não lido

`touring-intelligence` (80k LOC, 460 órfãos — a maior concentração do workspace):

| subsistema | LOC |
|---|---:|
| `rl/` (112 arquivos) | **47.051** |
| `reasoning/` | 16.474 |
| `ann/` | 5.595 |
| `index/` | 2.872 |

Dentro de `rl/`: `memory` 9.570 · **`aco` 7.238** · `bandit` 5.536 · `rl` 5.361 ·
`n1` 3.101 · `n3` 2.718 · `evolution` 1.585 · `ranking` 1.478 · `semantic` 1.341 ·
`meta` 1.104 · `data` 988 · `clustering` 986 · `templates` 836 · `observability` 553 ·
`online_learning` 389.

## Achado 7 — quatro colônias, nenhuma compartilhada

O substrato ACO **está wired em produção** — minha primeira sonda procurou
`PheromoneBus` e voltou `<nenhum>` para um símbolo que não existe; o nome real é
`UnifiedPheromoneBus`. Depósitos reais acontecem (`PheroKey::FilePath`, delta 1.0 em
`aco_wiring.rs:96,120`).

Descontados os testes, há **4 instanciações de produção**:

| sítio | forma |
|---|---|
| `touring-hooks-core/aco_wiring.rs:78` | campo de `AcoWiringState` |
| `touring-server/server/mod.rs:422` | `Arc`, comentado como *"**Shared** ACO pheromone bus"* |
| `touring-server/tools/generator_tools.rs:159` | `Arc<Mutex<…>>` próprio |
| `touring-intelligence/rl/aco/graph.rs:112` | campo de grafo |

**Não existe bus global** — nenhum `static`, `OnceLock` ou `LazyLock`. São quatro
colônias independentes: a trilha depositada pela camada de hooks não é legível pelo
decomposer nem pelo gerador. Se a partição é deliberada ou é o mesmo defeito de
"comentário afirma simetria inexistente", não afirmo sem ler mais — registro como fato
estrutural medido e pergunta aberta.

## Deliverables da rodada 3

| # | entrega | achado | tam |
|---|---|---|---|
| **F1** | Ligar `record_workflow_advice_emitted` / `record_workflow_antipattern_detected` no `cli_suggester` — 3 sítios, com teste que falha se um `detect_antipattern` não contar | 2 | **S** |
| **F2** | Persistir `gate-metrics` em disco (o `daily_flush` já existe sem chamador) + carregar no start | 1 | **M** |
| **F3** | Exercitar o fan-out: 1 spec real com `parallel` + `on_branch_fail` + `max_branches` — hoje o subsistema não tem prova de uso | 4 | **M** |
| **F4** | Compor `phase-close` (ou chamar `learning reward` direto) nas specs de `feature`/`bugfix`/`audit` — fechar o feromônio nos fluxos | 5 | **S** |
| **F5** | Diagnosticar o `ceg_sandboxed_count: 0` — 60 capturas, 60 fast-path: o sandbox é inalcançável ou a política nunca o exige? | 3 | **M** |
| **F6** | Decidir a topologia do `UnifiedPheromoneBus`: um bus por processo, ou documentar as 4 colônias como deliberadas | 7 | **M** |
| **F7** | `budget_usd` em ao menos uma spec — orçamento existe no runner e nenhum fluxo o declara | 4 | **S** |
| **F8** | Ler `touring-intelligence/rl` por dentro (47k LOC, 460 órfãos) — é a maior massa não auditada do workspace | 6 | **L** |

## Sequenciamento

```
F1 ─→ F2          (contar certo antes de persistir; persistir errado é pior)
F4 (independente) · F7 (independente)
F3 ─→ F7          (o fan-out é onde o orçamento importa)
F5 (independente)
F6 ─→ F8          (topologia antes de auditar a massa)
```

Acíclico. F1/F4/F7 são S.

## Riscos

| risco | prob | impacto | mitigação |
|---|---|---|---|
| F2 persistir contadores errados e petrificar o viés (F1 não feito antes) | ALTA | ALTO | a ordem F1→F2 é a mitigação; não invertê-la |
| F3 expor bugs no fan-out nunca exercitado | ALTA | MÉDIO | é o objetivo — código sem uso não tem prova; começar com `quorum:2` em 3 ramos |
| F6 unificar as colônias mudar o comportamento do RL vivo (EMA 0.859) | MÉDIA | ALTO | medir EMA antes/depois por N sessões; reverter é uma linha |
| F5 ligar o sandbox tornar toda sessão lenta | MÉDIA | ALTO | medir latência do X5 antes; manter fast-path como default e sandbox por classe de comando |

## O que esta rodada NÃO mediu

- Se as 4 colônias ACO **deveriam** ser uma — falta ler `aco/registry.rs` e `read_model.rs`.
- `reasoning/` (16.474 LOC) segue sem leitura alguma nas três rodadas.
- Cobertura de mutação — `mutation-test` continua não rodado nas três rodadas.
- Se `ceg_sandboxed_count: 0` decorre de política ou de caminho inalcançável.
