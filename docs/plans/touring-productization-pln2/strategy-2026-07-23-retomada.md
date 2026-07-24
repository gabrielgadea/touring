---
type: Strategy
title: "Retomada Pln2 — Touring instalável, per-project, fonte canônica em ~/projects/touring"
description: "Estratégia de retomada das Fases 1-5 com resequenciamento move-first (F4') + Anexo A: mapa de migração de ~/.claude (root vs per-project)"
tags: [productization, pln2, relocation, per-project-daemon, toolchain]
timestamp: 2026-07-23
plan: /00-INDEX.md
---

# Retomada da Produtização — Estratégia 2026-07-23

> Recuperado de: `~/.claude/plans/giggly-drifting-kahn.md` (Pln2, 6 fases) + `00-INDEX.md` (Fase 0 DONE 01/07) + memória `harness-premium:onda-a+fase0-productization:2026-07-01`.
> Pesquisa externa: cargo-dist (`/axodotdev/cargo-dist`) — installer shell + `install-updater = true` + release manifest JSON, o modelo do `install.touring.dev` (Fase 5).

## Estado verificado (23/07/2026)

| Fato | Evidência executada |
|---|---|
| Fase 0 DONE (env-decouple + version-pin + versão única 30.3.0) | `validate_phase0.sh` 5/5; 5 sites `TOURING_WORKSPACE_ROOT` em crates |
| Workspace real 5,9 GB (target/ 242 GB = cache, NÃO migra) | `du --exclude=target` |
| Daemon vivo singleton, exe `~/.claude/rust/target/release/touring-daemon` | `touring daemon-ctl status` PID 396214 |
| Symlinks `~/.local/bin/touring*` → `~/.claude/rust/target/release/` | `readlink -f` |
| `update-touring:34` hardcoda `RUST_WORKSPACE=~/.claude/rust` | grep |
| 35 .rs de produção ainda citam `claude/rust` (fallbacks históricos/testes — reclassificar antes do corte) | grep em crates |
| 86 arquivos de config em `~/.claude` citam `claude/rust` (72 em skills/, 6 commands, 4 agents, 2 rules, CLAUDE.md, settings.json) | grep |
| 6 projetos já têm `.claude/touring/` per-project db | ls projects/*/.claude/touring |
| Fundação ~70% pronta: init-project (W12.1) · toolchain manager (W12.2) · layered config (W12.4) · hook shim script (W12.6, NÃO instalado) · migrate-from-global (W12.7) | Pln2 §2 file:line |
| Gaps nucleares: lock per-socket (W12.5) · `touring update`/`component` (W12.3 AUSENTE) · `.touring/bin` vazio | Pln2 §2 |

## Arquitetura de 4 camadas (modelo rustup)

```
L1 FONTE CANÔNICA   ~/projects/touring          ← desenvolvimento/evolução; produz releases
L2 TOOLCHAIN HOME   ~/.touring/toolchains/<v>/  ← releases imutáveis; N versões; canal "dev" = build da fonte
L3 ROOT CC          ~/.claude/                  ← constituição + rules + skills genéricas + HOOK SHIM (walk-up)
L4 PROJETO          <proj>/.touring/            ← pin de versão, bin/, data/ (DBs), daemon.sock, adw/
                    <proj>/.claude/ (overlay)   ← skill Touring versionada, CLAUDE.md do projeto, commitments
```

Update flow: dev em L1 → release em L2 → **`touring update` em cada projeto** (re-linka bin, migra dados por schema, reinicia daemon local) — propagação individual por projeto, com rollback por pin.

## Resequenciamento proposto: F4' MOVE-FIRST

Ordem original: F1→F2→F3→F4→F5. Proposta: **F4' (move da fonte) → F1 → F2 → F3 → F5**, porque a Fase 4 só depende da Fase 0 (DONE) e Gabriel definiu a fonte em `~/projects` como *base fundacional sob a qual o desenvolvimento deve ser feito* — desenvolver F1-F3 já no endereço definitivo evita re-referenciar paths/docs duas vezes.

**F4' copy-first (nunca move destrutivo)**:
1. rsync `~/.claude/rust` (sem target/) → `~/projects/touring`; build frio lá; `touring doctor` verde do novo root.
2. `update-touring` parametrizado por `TOURING_WORKSPACE_ROOT` (fix da linha 34) → re-symlink `~/.local/bin/*` → `daemon-ctl restart` (REGRA #19; janela sem sessões CC concorrentes).
3. `~/.claude/rust` vira cópia congelada até E2E completo; descarte = decisão de Gabriel. Git do novo repo = Gabriel (REGRA #11).
4. Co-evolução: 86 configs em `~/.claude` re-referenciados; disk-watch TARGETS += novo target/ (REGRA #12); `touring index rebuild --dir ~/projects/touring`.

## Anexo A — Mapa de migração de ~/.claude

| Item | Destino | Nota |
|---|---|---|
| `rust/` (5,9 GB src) | **MIGRA** → `~/projects/touring` | target/ 242 GB fica (regenerável) |
| `plans/` touring-specific + `Touring.skill` + `TOURING_POTENCIALIZACAO.md` | **MIGRA** → docs do novo repo | |
| `CLAUDE.md`, `rules/`, `commands/`, `agents/`, skills genéricas | **FICA** (root CC) | paths atualizados (86 arquivos) |
| `hooks/` | **TRANSFORMA** | binário → **shim walk-up** (W12.6); guard scripts ficam |
| `touring/` (2,5 GB DBs globais) | **FICA** como store do escopo root | projetos ganham o seu via `migrate-from-global` (W12.7) |
| `skills/Touring` (master) | **DUAL** | master na fonte canônica; publicada como componente `touring component add claude-skill` → `<proj>/.claude/skills/Touring` na versão pinada; cópia root segue canal dev |
| `adw-library/` | **DUAL** | defaults na toolchain `share/`; customização em `<proj>/.touring/adw/` |
| `tools/` (disk-watch, safe-clean), `plugins/`, `projects/`, `data/`, `security/`, telemetry etc. | **FICA** | infra do CC, não do Touring |

## Decisões para Gabriel (human gate)

- **D1** Aprovar resequenciamento F4' move-first (vs ordem original F1-primeiro).
- **D2** Confirmar path canônico: `~/projects/touring`.
- **D3** Política de ativação per-project: piloto gradual (recomendo **konverter** como 1º piloto) vs all-in.
- **D4** Destino da cópia congelada `~/.claude/rust` pós-E2E.

## Após aprovação

DAG via `touring decompose create` (F4'→F1→F2→F3→F5, cada fase com `validate_phaseN.sh` + gates REGRA #21) · marker loop-engineering · execução fase a fase, 1 fase/sessão · estimativa ~8-11 dias + 2 ações git de Gabriel.
