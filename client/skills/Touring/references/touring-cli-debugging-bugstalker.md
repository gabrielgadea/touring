# Debugging Touring Daemon with BugStalker

> **Wave 6 (2026-04-26)** — Touring v4.18.0 documentation addendum
> **BugStalker**: v0.4.5 (released 2026-04-18) | **License**: MIT | **Platform**: Linux x86_64
> **Repo**: https://github.com/godzie44/BugStalker | **Docs**: https://godzie44.github.io/BugStalker

---

## Quando usar BugStalker (vs tokio-console / dhat-heap / OTLP)

Touring já oferece três camadas de observability built-in:

| Tool | Modo | Overhead | Quando usar |
|------|------|----------|-------------|
| **tokio-console** (port 6669) | Live stream de tasks | Requer `--cfg tokio_unstable` rebuild + `console-subscriber` | Continuous monitoring durante dev; spans manualmente anotadas |
| **dhat-heap** (`--features dhat-heap`) | Heap allocation profiling | Substitui global allocator | Investigar memory bloat |
| **OTLP** (`--features otlp`) | Distributed tracing | Network export overhead | Production span correlation |
| **BugStalker** | **Pause-the-world snapshot** | **Zero (attach to running PID)** | **Post-mortem hangs / production deadlocks / no-rebuild scenarios** |

**Killer feature**: BugStalker tem oracle tokio que mostra task tree **sem nenhuma instrumentação**.
Útil quando o daemon trava em produção e você não pode rebuild com `tokio_unstable` para anexar tokio-console.

---

## Installation

```bash
# Recomendado: cargo install (auto-build com toolchain corrente)
cargo install bugstalker

# Arch Linux
sudo pacman -S bugstalker

# NixOS
nix run github:godzie44/BugStalker
```

Verificar:
```bash
bs --version  # deve mostrar 0.4.5+ (ou versão mais recente)
```

**System requirements**: Linux x86_64. Built-in unwinder — sem deps externas (libdwarf, libcapstone).

---

## Permissions (ptrace_scope)

Linux restringe ptrace via `/proc/sys/kernel/yama/ptrace_scope`:

| Valor | Comportamento | Touring impact |
|-------|---------------|----------------|
| `0` | Qualquer processo do mesmo uid | Attach livre |
| `1` (default Ubuntu/Debian) | Apenas filhos OU mesmo uid via prctl | **Funciona** se você lançou o daemon |
| `2` | Apenas com CAP_SYS_PTRACE | Precisa `sudo bs ...` |
| `3` | Disabled | Precisa kernel rebuild |

Verificar: `cat /proc/sys/kernel/yama/ptrace_scope`

Se ptrace_scope=2 ou kernel hardening:
```bash
# Temporário (até reboot)
sudo sysctl kernel.yama.ptrace_scope=1
```

---

## Attach to Running Touring Daemon

Touring serve runs como daemon background. Get PID + attach:

```bash
# Find daemon PID
TOURING_PID=$(pgrep -f "touring serve" | head -1)
echo "Daemon PID: $TOURING_PID"

# Attach in console mode (default)
bs -p $TOURING_PID

# Attach with tokio oracle pre-loaded
bs --oracle tokio -p $TOURING_PID

# Attach in TUI mode (recommended for exploration)
bs --tui -p $TOURING_PID
```

Quando BugStalker detacha (Ctrl+D ou `q` no TUI), o daemon **continua rodando**.
Se você matar bs com SIGKILL, daemon sobrevive (ptrace detach automático no SIGKILL handling do kernel).

---

## REPL Commands — Touring-relevant cheatsheet

### Tokio task introspection (THE killer feature)

```
oracle tokio                      # List active tasks: id, state, location, sleep_until
oracle tokio task <id>             # Inspect specific task
async backtrace                    # Stack trace of current async task
```

Cenários comuns no Touring:

| Sintoma | Comando |
|---------|---------|
| Daemon trava após hook storm | `oracle tokio` → procurar tasks `Pending` em `handle_connection_async` |
| MCTS shadow rollout > 200ms | `async backtrace` em frame de `mcts_shadow_rollout_hint` |
| HookRuntime mutex contention | `oracle tokio` → procurar tasks bloqueadas em `try_lock` |
| Tantivy commit hang | `async backtrace` em task `tantivy_upsert_*` |

### Variable inspection

```
vars                              # Locals da current frame
vars <name>                       # Inspect específico variable
print <expr>                      # Evaluate expression (e.g. `print self.task_count`)
print *self                       # Deref + print struct fields
```

Touring uses `core::fmt::Debug` para todos os tipos significativos — BugStalker renderiza
Vec/HashMap/Arc/Box/Mutex naturally via Debug trait.

### Breakpoints

```
break <function_name>              # Break on function entry
break <file.rs>:<line>             # Break on file:line
break clear <id>                   # Remove breakpoint
break list                         # List active breakpoints
continue                           # Resume execution until next break
step                              # Step into next instruction
next                              # Step over (don't enter functions)
```

### Watchpoints (data breakpoints)

```
watch <variable>                   # Break when variable changes
watch <expr>                       # Break when expression changes
```

Útil para detectar quem muta `HookRuntime.gate_metrics` ou similar shared state.

---

## Touring-Specific Debugging Recipes

### Recipe 1: Daemon hang post-deploy

```bash
TOURING_PID=$(pgrep -f "touring serve" | head -1)
bs --oracle tokio -p $TOURING_PID
# Inside REPL:
oracle tokio                       # Snapshot tokio task tree
# Procurar tasks `Pending` há > 5s
# Para cada suspeita, examinar localização:
async backtrace
```

### Recipe 2: Investigate MCTS shadow rollout exceeding budget

Touring mete `200ms` budget no `handle_enter_plan_mode` para MCTS. Se exceeded:
```bash
bs -p $TOURING_PID
# Set breakpoint no entrypoint do MCTS
break crates/touring-cognitive/src/cognitive_mcts.rs:rollout
continue
# Quando hit, inspecionar state:
vars
# Step através do critical loop:
next
next
```

### Recipe 3: Actor pattern deadlock (Wave 4 refactor)

`HookRuntime` usa actor pattern (mpsc + oneshot) post-Wave 4. Deadlock típico:
oneshot reply nunca chega → caller hangs.

```bash
bs --oracle tokio -p $TOURING_PID
oracle tokio                       # Find caller task em Pending state
# Identify mpsc channel by name:
print actor_handle.tx
# Inspect channel state:
print actor_handle.tx.capacity
```

### Recipe 4: Tantivy index commit hang

```bash
bs -p $TOURING_PID
break crates/touring-hooks/src/cli_handlers_tantivy.rs:commit_to_disk
continue
# When hit:
async backtrace                    # See full async chain
vars                              # Inspect IndexWriter state
```

### Recipe 5: HookRuntime mutex contention

```bash
bs -p $TOURING_PID
# Set watchpoint on contention metric
break crates/touring-hooks/src/runtime.rs:HookRuntime::dispatch
continue
# When fires:
async backtrace                    # See call chain
print self.dispatch_count          # Confirm metric incremented
```

---

## DAP Mode (VSCode Integration)

BugStalker supports DAP (Debug Adapter Protocol). Para debugar Touring no VSCode:

```bash
# Launch in DAP mode
bs --dap -p $TOURING_PID

# Configure VSCode launch.json:
# {
#   "type": "bs",
#   "request": "attach",
#   "name": "Attach Touring Daemon",
#   "processId": "${command:pickProcess}"
# }
```

Veja documentação BugStalker IDE integration: https://godzie44.github.io/BugStalker/docs/ide_integration

---

## Helper Script

Touring ships `scripts/debug-touring-daemon.sh` que automatiza setup:

```bash
~/projects/touring/scripts/debug-touring-daemon.sh           # default: console mode
~/projects/touring/scripts/debug-touring-daemon.sh --tui     # TUI mode
~/projects/touring/scripts/debug-touring-daemon.sh --oracle  # tokio oracle pre-loaded
~/projects/touring/scripts/debug-touring-daemon.sh --dap     # DAP mode for VSCode
```

Script faz:
1. Sanity check: BugStalker installed? touring binary exists? daemon running?
2. Auto-detecta PID via `pgrep -f "touring serve"`
3. Verifica ptrace_scope com warning se restrictivo
4. Launch BugStalker com flags solicitadas

---

## Limitações conhecidas

1. **Linux x86_64 only** — não funciona em macOS/Windows/ARM
2. **Pause-the-world** — quando attached, daemon não responde a hooks (Claude Code timeout possível)
3. **Single oracle**: apenas tokio implementado (custom oracles "TODO" no upstream)
4. **No remote debugging**: precisa local PID access
5. **Symbol info**: melhor com debug builds (`cargo build` sem `--release`); release builds têm symbols stripped

---

## When NOT to use BugStalker

- **Live monitoring contínuo**: prefira `tokio-console` (port 6669) ou OTLP traces
- **Memory leak investigation**: prefira `dhat-heap` ou `touring profile heap-dump`
- **Performance hot path**: prefira `pprof` flamegraph ou criterion benchmarks
- **CI / automated test**: BugStalker é interactive — não automation friendly

---

## See Also

- `touring-cli-rl-quality.md` — Gate metrics + observability counters
- `touring-cli-overview.md` — Daemon actor pattern
- BugStalker docs: https://godzie44.github.io/BugStalker/docs/overview
- Tokio console comparison: https://github.com/tokio-rs/console
