---
type: PreRegistration
title: Gate de integridade da prosa de saída — critério de promoção do SendMessage
description: Escrito e commitado antes de qualquer replay ou dado de sombra. Fixa o sinal, o escape, a regra de rótulo e o critério que promove o SendMessage de sombra para bloqueio. Write/Edit seguem só com aviso.
plan_id: 2026-10-04-integridade-prosa
timestamp: 2026-10-04T22:29:48-03:00
tags: ["#kind:decision", "#domain:hooks", "#process:pre-registration"]
---

# Pré-registro — gate de integridade da prosa de saída

**Decisão de Gabriel (04/10/2026, 22:2x):** "recomendação aprovada" — opção C (sombra primeiro, bloqueio do
`SendMessage` por critério pré-registrado) com a opção D (limpar a corrupção já commitada). Pedido de origem: a
sessão `analise-ef`, 04/10 às 18:53, com o scanner `scan_corrupcao_prosa.py` do projeto analise.

Este documento é commitado **antes** de rodar o replay sobre os transcripts e antes de ligar a sombra. Nada do que
vem abaixo pode mudar depois de olhar os dados; uma mudança vira um pré-registro novo, com data própria.

## 1. Sinal

Um caractere cujo codepoint cai numa destas faixas (as do scanner da analise-ef, com o grego desligado, mais
duas que ele não cobre):

| Faixa | Rótulo | Origem |
|---|---|---|
| 0x3000–0x303F | `cjk_pontuacao` | scanner |
| 0x3040–0x30FF | `kana` | scanner |
| 0x2E80–0x9FFF (resto) | `cjk` | scanner |
| 0xAC00–0xD7AF | `hangul` | scanner |
| 0x0400–0x04FF | `cirilico` | scanner |
| 0x0530–0x058F | `armenio` | scanner |
| 0x0590–0x05FF | `hebraico` | scanner |
| 0x0600–0x06FF | `arabe` | scanner |
| 0x0900–0x097F | `devanagari` | scanner |
| 0x0E00–0x0E7F | `thai` | scanner |
| 0xF900–0xFAFF | `cjk_compat` | acrescentada |
| 0xFF00–0xFFEF | `largura_total` | acrescentada (`，` `：` `（`) |

Grego (0x0370–0x03FF) e tipografia (→ ★ ─ « » ° ≈ ≤ ≥ —) **não** são sinal. Os quatro controles do scanner
(C1 limpo não acusa · C2 dois CJK detectados · C3 tipografia ignorada · C4 grego ignorado) valem para o detector,
como teste.

## 2. Escape

No texto de um `SendMessage`, um caractere do sinal dentro de código inline (`` `…` ``) ou de bloco cercado
(```` ``` ````) é **citação proposital** e não conta. É a rota que a recusa ensina quando o CJK é o assunto da
mensagem. Os escapes são contados à parte, para que um escape usado como atalho apareça.

## 3. O que conta como detecção

- **`SendMessage`:** o campo `message` (string) com ≥ 1 caractere do sinal fora de escape.
- **`Write` / `Edit` / `MultiEdit` / `NotebookEdit`:** só os caracteres do sinal **introduzidos** — os que o texto
  novo tem a mais que o texto antigo (o arquivo em disco, para `Write`; o `old_string`, para `Edit`), por
  contagem de caractere. Editar um arquivo que já tinha CJK legítimo não acusa o que já estava lá.
- Unidade de revisão: **uma chamada de ferramenta** com detecção.

## 4. Rótulo de cada detecção

- **CORRUPT** — o caractere aparece embutido em prosa ou identificador latino, sem contexto que indique intenção
  (`a terceira返回值`, `Para节省 espaço`, `o公共 enum`).
- **LEGIT** — o texto estrangeiro é proposital: dado de teste, frase completa usada como exemplo, citação do
  próprio defeito numa mensagem que fala dele, conteúdo fornecido pelo usuário.
- **UNCLEAR** — não dá para decidir pelo contexto. **Conta como LEGIT** no critério (lado conservador).

Quem rotula registra o trecho (± 40 caracteres) e o motivo, num arquivo commitado que Gabriel pode auditar.

## 5. Evidência

1. **Replay offline**: todos os `tool_use` de `SendMessage`, `Write`, `Edit`, `MultiEdit` e `NotebookEdit` nos
   transcripts de `~/.claude/projects/*` até a data do replay, passados pelo mesmo detector do hook.
2. **Sombra ao vivo**: o hook registra cada detecção em `~/.claude/touring/prose_integrity/events.jsonl`, nunca
   bloqueia, e devolve um aviso nomeando as posições.

As duas fontes entram no critério; o relatório separa as contagens.

## 6. Critério de promoção do `SendMessage` (sombra → bloqueio)

O bloqueio do `SendMessage` (exit 2, recusa que nomeia caractere, codepoint, posição e a rota do escape) liga
quando, **somando replay e sombra**:

- **n ≥ 30** detecções de `SendMessage` revisadas, e
- o **limite superior de Wilson a 95%** da fração LEGIT (com UNCLEAR contando como LEGIT) **≤ 0,20**.

Abaixo de 30, segue em sombra. Acima de 0,20, segue em sombra e o próximo passo é uma regra nova (isenção ou
escape) **com pré-registro novo**, nunca afrouxar este limite.

## 7. Fora do alcance desta fase (declarado)

- `Write`/`Edit` **não bloqueiam** nesta fase, só avisam; a decisão é de Gabriel, com os dados da sombra.
- A **resposta no chat** não passa por ferramenta: nenhum PreToolUse a vê antes de aparecer.
- A **classe lexical** (fragmentos em inglês ou alemão no meio do português) passa em qualquer filtro de
  alfabeto; continua dependendo de releitura.
- Experimento de causa (mesmo prompt, N repetições, 2 checkpoints): gasta API, fora sem autorização.

## 8. Operação

- Modo: `TOURING_PROSE_GATE_MODE=shadow|block` (default `shadow`). Em `block`, só o `SendMessage` bloqueia.
- Kill switch humano: `TOURING_PROSE_GATE_DISABLED=1`.
- Fail-open: payload ilegível ou erro interno nunca bloqueiam nada.
