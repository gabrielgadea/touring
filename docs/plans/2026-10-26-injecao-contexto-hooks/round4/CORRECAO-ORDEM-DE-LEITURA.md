---
type: Correction
title: "C13 — a ordem de leitura do PreToolUse: o que o transcript mede, e o que o protocolo faz"
description: "Correção da interpretação do achado da rodada 4 sobre a ordem de leitura do PreToolUse."
plan_id: 2026-10-26-injecao-contexto-hooks
okf_version: 1
timestamp: 2026-10-26
---

# C13 — a ordem de leitura do PreToolUse: o que o transcript mede, e o que o protocolo faz

> **Origem**: Gabriel pediu (26/10/2026) para resolver o achado:
> *"o `additionalContext` do `PreToolUse` é gravado depois do `tool_use` que o
> modelo já emitiu — zero casos antes."* A resolução exige separar **o que o
> transcript mede** de **o que o protocolo faz**.

## 1. O que o transcript mede (FATO [1.0])

A medição da rodada 4 mede a **ordem de gravação** dos registros do transcript:
o registro `attachment` (o hook) vem **depois** do registro `assistant` (o
`tool_use`). Isso é verdade — e é o que se espera de um log que escreve na ordem
em que os eventos completam.

## 2. O que o protocolo faz (FATO [1.0])

O `PreToolUse` é o evento que **antecede** a execução da ferramenta. O modelo
já decidiu o que chamar (o `tool_use` está no transcript), mas a ferramenta
**não executou**. O hook pode:

- **permitir** (default);
- **negar** (`permissionDecision: deny` — a chamada não executa);
- **reescrever** (`updatedInput` — a chamada executa com o input novo).

O `additionalContext` é o canal de **contexto** — ele vai para o modelo na
próxima chamada, envolvido em *system reminder* (doc oficial,
`agent-sdk/modifying-system-prompts:1224`). Ele **não** pode mudar a chamada
que já foi emitida — mas pode **informar** a próxima.

## 3. O que isso significa para o achado

| leitura | veredito |
|---|---|
| ❌ "a injeção chega tarde demais para influenciar a decisão que ela pretendia orientar" | **precisa de precisão** — o `additionalContext` **não** pode mudar a chamada atual (o protocolo não permite); mas **pode** informar a próxima. O veredito correto é: **o `additionalContext` é o canal do "depois"; o `permissionDecision`/`updatedInput` é o canal do "agora"** |
| ✅ o gate funciona porque reescreve | **verdade** — o gate age **no momento da decisão** porque usa `permissionDecision`/`updatedInput`, não `additionalContext` |
| ✅ o sermão não muda a chamada atual | **verdade** — e o corpus mede isso (0,3% de adesão proximal) |
| ✅ o fato é usado **depois** | **verdade** — o corpus mede 75,6% de conformidade **distal** (o modelo usa o fato mais tarde, na próxima ação) |

## 4. O que a rodada 4 mediu de verdade

A rodada 4 mediu **a ordem de gravação** do transcript — e a interpretou como
**a ordem de percepção** do modelo. A correção é: a ordem de gravação é a ordem
de **log**; a ordem de percepção é a ordem de **chamada** — e o hook roda
**antes** da ferramenta executar, mas **depois** do modelo decidir chamar.

**O que isso muda**: nada no veredito (fato 52% / sermão 0,3% / alerta 0% está
medido e é real); mas **o mecanismo** é mais preciso: o `additionalContext` é o
canal do "depois" (informa a próxima ação), e o `permissionDecision`/`updatedInput`
é o canal do "agora" (muda a ação atual).

## 5. O que a rodada 4 recomenda (corrigido)

1. **Para informar** (a próxima ação): use `additionalContext` — é o canal certo.
2. **Para mudar a ação atual**: use `permissionDecision`/`updatedInput` — é o canal
   do gate.
3. **Para medir o efeito**: a ordem de gravação do transcript não é a medida —
   a medida é o comportamento do modelo **depois** do hook.

---

_Correção por TACO, 26/10/2026, a pedido de Gabriel Gadea.
O achado original está em `round4/evidence-v1-v2.md` (seção V1b)._