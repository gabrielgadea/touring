---
okf_version: "1.0"
type: Strategy
title: "Memento rodada 2 — a camada operacional: rótulo, admissão e o balanço do banco de casos"
description: "Segunda análise do Memento, focada no que a primeira não cobriu: o código do cliente CBR, o schema real dos casos e o balanço positivo/negativo. Achado central: o Touring construiu um log de erros e o chamou de banco de casos."
tags: [rl, memento, cbr, case-bank, swamping, langmem, admission-criterion]
timestamp: "2026-08-04T21:55:00-03:00"
plan_id: 2026-08-04-memento-rl-insights
---

# Rodada 2 — a camada operacional

> A rodada 1 leu o **formalismo** (M-MDP, soft Q-learning, Eqs. 1-16) e corrigiu a
> dinâmica de RL. Esta rodada leu o **código do cliente** e os **dados reais** —
> e é ali que estava o mecanismo que faltava.

## 1. O que a rodada 1 não viu

`client/no_parametric_cbr.py::build_prompt_from_cases` (linhas 131-172). Os casos
recuperados **não** são entregues como uma lista ordenada. São **particionados e
rotulados**:

```
Positive Examples (reward=1) - Showing N of M:
Example 1: Question … Plan …

Negative Examples (reward=0) - Showing N of M:
Example 1: Question … Plan …

Based on the above examples, please provide a plan for the current task.
Focus on the positive examples and avoid the patterns shown in negative examples.
```

Com caps **por classe** (`MEMORY_MAX_POS_EXAMPLES` / `MEMORY_MAX_NEG_EXAMPLES`),
nunca um orçamento único.

**A diferença é categórica, não de grau.** Reordenar entrega ao consumidor uma
lista sem semântica; rotular entrega instruções de uso por metade. Uma falha na
posição 15 de um ranking é indistinguível de um sucesso medíocre; a mesma falha
sob um cabeçalho "Negative Examples — avoid these patterns" é evidência
acionável.

Isso reenquadra o que a Fase 4 (rodada 1) entregou: o `rerank_by_case_value` é
**necessário mas insuficiente**. Ordenar 99 % de falhas ainda entrega 99 % de
falhas, só que ordenadas.

## 2. O schema real do caso

`memory/dummy_memo.jsonl`:

```json
{"question": "...", "plan": "{\"plan\":[{\"id\":1,\"description\":\"...\"}]}", "reward": 0}
```

O `(s, a, r)` literal: **s** = a pergunta, **a** = o plano, **r** ∈ {0,1}. O
banco embarcado (`memory/memory.jsonl`) usa a variante rotulada
`{case, plan, case_label: "positive"|"negative"}`.

O `r` vem de um **LLM-as-judge** (`client/…:565`):

```python
judge_res = await llm_judge(q, gt, pred_answer)
reward = 1 if judge_res["judgement"] == "correct" else 0
```

> **Aqui o Touring está à frente.** `loop_converged.py` sai 0/1 de forma
> determinística; `cargo test`/`clippy` também. O Touring tem um sinal de reward
> mais forte que o do paper — e não o conecta à escrita de casos.

## 3. O achado que reordena tudo: o balanço

| | Memento (1.278 casos) | Touring (3.478 casos) |
|---|---|---|
| **positivos** | 897 — **70,2 %** | 29 — **0,8 %** |
| **negativos** | 381 — 29,8 % | 3.448 — **99,1 %** |

Não é diferença de grau: é **inversão**.

E cruzando com o Context7 (`/langchain-ai/langmem`), o consenso de mercado:

| Sistema | Admissão na escrita | Formato na leitura |
|---|---|---|
| **LangMem** | só sucessos — *"Store this interaction if successful"*; instrução do extrator: *"Extract examples of successful interactions"* | campos rotulados `When / Thought / Did / Result` |
| **Memento** | julgado; 1 caso por **tarefa**; só o **passo final** (§5.3) | `Positive` / `Negative` + instrução de uso |
| **Touring** | qualquer erro de tool, sem juiz | lista JSON plana |

Só-sucessos funciona (LangMem). Ambos-rotulados funciona melhor (Memento, +4,7 a
+9,6 pts OOD). **Só-falhas-sem-rótulo é a única combinação que nenhum dos dois
valida** — e é a do Touring.

## 4. Diagnóstico: um log de erros chamado banco de casos

`transcript_miner.rs:517-522`:

```rust
fn lesson_memory_key(pair: &ErrorResolutionPair) -> String {
    …
    format!("outcome:{tool_class}:transcript-{hash}:failure")   // ← literal
}
```

A função recebe um `ErrorResolutionPair` e codifica `:failure` **na string**.
Não existe caminho de produção que minere sucesso — as únicas referências a
`:success` em produção são o código da Fase 4 (rodada 1). O banco é, por
construção, um log de erros.

`touring-ceg/src/gateway/learn.rs:454-484` mostra a assimetria em uma linha:

```rust
let outcome = run_gateway(tool, code_body, None, &deps).ok()?;
…
if !result.is_allowed {           // ← só o ramo da falha escreve caso
```

O gate **observa** os dois resultados e descarta metade da informação.

### Não é swamping por duplicação — medido

Hipótese testada e **refutada pelos dados**: 3.478 casos, **3.478 valores
distintos** — zero duplicação literal. O swamping é por **redundância semântica
com utilidade nula**: `"unknown certificate verification error"`,
`"read ECONNRESET"`, `"maxContentLength size exceeded"`. Cada um é único o
bastante para sobreviver a dedup e inútil o bastante para ser ruído.

Distribuição: `bash` 2.399 (69 %) · `edit` 769 (22 %) · `read` 147 · demais < 60.

É o **utility problem** de Francis & Ram (1993), citado pelo próprio Memento em
§2.3 — *"most systems keep adding cases without selective curation, leading to
the classic swamping problem where retrieval costs outweigh utility"*. As três
defesas do Memento: 1 caso por tarefa, só o passo final, K pequeno. O Touring
não tem nenhuma das três.

## 5. O que foi implementado nesta rodada

### R2-A — partição rotulada com caps por classe

`partition_cases()` em `cli/memory.rs`. `memory recall` passa a devolver, ao
lado do `entries` de sempre, um canal novo:

```json
"cases": {
  "positive": [...], "negative": [...], "unobserved": [...],
  "guidance": "Reuse the approach in `positive` … Treat `negative` as patterns
               to avoid, never as guidance. `unobserved` carries no verdict …",
  "cap_per_class": 4
}
```

Três decisões e o porquê de cada uma:

1. **Construído do conjunto NÃO-filtrado.** O filtro de prefixo de 02/08
   permanece intacto sobre `entries` — o default não muda. As falhas reaparecem
   apenas neste canal novo, **rotuladas e limitadas a 4**. O filtro existia
   porque falhas sem rótulo são indistinguíveis de orientação; rotuladas e
   capadas, o motivo do filtro deixa de se aplicar sem que eu precise reverter
   uma decisão medida sua.
2. **Cap por classe, não compartilhado.** Com 99:1, um top-K único garante
   **matematicamente** que a classe minoritária não recebe slot algum. O teste
   `a_flood_of_negatives_cannot_crowd_out_the_positives` afoga 1 positivo em 200
   negativos e exige que ele sobreviva.
3. **`unobserved` é classe própria.** É onde vivem as lições curadas, que nunca
   foram pontuadas — dobrá-las em qualquer um dos lados seria mentir sobre elas.

`MAX_CASES_PER_CLASS = 4` vem da Tabela 3 do paper (pico em K=4; K=8/16/32 são
piores).

### R2-B — o `r` deixa de depender da convenção de chave

`learn.rs` passa a gravar `"reward": 0.0` + `"outcome_context"` explícitos. A
chave continua carregando `:failure` e `case_value` continua sabendo derivá-lo,
mas o campo que a ordenação inteira usa não fica mais refém de um rename de
chave.

**Gates**: `cargo check --workspace --all-targets` exit 0 · `clippy -D warnings`
0 erros · testes de `touring-cli`/`touring-ceg`/`touring-intelligence` 0 falhas ·
6 testes novos.

## 6. O que NÃO foi implementado — e por quê

**R2-C — minerar sucessos (a restrição que amarra tudo).**

Enquanto o banco tiver 0,8 % de positivos, a classe `positive` da partição vem
quase sempre vazia. Toda a maquinaria das Fases 3-4 + R2-A só rende de verdade
quando existirem casos positivos.

Parei antes de implementar por uma razão específica, não por escopo: **o critério
de admissão é o problema difícil, e errá-lo troca um swamping por um maior.**
Escrever um caso a cada execução permitida inundaria o banco muito mais rápido
que as falhas (toda invocação de bash bem-sucedida). O critério certo é o do
Memento §5.3 — *informatividade*, não volume.

O critério que eu proporia: **gravar o sucesso apenas quando ele resolve uma
falha conhecida da mesma assinatura** — o par erro→resolução que o miner já
modela. Fica limitado pelo número de falhas distintas (≤ 3.448), não pelo volume
de execução, e produz exatamente a evidência pareada que a partição rotulada
quer: *"este padrão falhou → este funcionou"*.

**Bloqueio concreto**: o contrato do CEG (`touring-contracts/src/lib.rs`) expõe
`memory_store` e `learning_reward`, mas **nenhum método de recall**. Decidir "esta
assinatura já falhou antes?" exige adicionar um lookup ao contrato — mudança de
interface entre crates, que merece sua aprovação em vez da minha suposição.

**Alternativa mais barata**: `loop_phase_close.py` já roda `memory store` no
fecho de fase e já tem o veredito de `loop_converged.py` em mãos. Uma linha
(`--reward 1.0` quando o gate passa) começa a popular a classe positiva com o
sinal mais forte que o Touring tem. O arquivo vive em `~/.claude/skills/` — fora
do workspace, no seu ambiente.

## 7. Como medir se isto valeu

| Hipótese | Métrica | O que a falsifica |
|---|---|---|
| Rótulo > ordenação | taxa de reuso de caso `positive` vs `entries` | consumidores ignoram `cases` e seguem lendo `entries` |
| Cap por classe protege a minoria | nº de positivos servidos por recall | continua 0 — significa que o gargalo é R2-C, não o cap |
| Falhas rotuladas viram úteis | citações de `negative` em decisões | nenhuma — o rótulo não bastou, o conteúdo é que é ruim |

A terceira é a mais provável de falhar: `"read ECONNRESET"` é ruído mesmo bem
rotulado. Isso apontaria para **admissão na escrita** (R2-C) como a alavanca
real, e não para mais refinamento na leitura.

---

Antecessora: `/strategy-2026-08-04-memento-rl.md` (rodada 1 — formalismo e
dinâmica). Log: `/log.md`.
