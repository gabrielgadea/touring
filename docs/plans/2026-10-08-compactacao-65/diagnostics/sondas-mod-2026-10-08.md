---
type: Diagnostic
title: "Item 9 — as duas sondas do mod de corte: ambas negativas"
description: "Um mod de session.compact não enxerga o conselho dos hooks e o seu corte é desfeito pelo --resume no CC 2.1.294; o mod de corte não é construído."
plan_id: task_1791479126715571926
timestamp: 2026-10-08T14:50:00-03:00
tags: [compactacao, mods, sonda, claude-code-2.1.294]
---

# Sondas do mod de corte (08/10/2026)

Condição da recomendação 9: construir o mod de corte **só com as duas sondas positivas**. Ambas deram
negativo. O mod-sonda está em `scripts/probe-mod/` e as sessões rodaram em `claude -p` com backend Kimi
(`kimi-cc`, nenhum token da franquia Anthropic).

## (a) O mod enxerga o conselho dos hooks? — NÃO

- Sessão `1fe9a222-451e-4d94-ab6f-adbcfa98ba1e` (Read + Bash) e `/compact` em modo `observe`.
- O `session.compact` recebeu `eventKeys=[trigger, messages]`, 7 mensagens com `role, text, toolUses, handle`;
  os resultados trazem `tool_use_id, text, isError, result` (o conteúdo inteiro do arquivo lido).
- `advice_hits = 0`: nenhum texto com marca de conselho; as mensagens de usuário dos turnos de ferramenta
  chegam com `text` vazio.
- **Controle positivo**: o transcript da mesma sessão tem 3 anexos `hook_additional_context`, um deles com
  "CODE MODE", que a regex da sonda casaria. O conselho mora em anexos, fora de `event.messages`.

## (b) A compactação feita pelo mod sobrevive ao `--resume`? — NÃO (o #89 se reproduz no 2.1.294)

- Sessão `2af574f4-4064-4ff2-a85a-54a27ef5065e`, `/compact` em modo `cut`: o mod devolveu 3 mensagens com a
  primeira prefixada por `PROBE-MARKER-7731`.
- O transcript registra `compact_boundary` manual `preTokens 76.080 → postTokens 501`, `durationMs 6`
  (sem LLM), e a mensagem marcada gravada depois do limite.
- `--resume` + "copie os 60 primeiros caracteres da primeira mensagem de usuário": o modelo devolveu o prompt
  ORIGINAL, sem o marcador, viu o turno completo anterior, e a requisição leu 76.032 tokens de cache, o
  tamanho de antes do corte.
- **Controle**: o limite NATIVO da sessão de (a) traz `preservedSegment` (`has_preserved: true`,
  75.707 → 7.894 tokens, 49 s); o limite do mod não traz nenhuma âncora, e é por elas que o resume encadeia.

## Achados de plataforma registrados no caminho

1. Um módulo de mod importa só arquivos próprios por caminho relativo e `"claude-code"`. `node:fs` é
   recusado, e o `claude -p` **não** reporta a recusa no stderr. Só `claude plugin validate` acusa
   (`cannot import "node:fs"`). A escrita passa por `$.fs.read`/`$.fs.write` (sem append; 4 MiB).
2. `$.session.compact()` não existe em sessão headless (`not available in a headless (-p / SDK) session
   yet`); ali a compactação só roda dentro de um turno cujo prompt é `/compact`. Logo, a opção B3
   (compactar ao fechar subtarefa do DAG por mod) é impossível em headless hoje.

## Decisão

O mod de corte não é construído. A autocompactação aos 65% fica no mecanismo nativo
(`CLAUDE_AUTOCOMPACT_PCT_OVERRIDE=65`), que preserva as âncoras do resume. Reabrir quando uma versão do
Claude Code (a) entregar os anexos de hook ao `session.compact` e (b) gravar `preservedSegment` para uma
compactação feita por mod. As duas sondas deste diretório reproduzem a verificação em 2 comandos.
