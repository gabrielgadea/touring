# disk-hygiene — Configurations + Scripts (full detail)

Companion reference for `~/.claude/rules/disk-hygiene.md`. The rule keeps Princípio + REGRAs (1-7) as concise summary; this file holds the full Cargo profile configs, sccache/mold setup, safe-clean script gates, cron observability + trade-offs explicitados. Load when actively configuring a workspace or setting up disk hygiene.

## REGRA #1 — Profile defensivo OBRIGATÓRIO (full config)

Todo `Cargo.toml` de workspace deve ter `[profile.dev]` configurado para não desperdiçar disco. Caso contrário, `target/debug/` cresce até consumir o disco em poucas semanas de iteração.

```toml
[profile.dev]
opt-level = 0                       # ou 1, conforme projeto
debug = "line-tables-only"          # Cargo Book oficial: build-performance.html
incremental = false                 # sccache (rustc-wrapper) assume o papel
split-debuginfo = "unpacked"        # Linux: separa .dwo (~30% rlib menor)

[profile.dev.package."*"]
opt-level = 2                       # ou 1 se há OOM em 16+ jobs paralelos
debug = false                       # ZERO debug info em deps externas

# Opt-in profiles para casos específicos
[profile.fast-iter]                 # cargo build --profile fast-iter
inherits = "dev"
incremental = true                  # quando precisar de incremental local
split-debuginfo = "off"

[profile.debugging]                 # cargo build --profile debugging
inherits = "dev"
debug = true                        # debug info completo p/ gdb/lldb
```

**Por que `incremental = false`** — quando sccache está globalmente configurado (`rustc-wrapper = "sccache"` em `~/.cargo/config.toml`), `incremental = true` marca compilações como non-cacheable e cria 21 GB+ duplicados em `target/debug/incremental/`. A combinação custa **disco e velocidade**.

**Por que `debug = "line-tables-only"`** — preserva nomes de arquivo + linha (suficiente para panics e backtraces) mas remove tipos/locals que inflam rlibs em ~30%. Para sessões de debug interativo, opt-in via `--profile debugging`.

**Por que `split-debuginfo = "unpacked"`** (Linux) — separa debug info em arquivos `.dwo` adjacentes ao binário, removendo-o do payload do rlib. Default do Linux é `off` (debug info embutido = bloat). Note: é setting de **profile**, NÃO de `[build]` — colocar em `[build]` gera warning silencioso em cargo ≥ 1.93.

## REGRA #2 — sccache global ativado e dimensionado (full config)

Em `~/.cargo/config.toml`:

```toml
[build]
rustc-wrapper = "sccache"

[env]
SCCACHE_CACHE_SIZE = { value = "25G", force = false }
SCCACHE_IDLE_TIMEOUT = { value = "0", force = false }      # server permanente
SCCACHE_CACHE_ZSTD_LEVEL = { value = "10", force = false } # ~10% mais compacto
```

**Validar saúde** após algumas horas de uso:

```bash
sccache --show-stats | grep -E "Cache hits rate|Max cache size|Non-cacheable reasons"
```

Hit rate alvo: **>= 60%** (com `incremental = false` em dev). Se hit rate < 50%, investigar `Non-cacheable reasons` — geralmente é incremental ainda ativo em algum sub-projeto.

## REGRA #3 — mold linker (Linux) — link 5-10× mais rápido (full setup)

```bash
sudo apt install mold     # Ubuntu/Debian/Pop!_OS
# Arch: sudo pacman -S mold
# Fedora: sudo dnf install mold
```

Em `~/.cargo/config.toml` (global) E em `<workspace>/.cargo/config.toml` quando o workspace já tem `[target.*]` redefinido:

```toml
[target.x86_64-unknown-linux-gnu]
linker = "clang"
rustflags = ["-C", "link-arg=-fuse-ld=mold"]
```

**Atenção**: workspace local com `[build] rustflags = [...]` precisa preservar essas flags se já existirem (e.g., `tokio_unstable`). cargo **concatena** `[build] rustflags` com `[target.*] rustflags`.

## REGRA #4 — Limpeza CIRÚRGICA (full commands + gates)

**PROIBIDO**:

```bash
rm -rf target/                    # destrói build atual; quebra daemon vivo
rm -rf target/release/            # remove binário em uso pelo daemon
```

**OBRIGATÓRIO** — usar `safe-clean.sh` ou `cargo clean` com flags:

```bash
# Quick wipe (sempre seguro)
~/.claude/tools/safe-clean.sh incremental

# Remove rlibs órfãos > 7 dias (cargo-sweep)
~/.claude/tools/safe-clean.sh sweep
SAFE_CLEAN_DAYS=14 ~/.claude/tools/safe-clean.sh sweep

# Cargo nativo cirúrgico
cargo clean --doc                       # só documentação
cargo clean --release                   # só profile release
cargo clean --profile <name>            # profile específico
cargo clean -p <crate>                  # pacote específico
cargo clean --dry-run                   # preview SEM destrutivo
```

**Gates do `safe-clean.sh`**:

- Aborta (exit 2) se cargo build/test/check ou rustc estiverem ativos
- Avisa (continua) se `touring serve` daemon estiver rodando — preserva binário
- Loga toda ação em `~/.claude/touring/disk_cleanup.log`

## REGRA #5 — Observabilidade contínua (cron + scripts)

Cron diário 03:00 (instalado via Wave 2026-04-26):

```cron
0 3 * * * DISK_WATCH_QUIET=1 /home/gabrielgadea/.claude/tools/disk-watch.sh >/dev/null 2>&1
0 4 * * 0 /home/gabrielgadea/.claude/tools/safe-clean.sh sweep >>/home/gabrielgadea/.claude/touring/disk_cleanup.log 2>&1
```

**Output**:

- `~/.claude/touring/disk_baseline.json` — JSON state atual (multi-workspace)
- `~/.claude/touring/disk_watch.log` — append diário com timestamps
- `~/.claude/touring/disk_cleanup.log` — append por safe-clean run

**Inspeção interativa**:

```bash
~/.claude/tools/disk-watch.sh                          # report agora
~/.claude/tools/safe-clean.sh stats                    # via safe-clean
cat ~/.claude/touring/disk_baseline.json | jq          # último baseline
df -h /home | tail -1                                  # disco total
```

**Threshold de warning**: `DISK_WATCH_THRESHOLD_GB=50` (ajustável). Targets acima disso disparam linha WARN em log + stderr.

## REGRA #6 — Adicionar workspace novo ao monitor

Quando criar um workspace Rust novo (`cargo new --lib` ou similar) em diretório não-rastreado, **adicionar ao array `TARGETS` em** `~/.claude/tools/disk-watch.sh`:

```bash
declare -a TARGETS=(
  "touring|${HOME}/projects/touring/target"
  "analise-packages|${HOME}/projects/analise/packages/target"
  # ... existentes ...
  "novo-projeto|${HOME}/projects/novo-projeto/target"   # ← adicionar
)
```

Também atualizar a lista interna em `~/.claude/tools/safe-clean.sh` no `mode_incremental()` para que cleanups subsequentes capturem o novo path.

## Trade-offs explicitados

| Decisão | Custo | Benefício |
|---|---|---|
| `incremental = false` em dev | First-build após mudança +20-40% (5-10 min once) | Sccache hit rate ~37%→~75%; -21 GB recorrente |
| `debug = "line-tables-only"` | Debugger sem locals/types (mas mantém file:line) | Rlibs ~30% menores; rebuild ~25% mais rápido |
| `[profile.debugging] inherits = "dev"` | Opt-in via `cargo build --profile debugging` | Recupera debug experience quando precisar |
| `mold` linker | Apt install + 1 linha config | Link 5-10× mais rápido; menos arquivos intermediários |
| `cargo clean -p X --dry-run` no lugar de `rm -rf` | Pequeno overhead vs raw rm | Zero risco de quebrar build/daemon ativo |
