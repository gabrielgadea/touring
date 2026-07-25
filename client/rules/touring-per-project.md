# Touring Per-Project — Topologia rustup-like (constitutional, auto-load)

> **Auto-load** | **Version**: v1.0 | **Date**: 2026-07-24 (Pln2 F3+PILOT)
> **Authority**: Gabriel Gadea | **Origin**: programa Pln2 produtização — F4′ move-first,
> F1 daemon multi-instância (W12.5), F2 install lifecycle, F3 update+component (W12.3),
> PILOT konverter (D3). Validators: `~/projects/touring/docs/plans/touring-productization-pln2/validate_*.sh`.

## As 4 camadas

| Camada | Onde | Papel |
|---|---|---|
| **L1 Fonte canônica** | `~/projects/touring` | desenvolvimento/refino; `cargo build` aqui NÃO afeta projetos pinados |
| **L2 Toolchain home** | `~/.touring/toolchains/<versão>/` + `default` + `config.toml` | snapshots **imutáveis** (cópias, não symlinks) + canal default |
| **L3 Shim CC** | `~/.claude/hooks/touring-hook` (walk-up 4-layer, instalado por `update-touring`) | roteia cada evento de hook para o binário certo: project bin → toolchain default → dev `~/.local/bin` → fonte |
| **L4 Projeto** | `<proj>/.touring/` — `touring.toml` (pin) · `toolchain.lock` (resolvido) · `bin/` (symlinks→L2) · `daemon.sock` (opt-in) | a verdade local do projeto |

**Requested-vs-resolved (rustup)**: `touring.toml [toolchain] channel` = pedido do HUMANO
(máquina nunca reescreve); `.touring/toolchain.lock` = estado da MÁQUINA (`active` +
`previous` p/ rollback determinístico). Resolução do canal ativo: **lock > pin > dev**.

## Resolução em runtime

- **Binários/hooks**: dentro de projeto com `.touring/bin/` populado, o shim usa o binário
  DO PROJETO — a versão local pode diferir da global. `<proj>/.touring/bin/touring --version`
  é a verdade local.
- **Daemon**: `[daemon] per_project = true` no `touring.toml` → clients resolvem
  `<proj>/.touring/daemon.sock` (walk-up); daemon próprio, auto-spawnado pelo hook, com
  **root pinado ao projeto derivado do socket** (fix PILOT 24/07 — nunca herda env do
  invocador; DBs em `<proj>/.claude/touring/`). Sem opt-in → daemon global.
- **⚠ Precedência de env**: `TOURING_DAEMON_SOCKET`/`TOURING_DAEMON_SOCK` exportados (toda
  sessão CC exporta) vencem o walk-up. Para reproduzir o ambiente per-project em teste:
  `env -u TOURING_DAEMON_SOCKET -u TOURING_DAEMON_SOCK <cmd>`.

## Operação (comandos canônicos)

```bash
# Toolchain home (1×) + instalar versão da fonte canônica
touring toolchain init
touring toolchain install --from-source ~/projects/touring <versão> [--force]
touring toolchain default <versão>            # também: --from-tarball / --from-url

# Projeto novo → per-project
touring init-project [--root <proj>]          # scaffold + pin + popular bin/

# Propagar update / reverter (por projeto — o núcleo W12.3)
touring update [--project <root>] [<canal>|--channel <c>] [--rollback] [--dry-run] [--all-projects] [--no-restart]

# Componentes opcionais (ex.: touring-quality)
touring component list|add|remove <nome> [--project <root>]

# Daemons (REGRA #19 — nunca pkill)
touring daemon-ctl list-all                   # global + todos per-project
touring daemon-ctl status|stop|restart --socket <proj>/.touring/daemon.sock
```

## Gotchas verificados (PILOT 24/07/2026)

1. **Root do daemon per-project é derivado do socket** (`<root>/.touring/daemon.sock`) e
   pinado no spawn (env+cwd) — qualquer caller é correto por construção. NUNCA depender
   do env do invocador.
2. **`touring doctor` `project_db` é resolução CLIENT-side** — não reflete o daemon
   consultado via `--socket`. Prova real do root de um daemon: `/proc/<pid>/environ` + `cwd`.
3. **NUNCA editar `.touring/bin/*` ou `toolchain.lock` à mão** — `touring update` é o dono.
4. **Toolchains são imutáveis**: rebuild da fonte NÃO muda projetos pinados até
   `toolchain install --force` + `touring update` explícitos.

## Bloco padrão para CLAUDE.md de projeto consumidor (camada 3)

Novos projetos per-project devem ter no `.claude/CLAUDE.md` a tabela local
(pin/lock/bins/daemon/dados + operação) — modelo vivo:
`~/projects/konverter/.claude/CLAUDE.md` §"Touring Integration (PER-PROJECT)".
Potencialização registrada: `touring init-project` gerar esse bloco automaticamente.

## Cross-references

| Tópico | Local |
|---|---|
| Fonte canônica — regras do workspace | `~/projects/touring/CLAUDE.md` |
| Programa Pln2 (fases + validators) | `~/projects/touring/docs/plans/touring-productization-pln2/00-INDEX.md` |
| Process hygiene (REGRA #19) | `~/.claude/rules/touring-process-hygiene.md` |
| Rebuild via update-touring | `~/.claude/rules/touring-rebuild.md` |

---
_v1.0 — 2026-07-24 | Materializa a topologia per-project (Pln2 F1-F3+PILOT) como rule constitucional._
