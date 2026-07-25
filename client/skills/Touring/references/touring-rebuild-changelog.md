# touring-rebuild — Changelog + LEGACY Patterns + Sprint Notes

Companion reference for `~/.claude/rules/touring-rebuild.md`. The rule keeps only currently-operational instructions; this file holds historical sprint discoveries, deprecated patterns, and forensic notes from waves that shaped the current protocol.

## Changelog

### 2026-05-24 Sprint 4.6 (Daemon Death Root Cause + FIX + Hardlink Recovery)

- **ROOT CAUSE confirmado** via strace wrapper (Etapa G):
  `hook_registry.rs:671` post-bash dispatch invocava `crate::post_bash::run(rt, v)`
  cujo body termina em `HookResponse::emit()` → **`process::exit(0)` dentro da
  própria tokio task do daemon**, matando o processo inteiro. Smoking gun:
  strace mostrou PID tokio-worker chamando `exit_group(0)` LIMPO — zero signals,
  zero kills, zero panics. Era o único dispatch entry usando `.run()` ao invés
  do safe `.run_returning().to_json()` pattern dos outros 14+ hooks.
- **FIX permanente** (Etapa L): trocar para
  `crate::post_bash::run_returning(rt, v).to_json()` + comentário Sprint 4.6
  explicando o anti-padrão.
- **Regression test**: `sprint_4_6_post_bash_dispatch_must_not_call_emit` em
  `hook_registry.rs:1884` via `include_str!("hook_registry.rs")` source-scan
  — falha se alguém reverter para `crate::post_bash::run(rt`.
- **VALIDATED em produção**: stress 300× parallel + 90s idle soak = daemon
  ALIVE (PID estável), `daemon-crash.jsonl` VAZIO, 7/8 healthy. Antes morria
  ~2×/sessão em janelas idle; agora zero deaths.
- **SUB-PROBLEMA descoberto durante validation** (anti-padrão de build): o
  binário `target/release/touring-hook` estava CORROMPIDO (`file=data`, primeiros
  bytes todos zero, não-ELF) com **hardlink** em `target/release/deps/`. Cargo
  confiava em mtime e via "up-to-date", sem detectar corrupção. `rm` no
  symlink target removeu UMA entry mas hardlink persistia. Sintoma observado
  pelo Gabriel: `/bin/sh: 1: /home/.../touring-hook: Exec format error` em
  hooks PreToolUse/PostToolUse (fail-open silencioso).
- **Hardlink recovery procedure** (nova): quando binário aparece como `data`
  em `file`, NÃO basta `rm` + `cargo build` — Cargo vê o hardlink remanescente
  e nada reconstrói. Procedimento canônico:
  ```bash
  find ~/projects/touring/target -inum $(stat -c '%i' <binary>) -delete
  touch <crate>/src/main.rs <crate>/src/lib.rs    # invalidate cache
  cargo build --release -p <crate> --bin <binary>
  file <binary>                                    # MUST be ELF
  ```
- **Files Sprint 4.6**: `crates/touring-hooks/src/hook_registry.rs` (linha 671
  fix + linha 1884 regression test); `~/.local/bin/touring-daemon-strace`
  (wrapper opt-in via `TOURING_DAEMON_STRACE=1`, ~80 LOC).
- **D1-FOLLOWUP estrutural (REGRA #0 potencializar)**: novo teste
  `sprint_4_6_no_dispatch_entry_may_call_an_emitting_run` em
  `hook_registry.rs:1925` enumera 14 módulos perigosos (`permission_request,
  post_bash, post_edit, post_tool_use, post_write, pre_bash, pre_edit,
  pre_edit_prevention, pre_glob, pre_grep, pre_read, pre_tool_use,
  pre_write, stop`) cujos `pub fn run()` terminam em `.emit()`. O teste
  scopeia ao production code (exclui `#[cfg(test)]` para evitar self-match
  em string literals) e filtra comentários. Impede recidiva do bug para
  qualquer hook futuro. 2/2 testes Sprint 4.6 PASS.
- **Etapa H auditd (REGRA #19 LLM não sudo)**: artefatos preparados —
  `~/.claude/touring/touring-daemon-audit.rules` (signal+exit_group rules
  com keys `touring_signal`/`touring_exit`, b64+b32) + runbook executável
  `~/.claude/touring/auditd-touring-setup.sh` (`install|status|uninstall|
  tail|grep|rules` subcommands). Gabriel executa manualmente com sudo.
- **Etapa J valgrind SKIP formal**: documentado em
  `~/projects/touring/docs/2026-05-24-sprint-4.6-etapa-J-valgrind-gated-skip.md`
  — gate condition (memcorrupt evidence) NÃO atingida pois root cause foi
  `exit_group(0)` clean. ROI <2% por engineer-hour. Re-activation criteria
  definidos (SIGSEGV/SIGABRT futuro, unsafe code panic, miri violation).
- **WIN COLATERAL Sprint 4.5+4.6 infrastructure**: imediatamente após o
  post-bash fix, o panic_log + stderr-isolated **capturaram um SEGUNDO
  bug latente** previamente invisível — `ropey 1.6.1` OOB unwrap em
  `touring-code/src/ast/document.rs` (`Rope::byte_to_char` chamado com
  `byte_idx=84877` vs `Rope length=84279`, thread `touring-project-actor`
  uptime 866s, location `ropey-1.6.1/src/rope.rs:635:41`). `byte_to_point_safe`
  (line 106) já existe no mesmo arquivo com padrão `try_byte_to_char.ok()`
  canônico — fix exists, não está sendo usado em todos sites. Sprint 4.7
  candidate (`Task #73` + `gotcha #20` `ropey-Rope-byte_to_char-OOB-stale-byte-idx`).
  Lição: a infraestrutura observability se paga sozinha — bug invisível por
  semanas virou loud-and-located em minutos.

### 2026-05-23 Sprint 4.5 (Daemon Resilience Observability)

- **`update-touring start_daemon` instrumentado**: `setsid` (new session,
  immune to controlling terminal SIGHUP) + `RUST_BACKTRACE=full` +
  `RUST_LOG=info,touring_hooks::daemon=debug` + `TOURING_CRASH_LOG_PATH`
  env var. Stderr isolado em `~/.claude/touring/daemon-stderr.log` (10 MiB
  rotation one-generation) — separado de `update.log`.
- **Panic hook** (`crates/touring-hooks/src/panic_log.rs` + wire em
  `daemon_main.rs`): captura forensics (thread name, location, payload,
  uptime, pid) para `~/.claude/touring/daemon-crash.jsonl` antes do
  `panic = "abort"` terminar o processo. Chain previous hook preserva
  stderr output. Idempotente. 6/6 unit tests PASS.
- **eprintln markers em 3 exit paths** (daemon.rs): `AlreadyAlive` (REGRA
  #19 silent race-loser exit, antes invisível via `tracing::info!` sem
  `TOURING_LOG=info`), `graceful_shutdown CALLED` (início de shutdown),
  `rt.block_on returned Ok` (post-loop UNEXPECTED). Cada path agora é
  observável em stderr sem dependência de env var.
- **Diagnóstico em aberto**: deaths via path NÃO-Rust ainda ocorrem em
  momentos idle. Daemon sobreviveu soak 120s + 6901 reqs / 0 fail, mas
  morre em janelas sem load. Stderr corta sempre no mesmo offset (74)
  após WASM inferlets init — silêncio total (sem panic, sem
  graceful_shutdown, sem markers). Conclusão FACT[0.95]: SIGKILL externo
  OU libc::abort (SQLite assert) OU SIGSEGV (stack overflow). Próxima
  wave (Sprint 4.6): strace wrapper, auditd para SIGKILL, SQLite assert
  audit, valgrind se memcorrupt suspeito.
- **Files diagnósticos pós-instrumentação**:

  | Path | Conteúdo |
  |---|---|
  | `~/.claude/touring/daemon-stderr.log` | stderr isolado (RUST_BACKTRACE + tracing) |
  | `~/.claude/touring/daemon-crash.jsonl` | JSON line per Rust panic (timestamp, location, payload) |
  | `~/.claude/touring/update.log` | script log (kill/build/install/restart) |
  | `~/.claude/touring/daemon-stderr.log.old` | stderr after rotation (10 MiB) |

### 2026-05-23 Sprint 4 PD (Touring Process Hygiene)

Daemon canonical é o binário dedicado `touring-daemon` (não mais `touring-hook --start-daemon` polimórfico). PD-1 atualizou update-touring (spawn dedicated + pgrep dual-pattern transição). PD-2 atualizou daemon_ctl CLI handler (comm-based detection via `/proc/<pid>/comm`, spawn touring-daemon). PD-3 deprecou `--start-daemon` mode em touring-hook (exit 2 + migration message). PC-1 normalizou comm strings via `prctl(PR_SET_NAME)`: touring-daemon | touring-mcp | touring-hook | touring-cli. REGRA #2.5 SUPERSEDED parcialmente — pgrep/spawn patterns atualizados inline na rule slim.

### 2026-05-01 — REGRA #8 (idle watchdog opt-in)

`TOURING_IDLE_TIMEOUT_SECS=0` é o default novo — daemon não se auto-desliga, eliminando o cold-start race no SessionStart de cada CC. Para restaurar comportamento legacy de 5min: exportar `TOURING_IDLE_TIMEOUT_SECS=300` antes de spawn do daemon.

**Por quê o default mudou**: o auto-shutdown de 5min causava o cold-start race em cada SessionStart de CC após gaps idle (`Connection refused (os error 111)` em `touring doctor`). O daemon RSS é ~92 MB residente — barato para manter vivo, e a ausência do shutdown elimina a degradação fantasma do `composite_health_score=0.5`.

**Sutileza importante**: env vars são lidas pelo processo **daemon**, não pelo client. Para alterar o setting, exporte ANTES do `update-touring` ou kill+respawn. O watchdog re-lê a env var a cada tick (30s), então mudanças via `set_var` em runtime no mesmo processo daemon entram em vigor — útil em testes, não em deploy normal.

### 2026-04-27 — Fixed pgrep/kill patterns

Fixed pgrep/kill patterns para usar `touring-hook --start-daemon` (REGRA #2.5 discovery). Adicionado `verify_daemon_exe()` check no script.

### 2026-04-26 — Criação inicial

Criado — descobriu desincronia `~/.claude/hooks/` (debug) vs `~/.local/bin/` (release). Dual-target install + daemon spawn pattern.

---

## REGRA #2.5 (LEGACY) — Daemon Spawn Pattern: `touring-hook --start-daemon`

> Pattern LEGACY (pré Sprint 4 PD 2026-05-23). O canonical agora é o binário dedicado `touring-daemon`. Esta seção é preservada para compreender sistemas legacy/transição.

**FACT [1.0]** descoberto na Wave 2026-04-26: o daemon Touring **NÃO era spawnado** como `touring serve` em produção. O padrão real era:

```
COMANDO REAL:  touring-hook --start-daemon
PROCESSO:      /home/gabrielgadea/projects/touring/target/release/touring-hook
```

Auto-spawn: quando um cliente CLI (`touring <cmd>`) ou hook (`touring-hook <event>`) detectava que o socket `/tmp/touring-daemon-1000.sock` não respondia, ele invocava `touring-hook --start-daemon` para iniciar o daemon detached.

**Implicações operacionais (LEGACY)**:

| Operação | Padrão correto (LEGACY) | Padrão errado |
|---------|----------------|---------------|
| pgrep por daemon | `pgrep -f "touring-hook --start-daemon\|touring-daemon$\|touring serve"` | só `"touring serve"` ❌ |
| spawn manual | `nohup touring-daemon &` (Sprint 4 PD canonical) ou `nohup touring-hook --start-daemon &` (LEGACY, deprecated) | `nohup touring serve &` ❌ |
| Kill por nome | **REGRA #19 PROIBIDO** — use `touring daemon-ctl stop` (Sprint 1 S-2). Não usar pkill. | qualquer pkill/killall que casa "touring" ❌ |

**Multi-daemon**: múltiplas sessões Claude Code paralelas podem cada uma spawnar seu próprio daemon — apenas **UM** segura o socket LISTEN (winner of bind race), os demais terminam. Verificar via `lsof /tmp/touring-daemon-1000.sock`.

---

## Sintomas de hooks degradados (full)

Se `~/.claude/hooks/touring-hook` for symlink quebrado ou apontar para binário antigo, hooks **fail-open** (silenciosamente):

- Não enriquecem context (`instructions_loaded` fica mudo)
- Não tracking quality delta (pre/post_edit não rodam)
- Não emitem RFC-100 diagnostics (Q-201/Q-202, B-300, M-5xx, W-1xx)

**Sintomas observáveis**:

- `touring synergy --with-metrics` mostra counters em zero
- `touring gate-metrics -j` retorna `health_delta_record_count: 0`
- Session start não mostra `composite_health_score`
- `touring doctor -j` reporta `daemon_socket: error`
