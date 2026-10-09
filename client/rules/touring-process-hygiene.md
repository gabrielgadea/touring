---
# REGRA #19 — Touring Process Hygiene
# (sem `paths:` de proposito: regra de alcance global, sempre carregada)

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