# REGRA #19 — Touring Process Hygiene

<!-- Sem frontmatter e sem `paths:` de propósito: regra de alcance global, sempre carregada. O `---` de abertura sem
     fechamento cortava o início desta regra no carregamento (L4-004 da análise crítica, 18/10/2026). -->


**JAMAIS** `pkill`/`kill` em processo cujo cmdline contenha "touring". Os quatro tipos:

| Tipo | Papel |
|---|---|
| `touring-daemon` | backend singleton (singleton/usuário) |
| `touring-mcp` | ponte stdio↔socket de **uma** sessão CC |
| `touring-hook` | handler efêmero de hook |
| `touring-cli` | cliente CLI efêmero |

**Substituto único:** `touring daemon-ctl status|restart|stop` · `update-touring`.
Identificar antes: `cat /run/user/$UID/touring-daemon.pid` · `lsof` no socket.

**Bypass:** só humano, com `TOURING_PROCESS_GUARD_DISABLED=1`. Hook dono:
`~/.claude/hooks/touring-process-guard.sh`.

> Corpo canônico: `~/.claude/skills/Touring/references/touring-process-hygiene.md`