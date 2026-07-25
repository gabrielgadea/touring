# Touring Rebuild Protocol (pointer)

> **Auto-load stub** (migrado 19/07/2026, /doctor) | **Canonical body**: `~/.claude/skills/Touring/references/touring-rebuild-rule.md` (v3)

SEMPRE `update-touring` para rebuilds — NUNCA `cargo build` standalone (pipeline KILL→CLEANUP→BUILD→INSTALL dual-target `~/.local/bin/` **E** `~/.claude/hooks/`→RESTART→VERIFY; exit 4 = daemon "(deleted)" ou health fail). Após rebuild o daemon antigo segura o inode velho — `update-touring --verify-only` detecta. Kill por nome PROIBIDO (REGRA #19) — usar `touring daemon-ctl status|restart|stop`. Flags, REGRAs #1-#8, rollback e diagnóstico de drift: canonical body acima.
