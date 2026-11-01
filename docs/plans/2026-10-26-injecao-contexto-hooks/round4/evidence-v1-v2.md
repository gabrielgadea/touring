---
type: Evidence
title: "Verificações V1 e V2 — o teto de 10.000 chars e o canal de protocolo"
description: "Duas previsões extraídas da doc oficial (Context7) testadas contra o corpus real do Touring."
plan_id: 2026-10-26-injecao-contexto-hooks
okf_version: 1
timestamp: 2026-10-26
---

# V1 e V2 — o que a doc oficial torna testável, testado no corpus

> **Origem das previsões**: scout do Context7 (rodada 4), que varriu 65 docs
> first-party da Anthropic. Duas delas são **verificáveis no transcript**:
> o teto de 10.000 caracteres e o canal de protocolo.
> **Corpus**: 400 sessões (`-touring`, `-analise`, `-konverter`),
> **34.271 blocos** de contexto do Touring, 18.898.797 bytes.
> **Instrumento**: `/tmp/check_10k_and_channel.py` (reprodutível a partir de
> `scripts/measure_injection_effect.py`).

## V1 — O teto de 10.000 caracteres: 1,83% do volume do Touring (NÃO 10,2%)

> ⚠️ **CORREÇÃO (26/10/2026, ainda nesta rodada).** A primeira medição deste
> item deu **10,2%** — e estava **errada**: o instrumento **não filtrava a
> origem**, então contava também os hooks de terceiros. Refeita **com o filtro
> de origem**, a fatia do **Touring** é **1,83%**. O número de 10,2% foi
> **retirado**. A correção foi sugerida pelo scout do corpus qualitativo.

**O que a doc diz** (`docs/context:348`, repetido em 5 docs):

> "If a value exceeds 10,000 characters, Claude Code writes the text to a file …
> Claude can read the file, but **Claude Code doesn't ask it to**."

**Medido, por origem** (só os campos que entram na janela):

| origem | blocos > 10.000 | bytes | % do volume da origem |
|---|---:|---:|---:|
| **Touring** | **18** | 272.790 | **1,83%** de 14.867.530 B |
| terceiros | 0 | 0 | 0,00% de 1.099.652 B |

- **maior payload observado**: 42.708 caracteres (medido no total, sem filtro)
- o excedente maior cai na família **não classificada** (`outros`) e em
  `code-mode-deny` (os 12 blocos são o programa fundido do gate).

**Leitura**: ~1,8% do volume do Touring **não chega ao modelo como contexto
lido** — vira arquivo + prévia, e ninguém é convidado a lê-lo. É real, mas
**menor** do que a primeira medição dizia.

**Consequência de produto**: o truncamento do payload no lado do Touring é
**pior que a ausência** quando o sinal é truncado (o `pre_bash.rs` corta em
9.500 chars e o `post-edit` também) — a mesma conclusão do harness2
(`spec.py:163-167`: *"is in the range that fabricates defects; use >= 10_000"*).
E a medição do **S-7** mostrou o caso extremo: **58,0% das ordens MUST do
`code-mode-contract` chegam sintaticamente inválidas** (ver
`evidence-ordens-inexecutaveis.md`).

## V1b — ⚡ A ORDEM DE LEITURA: a injeção chega DEPOIS da decisão

**Medido**: em **77.794 de 77.794** casos (100,0%), o attachment do hook
`PreToolUse` é gravado no transcript **DEPOIS** do registro `assistant` que
contém o `tool_use`. **Nenhum caso** de a injeção anteceder a tool_use.

| | casos |
|---|---:|
| hook **depois** do tool_use | **77.794** |
| hook **antes** do tool_use | **0** |

⇒ **O `additionalContext` do `PreToolUse` não entra no contexto antes da
decisão que ele queria influenciar — ele entra depois.** O modelo só pode
usá-lo na **próxima** ação.

**Isto é o mecanismo que explica o veredito**, e não uma correlação:

- `touring-suggest` tem **a melhor prosa do corpus** (79,2% dos bytes são uma
  ordem completa, um comando por linha) e **a pior adesão (0,3%)** — porque a
  ordem chega tarde;
- `pre-edit-prevention` — a única família com ação de verdade (98,8%) — tem
  **680 de 700 blocos chegando depois do Edit já aplicado**: é pós-facto, não
  preventivo;
- e o gate de code mode funciona porque **reescreve a chamada** (`updatedInput`
  / `permissionDecision`), ou seja, age **no momento da decisão** — não
  depois.

**Consequência de projeto**: um hook `PreToolUse` que só **informa** chega
depois do ato; para **influenciar** o ato, ele precisa **reescrever ou negar**
(o caminho do gate), ou ser movido para um evento que realmente precede a
decisão (o `UserPromptSubmit`, por exemplo).

## V2 — O canal: o Touring usa **só** `additionalContext`

**Medido** (campo de protocolo efetivamente usado no stdout do hook):

| campo | ocorrências |
|---|---:|
| `hookSpecificOutput.additionalContext` | **28.188** |
| `hookSpecificOutput.systemMessage` | **0** |
| `additionalContext` (raiz) | **0** |
| `systemMessage` (raiz) | **0** |
| stdout cru (não-JSON) | 68.821 |

Por família, **todas as 16 famílias** injetam por `hso.additionalContext`
(`pre-edit` 6.812 · `touring-suggest` 4.749 · `post-edit` 3.130 ·
`evidence-guard` 2.338 · `code-mode-contract` 1.850 · `similar-fail` 1.678 ·
`generic-noise` 1.507 · `post-tool-echo` 1.336 · `code-mode-deny` 897 ·
`plugin-hook` 583 · `pipefail-evidence` 540 · `compact-warning` 282 ·
`pre-edit-prevention` 89 · `session-contract` 72).

### V2.1 Isso **disprova** o risco que o scout levantou

O scout do Context7 registrou uma **contradição real entre duas fontes
first-party** sobre `systemMessage`:

- a referência da API: *"Shows a message to the **user**"*;
- o skill oficial de hook-development: *"**Message for Claude**"* em 6 exemplos.

E levantou o risco: *"se o payload do sermão do Touring vai por `systemMessage`
e não por `additionalContext`, ele pode não estar chegando ao modelo"*.

**O risco está desprovado.** O Touring **nunca** usa `systemMessage` — toda a
injeção vai por `hookSpecificOutput.additionalContext`, que é o campo que a
doc descreve como *"passes a string from your hook into Claude's context
window"* e que a Claude Code **envolve em um system reminder**.

⇒ **A contradição entre as duas fontes da Anthropic permanece sem resolução
(documentada, não resolvida por mim), mas ela é irrelevante para o Touring.**

### V2.2 E dá o **mecanismo** do veredito 52% × 0,3%

A mesma doc (`agent-sdk/modifying-system-prompts:1224`):

> "Treat them as context **from the application**, not as messages from the user."

O conteúdo injetado por hook é, para o modelo, **contexto da aplicação** —
não ordem do usuário, não instrução do operador. Um **fato** nesse canal é
lido como dado disponível; um **sermão** é lido como conselho de uma aplicação
que o modelo não tem obrigação de obedecer. Isso é o **mecanismo** (não a
correlação) do que a medição do corpus encontrou.

## V3 — onde o texto veio: contexto × motivo-do-deny

| família | total | via contexto | via `permissionDecisionReason` |
|---|---:|---:|---:|
| `pre-edit` | 6.812 | 6.812 | 0 |
| `touring-suggest` | 4.749 | 4.749 | 0 |
| `post-edit` | 3.130 | 3.130 | 0 |
| `evidence-guard` | 2.338 | 2.338 | 0 |
| `code-mode-contract` | 1.850 | 1.850 | 0 |
| `similar-fail` | 1.678 | 1.678 | 0 |
| `generic-noise` | 1.507 | 1.507 | 0 |
| `post-tool-echo` | 1.336 | 1.336 | 0 |
| `code-mode-deny` | 999 | 908 | **91** |
| `outros` | 5.227 | 4.542 | **685** |

⇒ O gate de code mode é o único que **também** escreve no motivo do deny
(91 casos) — e é o único que **reescreve a chamada**. Os demais vão 100% pelo
canal de contexto. **A separação "contexto × enforcement" que a §6 do
documento principal supunha é verdadeira, e é mais limpa do que se supunha:**
só o gate usa os dois.

---

_Medido por TACO, 26/10/2026, contra a doc oficial via Context7.
Reprodutível: `/tmp/check_10k_and_channel.py` sobre
`scripts/measure_injection_effect.py`._
