# REGRA #19 — Touring Process Hygiene (CONSTITUTIONAL)

> **Auto-load** (constitutional operational rule) | **Version**: v1.0 | **Date**: 2026-05-23
> **Authority**: Gabriel Gadea
> **Origin**: Sessão 2026-05-23 — múltiplas sessões CC concorrentes resultaram em
> cenário onde LLM via `touring doctor` reportando degraded (spurious race no SessionStart),
> rodava `pgrep -f touring`, via N PIDs indistinguíveis e considerava `pkill -9 -f touring`,
> matando colateralmente MCP bridges de OUTRAS sessões CC + handlers em execução
> → cascading degradation.
>
> **Reforço runtime**: hook `~/.claude/hooks/touring-process-guard.sh` (PreToolUse Bash matcher,
> bloqueia padrões `pkill|killall.*touring`). Hook standalone dedicado à REGRA #19 (2026-07-02).

---

## Princípio operacional (NÃO-NEGOCIÁVEL)

A LLM **JAMAIS** executa `kill`, `pkill`, `killall` ou variantes em processos cujo
cmdline contém "touring" SEM antes:

1. **Ler PID file canônico**: `/run/user/$UID/touring-daemon.pid` (preferencial)
   ou `/tmp/touring-daemon-1000.pid` (fallback transição)
2. **Verificar tipo** via `/proc/<pid>/comm` ∈ `{touring-daemon, touring-mcp, touring-hook, touring-cli}`
3. **Distinguir** o que está prestes a matar:
   - `touring-daemon` → o singleton backend
   - `touring-mcp` → MCP bridge stdio↔socket DE UMA SESSÃO CC ESPECÍFICA
   - `touring-hook` → hook handler efêmero em execução
   - `touring-cli` → cliente CLI efêmero
4. **JAMAIS** matar `touring-mcp`/`touring-cli`/`touring-hook` que não pertença
   à sessão CC atual

## Topologia (estado de transição 2026-05-23)

| Processo | Papel | Multiplicidade | Como spawnado |
|---|---|---|---|
| `touring-daemon` (canonical pós Sprint 4 PD 2026-05-23) | Backend RPC, segura socket via flock LOCK_EX|LOCK_NB + comm-based idempotency | **Singleton/user** | `update-touring` ou auto-spawn por client |
| `touring serve` (cmdline) / `touring-mcp` (comm pós PC-1) | MCP bridge stdio↔socket | 1 por sessão CC | Claude Code spawn como MCP server |
| `touring <subcmd>` / `touring-cli` (comm pós PC-1) | CLI client RPC efêmero | N por sessão CC | invoked manualmente ou por scripts |
| `touring-hook <event>` (cmdline) / `touring-hook` (comm) | Hook handler efêmero | N por sessão CC | Claude Code lifecycle events |

**Nota**: até PC-1 implementar `prctl(PR_SET_NAME)`, todos os `touring-hook*` aparecem
com COMM idêntico `touring-hook` (truncado para 15 chars). Distinção temporária via
`/proc/<pid>/cmdline`.

## Detecção segura (substituto para pkill)

```bash
# Quem segura o socket?
lsof /tmp/touring-daemon-1000.sock 2>/dev/null

# Quem é o daemon canônico?
cat /run/user/$UID/touring-daemon.pid 2>/dev/null \
  || cat /tmp/touring-daemon-1000.pid 2>/dev/null

# Listar TODOS os PIDs touring por tipo (pós PC-1):
for pid in $(pgrep touring); do
    comm=$(cat /proc/$pid/comm 2>/dev/null)
    cmdline=$(tr '\0' ' ' < /proc/$pid/cmdline 2>/dev/null)
    echo "PID=$pid COMM=$comm CMDLINE=$cmdline"
done
```

## Restart canônico (SUBSTITUTO ÚNICO para pkill)

```bash
# Helper canônico (pós PA-2 implementado)
touring daemon-ctl status        # status JSON do daemon
touring daemon-ctl restart       # SIGTERM graceful → wait drain → respawn
touring daemon-ctl stop          # SIGTERM com WARN se MCP bridges ativos
touring daemon-ctl reset         # nuclear, exige --yes-i-know-cascading-kill

# OU pipeline completo (build + install + restart + verify)
update-touring                   # full
update-touring --no-build        # só re-symlink + restart (rebuild externo)
update-touring --verify-only     # apenas check, não toca
```

## Anti-padrões PROIBIDOS

| Anti-padrão | Por quê é fatal | Substituir por |
|---|---|---|
| `pkill -f touring` | Mata daemon + MCP bridges + handlers + CLI de TODAS sessões CC | `touring daemon-ctl restart` |
| `pkill -9 touring-hook` | Mata handlers em execução de outras sessões | `touring daemon-ctl restart` |
| `kill -9 $(pgrep touring)` | Mesmo que acima | `touring daemon-ctl restart` |
| `killall touring-hook` | Mata handlers + daemon legado | `touring daemon-ctl restart` |
| `pkill touring-mcp` | Mata MCP bridge de OUTRA sessão CC (perda colateral) | NUNCA — Claude Code gerencia seus próprios MCP servers |

## Daemon degraded ≠ Daemon inexistente

**Spurious race no SessionStart** é comum:
- Hook `instructions-loaded` dispara no início da sessão
- Tenta `touring status` antes do daemon completar bind do socket
- Recebe `Connection refused (os error 111)`
- Reporta `composite_health_score: 0.5` (default degraded)
- **Auto-recupera em 1-2s** quando daemon completa startup

**Protocolo correto** quando degraded é observado:
1. **AGUARDAR 2-3s** (pode ser race transitório)
2. `touring daemon-ctl status` (não pgrep) para confirmação canônica
3. Se persistir, `update-touring --verify-only` para diagnose
4. **APENAS** se daemon comprovadamente morto: `touring daemon-ctl restart`
5. **NUNCA** pular para `pkill` mesmo após N tentativas

## Bypass (apenas em situações documentadas)

Se você é Gabriel (ou outro humano operador) e SABE o que está fazendo:

```bash
export TOURING_PROCESS_GUARD_DISABLED=1
pkill -9 -f touring-daemon         # agora permitido (mas pense duas vezes)
unset TOURING_PROCESS_GUARD_DISABLED
```

A LLM **NÃO** pode setar essa env var por conta própria. Bypass exige decisão humana explícita.

## Cross-references

| Tópico | Local |
|---|---|
| REGRA #11 (git proibido) + topologia legada | `~/.claude/CLAUDE.md` (TOURING NERVOUS SYSTEM section) |
| REGRA #2.5 daemon spawn pattern | `~/.claude/skills/Touring/references/touring-rebuild-rule.md` |
| touring-process-guard.sh enforcement | `~/.claude/hooks/touring-process-guard.sh` (REGRA #19 anti-pkill, standalone) |
| Plano de implementação completo | `~/.claude/plans/touring-process-hygiene-2026-05-23/plan.md` |
| Touring CLI ranks | `~/.claude/rules/touring-cli-index.md` |

---

_v1.0 — 2026-05-23 | Defesa institucional contra cascading kill em ambientes multi-sessão CC._
_Anchor: 2026-05-23 — daemon PID 3938525 (4 zombies) + 2+ sessões CC concorrentes em uso simultâneo._
