---
okf_version: "1.0"
type: Strategy
title: "Rodada 4 — o validador da validação está morto"
description: "Quarta rodada: topologia de testes, a cadeia causal que mata o mutation-test, o sandbox como política, o subsistema reasoning e a superfície MCP escondida"
plan_id: 2026-08-20-crates-capability-audit
tags: [adw, loop-engineering, mutation-testing, ceg, reasoning, mcp, got]
timestamp: 2026-08-20T03:05:00-03:00
---

# Rodada 4 — o validador da validação está morto

Ligado a [index](./index.md) · [r1](./strategy-2026-08-20-capability-reach.md) · [r2](./strategy-2026-08-20-rodada2-inalcancavel.md) · [r3](./strategy-2026-08-20-rodada3-runtime.md)

Esta rodada ataca só o que eu mesmo declarei não-medido nas três anteriores — incluindo o
`mutation-test`, adiado por "caro" **três vezes**. Adiar quatro vezes vira política.

## Duas correções ao meu próprio trabalho

1. **`crates/hooks/` não é um crate.** Diretório vazio, sem `Cargo.toml`. A tabela de
   "42 crates" da rodada 1 veio de `ls` e incluía `hooks`, `.claude` e `.touring-cache` —
   estado, não código. Membros reais do workspace: **42 declarados no `Cargo.toml`**, o
   que coincide por acaso.
2. **Os crates-stub são legítimos.** `touring-python` (10 LOC), `touring-web` (33),
   `touring-web-server` (23) são *shims transparentes* que reexportam
   `touring-bindings`. Suspeitei que fossem casca vazia porque as 8 features do
   `touring-bindings` estão OFF por default — mas **o shim liga a própria feature**
   (`features = ['bind-web']` na dependência). Isso também valida o resolvedor de fecho
   da rodada 2, que contabiliza features declaradas por dependente.

## A topologia de testes é forte

| medida | valor |
|---|---:|
| LOC em `src/` | 599.969 |
| testes unitários | 13.352 |
| testes de integração | 2.598 |
| **total** | **15.950** |
| densidade | **1 teste por 37 LOC** |

Crates com zero teste: apenas os quatro stubs. Menor densidade entre os grandes:
`touring-cli` 1/92 · `touring-bindings` 1/76 · `touring-storage` 1/56.

Quantidade, porém, não é eficácia. Quem responde isso é o teste de mutação.

## O achado central — a cadeia causal completa

`touring mutation-test` está morto, e a causa não é uma:

```
1. iai-callgrind-runner NÃO está instalado no ambiente
        ↓
2. `cargo nextest list` falha ao listar o alvo
   touring-analysis::bench/quality_metrics_iai  →  "creating test list failed"
        ↓
3. baseline do cargo-mutants: Failure(104), total_mutants = 0
   (corrida real de 18/08, elapsed 1160s — 19 minutos gastos para produzir nada)
        ↓
4. o CLI reporta: {"ok": true, "kill_rate": 0.0, "mutants_total": 0}
        ↓
5. `repo-score` consome mutants_total → a categoria "testing" é pontuada por esse zero
```

E há um quinto defeito independente: **o handler `cli-mutation-test` tem orçamento de
15 segundos** no daemon. Uma corrida honesta leva de minutos a horas. Toda invocação via
CLI morre com *"exceeded its 15s budget"*. Mesmo `--cache-only --package X` bate no
orçamento. É a mesma forma do gate mais lento que o próprio timeout, agora em Rust.

**Existe um artefato bom que ninguém lê.** `mutants.out/outcomes.json` na raiz do
repositório, de 15/04/2026, tem **21 outcomes reais: 14 mortos, 4 sobreviventes, 2
inviáveis** — kill rate ~78%. O leitor procura em
`target/mutants/<pacote>/mutants.out/outcomes.json` e encontra lá o arquivo de 18/08 com
um único outcome `Failure`.

> **Correção (G4, 20/08)**: esta seção afirmava que "nenhum dos caminhos verifica
> idade". **Falso.** `cache_load` aplica `CACHE_STALE_SECS` (7 dias) e devolve
> `Ok(None)` para entradas vencidas — a verificação existe desde que a função foi
> escrita. O que era verdade: **nada a testava**, então a guarda podia ser apagada numa
> edição com todas as suítes verdes. Corrigido em G4 com um teste provado por mutação.
> O artefato de abril na raiz era órfão de verdade (o leitor nunca olha lá) e foi
> removido.

Os 4 sobreviventes de abril, para o registro — todos em um arquivo só:

```
touring-generator/src/core/context.rs:68   NoopFuzzyMatcher::top_k        → vec![]
touring-generator/src/core/context.rs:127  BkTreeFuzzyAdapter::load_from_cli → vec![]
touring-generator/src/core/context.rs:133  match guard o.status.success() → true
touring-generator/src/core/context.rs:133  match guard o.status.success() → false
```

O terceiro e o quarto são o par que importa: **trocar o guard por `true` E por `false`
passa nos testes** — aquele ramo não é testado em direção nenhuma.

## O sandbox do CEG é política, não impossibilidade

A pergunta aberta da rodada 3 tem resposta. `gateway/fast_path.rs`:

```rust
pub fn fast_path_decision(raw: &RawInvocation) -> FastPathDecision {
    let classification = Classification::derive(raw);
    if is_provably_pure(&body.source, body.language) { /* → SkipSandbox */ }
```

`ceg_sandboxed_count: 0` decorre de `is_provably_pure` ter respondido **verdadeiro nas 60
capturas** da janela. Nessa mesma janela eu executei heredocs que escrevem arquivo e
`sed -i`. Ou o classificador de pureza é permissivo demais, ou a captura não viu o que
rodei. Não resolvo por inspeção — vira medição (G5).

O gateway tem 35 módulos, vários com nome de capacidade que nenhuma rodada alcançou:
`harness_metric.rs` · `change_contract.rs` · `outcome_learner.rs` · `drift_corrector.rs` ·
`speculative.rs` · `supervised.rs` · `selective_checkpoint.rs` · `offensive_integration.rs`.

## `reasoning/` — 16.474 LOC, o último grande não lido

37 arquivos. Os maiores:

| arquivo | LOC | o que é |
|---|---:|---|
| `got.rs` | **1.975** | **Graph of Thoughts** — topologia de raciocínio além de CoT/ToT |
| `mcts.rs` | 1.872 | MCTS base |
| `semantic_graph.rs` | 1.400 | grafo semântico |
| `bridge.rs` · `cognitive_mcts.rs` · `focus_cache.rs` · `session_predictor.rs` | 724·572·544·534 | pontes e caches |

Mais `gated_mcts.rs`, `mcts_streaming.rs` (quatro variantes de MCTS ao todo),
`coedit_predictor.rs`, `consistency_gate.rs` (a engine do comando `consistency`),
`tool_planning.rs` (a do `plan-chain`), `predictive_focus_cache.rs`,
`agent_state_machine.rs`, `adaptive_engine.rs`, `refinement.rs`.

**Resultado negativo importante**: `GotEngine` e `GotNode` alcançam
`touring-hook-handlers` e `touring-hooks-shared` — Graph of Thoughts **está wired**, não
é órfão. E `got.rs` traz `GotPheromoneMemory`: uma **quinta** memória de feromônio, além
das quatro colônias da rodada 3.

## A superfície MCP está escondida, não ausente

Teste decisivo, por execução: `touring_assist_list_kinds` **não aparece no `tools/list`**
(23 ferramentas) e respondeu plenamente a um `tools/call`, devolvendo os 10 assist kinds
reais — incluindo `auto_wire`.

O fonte do servidor tem **167** literais `touring_*`; 23 estão no handshake; os 144
restantes incluem nomes de handler reais (`touring_ast_grep`, `touring_ctx_batch_execute`,
`touring_activity_replay`, `touring_cluster_skills`). **Quantos exatamente são invocáveis
não está estabelecido** — provei que ao menos um é. A consequência independe do número
exato: um cliente MCP que descobre por `tools/list` enxerga 23; o servidor atende mais.
É lacuna de **descoberta**, não de capacidade.


## Adendo — a corrida real, e dois erros meus no caminho

O `--force` sobreviveu ao cliente (o ator seguiu como filho do daemon) e **ainda está
rodando**. Registro o estado honesto e os dois erros, porque ambos são do tipo que esta
auditoria existe para caçar.

**Erro 1 — li exaustão de laço como término.** Meu laço de espera fez 40 × 15 s e caiu
por esgotamento; o script então imprimiu "resultado real". Reportei **19 mutantes /
85,7%** como final. Era um instantâneo de **17%**: o lote tem **135 mutantes planejados**.
É a lição `ausencia-de-sinal-tem-duas-causas` de novo — parar de observar não é o mesmo
que ter terminado.

**Erro 2 — fórmula ad-hoc contradizendo o contrato do tool.** Meu shell calculou
`caught/(caught+missed)` e imprimiu 60,0%. A definição do próprio tool
(`mutation_test.rs:174`) é `(killed + timeout) / (killed + timeout + survived)`, com
timeouts contando **como mortos** e inviáveis fora do denominador. As duas fórmulas dão
conclusões opostas sobre o mesmo dado.

Parcial em 25 min (23 de 135): 4 caught · 2 missed · 12 timeout · 5 unviable.

O que já se sustenta, independente do total final: **os timeouts dominam** (12 de 18
viáveis até aqui). Pela convenção do tool eles contam como kills — então o limiar de 80%
pode ser cumprido majoritariamente por **travamentos**, não por asserções. Sete dos
primeiros nove estão em `define_batch` / `entity_row` / `resolve`: mutações que tornam a
consulta SQL ilimitada, numa suíte sem guarda de tempo própria.

### Os sobreviventes até agora — ambos em `IdentityRegistry::resolve`

```
registry.rs:253:59  replace || with &&   em IdentityRegistry::resolve
registry.rs:266:30  replace >  with >=   em IdentityRegistry::resolve
```

1. **`:253`** decide `same_crate`, que só escolhe entre `confidence = 0.98` e `0.95`.
   Ambos os ramos produzem candidato; nenhum teste afirma a confiança exata, então o ramo
   que **ordena candidatos** não tem cobertura.
2. **`:266`** é o portão do Tier 3 (fuzzy): `max_edit_distance > 0`. Com `>=`, o caso
   `== 0` entraria no fuzzy — e nada reprova isso. As mutações irmãs da mesma linha
   (`> → ==`, `> → <`) estão entre os timeouts. A fronteira não tem cobertura em direção
   nenhuma.

`IdentityRegistry::resolve` é o ponto de entrada da **REGRA #17** (Entity Identity
Determinism), que exige resolução determinística e total. O ramo de confiança e a
fronteira do fuzzy são exatamente onde o determinismo se decide.

### O achado que a corrida produziu sem querer — fome do ator de projeto

Enquanto o job roda, medido por execução:

| handler | estado |
|---|---|
| `touring doctor -j` | **OK — reporta 6/6 saudável** |
| `touring status -j` | estoura 15 s |
| `touring index status` | estoura 15 s |
| `touring memory recall` | estoura 15 s |
| `touring e2e -j` | estoura 15 s |

Não é o daemon inteiro — a CPU está em 4,5%. É o **ator de projeto**, serializado atrás de
um job de 19+ minutos despachado para um handler de orçamento 15 s. Um único
`touring mutation-test` derruba `status`, `index`, `memory` e `e2e` para toda sessão
naquele projeto.

**E o gate de saúde não enxerga.** `doctor` responde 6/6 OK durante a fome inteira. Os
handlers mortos são precisamente os que `loop_diagnose.py` e `loop_converged.py`
consomem: um loop que rodasse agora leria "saudável" e depois falharia a cada cláusula
por timeout — sem nunca saber por quê.

Isto reordena a rodada: **G3 deixa de ser conforto de UX e vira contenção de dano.**

**G7 reescrito**: cobrir `IdentityRegistry::resolve` — asserção sobre `confidence`
(0.98 vs 0.95) e sobre `max_edit_distance == 0` não acionar o Tier 3.

**G9 (novo, S)**: guarda de tempo na suíte de `touring-identity`.

**G10 (novo, M)**: `doctor` deve sondar a responsividade do ator de projeto, não só a
existência do socket — hoje ele aprova um daemon cujo trabalho útil está parado.

## Deliverables da rodada 4

| # | entrega | achado | tam |
|---|---|---|---|
| **G1** | Instalar `iai-callgrind-runner` **ou** excluir os alvos `bench` da lista do nextest usada pelo cargo-mutants — destrava toda a cadeia | central | **S** |
| **G2** | `mutants_total == 0` deve ser **erro**, nunca `ok: true`; e `repo-score` deve marcar a categoria como indisponível em vez de pontuar zero | central | **S** |
| **G3** | Orçamento do handler `cli-mutation-test`: 15s → job assíncrono (`touring jobs`) com poll, como o `index rebuild` | central | **M** |
| **G4** | Verificação de idade no leitor de cache + ler também o artefato da raiz, ou apagá-lo | central | **S** |
| **G5** | Medir `is_provably_pure` contra um corpus de comandos comprovadamente impuros (heredoc que escreve, `sed -i`, `cargo build`) — 60/60 fast-path é hipótese, não resultado | sandbox | **M** |
| **G6** | Expor no `tools/list` os handlers invocáveis, ou documentar a lista oculta — hoje a descoberta mente por omissão | MCP | **M** |
| **G7** | Testar o ramo `o.status.success()` de `BkTreeFuzzyAdapter::load_from_cli` — 2 mutantes sobreviveram nas duas direções | central | **S** |
| **G8** | Remover `crates/touring-web/test_{suspense,resource}.rs` — fora de qualquer target, nunca compilados (REGRA #0) | debris | **S** |

## Sequenciamento

```
G1 ─→ G2 ─→ G3 ─→ G4     (destravar → falhar honesto → tornar viável → não mentir por cache)
G1 ─→ G7                  (só dá para matar o mutante depois que a corrida roda)
G5 · G6 · G8 (independentes)
```

Acíclico. Seis das oito são S, e **G1 é provavelmente um comando**.

## Riscos

| risco | prob | impacto | mitigação |
|---|---|---|---|
| G1 destravar o baseline e revelar kill rate muito abaixo de 80% | ALTA | BAIXO | é o objetivo; a nota honesta baixa vale mais que o zero atual travestido de `ok` |
| G3 tornar o mutation-test viável e ele consumir horas de CI | MÉDIA | MÉDIO | escopar por pacote tocado, não workspace inteiro |
| G6 inchar o handshake MCP (foi trimado de propósito) | MÉDIA | BAIXO | expor um `tools/list` completo sob flag, mantendo o curado como default |
| G5 confirmar o classificador permissivo e obrigar sandbox amplo | MÉDIA | ALTO | medir latência do X5 antes de mudar política |

## O que as quatro rodadas NÃO mediram

- Se os 46 inferlets do pool produzem resultado **correto** (existem ≠ funcionam) — aberto desde a r2.
- Se as 4+1 memórias de feromônio deveriam ser uma.
- `touring-code`, `touring-dispatch`, `touring-bindings`, `touring-storage` — grandes e nunca abertos por dentro.
- O número exato de tools MCP invocáveis fora do `tools/list`.
