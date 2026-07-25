# Touring Rebuild Protocol — Binário, Symlinks, Daemon Lifecycle

> **Auto-load** (constituição operacional) | **Version**: v3 (slim) | **Last updated**: 2026-05-26
> **Tooling**: `~/.local/bin/update-touring` (v2)
> **Historical changelog + LEGACY patterns + Sprint forensics**: `~/.claude/skills/Touring/references/touring-rebuild-changelog.md`

---

## Princípio Operacional

`target/release/` é a **única fonte de verdade** dos binários Touring. Tudo o resto são symlinks. Toda atualização é **atômica**, **idempotente** e **dual-target**.

---

## Topografia dos Binários

```
SOURCE OF TRUTH:
  ~/projects/touring/target/release/
    touring           (~70 MB, opt-level=s, lto=fat, strip=symbols, panic=abort)
    touring-hook      (~60 MB)
    touring-daemon    (~60 MB)

SYMLINK TARGETS (ambos atualizados pelo update-touring):

  ~/.local/bin/                           (shell PATH — `which touring`)
    touring           → release/touring
    touring-hook      → release/touring-hook
    touring-daemon    → release/touring-daemon
    touring-bootstrap  (script real, separado)
    touring-mcp        (script real, separado)
    update-touring    (este script — não symlink)

  ~/.claude/hooks/                        (Claude Code lê via settings.json)
    touring           → release/touring
    touring-hook      → release/touring-hook
    touring-daemon    → release/touring-daemon
    touring_batch_indexer.py              (Python wrapper)
    touring_graph_indexer.py              (Python wrapper)
    touring-startup.sh                   (script de boot)

DAEMON RUNTIME STATE:
  /tmp/touring-daemon-1000.sock           (Unix socket — RPC client/server)
  /tmp/touring-daemon-1000.lock           (PID lockfile)

LOG:
  ~/.claude/touring/update.log            (append-only history)
```

---

## REGRA #1 — Single Pipeline para Rebuild

**SEMPRE** usar `update-touring` para rebuilds, NUNCA `cargo build` standalone.

```bash
update-touring                  # full: kill → build → install → restart → verify
update-touring --clean          # full + cargo clean (rebuild from scratch)
update-touring --no-build       # apenas refresh dos symlinks (após edit manual)
update-touring --no-kill        # build sem matar daemon (RISCO: daemon antigo continua)
update-touring --no-restart     # build + install, mas daemon fica down (lazy auto-start)
update-touring --verify-only    # apenas health check (não toca em nada)
```

**Pipeline interno** (6 fases): KILL_DAEMON → CLEANUP (sock+lock) → BUILD (cargo --release --workspace) → INSTALL (symlinks em ~/.local/bin/ E ~/.claude/hooks/, com backup .old) → RESTART (nohup touring-daemon detached + wait socket 5s) → VERIFY (touring doctor + verify_daemon_exe).

**Exit codes**: `0` success | `1` argument error | `2` cargo build failed | `3` symlink install failed | `4` daemon restart/health failed OR daemon exe is "(deleted)".

---

## REGRA #2 — Dual-Target Install OBRIGATÓRIO

Após **CADA** rebuild, AMBOS diretórios devem ter symlinks atualizados:

```bash
~/.local/bin/touring{,-hook,-daemon}        # shell PATH
~/.claude/hooks/touring{,-hook,-daemon}     # settings.json do Claude Code
```

**Por quê é crítico**: `settings.json` registra hooks com `$HOME/.claude/hooks/touring-hook <evento>`. Se esse symlink aponta para `target/debug` (ou release antigo), o Claude Code roda binário inconsistente — silenciosamente, porque hooks **fail-open** por design.

**Verificação rápida**:

```bash
for f in ~/.local/bin/touring{,-hook,-daemon} ~/.claude/hooks/touring{,-hook,-daemon}; do
  printf '%-50s -> %s\n' "$f" "$(readlink "$f")"
done
# Todos devem apontar para .../target/release/...
```

---

## REGRA #2.5 — Daemon Spawn Pattern (Sprint 4 PD canonical, 2026-05-23)

Daemon canonical é o binário dedicado **`touring-daemon`** (spawned sem args). O pattern legacy `touring-hook --start-daemon` agora emite deprecation message + exit 2 (PD-3).

| Operação | Comando canônico atual |
|---|---|
| pgrep por daemon | `pgrep -f "touring-daemon$"` (preferido) ou `pgrep -f "touring-hook --start-daemon\|touring-daemon$"` (transição) |
| spawn manual | `nohup touring-daemon &` |
| Kill por nome | **REGRA #19 PROIBIDO** — use `touring daemon-ctl stop`. Não usar pkill. |

**Multi-daemon**: múltiplas sessões CC podem spawnar — apenas UM segura o socket LISTEN. Verificar via `lsof /tmp/touring-daemon-1000.sock`.

**Padrão LEGACY (`touring-hook --start-daemon`) + contexto histórico**: ver `references/touring-rebuild-changelog.md#regra-25-legacy`.

---

## REGRA #3 — Detectar daemon "(deleted)" (P5 verification)

Após rebuild, o daemon que estava rodando carrega o binário **ANTIGO** em memória — o novo binário no disco substituiu o inode, mas o processo segura referência ao inode original (visível como `(deleted)` em `/proc/<pid>/exe`).

```bash
for pid in $(pgrep -f "touring-daemon$\|touring-hook --start-daemon"); do
  exe=$(readlink -f /proc/${pid}/exe 2>/dev/null)
  echo "PID ${pid}: ${exe}"
done
# Se output contém "(deleted)" → restart necessário
```

`update-touring --verify-only` já executa `verify_daemon_exe()` e retorna **exit 4** quando detecta `(deleted)`.

---

## REGRA #4 — Hooks dependem do settings.json

`~/.claude/settings.json` registra TODOS os Touring hooks como `$HOME/.claude/hooks/touring-hook <command>`. Eventos cobertos (Hook Registry 176+):

```
PreToolUse:   pre-read | pre-edit | pre-edit-prevention | pre-write | pre-bash
              enter-plan-mode | exit-plan-mode | pre-task-scout (via PATH touring)
PostToolUse:  post-edit | post-write | post-read | post-bash | post-tool-rl
Session*:     SessionStart | SessionStop | SubagentStop | PreCompact
Task*:        TaskCreated | TaskCompleted | PreTaskScout
Hook*:        HookMemoryStore | HookMemoryRecall | instructions-loaded
Decompose*:   decompose-event | decompose-create | decompose-add
CLI*:         cli-* (tasksfile/devrcfile/mpatch/ast/wiring/session/etc.)
Neural*:      classify-intent | scan-pii
RL*:          post-tool-rl | pre-tool-rl
```

Se symlink quebrado ou apontar para binário antigo, hooks **fail-open silenciosamente** — não enriquecem context, não emitem RFC-100 diagnostics, sintomas: `touring gate-metrics -j` returns zero counters, `touring doctor -j` reporta `daemon_socket: error`.

**Lista completa de sintomas degradados**: `references/touring-rebuild-changelog.md#sintomas-de-hooks-degradados-full`.

---

## REGRA #5 — Backup automático antes de substituir

`update-touring` v2 cria backup `<symlink>.old` antes de criar o novo symlink. Rollback rápido:

```bash
cd ~/.local/bin/
mv touring.old touring && mv touring-hook.old touring-hook && mv touring-daemon.old touring-daemon
cd ~/.claude/hooks/
mv touring.old touring && mv touring-hook.old touring-hook && mv touring-daemon.old touring-daemon
update-touring --no-build     # re-symlink com versão antiga (mantém backup)
```

---

## REGRA #6 — Quando rodar update-touring

| Situação | Comando |
|----------|---------|
| Após editar código de touring crate | `update-touring` (full pipeline) |
| Após `cargo build --release` manual | `update-touring --no-build --no-kill` (só refresh) |
| Após `git pull` em `~/projects/touring/` | `update-touring --clean` (rebuild from scratch) |
| Daemon sem responder | `update-touring --verify-only` primeiro |
| Daemon "(deleted)" detectado | `update-touring --no-build` (re-link + restart) |
| Drift suspect entre hooks ↔ release | `update-touring --no-build` |
| CI / cron task | `update-touring --no-restart` (deixa daemon down — sobe lazy) |

---

## REGRA #7 — settings.json é Source of Truth dos Hooks

Quando adicionar novo hook event ao Touring, **NUNCA** editar `~/.claude/settings.json` sem antes:

1. Confirmar que `touring-hook <novo-evento>` existe (`touring-hook --help`)
2. Garantir que `~/.claude/hooks/touring-hook` → `target/release/touring-hook`
3. Adicionar entry no formato:
   ```json
   {
     "matcher": "Tool|Pattern",
     "hooks": [{"type": "command", "command": "$HOME/.claude/hooks/touring-hook <evento>"}]
   }
   ```

---

## REGRA #8 — Idle watchdog é OPT-IN (`TOURING_IDLE_TIMEOUT_SECS`)

A partir de 2026-05-01, o daemon NÃO se auto-desliga por inatividade por padrão.

| Cenário | Configuração | Comportamento |
|---------|--------------|---------------|
| Workstation dev (default) | UNSET ou `=0` | Watchdog NÃO spawna; daemon vivo até SIGTERM/SIGINT |
| Cloud / CI / container | `=300` (ou N>0) | Watchdog ativo; auto-shutdown após N segundos idle |
| Restaurar legacy 5min | `=300` exportado antes do spawn | Mesmo comportamento pré-2026-05-01 |

**Verificação**:
```bash
PID=$(pgrep -of touring-daemon)
cat /proc/$PID/environ | tr '\0' '\n' | grep -c TOURING_IDLE_TIMEOUT_SECS
# 0 = disabled (default). 1 = enabled, ver valor.
```

**Motivação histórica + sutilezas de runtime**: `references/touring-rebuild-changelog.md#2026-05-01--regra-8-idle-watchdog-opt-in`.

---

## Dinâmica de Restart Recomendada

| Cenário | Comando |
|---|---|
| **A** — Sessão Claude Code aberta | `update-touring` (full) em outro terminal → recarregue MCP via `/mcp` dialog |
| **B** — Headless (sem sessão ativa) | `update-touring --clean` (full clean rebuild) |
| **C** — Hot-reload (sem matar daemon) | `update-touring --no-kill` (RISCO: incompatibilidade protocolo se houve breaking change) |

---

## Diagnóstico de Drift (5 checks rápidos)

```bash
# 1. Symlinks corretos?
for f in ~/.local/bin/touring{,-hook,-daemon} ~/.claude/hooks/touring{,-hook,-daemon}; do
  printf '%-50s -> %s\n' "$f" "$(readlink "$f")"
done

# 2. Binários release existem e são recentes?
ls -la ~/projects/touring/target/release/touring{,-hook,-daemon}

# 3. Daemon vivo e em binário fresco?
update-touring --verify-only

# 4. Touring CLI versão?
touring doctor -j

# 5. Tail do log de update
tail -20 ~/.claude/touring/update.log
```

---

## Anti-padrões a evitar

| Anti-padrão | Por quê | Substituir por |
|-------------|---------|----------------|
| `cargo build` sem `update-touring` | Symlinks antigos continuam, daemon não reinicia | `update-touring` (full) |
| `cp target/release/touring ~/.local/bin/` | Quebra invariante symlinks; próximo update detona | `ln -sf` ou `update-touring --no-build` |
| `kill -9 touring-daemon` | Deixa socket+lock stale; próximo daemon falha | `touring daemon-ctl restart` (REGRA #19) |
| Editar `~/.claude/hooks/touring-hook` (binário) | É symlink — edit afeta release source | Editar em `crates/`, depois `update-touring` |
| `pgrep "touring serve"` (sem `--start-daemon`) | Não encontra daemon spawnado via hook pattern | `pgrep -f "touring-daemon$\|touring-hook --start-daemon"` |

---

## Checklist após rebuild

- [ ] `update-touring` retornou **exit 0**
- [ ] `touring doctor -j` retorna **5/5 ok**
- [ ] `pgrep -f "touring-daemon$"` retorna **1 PID**
- [ ] `readlink -f /proc/<pid>/exe` **NÃO** contém `"(deleted)"`
- [ ] `~/.claude/hooks/touring-hook` → `target/release/touring-hook`
- [ ] `~/.local/bin/touring` → `target/release/touring`
- [ ] `tail ~/.claude/touring/update.log` mostra **"done"** ou **"verified"**

---

## Referências cruzadas

| Item | Local |
|------|-------|
| Script | `~/.local/bin/update-touring` (v2) |
| Logs | `~/.claude/touring/update.log` |
| Settings | `~/.claude/settings.json` |
| Hook Registry | `crates/touring-hooks/src/hook_registry.rs` |
| Changelog + LEGACY patterns + Sprint forensics | `~/.claude/skills/Touring/references/touring-rebuild-changelog.md` |
| Disk hygiene | `~/.claude/rules/disk-hygiene.md` |
| Process hygiene | `~/.claude/rules/touring-process-hygiene.md` |
| Skill master | `~/.claude/skills/Touring/SKILL.md` |
| CLI ranks | `~/.claude/rules/touring-cli-index.md` |
