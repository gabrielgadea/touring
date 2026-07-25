# Disk Hygiene — Rust `target/` Sprawl Control (REGRA #12)

> **Auto-load** (slim) | **Version**: v2 (slim) | **Origin**: Wave 2026-04-26 recovered 148 GB perdidos | **Disco 78%→62%**
> **Full configs + scripts + trade-offs**: `~/.claude/skills/Touring/references/disk-hygiene-detail.md` (load on demand)

---

## Princípio Operacional

`target/` é cache regenerável, não estado. Tratar com hygiene defensiva: **meça → previna → observe → limpe cirurgicamente**. Limpezas reativas com `rm -rf target/` cego durante builds vivos é o principal causador de "erros durante limpeza". `safe-clean.sh` substitui esse padrão.

---

## REGRAs (resumo)

| # | REGRA | Resumo operacional |
|---|---|---|
| **#1** | Profile defensivo OBRIGATÓRIO em todo Cargo.toml | `[profile.dev]`: `opt-level=0`, `debug="line-tables-only"`, `incremental=false` (sccache assume), `split-debuginfo="unpacked"`. Deps externas `debug=false`, `opt-level=2`. Opt-in `[profile.debugging]` para sessões gdb/lldb. |
| **#2** | sccache global ativado | `~/.cargo/config.toml` com `rustc-wrapper = "sccache"` + `SCCACHE_CACHE_SIZE=25G` + `SCCACHE_IDLE_TIMEOUT=0`. Hit rate alvo ≥ 60% **para C/C++** (medido 47%); **Rust local mede ~2%** (bins/proc-macros non-cacheable) → ver EXCEÇÃO acima (workspace touring desativa sccache). |
| **#3** | mold linker (Linux) | `apt install mold` + `[target.x86_64-unknown-linux-gnu] linker="clang", rustflags=["-C","link-arg=-fuse-ld=mold"]`. Link 5-10× mais rápido. Preservar `[build] rustflags` existentes (cargo concatena). |
| **#4** | Limpeza CIRÚRGICA, nunca cega | **PROIBIDO**: `rm -rf target/`. **USE**: `~/.claude/tools/safe-clean.sh {incremental,sweep,stats}` OR `cargo clean {--doc,--release,--profile <X>,-p <crate>,--dry-run}`. safe-clean tem gates anti-build-vivo + preserva binário do daemon. |
| **#5** | Observabilidade contínua | Cron diário 03:00 → `disk-watch.sh`; cron semanal Dom 04:00 → `safe-clean.sh sweep`. Output em `~/.claude/touring/disk_{baseline.json,watch.log,cleanup.log}`. |
| **#6** | Workspace novo → adicionar ao monitor | Editar `TARGETS` array em `~/.claude/tools/disk-watch.sh` + lista em `safe-clean.sh::mode_incremental()`. |
| **#7** | Anti-padrões a evitar | Ver tabela abaixo. |

> **⚠ EXCEÇÃO — workspace `~/projects/touring` (touring) — 2026-06-26**: este workspace **desativa sccache** (`.cargo/config.toml` → `[build] rustc-wrapper = ""`) e usa `[profile.dev] incremental = true`. Dois motivos: (1) sccache Rust hit-rate medido = **2.29%** (não o ≥60% da REGRA #2 — esse alvo vale para C/C++, não Rust); (2) sccache é **correctness hazard** aqui — crates `bin`/`proc-macro` são non-cacheable e proc-macros que leem o filesystem podem servir objeto **stale** (binário≠source observado 2026-06-26; `mozilla/sccache docs/Rust.md`). Para dev Rust local iterativo o **incremental do cargo é o cache correto**. As REGRAs #1/#2 abaixo permanecem válidas para os DEMAIS projetos (sccache global segue ON, útil em C/C++ 47% e CI cold). Detalhe + prova: `memory/project_sccache_daemon_status_fixes_2026_06_26.md`.

**Full configs (toml, sccache stats, mold install, safe-clean gates, cron schedule, registration procedure, trade-offs)**: `references/disk-hygiene-detail.md`.

---

## Anti-padrões a evitar (REGRA #7)

| Anti-padrão | Por quê | Substituir por |
|---|---|---|
| `target/` dentro de `crates/<x>/` | Bug — workspace usa `target/` raiz | `rm -rf crates/<x>/target` |
| `[build] split-debuginfo = "..."` | cargo ≥ 1.93 ignora silenciosamente | Mover para `[profile.<name>]` |
| `rm -rf target/` durante build | Quebra locks, daemon, sccache mmap | `safe-clean.sh incremental` |
| `incremental = true` **com sccache ON** | Duplica cache + sabota hits | `incremental = false` em dev (mas se sccache OFF — ex. workspace touring — `incremental = true` é o correto) |
| `cargo clean` sem flag | Apaga TUDO; rebuild full | `cargo clean -p X` ou `--doc/--release` |
| `awk 'printf "%.2f"'` em pt_BR | Emite `12,34` (vírgula) → quebra JSON | `export LC_ALL=C` no script |

---

## Diagnóstico Rápido (5 checks)

```bash
# Snapshot do estado
df -h /home | tail -1
~/.claude/tools/disk-watch.sh

# Top 5 ofensores
du -sh /home/gabrielgadea/projects/*/target ~/projects/touring/target 2>/dev/null | sort -rh | head -5

# Sccache health
sccache --show-stats | head -25

# Verificar profile aplicado em workspace
grep -A 5 "^\[profile\.dev\]" <workspace>/Cargo.toml

# Cargo clean preview seguro
cargo clean --dry-run --manifest-path <workspace>/Cargo.toml
```

---

## Checklist — Workspaces Rust Novos

- [ ] `Cargo.toml` tem `[profile.dev]` com as 4 settings da REGRA #1
- [ ] `Cargo.toml` tem `[profile.dev.package."*"] debug = false`
- [ ] `Cargo.toml` tem `[profile.fast-iter]` + `[profile.debugging]` opt-in
- [ ] Workspace adicionado ao `TARGETS` em `disk-watch.sh`
- [ ] Workspace adicionado ao `mode_incremental()` em `safe-clean.sh`
- [ ] Primeiro `cargo build` validado — esperado rebuild full inicial (~5-10 min)
- [ ] `sccache --show-stats` confirma hit rate subindo após N builds
- [ ] `~/.claude/tools/disk-watch.sh` lista o novo target

---

## Referências cruzadas

| Item | Local |
|---|---|
| **Full configs + scripts + trade-offs** | `~/.claude/skills/Touring/references/disk-hygiene-detail.md` |
| Wave session report (lessons + métricas) | `~/.claude/projects/-home-gabrielgadea/memory/project_disk_optimization_2026_04_26.md` |
| Cargo Book oficial | https://doc.rust-lang.org/cargo/guide/build-performance.html |
| sccache config | https://github.com/mozilla/sccache/blob/main/docs/Configuration.md |
| Touring CLI integration | `~/.claude/skills/Touring/SKILL.md` |
| Scripts | `~/.claude/tools/{disk-watch.sh,safe-clean.sh}` |
