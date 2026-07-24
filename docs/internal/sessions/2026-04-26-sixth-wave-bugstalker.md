# Sixth Wave — BugStalker Debugging Integration (Documentation-Only)

**Date**: 2026-04-26 | **Session**: TACO L3 (no engineer phase) | **Skill**: Touring v4.18.0

## Objetivo

Análise profunda do repositório [godzie44/BugStalker](https://github.com/godzie44/BugStalker)
+ extração de insights/estratégias para potencializar Touring.

## Verdict: INTEGRATE-AS-DOCUMENTATION

BugStalker é **binary CLI standalone** (não library consumível) — versão 0.4.5 publicada
2026-04-18, MIT, Linux x86_64. Análise via WebFetch + grep do codebase Touring revelou:

**Não há overlap de mission**:
- Touring = static analysis de source code (tree-sitter, syn, AST)
- BugStalker = dynamic runtime analysis de compiled binaries (DWARF, gimli, ptrace)

**Mas BugStalker COMPLEMENTA Touring's tokio-console**:
- Touring já tem `console-subscriber` wired (port 6669, telemetry_init.rs:128)
- Mas requer rebuild com `RUSTFLAGS="--cfg tokio_unstable"` + manual `#[tokio::instrument]` spans
- BugStalker oferece tokio task introspection via `oracle tokio` **sem nenhuma modificação**
- Killer use case: daemon hang em produção sem possibilidade de rebuild

## Sumário Executivo

| ID | Deliverable | Arquivo | LOC |
|----|-------------|---------|-----|
| D1 | Reference doc completa | `~/.claude/skills/Touring/references/touring-cli-debugging-bugstalker.md` | ~280 |
| D2 | Helper script com --oracle/--tui/--dap modes | `~/.claude/rust/scripts/debug-touring-daemon.sh` | ~115 |
| D3 | SKILL.md addendum (seção "Debugging Touring Daemon") | `~/.claude/skills/Touring/SKILL.md` | ~50 |
| **TOTAL** | | **3 arquivos** | **~445 LOC docs** |

## Resultados

- `cargo check --workspace`: EXIT:0 (zero código modificado em Touring)
- Tests: 3785 baseline preservado (zero regressão por definição)
- Orphan baseline: 9106 (preservado)
- `bash -n` syntax check: PASS
- `shellcheck`: PASS exit 0 (zero warnings)
- `debug-touring-daemon.sh --help`: validado (output limpo, formatação correta)

## Análise por Hipótese (Sequential-Thinking)

| H | Hipótese | Verdict | Razão |
|---|----------|---------|-------|
| H1 | DWARF parsing knowledge | DEFER | Requer arquitetura touring-binary-analysis nova; escopo grande |
| H2 | Tokio task introspection | **INTEGRATE-AS-DOCS** | Killer feature; complementa tokio-console sem rebuild |
| H3 | TUI patterns | SKIP | Touring filosofia é "machine-readable JSON"; TUI fora do escopo |
| H4 | Std library pretty-printers | NOT APPLICABLE | Touring é static analysis, não runtime |
| H5 | gimli + ELF analysis (orphan validation post-DCE) | DEFER | Interessante mas requer touring-binary-analysis crate |
| H6 | Watchpoints / breakpoints structure | NOT FIT | Muito longe de code intelligence |
| H7 | Documentação debugging Touring com BugStalker | **INTEGRATE-AS-DOCS** | Resolve real Touring pain point (daemon hangs) |

## Detalhes por Deliverable

### D1 — Reference Documentation

**Arquivo**: `~/.claude/skills/Touring/references/touring-cli-debugging-bugstalker.md`

Estrutura (~280 LOC):
1. **Quando usar**: tabela comparativa vs tokio-console / dhat-heap / OTLP
2. **Installation**: cargo install + Arch + Nix alternatives
3. **Permissions**: ptrace_scope explanation (0/1/2/3) com touring impact
4. **Attach to running daemon**: 3 launch modes
5. **REPL Commands cheatsheet**: tokio oracle, vars, breakpoints, watchpoints
6. **5 Touring-specific recipes**:
   - Recipe 1: Daemon hang post-deploy
   - Recipe 2: MCTS shadow rollout exceeding budget
   - Recipe 3: Actor pattern deadlock (Wave 4 refactor)
   - Recipe 4: Tantivy index commit hang
   - Recipe 5: HookRuntime mutex contention
7. **DAP Mode**: VSCode integration via launch.json
8. **Helper Script**: pointer para D2
9. **Limitações conhecidas**: Linux-only, pause-the-world, single oracle, etc
10. **When NOT to use**: critérios para preferir tokio-console / dhat-heap / pprof

### D2 — Helper Script

**Arquivo**: `~/.claude/rust/scripts/debug-touring-daemon.sh` (executable, ~115 LOC)

Bash strict mode (`set -euo pipefail`). Features:
- Auto-detect PID via `pgrep -u $USER -f 'touring serve'`
- Sanity checks: BugStalker installed, ptrace_scope value, PID is touring (via /proc/$PID/cmdline)
- Mode flags: `--tui`, `--oracle` (tokio), `--dap`, `--pid <N>` (manual)
- Coloured logging (`print_err` red, `print_warn` yellow, `print_info` green)
- Graceful exit codes (3-7 distinct meanings)
- `--help` text completo
- shellcheck PASS (zero warnings)

Usage examples:
```bash
debug-touring-daemon.sh                # console, auto-PID
debug-touring-daemon.sh --oracle       # tokio task tree
debug-touring-daemon.sh --tui          # TUI mode
debug-touring-daemon.sh --dap          # VSCode DAP
debug-touring-daemon.sh --pid 54321    # explicit PID
```

### D3 — SKILL.md Section

Adicionada seção v4.18.0 com:
- Wave summary
- Comparison matrix (tokio-console / dhat / OTLP / BugStalker)
- Quick usage examples
- Pointer para D1 reference doc
- Não-deliverables intencionais (false positives preventidos)

## Metodologia — Pre-Scout Ultrathink

Diferente das Waves 1-5 (que spawnaram 1-3 scout agents), Wave 6 usou ultrathink
sequential-thinking + WebFetch direto **antes** de spawn de scout. Razão:

- BugStalker é 1 repositório (não 5+ crates) — scope estreito
- Hipóteses claras desde início (não há "discovery")
- WebFetch responde perguntas-chave (vs library? install? oracle commands?)
- Risk de scout: gerar propostas de arquitetura grandes demais (e.g. touring-binary-analysis)

Resultado: 3 thoughts de sequential-thinking + 5 WebFetches = full ground truth em ~10 minutos.
Saving: ~30-45min de scout overhead que retornaria conteúdo equivalente.

## Lições Aprendidas

1. **Discovery + grep + WebFetch > scout pesado para repos focados**: quando o repo é uma
   ferramenta única (não library suite), scout agent é overkill. Sequential-thinking +
   targeted WebFetch suffice.
2. **BugStalker complementa tokio-console (não substitui)**: live stream com instrumentation
   vs pause-the-world snapshot sem instrumentation são casos de uso DIFERENTES.
3. **ptrace_scope=1 é amigável para self-debugging**: Linux default permite attach a same-uid
   processes. Apenas hardened systems (=2 ou =3) requerem sudo/CAP_SYS_PTRACE.
4. **Documentation-first é integration legítima**: nem toda análise resulta em código modificado.
   Quando a integração é workflow (não library), docs + helper script entregam valor real.
5. **`pgrep -u $USER` é safer que `pgrep` puro**: evita attach acidental a daemons de outros
   usuários (root, system services). Defensive default no helper script.

## Comparação com Waves anteriores

| Wave | Crates analisadas | Scouts | Code mods | Docs LOC | Verdict |
|------|-------------------|--------|-----------|----------|---------|
| 1 (StringZilla) | 1 (perf) | 0 | 8 hotspots | — | Performance |
| 2 (Predictive) | 0 | 0 | 4 vectors | — | Architecture |
| 3 (RFC-100 Diagnostics) | 0 | 0 | 3 emission sites | — | Wiring |
| 4 (Rich Rendering) | 3 (termtree, miette, annotate) | 3 paralelos | 5 sites | — | Library integration |
| 5 (syntect) | 5 (4 SKIP + syntect) | 3 paralelos | 4 sites + 1 fix | — | Library integration |
| **6 (BugStalker)** | **1 (binary tool)** | **0 (ultrathink)** | **0** | **~445** | **Documentation** |

Wave 6 é única no padrão: zero código Touring modificado, valor entregue 100% via docs + script.

## Touring CLI Changes

**Nenhum comando CLI novo.** Touring binary inalterado.

Mas: novo comando "human-facing" disponível:
```bash
~/.claude/rust/scripts/debug-touring-daemon.sh [--tui | --oracle | --dap] [--pid N]
```

## Deferred — Wave 7+

- **D4 — gimli-based binary analysis (touring-binary-analysis crate)**: validar orphan symbols
  post-DCE (dead code elimination). Requer nova arquitetura, escopo Wave 7+.
- **D5 — DAP server**: Touring como DAP server (não client) para integração nativa com IDEs.
  Requer protocol implementation, escopo grande.
- **D6 — `touring debug attach` CLI command**: thin wrapper sobre debug-touring-daemon.sh.
  Avaliar se vale code-side vs script-side approach.

## See Also

- Reference doc completa: `~/.claude/skills/Touring/references/touring-cli-debugging-bugstalker.md`
- Helper script: `~/.claude/rust/scripts/debug-touring-daemon.sh`
- BugStalker upstream: https://github.com/godzie44/BugStalker
- BugStalker docs: https://godzie44.github.io/BugStalker/docs/overview
- Tokio console comparison: https://github.com/tokio-rs/console
