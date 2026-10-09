# Aposentados em 08/10/2026 — compactação aos 65% (ordem de Gabriel)

- `auto_compact.sh` (Stop): lia a flag GLOBAL `~/.claude/context_alert` (compartilhada por todas as
  sessões vivas — a primeira a cruzar o limiar acendia o alerta para todas, e o Stop de qualquer uma o
  consumia) e devolvia `systemMessage` "CRITICAL: ... Voce DEVE executar /compact", que um modelo não
  consegue digitar e que tem a forma TODO-CAPS que a REGRA #20 classifica como injeção. A variável
  `COMPACT_FOCUS` que ele declarava nunca era usada.
- Substituto: `CLAUDE_AUTOCOMPACT_PCT_OVERRIDE=65` (env do `~/.claude/settings.json`) — a compactação
  nativa dispara sozinha aos 65% — mais o aviso único por sessão aos 55% (`status_monitor.sh` arma
  `~/.claude/touring/context_alert/<sessão>.pct`; `check_context.sh` fala 1× por ciclo).
- Plano: `docs/plans/2026-10-08-compactacao-65/`.
