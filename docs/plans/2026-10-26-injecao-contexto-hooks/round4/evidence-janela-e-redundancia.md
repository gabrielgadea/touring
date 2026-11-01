---
type: Evidence
title: "A janela real e a redundância — 12,0% na janela, 69,8% de repetição"
description: "Duas medições que revisam o headline das rodadas 1-4."
plan_id: 2026-10-26-injecao-contexto-hooks
okf_version: 1
timestamp: 2026-10-26
---

# A janela real, e o quanto do que se injeta é repetição

> Duas medições novas da rodada 4, feitas sobre o corpus de **400 sessões**
> (`-touring`, `-analise`, `-konverter`), com leitor de transcript **completo**.
> Instrumentos: `/tmp/measure_window_share2.py`, `/tmp/measure_redundancy.py`
> (a partir de `scripts/measure_injection_effect.py`).

## 1. A janela: o que REALMENTE entra no contexto do modelo

A doc oficial (via Context7) é explícita sobre o mecanismo: em `exit 0`, o
stdout do hook vai para o **transcript**; o que entra na **janela do modelo** é
`hookSpecificOutput.additionalContext` (envolvido em *system reminder*),
`systemMessage` e `permissionDecisionReason`.

| componente | bytes | % |
|---|---:|---:|
| resultados de ferramenta | 53.602.262 | 43,2% |
| chamadas de ferramenta do modelo | 30.644.663 | 24,7% |
| mensagens do usuário | 16.214.848 | 13,1% |
| **injeção do Touring que entra na janela** | **14.858.084** | **12,0%** |
| texto do modelo | 4.438.050 | 3,6% |
| injeção de terceiros que entra na janela | 1.108.353 | 0,9% |
| stdout cru de terceiros (só transcript) | 2.969.938 | 2,4% |
| **stdout cru do Touring (só transcript)** | **695** | **0,0%** |

**Duas consequências**:

1. **O Touring praticamente não emite stdout cru** — 695 bytes em 400 sessões.
   Toda a injeção dele (14,9 MB) vai por `hso.additionalContext`. **A hipótese
   de que o "27,8%" da rodada 1 estivesse inflado por stdout cru do Touring está
   REFUTADA**; o stdout cru é de hooks de terceiros.
2. **O headline divergiu**: 12,9% (só o que entra na janela) a 15,3% (contando
   stdout cru) contra **27,8%** da rodada 1 (que replicou 26,6% de 04/09).
   **Não é uma troca silenciosa** — a reconciliação do denominador da rodada 1
   está pendente e registrada nas pendências.

## 2. Redundância: 69,8% dos bytes injetados são repetição

Excedente ao **conteúdo distinto** de cada família, sobre o total de bytes
da família:

| família | blocos | **payloads distintos** | taxa de repetição |
|---|---:|---:|---:|
| `debug-stdout` | 2.869 | **46** | **99,6%** |
| `compact-warning` | 284 | **11** | **99,5%** |
| `evidence-guard` | 2.356 | 134 | **96,2%** |
| `session-contract` | 72 | **4** | **95,8%** |
| `plugin-hook` | 780 | 473 | 93,4% |
| `similar-fail` | 1.678 | 208 | 92,3% |
| `pipefail-evidence` | 540 | 116 | 90,5% |
| `post-tool-echo` | 1.336 | 732 | 77,2% |
| `code-mode-deny` | 999 | 457 | 72,6% |
| `outros` | 5.230 | 1.988 | 65,7% |
| `pre-edit` | 6.818 | 6.578 | 64,8% |
| `touring-suggest` | 4.755 | 4.102 | 61,2% |
| `code-mode-contract` | 1.851 | 1.370 | 61,4% |

**Total: 13.193.705 de 18.910.157 bytes = 69,8% são repetição.**

### 2.1 Os payloads mais repetidos

| repetições | bytes | payload |
|---:|---:|---|
| **1.577×** | 135 | o aviso DEGRADED do `evidence-guard` |
| **1.405×** | 9 | `[generic]` |
| **1.206×** | 14 | `[gabrielgadea]` |
| **1.854×** (3 variantes) | 402/341/444 | `[EVENT_READER] Reading hook event …` |
| 490× | 1.131 | `[medição sem relação verificada] …` |
| **399×** | 155 | **`[G2 exit-code-through-pipe → REESCRITO]`** — um dos *melhores* blocos do corpus |

### 2.2 O cruzamento com o harness2

O paper do harness2 (arXiv 2610.14224) e o corpus do NotebookLM dizem a mesma
coisa por caminhos diferentes:

- *"**duplicated instruction dilutes the rest**"*
  (`scout-harness2.md`, `prompts.py:864-868`);
- *"**if you add always-on text, cut something else**"* (`prompts.py:254`);
- *"com 100 skills isso são **100 chances de escolher a errada**"*
  (notebook "Claude Code ", fonte [4]) — o dano do acúmulo é **superadditivo**.

**E a infraestrutura já existe no projeto**:
`crates/touring-hooks-shared/src/injection_dedup.rs:271` (`dedup_hook_output`) e
o A/B `crates/touring-hooks-shared/tests/s8_corpus_ab.rs` (16.477 B) — **que
mede o A/B da DEDUPE, não o da política de injeção**.

O dedup do daemon só alcança JSON com `hookSpecificOutput.additionalContext` e
opera **por seção**, chaveado por `root` + `session`.⇒ **Por que 1.577
repetições do `evidence-guard` sobreviveram** é uma pergunta em aberto, e é a
pergunta do experimento de maior valor: **"dedup ligado × desligado"**, e não
"três braços da política".

## 3. A regra operacional, agora com três eixos

1. **carga de fato** — o conteúdo carrega algo que o modelo não tem
   ⇒ funciona (`pre-edit` 75,6%);
2. **decisão** — o hook reescreve **e explica**
   ⇒ funciona por coerção (gate; o `[G2 …→ REESCRITO]` é o melhor bloco do
   corpus);
3. **densidade** — o payload repete?
   ⇒ **69,8% do volume é repetição**, e a redundância dilui o resto.

O "So what?" do Touring (`pre_read.rs:1-7`) cobre **o eixo 1** e **o eixo 3**
(não repetir o que o modelo descobre lendo), mas **não o eixo 2** — e o eixo 2
é o único que o harness2 chama de enforcement (*"Prose can be stopped reading;
a hook cannot"*).

---

_Medido por TACO, 26/10/2026. Reprodutível: `/tmp/measure_window_share2.py` e
`/tmp/measure_redundancy.py`._
